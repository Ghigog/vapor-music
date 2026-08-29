//! Track metadata and transition cost.
//!
//! Port of `calculate_transition_cost` from `dj_pathfinder.gd`, plus the track
//! shape it reads.
//!
//! The GDScript passes untyped `Dictionary` values around and re-derives fields
//! with chained `get()` fallbacks at every call site. Here the fallbacks live in
//! one place — [`TrackMeta::outro_key`] and [`TrackMeta::intro_key`] — so the
//! precedence rule is stated once instead of being repeated and drifting.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::camelot::harmonic_relation_cost;
use crate::genre::genre_distance;

/// Cost weights, from the constants at the top of `dj_pathfinder.gd`.
pub const WEIGHT_KEY: f32 = 2.5;
pub const WEIGHT_BPM: f32 = 0.15;
pub const WEIGHT_ENERGY: f32 = 6.0;
pub const WEIGHT_GENRE: f32 = 3.0;

/// Energy difference beyond which a steep extra penalty applies, so the
/// pathfinder will not drop a dancefloor into an ambient track just because the
/// keys happen to line up.
pub const DEFAULT_ENERGY_THRESHOLD: f32 = 0.5;
const OVER_THRESHOLD_PENALTY: f32 = 100.0;

/// What the pathfinder needs to know about a track.
///
/// Deliberately not the full metadata record: this is the mixing view, and
/// keeping it narrow means the pathfinder cannot quietly grow a dependency on
/// artwork or play counts.
// Serialised for two audiences at once: the IPC boundary, where the frontend
// reads camelCase, and the JSON on disk, which was written snake_case before
// this was noticed. `rename_all` fixes the wire; the per-field `alias` keeps
// every existing file readable, so nobody's folders or manual ordering reset
// on upgrade. Removing an alias is a data migration, not a tidy-up.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrackMeta {
    pub href: String,
    pub bpm: f32,
    /// Whole-track key, in Camelot notation.
    #[serde(default)]
    #[serde(alias = "musical_key")]
    pub musical_key: String,
    /// Key of the outro, when segmented analysis produced one.
    #[serde(default)]
    #[serde(alias = "outro_key")]
    pub outro_key: String,
    /// Key of the intro, when segmented analysis produced one.
    #[serde(default)]
    #[serde(alias = "intro_key")]
    pub intro_key: String,
    #[serde(default = "half")]
    #[serde(alias = "energy_level")]
    pub energy_level: f32,
    #[serde(default)]
    pub genre: String,
}

fn half() -> f32 {
    0.5
}

impl TrackMeta {
    /// Key to mix *out* of: the outro key when known, else the whole-track key.
    ///
    /// Using the outro rather than the average is the point of segmented
    /// analysis — a track that modulates should be judged on where it ends, not
    /// on where it spent most of its time.
    pub fn outro_key(&self) -> &str {
        if !self.outro_key.is_empty() {
            &self.outro_key
        } else {
            &self.musical_key
        }
    }

    /// Key to mix *into*: the intro key when known, else the whole-track key.
    pub fn intro_key(&self) -> &str {
        if !self.intro_key.is_empty() {
            &self.intro_key
        } else {
            &self.musical_key
        }
    }
}

/// Perceived intensity, 0–1, from integrated loudness.
///
/// The measure this replaces for every purpose that wanted "how hard does this
/// go": [`vapor_dsp::loudness::energy_level`] is mean RMS over peak RMS, which
/// is a *consistency* ratio. A track that sits at one level the whole way
/// through scores high and one with a breakdown scores low, so on the owner's
/// library drum & bass averaged 0.661 against 0.629 for Sade, De André and
/// Knxwledge — ranges fully overlapping, with ballads at the top.
///
/// That is a defensible measure of something. It is not intensity, and it was
/// driving three things that needed intensity: the Build and Chill curves, the
/// energy term in [`transition_cost`], and whether two tracks count as a match.
///
/// Loudness separates the same two groups cleanly — −10.6 LUFS against −16.9 —
/// and is already measured for every track, so this costs no re-analysis.
///
/// The band is −30 to −5 LUFS. Below −30 is quiet enough to be ambient
/// whatever it is; above −5 is as loud as mastering goes. The owner's library
/// runs −23.5 to −8.4, which sits inside it with room at both ends rather than
/// saturating.
pub fn intensity_from_lufs(lufs: f32) -> f32 {
    const QUIET: f32 = -30.0;
    const LOUD: f32 = -5.0;

    // Silence and unmeasurable both arrive as 0.0 from the loudness stage, and
    // an unknown belongs in the middle rather than at an end where it would
    // drag a curve toward it — the same fallback `energy_level` uses.
    if !lufs.is_finite() || lufs >= 0.0 {
        return 0.5;
    }
    ((lufs - QUIET) / (LOUD - QUIET)).clamp(0.0, 1.0)
}

/// Cost of mixing `from` into `to`. Lower is better.
///
/// `skip_penalty` carries the learned dislike of a specific pair, which the
/// GDScript reads from a history file inside this function. It is a parameter
/// here so the cost stays a pure function — that is what lets it be tested
/// without a filesystem and reused off the main thread.
pub fn transition_cost(
    from: &TrackMeta,
    to: &TrackMeta,
    energy_threshold: f32,
    skip_penalty: f32,
) -> f32 {
    let key_cost = harmonic_relation_cost(from.outro_key(), to.intro_key());
    let bpm_diff = (from.bpm - to.bpm).abs();
    let energy_diff = (from.energy_level - to.energy_level).abs();
    let genre_cost = genre_distance(&from.genre, &to.genre);

    let extra = if energy_diff > energy_threshold {
        OVER_THRESHOLD_PENALTY * (energy_diff - energy_threshold)
    } else {
        0.0
    };

    key_cost * WEIGHT_KEY
        + bpm_diff * WEIGHT_BPM
        + energy_diff * WEIGHT_ENERGY
        + genre_cost * WEIGHT_GENRE
        + skip_penalty
        + extra
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(bpm: f32, key: &str, energy: f32, genre: &str) -> TrackMeta {
        TrackMeta {
            href: format!("{key}-{bpm}"),
            bpm,
            musical_key: key.into(),
            energy_level: energy,
            genre: genre.into(),
            ..Default::default()
        }
    }

    /// Ported from `test_transition_cost`.
    #[test]
    fn compatible_transitions_cost_less_than_incompatible_ones() {
        let a = track(120.0, "8A", 0.5, "");
        let b = track(122.0, "8B", 0.6, "");
        let c = track(140.0, "2A", 0.9, "");

        let ab = transition_cost(&a, &b, DEFAULT_ENERGY_THRESHOLD, 0.0);
        let ac = transition_cost(&a, &c, DEFAULT_ENERGY_THRESHOLD, 0.0);
        assert!(ab < ac, "compatible {ab} should beat incompatible {ac}");
    }

    /// Ported from `test_genre_taxonomy_cost`.
    #[test]
    fn genre_relatedness_shows_up_in_the_cost() {
        let tech_house = track(120.0, "8A", 0.5, "Tech House");
        let techno = track(120.0, "8A", 0.5, "Techno");
        let liquid = track(120.0, "8A", 0.5, "Liquid DNB");
        let house = track(120.0, "8A", 0.5, "House");

        let t = DEFAULT_ENERGY_THRESHOLD;
        let ab = transition_cost(&tech_house, &techno, t, 0.0);
        let db = transition_cost(&house, &techno, t, 0.0);
        let dc = transition_cost(&house, &liquid, t, 0.0);

        assert!(
            ab < dc,
            "Tech House->Techno {ab} should beat House->DNB {dc}"
        );
        assert!(db < dc, "House->Techno {db} should beat House->DNB {dc}");
    }

    /// The threshold exists to stop the pathfinder pairing tracks whose keys
    /// match but whose energy is nowhere near.
    #[test]
    fn a_large_energy_jump_is_penalised_steeply() {
        let calm = track(120.0, "8A", 0.1, "House");
        let near = track(120.0, "8A", 0.5, "House");
        let wild = track(120.0, "8A", 0.95, "House");

        let t = DEFAULT_ENERGY_THRESHOLD;
        let modest = transition_cost(&calm, &near, t, 0.0);
        let extreme = transition_cost(&calm, &wild, t, 0.0);

        assert!(
            extreme > modest * 5.0,
            "crossing the threshold should be severe: {modest} -> {extreme}"
        );
    }

    #[test]
    fn segmented_keys_take_precedence_over_the_whole_track_key() {
        let mut from = track(120.0, "8A", 0.5, "House");
        from.outro_key = "10A".into();
        let mut to = track(120.0, "2A", 0.5, "House");
        to.intro_key = "10A".into();

        // Judged on 10A -> 10A (free), not 8A -> 2A (a clash).
        let cost = transition_cost(&from, &to, DEFAULT_ENERGY_THRESHOLD, 0.0);
        let ignoring_segments = transition_cost(
            &track(120.0, "8A", 0.5, "House"),
            &track(120.0, "2A", 0.5, "House"),
            DEFAULT_ENERGY_THRESHOLD,
            0.0,
        );
        assert!(
            cost < ignoring_segments,
            "segment keys were ignored: {cost} vs {ignoring_segments}"
        );
    }

    #[test]
    fn skip_penalty_is_added_verbatim() {
        let a = track(120.0, "8A", 0.5, "House");
        let b = track(120.0, "8B", 0.5, "House");
        let base = transition_cost(&a, &b, DEFAULT_ENERGY_THRESHOLD, 0.0);
        let penalised = transition_cost(&a, &b, DEFAULT_ENERGY_THRESHOLD, 7.5);
        assert!((penalised - base - 7.5).abs() < 1e-4);
    }

    // -----------------------------------------------------------------------
    // Intensity, which is not what `energy_level` measures
    // -----------------------------------------------------------------------

    /// The two groups that motivated this, at their measured loudnesses.
    #[test]
    fn loud_music_reads_as_more_intense_than_quiet_music() {
        let dnb = intensity_from_lufs(-10.6);
        let mellow = intensity_from_lufs(-16.9);
        assert!(
            dnb > mellow,
            "drum & bass {dnb:.3} did not outrank mellow {mellow:.3}"
        );
        // And by a margin worth acting on, rather than a rounding difference.
        assert!(
            dnb - mellow > 0.2,
            "separation {:.3} is too small",
            dnb - mellow
        );
    }

    /// The owner's library runs -23.5 to -8.4 LUFS and should spread across
    /// the range rather than bunching at one end.
    #[test]
    fn a_real_library_spreads_across_the_range() {
        let quietest = intensity_from_lufs(-23.5);
        let loudest = intensity_from_lufs(-8.4);
        assert!(quietest > 0.05 && quietest < 0.35, "{quietest}");
        assert!(loudest > 0.75 && loudest < 1.0, "{loudest}");
    }

    #[test]
    fn it_is_monotonic_and_bounded() {
        let mut previous = -1.0;
        for lufs in [-40.0f32, -30.0, -25.0, -20.0, -15.0, -10.0, -5.0, -1.0] {
            let v = intensity_from_lufs(lufs);
            assert!((0.0..=1.0).contains(&v), "{lufs} gave {v}");
            assert!(v >= previous, "not monotonic at {lufs}");
            previous = v;
        }
    }

    /// An unmeasurable loudness sits in the middle rather than at an end,
    /// where it would drag a curve toward it.
    #[test]
    fn an_unknown_loudness_sits_in_the_middle() {
        assert_eq!(intensity_from_lufs(0.0), 0.5);
        assert_eq!(intensity_from_lufs(f32::NAN), 0.5);
        assert_eq!(intensity_from_lufs(f32::INFINITY), 0.5);
    }

    /// The set-building failure this was reported as, priced.
    ///
    /// An ambient piece with no percussion gets read at 178, which is a real
    /// reading of a real signal and not a bug the estimator can fix. Two things
    /// then have to happen for a set not to mix it into a brostep record:
    /// `octave_correct` has to bring the tempo back to the octave a listener
    /// counts, and the genre term has to make the pairing expensive even after
    /// the tempi stop arguing.
    ///
    /// Only the second is measured here — see `genre.rs` for the first. What it
    /// pins is that the genre term now *discriminates*: before the taxonomy
    /// grew, neither `ambient` nor `brostep` nor `modern classical` was in it,
    /// every pair scored `UNRELATED_COST`, and these two costs were equal.
    #[test]
    fn a_set_prefers_the_neighbouring_genre_to_the_distant_one() {
        // The corrected tempo, not the 178 that was measured.
        let budd = track(89.0, "8A", 0.15, "Ambient");
        // Same shelf: quiet, slow, adjacent in the taxonomy.
        let neighbour = track(92.0, "8A", 0.20, "Modern Classical");
        // The record it was being mixed into. Deliberately given the *same key*
        // and a plausible tempo, so key and tempo cannot be what separates them
        // — the genre has to.
        let skrillex = track(140.0, "8A", 0.85, "Brostep");

        let near = transition_cost(&budd, &neighbour, DEFAULT_ENERGY_THRESHOLD, 0.0);
        let far = transition_cost(&budd, &skrillex, DEFAULT_ENERGY_THRESHOLD, 0.0);

        assert!(
            near < far,
            "ambient->modern classical cost {near}, ambient->brostep cost {far}"
        );

        // And by a margin the planner cannot shrug off. The A* sums these over
        // a whole set, so a difference of a point or two would be swamped by
        // one lucky key match somewhere else in the path.
        assert!(
            far - near > 20.0,
            "the gap is only {}, which a single good key match could cancel",
            far - near
        );
    }

    /// A track nobody has identified is not treated as the worst possible one.
    ///
    /// The regression this guards against is subtle and is *caused* by fixing
    /// the genre source: in a part-identified library, if an absent genre costs
    /// the same as a clashing one, then every track still waiting to be looked
    /// up is as expensive as the worst pairing available and the planner works
    /// around all of them.
    #[test]
    fn an_unidentified_track_is_not_ranked_below_a_genre_clash() {
        let budd = track(89.0, "8A", 0.15, "Ambient");
        let unknown = track(92.0, "8A", 0.20, "");
        let clashing = track(92.0, "8A", 0.20, "Brostep");

        let unknown_cost = transition_cost(&budd, &unknown, DEFAULT_ENERGY_THRESHOLD, 0.0);
        let clash_cost = transition_cost(&budd, &clashing, DEFAULT_ENERGY_THRESHOLD, 0.0);

        assert!(
            unknown_cost < clash_cost,
            "not-yet-looked-up ({unknown_cost}) must not cost more than a known \
             clash ({clash_cost})"
        );
    }
}

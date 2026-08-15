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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TrackMeta {
    pub href: String,
    pub bpm: f32,
    /// Whole-track key, in Camelot notation.
    #[serde(default)]
    pub musical_key: String,
    /// Key of the outro, when segmented analysis produced one.
    #[serde(default)]
    pub outro_key: String,
    /// Key of the intro, when segmented analysis produced one.
    #[serde(default)]
    pub intro_key: String,
    #[serde(default = "half")]
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
}

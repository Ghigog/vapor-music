//! Harmonic set ordering — the A* mood path.
//!
//! Port of `generate_mood_path` from `dj_pathfinder.gd`. Given a pool of tracks
//! and a target curve, order them so each transition is cheap *and* the set
//! follows an energy and tempo shape over time.
//!
//! Two costs are in play and they pull differently: the transition cost keeps
//! neighbours compatible, while the curve cost keeps the set heading somewhere.
//! Optimising only the first produces a set that never goes anywhere; only the
//! second produces jarring but on-trend jumps.
//!
//! The search is bounded on three axes — beam width, expansion count, and path
//! length — because this is not an optimisation problem with a right answer, it
//! is a taste problem where a good ordering found quickly beats a marginally
//! better one found slowly.

use std::collections::HashMap;

use crate::track::{transition_cost, TrackMeta};

/// Curve weights.
///
/// # Why these are twenty times what they were
///
/// They were 8.0 and 0.3, ported from `dj_pathfinder.gd`, and at those values
/// the curve was decoration. The arithmetic, measured rather than guessed:
///
/// A Build asked for +0.4 of energy across ten tracks — 0.044 a step. Ignoring
/// that cost `0.044 * 8 = 0.35`. Taking the step cost `0.044 * WEIGHT_ENERGY`,
/// which is `0.26`, so the *net* pull toward the curve was 0.09 per step.
/// A single key mode-shift costs `1.0 * WEIGHT_KEY = 2.5`, and a genre clash
/// `5.0 * WEIGHT_GENRE = 15`. The curve was outbid by every other term in the
/// model by one to two orders of magnitude, so all four curves returned very
/// nearly the same set and the screen's four buttons did the same thing.
///
/// At 45.0 a candidate 0.1 of energy off the curve costs 4.5 — more than a
/// mode shift, less than a clash — which is the band where the curve can steer
/// without dragging the set into a pairing the mixer would refuse.
pub const WEIGHT_CURVE_ENERGY: f32 = 45.0;
pub const WEIGHT_CURVE_BPM: f32 = 0.5;

/// What a step *against* the curve costs, on top of being off it.
///
/// The position term above says "be at 0.61 by now". It does not say "do not
/// go down", and those are different: a set sitting at 0.25 under a Build pays
/// the same whether the last step moved it up or down. This charges the
/// component of a step that points the wrong way, so a Build that drops 0.1 of
/// energy pays 6 for the direction on top of whatever it pays for the
/// position. Nothing is charged under [`Curve::Flat`], where there is no
/// direction to go against.
pub const WEIGHT_BACKTRACK: f32 = 60.0;

/// What repeating an artist or an album inside the visible window costs.
///
/// The complaint this answers: a Vibe set that played twelve tracks from one
/// album in a row. Nothing in the cost model objected, because nothing in it
/// could tell two records apart — see [`TrackMeta::artist`].
///
/// Sized just above the worst an ordinary transition costs — a key clash is
/// 20 and a genre clash 15, so 35 buys the whole of both — and well under what
/// the curve charges for a step in the wrong direction. That ordering is the
/// whole of the tuning: leaving the record has to beat staying on it, and it
/// must not beat staying on the curve. At 95 it did beat the curve, and a
/// Build would abandon a climb to avoid a second techno record.
pub const REPEAT_ARTIST: f32 = 25.0;
pub const REPEAT_ALBUM: f32 = 60.0;

/// How many tracks back the planner remembers, for variety.
///
/// Ten is what the screen shows, six is what a listener notices; charging over
/// the whole visible window would forbid an artist from recurring in a set at
/// all, which is a different and worse fault.
pub const VARIETY_WINDOW: usize = 6;

/// Steps a Build or a Chill takes to travel its whole span, and the period of
/// a Wave.
///
/// The same ten the screen shows, so a curve is a shape you can *see* rather
/// than one you would have to sit through an hour to observe.
pub const CURVE_STEPS: usize = 10;

/// Longest path the A* stage plans. Beyond this the greedy tail takes over.
const MAX_PLANNED: usize = 10;
/// Successors expanded per node — the beam width.
const BEAM_WIDTH: usize = 6;
/// Hard cap on expansions, so a large library cannot stall the search.
const MAX_EXPANSIONS: usize = 1500;

/// How far a curve can actually travel, read off the pool it will be drawn
/// from.
///
/// # Why a curve cannot be `start ± 0.4`
///
/// That is what it was, and it fails at both ends of the library. Starting on
/// an ambient record at 0.30, a Build aimed at 0.70 and the drum & bass at
/// 0.92 was never a target — so "build" meant "become hip hop" and stopped.
/// Starting at 0.85 it aimed at 1.0, which nothing reaches, so the target
/// saturated and the term went flat: a constant offers no gradient, and the
/// planner had nothing to climb.
///
/// Reading the range off the pool fixes both. A Build goes to the top of what
/// this person actually owns *in this scope*, which is the journey they
/// described wanting — ambient, then folk, then rock, then metal — expressed
/// in the only terms the app can measure.
///
/// The 10th and 90th percentiles rather than the extremes, because one
/// mis-analysed track at 178 BPM should not become the destination of every
/// Build in the library.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub energy_floor: f32,
    pub energy_ceiling: f32,
    pub bpm_floor: f32,
    pub bpm_ceiling: f32,
}

impl Default for Span {
    /// What a curve travels when there is no pool to measure — a small
    /// selection, or a caller that has not got one. The old fixed ±0.4 and
    /// ±15, kept only as the fallback it always should have been.
    fn default() -> Self {
        Span {
            energy_floor: 0.1,
            energy_ceiling: 0.9,
            bpm_floor: 60.0,
            bpm_ceiling: 160.0,
        }
    }
}

impl Span {
    /// Measure the reachable range of a pool.
    ///
    /// Falls back to [`Span::default`] below eight tracks, where a percentile
    /// is a single track's opinion.
    pub fn of(tracks: &HashMap<String, TrackMeta>) -> Span {
        if tracks.len() < 8 {
            return Span::default();
        }
        // `curve_energy`, not `energy_level`: the span is what the curve
        // travels, so it has to be measured in the units the curve reads.
        let mut energies: Vec<f32> = tracks.values().map(TrackMeta::curve_energy).collect();
        let mut tempi: Vec<f32> = tracks.values().map(|t| t.bpm).filter(|b| *b > 0.0).collect();
        if tempi.len() < 8 {
            tempi = vec![Span::default().bpm_floor, Span::default().bpm_ceiling];
        }
        energies.sort_by(f32::total_cmp);
        tempi.sort_by(f32::total_cmp);

        Span {
            energy_floor: percentile(&energies, 0.10),
            energy_ceiling: percentile(&energies, 0.90),
            bpm_floor: percentile(&tempi, 0.10),
            bpm_ceiling: percentile(&tempi, 0.90),
        }
    }
}

/// Nearest-rank percentile of an already-sorted, non-empty slice.
fn percentile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f32 * q).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

/// Shape a set should follow over time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    /// Rising energy and tempo.
    Build,
    /// Falling energy and tempo.
    Chill,
    /// Up then back down.
    Wave,
    /// Hold roughly steady.
    Flat,
}

impl Curve {
    /// Accepts the spellings the Godot UI passes, including the spaced and
    /// underscored variants.
    pub fn parse(s: &str) -> Curve {
        match s.trim().to_lowercase().as_str() {
            "build" | "build vibe" | "build_vibe" => Curve::Build,
            "chill" | "chill down" | "chill_down" => Curve::Chill,
            "wave" => Curve::Wave,
            _ => Curve::Flat,
        }
    }

    /// The canonical spelling, so a curve round-trips through settings and the
    /// wire without drifting into one of the variants `parse` tolerates.
    pub fn as_str(self) -> &'static str {
        match self {
            Curve::Build => "build",
            Curve::Chill => "chill",
            Curve::Wave => "wave",
            Curve::Flat => "flat",
        }
    }

    /// Energy the set should be at, `step` tracks after the curve was chosen.
    ///
    /// # Why this counts steps rather than dividing a total
    ///
    /// It took `(index, total)` and returned the shape stretched across
    /// exactly that many tracks. That signature assumes the set is a finite
    /// thing planned in one go, which is what the DJ used to do — plan ten,
    /// play them, plan ten more — and the assumption showed: every batch
    /// restarted the curve from the track playing, so a Build climbed for ten
    /// tracks and then dropped back to wherever it had got to and climbed
    /// again from there.
    ///
    /// A set has no end. So the curve is a function of how far the set has
    /// travelled, and it is defined past its own span: a Build that reaches
    /// the ceiling stays there, a Chill that reaches the floor stays there,
    /// and a Wave goes on breathing at [`CURVE_STEPS`] to the cycle.
    pub fn target_energy(self, start: f32, step: usize, span: &Span) -> f32 {
        let (lo, hi) = (span.energy_floor, span.energy_ceiling);
        self.shape(start, step, lo, hi).clamp(0.0, 1.0)
    }

    /// Tempo the set should be at, `step` tracks after the curve was chosen.
    pub fn target_bpm(self, start: f32, step: usize, span: &Span) -> f32 {
        let (lo, hi) = (span.bpm_floor, span.bpm_ceiling);
        self.shape(start, step, lo, hi).max(0.0)
    }

    /// The shape itself, in whatever units `lo` and `hi` are given in.
    fn shape(self, start: f32, step: usize, lo: f32, hi: f32) -> f32 {
        // A start outside the pool's own 10th–90th band is not an error — it
        // is the one loud record in an ambient library — and the curve must
        // still have somewhere to go, so the band is widened to hold it.
        let (lo, hi) = (lo.min(start), hi.max(start));
        let t = step as f32 / CURVE_STEPS as f32;
        match self {
            Curve::Build => lerp(start, hi, t.min(1.0)),
            Curve::Chill => lerp(start, lo, t.min(1.0)),
            // A full cycle per span, so the rise and the fall both happen
            // inside the ten tracks the screen shows. Six tenths of the way to
            // each end rather than all of it: a wave that touches the ceiling
            // is a Build with a Chill after it.
            Curve::Wave => {
                let s = (t * std::f32::consts::PI * 2.0).sin();
                if s >= 0.0 {
                    start + s * (hi - start) * 0.6
                } else {
                    start + s * (start - lo) * 0.6
                }
            }
            Curve::Flat => start,
        }
    }

    /// Which way the curve wants the set to move on the step into `step`.
    ///
    /// Positive to climb, negative to fall, zero to hold. Read from the shape
    /// rather than from the variant, so a Wave gets the right answer at each
    /// point of its cycle and a Build that has topped out correctly asks for
    /// nothing.
    fn energy_direction(self, start: f32, step: usize, span: &Span) -> f32 {
        let now = self.target_energy(start, step, span);
        let before = self.target_energy(start, step.saturating_sub(1), span);
        now - before
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// How far a candidate sits from where the curve wants the set to be.
fn curve_cost(energy: f32, bpm: f32, target_energy: f32, target_bpm: f32) -> f32 {
    (energy - target_energy).abs() * WEIGHT_CURVE_ENERGY
        + (bpm - target_bpm).abs() * WEIGHT_CURVE_BPM
}

/// What a step that points against the curve costs. See [`WEIGHT_BACKTRACK`].
fn backtrack_cost(from_energy: f32, to_energy: f32, want: f32) -> f32 {
    const FLAT: f32 = 1e-4;
    let moved = to_energy - from_energy;
    let against = if want > FLAT {
        (-moved).max(0.0)
    } else if want < -FLAT {
        moved.max(0.0)
    } else {
        0.0
    };
    against * WEIGHT_BACKTRACK
}

/// What playing this record again so soon costs.
///
/// `recent` is most-recent-first and may name tracks the pool does not hold;
/// those are skipped rather than guessed at. The charge decays with distance,
/// so the record that just played weighs most and one six tracks back barely
/// weighs at all — the aim is a set that does not repeat itself, not one
/// forbidden from ever returning to an artist.
fn variety_cost(tracks: &HashMap<String, TrackMeta>, recent: &[String], to: &TrackMeta) -> f32 {
    let mut cost = 0.0;
    for (i, href) in recent.iter().take(VARIETY_WINDOW).enumerate() {
        let Some(prev) = tracks.get(href) else {
            continue;
        };
        // Squared, so the charge is concentrated on the last two or three
        // rather than spread evenly over six. Back-to-back is the fault; the
        // same artist five tracks ago is a set with a favourite.
        let linear = 1.0 - i as f32 / VARIETY_WINDOW as f32;
        let decay = linear * linear;
        if !to.album.is_empty() && to.album == prev.album {
            cost += REPEAT_ALBUM * decay;
        } else if !to.artist.is_empty() && to.artist == prev.artist {
            cost += REPEAT_ARTIST * decay;
        }
    }
    cost
}

/// Everything a single step of the set is judged on.
///
/// One definition, so the step-at-a-time DJ in [`next_track`] and the whole-set
/// ordering in [`generate_mood_path`] cannot come to different conclusions
/// about the same pair — which is precisely the drift the exits and the planner
/// used to have.
#[allow(clippy::too_many_arguments)]
fn step_score(
    tracks: &HashMap<String, TrackMeta>,
    from: &TrackMeta,
    to: &TrackMeta,
    recent: &[String],
    curve: Curve,
    start_energy: f32,
    start_bpm: f32,
    step: usize,
    span: &Span,
    energy_threshold: f32,
    skip_penalty: f32,
) -> f32 {
    let target_energy = curve.target_energy(start_energy, step, span);
    let target_bpm = curve.target_bpm(start_bpm, step, span);
    let want = curve.energy_direction(start_energy, step, span);

    transition_cost(from, to, energy_threshold, skip_penalty)
        + curve_cost(to.curve_energy(), to.bpm, target_energy, target_bpm)
        + backtrack_cost(from.curve_energy(), to.curve_energy(), want)
        + variety_cost(tracks, recent, to)
}

/// Remaining cost estimate for A*.
///
/// Scaled by the fraction of the path still unplanned, so it shrinks toward
/// zero as the path completes.
fn heuristic(
    path_len: usize,
    total: usize,
    energy: f32,
    end_energy: f32,
    bpm: f32,
    end_bpm: f32,
) -> f32 {
    if path_len >= total {
        return 0.0;
    }
    let steps_left = (total - path_len) as f32;
    curve_cost(energy, bpm, end_energy, end_bpm) * (steps_left / total as f32)
}

/// The one track the DJ plays next.
///
/// # Why the DJ no longer plans ten
///
/// It planned ten, queued them, played them, and planned ten more. Three
/// things were wrong with that and all three were visible on screen.
///
/// The freeze: ten tracks meant an A* over the whole library, which is seconds
/// of work, and every press of a curve paid for all ten before anything
/// appeared. One step is a single pass over the pool.
///
/// The waste: nine of the ten decisions were thrown away the moment anybody
/// changed their mind, which is the normal case — the curve buttons exist to
/// be pressed.
///
/// The reset: each batch re-planned from the track playing, so the curve
/// started over every ten tracks and a Build was really nine small builds. Now
/// `step` counts from where the curve was chosen and keeps counting, so the
/// shape is continuous however many times the tail is topped up.
///
/// `recent` is the set so far, most-recent-first; the caller owns it because
/// the caller owns the queue. Tracks named in it are not offered again.
///
/// `origin` is the track the curve was chosen on, and `step` is how many
/// tracks after it this one is. Passed rather than inferred from `recent`,
/// which is a different thing: the head of the history is where the set *is*,
/// and the curve is measured from where it *began*. An `origin` the pool does
/// not hold falls back to `from`, which is a curve starting here — degraded,
/// but not wrong in a way that compounds.
#[allow(clippy::too_many_arguments)]
pub fn next_track(
    tracks: &HashMap<String, TrackMeta>,
    from: &str,
    origin: &str,
    recent: &[String],
    curve: Curve,
    step: usize,
    span: &Span,
    energy_threshold: f32,
    skip_penalty: &HashMap<(String, String), f32>,
) -> Option<String> {
    let current = tracks.get(from)?;
    let origin = tracks.get(origin).unwrap_or(current);

    // Deterministic in the face of ties: a HashMap's iteration order is not,
    // and "why did it pick a different track that time" is not a question this
    // should ever raise.
    let mut keys: Vec<&String> = tracks.keys().collect();
    keys.sort();

    keys.into_iter()
        .filter(|href| *href != from && !recent.contains(href))
        .map(|href| {
            let to = &tracks[href];
            let penalty = skip_penalty
                .get(&(from.to_string(), href.clone()))
                .copied()
                .unwrap_or(0.0);
            let score = step_score(
                tracks,
                current,
                to,
                recent,
                curve,
                origin.curve_energy(),
                origin.bpm,
                step,
                span,
                energy_threshold,
                penalty,
            );
            (href, score)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(href, _)| href.clone())
}

struct State {
    path: Vec<String>,
    g: f32,
    f: f32,
}

/// Order `tracks` into a set following `curve`, starting from `start`.
///
/// `skip_penalty` supplies the learned dislike of a specific pair, keyed by
/// `(from_href, to_href)`; pass an empty map when there is no history. Keeping
/// it a parameter is what lets this run without a filesystem.
///
/// Every track is returned. The A* stage plans the first [`MAX_PLANNED`], then
/// the remainder are appended greedily — planning the whole set exactly is
/// exponential and buys nothing a listener would notice past the first handful.
///
/// **This is no longer what the DJ uses.** Ordering a set the caller already
/// has — a playlist, a selection — is a different job from conducting an
/// endless one, and it is the only job left here; [`next_track`] is what
/// decides what plays next. The two share [`step_score`], so they cannot
/// disagree about what a good step is.
pub fn generate_mood_path(
    tracks: &HashMap<String, TrackMeta>,
    start: &str,
    curve: Curve,
    energy_threshold: f32,
    skip_penalty: &HashMap<(String, String), f32>,
) -> Vec<String> {
    if tracks.is_empty() {
        return Vec::new();
    }

    // Deterministic ordering. The GDScript relied on Dictionary key order,
    // which made its output depend on insertion history; sorting here means the
    // same pool always yields the same set.
    let mut keys: Vec<&String> = tracks.keys().collect();
    keys.sort();

    let start_href = if tracks.contains_key(start) {
        start.to_string()
    } else {
        keys[0].clone()
    };

    let penalty = |a: &str, b: &str| -> f32 {
        skip_penalty
            .get(&(a.to_string(), b.to_string()))
            .copied()
            .unwrap_or(0.0)
    };
    let step_cost = |a: &TrackMeta, b: &TrackMeta| -> f32 {
        transition_cost(a, b, energy_threshold, penalty(&a.href, &b.href))
    };

    let start_meta = &tracks[&start_href];
    let start_energy = start_meta.curve_energy();
    let start_bpm = start_meta.bpm;

    // The range this particular selection can travel, rather than a fixed
    // ±0.4 that a quiet library never reaches and a loud one saturates.
    let span = Span::of(tracks);

    let planned = MAX_PLANNED.min(tracks.len());
    let end_energy = curve.target_energy(start_energy, planned.saturating_sub(1), &span);
    let end_bpm = curve.target_bpm(start_bpm, planned.saturating_sub(1), &span);

    let mut open: Vec<State> = vec![State {
        path: vec![start_href.clone()],
        g: 0.0,
        f: 0.0,
    }];
    let mut best = State {
        path: vec![start_href.clone()],
        g: 0.0,
        f: 0.0,
    };
    let mut expansions = 0usize;

    while !open.is_empty() && expansions <= MAX_EXPANSIONS {
        expansions += 1;
        let current = open.remove(0);

        if current.path.len() > best.path.len()
            || (current.path.len() == best.path.len() && current.g < best.g)
        {
            best = State {
                path: current.path.clone(),
                g: current.g,
                f: current.f,
            };
        }
        if current.path.len() == planned {
            best = current;
            break;
        }

        let last = &tracks[current.path.last().expect("path is never empty")];
        let next_idx = current.path.len();
        // What has just been heard, most recent first, so the variety term in
        // `step_score` can see this branch's own history rather than the
        // queue's.
        let recent: Vec<String> = current.path.iter().rev().cloned().collect();

        // Score every unvisited candidate, then expand only the best few.
        let mut scored: Vec<(&String, f32)> = keys
            .iter()
            .filter(|h| !current.path.contains(**h))
            .map(|h| {
                let m = &tracks[*h];
                let cost = step_score(
                    tracks,
                    last,
                    m,
                    &recent,
                    curve,
                    start_energy,
                    start_bpm,
                    next_idx,
                    &span,
                    energy_threshold,
                    penalty(&last.href, &m.href),
                );
                (*h, cost)
            })
            .collect();
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        for (href, cost) in scored.into_iter().take(BEAM_WIDTH) {
            let mut path = current.path.clone();
            path.push(href.clone());
            let m = &tracks[href];
            let g = current.g + cost;
            let h = heuristic(
                path.len(),
                planned,
                m.curve_energy(),
                end_energy,
                m.bpm,
                end_bpm,
            );
            let f = g + h;

            // Keep `open` sorted by f, cheapest first.
            let idx = open.partition_point(|s| s.f <= f);
            open.insert(idx, State { path, g, f });
        }
    }

    // Greedy tail for anything the planner did not reach.
    let mut path = best.path;
    let mut remaining: Vec<&String> = keys
        .iter()
        .filter(|h| !path.contains(**h))
        .copied()
        .collect();

    while !remaining.is_empty() {
        let last = &tracks[path.last().expect("path is never empty")];
        let (best_i, _) = remaining
            .iter()
            .enumerate()
            .map(|(i, h)| (i, step_cost(last, &tracks[*h])))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .expect("remaining is non-empty");
        path.push(remaining.remove(best_i).clone());
    }

    path
}

/// Transition length from the outgoing outro and incoming intro.
///
/// Port of `calculate_transition_duration`. Clamped to 3–15 s: shorter reads as
/// a cut, longer outlasts most intros.
pub fn transition_duration(outro_len: f32, intro_len: f32) -> f32 {
    const MIN: f32 = 3.0;
    const MAX: f32 = 15.0;
    match (outro_len > 0.0, intro_len > 0.0) {
        (true, true) => outro_len.min(intro_len).clamp(MIN, MAX),
        (true, false) => outro_len.clamp(MIN, MAX),
        (false, true) => intro_len.clamp(MIN, MAX),
        (false, false) => 5.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> HashMap<String, TrackMeta> {
        let spec = [
            ("track_low", 110.0, 0.2),
            ("track_mid_1", 115.0, 0.4),
            ("track_mid_2", 120.0, 0.6),
            ("track_high", 125.0, 0.8),
        ];
        spec.iter()
            .map(|(href, bpm, energy)| {
                (
                    href.to_string(),
                    TrackMeta {
                        href: href.to_string(),
                        bpm: *bpm,
                        musical_key: "8A".into(),
                        energy_level: *energy,
                        genre: "Tech House".into(),
                        ..Default::default()
                    },
                )
            })
            .collect()
    }

    /// Ported from `test_a_star_pathfinding_optimization`.
    #[test]
    fn build_and_chill_curves_move_energy_in_opposite_directions() {
        let tracks = pool();
        let no_history = HashMap::new();

        let built = generate_mood_path(&tracks, "track_low", Curve::Build, 0.5, &no_history);
        assert_eq!(built.len(), 4, "every track should appear");
        assert_eq!(built[0], "track_low", "the set starts where asked");
        assert!(
            tracks[&built[3]].energy_level > tracks[&built[0]].energy_level,
            "a build should end higher than it started: {built:?}"
        );

        let chilled = generate_mood_path(&tracks, "track_high", Curve::Chill, 0.5, &no_history);
        assert!(
            tracks[&chilled[3]].energy_level < tracks[&chilled[0]].energy_level,
            "a chill should end lower than it started: {chilled:?}"
        );
    }

    #[test]
    fn every_track_appears_exactly_once() {
        let tracks = pool();
        let path = generate_mood_path(&tracks, "track_low", Curve::Build, 0.5, &HashMap::new());
        let mut sorted = path.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            tracks.len(),
            "duplicates or omissions: {path:?}"
        );
    }

    /// The GDScript iterated a Dictionary, so its output depended on insertion
    /// order. Sorting the key list makes the same pool yield the same set.
    #[test]
    fn output_is_deterministic() {
        let tracks = pool();
        let a = generate_mood_path(&tracks, "track_low", Curve::Build, 0.5, &HashMap::new());
        for _ in 0..5 {
            let b = generate_mood_path(&tracks, "track_low", Curve::Build, 0.5, &HashMap::new());
            assert_eq!(a, b, "ordering varied between runs");
        }
    }

    #[test]
    fn an_unknown_start_falls_back_rather_than_failing() {
        let tracks = pool();
        let path = generate_mood_path(&tracks, "nonexistent", Curve::Build, 0.5, &HashMap::new());
        assert_eq!(path.len(), 4);
    }

    #[test]
    fn an_empty_pool_yields_an_empty_path() {
        let empty = HashMap::new();
        assert!(generate_mood_path(&empty, "", Curve::Build, 0.5, &HashMap::new()).is_empty());
    }

    /// A learned dislike must actually push a pairing down the order.
    #[test]
    fn skip_history_influences_the_ordering() {
        let tracks = pool();
        let mut history = HashMap::new();
        history.insert(("track_low".to_string(), "track_mid_1".to_string()), 500.0);
        let path = generate_mood_path(&tracks, "track_low", Curve::Build, 0.5, &history);
        assert_ne!(
            path[1], "track_mid_1",
            "a heavily penalised pair was still chosen: {path:?}"
        );
    }

    /// The span the curve tests are written against: a full library's worth of
    /// range, so the shapes have somewhere to go.
    fn span() -> Span {
        Span {
            energy_floor: 0.1,
            energy_ceiling: 0.9,
            bpm_floor: 70.0,
            bpm_ceiling: 170.0,
        }
    }

    #[test]
    fn curve_targets_move_the_right_way() {
        let s = span();
        assert!(Curve::Build.target_energy(0.5, 9, &s) > 0.5);
        assert!(Curve::Chill.target_energy(0.5, 9, &s) < 0.5);
        assert_eq!(Curve::Flat.target_energy(0.5, 9, &s), 0.5);

        assert!(Curve::Build.target_bpm(120.0, 9, &s) > 120.0);
        assert!(Curve::Chill.target_bpm(120.0, 9, &s) < 120.0);
    }

    /// A wave returns near where it began — that is what distinguishes it from
    /// a build.
    #[test]
    fn a_wave_returns_toward_its_starting_energy() {
        let s = span();
        let start = 0.5;
        let end = Curve::Wave.target_energy(start, CURVE_STEPS, &s);
        let mid = Curve::Wave.target_energy(start, 2, &s);
        let trough = Curve::Wave.target_energy(start, 7, &s);
        assert!(mid > start, "a wave should rise first, got {mid}");
        assert!(trough < start, "a wave should then fall, got {trough}");
        assert!(
            (end - start).abs() < 0.05,
            "a wave should come back, ended at {end}"
        );
    }

    /// A set has no end, so neither does a curve. Past its span a Build holds
    /// the ceiling rather than wrapping, restarting, or running off the top —
    /// the bug this replaced re-planned from the playing track every ten
    /// tracks, so a Build was really nine small builds in a row.
    #[test]
    fn a_curve_is_defined_past_its_own_span() {
        let s = span();
        let top = Curve::Build.target_energy(0.3, CURVE_STEPS, &s);
        for step in CURVE_STEPS..CURVE_STEPS * 4 {
            let e = Curve::Build.target_energy(0.3, step, &s);
            assert!((e - top).abs() < 1e-5, "a build wandered at step {step}: {e}");
        }
        // And a wave keeps breathing rather than settling.
        let a = Curve::Wave.target_energy(0.5, 2, &s);
        let b = Curve::Wave.target_energy(0.5, 2 + CURVE_STEPS, &s);
        assert!((a - b).abs() < 1e-5, "the wave lost its period: {a} vs {b}");
    }

    /// The curve aims at the pool, not at a fixed offset. This is what makes
    /// "build" mean the top of the library rather than `start + 0.4` — which
    /// on a quiet library never reached the loud records and on a loud one
    /// saturated at 1.0 and stopped offering a gradient at all.
    #[test]
    fn a_build_aims_at_the_top_of_the_pool() {
        let quiet = Span {
            energy_floor: 0.05,
            energy_ceiling: 0.35,
            bpm_floor: 60.0,
            bpm_ceiling: 90.0,
        };
        let loud = span();
        let from_quiet = Curve::Build.target_energy(0.1, CURVE_STEPS, &quiet);
        let from_loud = Curve::Build.target_energy(0.1, CURVE_STEPS, &loud);
        assert!((from_quiet - 0.35).abs() < 1e-5, "{from_quiet}");
        assert!((from_loud - 0.9).abs() < 1e-5, "{from_loud}");
    }

    /// A start outside the pool's own band — the one loud record in an ambient
    /// library — still has somewhere to go.
    #[test]
    fn a_start_outside_the_span_still_has_room() {
        let s = span();
        assert!(Curve::Chill.target_energy(0.99, 5, &s) < 0.99);
        // And a build from above the ceiling asks for no movement rather than
        // for a drop.
        assert!(Curve::Build.target_energy(0.99, 5, &s) >= 0.99);
    }

    #[test]
    fn curve_targets_are_clamped_to_valid_energy() {
        let s = span();
        for i in 0..30 {
            let e = Curve::Build.target_energy(0.95, i, &s);
            assert!((0.0..=1.0).contains(&e), "energy left range: {e}");
            let e = Curve::Chill.target_energy(0.05, i, &s);
            assert!((0.0..=1.0).contains(&e), "energy left range: {e}");
            let e = Curve::Wave.target_energy(0.5, i, &s);
            assert!((0.0..=1.0).contains(&e), "energy left range: {e}");
        }
    }

    /// A span read off a real pool sits inside it, and ignores the outlier at
    /// each end.
    #[test]
    fn a_span_is_measured_off_the_pool_and_ignores_outliers() {
        let mut tracks = HashMap::new();
        for i in 0..20 {
            let href = format!("t{i:02}");
            tracks.insert(
                href.clone(),
                TrackMeta {
                    href,
                    bpm: 100.0 + i as f32,
                    energy_level: 0.3 + i as f32 * 0.01,
                    ..Default::default()
                },
            );
        }
        // One mis-analysed track at each end.
        tracks.insert(
            "junk-high".into(),
            TrackMeta {
                href: "junk-high".into(),
                bpm: 240.0,
                energy_level: 1.0,
                ..Default::default()
            },
        );
        let s = Span::of(&tracks);
        assert!(s.energy_ceiling < 0.6, "outlier became the target: {s:?}");
        assert!(s.bpm_ceiling < 130.0, "outlier became the target: {s:?}");
        assert!(s.energy_floor >= 0.3, "{s:?}");
    }

    // -----------------------------------------------------------------------
    // One step at a time — what the DJ actually does
    // -----------------------------------------------------------------------

    /// A library with a real range in it, tagged so the genre terms are live.
    fn library() -> HashMap<String, TrackMeta> {
        let spec: &[(&str, f32, &str, f32, &str, &str)] = &[
            ("calm/a", 70.0, "8A", 0.15, "Ambient", "calm"),
            ("calm/b", 72.0, "8B", 0.18, "Ambient", "calm"),
            ("calm/c", 68.0, "9A", 0.16, "Ambient", "calm"),
            ("calm/d", 74.0, "8A", 0.20, "Ambient", "calm"),
            ("mid/a", 95.0, "8A", 0.45, "Hip Hop", "mid"),
            ("mid/b", 98.0, "9A", 0.48, "Hip Hop", "mid"),
            ("mid/c", 92.0, "8B", 0.44, "Hip Hop", "mid"),
            ("mid/d", 100.0, "3A", 0.50, "Hip Hop", "mid"),
            ("floor/a", 126.0, "8A", 0.70, "House", "floor"),
            ("floor/b", 128.0, "9A", 0.72, "House", "floor"),
            ("floor/c", 130.0, "8B", 0.74, "House", "floor"),
            ("floor/d", 124.0, "3A", 0.68, "House", "floor"),
            ("hard/a", 172.0, "8A", 0.90, "Drum & Bass", "hard"),
            ("hard/b", 174.0, "9A", 0.92, "Drum & Bass", "hard"),
            ("hard/c", 170.0, "8B", 0.88, "Drum & Bass", "hard"),
            ("hard/d", 176.0, "3A", 0.91, "Drum & Bass", "hard"),
        ];
        spec.iter()
            .map(|(href, bpm, key, energy, genre, album)| {
                (
                    (*href).to_string(),
                    TrackMeta {
                        href: (*href).to_string(),
                        bpm: *bpm,
                        musical_key: (*key).to_string(),
                        energy_level: *energy,
                        genre: (*genre).to_string(),
                        artist: (*album).to_string(),
                        album: (*album).to_string(),
                        ..Default::default()
                    },
                )
            })
            .collect()
    }

    /// Walk a curve one step at a time, the way the DJ does.
    fn walk(curve: Curve, start: &str, steps: usize) -> Vec<String> {
        let tracks = library();
        let span = Span::of(&tracks);
        let empty = HashMap::new();
        let mut recent = vec![start.to_string()];
        let mut here = start.to_string();
        let mut out = Vec::new();
        for step in 1..=steps {
            let Some(next) = next_track(
                &tracks, &here, start, &recent, curve, step, &span, 0.5, &empty,
            ) else {
                break;
            };
            out.push(next.clone());
            recent.insert(0, next.clone());
            here = next;
        }
        out
    }

    /// The complaint, as a test: four curves that produced the same set.
    ///
    /// It was not a near-miss. With the old weights the curve term was worth
    /// about 0.09 a step against a key mode-shift's 2.5, so the planner
    /// optimised transition cost and paid the curve's fine, and Build, Chill,
    /// Wave and Hold returned very nearly the same ten tracks.
    #[test]
    fn the_four_curves_are_not_the_same_set() {
        let build = walk(Curve::Build, "mid/a", 6);
        let chill = walk(Curve::Chill, "mid/a", 6);
        let wave = walk(Curve::Wave, "mid/a", 6);
        let flat = walk(Curve::Flat, "mid/a", 6);
        for (name, other) in [("chill", &chill), ("wave", &wave), ("flat", &flat)] {
            assert_ne!(&build, other, "build and {name} produced the same set");
        }
        assert_ne!(chill, flat, "chill and hold produced the same set");
    }

    /// And the set has to arrive somewhere inside the ten tracks a listener
    /// can see, which is the whole of what "build" promises.
    #[test]
    fn a_build_climbs_and_a_chill_descends_within_the_visible_set() {
        let tracks = library();
        let energy = |h: &String| tracks[h].curve_energy();

        let build = walk(Curve::Build, "calm/a", 8);
        let climbed = energy(build.last().unwrap()) - tracks["calm/a"].curve_energy();
        assert!(climbed > 0.3, "a build gained only {climbed}: {build:?}");

        let chill = walk(Curve::Chill, "hard/a", 8);
        let fell = tracks["hard/a"].curve_energy() - energy(chill.last().unwrap());
        assert!(fell > 0.3, "a chill lost only {fell}: {chill:?}");
    }

    /// A wave has to be a wave: up, then back down, inside the visible set.
    #[test]
    fn a_wave_rises_and_falls_within_the_visible_set() {
        let tracks = library();
        let start = tracks["mid/a"].curve_energy();
        let path = walk(Curve::Wave, "mid/a", CURVE_STEPS);
        let energies: Vec<f32> = path.iter().map(|h| tracks[h].curve_energy()).collect();
        let peak = energies.iter().take(5).cloned().fold(f32::MIN, f32::max);
        let trough = energies.iter().skip(4).cloned().fold(f32::MAX, f32::min);
        assert!(peak > start + 0.1, "the wave never rose: {energies:?}");
        assert!(trough < start - 0.05, "the wave never fell: {energies:?}");
    }

    /// The other half of the complaint: twelve tracks from one album in a row.
    ///
    /// Nothing in the old model could tell two records apart — 488 of 534
    /// tracks in the library it was reported on carry no genre tag, so
    /// `genre_distance` answered `UNKNOWN_COST` for every pair — and with key,
    /// tempo and loudness the only live signals the cheapest neighbour of a
    /// track is reliably the next track off the same record.
    #[test]
    fn a_hold_does_not_park_on_one_album() {
        let tracks = library();
        let path = walk(Curve::Flat, "mid/a", 6);
        let albums: Vec<&str> = path.iter().map(|h| tracks[h].album.as_str()).collect();
        let mut runs = 1;
        let mut longest = 1;
        for pair in albums.windows(2) {
            if pair[0] == pair[1] {
                runs += 1;
                longest = longest.max(runs);
            } else {
                runs = 1;
            }
        }
        assert!(longest <= 2, "parked on one album: {albums:?}");
    }

    /// A step never repeats what is already in the set.
    #[test]
    fn a_step_never_offers_a_track_already_played() {
        let path = walk(Curve::Build, "calm/a", 12);
        let mut seen = std::collections::HashSet::new();
        for href in &path {
            assert!(seen.insert(href.clone()), "{href} came round twice: {path:?}");
        }
    }

    /// Nothing left to play is `None`, not a panic and not a repeat.
    #[test]
    fn a_pool_with_nothing_new_in_it_has_no_next_track() {
        let tracks = library();
        let span = Span::of(&tracks);
        let recent: Vec<String> = tracks.keys().cloned().collect();
        assert_eq!(
            next_track(
                &tracks, "mid/a", "mid/a", &recent, Curve::Build, 1, &span, 0.5,
                &HashMap::new()
            ),
            None
        );
    }

    /// An href the pool does not hold cannot be planned from.
    #[test]
    fn an_unknown_start_has_no_next_track() {
        let tracks = library();
        let span = Span::of(&tracks);
        assert_eq!(
            next_track(
                &tracks, "nope", "nope", &[], Curve::Build, 1, &span, 0.5, &HashMap::new()
            ),
            None
        );
    }

    /// A learned dislike reaches the step chooser too, not only the A*.
    #[test]
    fn skip_history_reaches_the_step_chooser() {
        let tracks = library();
        let span = Span::of(&tracks);
        let empty = HashMap::new();
        let recent = vec!["calm/a".to_string()];
        let first = next_track(
            &tracks, "calm/a", "calm/a", &recent, Curve::Flat, 1, &span, 0.5, &empty,
        )
        .expect("a pool this size always has a next track");

        let mut history = HashMap::new();
        history.insert(("calm/a".to_string(), first.clone()), 500.0);
        let second = next_track(
            &tracks, "calm/a", "calm/a", &recent, Curve::Flat, 1, &span, 0.5, &history,
        );
        assert_ne!(second, Some(first), "a heavily penalised pair was still chosen");
    }

    /// The genre ordering has to place the shelves in the order a listener
    /// would, since that is the whole reason it exists.
    #[test]
    fn the_genre_ordering_runs_from_ambient_to_metal() {
        let of = |g: &str| crate::genre_intensity(g).unwrap_or_else(|| panic!("no score for {g}"));
        // The band ladder the complaint named. House is deliberately absent:
        // it sits level with rock rather than between hip hop and it, and
        // forcing a total order across two different families would be an
        // opinion the table should not hold.
        let ladder = ["Ambient", "Folk", "Hip Hop", "Rock", "Metal"];
        for pair in ladder.windows(2) {
            assert!(
                of(pair[0]) < of(pair[1]),
                "{} did not rank below {}",
                pair[0],
                pair[1]
            );
        }
        // And the electronic ladder the same way.
        let electronic = ["Downtempo", "House", "Dubstep", "Drum & Bass"];
        for pair in electronic.windows(2) {
            assert!(of(pair[0]) < of(pair[1]), "{pair:?} out of order");
        }
    }

    /// An untagged library must read exactly as it did, since that is most of
    /// a folder-organised one and nothing there should move.
    #[test]
    fn an_untagged_track_reads_its_measured_loudness_unchanged() {
        let t = TrackMeta {
            energy_level: 0.42,
            ..Default::default()
        };
        assert_eq!(t.curve_energy(), 0.42);
    }

    #[test]
    fn curve_parsing_accepts_the_ui_spellings() {
        assert_eq!(Curve::parse("build"), Curve::Build);
        assert_eq!(Curve::parse("Build Vibe"), Curve::Build);
        assert_eq!(Curve::parse("chill_down"), Curve::Chill);
        assert_eq!(Curve::parse("WAVE"), Curve::Wave);
        assert_eq!(Curve::parse("nonsense"), Curve::Flat);
    }

    /// Ported from `test_dynamic_duration_selection`.
    #[test]
    fn transition_duration_uses_the_shorter_segment() {
        assert_eq!(transition_duration(20.0, 8.0), 8.0);
        assert_eq!(transition_duration(8.0, 20.0), 8.0);
        assert_eq!(transition_duration(20.0, 0.0), 15.0, "clamped to the max");
        assert_eq!(transition_duration(1.0, 0.0), 3.0, "clamped to the min");
        assert_eq!(transition_duration(0.0, 0.0), 5.0, "default when unknown");
    }

    /// A large pool must not stall: the expansion cap and beam width exist to
    /// bound the search, and this fails loudly if either is removed.
    #[test]
    fn a_large_pool_completes() {
        let mut tracks = HashMap::new();
        for i in 0..200 {
            let href = format!("t{i:03}");
            tracks.insert(
                href.clone(),
                TrackMeta {
                    href,
                    bpm: 110.0 + (i % 30) as f32,
                    musical_key: format!("{}A", (i % 12) + 1),
                    energy_level: (i % 10) as f32 / 10.0,
                    genre: "Tech House".into(),
                    ..Default::default()
                },
            );
        }
        let path = generate_mood_path(&tracks, "t000", Curve::Build, 0.5, &HashMap::new());
        assert_eq!(path.len(), 200, "every track should still be ordered");
    }
}

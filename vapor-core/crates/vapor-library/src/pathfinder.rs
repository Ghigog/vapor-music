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

/// Curve weights, from the constants at the top of `dj_pathfinder.gd`.
pub const WEIGHT_CURVE_ENERGY: f32 = 8.0;
pub const WEIGHT_CURVE_BPM: f32 = 0.3;

/// Longest path the A* stage plans. Beyond this the greedy tail takes over.
const MAX_PLANNED: usize = 10;
/// Successors expanded per node — the beam width.
const BEAM_WIDTH: usize = 6;
/// Hard cap on expansions, so a large library cannot stall the search.
const MAX_EXPANSIONS: usize = 1500;

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

    /// Energy the set should be at, `index` steps into a path of `total`.
    pub fn target_energy(self, start: f32, index: usize, total: usize) -> f32 {
        if total <= 1 {
            return start;
        }
        let t = index as f32 / (total - 1) as f32;
        match self {
            Curve::Build => lerp(start, (start + 0.4).min(1.0), t),
            Curve::Chill => lerp(start, (start - 0.4).max(0.0), t),
            Curve::Wave => (start + 0.3 * (t * std::f32::consts::PI * 2.0).sin()).clamp(0.0, 1.0),
            Curve::Flat => start,
        }
    }

    /// Tempo the set should be at, `index` steps into a path of `total`.
    pub fn target_bpm(self, start: f32, index: usize, total: usize) -> f32 {
        if total <= 1 {
            return start;
        }
        let t = index as f32 / (total - 1) as f32;
        match self {
            Curve::Build => lerp(start, start + 15.0, t),
            Curve::Chill => lerp(start, start - 15.0, t),
            Curve::Wave => start + 10.0 * (t * std::f32::consts::PI * 2.0).sin(),
            Curve::Flat => start,
        }
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
    let start_energy = start_meta.energy_level;
    let start_bpm = start_meta.bpm;

    let planned = MAX_PLANNED.min(tracks.len());
    let end_energy = curve.target_energy(start_energy, planned.saturating_sub(1), planned);
    let end_bpm = curve.target_bpm(start_bpm, planned.saturating_sub(1), planned);

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
        let target_energy = curve.target_energy(start_energy, next_idx, planned);
        let target_bpm = curve.target_bpm(start_bpm, next_idx, planned);

        // Score every unvisited candidate, then expand only the best few.
        let mut scored: Vec<(&String, f32)> = keys
            .iter()
            .filter(|h| !current.path.contains(**h))
            .map(|h| {
                let m = &tracks[*h];
                let cost = step_cost(last, m)
                    + curve_cost(m.energy_level, m.bpm, target_energy, target_bpm);
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
                m.energy_level,
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

    #[test]
    fn curve_targets_move_the_right_way() {
        assert!(Curve::Build.target_energy(0.5, 9, 10) > 0.5);
        assert!(Curve::Chill.target_energy(0.5, 9, 10) < 0.5);
        assert_eq!(Curve::Flat.target_energy(0.5, 9, 10), 0.5);

        assert!(Curve::Build.target_bpm(120.0, 9, 10) > 120.0);
        assert!(Curve::Chill.target_bpm(120.0, 9, 10) < 120.0);
    }

    /// A wave returns near where it began — that is what distinguishes it from
    /// a build.
    #[test]
    fn a_wave_returns_toward_its_starting_energy() {
        let start = 0.5;
        let end = Curve::Wave.target_energy(start, 9, 10);
        let mid = Curve::Wave.target_energy(start, 2, 10);
        assert!(mid > start, "a wave should rise first, got {mid}");
        assert!(
            (end - start).abs() < 0.15,
            "a wave should come back down, ended at {end}"
        );
    }

    #[test]
    fn curve_targets_are_clamped_to_valid_energy() {
        for i in 0..10 {
            let e = Curve::Build.target_energy(0.95, i, 10);
            assert!((0.0..=1.0).contains(&e), "energy left range: {e}");
            let e = Curve::Chill.target_energy(0.05, i, 10);
            assert!((0.0..=1.0).contains(&e), "energy left range: {e}");
        }
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

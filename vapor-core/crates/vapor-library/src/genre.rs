//! Genre taxonomy and distance.
//!
//! Port of the taxonomy half of `dj_pathfinder.gd`. Genres form a tree
//! (Club Music → House → Tech House); distance is the number of edges between
//! two genres in that tree, found by breadth-first search.
//!
//! The tree is embedded rather than read from `assets/genre_taxonomy.json` at
//! runtime. The GDScript already carried an identical hardcoded fallback for
//! when the file failed to load, so there were effectively two copies; keeping
//! the compiled-in one means the library has no file dependency and works
//! unchanged in the browser.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

/// Cost returned when a genre is unknown, absent, or not in the tree.
pub const UNRELATED_COST: f32 = 5.0;

/// The taxonomy, matching `assets/genre_taxonomy.json`.
const TAXONOMY: &[(&str, &[&str])] = &[
    ("Club Music", &["House", "Techno"]),
    (
        "House",
        &[
            "Tech House",
            "Deep House",
            "Progressive House",
            "Minimal House",
        ],
    ),
    ("Techno", &["Acid Techno", "Minimal Techno"]),
    ("Bass Music", &["Drum & Bass"]),
    (
        "Drum & Bass",
        &["Liquid DNB", "Neurofunk", "Jungle", "Drum and Bass"],
    ),
    ("Rock", &["Alternative Rock", "Classic Rock", "Hard Rock"]),
];

/// Where a genre's tempo actually sits, in BPM.
///
/// Beat trackers are reliable about the *pulse* and unreliable about which
/// octave of it to report — a drum & bass track at 174 has a half-time feel at
/// 87 and both readings are defensible from the signal alone. Essentia and this
/// crate's own estimator agree on 87 for Delta Heavy's "Space Time"; neither is
/// broken, and neither can tell 87-because-hip-hop from 87-because-half-of-174.
///
/// A genre can. Nothing else the app measures distinguishes those two cases,
/// which is why this table exists rather than another signal-processing
/// attempt: it is knowledge about music, not about audio.
///
/// Bands are deliberately generous. The job is to choose between `bpm`,
/// `bpm * 2` and `bpm / 2`, and those are a factor of two apart — so a band
/// only has to be right to within about 40% to pick correctly, and a narrow one
/// would refuse to answer for perfectly ordinary records.
const TEMPO_BANDS: &[(&str, f32, f32)] = &[
    ("drum & bass", 160.0, 185.0),
    ("drum and bass", 160.0, 185.0),
    ("liquid dnb", 160.0, 185.0),
    ("neurofunk", 160.0, 185.0),
    ("jungle", 155.0, 185.0),
    ("dubstep", 130.0, 150.0),
    ("house", 115.0, 132.0),
    ("tech house", 118.0, 132.0),
    ("deep house", 110.0, 128.0),
    ("progressive house", 120.0, 134.0),
    ("minimal house", 118.0, 130.0),
    ("techno", 120.0, 150.0),
    ("acid techno", 125.0, 150.0),
    ("minimal techno", 120.0, 140.0),
    ("trance", 130.0, 145.0),
    ("garage", 128.0, 138.0),
    ("hip hop", 70.0, 110.0),
    ("hip-hop", 70.0, 110.0),
    ("rap", 70.0, 110.0),
    ("reggae", 60.0, 100.0),
    ("dub", 60.0, 100.0),
    ("disco", 100.0, 130.0),
    ("funk", 90.0, 125.0),
    ("soul", 60.0, 120.0),
    ("rock", 90.0, 160.0),
    ("metal", 100.0, 200.0),
    ("ambient", 50.0, 120.0),
];

/// The tempo band a genre is played at, if this crate knows one.
///
/// Matched on the most specific name first, then on any parent in the taxonomy,
/// so "Liquid DNB" answers even when only "Drum & Bass" is listed.
pub fn tempo_band(genre: &str) -> Option<(f32, f32)> {
    let g = genre.trim().to_lowercase();
    if g.is_empty() {
        return None;
    }
    if let Some((_, lo, hi)) = TEMPO_BANDS.iter().find(|(name, _, _)| *name == g) {
        return Some((*lo, *hi));
    }
    // A genre this table does not list may still be a child of one it does.
    graph().get(&g).and_then(|neighbours| {
        neighbours.iter().find_map(|n| {
            TEMPO_BANDS
                .iter()
                .find(|(name, _, _)| name == n)
                .map(|(_, lo, hi)| (*lo, *hi))
        })
    })
}

/// The tempo a trusted reference says this track is really at.
///
/// Better evidence than [`octave_correct`] and used in preference to it: a
/// per-track number beats a guess from the genre a record was filed under, and
/// it works for the great majority of a library that carries no genre tag at
/// all.
///
/// Only ever returns an *octave* of the measured tempo, never the reference
/// value itself. The reference can be a different mix, a radio edit, or simply
/// wrong, and its job here is narrow: to say which octave of a pulse this crate
/// already found is the one a listener counts. Replacing 87.0 with a stranger's
/// 173.7 would import their error along with their answer, so 87.0 becomes
/// exactly 174.0.
///
/// `None` when the two already agree, when the reference is missing or absurd,
/// and when no octave lands close enough to be the same pulse.
pub fn octave_from_reference(measured: f32, reference: f32) -> Option<f32> {
    if !measured.is_finite() || measured <= 0.0 {
        return None;
    }
    // A reference of zero is Deezer's "we do not know", not a tempo, and the
    // band beyond it is nothing any music is played at.
    if !reference.is_finite() || !(20.0..=300.0).contains(&reference) {
        return None;
    }

    // Within this of the reference counts as the same reading. Wide enough to
    // absorb the disagreement between two estimators on the same track — they
    // routinely differ by a percent or so — and far narrower than the factor of
    // two that separates the octaves being chosen between.
    const TOLERANCE: f32 = 0.06;
    let agrees = |a: f32| (a / reference - 1.0).abs() <= TOLERANCE;

    if agrees(measured) {
        return None;
    }
    // Halves and doubles only. A reference that agrees with none of them is
    // describing a different recording, and the right answer is to keep what
    // was measured here.
    [2.0f32, 0.5, 4.0, 0.25]
        .into_iter()
        .map(|m| measured * m)
        .find(|candidate| agrees(*candidate))
}

/// The tempo a genre says this track is really at.
///
/// Returns `Some` only when the detected tempo is *outside* the genre's band
/// and exactly one octave of it lands inside — the case where the reading is a
/// half or double of the truth and the genre resolves which. Anything else
/// returns `None`, including a tempo already in the band, an unknown genre, and
/// the ambiguous case where two octaves both fit.
///
/// Deliberately conservative. Being wrong here does not produce a slightly odd
/// suggestion, it doubles a number the whole app reasons with.
pub fn octave_correct(bpm: f32, genre: &str) -> Option<f32> {
    if !bpm.is_finite() || bpm <= 0.0 {
        return None;
    }
    let (lo, hi) = tempo_band(genre)?;
    let inside = |b: f32| b >= lo && b <= hi;

    if inside(bpm) {
        return None;
    }
    let candidates: Vec<f32> = [bpm * 2.0, bpm / 2.0, bpm * 4.0, bpm / 4.0]
        .into_iter()
        .filter(|b| inside(*b))
        .collect();

    // Exactly one octave fitting is the only unambiguous answer. Two fitting
    // means the band spans an octave and cannot decide.
    match candidates.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// Undirected adjacency, keyed by lowercased genre name.
fn graph() -> &'static HashMap<String, Vec<String>> {
    static GRAPH: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    GRAPH.get_or_init(|| {
        let mut g: HashMap<String, Vec<String>> = HashMap::new();
        let mut link = |a: &str, b: &str| {
            let (a, b) = (a.to_lowercase(), b.to_lowercase());
            g.entry(a.clone()).or_default();
            g.entry(b.clone()).or_default();
            if let Some(v) = g.get_mut(&a) {
                if !v.contains(&b) {
                    v.push(b.clone());
                }
            }
            if let Some(v) = g.get_mut(&b) {
                if !v.contains(&a) {
                    v.push(a);
                }
            }
        };
        for (parent, children) in TAXONOMY {
            for child in *children {
                link(parent, child);
            }
        }
        g
    })
}

/// Resolve a genre string to a node, allowing substring matches.
///
/// The substring fallback is inherited from the GDScript: real tag data is
/// messy ("Progressive House / Melodic") and an exact-match-only lookup sends
/// most of a real library to the unknown-genre penalty.
fn find_node(genre: &str) -> Option<&'static String> {
    let g = graph();
    let key = genre.trim().to_lowercase();
    if let Some((k, _)) = g.get_key_value(&key) {
        return Some(k);
    }
    g.keys()
        .find(|k| k.contains(&key) || key.contains(k.as_str()))
}

/// Edge distance between two genres in the taxonomy.
///
/// Returns [`UNRELATED_COST`] when either is unknown or when no path exists.
pub fn genre_distance(a: &str, b: &str) -> f32 {
    let (ca, cb) = (a.trim().to_lowercase(), b.trim().to_lowercase());

    if ca.is_empty() || cb.is_empty() || ca == "unknown" || cb == "unknown" {
        return UNRELATED_COST;
    }
    if ca == cb {
        return 0.0;
    }

    let (Some(start), Some(goal)) = (find_node(&ca), find_node(&cb)) else {
        return UNRELATED_COST;
    };
    if start == goal {
        return 0.0;
    }

    let g = graph();
    let mut queue = VecDeque::from([(start, 0u32)]);
    let mut seen = HashSet::from([start]);

    while let Some((node, dist)) = queue.pop_front() {
        if node == goal {
            return dist as f32;
        }
        for n in g.get(node).map(|v| v.as_slice()).unwrap_or(&[]) {
            if let Some((k, _)) = g.get_key_value(n) {
                if seen.insert(k) {
                    queue.push_back((k, dist + 1));
                }
            }
        }
    }
    UNRELATED_COST
}

/// Loose similarity check used to bucket "interesting" versus "creative"
/// candidates. Deliberately more permissive than [`genre_distance`].
pub fn is_similar_genre(a: &str, b: &str) -> bool {
    if is_unknown_genre(a) || is_unknown_genre(b) {
        return false;
    }
    let (ca, cb) = (a.trim().to_lowercase(), b.trim().to_lowercase());
    ca == cb || ca.contains(&cb) || cb.contains(&ca)
}

/// Whether a genre string says anything.
///
/// `"unknown"` was the only placeholder recognised, and taggers do not agree on
/// it: a real library carries `"Unknown genre"`, `"Other"` and `"Genre"` too,
/// and each was being treated as the name of a genre — so two tracks tagged
/// `"Unknown genre"` counted as a genre *match*, and matched nothing else.
pub fn is_unknown_genre(g: &str) -> bool {
    let g = g.trim().to_lowercase();
    g.is_empty()
        || g == "unknown"
        || g == "unknown genre"
        || g == "other"
        || g == "genre"
        || g == "none"
        || g == "n/a"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `test_genre_taxonomy_cost`.
    #[test]
    fn distances_match_the_godot_values() {
        assert_eq!(genre_distance("Tech House", "Techno"), 3.0);
        assert_eq!(genre_distance("House", "Liquid DNB"), 5.0);
    }

    #[test]
    fn identical_genres_cost_nothing() {
        assert_eq!(genre_distance("Techno", "Techno"), 0.0);
        assert_eq!(genre_distance("techno", "  TECHNO "), 0.0);
    }

    #[test]
    fn unknown_genres_take_the_penalty() {
        assert_eq!(genre_distance("", "Techno"), UNRELATED_COST);
        assert_eq!(genre_distance("Unknown", "Techno"), UNRELATED_COST);
        assert_eq!(genre_distance("Polka", "Techno"), UNRELATED_COST);
    }

    /// Siblings are closer than cousins, which is what makes the cost useful
    /// for ranking rather than merely non-zero.
    #[test]
    fn siblings_are_closer_than_distant_relatives() {
        let siblings = genre_distance("Tech House", "Deep House");
        let cousins = genre_distance("Tech House", "Acid Techno");
        assert!(
            siblings < cousins,
            "siblings {siblings} should beat cousins {cousins}"
        );
    }

    #[test]
    fn distance_is_symmetric() {
        for (a, b) in [("Tech House", "Techno"), ("House", "Jungle")] {
            assert_eq!(genre_distance(a, b), genre_distance(b, a), "{a} <-> {b}");
        }
    }

    /// Real tag data is messy; the substring fallback is what keeps a live
    /// library out of the unknown-genre penalty.
    #[test]
    fn messy_tags_still_resolve() {
        assert!(genre_distance("Tech House / Melodic", "Techno") < UNRELATED_COST);
    }

    #[test]
    fn similarity_is_looser_than_distance() {
        assert!(is_similar_genre("Tech House", "tech house"));
        assert!(is_similar_genre("Tech House", "House"));
        assert!(!is_similar_genre("Unknown", "House"));
        assert!(!is_similar_genre("", "House"));
    }

    // -----------------------------------------------------------------------
    // Tempo octaves, which a genre can resolve and a beat tracker cannot
    // -----------------------------------------------------------------------

    /// The case that started this: Delta Heavy's "Space Time" is drum & bass at
    /// 174 and reads as 87. Both this crate's estimator and Essentia say 87, so
    /// neither is broken — 87 is the half-time feel, and nothing in the signal
    /// says which octave a listener would count.
    ///
    /// At 87 it sits beside every chill hip hop track in the library and the DJ
    /// calls it a match, because by tempo, key and energy it *is* one.
    #[test]
    fn drum_and_bass_read_at_half_tempo_is_corrected() {
        assert_eq!(octave_correct(87.0, "Drum & Bass"), Some(174.0));
        assert_eq!(octave_correct(86.9, "drum and bass"), Some(173.8));
        // Through the taxonomy, for a genre the band table does not list.
        assert_eq!(octave_correct(87.0, "Liquid DNB"), Some(174.0));
    }

    /// And the track it was wrongly matched against stays where it is.
    #[test]
    fn hip_hop_at_eighty_seven_is_already_right() {
        assert_eq!(octave_correct(87.0, "Hip Hop"), None);
        assert_eq!(octave_correct(95.0, "rap"), None);
    }

    /// Double-time readings too, which happen to slower music.
    #[test]
    fn a_doubled_reading_is_halved_back() {
        assert_eq!(octave_correct(160.0, "Hip Hop"), Some(80.0));
        assert_eq!(octave_correct(240.0, "House"), Some(120.0));
    }

    /// Silence is the right answer far more often than a guess.
    #[test]
    fn nothing_is_claimed_without_grounds() {
        // No genre, or one with no known band.
        assert_eq!(octave_correct(87.0, ""), None);
        assert_eq!(octave_correct(87.0, "Shoegaze"), None);
        // Already inside the band.
        assert_eq!(octave_correct(174.0, "Drum & Bass"), None);
        // Nonsense in, nothing out.
        assert_eq!(octave_correct(0.0, "Drum & Bass"), None);
        assert_eq!(octave_correct(-5.0, "Drum & Bass"), None);
        assert_eq!(octave_correct(f32::NAN, "Drum & Bass"), None);
    }

    /// A band an octave wide cannot choose, and must not pretend to.
    #[test]
    fn an_ambiguous_band_refuses_rather_than_guesses() {
        // Metal spans 100-200, so 50 could be doubled to 100 or quadrupled to
        // 200 — both inside. No answer is the only honest one.
        assert_eq!(octave_correct(50.0, "metal"), None);
    }

    /// The correction only ever moves a tempo by a factor of two or four, never
    /// to an arbitrary number: it resolves which octave, it does not re-measure.
    #[test]
    fn a_correction_is_always_an_octave_of_the_original() {
        for (bpm, genre) in [
            (87.0f32, "Drum & Bass"),
            (160.0, "Hip Hop"),
            (240.0, "House"),
        ] {
            let Some(fixed) = octave_correct(bpm, genre) else {
                continue;
            };
            let ratio = fixed / bpm;
            assert!(
                [0.25f32, 0.5, 2.0, 4.0]
                    .iter()
                    .any(|r| (ratio - r).abs() < 1e-3),
                "{genre} {bpm} -> {fixed} is not an octave ({ratio})"
            );
        }
    }

    #[test]
    fn a_band_is_found_for_a_child_genre() {
        assert_eq!(tempo_band("Neurofunk"), Some((160.0, 185.0)));
        assert_eq!(tempo_band("Tech House"), Some((118.0, 132.0)));
        assert_eq!(tempo_band("  DRUM & BASS  "), Some((160.0, 185.0)));
        assert_eq!(tempo_band("Sea Shanty"), None);
        assert_eq!(tempo_band(""), None);
    }

    // -----------------------------------------------------------------------
    // Resolving an octave against a per-track reference
    // -----------------------------------------------------------------------

    /// The case from the owner's library: Knife Party's "Bonfire" measures 87.0
    /// here and Deezer reports 173.7. Both describe the same pulse; only one is
    /// the tempo a listener counts.
    #[test]
    fn a_reference_resolves_a_half_time_reading() {
        assert_eq!(octave_from_reference(87.0, 173.7), Some(174.0));
        assert_eq!(octave_from_reference(86.9, 172.3), Some(173.8));
    }

    /// The corrected value is an octave of *our* measurement, never the
    /// reference. Their number may carry their own error, and this borrows
    /// their judgement about the octave rather than their arithmetic.
    #[test]
    fn the_result_is_an_octave_of_the_measurement_not_the_reference() {
        let fixed = octave_from_reference(87.0, 173.7).expect("a correction");
        assert_eq!(fixed, 174.0, "took the reference verbatim");
        assert_ne!(fixed, 173.7);
    }

    #[test]
    fn a_double_time_reading_is_halved() {
        assert_eq!(octave_from_reference(160.0, 80.0), Some(80.0));
    }

    /// Agreement needs no correction, and near-agreement is still agreement:
    /// two estimators differ by a percent or so on the same track.
    #[test]
    fn nothing_is_changed_when_the_two_already_agree() {
        assert_eq!(octave_from_reference(123.0, 123.0), None);
        assert_eq!(octave_from_reference(119.2, 120.0), None);
        assert_eq!(octave_from_reference(174.0, 173.7), None);
    }

    /// Deezer writes 0 when it does not know, which is most of the time. That
    /// is not a tempo and must never be treated as one.
    #[test]
    fn a_missing_reference_is_not_a_tempo() {
        assert_eq!(octave_from_reference(87.0, 0.0), None);
        assert_eq!(octave_from_reference(87.0, f32::NAN), None);
        assert_eq!(octave_from_reference(87.0, 5.0), None);
        assert_eq!(octave_from_reference(87.0, 900.0), None);
        assert_eq!(octave_from_reference(0.0, 174.0), None);
    }

    /// A reference that is neither our reading nor an octave of it is
    /// describing a different recording — a remix, a radio edit, a mismatch.
    /// Keep what was measured here.
    #[test]
    fn an_unrelated_reference_is_ignored() {
        assert_eq!(octave_from_reference(87.0, 128.0), None);
        assert_eq!(octave_from_reference(140.0, 100.0), None);
    }
}

//! Genre taxonomy and distance.
//!
//! Port of the taxonomy half of `dj_pathfinder.gd`. Genres form a tree
//! (Club Music → House → Tech House); distance is the number of edges between
//! two genres in that tree, found by breadth-first search.
//!
//! The tree is embedded rather than read from a file at runtime. The Godot
//! build kept it in `assets/genre_taxonomy.json` *and* as a hardcoded fallback
//! in GDScript for when the file failed to load, so there were two copies of it
//! already; the compiled-in one was kept because it gives the library no file
//! dependency and works unchanged in the browser. The JSON went with the Godot
//! tree, so this is now the only copy.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

/// Cost returned when a genre is unknown, absent, or not in the tree.
pub const UNRELATED_COST: f32 = 5.0;

/// The taxonomy. Sole copy since the Godot tree was removed.
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
///
/// **Every key here must already be in [`normalise`] form** — lowercase, runs
/// of alphanumerics separated by single spaces, `&` spelled `and`. Lookups are
/// normalised before they are compared, so a key that is not would be dead
/// weight that nothing could ever match. `keys_are_already_normalised` fails
/// the build's test run if one slips in.
///
/// The spellings are here because the table is the *only* thing standing
/// between a half-read drum & bass record and the DJ treating it as hip hop,
/// and taggers do not agree on a single name for that genre. AUD-26 measured
/// the cost: 108 of 563 corpus entries pile up in the 84–90 BPM band, and the
/// correction that fixes them never fired because the lookup was exact string
/// equality and the tags said "DnB", "D&B" or "Breakbeat".
const TEMPO_BANDS: &[(&str, f32, f32)] = &[
    // Drum & bass, under every spelling a tagger has been seen to use.
    ("drum and bass", 160.0, 185.0),
    ("drum n bass", 160.0, 185.0),
    ("drumandbass", 160.0, 185.0),
    ("drumnbass", 160.0, 185.0),
    ("dnb", 160.0, 185.0),
    ("d n b", 160.0, 185.0),
    ("d and b", 160.0, 185.0),
    ("liquid dnb", 160.0, 185.0),
    ("liquid drum and bass", 160.0, 185.0),
    ("liquid funk", 160.0, 185.0),
    ("neurofunk", 160.0, 185.0),
    ("neuro", 160.0, 185.0),
    ("jump up", 160.0, 185.0),
    ("drumstep", 160.0, 185.0),
    ("halftime", 160.0, 185.0),
    // Breakbeat is filed with drum & bass rather than given a band of its own.
    // It is the tag this library's jungle and DnB actually carries, and the
    // 130–140 nu-skool records it also covers are safe: 132 is outside
    // 160–185 and so are 66 and 264, so `octave_correct` finds no candidate
    // and leaves them exactly where they are.
    ("breakbeat", 160.0, 185.0),
    ("breakbeats", 160.0, 185.0),
    ("jungle", 155.0, 185.0),
    ("ragga jungle", 155.0, 185.0),
    ("dubstep", 130.0, 150.0),
    ("brostep", 130.0, 150.0),
    ("riddim", 135.0, 150.0),
    ("house", 115.0, 132.0),
    ("tech house", 118.0, 132.0),
    ("deep house", 110.0, 128.0),
    ("progressive house", 120.0, 134.0),
    ("minimal house", 118.0, 130.0),
    ("techno", 120.0, 150.0),
    ("acid techno", 125.0, 150.0),
    ("minimal techno", 120.0, 140.0),
    ("trance", 130.0, 145.0),
    ("psytrance", 135.0, 150.0),
    ("psy trance", 135.0, 150.0),
    ("psychedelic trance", 135.0, 150.0),
    ("garage", 128.0, 138.0),
    ("uk garage", 128.0, 138.0),
    ("2 step", 128.0, 138.0),
    ("2step", 128.0, 138.0),
    ("hip hop", 70.0, 110.0),
    ("hiphop", 70.0, 110.0),
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

/// A genre name reduced to the form [`TEMPO_BANDS`] is keyed by.
///
/// Lowercased; `&` spelled out, so "D&B" and "D and B" are one name; and every
/// other run of punctuation or whitespace collapsed to a single space, so
/// "Drum'n'Bass", "drum-n-bass" and "Drum   N   Bass" are one name too.
///
/// Deliberately *not* used by [`genre_distance`] or [`find_node`], which key
/// the taxonomy graph on a plain lowercase and have their own substring
/// fallback. Changing what those two consider equal would move mix costs, and
/// this is a tempo lookup, not a taxonomy change.
fn normalise(genre: &str) -> String {
    genre
        .replace('&', " and ")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The tempo band a genre is played at, if this crate knows one.
///
/// Matched on the most specific name first, then on any parent in the taxonomy,
/// so "Liquid DNB" answers even when only "Drum & Bass" is listed.
///
/// A tag field holding several genres — "Drum & Bass / Neurofunk", the shape a
/// file tag and a multi-genre service response both arrive in — is tried a
/// segment at a time, and the first segment with a band wins. First rather than
/// some reconciliation of all of them because the alternative is refusing to
/// answer whenever a record is filed under two things, which is most of the
/// interesting ones; and because the segments of a real tag are near enough
/// always the same music described at two levels of detail.
pub fn tempo_band(genre: &str) -> Option<(f32, f32)> {
    genre.split(['/', ',', ';', '|']).find_map(band_of_one)
}

/// [`tempo_band`] for a name already known to hold a single genre.
fn band_of_one(genre: &str) -> Option<(f32, f32)> {
    let g = normalise(genre);
    if g.is_empty() {
        return None;
    }
    let listed = |name: &str| {
        TEMPO_BANDS
            .iter()
            .find(|(key, _, _)| *key == name)
            .map(|(_, lo, hi)| (*lo, *hi))
    };
    if let Some(band) = listed(&g) {
        return Some(band);
    }
    // A genre this table does not list may still be a child of one it does.
    // The graph is keyed on a plain lowercase, so look it up that way and
    // normalise the neighbours on the way back out.
    graph()
        .get(&genre.trim().to_lowercase())
        .and_then(|neighbours| neighbours.iter().find_map(|n| listed(&normalise(n))))
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
/// Genre names this crate recognises beyond the taxonomy and the tempo table.
///
/// The taxonomy exists to measure *distance* between genres and the tempo table
/// to pick an octave, so both are deliberately small — they only list what those
/// two jobs need. Recognising a genre is a third job, and a wider one: a
/// community tag source offers "brostep", "neo-psychedelia" and "trip hop"
/// alongside "seen live", "female singer" and "icelandic", and something has to
/// tell those apart.
///
/// An allowlist rather than a blocklist of non-genres. The noise in a tag cloud
/// is open-ended — nationalities, decades, moods, instruments, "albums I own" —
/// and a blocklist would be permanently one surprise behind. The cost is that an
/// artist whose only tags are unlisted falls through to the next source rather
/// than getting a granular answer, which is the safe direction to fail: the app
/// keeps the coarse genre it already had instead of filing a record under
/// "swedish".
///
/// **Every entry must be in [`normalise`] form**, like [`TEMPO_BANDS`] — lookups
/// are normalised before comparison, so an entry that is not would be dead
/// weight nothing could match. `known_genres_are_normalised` fails the run if
/// one slips in. Names the tempo table already carries are not repeated here;
/// [`is_known_genre`] consults both.
const KNOWN_GENRES: &[&str] = &[
    // Bass music beyond what the tempo table needs.
    "future garage",
    "grime",
    "big beat",
    "trap",
    "footwork",
    "juke",
    "breakcore",
    // Four to the floor.
    "electro house",
    "acid house",
    "disco house",
    "french house",
    "detroit techno",
    "hardstyle",
    "gabber",
    // Broader electronic.
    "electronic",
    "electronica",
    "idm",
    "dark ambient",
    "drone",
    "downtempo",
    "trip hop",
    "synth pop",
    "synthwave",
    "vaporwave",
    "chiptune",
    "industrial",
    "ebm",
    // Rock and its neighbours.
    "indie rock",
    "post rock",
    "psychedelic rock",
    "progressive rock",
    "punk",
    "post punk",
    "shoegaze",
    "grunge",
    "heavy metal",
    "black metal",
    "death metal",
    "doom metal",
    "math rock",
    "emo",
    "neo psychedelia",
    "art pop",
    "dream pop",
    "noise rock",
    // Song, soul and roots.
    "pop",
    "smooth soul",
    "r and b",
    "contemporary r and b",
    "motown",
    "blues",
    "gospel",
    "country",
    "folk",
    "folk rock",
    "singer songwriter",
    "americana",
    "bluegrass",
    // Hip hop.
    "boom bap",
    "conscious hip hop",
    "jazz rap",
    "lo fi hip hop",
    "instrumental hip hop",
    // Jazz and composed.
    "jazz",
    "smooth jazz",
    "jazz pop",
    "jazz fusion",
    "bebop",
    "free jazz",
    "classical",
    "modern classical",
    "contemporary classical",
    "minimalism",
    "baroque",
    "opera",
    "soundtrack",
    "score",
    // Elsewhere in the world.
    "dancehall",
    "ska",
    "afrobeat",
    "highlife",
    "bossa nova",
    "samba",
    "mpb",
    "tropicalia",
    "cumbia",
    "salsa",
    "flamenco",
    "fado",
    "chanson",
    "klezmer",
    "qawwali",
];

/// Whether this crate recognises `name` as the name of a genre.
///
/// Normalised on both sides, so `"Drum & Bass"`, `"drum and bass"` and
/// `"DRUM-N-BASS"` are one answer. Consults the tempo table and the taxonomy as
/// well as [`KNOWN_GENRES`]: a name worth timing or measuring distance with is
/// automatically a name worth recognising, and repeating those here would be two
/// lists to keep in step.
pub fn is_known_genre(name: &str) -> bool {
    if is_unknown_genre(name) {
        return false;
    }
    let n = normalise(name);
    if n.is_empty() {
        return false;
    }
    KNOWN_GENRES.contains(&n.as_str())
        || TEMPO_BANDS.iter().any(|(key, _, _)| *key == n)
        // The graph is keyed on a plain lowercase rather than the normalised
        // form — see `normalise`'s note about not changing what the taxonomy
        // considers equal — so it is asked in its own terms.
        || graph().contains_key(&name.trim().to_lowercase())
}

/// Choose one genre from a tag cloud, or nothing.
///
/// Community tags are a popularity contest with no schema: they arrive weighted,
/// and mixed in with things that are not genres at all. This takes the
/// most-agreed-upon tag that this crate recognises as a genre and ignores the
/// rest.
///
/// Most-agreed rather than most-specific. "Delta Heavy" is tagged `drum and
/// bass` 12, `jungle` 2, `deep house` 1 — the long tail of a tag cloud is where
/// the mistakes live, and a rule that reached for the narrowest name would file
/// a drum & bass act under deep house on one person's say-so. Ties break on the
/// name so the answer does not depend on iteration order.
///
/// `tags` is `(name, count)` in any order. Returns the tag's own spelling, not
/// the lowercased form, because it is going on a screen.
pub fn pick_genre_tag<'a>(tags: &[(&'a str, u32)]) -> Option<&'a str> {
    tags.iter()
        .filter(|(name, count)| *count > 0 && is_known_genre(name))
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(name, _)| *name)
}

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

    /// A key that is not already normalised can never be matched, because the
    /// lookup normalises first. This is the test that catches "drum & bass"
    /// being added back to the table and silently never firing.
    /// Same rule as [`TEMPO_BANDS`], same reason: a lookup normalises before it
    /// compares, so an un-normalised entry is one nothing can ever match.
    #[test]
    fn known_genres_are_normalised() {
        for key in KNOWN_GENRES {
            assert_eq!(&normalise(key), key, "{key} is not in normalised form");
        }
        // And not repeated from the tempo table, which `is_known_genre` also
        // consults — two lists saying the same thing drift apart.
        for key in KNOWN_GENRES {
            assert!(
                !TEMPO_BANDS.iter().any(|(k, _, _)| k == key),
                "{key} is already in TEMPO_BANDS"
            );
        }
    }

    #[test]
    fn keys_are_already_normalised() {
        for (key, _, _) in TEMPO_BANDS {
            assert_eq!(&normalise(key), key, "{key} is not in normalised form");
        }
    }

    /// The reason AUD-26's correction never ran: the lookup was exact string
    /// equality, and no tagger writes "drum & bass".
    #[test]
    fn every_spelling_of_drum_and_bass_finds_the_band() {
        for spelling in [
            "DnB",
            "dnb",
            "D&B",
            "d & b",
            "D.N.B.",
            "Drum n Bass",
            "drum'n'bass",
            "Drum-n-Bass",
            "Drum   N   Bass",
            "Drum & Bass",
            "Drum and Bass",
            "DrumAndBass",
            "Liquid Funk",
            "Jump Up",
            "Neurofunk",
            "Breakbeat",
        ] {
            assert_eq!(
                tempo_band(spelling),
                Some((160.0, 185.0)),
                "{spelling} missed the band"
            );
            assert_eq!(
                octave_correct(87.0, spelling),
                Some(174.0),
                "{spelling} did not correct 87"
            );
        }
    }

    /// Punctuation and case are noise; the words are the name.
    #[test]
    fn punctuation_and_spacing_are_not_part_of_a_name() {
        assert_eq!(normalise("  Drum'n'Bass  "), "drum n bass");
        assert_eq!(normalise("D&B"), "d and b");
        assert_eq!(normalise("Hip-Hop"), "hip hop");
        assert_eq!(normalise("///"), "");
        assert_eq!(tempo_band("HIP-HOP"), Some((70.0, 110.0)));
        assert_eq!(tempo_band("Tech-House"), Some((118.0, 132.0)));
    }

    /// Real tag fields hold several genres at once, and step one of AUD-24
    /// makes the service's response one too.
    #[test]
    fn a_multi_genre_tag_is_read_a_segment_at_a_time() {
        assert_eq!(tempo_band("Drum & Bass / Neurofunk"), Some((160.0, 185.0)));
        assert_eq!(tempo_band("Electronic, Drum & Bass"), Some((160.0, 185.0)));
        assert_eq!(tempo_band("Electronic; Dance"), None);
        // First segment with a band wins, and says so.
        assert_eq!(tempo_band("Hip Hop / Drum & Bass"), Some((70.0, 110.0)));
    }

    /// Nu-skool breaks sits at 130–140 and is nowhere near the band this table
    /// files "breakbeat" under. That is safe rather than lucky: no octave of
    /// 132 lands inside 160–185 either, so the correction declines.
    #[test]
    fn a_breakbeat_record_at_its_own_tempo_is_left_alone() {
        assert_eq!(octave_correct(132.0, "Breakbeat"), None);
        assert_eq!(octave_correct(174.0, "Breakbeat"), None);
    }

    /// The alias table must not swallow the coarse label the service returns.
    /// "Electronic" covers everything from Nils Frahm to Eptic (AUD-24), and a
    /// band for it would be a guess dressed as knowledge.
    #[test]
    fn the_coarse_service_label_still_has_no_band() {
        assert_eq!(tempo_band("Electronic"), None);
        assert_eq!(tempo_band("Dance"), None);
        assert_eq!(octave_correct(87.0, "Electronic"), None);
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

    /// Real tag clouds and real counts, captured from MusicBrainz 2026-08-28,
    /// against the genre Deezer returns for the same artist.
    ///
    /// The case for a second source is the right-hand column: Delta Heavy,
    /// Noisia and Zero T are drum & bass acts and Deezer calls all three
    /// "Dance", because "Dance" is one of the ten words it has for this whole
    /// library.
    #[test]
    fn a_real_tag_cloud_yields_the_genre_the_artist_actually_plays() {
        for (artist, tags, want) in [
            (
                "Delta Heavy",
                &[
                    ("drum and bass", 7u32),
                    ("electronic", 1),
                    ("jungle", 1),
                    ("deep house", 1),
                ][..],
                "drum and bass",
            ),
            (
                "Noisia",
                &[
                    ("electronic", 8),
                    ("drum and bass", 8),
                    ("dubstep", 5),
                    ("neurofunk", 2),
                    ("electro house", 2),
                    ("edm", 2),
                    ("halftime", 1),
                    ("dance and electronica", 0),
                ][..],
                // A genuine 8–8 tie between a precise name and a vague one, and
                // the tie-break decides it: alphabetically first wins, which is
                // "drum and bass". Arbitrary as a rule, but it has to be
                // *stable* — the alternative is a genre tile that changes its
                // mind between two reads of the same data.
                "drum and bass",
            ),
            (
                "Sade",
                &[
                    ("smooth jazz", 7),
                    ("soul", 4),
                    ("jazz pop", 4),
                    // Tied with two real genres and not a genre at all. The
                    // filter has to drop it before the count is consulted.
                    ("female singer", 4),
                    ("contemporary r&b", 4),
                    ("smooth soul", 3),
                    ("sophisti-pop", 3),
                ][..],
                "smooth jazz",
            ),
            (
                "Nils Frahm",
                &[
                    // "instrumental" ties for the lead and is not a genre.
                    ("instrumental", 3),
                    ("modern classical", 3),
                    ("electronic", 1),
                    ("ambient", 1),
                    ("minimalism", 1),
                ][..],
                "modern classical",
            ),
            (
                "Tame Impala",
                &[
                    ("psychedelic rock", 13),
                    ("neo-psychedelia", 12),
                    ("psychedelic pop", 8),
                    ("alternative rock", 5),
                    ("australia", 2),
                ][..],
                "psychedelic rock",
            ),
        ] {
            assert_eq!(
                pick_genre_tag(tags),
                Some(want),
                "{artist}: picked the wrong tag out of {tags:?}"
            );
        }
    }

    /// Nothing recognisable means nothing, not a guess.
    ///
    /// Falling through leaves whatever coarse genre the app already had, which
    /// is a far better failure than filing a record under "swedish".
    #[test]
    fn a_cloud_with_no_genre_in_it_picks_nothing() {
        assert_eq!(
            pick_genre_tag(&[("seen live", 40), ("icelandic", 22), ("female singer", 18)]),
            None
        );
        assert_eq!(pick_genre_tag(&[]), None);
        // A zero count is an absent vote, not a quiet one.
        assert_eq!(pick_genre_tag(&[("techno", 0)]), None);
    }

    /// The spelling that goes on the screen is the one the source used.
    #[test]
    fn the_tags_own_spelling_survives() {
        assert_eq!(
            pick_genre_tag(&[("Drum & Bass", 5)]),
            Some("Drum & Bass"),
            "a recognised genre must not be lowercased on the way out"
        );
    }

    #[test]
    fn recognition_ignores_case_and_reaches_the_taxonomy() {
        assert!(is_known_genre("DRUM AND BASS"));
        assert!(is_known_genre("  Tech House  "));
        // In the taxonomy but not in the extra list — recognised via the graph.
        assert!(is_known_genre("Liquid DNB"));
        // In the tempo table.
        assert!(is_known_genre("hip hop"));
        assert!(!is_known_genre("seen live"));
        assert!(!is_known_genre(""));
        assert!(!is_known_genre("unknown"));
    }
}

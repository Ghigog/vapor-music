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

/// Cost returned when two genres are both known and share no ancestry.
///
/// Also the ceiling on any distance: with the families joined under one root
/// (see [`TAXONOMY`]) a path exists between almost any two nodes, and an
/// uncapped walk from qawwali to brostep would return a number larger than
/// "unrelated" and blow past the scale every weight in `track.rs` is tuned to.
pub const UNRELATED_COST: f32 = 5.0;

/// Cost returned when either side has no genre at all.
///
/// **Deliberately below [`UNRELATED_COST`], and this is a change in meaning.**
/// Absence used to score the same as a known clash, which was harmless while it
/// was universal — every track in this library had an empty genre, so the term
/// was a constant that cancelled out of every comparison the planner made.
///
/// It stops being harmless the moment genres start arriving. A part-identified
/// library has some tracks with a real genre and some without, and if absence
/// scores as the maximum then every un-identified track is as expensive as the
/// worst possible pairing — so the planner would work its way through the
/// identified half and quietly avoid the rest. That is a bias introduced by
/// *fixing* the genre source, which is the wrong direction to fail.
///
/// Halfway is the honest position: no evidence should neither recommend a track
/// nor rule it out. Reasoned rather than measured — the library it would be
/// measured against has not been identified yet — and this note is here so the
/// number is not later mistaken for one that came off a distribution.
pub const UNKNOWN_COST: f32 = 2.5;

/// No evidence must never cost more than a known clash.
///
/// Checked at compile time rather than in a test: it is a relationship between
/// two constants, so there is no run in which it could hold and a later one in
/// which it could not. If someone raises [`UNKNOWN_COST`] past
/// [`UNRELATED_COST`], the build stops — which is the moment the planner would
/// otherwise have quietly gone back to avoiding every un-identified track.
const _: () = assert!(UNKNOWN_COST < UNRELATED_COST);

/// How many edges apart two genres can be and still count as one family.
///
/// Two, which is a node and its parent's other children: `Neurofunk` and
/// `Jungle` are both drum & bass, `Tech House` and `Deep House` are both house.
/// Three would reach across [`TAXONOMY`]'s next tier up — Tech House to Techno —
/// and those are a gear change a set should be allowed to notice.
pub const FAMILY_DISTANCE: f32 = 2.0;

/// The taxonomy. Sole copy since the Godot tree was removed.
///
/// An **undirected graph**, not a tree, despite the parent-and-children shape:
/// [`graph`] links every pair symmetrically, so an entry can also state a
/// crossing edge between two families. `Ambient`/`Modern Classical` is one, and
/// it is there because Harold Budd is honestly both — a taxonomy that filed him
/// only under one would put the other half of his catalogue five edges away.
///
/// # Why it grew
///
/// It used to hold nineteen nodes: house, techno, drum & bass and rock. That was
/// enough while every track's genre was empty, because nothing ever reached a
/// lookup. Now that MusicBrainz supplies names for a real library, the names it
/// supplies — `ambient`, `modern classical`, `brostep`, `smooth jazz`,
/// `psychedelic rock` — were none of them in here, so [`genre_distance`] hit
/// [`UNRELATED_COST`] for nearly every pair and the genre term went back to
/// being a constant. A cost that is the same for everything cannot order
/// anything, which is the exact failure this replaced in a new disguise.
///
/// # What the arrangement has to preserve
///
/// The Godot distances are pinned by `distances_match_the_godot_values`, and
/// the tiers here keep them: `Tech House` to `Techno` is still three edges, via
/// `House` and `Club Music`. Adding `Electronic` above `Club Music` lengthens
/// no existing shortest path, because nothing existing routed through the top.
const TAXONOMY: &[(&str, &[&str])] = &[
    // ---- Electronic ----------------------------------------------------
    (
        "Electronic",
        &[
            "Club Music",
            "Bass Music",
            "Ambient",
            "Downtempo",
            "IDM",
            "Industrial",
            "Electronica",
        ],
    ),
    ("Club Music", &["House", "Techno", "Trance", "Disco"]),
    (
        "House",
        &[
            "Tech House",
            "Deep House",
            "Progressive House",
            "Minimal House",
            "Electro House",
            "Acid House",
            "French House",
            "Disco House",
        ],
    ),
    (
        "Techno",
        &["Acid Techno", "Minimal Techno", "Detroit Techno"],
    ),
    ("Trance", &["Psytrance", "Hardstyle", "Gabber"]),
    (
        "Bass Music",
        &["Drum & Bass", "Dubstep", "Garage", "Breakbeat", "Trap"],
    ),
    (
        "Drum & Bass",
        &[
            "Liquid DNB",
            "Neurofunk",
            "Jungle",
            "Drum and Bass",
            "DnB",
            "Liquid Funk",
        ],
    ),
    ("Dubstep", &["Brostep", "Riddim"]),
    ("Garage", &["UK Garage", "2 Step", "Future Garage", "Grime"]),
    ("Breakbeat", &["Big Beat", "Breakcore"]),
    ("Trap", &["Footwork", "Juke"]),
    ("Ambient", &["Dark Ambient", "Drone", "Modern Classical"]),
    ("Downtempo", &["Trip Hop", "Chillout"]),
    ("IDM", &["Glitch"]),
    ("Industrial", &["EBM"]),
    (
        "Electronica",
        &["Synth-pop", "Synthwave", "Vaporwave", "Chiptune"],
    ),
    // ---- Rock ----------------------------------------------------------
    (
        "Rock",
        &[
            "Alternative Rock",
            "Classic Rock",
            "Hard Rock",
            "Indie Rock",
            "Psychedelic Rock",
            "Post-rock",
            "Punk",
            "Metal",
            "Math Rock",
            "Noise Rock",
        ],
    ),
    (
        "Alternative Rock",
        &["Grunge", "Shoegaze", "Emo", "Dream Pop", "Neo-psychedelia"],
    ),
    ("Psychedelic Rock", &["Neo-psychedelia", "Space Rock"]),
    ("Punk", &["Post-punk", "Hardcore Punk"]),
    (
        "Metal",
        &["Heavy Metal", "Black Metal", "Death Metal", "Doom Metal"],
    ),
    // Rock and electronica meet at the synthesiser, which is where post-punk
    // actually went. Without this edge the whole rock family is unrelated to
    // everything, and a set that drifts from shoegaze into ambient — an
    // ordinary and good move — reads as a maximum-cost jump.
    ("Post-punk", &["Industrial"]),
    ("Dream Pop", &["Ambient"]),
    // ---- Song, soul and roots -------------------------------------------
    (
        "Pop",
        &["Art Pop", "Dream Pop", "Synth-pop", "Sophisti-pop"],
    ),
    ("Soul", &["Smooth Soul", "Motown", "R&B", "Gospel", "Funk"]),
    ("R&B", &["Contemporary R&B", "Quiet Storm"]),
    ("Funk", &["Disco", "Afrobeat"]),
    (
        "Folk",
        &["Folk Rock", "Singer-songwriter", "Americana", "Bluegrass"],
    ),
    ("Folk Rock", &["Rock"]),
    ("Blues", &["Rock", "Soul", "Gospel"]),
    ("Country", &["Americana", "Bluegrass", "Folk"]),
    // ---- Hip hop ---------------------------------------------------------
    (
        "Hip Hop",
        &[
            "Rap",
            "Boom Bap",
            "Conscious Hip Hop",
            "Jazz Rap",
            "Lo-fi Hip Hop",
            "Instrumental Hip Hop",
            "Trap",
        ],
    ),
    ("Jazz Rap", &["Jazz"]),
    ("Lo-fi Hip Hop", &["Downtempo"]),
    ("Hip Hop", &["Funk"]),
    // ---- Jazz and composed ------------------------------------------------
    (
        "Jazz",
        &[
            "Smooth Jazz",
            "Jazz Pop",
            "Jazz Fusion",
            "Bebop",
            "Free Jazz",
        ],
    ),
    ("Smooth Jazz", &["Smooth Soul"]),
    ("Jazz Pop", &["Pop"]),
    (
        "Classical",
        &[
            "Modern Classical",
            "Contemporary Classical",
            "Baroque",
            "Opera",
        ],
    ),
    ("Modern Classical", &["Minimalism", "Soundtrack"]),
    ("Soundtrack", &["Score"]),
    // ---- Elsewhere in the world --------------------------------------------
    ("Reggae", &["Dub", "Dancehall", "Ska", "Roots Reggae"]),
    ("Dub", &["Bass Music"]),
    ("Brazilian", &["Bossa Nova", "Samba", "MPB", "Tropicalia"]),
    ("Bossa Nova", &["Jazz"]),
    ("Latin", &["Cumbia", "Salsa", "Brazilian"]),
    (
        "Folk",
        &["Flamenco", "Fado", "Chanson", "Klezmer", "Qawwali"],
    ),
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

/// Split a genre field into the genres it names.
///
/// One place, because three parts of this crate were already splitting the same
/// field with their own separator lists and a fourth was about to. `tempo_band`
/// has read `/`-separated genres a segment at a time since AUD-24, the index
/// now stores a list, and the two have to agree on what a separator is or a
/// track gains a genre on one screen and loses it on another.
///
/// Empty segments are dropped and whitespace is trimmed, so `"Electronic / "`
/// is one genre and `""` is none. Order is kept: the first genre named is the
/// one a caller that can only show one should show.
pub fn split_genres(field: &str) -> Vec<String> {
    field
        .split(['/', ',', ';', '|'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// [`split_genres`], with the placeholders dropped.
///
/// One call rather than two because the order is load-bearing: `"N/A"` is a
/// placeholder whose slash is a separator, so a caller that split first and
/// filtered afterwards is left holding `"N"` and `"A"` — two genres, neither of
/// them recognisable as a placeholder any more, out of a field that named none.
/// The whole field is tested first, then each segment, because `"House / N/A"`
/// needs both passes.
///
/// This is what a raw tag field should go through on its way to becoming a
/// list. [`split_genres`] stays the plain splitter for callers that already
/// know their input is real.
pub fn split_real_genres(field: &str) -> Vec<String> {
    if is_unknown_genre(field) {
        return Vec::new();
    }
    let segments = split_genres(field);
    let mut kept = Vec::with_capacity(segments.len());
    let mut i = 0;
    while i < segments.len() {
        // "N/A" is the one placeholder whose own text contains a separator, so
        // `split_genres` has already cut it in half before anything can
        // recognise it. Two adjacent segments that rejoin into a placeholder
        // were one, and both go — testing the segments singly leaves "N" and
        // "A", which are not placeholders to anything downstream.
        if i + 1 < segments.len() && is_unknown_genre(&format!("{}/{}", segments[i], segments[i + 1]))
        {
            i += 2;
            continue;
        }
        if !is_unknown_genre(&segments[i]) {
            kept.push(segments[i].clone());
        }
        i += 1;
    }
    kept
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
    split_genres(genre).iter().find_map(|g| band_of_one(g))
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
    // A genre this table does not list may still be a child of one it does —
    // *child*, which is why this reads [`parents`] and not [`graph`]. The map is
    // keyed on a plain lowercase, so look it up that way and normalise the
    // names on the way back out.
    parents()
        .get(&genre.trim().to_lowercase())
        .and_then(|above| above.iter().find_map(|n| listed(&normalise(n))))
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
/// Which nodes a genre sits *under*, keyed by the child.
///
/// [`graph`] deliberately forgets direction — distance does not care which way
/// an edge points — but one caller does care. [`band_of_one`] infers a tempo
/// band from a relative, and that inference is only sound *upward*: `Liquid
/// DNB` may take drum & bass's 160–185 because it is a kind of drum & bass.
/// Reading the same edge downward is how `Electronic` came to be assigned
/// ambient's 50–120: an umbrella label borrowing a band from one of the many
/// different things underneath it, which is a guess dressed as knowledge and is
/// exactly what `the_coarse_service_label_still_has_no_band` exists to catch.
fn parents() -> &'static HashMap<String, Vec<String>> {
    static PARENTS: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    PARENTS.get_or_init(|| {
        let mut p: HashMap<String, Vec<String>> = HashMap::new();
        for (parent, children) in TAXONOMY {
            for child in *children {
                p.entry(child.to_lowercase())
                    .or_default()
                    .push(parent.to_lowercase());
            }
        }
        p
    })
}

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
    // Longest match, then alphabetical — **not** the first one iteration finds.
    //
    // This was `g.keys().find(...)` over a `HashMap`, whose order is arbitrary
    // and varies between runs. With nineteen nodes a tag rarely matched two of
    // them and the bug stayed hidden; with the taxonomy above it is routine —
    // "jazz rap" contains both `jazz` and `rap`, "dub techno" contains both
    // `dub` and `techno` — and an arbitrary winner means a genre distance, and
    // therefore a built set, that can differ between two runs over identical
    // data. Longest is also the better answer on the merits: it is the most
    // specific node the name contains.
    g.keys()
        .filter(|k| k.contains(&key) || key.contains(k.as_str()))
        .max_by(|a, b| a.len().cmp(&b.len()).then_with(|| b.cmp(a)))
}

/// Edge distance between two genres in the taxonomy.
///
/// Returns [`UNRELATED_COST`] when either is unknown or when no path exists.
pub fn genre_distance(a: &str, b: &str) -> f32 {
    let (ca, cb) = (a.trim().to_lowercase(), b.trim().to_lowercase());

    // No genre is not the same as a genre that does not fit — see
    // [`UNKNOWN_COST`]. Asked through `is_unknown_genre` so every placeholder a
    // real tagger writes ("Other", "Unknown genre") lands here too, rather than
    // only the one spelling this used to check.
    if is_unknown_genre(&ca) || is_unknown_genre(&cb) {
        return UNKNOWN_COST;
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
            // Capped: the families are joined now, so a path exists between
            // almost any two nodes and the far ones run past the scale the
            // weights in `track.rs` are tuned against.
            return (dist as f32).min(UNRELATED_COST);
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
    if ca == cb || ca.contains(&cb) || cb.contains(&ca) {
        return true;
    }
    // And the taxonomy, which is what makes this useful on granular names.
    //
    // Substrings alone answered "different" for `Neurofunk` and `Liquid DNB` —
    // two kinds of drum & bass that share not one character. Every caller reads
    // a false here as a deliberate gear change: `exit_between` forces a Switch,
    // and `choose_transition` hides the mix behind an echo. Doing that between
    // two drum & bass records is precisely the over-reaction that arrives with
    // better genre data, and it would have looked like the *new* genres being
    // wrong rather than this test of them.
    genre_distance(&ca, &cb) <= FAMILY_DISTANCE
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

    /// The case the splitter alone gets wrong: a placeholder with a separator
    /// in it. "N/A" must name no genres rather than two.
    #[test]
    fn a_placeholder_with_a_slash_in_it_names_no_genres() {
        assert_eq!(split_genres("N/A"), vec!["N", "A"], "the plain splitter");
        assert!(split_real_genres("N/A").is_empty());
        assert!(split_real_genres("unknown").is_empty());
        assert!(split_real_genres("").is_empty());
        assert_eq!(split_real_genres("House / N/A"), vec!["House"]);
        assert_eq!(
            split_real_genres("Liquid Funk / Jazz"),
            vec!["Liquid Funk", "Jazz"],
            "real genres are untouched"
        );
    }

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

    /// Three different states, and they must not all cost the same.
    ///
    /// Absence is not a clash. While every track's genre was empty this made no
    /// difference — the term was a constant — but a part-identified library is
    /// the case that matters, and there an un-identified track scoring the
    /// maximum means the planner routes around everything it has not looked up
    /// yet. See [`UNKNOWN_COST`].
    #[test]
    fn an_absent_genre_costs_less_than_a_clashing_one() {
        assert_eq!(genre_distance("", "Techno"), UNKNOWN_COST);
        assert_eq!(genre_distance("Unknown", "Techno"), UNKNOWN_COST);
        // Every placeholder a real tagger writes, not just the one spelling.
        assert_eq!(genre_distance("Unknown genre", "Techno"), UNKNOWN_COST);
        assert_eq!(genre_distance("Other", "Techno"), UNKNOWN_COST);

        // A real genre this crate cannot place is a different thing again: we
        // have a name and it is not one we know, which is evidence of distance
        // rather than an absence of evidence.
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

    // -----------------------------------------------------------------------
    // What a set needs the taxonomy for
    // -----------------------------------------------------------------------

    /// The case this was all for: an ambient record read at a dancefloor tempo.
    ///
    /// A Harold Budd piece has no percussion to track, so the estimator latches
    /// onto something and reports 178 — a perfectly ordinary failure, and one
    /// no amount of signal processing fixes, because 178 *is* present in the
    /// signal. The genre is the only thing that knows nobody counts an ambient
    /// record at 178, and with the ambient band at 50–120 the octave below is
    /// the one a listener would feel.
    ///
    /// Without this the planner sees 178 and offers Skrillex as a tempo match.
    #[test]
    fn an_ambient_record_read_at_a_dancefloor_tempo_is_brought_back_down() {
        assert_eq!(octave_correct(178.0, "Ambient"), Some(89.0));
        // The same for the names MusicBrainz actually returns for that shelf.
        assert_eq!(octave_correct(178.0, "Dark Ambient"), Some(89.0));
        assert_eq!(octave_correct(178.0, "Modern Classical"), Some(89.0));

        // And the record it was being mixed into is left exactly where it is:
        // 140 is where brostep lives, so there is nothing to correct.
        assert_eq!(octave_correct(140.0, "Brostep"), None);
    }

    /// And once the tempi are honest, the genres still have to keep them apart.
    ///
    /// Correcting 178 to 89 is not on its own enough — a set can still put an
    /// ambient piece next to a brostep one on a key match. The genre term is
    /// what makes that expensive, and it can only do so if both names are in
    /// the taxonomy. Before it grew, neither was, and both pairs below scored
    /// an identical `UNRELATED_COST`.
    #[test]
    fn ambient_and_brostep_are_further_apart_than_ambient_and_its_neighbours() {
        let far = genre_distance("Ambient", "Brostep");
        let near = genre_distance("Ambient", "Modern Classical");
        assert!(
            near < far,
            "ambient->modern classical ({near}) should beat ambient->brostep ({far})"
        );
        // Not merely ordered: the near pair has to be close enough that a set
        // is happy to make the move, and the far pair far enough that it is a
        // deliberate one.
        assert!(near <= FAMILY_DISTANCE, "{near}");
        assert!(far >= 4.0, "{far}");
    }

    /// The vocabulary the genre source actually returns is all placeable.
    ///
    /// The failure this guards is silent and total: a name the taxonomy does not
    /// hold scores `UNRELATED_COST` against everything, so if the common names
    /// were missing then genre would be a constant again and would order
    /// nothing — the same dead term as before, wearing better data.
    #[test]
    fn the_genres_this_library_actually_carries_are_all_in_the_taxonomy() {
        for name in [
            // Measured off the owner's library via MusicBrainz.
            "drum and bass",
            "neurofunk",
            "brostep",
            "dubstep",
            "modern classical",
            "ambient",
            "smooth jazz",
            "soul",
            "psychedelic rock",
            "neo-psychedelia",
            "trip hop",
            "hip hop",
            "electronic",
            "jungle",
            "deep house",
        ] {
            assert!(
                find_node(name).is_some(),
                "{name} is not in the taxonomy, so it would score UNRELATED against everything"
            );
        }
    }

    /// Two kinds of one genre are not a gear change.
    ///
    /// `is_similar_genre` decides whether a transition is a Switch and whether
    /// the mixer hides it behind an echo. On pure substrings `Neurofunk` and
    /// `Liquid DNB` share no characters and so read as different music — which
    /// would have made better genre data look *worse*, by turning every move
    /// inside drum & bass into a deliberate gear change.
    #[test]
    fn two_kinds_of_one_genre_are_not_a_gear_change() {
        assert!(is_similar_genre("Neurofunk", "Liquid DNB"));
        assert!(is_similar_genre("Tech House", "Deep House"));
        assert!(is_similar_genre("Brostep", "Riddim"));

        // But a real change of music still is one.
        assert!(!is_similar_genre("Ambient", "Brostep"));
        assert!(!is_similar_genre("Smooth Jazz", "Neurofunk"));
    }

    /// The same two genres must score the same on every run.
    ///
    /// `find_node`'s substring fallback used to be `HashMap::keys().find(..)`,
    /// whose order is arbitrary — so a name containing two nodes ("jazz rap"
    /// holds both `jazz` and `rap`) could resolve differently between runs, and
    /// a set built twice over identical data could differ. Longest match wins,
    /// which is both stable and the more specific answer.
    #[test]
    fn resolution_is_stable_and_prefers_the_most_specific_node() {
        for name in ["jazz rap", "dub techno", "psychedelic pop", "acid house"] {
            let first = find_node(name).cloned();
            for _ in 0..50 {
                assert_eq!(find_node(name).cloned(), first, "{name} resolved two ways");
            }
        }
        // The specific node, not whichever substring happened to come first.
        assert_eq!(find_node("jazz rap").map(String::as_str), Some("jazz rap"));
        assert_eq!(find_node("dub techno").map(String::as_str), Some("techno"));
    }
}

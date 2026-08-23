//! Lyrics and artwork looked up from public services.
//!
//! Port of the network half of `metadata_service.gd`: LRCLIB for lyrics,
//! Deezer for an artist portrait, album art and a genre when the file carries
//! none.
//!
//! ## This is the one part of the app that talks to a stranger
//!
//! Everything else Vapor knows about a track it works out on the device from
//! the audio itself — that is what the sovereignty green in the palette
//! *means*, and Liner Notes says so in as many words. A lookup sends the
//! artist and title of what someone is listening to to a server they have no
//! relationship with. The Godot build did it unconditionally and said nothing.
//!
//! So: off unless asked ([`vapor_library::Settings::metadata_lookup_enabled`]),
//! and everything it returns is labelled as looked up rather than mixed in
//! with what the app measured. The lookup itself is a faithful port; the
//! consent around it is new.
//!
//! ## Parsing is separate from the transport
//!
//! Same reason `webdav.rs` split its walk from its client: a response shape is
//! the part that breaks, and a test should be able to drive every branch of it
//! from a canned string with no network. [`parse_lrc`], [`lyrics_of`],
//! [`image_url_of`] and [`genre_of`] are pure and take `&str`.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use ts_rs::TS;

/// How long a lookup may take before it is abandoned.
///
/// Short on purpose. This is decoration on a screen that is already useful
/// without it, and a person waiting on a spinner for artwork is worse served
/// than one who never sees it.
const TIMEOUT: Duration = Duration::from_secs(8);

/// Sent so the services can identify the client, as the original did.
///
/// Both services this file talks to ask for exactly this shape, and one of
/// them requires it (AUD-18, read 2026-08-23):
///
/// * **LRCLIB** — "we require you to identify your client in requests. Set the
///   `User-Agent` header with your application's name, version, and a link to
///   its homepage or project page **or an email address**."
///   <https://lrclib.net/docs>
/// * **MusicBrainz**, should the lookups ever move there — "Application
///   name/&lt;version&gt; ( contact-url )", and a request without one is
///   throttled as "anonymous".
///   <https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting>
///
/// A project URL satisfies both, so this is already compliant and no address
/// is invented here. **If Dylan wants a mailbox on it instead**, this is the
/// one line to change; nothing else reads it.
const USER_AGENT: &str = concat!(
    "VaporMusic/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/Ghigog/vapor-music)"
);

/// The smallest gap between two requests to the same service.
///
/// **Deezer** documents no quota at all — the terms of use are silent and the
/// developer FAQ says only "there is no limitation on data in the API, but
/// there is a query quota". The number every client in the wild converges on
/// is 50 requests per 5 seconds per IP, and exceeding it returns error code 4,
/// "Quota limit exceeded". 200 ms is 5 requests a second: half of a limit
/// nobody will confirm, which is the right side to be wrong on when the
/// consequence is a blocked address.
///
/// Why it matters here rather than at the call site: `identify_library` runs
/// two lookups per track on scoped threads, and each is two round trips deep
/// — measured 2026-08-23 at ~0.29 s per Deezer request from this machine,
/// which is about **7 requests a second sustained across a 563-track pass**,
/// with no gap between them and nothing watching for a refusal.
const DEEZER_GAP: Duration = Duration::from_millis(200);

/// LRCLIB asks for this in as many words, and names this exact use:
///
/// > "add a short delay between requests (200–500 ms), especially for batch
/// > operations like scanning a full music library."
///
/// The middle of the band they gave. <https://lrclib.net/docs>
const LRCLIB_GAP: Duration = Duration::from_millis(300);

/// Artwork comes off a static CDN rather than the API, so it gets its own
/// clock — but it is still Deezer's infrastructure and still a whole library's
/// worth of files, so it is paced the same way.
const ARTWORK_GAP: Duration = Duration::from_millis(200);

/// How many times one request is sent before it is given up on.
///
/// Four attempts with a doubling wait spends at most 3.5 s of backoff, which
/// is inside a quota window on any of the numbers above.
const ATTEMPTS: u32 = 4;

/// The wait before a second attempt, doubled for each one after.
const BACKOFF: Duration = Duration::from_millis(500);

/// A refusal that names its own wait is still capped: a service asking for ten
/// minutes gets the request abandoned, not a parked thread.
const LONGEST_WAIT: Duration = Duration::from_secs(30);

/// One line of time-aligned lyrics.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LyricLine {
    /// Seconds from the start of the track.
    pub time: f32,
    pub text: String,
}

/// The words to a track, however they arrived.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Lyrics {
    /// Whether `lines` carry usable timings.
    pub synced: bool,
    /// Time-aligned lines, empty when only plain text was available.
    pub lines: Vec<LyricLine>,
    /// The unaligned text, empty when a synced version was available.
    pub plain: String,
}

/// What a lookup found for one track. Persisted, so a track is looked up once.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Looked {
    #[serde(default)]
    pub lyrics: Option<Lyrics>,
    /// Artist portrait, as a URL on the service.
    #[serde(default)]
    pub artist_image: String,
    /// Album art, as a URL on the service.
    #[serde(default)]
    pub album_art: String,
    #[serde(default)]
    pub genre: String,
    /// Deezer's own tempo for this recording. **Zero means they do not know**,
    /// which is most of the time — it is an absent value, not a default.
    ///
    /// Never used as a tempo. Its only job is to say which octave of the tempo
    /// measured here a listener would count; see
    /// `vapor_library::octave_from_reference`.
    #[serde(default)]
    pub deezer_bpm: f32,
    /// Their length in seconds, used to check the match is the same recording
    /// before anything of theirs is believed.
    #[serde(default)]
    pub deezer_duration: u32,
    /// Which Deezer release this track was matched to. 0 when unmatched.
    ///
    /// A pointer rather than a copy: the tracklist behind it can be fourteen
    /// titles, and storing that on all fourteen tracks would write the same
    /// list fourteen times. See [`Albums`].
    #[serde(default)]
    pub deezer_album_id: u64,
    /// Whether the **facts** pass has run for this track — genre, tempo,
    /// duration, from Deezer.
    ///
    /// Named `attempted` since the port, and it used to be read as "everything
    /// has been tried", which is how it came to deny the whole library two
    /// features. `identify_library` sets it after asking Deezer and never
    /// asking LRCLIB at all, and the background fetcher treated it as proof
    /// there was nothing left to get. Measured 2026-08-22: 534 of 534 cached
    /// entries had `attempted: true`, and **zero** had lyrics or album art.
    ///
    /// One flag per thing actually attempted, so a pass cannot close a door it
    /// never opened. See [`Looked::words_attempted`].
    #[serde(default)]
    pub attempted: bool,
    /// Whether **LRCLIB** has been asked for this track's words.
    ///
    /// This is the flag the old doc comment described: a track whose lyrics
    /// simply do not exist must not be asked for again on every visit to the
    /// screen, and "nothing found" and "not yet looked" are otherwise the same
    /// value.
    ///
    /// Defaults to false, which is what repairs the caches already written —
    /// every entry poisoned by the facts pass asks once more, gets its words
    /// and its sleeve, and settles.
    #[serde(default)]
    pub words_attempted: bool,
}

/// Everything looked up so far, keyed by href.
pub type Cache = std::collections::HashMap<String, Looked>;

// ---------------------------------------------------------------------------
// Parsing — pure, and where the tests live
// ---------------------------------------------------------------------------

/// Parse LRC text: `[MM:SS.CC] words`.
///
/// Port of `parse_lrc`. Two deliberate differences from the GDScript regex:
///
/// * The fractional part is optional and read at its own precision. The
///   original's `(\d+)` divided by 100 whatever it matched, so LRCLIB's
///   occasional three-digit milliseconds became *ten times* the offset they
///   should have — `[01:02.500]` landing five seconds late.
/// * A line may carry several timestamps, which is how LRC files write a
///   repeated chorus. The original kept only the first and dropped the rest,
///   so a chorus appeared once and then the words stopped moving.
pub fn parse_lrc(text: &str) -> Vec<LyricLine> {
    let mut lines: Vec<LyricLine> = Vec::new();

    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps: Vec<f32> = Vec::new();

        // Timestamps are a prefix run: take them until one does not parse.
        while let Some(close) = rest.strip_prefix('[').and_then(|r| r.find(']')) {
            let Some(at) = parse_timestamp(&rest[1..close + 1]) else {
                break;
            };
            stamps.push(at);
            rest = rest[close + 2..].trim_start();
        }

        if stamps.is_empty() {
            continue;
        }
        // An empty line is kept: LRCLIB uses it for an instrumental break, and
        // dropping it makes the previous line hang on screen through it.
        let text = rest.trim().to_string();
        for at in stamps {
            lines.push(LyricLine {
                time: at,
                text: text.clone(),
            });
        }
    }

    // A multi-timestamp line puts its repeats out of order; a player stepping
    // through these needs them monotonic.
    lines.sort_by(|a, b| a.time.total_cmp(&b.time));
    lines
}

/// `MM:SS`, `MM:SS.CC` or `MM:SS.mmm` as seconds.
fn parse_timestamp(s: &str) -> Option<f32> {
    let (minutes, rest) = s.split_once(':')?;
    let minutes: f32 = minutes.trim().parse().ok()?;

    let (seconds, fraction) = match rest.split_once(['.', ':']) {
        Some((s, f)) => (s, Some(f)),
        None => (rest, None),
    };
    let seconds: f32 = seconds.trim().parse().ok()?;

    // Read at the precision it was written: two digits are centiseconds,
    // three are milliseconds. Dividing everything by 100 is the bug this
    // replaces.
    let extra = match fraction {
        None => 0.0,
        Some(f) => {
            let digits = f.trim();
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let value: f32 = digits.parse().ok()?;
            value / 10f32.powi(digits.len() as i32)
        }
    };

    Some(minutes * 60.0 + seconds + extra)
}

/// Read an LRCLIB `/api/get` response.
///
/// Synced lyrics win when both are present, because a synced line can always
/// be shown as plain text and the reverse is not true.
pub fn lyrics_of(body: &str) -> Option<Lyrics> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;

    let synced = value
        .get("syncedLyrics")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !synced.trim().is_empty() {
        let lines = parse_lrc(synced);
        if !lines.is_empty() {
            return Some(Lyrics {
                synced: true,
                lines,
                plain: String::new(),
            });
        }
    }

    let plain = value
        .get("plainLyrics")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if plain.trim().is_empty() {
        return None;
    }
    Some(Lyrics {
        synced: false,
        lines: Vec::new(),
        plain: plain.to_string(),
    })
}

/// The best image URL in a Deezer search response.
///
/// `keys` are tried in order — the original's `picture_xl` … `picture_small`
/// and `cover_xl` … `cover_small` ladders, which is what makes a hit still
/// usable when the service has no large version of a sleeve.
pub fn image_url_of(body: &str, keys: &[&str]) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return String::new();
    };
    let Some(first) = value
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
    else {
        return String::new();
    };
    for key in keys {
        if let Some(url) = first.get(*key).and_then(|v| v.as_str()) {
            if !url.is_empty() {
                return url.to_string();
            }
        }
    }
    String::new()
}

/// Size ladders, in the order the original tried them.
pub const ARTIST_KEYS: &[&str] = &[
    "picture_xl",
    "picture_big",
    "picture_medium",
    "picture_small",
];
pub const ALBUM_KEYS: &[&str] = &["cover_xl", "cover_big", "cover_medium", "cover_small"];

/// What Deezer knows about one recording.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackFacts {
    /// Deezer's own tempo. **Zero means they do not know**, which is most of
    /// the time — it is a real field with a real absent value, not a default.
    pub bpm: f32,
    /// Length in seconds, used to check the match is the same recording.
    pub duration: u32,
}

/// Read a Deezer `/track/{id}` response.
pub fn track_facts_of(body: &str) -> Option<TrackFacts> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    // An error object is a valid document and not a track.
    if value.get("error").is_some() {
        return None;
    }
    Some(TrackFacts {
        bpm: value.get("bpm").and_then(|b| b.as_f64()).unwrap_or(0.0) as f32,
        duration: value.get("duration").and_then(|d| d.as_u64()).unwrap_or(0) as u32,
    })
}

/// The first hit of a track search, with the release it sits on.
///
/// The search response was being read for one field — the track id — and thrown
/// away, when it already names the album, its id, and the recording's length.
/// That is everything needed to say which record a loose file came off, without
/// a single extra request.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackHit {
    pub id: u64,
    pub title: String,
    /// Length in seconds, for the [`same_recording`] check.
    pub duration: u32,
    pub album_id: u64,
    pub album_title: String,
}

/// Read the first hit out of a Deezer `/search/track` response.
pub fn track_hit_of(body: &str) -> Option<TrackHit> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let hit = value.get("data")?.as_array()?.first()?;
    let album = hit.get("album");
    Some(TrackHit {
        id: hit.get("id")?.as_u64()?,
        title: hit
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        duration: hit.get("duration").and_then(|d| d.as_u64()).unwrap_or(0) as u32,
        album_id: album
            .and_then(|a| a.get("id"))
            .and_then(|i| i.as_u64())
            .unwrap_or(0),
        album_title: album
            .and_then(|a| a.get("title"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
    })
}

/// The id of the first track in a Deezer search response.
pub fn track_id_of(body: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("data")?
        .as_array()?
        .first()?
        .get("id")?
        .as_u64()
}

/// Whether two durations describe the same recording.
///
/// Track search is fuzzy: asking for an artist and a title can return a remix,
/// a live cut, or a different song altogether. Accepting whatever comes back
/// would import a stranger's tempo for a recording nobody is playing, so the
/// length has to agree first.
///
/// Five seconds, which absorbs the usual disagreement about where a track ends
/// — trailing silence, a fade counted or not — while still separating a radio
/// edit from an extended mix.
pub fn same_recording(ours_secs: f64, theirs_secs: u32) -> bool {
    if theirs_secs == 0 || !ours_secs.is_finite() || ours_secs <= 0.0 {
        return false;
    }
    (ours_secs - theirs_secs as f64).abs() <= 5.0
}

/// The id of the first album in a Deezer search response.
///
/// Needed because the search result does not carry a genre *name* — see
/// [`genre_of`]. The id is what turns a search hit into a request for the full
/// album, which does.
pub fn album_id_of(body: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("data")?
        .as_array()?
        .first()?
        .get("id")?
        .as_u64()
}

/// The genres named by a Deezer **album** response, joined with " / ".
///
/// `genres.data[*].name`, and the emphasis on *album* is the whole point. This
/// parser was correct all along and was being handed the wrong document: it was
/// fed the response from `/search/album`, which has no `genres` object at any
/// level — only a numeric `genre_id`. So it looked, found nothing, and returned
/// an empty string for every track ever looked up, which is indistinguishable
/// from "this album has no genre".
///
/// Verified against the live service 2026-08-16 (TD-51). `/search/album` for
/// Daft Punk's *Discovery* returns `genre_id: 106` and no `genres`;
/// `/album/302127` returns `genres.data[0].name == "Electro"`. Both shapes are
/// in the tests below, captured from the real responses rather than written
/// from reading the GDScript this was ported from — which is how the mismatch
/// survived a full suite of passing tests in the first place.
///
/// **All of them, not `[0]`** (AUD-24). An album filed under both Electronic
/// and Drum & Bass used to keep whichever Deezer listed first, and the coarse
/// one is the likelier first. `vapor_library::tempo_band` reads a `/`-joined
/// field a segment at a time, so a second genre is not decoration: it is what
/// can make the octave correction fire on a track the coarse label could never
/// resolve.
///
/// This does not make Deezer granular. Its taxonomy is roughly twenty-five
/// top-level genres with no drum and bass, no riddim and no neo-classical in
/// it, so most albums still come back with the single word "Electronic". What
/// this recovers is the minority that carry more, at no cost — the real answer
/// to AUD-24 needs a source with a deeper taxonomy, which is AUD-18's decision
/// and not made here.
///
/// Joined rather than returned as a `Vec` because `Row::genre` is one `String`
/// end to end — index, filter, sort, group, the genre tiles and the smart
/// group rules all key on it. Widening that type is its own change.
pub fn genre_of(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return String::new();
    };
    let Some(names) = value
        .get("genres")
        .and_then(|g| g.get("data"))
        .and_then(|d| d.as_array())
    else {
        return String::new();
    };

    let mut kept: Vec<&str> = Vec::new();
    for name in names
        .iter()
        .filter_map(|g| g.get("name").and_then(|n| n.as_str()))
        .map(str::trim)
        .filter(|name| !is_unknown_genre(name))
    {
        // A repeat would read downstream as two genres when it is one, and
        // every consumer of this field — the genre tiles, the group rules,
        // `tempo_band`'s segment split — would see the duplicate.
        if !kept.iter().any(|k| k.eq_ignore_ascii_case(name)) {
            kept.push(name);
        }
    }
    kept.join(" / ")
}

/// What a Deezer **album** response says an album is made of.
///
/// The `/album/{id}` request was already being made — for the genre, one field
/// out of a document that also names how many tracks the release has and what
/// every one of them is called — and the rest was parsed away and dropped. This
/// keeps it, which is what lets the library say "you have 4 of the 8 tracks on
/// *Bangarang EP*" instead of showing a tile that looks like a whole record.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlbumFacts {
    pub id: u64,
    pub title: String,
    pub artist: String,
    /// `"album"`, `"single"` or `"ep"`.
    ///
    /// Kept because it changes what "incomplete" means. Holding one track of a
    /// two-track single is a different statement about a library than holding
    /// one track of a nineteen-track album, and a person reading the tab needs
    /// to be able to tell them apart.
    pub record_type: String,
    /// How many tracks the release has, as the service counts them.
    pub nb_tracks: u32,
    /// Every track title, in album order.
    ///
    /// The embedded list carries no `track_position` — that field is only on
    /// the full `/track/{id}` document — so position *is* array order here, and
    /// nothing downstream may assume otherwise.
    pub tracks: Vec<String>,
}

impl AlbumFacts {
    /// True when the service told us nothing worth keeping.
    pub fn is_usable(&self) -> bool {
        self.id != 0 && self.nb_tracks > 0
    }
}

/// Parse an album's facts out of a Deezer `/album/{id}` response.
///
/// Returns `None` for a document that is not one — a `/search/album` hit, say,
/// which carries `nb_tracks` but no `tracks` and no `genres`. That distinction
/// is the bug [`genre_of`] documents, and this parser is built to be given the
/// wrong document and say so rather than return a confident half-answer.
pub fn album_facts_of(body: &str) -> Option<AlbumFacts> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let id = value.get("id")?.as_u64()?;

    // `tracks.data` is what separates the full document from a search hit.
    let tracks: Vec<String> = value
        .get("tracks")
        .and_then(|t| t.get("data"))
        .and_then(|d| d.as_array())?
        .iter()
        .filter_map(|t| t.get("title")?.as_str())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    let str_at = |key: &str| -> String {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };

    // Prefer the stated count, but never below what was actually listed: a
    // release whose `nb_tracks` disagrees with its own tracklist would
    // otherwise be reported as more than complete.
    let stated = value.get("nb_tracks").and_then(|n| n.as_u64()).unwrap_or(0) as u32;

    Some(AlbumFacts {
        id,
        title: str_at("title"),
        artist: value
            .get("artist")
            .and_then(|a| a.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        record_type: str_at("record_type").to_ascii_lowercase(),
        nb_tracks: stated.max(tracks.len() as u32),
        tracks,
    })
}

/// What one album lookup came back with.
///
/// A struct rather than the tuple this used to return. Three values out of two
/// requests, and a `(String, String, Option<AlbumFacts>)` at four call sites is
/// a shape nobody can read — the art and the genre are both strings, so
/// swapping them is a mistake the compiler cannot catch.
#[derive(Clone, Debug, Default)]
pub struct Found {
    /// Album art, as a URL on the service. Empty when nothing matched.
    pub art: String,
    /// The album's genre, empty when the service names none.
    pub genre: String,
    /// The tracklist and how long the release is. `None` when the album could
    /// not be fetched in full — art can be had from the search hit alone, and
    /// a miss here must not discard it.
    pub facts: Option<AlbumFacts>,
}

/// Everything known about the albums that have been looked up, by Deezer id.
///
/// Keyed by *their* id rather than by our album key, so one tracklist is stored
/// once however many tracks of it the library holds — and so two local folders
/// that turn out to be the same release share an answer.
pub type Albums = std::collections::HashMap<u64, AlbumFacts>;

/// Whether a genre string says nothing.
///
/// Ported from `_is_unknown_genre`. Services return placeholder genres, and a
/// track filed under "Unknown" is worse than one filed under nothing — the
/// first looks like an answer.
pub fn is_unknown_genre(genre: &str) -> bool {
    let g = genre.trim().to_ascii_lowercase();
    g.is_empty() || g == "unknown" || g == "unknown genre" || g == "n/a" || g == "none"
}

/// Whether a name is real enough to search for.
///
/// The original checked `!= "Unknown Artist"` at four call sites; the path
/// parser produces exactly those placeholders, and searching for them returns
/// whatever Deezer thinks "Unknown Artist" is.
pub fn is_searchable(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n != "Unknown Artist" && n != "Unknown Album" && n != "Unknown Track"
}

/// Where a fetched image is kept on disk.
///
/// Named by a hash of its URL, as `_download_image` did, because two tracks
/// from the same album resolve to the same art and downloading it per track
/// would fetch one sleeve a dozen times. Kept as a file rather than inside the
/// JSON cache: a `picture_xl` is a few hundred kilobytes, and a 563-track
/// library's worth of base64 in one document is a file the app would have to
/// read whole to answer any question about any track.
pub fn image_path(dir: &std::path::Path, url: &str) -> std::path::PathBuf {
    // The extension is the server's, minus any query string, and is only ever
    // used to name the file — the data URI's type comes from the bytes.
    let tail = url.rsplit('/').next().unwrap_or("");
    let ext = tail
        .split('?')
        .next()
        .unwrap_or("")
        .rsplit_once('.')
        .map(|(_, e)| e)
        .filter(|e| !e.is_empty() && e.len() <= 4 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("jpg");

    dir.join("metadata_images")
        .join(format!("{}.{ext}", fingerprint(url)))
}

/// A short stable name for a URL. FNV-1a: not a security hash, and does not
/// need to be — it names a cache file.
fn fingerprint(url: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in url.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{h:016x}")
}

/// Read an image file back as a `data:` URI.
///
/// A data URI rather than a file path or an asset URL because the window's CSP
/// allows `data:` and not a remote host — which is the right way round. The
/// image is fetched once, by Rust, at a moment the person asked for; the
/// webview never talks to Deezer.
pub fn image_data_uri(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(format!(
        "data:{};base64,{}",
        image_mime(&bytes),
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)
    ))
}

/// The type of an image from its first bytes.
///
/// Read from the content rather than from the URL's extension: the extension
/// is whatever the path happened to say, and a mislabelled type renders as a
/// broken image with no clue why.
fn image_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(b"GIF") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// Which stranger a URL belongs to.
///
/// Each is paced on its own clock: a wait owed to LRCLIB is not a reason to
/// hold up a Deezer request, and the two run concurrently by design — see the
/// scoped threads in `identify_library`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Service {
    Deezer,
    Lrclib,
    Artwork,
}

impl Service {
    /// Which service a URL is for.
    ///
    /// Matched on the host, not on a substring of the whole URL: a Deezer
    /// search response is full of `cdn-images.dzcdn.net` links, and a query
    /// string can contain anything a track is called.
    fn of(url: &str) -> Option<Self> {
        let host = url
            .split_once("://")
            .map(|(_, rest)| rest)?
            .split(['/', '?', '#'])
            .next()?;
        match host {
            "api.deezer.com" => Some(Service::Deezer),
            "lrclib.net" => Some(Service::Lrclib),
            // Every artwork host seen in a response so far, and the fallback
            // for anything else: an unrecognised host is paced, not exempt.
            _ => Some(Service::Artwork),
        }
    }

    fn gap(self) -> Duration {
        match self {
            Service::Deezer => DEEZER_GAP,
            Service::Lrclib => LRCLIB_GAP,
            Service::Artwork => ARTWORK_GAP,
        }
    }
}

/// The clocks that keep each service's requests apart.
///
/// One mutex per service, holding when that service was last asked. The lock
/// is held **across the sleep**, which is the whole mechanism: two threads
/// wanting Deezer queue behind each other rather than both deciding they may
/// go now. Nothing else is inside it, so the only thing a caller can wait on
/// is the gap it owes.
#[derive(Default)]
struct Pace {
    deezer: Mutex<Option<Instant>>,
    lrclib: Mutex<Option<Instant>>,
    artwork: Mutex<Option<Instant>>,
}

impl Pace {
    fn slot(&self, service: Service) -> &Mutex<Option<Instant>> {
        match service {
            Service::Deezer => &self.deezer,
            Service::Lrclib => &self.lrclib,
            Service::Artwork => &self.artwork,
        }
    }

    /// Block until this service may be asked again, then claim the slot.
    ///
    /// `extra` is a backoff a refused attempt owes on top of the ordinary gap.
    ///
    /// A poisoned lock is recovered from rather than propagated: the only
    /// state behind it is "when did we last ask", and a panic elsewhere is no
    /// reason to stop a lookup — the worst a stale instant costs is one
    /// request sent a little early.
    fn wait(&self, service: Service, extra: Duration) {
        let mut slot = self
            .slot(service)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let owed = match *slot {
            Some(last) => service.gap().saturating_sub(last.elapsed()),
            None => Duration::ZERO,
        } + extra;
        if !owed.is_zero() {
            std::thread::sleep(owed);
        }
        *slot = Some(Instant::now());
    }
}

/// What to do with one response, before its body is used for anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// Usable. Read it.
    Keep,
    /// Refused for a reason that passes — try again after `Duration`, or after
    /// the caller's own backoff when the service named no time.
    Wait(Option<Duration>),
    /// Refused for a reason that will not change. Nothing to retry.
    Drop,
}

/// Read the status line.
///
/// The words are the analysis pass's — a `retryable` outcome there is one
/// where "trying again could plausibly succeed" — but the machinery is not
/// shared, because `analysis::Progress::retryable` is a flag reported to the
/// screen about one track, not a loop. Same distinction, different place to
/// make it.
fn verdict_of(status: u16) -> Verdict {
    match status {
        200..=299 => Verdict::Keep,
        // Too many requests, and the only honest response to it is to stop
        // sending them for a moment.
        429 => Verdict::Wait(None),
        // A server having a bad minute. The request was fine.
        500..=599 => Verdict::Wait(None),
        _ => Verdict::Drop,
    }
}

/// Seconds from a `Retry-After` header, when it is the numeric form.
///
/// The HTTP-date form is allowed by the spec and is not parsed: it needs a
/// date parser for a case neither of these services uses, and falling through
/// to the caller's own backoff is a correct answer, just a less informed one.
fn retry_after(value: &str) -> Option<Duration> {
    let seconds: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(seconds).min(LONGEST_WAIT))
}

/// Whether a Deezer body is a refusal wearing a 200.
///
/// **This is the whole reason the status code is not enough.** Deezer answers
/// errors with `HTTP 200` and an error object — confirmed against the live API
/// on 2026-08-23:
///
/// ```text
/// $ curl -s -w '%{http_code}' https://api.deezer.com/track/0
/// {"error":{"type":"DataException","message":"no data","code":800}}200
/// ```
///
/// So `status().is_success()` is true for a quota refusal, the parsers find no
/// fields they want, and the pass records the track as looked up and empty.
/// Code 4 is "Quota limit exceeded"; code 700 is their service busy. Every
/// other code is a real answer about the track and must not be retried — 800
/// means they genuinely do not have it.
fn deezer_is_throttling(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    let Some(code) = value.get("error").and_then(|e| e.get("code")) else {
        return false;
    };
    // Sent as a number by the API and as a string by at least one of their
    // error paths, so both are read.
    let code = code
        .as_u64()
        .or_else(|| code.as_str().and_then(|s| s.parse().ok()));
    matches!(code, Some(4) | Some(700))
}

/// A client for the two services.
pub struct Lookup {
    client: reqwest::blocking::Client,
    pace: Pace,
}

impl Lookup {
    pub fn new() -> Result<Self, String> {
        // Through `crate::http`: `Lookup::new` is reached from Tauri commands,
        // and building a blocking client on a runtime worker panics.
        let client = crate::http::build_blocking(|| {
            reqwest::blocking::Client::builder()
                .timeout(TIMEOUT)
                .user_agent(USER_AGENT)
                .build()
                .map_err(|e| e.to_string())
        })?;
        Ok(Lookup {
            client,
            pace: Pace::default(),
        })
    }

    /// Ask for `url`, at a pace the service asked for, giving up only on
    /// answers that trying again cannot change.
    ///
    /// `read` turns a kept response into the thing wanted. It returns `None`
    /// twice over: outer for "this response is not usable at all", inner for
    /// the body itself. Splitting it out is what lets one loop serve both a
    /// JSON document and an image.
    fn fetch<T>(
        &self,
        url: &str,
        read: impl Fn(reqwest::blocking::Response) -> Option<T>,
        again: impl Fn(&T) -> bool,
    ) -> Option<T> {
        let Some(service) = Service::of(url) else {
            return None;
        };
        let mut backoff = BACKOFF;
        let mut owed = Duration::ZERO;

        for attempt in 1..=ATTEMPTS {
            let last = attempt == ATTEMPTS;
            // Taken rather than read: a backoff is owed once, and leaving it
            // set would charge every later attempt for the first refusal.
            self.pace.wait(service, std::mem::take(&mut owed));

            let response = match self.client.get(url).send() {
                Ok(response) => response,
                // A connection that never landed, or one that timed out, is
                // the same kind of nothing as a 503 — the difference is only
                // where it stopped.
                Err(e) if e.is_timeout() || e.is_connect() || e.is_request() => {
                    if last {
                        return None;
                    }
                    owed = backoff;
                    backoff *= 2;
                    continue;
                }
                Err(_) => return None,
            };

            match verdict_of(response.status().as_u16()) {
                Verdict::Drop => return None,
                Verdict::Wait(_) if last => return None,
                Verdict::Wait(_) => {
                    owed = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(retry_after)
                        .unwrap_or(backoff);
                    backoff *= 2;
                    continue;
                }
                Verdict::Keep => {
                    let value = read(response)?;
                    if !again(&value) {
                        return Some(value);
                    }
                    if last {
                        return None;
                    }
                    owed = backoff;
                    backoff *= 2;
                }
            }
        }
        None
    }

    fn get(&self, url: &str) -> Option<String> {
        self.fetch(
            url,
            |response| response.text().ok(),
            // The 200-with-an-error-body case. Anything else in the document
            // is an answer, including "no data".
            |body: &String| deezer_is_throttling(body),
        )
    }

    /// Lyrics for one track, or `None` when the service has none.
    pub fn lyrics(&self, artist: &str, title: &str) -> Option<Lyrics> {
        if !is_searchable(artist) || !is_searchable(title) {
            return None;
        }
        let url = format!(
            "https://lrclib.net/api/get?artist_name={}&track_name={}",
            encode(artist),
            encode(title)
        );
        lyrics_of(&self.get(&url)?)
    }

    pub fn artist_image(&self, artist: &str) -> String {
        if !is_searchable(artist) {
            return String::new();
        }
        let url = format!("https://api.deezer.com/search/artist?q={}", encode(artist));
        self.get(&url)
            .map(|b| image_url_of(&b, ARTIST_KEYS))
            .unwrap_or_default()
    }

    /// What Deezer knows about one recording: its tempo and its length.
    ///
    /// Two requests, because the search result does not carry a tempo — the
    /// same shape as the album lookup, and for the same reason.
    pub fn track_facts(&self, artist: &str, title: &str) -> Option<TrackFacts> {
        if !is_searchable(title) {
            return None;
        }
        let query = if is_searchable(artist) {
            format!("{artist} {title}")
        } else {
            title.to_string()
        };
        let url = format!("https://api.deezer.com/search/track?q={}", encode(&query));
        let id = track_id_of(&self.get(&url)?)?;
        track_facts_of(&self.get(&format!("https://api.deezer.com/track/{id}"))?)
    }

    /// The release a loose track came off, found from the track alone.
    ///
    /// [`Self::album`] cannot help here: it searches by album name, and these
    /// are the files that have no album name — 97 of this library's 563, sitting
    /// in the root because they were downloaded one at a time. They were
    /// dropped from the Albums tab entirely, which is why a person who owns
    /// four tracks of *Bangarang EP* saw no sign of it.
    ///
    /// Guarded by length. Track search is fuzzy — the same title by another
    /// artist, a remix, a live cut — and attaching a file to the wrong record
    /// would invent an album the library does not have and then report it as
    /// 1 of 19. `ours_secs` is the duration this device measured; a hit that
    /// disagrees is a different recording and is refused. Same rule, and the
    /// same reason, as the tempo correction in [`same_recording`].
    pub fn album_of_track(&self, artist: &str, title: &str, ours_secs: f64) -> Option<AlbumFacts> {
        if !is_searchable(title) {
            return None;
        }
        let query = if is_searchable(artist) {
            format!("{artist} {title}")
        } else {
            title.to_string()
        };
        let url = format!("https://api.deezer.com/search/track?q={}", encode(&query));
        let hit = track_hit_of(&self.get(&url)?)?;
        if hit.album_id == 0 || !same_recording(ours_secs, hit.duration) {
            return None;
        }
        album_facts_of(&self.get(&format!("https://api.deezer.com/album/{}", hit.album_id))?)
    }

    /// Album art, and the genre.
    ///
    /// Two requests, not one. This used to claim the search response "already
    /// names the genre the original was going back for" — it does not, and the
    /// genre has therefore been empty for every track since the port. The
    /// search gives the art and an album id; the genre needs `/album/{id}`.
    /// See [`genre_of`].
    ///
    /// The second request is only made once there is art, so a miss costs
    /// nothing extra: no art means no album matched, and there is no id worth
    /// asking about.
    pub fn album(&self, artist: &str, album: &str) -> Found {
        if !is_searchable(album) {
            return Found::default();
        }
        // "Artist Album" first, then the album alone — the original's fallback,
        // which exists because a common album title matches half the catalogue.
        let queries = if is_searchable(artist) {
            vec![format!("{artist} {album}"), album.to_string()]
        } else {
            vec![album.to_string()]
        };

        for query in queries {
            let url = format!("https://api.deezer.com/search/album?q={}", encode(&query));
            let Some(body) = self.get(&url) else { continue };
            let art = image_url_of(&body, ALBUM_KEYS);
            if !art.is_empty() {
                // The second request is worth making but not worth failing
                // over: art on a screen is the point, and a missing genre is
                // the state the app has been in all along. One request, two
                // answers — the genre and the tracklist come out of the same
                // document, so keeping the facts costs nothing beyond parsing
                // what was already on the wire.
                let full = album_id_of(&body)
                    .and_then(|id| self.get(&format!("https://api.deezer.com/album/{id}")));
                return Found {
                    art,
                    genre: full.as_deref().map(genre_of).unwrap_or_default(),
                    facts: full.as_deref().and_then(album_facts_of),
                };
            }
        }
        Found::default()
    }

    /// Fetch an image and keep it, returning where it was kept.
    ///
    /// Already downloaded means already done — the file *is* the cache, which
    /// is what stops one album sleeve being fetched once per track on it.
    /// A non-200 leaves nothing behind: the original wrote the response body
    /// to the destination first and had to delete the truncated file
    /// afterwards, and any failure between the two left a broken image that
    /// the next `file_exists` check would hand back as a hit.
    pub fn download_image(&self, url: &str, dir: &std::path::Path) -> Option<std::path::PathBuf> {
        if url.is_empty() || !url.starts_with("http") {
            return None;
        }
        let path = image_path(dir, url);
        if path.is_file() {
            return Some(path);
        }

        // Same pacing and the same backoff as everything else: a library-wide
        // pass fetches a sleeve per album, and the CDN is entitled to the same
        // manners as the API. No body check — an image has no error object to
        // read, so `false` is the honest answer to "should this be retried".
        let bytes = self.fetch(url, |response| response.bytes().ok(), |_| false)?;
        if bytes.is_empty() {
            return None;
        }

        std::fs::create_dir_all(path.parent()?).ok()?;
        // Written whole and then renamed, so a partial file never exists under
        // the name the cache check looks for.
        let temporary = path.with_extension("part");
        std::fs::write(&temporary, &bytes).ok()?;
        std::fs::rename(&temporary, &path).ok()?;
        Some(path)
    }
}

/// Percent-encode a query value.
///
/// Written out rather than pulled in: one dependency for `encode` is not worth
/// it, and the unreserved set is four lines.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    // -----------------------------------------------------------------------
    // Manners (AUD-18)
    // -----------------------------------------------------------------------

    /// LRCLIB requires a name, a version and a contact link; MusicBrainz would
    /// require the same shape and throttles anything it calls "anonymous".
    /// This asserts the header is all three things, so a well-meant tidy-up
    /// cannot quietly reduce it to a product name.
    #[test]
    fn the_user_agent_names_the_app_its_version_and_somewhere_to_complain() {
        assert!(USER_AGENT.starts_with("VaporMusic/"), "{USER_AGENT}");
        let (_, rest) = USER_AGENT.split_once('/').expect("a version after the name");
        let (version, contact) = rest.split_once(' ').expect("a contact after the version");
        assert!(
            version.split('.').count() >= 3 && version.starts_with(|c: char| c.is_ascii_digit()),
            "the version is not a version: {version}"
        );
        assert!(
            contact.starts_with("(http") || contact.contains('@'),
            "no contact url or address: {contact}"
        );
    }

    /// Pacing is decided by host, and a Deezer search response is full of CDN
    /// links — so a substring match on the whole URL would put an artwork
    /// download on the API's clock, and a track called "lrclib.net" on
    /// LRCLIB's.
    #[test]
    fn a_url_is_placed_by_its_host_and_not_by_its_query() {
        assert_eq!(
            Service::of("https://api.deezer.com/search/track?q=x"),
            Some(Service::Deezer)
        );
        assert_eq!(
            Service::of("https://lrclib.net/api/get?artist_name=a"),
            Some(Service::Lrclib)
        );
        assert_eq!(
            Service::of("https://cdn-images.dzcdn.net/images/cover/x.jpg"),
            Some(Service::Artwork)
        );
        assert_eq!(
            Service::of("https://lrclib.net.example.com/x"),
            Some(Service::Artwork),
            "a host that merely begins with a known one is not that service"
        );
        assert_eq!(
            Service::of("https://example.com/track?q=api.deezer.com"),
            Some(Service::Artwork),
            "a query naming a service is not a request to it"
        );
        assert_eq!(Service::of("not a url"), None);
    }

    /// The distinction the retry loop is built on.
    #[test]
    fn a_refusal_is_retried_and_an_answer_is_not() {
        assert_eq!(verdict_of(200), Verdict::Keep);
        assert_eq!(verdict_of(429), Verdict::Wait(None));
        assert_eq!(verdict_of(503), Verdict::Wait(None));
        assert_eq!(verdict_of(500), Verdict::Wait(None));
        // Not found is an answer about the track. Retrying it is asking the
        // same question again and getting told the same thing.
        assert_eq!(verdict_of(404), Verdict::Drop);
        assert_eq!(verdict_of(403), Verdict::Drop);
    }

    /// The fault a status-code-only check cannot see.
    ///
    /// Deezer sends its errors with `HTTP 200`, so without this the quota
    /// refusal parses as a track with no genre and is cached as one.
    #[test]
    fn deezers_quota_refusal_arrives_wearing_a_200() {
        assert!(deezer_is_throttling(
            r#"{"error":{"type":"Exception","message":"Quota limit exceeded","code":4}}"#
        ));
        assert!(
            deezer_is_throttling(r#"{"error":{"code":"4"}}"#),
            "the code is sent as a string on at least one of their error paths"
        );
        assert!(deezer_is_throttling(r#"{"error":{"code":700}}"#));
        assert!(
            !deezer_is_throttling(
                r#"{"error":{"type":"DataException","message":"no data","code":800}}"#
            ),
            "\"we do not have this track\" is an answer, and re-asking wastes \
             three more requests on it"
        );
        assert!(!deezer_is_throttling(REAL_ALBUM_FULL));
        assert!(!deezer_is_throttling("not json at all"));
    }

    /// A `Retry-After` is honoured, and a silly one is not.
    #[test]
    fn a_named_wait_is_taken_but_capped() {
        assert_eq!(retry_after("2"), Some(Duration::from_secs(2)));
        assert_eq!(retry_after(" 5 "), Some(Duration::from_secs(5)));
        assert_eq!(
            retry_after("600"),
            Some(LONGEST_WAIT),
            "a ten-minute wait parks a thread; the request is abandoned instead"
        );
        // The HTTP-date form is legal and unparsed — the caller's own backoff
        // covers it.
        assert_eq!(retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), None);
    }

    /// The behaviour the whole ticket is about: two threads asking the same
    /// service cannot both decide they may go now.
    ///
    /// `identify_library` runs `track_facts` and `album` on scoped threads
    /// against one `Lookup`, so the gate has to hold across them or it holds
    /// nothing.
    #[test]
    fn two_threads_wanting_one_service_queue_behind_each_other() {
        let pace = Pace::default();
        let started = Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..3 {
                scope.spawn(|| pace.wait(Service::Deezer, Duration::ZERO));
            }
        });
        // Three requests, two gaps between them — the first is free.
        assert!(
            started.elapsed() >= DEEZER_GAP * 2,
            "three Deezer requests took {:?}, which is faster than the pace \
             allows",
            started.elapsed()
        );
    }

    /// And a wait owed to one service does not hold up another. The two are
    /// asked concurrently on purpose; sharing one clock would undo that.
    #[test]
    fn the_services_are_paced_on_separate_clocks() {
        let pace = Pace::default();
        pace.wait(Service::Lrclib, Duration::ZERO);
        let started = Instant::now();
        pace.wait(Service::Deezer, Duration::ZERO);
        assert!(
            started.elapsed() < LRCLIB_GAP,
            "a Deezer request waited on LRCLIB's clock"
        );
    }

    /// The defect this pair of flags exists to prevent.
    ///
    /// Measured on a real cache 2026-08-22: 534 of 534 entries had
    /// `attempted: true` and **zero** had lyrics or album art, because the
    /// facts pass set one flag that every other path read as "nothing left to
    /// get". A record can only claim what it actually did.
    #[test]
    fn asking_deezer_for_a_genre_does_not_claim_the_words_were_asked_for() {
        let mut entry = Looked {
            genre: "Pop".to_string(),
            deezer_bpm: 160.2,
            ..Default::default()
        };
        // What the facts pass sets.
        entry.attempted = true;

        assert!(
            !entry.words_attempted,
            "the facts pass claimed LRCLIB had been asked"
        );
        assert!(entry.lyrics.is_none());
        assert!(entry.album_art.is_empty());
    }

    /// The caches already written are repaired by reading them, with no
    /// migration: `words_attempted` is absent from every one of them, and
    /// absent deserialises to false.
    #[test]
    fn a_cache_written_before_the_split_asks_for_its_words_again() {
        let json = r#"{
            "lyrics": null,
            "artistImage": "",
            "albumArt": "",
            "genre": "Pop",
            "deezerBpm": 160.2,
            "deezerDuration": 377,
            "attempted": true
        }"#;

        let entry: Looked = serde_json::from_str(json).expect("parse");

        assert!(entry.attempted, "the old flag still reads");
        assert!(
            !entry.words_attempted,
            "an entry from before the split must not look already-asked"
        );
        assert_eq!(entry.genre, "Pop", "the facts it did get are kept");
    }

    use super::*;

    /// The real path behind the crash, not a stand-in for it.
    ///
    /// `look_up_track` and `find_album_art` are Tauri commands, Tauri runs
    /// commands on its async runtime's workers, and both call `Lookup::new`.
    /// Building a `reqwest::blocking::Client` there took the process down —
    /// so opening Liner Notes could kill the app, and at shutdown the corpse
    /// survived `tauri dev`'s attempt to replace it.
    #[test]
    fn a_lookup_can_be_built_from_a_runtime_thread() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("a runtime");

        rt.block_on(async {
            Lookup::new().expect("a lookup, rather than a panicked worker");
        });
    }

    #[test]
    fn a_plain_lrc_line_lands_at_its_timestamp() {
        let lines = parse_lrc("[01:02.50]Halfway through the second minute");

        assert_eq!(lines.len(), 1);
        assert!((lines[0].time - 62.5).abs() < 0.001, "{}", lines[0].time);
        assert_eq!(lines[0].text, "Halfway through the second minute");
    }

    /// The original divided whatever it matched by 100, so LRCLIB's
    /// three-digit milliseconds arrived ten times too large — a line five
    /// seconds late rather than half a second in.
    #[test]
    fn milliseconds_are_read_as_milliseconds_not_as_centiseconds() {
        let lines = parse_lrc("[00:10.500]Here");

        assert!((lines[0].time - 10.5).abs() < 0.001, "{}", lines[0].time);
    }

    /// A repeated chorus is written as one line under several timestamps. The
    /// original kept the first and dropped the rest, so the words stopped
    /// moving the second time round.
    #[test]
    fn a_line_under_several_timestamps_appears_at_each_of_them() {
        let lines = parse_lrc("[00:30.00][01:30.00][02:30.00]The chorus");

        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines.iter().map(|l| l.time as i32).collect::<Vec<_>>(),
            [30, 90, 150]
        );
        assert!(lines.iter().all(|l| l.text == "The chorus"));
    }

    #[test]
    fn lines_come_back_in_time_order() {
        let lines = parse_lrc("[02:00.00]Late\n[00:10.00][01:00.00]Early");

        let times: Vec<i32> = lines.iter().map(|l| l.time as i32).collect();
        assert_eq!(times, [10, 60, 120]);
    }

    /// LRCLIB marks an instrumental break with a timed empty line. Dropping it
    /// leaves the previous line on screen through the whole break.
    #[test]
    fn a_timed_empty_line_is_kept() {
        let lines = parse_lrc("[00:05.00]Words\n[00:20.00]\n[00:40.00]More");

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].text, "");
    }

    #[test]
    fn metadata_headers_and_untimed_lines_are_ignored() {
        let lines = parse_lrc("[ar:Someone]\n[by:Someone else]\nloose text\n[00:01.00]Real");

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Real");
    }

    #[test]
    fn synced_lyrics_are_preferred_over_plain_ones() {
        let body = r#"{"syncedLyrics":"[00:01.00]Timed","plainLyrics":"Untimed"}"#;

        let lyrics = lyrics_of(body).expect("some lyrics");
        assert!(lyrics.synced);
        assert_eq!(lyrics.lines[0].text, "Timed");
        assert!(lyrics.plain.is_empty());
    }

    #[test]
    fn plain_lyrics_are_used_when_there_are_no_synced_ones() {
        let body = r#"{"syncedLyrics":"","plainLyrics":"Just the words"}"#;

        let lyrics = lyrics_of(body).expect("some lyrics");
        assert!(!lyrics.synced);
        assert_eq!(lyrics.plain, "Just the words");
    }

    /// LRCLIB answers 200 with both fields empty for a track it knows about
    /// but has no words for. That is "none", not "some blank ones".
    #[test]
    fn a_response_with_no_words_in_it_yields_nothing() {
        assert!(lyrics_of(r#"{"syncedLyrics":"","plainLyrics":""}"#).is_none());
        assert!(lyrics_of(r#"{"statusCode":404}"#).is_none());
        assert!(lyrics_of("not json at all").is_none());
    }

    #[test]
    fn the_largest_available_image_is_chosen() {
        let body = r#"{"data":[{"picture_small":"small.jpg","picture_big":"big.jpg"}]}"#;

        assert_eq!(image_url_of(body, ARTIST_KEYS), "big.jpg");
    }

    #[test]
    fn an_empty_search_result_yields_no_image() {
        assert_eq!(image_url_of(r#"{"data":[]}"#, ARTIST_KEYS), "");
        assert_eq!(image_url_of(r#"{"error":{}}"#, ALBUM_KEYS), "");
        assert_eq!(image_url_of("", ALBUM_KEYS), "");
    }

    /// An empty string for a size is not a URL, and taking it would leave an
    /// `<img>` pointed at nothing rather than falling through to a size that
    /// exists.
    #[test]
    fn an_empty_url_falls_through_to_the_next_size() {
        let body = r#"{"data":[{"cover_xl":"","cover_medium":"medium.jpg"}]}"#;

        assert_eq!(image_url_of(body, ALBUM_KEYS), "medium.jpg");
    }

    #[test]
    fn a_placeholder_genre_is_treated_as_no_genre() {
        assert!(is_unknown_genre("Unknown"));
        assert!(is_unknown_genre("  unknown genre "));
        assert!(is_unknown_genre(""));
        assert!(!is_unknown_genre("Ambient"));

        let body = r#"{"genres":{"data":[{"name":"Unknown"}]}}"#;
        assert_eq!(genre_of(body), "");
    }

    #[test]
    fn a_real_genre_is_read_from_the_album_response() {
        let body = r#"{"genres":{"data":[{"name":"Electronic"}]}}"#;

        assert_eq!(genre_of(body), "Electronic");
    }

    /// AUD-24's `[0]` half. The first genre is the shelf Deezer had room for;
    /// anything after it is the one that says something about the record, and
    /// `vapor_library::tempo_band` splits this field on `/` to read it.
    #[test]
    fn every_genre_the_album_names_is_kept() {
        let body = r#"{"genres":{"data":[{"name":"Electronic"},{"name":"Drum & Bass"}]}}"#;
        assert_eq!(genre_of(body), "Electronic / Drum & Bass");
        assert_eq!(
            vapor_library::tempo_band(&genre_of(body)),
            Some((160.0, 185.0)),
            "the second genre has to reach the tempo band to be worth keeping"
        );
    }

    /// A placeholder among real genres is dropped, not joined; and a genre
    /// listed twice is one genre.
    #[test]
    fn placeholders_and_repeats_do_not_survive_the_join() {
        let body = r#"{"genres":{"data":[{"name":"Unknown"},{"name":"Techno"},{"name":"techno"},{"name":""}]}}"#;
        assert_eq!(genre_of(body), "Techno");
    }

    // -----------------------------------------------------------------------
    // Against what the services actually return (TD-51)
    // -----------------------------------------------------------------------
    //
    // Every test above this line was written from reading `metadata_service.gd`
    // rather than from a real response, which is exactly how a parser can pass
    // a full suite and still never work. These four bodies were captured from
    // the live services on 2026-08-16 — Daft Punk, *Discovery*, "One More
    // Time", chosen because it is a well-known public record and not something
    // out of anyone's library. Trimmed to the fields the parsers read; shapes
    // and spellings are untouched.

    /// `GET https://api.deezer.com/search/artist?q=Daft%20Punk`
    const REAL_ARTIST_SEARCH: &str = r#"{"data":[{"id":27,"name":"Daft Punk","picture":"https://api.deezer.com/artist/27/image","picture_small":"https://cdn-images.dzcdn.net/images/artist/638e69b9caaf9f9f3f8826febea7b543/56x56-000000-80-0-0.jpg","picture_medium":"https://cdn-images.dzcdn.net/images/artist/638e69b9caaf9f9f3f8826febea7b543/250x250-000000-80-0-0.jpg","picture_big":"https://cdn-images.dzcdn.net/images/artist/638e69b9caaf9f9f3f8826febea7b543/500x500-000000-80-0-0.jpg","picture_xl":"https://cdn-images.dzcdn.net/images/artist/638e69b9caaf9f9f3f8826febea7b543/1000x1000-000000-80-0-0.jpg","type":"artist"}],"total":58}"#;

    /// `GET https://api.deezer.com/search/album?q=Daft%20Punk%20Discovery`
    ///
    /// Note what is *not* here: any `genres` object. Only `genre_id`.
    const REAL_ALBUM_SEARCH: &str = r#"{"data":[{"id":302127,"title":"Discovery","cover":"https://api.deezer.com/album/302127/image","cover_small":"https://cdn-images.dzcdn.net/images/cover/5718f7c81c27e0b2417e2a4c45224f8a/56x56-000000-80-0-0.jpg","cover_medium":"https://cdn-images.dzcdn.net/images/cover/5718f7c81c27e0b2417e2a4c45224f8a/250x250-000000-80-0-0.jpg","cover_big":"https://cdn-images.dzcdn.net/images/cover/5718f7c81c27e0b2417e2a4c45224f8a/500x500-000000-80-0-0.jpg","cover_xl":"https://cdn-images.dzcdn.net/images/cover/5718f7c81c27e0b2417e2a4c45224f8a/1000x1000-000000-80-0-0.jpg","genre_id":106,"nb_tracks":14,"record_type":"album","explicit_lyrics":false,"type":"album"}],"total":300}"#;

    /// `GET https://api.deezer.com/album/302127` — recaptured 2026-08-21.
    ///
    /// The earlier capture kept only the fields the genre lookup read. The rest
    /// of the document is the point now: `record_type`, and a `tracks.data`
    /// naming all fourteen. Fields irrelevant to any parser here (`fans`,
    /// `upc`, the four smaller cover sizes) are still trimmed out.
    const REAL_ALBUM_FULL: &str = r#"{"id":302127,"title":"Discovery","artist":{"name":"Daft Punk"},"record_type":"album","nb_tracks":14,"genres":{"data":[{"id":106,"name":"Electro"}]},"cover_xl":"https://cdn-images.dzcdn.net/images/cover/5718f7c81c27e0b2417e2a4c45224f8a/1000x1000-000000-80-0-0.jpg","tracks":{"data":[{"id":3135553,"title":"One More Time"},{"id":3135554,"title":"Aerodynamic"},{"id":3135555,"title":"Digital Love"},{"id":3135556,"title":"Harder, Better, Faster, Stronger"},{"id":3135557,"title":"Crescendolls"},{"id":3135558,"title":"Nightvision"},{"id":3135559,"title":"Superheroes"},{"id":3135560,"title":"High Life"},{"id":3135561,"title":"Something About Us"},{"id":3135562,"title":"Voyager"},{"id":3135563,"title":"Veridis Quo"},{"id":3135564,"title":"Short Circuit"},{"id":3135565,"title":"Face to Face"},{"id":3135566,"title":"Too Long"}]}}"#;

    /// `GET https://lrclib.net/api/get?artist_name=Daft%20Punk&track_name=One%20More%20Time`
    const REAL_LRCLIB: &str = r#"{"id":250327,"trackName":"One More Time","artistName":"Daft Punk","albumName":"Discovery","duration":320.0,"instrumental":false,"plainLyrics":"One more time\n\nOne more time\n","syncedLyrics":"[00:30.75] One more time\n[00:33.18] \n[00:46.35] One more time\n[00:49.25] \n[01:03.76] One more time\n[01:04.92] We're gonna celebrate"}"#;

    /// The bug TD-51 was recorded to catch, kept as the thing that would catch
    /// it again: the album *search* response names no genre, so asking it for
    /// one returns nothing — for every track, silently.
    #[test]
    fn the_album_search_response_carries_no_genre_at_all() {
        assert_eq!(
            genre_of(REAL_ALBUM_SEARCH),
            "",
            "if this ever returns a genre, Deezer changed the search response \
             and `album` can go back to one request"
        );
        // It does carry the art, which is why the mistake was invisible.
        assert!(
            image_url_of(REAL_ALBUM_SEARCH, ALBUM_KEYS).ends_with("1000x1000-000000-80-0-0.jpg")
        );
    }

    /// And the full album response, which is where the genre actually is.
    #[test]
    fn the_full_album_response_names_the_genre() {
        assert_eq!(album_id_of(REAL_ALBUM_SEARCH), Some(302127));
        assert_eq!(genre_of(REAL_ALBUM_FULL), "Electro");
    }

    /// The same document also says how long the record is, and what is on it.
    ///
    /// This was being fetched and thrown away: one field read, the tracklist
    /// parsed past. It is the only thing in the app that can tell a complete
    /// album from four tracks of one.
    #[test]
    fn the_full_album_response_names_the_whole_tracklist() {
        let facts = album_facts_of(REAL_ALBUM_FULL).expect("a full album document parses");
        assert_eq!(facts.id, 302127);
        assert_eq!(facts.title, "Discovery");
        assert_eq!(facts.artist, "Daft Punk");
        assert_eq!(facts.record_type, "album");
        assert_eq!(facts.nb_tracks, 14);
        assert_eq!(facts.tracks.len(), 14);
        // Array order is album order — the embedded list carries no
        // `track_position` to sort on, so first must mean first.
        assert_eq!(
            facts.tracks.first().map(String::as_str),
            Some("One More Time")
        );
        assert_eq!(facts.tracks.last().map(String::as_str), Some("Too Long"));
        assert!(facts.is_usable());
    }

    /// Given the wrong document, it says so rather than half-answering.
    ///
    /// The search response carries a believable `nb_tracks` and no tracklist,
    /// which is exactly the shape that fooled [`genre_of`] for every track
    /// since the port (TD-51). A parser that accepted it would report an album
    /// as complete on the strength of a number with nothing behind it.
    #[test]
    fn an_album_search_hit_is_not_mistaken_for_a_full_album() {
        assert_eq!(
            album_facts_of(REAL_ALBUM_SEARCH),
            None,
            "the search response has no tracks.data and must not parse as facts"
        );
        assert_eq!(album_facts_of("not json"), None);
        assert_eq!(album_facts_of("{}"), None);
    }

    /// A stated count below the tracklist is not believed.
    ///
    /// Otherwise an album could be held "more than completely" — 3 of 2 — and
    /// the completeness sort would put nonsense at the top of the tab.
    #[test]
    fn the_track_count_is_never_less_than_the_tracks_listed() {
        let body = r#"{"id":1,"nb_tracks":2,"tracks":{"data":[{"title":"A"},{"title":"B"},{"title":"C"}]}}"#;
        let facts = album_facts_of(body).expect("parses");
        assert_eq!(
            facts.nb_tracks, 3,
            "the listed tracks outnumbered the stated count"
        );
    }

    #[test]
    fn the_artist_ladder_matches_what_deezer_sends() {
        let url = image_url_of(REAL_ARTIST_SEARCH, ARTIST_KEYS);
        assert!(
            url.contains("1000x1000"),
            "expected the xl rung of the ladder, got {url}"
        );
    }

    // -----------------------------------------------------------------------
    // Deezer's own tempo, and the guards on believing it
    // -----------------------------------------------------------------------

    /// `GET https://api.deezer.com/track/3786816142` — captured 2026-08-17.
    /// Note `bpm: 0`, which is Deezer for "we do not know" and is the common
    /// case rather than the exception.
    const REAL_TRACK_UNKNOWN_BPM: &str = r#"{"id":3786816142,"title":"Space Time","duration":289,"bpm":0,"gain":-7.3,"type":"track"}"#;

    /// A recording they do have a tempo for. Ours reads 87.0 for this one.
    const REAL_TRACK_KNOWN_BPM: &str = r#"{"id":128069739,"title":"Bonfire","duration":272,"bpm":173.7,"gain":-5.4,"type":"track"}"#;

    #[test]
    fn a_track_response_is_read_including_the_absent_tempo() {
        let known = track_facts_of(REAL_TRACK_KNOWN_BPM).expect("a track");
        assert!((known.bpm - 173.7).abs() < 1e-3);
        assert_eq!(known.duration, 272);

        let unknown = track_facts_of(REAL_TRACK_UNKNOWN_BPM).expect("a track");
        assert_eq!(unknown.bpm, 0.0, "zero is absent, not a tempo");
        assert_eq!(unknown.duration, 289);
    }

    /// Deezer answers a bad id with an error object, which is valid JSON and
    /// not a track. Reading it as one would give a tempo of zero and a length
    /// of zero, and the length is what guards everything else.
    #[test]
    fn an_error_response_is_not_a_track() {
        let body = r#"{"error":{"type":"DataException","message":"no data","code":800}}"#;
        assert_eq!(track_facts_of(body), None);
        assert_eq!(track_facts_of("not json"), None);
    }

    /// Track search is fuzzy — an artist and a title can return a remix, a live
    /// cut, or a different song. The length has to agree before anything of
    /// theirs is believed, or a wrong hit imports a stranger's tempo for a
    /// recording nobody is playing.
    #[test]
    fn a_different_recording_is_refused_by_its_length() {
        // The same track: 289 s against 289.7 s measured here.
        assert!(same_recording(289.7, 289));
        assert!(same_recording(272.0, 272));
        // A radio edit against an extended mix.
        assert!(!same_recording(289.0, 210));
        // An absent length proves nothing, so it is refused.
        assert!(!same_recording(289.0, 0));
        assert!(!same_recording(0.0, 289));
        assert!(!same_recording(f64::NAN, 289));
    }

    #[test]
    fn a_search_response_yields_the_first_track_id() {
        let body = r#"{"data":[{"id":3786816142,"title":"Space Time"},{"id":9,"title":"Other"}],"total":2}"#;
        assert_eq!(track_id_of(body), Some(3786816142));
        assert_eq!(track_id_of(r#"{"data":[]}"#), None);
        assert_eq!(track_id_of("{}"), None);
    }

    /// The whole lookup, against the live services.
    ///
    /// `#[ignore]` for two reasons and both matter: CI has no network, and
    /// running this sends a query to two third parties — the precise thing the
    /// app makes people opt into. It is not run for you.
    ///
    /// It exists because TD-51's actual complaint was that nothing had *ever*
    /// been run against the real thing, so the captured bodies above are only
    /// as current as the day they were captured. This is the one command that
    /// re-checks them:
    ///
    /// ```text
    /// cargo test --lib live_services -- --ignored --nocapture
    /// ```
    ///
    /// A failure here means a service changed shape, not that the app is
    /// broken — read the printed output before changing any parser.
    #[test]
    #[ignore = "makes real network requests to LRCLIB and Deezer"]
    fn the_parsers_still_match_the_live_services() {
        let lookup = Lookup::new().expect("http client");

        // A well-known public record, deliberately not anything out of the
        // owner's library: this is a request to a stranger's server, and the
        // query is the one thing it learns.
        let (artist, album, title) = ("Daft Punk", "Discovery", "One More Time");

        let lyrics = lookup.lyrics(artist, title);
        println!(
            "lyrics: {:?}",
            lyrics.as_ref().map(|l| (l.synced, l.lines.len()))
        );
        let lyrics = lyrics.expect("LRCLIB returned no usable lyrics");
        assert!(lyrics.synced, "synced lyrics stopped arriving");
        assert!(lyrics.lines.len() > 10);
        assert!(
            lyrics.lines.windows(2).all(|w| w[0].time <= w[1].time),
            "lines came back out of order"
        );

        let portrait = lookup.artist_image(artist);
        println!("artist image: {portrait}");
        assert!(portrait.starts_with("http"), "no artist portrait");

        let found = lookup.album(artist, album);
        println!(
            "album art: {}\ngenre: {}\nfacts: {:?}",
            found.art, found.genre, found.facts
        );
        assert!(found.art.starts_with("http"), "no album art");
        // The regression this ticket found. An empty genre here is the bug
        // coming back, not a record without a genre — Discovery has one.
        assert!(
            !found.genre.is_empty(),
            "the genre is empty again: the album search response is being asked \
             for something it does not carry"
        );

        // The tracklist, which is what album completeness is measured against.
        // If Deezer ever stops embedding `tracks.data` in the album document,
        // every album in the library silently becomes "length unknown" and the
        // Incomplete group empties out — so it is worth failing loudly here.
        let facts = found
            .facts
            .expect("no album facts: tracks.data went missing");
        assert_eq!(facts.nb_tracks, 14, "Discovery is a fourteen-track record");
        assert_eq!(
            facts.tracks.len(),
            14,
            "the tracklist did not arrive in full"
        );
        assert_eq!(facts.record_type, "album");
    }

    /// LRCLIB's field names and its timestamp precision, from a real body.
    /// `[00:30.75]` is centiseconds — read as milliseconds it would be 30.0008s
    /// and every line would land on top of the one before it.
    #[test]
    fn lrclib_synced_lyrics_parse_at_the_right_times() {
        let lyrics = lyrics_of(REAL_LRCLIB).expect("a body with words in it");

        assert!(
            lyrics.synced,
            "synced lyrics were present and not preferred"
        );
        assert_eq!(lyrics.lines.len(), 6);
        assert!(
            (lyrics.lines[0].time - 30.75).abs() < 1e-3,
            "{}",
            lyrics.lines[0].time
        );
        assert_eq!(lyrics.lines[0].text, "One more time");
        // A minute rolls over correctly: [01:03.76] is 63.76s, not 1.0376.
        assert!(
            (lyrics.lines[4].time - 63.76).abs() < 1e-3,
            "{}",
            lyrics.lines[4].time
        );
    }

    /// The placeholders the path parser produces must never become a search
    /// term — "Unknown Artist" returns whatever Deezer thinks that is.
    #[test]
    fn the_parsers_placeholders_are_not_searched_for() {
        assert!(!is_searchable("Unknown Artist"));
        assert!(!is_searchable("Unknown Album"));
        assert!(!is_searchable("   "));
        assert!(is_searchable("Aphex Twin"));
    }

    #[test]
    fn an_image_is_named_by_its_url_not_by_its_track() {
        let dir = std::path::Path::new("/data");
        // The same sleeve from two tracks on the album is one file, which is
        // what stops it being fetched once per track.
        let a = image_path(dir, "https://cdn.example/cover/abc/1000x1000.jpg");
        let b = image_path(dir, "https://cdn.example/cover/abc/1000x1000.jpg");
        assert_eq!(a, b);

        let other = image_path(dir, "https://cdn.example/cover/xyz/1000x1000.jpg");
        assert_ne!(a, other);
        assert_eq!(a.extension().and_then(|e| e.to_str()), Some("jpg"));
    }

    /// A URL with no usable extension still has to name a file.
    #[test]
    fn an_extensionless_url_falls_back_to_jpg() {
        let dir = std::path::Path::new("/data");
        assert_eq!(
            image_path(dir, "https://cdn.example/cover/abc")
                .extension()
                .and_then(|e| e.to_str()),
            Some("jpg")
        );
        // A query string is not part of the extension.
        assert_eq!(
            image_path(dir, "https://cdn.example/a.png?size=xl")
                .extension()
                .and_then(|e| e.to_str()),
            Some("png")
        );
    }

    /// Read from the bytes, not from the name: a mislabelled type renders as a
    /// broken image with nothing on screen to say why.
    #[test]
    fn the_image_type_comes_from_the_content() {
        assert_eq!(image_mime(&[0x89, b'P', b'N', b'G', 0, 0]), "image/png");
        assert_eq!(image_mime(b"GIF89a...."), "image/gif");
        assert_eq!(image_mime(b"RIFF____WEBPVP8 "), "image/webp");
        assert_eq!(image_mime(&[0xFF, 0xD8, 0xFF]), "image/jpeg");
    }

    #[test]
    fn a_query_is_percent_encoded() {
        assert_eq!(encode("Boards of Canada"), "Boards%20of%20Canada");
        assert_eq!(encode("Sigur Rós"), "Sigur%20R%C3%B3s");
        assert_eq!(encode("a&b=c"), "a%26b%3Dc");
    }
}

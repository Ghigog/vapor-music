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
use std::time::Duration;

/// How long a lookup may take before it is abandoned.
///
/// Short on purpose. This is decoration on a screen that is already useful
/// without it, and a person waiting on a spinner for artwork is worse served
/// than one who never sees it.
const TIMEOUT: Duration = Duration::from_secs(8);

/// Sent so the services can identify the client, as the original did.
const USER_AGENT: &str = concat!(
    "VaporMusic/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/Ghigog/vapor-music)"
);

/// One line of time-aligned lyrics.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LyricLine {
    /// Seconds from the start of the track.
    pub time: f32,
    pub text: String,
}

/// The words to a track, however they arrived.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// Whether a lookup has been attempted at all.
    ///
    /// Distinct from every field being empty: a track whose lyrics simply do
    /// not exist on LRCLIB must not be asked for again on every visit to the
    /// screen, and "nothing found" and "not yet looked" are otherwise the same
    /// value.
    #[serde(default)]
    pub attempted: bool,
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

/// The genre named by a Deezer album response, if it names a real one.
pub fn genre_of(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return String::new();
    };
    let name = value
        .get("genres")
        .and_then(|g| g.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|g| g.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("");

    if is_unknown_genre(name) {
        String::new()
    } else {
        name.to_string()
    }
}

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
    } else if bytes.starts_with(&[b'G', b'I', b'F']) {
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

/// A client for the two services.
pub struct Lookup {
    client: reqwest::blocking::Client,
}

impl Lookup {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Lookup { client })
    }

    fn get(&self, url: &str) -> Option<String> {
        let response = self.client.get(url).send().ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.text().ok()
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

    /// Album art, and the genre the same response carries.
    ///
    /// One request for both: the original made two, and the album search
    /// response already names the genre it was going back for.
    pub fn album(&self, artist: &str, album: &str) -> (String, String) {
        if !is_searchable(album) {
            return (String::new(), String::new());
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
                return (art, genre_of(&body));
            }
        }
        (String::new(), String::new())
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

        let response = self.client.get(url).send().ok()?;
        if !response.status().is_success() {
            return None;
        }
        let bytes = response.bytes().ok()?;
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
    use super::*;

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

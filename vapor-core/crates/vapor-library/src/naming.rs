//! Deriving artist/album/title from file paths.
//!
//! Port of the parsing half of `metadata_service.gd`. The network lookups and
//! the cache stay in the shell; this is the part that turns
//! `Björk/[A] Homogenic [8579…] (1997)/03 - Jóga.flac` into something a person
//! wants to read.
//!
//! Worth keeping rather than rewriting: the cleaning rules encode a lot of
//! hard-won specifics about how release dumps are named, and each one exists
//! because something looked wrong in the library.

use regex::Regex;
use std::sync::OnceLock;

/// Bracket contents carrying real musical meaning, which must survive cleaning.
fn keep_qualifier() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)remix|\bmix\b|edit|vip|live|acoustic|instrumental|bootleg|feat|ft\.")
            .expect("valid")
    })
}

/// Parenthesised contents that are pure release-dump noise. Anything else in
/// parens — "(Xilent Remix)", "(What's the Story)" — is kept.
fn paren_noise() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(dolby\s*atmos|atmos|hi[\s-]?res|lossless|flac|alac|wav|aac|mp3|web|cd|vinyl|explicit|clean|remaster(?:ed)?|deluxe(?:\s+edition)?|\d{5,})\s*$",
        )
        .expect("valid")
    })
}

fn year_group() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[\[(]\s*((?:19|20)\d{2})\s*[\])]").expect("valid"))
}

fn bracket_group() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\[([^\]]*)\]").expect("valid"))
}

fn paren_group() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\(([^)]*)\)").expect("valid"))
}

fn trailing_noise() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)[\s\-·]+(dolby\s*atmos|hi[\s-]?res|lossless)\s*$").expect("valid")
    })
}

fn track_number_prefix() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*\d{1,3}\s*[.\-_]?\s+(\S.*)$").expect("valid"))
}

fn space_collapse() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\s{2,}").expect("valid"))
}

pub struct Cleaned {
    pub text: String,
    /// Year found in the segment, if any.
    pub year: Option<u32>,
}

/// Strip release-dump noise from one path segment.
///
/// `"Homogenic [A] [85792657826873562] (1997) [Dolby Atmos]"` → `"Homogenic"`,
/// year 1997.
///
/// A segment containing no noise is returned untouched, including its
/// punctuation — a stylised title like `"-3.AM.-"` must not be tidied into
/// something else.
pub fn clean_segment(seg: &str) -> Cleaned {
    let mut text = seg.to_string();
    let mut year = None;

    if let Some(m) = year_group().captures(&text) {
        year = m.get(1).and_then(|y| y.as_str().parse().ok());
        let whole = m
            .get(0)
            .expect("group 0 always present")
            .as_str()
            .to_string();
        text = text.replace(&whole, " ");
    }

    // Brackets go unless they carry musical meaning.
    for caps in bracket_group().captures_iter(&text.clone()) {
        let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if !keep_qualifier().is_match(inner) {
            let whole = caps.get(0).expect("group 0").as_str();
            text = text.replace(whole, " ");
        }
    }
    // Parens go only when they are known noise.
    for caps in paren_group().captures_iter(&text.clone()) {
        let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if paren_noise().is_match(inner) {
            let whole = caps.get(0).expect("group 0").as_str();
            text = text.replace(whole, " ");
        }
    }
    text = trailing_noise().replace_all(&text, "").to_string();

    // Only tidy separators when something was actually removed.
    if text == seg {
        return Cleaned {
            text: seg.to_string(),
            year: None,
        };
    }

    text = space_collapse().replace_all(&text, " ").trim().to_string();
    while text.ends_with('-') || text.ends_with('·') || text.ends_with(',') {
        text.pop();
        text = text.trim().to_string();
    }

    Cleaned { text, year }
}

/// Clean a segment, but never empty it.
///
/// If stripping noise leaves nothing — the whole name *was* the junk — the
/// original is kept. A junk name is more honest than a blank one.
pub fn clean_keep_nonempty(seg: &str) -> Cleaned {
    if seg.is_empty() {
        return Cleaned {
            text: String::new(),
            year: None,
        };
    }
    let c = clean_segment(seg);
    if c.text.is_empty() {
        Cleaned {
            text: seg.to_string(),
            year: c.year,
        }
    } else {
        c
    }
}

/// Strip a leading track number from a title.
///
/// `"03 - Jóga"` → `"Jóga"`. Titles that *are* numbers ("1985") have no
/// remainder and are left alone; "24K Magic" does not match because the digits
/// are not followed by a separator.
pub fn strip_track_number(title: &str) -> String {
    match track_number_prefix().captures(title) {
        Some(c) => c
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| title.to_string()),
        None => title.to_string(),
    }
}

/// True when a segment is only a track number, so it can be dropped from a
/// `" - "`-separated filename.
pub fn is_track_number(seg: &str) -> bool {
    let t = seg.trim();
    !t.is_empty() && t.len() <= 3 && t.chars().all(|c| c.is_ascii_digit())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TrackInfo {
    pub artist: String,
    pub album: String,
    pub title: String,
    pub year: Option<u32>,
}

/// Derive artist, album and title from a path.
///
/// Two sources, filename first: a structured `"Artist - Album - Title"`
/// filename is more reliable than folder position, which varies by how the
/// collection was ripped. Folders fill whatever the filename did not supply.
///
/// `base_folder` is the library root ("Music"); segments before it are the
/// server's own path and carry no meaning.
pub fn parse_path(path: &str, base_folder: &str) -> TrackInfo {
    let decoded = percent_decode(path);
    let segments: Vec<&str> = decoded.split('/').filter(|s| !s.is_empty()).collect();

    let filename = segments.last().copied().unwrap_or("");
    let stem = filename
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(filename);

    // Folders between the library root and the file.
    //
    // `base_folder` is a path, not a name: the shell passes
    // `settings.remote.folder`, which is what the server calls the library —
    // "/dav/Koofr/Music". This used to compare it against one segment at a
    // time, so a multi-segment base never matched and the fallback below
    // treated the server's own path components as meaningful, turning
    // "/dav/Koofr/Music/track.m4a" into artist "dav", album "Koofr".
    //
    // Matched as a run of segments so that either form works, since a
    // single-segment base is still a run of one.
    let base_segments: Vec<&str> = base_folder.split('/').filter(|s| !s.is_empty()).collect();
    let start = if base_segments.is_empty() || base_segments.len() > segments.len() {
        None
    } else {
        segments
            .windows(base_segments.len())
            .position(|window| {
                window
                    .iter()
                    .zip(&base_segments)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
            })
            .map(|i| i + base_segments.len())
    };
    let relative: Vec<&str> = match start {
        Some(i) if i < segments.len() => segments[i..segments.len() - 1].to_vec(),
        _ if segments.len() >= 2 => segments[..segments.len() - 1].to_vec(),
        _ => Vec::new(),
    };

    let (mut folder_artist, mut folder_album) = (String::new(), String::new());
    if relative.len() >= 2 {
        folder_artist = relative[0].trim().to_string();
        folder_album = relative[1].trim().to_string();
    } else if relative.len() == 1 {
        let seg = normalise_dashes(relative[0].trim());
        match seg.split(" - ").collect::<Vec<_>>()[..] {
            [a, b] => {
                folder_artist = a.trim().to_string();
                folder_album = b.trim().to_string();
            }
            _ => folder_album = relative[0].trim().to_string(),
        }
    }

    // Structured filename.
    let (mut file_artist, mut file_album) = (String::new(), String::new());
    let mut file_title = stem.to_string();
    let display = normalise_dashes(stem);
    if display.contains(" - ") {
        let mut parts: Vec<&str> = display.split(" - ").collect();
        if parts.len() > 1 && is_track_number(parts[0]) {
            parts.remove(0);
        }
        match parts.len() {
            0 => {}
            1 => file_title = parts[0].trim().to_string(),
            2 => {
                file_artist = parts[0].trim().to_string();
                file_title = parts[1].trim().to_string();
            }
            _ => {
                file_artist = parts[0].trim().to_string();
                file_album = parts[1].trim().to_string();
                file_title = parts[2..].join(" - ").trim().to_string();
            }
        }
    }

    let mut year = None;
    let mut take = |s: &str| -> String {
        let c = clean_keep_nonempty(s);
        if year.is_none() {
            year = c.year;
        }
        c.text
    };

    let artist = take(if !file_artist.is_empty() {
        &file_artist
    } else {
        &folder_artist
    });
    let album = take(if !file_album.is_empty() {
        &file_album
    } else {
        &folder_album
    });
    let title = strip_track_number(&take(&file_title));

    TrackInfo {
        artist,
        album,
        title,
        year,
    }
}

/// En and em dashes are used interchangeably with hyphens in release names.
fn normalise_dashes(s: &str) -> String {
    s.replace(['\u{2013}', '\u{2014}'], "-")
}

/// Minimal percent-decoding — hrefs come URL-encoded from WebDAV.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_release_dump_noise_and_finds_the_year() {
        let c = clean_segment("Homogenic [A] [85792657826873562] (1997) [Dolby Atmos]");
        assert_eq!(c.text, "Homogenic");
        assert_eq!(c.year, Some(1997));
    }

    /// Remix and live markers are the reason cleaning is selective rather than
    /// stripping every bracket.
    #[test]
    fn musical_qualifiers_survive() {
        assert_eq!(
            clean_segment("Track [Xilent Remix]").text,
            "Track [Xilent Remix]"
        );
        assert_eq!(clean_segment("Song (Live)").text, "Song (Live)");
        assert_eq!(
            clean_segment("Song (What's the Story)").text,
            "Song (What's the Story)",
            "unknown paren content is kept"
        );
    }

    /// A name with no noise must pass through byte-for-byte, punctuation and
    /// all — tidying it would corrupt stylised titles.
    #[test]
    fn clean_names_are_untouched() {
        for s in ["-3.AM.-", "Song", "A - B", "Track...", "!!!"] {
            assert_eq!(clean_segment(s).text, s, "{s} was altered");
        }
    }

    /// A name that is entirely junk keeps its original rather than becoming
    /// blank.
    #[test]
    fn cleaning_never_empties_a_segment() {
        let c = clean_keep_nonempty("[Dolby Atmos]");
        assert!(!c.text.is_empty(), "a junk name beats a blank one");
    }

    #[test]
    fn strips_track_numbers_without_eating_titles() {
        assert_eq!(strip_track_number("03 - Jóga"), "Jóga");
        assert_eq!(strip_track_number("01 Song"), "Song");
        assert_eq!(
            strip_track_number("07.Track"),
            "07.Track",
            "no separator space"
        );
        assert_eq!(
            strip_track_number("1985"),
            "1985",
            "a title that is a number"
        );
        assert_eq!(strip_track_number("24K Magic"), "24K Magic");
    }

    #[test]
    fn parses_artist_album_title_from_folders() {
        let info = parse_path(
            "/dav/Music/Bj%C3%B6rk/Homogenic/03%20-%20J%C3%B3ga.flac",
            "Music",
        );
        assert_eq!(info.artist, "Björk");
        assert_eq!(info.album, "Homogenic");
        assert_eq!(info.title, "Jóga");
    }

    /// A structured filename should win over folder position, which varies by
    /// how a collection was ripped.
    #[test]
    fn a_structured_filename_beats_folder_position() {
        let info = parse_path("/dav/Music/Misc/Aphex Twin - SAW - Xtal.mp3", "Music");
        assert_eq!(info.artist, "Aphex Twin");
        assert_eq!(info.album, "SAW");
        assert_eq!(info.title, "Xtal");
    }

    #[test]
    fn a_leading_track_number_is_dropped_from_a_structured_filename() {
        let info = parse_path("/dav/Music/X/Y/01 - Artist - Album - Title.mp3", "Music");
        assert_eq!(info.artist, "Artist");
        assert_eq!(info.album, "Album");
        assert_eq!(info.title, "Title");
    }

    #[test]
    fn a_single_folder_splits_on_a_dash() {
        let info = parse_path(
            "/dav/Music/Daft Punk - Discovery/One More Time.mp3",
            "Music",
        );
        assert_eq!(info.artist, "Daft Punk");
        assert_eq!(info.album, "Discovery");
    }

    #[test]
    fn en_and_em_dashes_are_treated_as_hyphens() {
        let info = parse_path("/dav/Music/X/Y/Artist \u{2013} Title.mp3", "Music");
        assert_eq!(info.artist, "Artist");
        assert_eq!(info.title, "Title");
    }

    #[test]
    fn the_year_is_carried_out_of_a_noisy_album_folder() {
        let info = parse_path("/dav/Music/Björk/Homogenic (1997)/Jóga.flac", "Music");
        assert_eq!(info.album, "Homogenic");
        assert_eq!(info.year, Some(1997));
    }

    #[test]
    fn a_bare_filename_still_yields_a_title() {
        let info = parse_path("/song.mp3", "Music");
        assert_eq!(info.title, "song");
        assert!(info.artist.is_empty());
    }

    /// The form the app actually passes.
    ///
    /// Every test here used `"Music"` — a single segment — while the shell
    /// passes `settings.remote.folder`, which is a full server path like
    /// `/dav/Koofr/Music`. The base was located with
    /// `segments.iter().position(|s| s == base_folder)`, comparing one path
    /// segment against the whole path, so it never matched and the fallback
    /// treated *every* folder as meaningful: `/dav/Koofr/Music/track.m4a`
    /// became artist "dav", album "Koofr".
    ///
    /// The album mattered twice over. `apply_tags` fills gaps only, so a row
    /// carrying the album "Koofr" was not a gap, and the real album from the
    /// file's own tags was never applied.
    #[test]
    fn a_base_folder_given_as_a_full_path_is_stripped() {
        let info = parse_path(
            "/dav/Koofr/Music/37 - The Glitch Mob - A Dream Within A Dream.m4a",
            "/dav/Koofr/Music",
        );

        assert_eq!(info.artist, "The Glitch Mob");
        assert_eq!(info.title, "A Dream Within A Dream");
        assert_eq!(
            info.album, "",
            "a track sitting directly in the library root has no album, and \
             saying it has one blocks the file's own tags from supplying the \
             real one"
        );
    }

    #[test]
    fn folders_below_a_full_base_path_still_give_artist_and_album() {
        let info = parse_path(
            "/dav/Koofr/Music/Boards of Canada/Music Has the Right/03 - Roygbiv.mp3",
            "/dav/Koofr/Music",
        );

        assert_eq!(info.artist, "Boards of Canada");
        assert_eq!(info.album, "Music Has the Right");
        assert_eq!(info.title, "Roygbiv");
    }
}

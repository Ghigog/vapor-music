//! WebDAV response parsing.
//!
//! Port of the parsing half of `webdav_service.gd`. The HTTP transport is not
//! here: the GDScript hand-rolled a TCP client with its own chunked-transfer
//! decoder, header splitting and terminal-chunk detection, all of which a real
//! HTTP client does correctly and none of which is this crate's business.
//! What remains is the part that is actually ours — turning a PROPFIND body
//! into a track list.

/// Extensions the library treats as audio.
///
/// `.m4a` is here because it is 58% of a real library; the Godot playback
/// fallback omitting it is what made the non-macOS builds unable to play most
/// of a collection.
pub const AUDIO_EXTENSIONS: [&str; 5] = ["mp3", "flac", "ogg", "wav", "m4a"];

pub fn is_audio_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    AUDIO_EXTENSIONS
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
}

/// Strip scheme and host, leaving an absolute path.
///
/// PROPFIND responses may return either absolute URLs or bare paths depending
/// on the server, so both have to collapse to the same key — the href is what
/// the cache and playlists are keyed on.
fn strip_origin(raw: &str) -> String {
    match raw.split_once("://") {
        Some((_, rest)) => match rest.find('/') {
            Some(i) => rest[i..].to_string(),
            None => "/".to_string(),
        },
        None => raw.to_string(),
    }
}

/// Extract audio file hrefs from a PROPFIND response body.
///
/// Deliberately a targeted scan for `<d:href>` text rather than a full XML
/// parse: servers disagree on namespace prefixes, and the only thing needed
/// from the document is one repeated element.
pub fn parse_propfind(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = xml.to_lowercase();
    let mut cursor = 0usize;

    while let Some(rel_open) = lower[cursor..]
        .find("<d:href>")
        .or_else(|| lower[cursor..].find("<href>"))
    {
        let open = cursor + rel_open;
        // Length of the tag actually matched at this position.
        let tag_len = if lower[open..].starts_with("<d:href>") {
            "<d:href>".len()
        } else {
            "<href>".len()
        };
        let value_start = open + tag_len;

        let Some(rel_close) = lower[value_start..].find("</") else {
            break;
        };
        let value_end = value_start + rel_close;

        let raw = xml[value_start..value_end].trim();
        if is_audio_path(raw) {
            out.push(strip_origin(raw));
        }
        cursor = value_end;
    }
    out
}

/// Normalise a user-entered base URL to a directory path: leading and trailing
/// slash, no scheme or host.
pub fn normalize_dir(path: &str) -> String {
    let mut clean = strip_origin(path.trim());
    if !clean.starts_with('/') {
        clean.insert(0, '/');
    }
    if !clean.ends_with('/') {
        clean.push('/');
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/dav/Music/</d:href></d:response>
  <d:response><d:href>/dav/Music/a%20song.mp3</d:href></d:response>
  <d:response><d:href>/dav/Music/track.m4a</d:href></d:response>
  <d:response><d:href>/dav/Music/notes.txt</d:href></d:response>
  <d:response><d:href>https://dav.example.com/dav/Music/remote.flac</d:href></d:response>
</d:multistatus>"#;

    #[test]
    fn extracts_only_audio_files() {
        let files = parse_propfind(BODY);
        assert_eq!(
            files,
            vec![
                "/dav/Music/a%20song.mp3",
                "/dav/Music/track.m4a",
                "/dav/Music/remote.flac",
            ],
            "directories and non-audio must be skipped"
        );
    }

    /// Absolute and relative hrefs must collapse to the same key, or the same
    /// track ends up cached twice under different names.
    #[test]
    fn absolute_urls_reduce_to_paths() {
        let files = parse_propfind(
            "<d:href>https://host.example/dav/x.mp3</d:href><d:href>/dav/y.mp3</d:href>",
        );
        assert_eq!(files, vec!["/dav/x.mp3", "/dav/y.mp3"]);
    }

    /// Servers differ on namespace prefixes; both spellings must work.
    #[test]
    fn handles_unprefixed_href_elements() {
        let files = parse_propfind("<href>/dav/x.mp3</href>");
        assert_eq!(files, vec!["/dav/x.mp3"]);
    }

    #[test]
    fn recognises_every_supported_extension() {
        for ext in AUDIO_EXTENSIONS {
            assert!(
                is_audio_path(&format!("/a/b.{ext}")),
                "{ext} not recognised"
            );
            assert!(
                is_audio_path(&format!("/a/b.{}", ext.to_uppercase())),
                "{ext} should be case-insensitive"
            );
        }
        assert!(!is_audio_path("/a/b.txt"));
        assert!(!is_audio_path("/a/mp3-folder/"));
    }

    #[test]
    fn empty_or_malformed_bodies_yield_nothing() {
        assert!(parse_propfind("").is_empty());
        assert!(parse_propfind("<d:href>/unterminated.mp3").is_empty());
        assert!(parse_propfind("not xml at all").is_empty());
    }

    #[test]
    fn normalises_directory_paths() {
        assert_eq!(normalize_dir("Music"), "/Music/");
        assert_eq!(normalize_dir("/Music"), "/Music/");
        assert_eq!(normalize_dir("/Music/"), "/Music/");
        assert_eq!(
            normalize_dir("https://app.koofr.net/dav/Koofr"),
            "/dav/Koofr/"
        );
    }
}

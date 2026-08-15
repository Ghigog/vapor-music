//! Embedded metadata: cover art, and the tags a filename cannot carry.
//!
//! Everything the library knows today is derived from the *path* — see
//! `vapor_library::parse_path` — which works well on a well-organised drive and
//! not at all on a badly named one. It also cannot carry artwork, a year, a
//! genre, or the sleeve notes a person actually wants on Liner Notes.
//!
//! This reads what the file itself says (TD-39, TD-41b).
//!
//! ## Why it reads the cached copy
//!
//! Tags live at the head of a file, so in principle a ranged request would do.
//! In practice WebDAV servers vary in whether they honour `Range`, ID3v1 sits
//! at the *end*, and the app already downloads a track before it can play or
//! analyse it. So this reads whatever is in the cache and says nothing about
//! what is not — no extra network path, and no second thing that can fail.
//!
//! ## Path still wins for identity
//!
//! Tags fill gaps; they do not overrule. A library filed as
//! `Artist/Album/Track` is a deliberate statement about how it should be
//! organised, and a track whose tag disagrees is usually the tag being wrong —
//! a compilation track carrying its original album, say. So the caller prefers
//! what it already derived and takes a tag only where it had nothing.

use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;

/// What a file's own tags say about it.
#[derive(Clone, Debug, Default)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    /// Free text. The closest thing to the design's written notes that a file
    /// actually carries.
    pub comment: Option<String>,
    /// Cover art as a data URI, ready for `<img src>`.
    ///
    /// A data URI rather than a file path because the webview cannot read the
    /// app's data directory without punching a hole in the CSP, and artwork is
    /// small enough that encoding it is cheaper than a second asset protocol.
    pub cover: Option<String>,
}

impl Tags {
    /// True when the file said nothing, so a caller can tell "read it and it
    /// was bare" from "have not read it".
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.artist.is_none()
            && self.album.is_none()
            && self.genre.is_none()
            && self.year.is_none()
            && self.comment.is_none()
            && self.cover.is_none()
    }
}

/// Largest artwork encoded and handed to the webview.
///
/// Some releases embed a 3000×3000 sleeve; base64 inflates by a third, and a
/// table of a hundred rows would then move tens of megabytes through IPC to
/// draw thumbnails. Anything larger is dropped rather than scaled, because
/// scaling means an image decoder and this is a metadata reader.
const MAX_COVER_BYTES: usize = 2 * 1024 * 1024;

/// Read tags and artwork from a local file.
///
/// Never fails loudly: no tags, a format lofty cannot parse, and a file that is
/// not there are all "nothing to say" — which is how the caller has to treat
/// each of them anyway.
pub fn read(path: &Path) -> Tags {
    let Ok(tagged) = Probe::open(path).and_then(|p| p.read()) else {
        return Tags::default();
    };
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return Tags::default();
    };

    let text = |value: Option<std::borrow::Cow<'_, str>>| -> Option<String> {
        let value = value?.trim().to_string();
        // An empty tag is not a value. Writing "" into a row would read as a
        // known-blank field rather than an absent one.
        (!value.is_empty()).then_some(value)
    };

    Tags {
        title: text(tag.title()),
        artist: text(tag.artist()),
        album: text(tag.album()),
        genre: text(tag.genre()),
        // A tag year of 0 or 9999 is a broken tagger, not a release date.
        //
        // `Year` rather than `RecordingDate`: the latter is a full date in some
        // formats and a bare year in others, and only the year is displayed.
        year: tag
            .get_string(ItemKey::Year)
            .and_then(|y| y.trim().get(..4).and_then(|y| y.parse::<u32>().ok()))
            .filter(|y| (1900..=2200).contains(y)),
        comment: text(tag.comment()).or_else(|| {
            tag.get_string(ItemKey::Description)
                .map(str::to_string)
                .filter(|s| !s.trim().is_empty())
        }),
        cover: tag
            .pictures()
            .iter()
            .find(|p| p.data().len() <= MAX_COVER_BYTES)
            .map(|p| {
                let mime = p
                    .mime_type()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "image/jpeg".to_string());
                format!("data:{mime};base64,{}", base64_encode(p.data()))
            }),
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every failure mode gives the same answer, because the caller can only do
    /// one thing about any of them.
    #[test]
    fn an_unreadable_file_says_nothing_rather_than_failing() {
        assert!(read(Path::new("/nonexistent/vapor-not-a-file.mp3")).is_empty());
    }

    #[test]
    fn a_file_that_is_not_audio_says_nothing() {
        let dir = std::env::temp_dir().join(format!("vapor-tags-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("notes.txt");
        std::fs::write(&path, b"this is not a music file").expect("write");

        assert!(read(&path).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The cover has to arrive in a form an `<img>` can use directly, or the
    /// webview needs a second asset protocol and a hole in the CSP.
    #[test]
    fn a_cover_is_encoded_as_a_usable_data_uri() {
        let uri = format!("data:image/jpeg;base64,{}", base64_encode(&[1, 2, 3, 4]));
        assert!(uri.starts_with("data:image/jpeg;base64,"));
        assert!(
            !uri.contains(' '),
            "a data URI with whitespace will not load"
        );
    }
}

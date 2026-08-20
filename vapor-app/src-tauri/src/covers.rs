//! Cover art on disk.
//!
//! Covers used to live inside `tags.json`, one base64 data URI per track. That
//! file reached 155 MB on a 563-track library, and every one of those bytes was
//! parsed into memory at startup and held there for the life of the process —
//! on a phone, next to a 256 MB Java heap. The tags themselves are titles and
//! genres: a few hundred kilobytes. It was artwork, entirely.
//!
//! So artwork moved out. `tags.json` holds text, and a cover is a file named by
//! a hash of its href, read only when something is about to draw it.
//!
//! ## Stored as the data URI, not as decoded bytes
//!
//! What the webview wants is `data:image/jpeg;base64,...`, so that is what is
//! on disk. Decoding to raw bytes would save a third of the space and cost a
//! re-encode on every read, on the path that draws the now-playing tile. Disk
//! was never the problem being solved here.

use std::fs;
use std::path::{Path, PathBuf};

/// The cover directory.
pub struct Covers {
    dir: PathBuf,
}

impl Covers {
    pub fn new(dir: PathBuf) -> Self {
        Covers { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path_for(&self, href: &str) -> PathBuf {
        self.dir.join(format!("{:016x}.txt", hash(href)))
    }

    /// The cover for a track, or `None` if it has none.
    ///
    /// A read error reads as "no cover": the caller is drawing a tile, and a
    /// missing image is the same picture as an unreadable one.
    pub fn get(&self, href: &str) -> Option<String> {
        fs::read_to_string(self.path_for(href)).ok()
    }

    /// Write a cover, replacing any existing one.
    ///
    /// Atomic, like every other write in the app: a torn cover file would be
    /// served to the webview as a truncated data URI, which renders as a broken
    /// image rather than as no image.
    pub fn put(&self, href: &str, cover: &str) -> Result<(), std::io::Error> {
        fs::create_dir_all(&self.dir)?;
        let path = self.path_for(href);
        // A unique temporary name, not a fixed one: the analysis pool writes
        // covers from several threads, and two of them landing on the same
        // href would otherwise rename each other's half-written file.
        let tmp = path.with_extension(format!("{}.tmp", next_attempt()));
        fs::write(&tmp, cover)?;
        fs::rename(&tmp, &path)?;

        /*
         * And the thumbnail, now rather than when a row first asks for one.
         *
         * Covers are written by the analysis pass, which is already a plain
         * background thread doing seconds of work per track — 38 ms more to
         * shrink one is not felt there. A row asking for it later is a person
         * waiting: on a library with no thumbnails yet, the first open of Songs
         * generates a screenful before it can draw, which is the pause this was
         * meant to remove (PERF-004).
         *
         * Best effort. A thumbnail that cannot be written is regenerated on
         * demand, which is the old behaviour and still correct.
         */
        if let Some(small) = shrink(cover) {
            let thumb = self.thumb_path_for(href);
            let tmp = thumb.with_extension(format!("{}.tmp", next_attempt()));
            if fs::write(&tmp, &small).is_ok() {
                let _ = fs::rename(&tmp, &thumb);
            }
        }
        Ok(())
    }

    /// A small square version of the cover, for a table row (PERF-004).
    ///
    /// ## Why this exists
    ///
    /// Row artwork draws at 48 px and was being handed the full stored cover.
    /// Measured on the 563-track library: 516 covers, 149 MB, median 281 KB
    /// each and the largest 954 KB. A screenful is about 26 rows, so opening
    /// Songs pushed ~7 MB — and up to ~23 MB on a run of large ones — through
    /// IPC, JSON and image decoding, to draw 26 postage stamps. That is the
    /// second-long freeze.
    ///
    /// A 128 px JPEG covers the largest of those slots on a 2× display and runs
    /// about 6 KB, so the same screenful costs roughly 160 KB instead of 7 MB.
    ///
    /// Generated once and kept beside the full cover. Returns `None` when there
    /// is no cover, and falls back to the full one when the stored bytes cannot
    /// be decoded — a cover this cannot shrink is still a cover, and a row with
    /// artwork missing for a reason nobody can see is worse than a slow row.
    pub fn thumb(&self, href: &str) -> Option<String> {
        let path = self.thumb_path_for(href);
        if let Ok(existing) = fs::read_to_string(&path) {
            return Some(existing);
        }

        let full = self.get(href)?;
        let Some(small) = shrink(&full) else {
            return Some(full);
        };

        // Best-effort: a thumbnail that cannot be written is regenerated next
        // time, which is slower and still correct.
        let _ = fs::create_dir_all(&self.dir);
        let tmp = path.with_extension(format!("{}.tmp", next_attempt()));
        if fs::write(&tmp, &small).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
        Some(small)
    }

    fn thumb_path_for(&self, href: &str) -> PathBuf {
        self.dir.join(format!("{:016x}.thumb.txt", hash(href)))
    }

    /// Total bytes held, for the Your Data screen.
    pub fn size(&self) -> u64 {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return 0;
        };
        entries
            .flatten()
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum()
    }
}

/// Longest edge of a small-tile thumbnail, in pixels.
///
/// The two places that draw one are a song row at 48 px and the queue's
/// now-playing tile at 56 px, so this is the 2× version of the larger and
/// covers every display the app runs on. Larger buys nothing either can show.
const THUMB_PX: u32 = 128;

/// JPEG quality for thumbnails. At 96 px the difference above this is bytes,
/// not pixels anyone can see.
const THUMB_QUALITY: u8 = 78;

/// Shrink a `data:` URI to a [`THUMB_PX`] JPEG one.
///
/// `None` when the input is not a data URI this can decode, which the caller
/// treats as "serve the original" rather than as a failure.
fn shrink(data_uri: &str) -> Option<String> {
    use base64::Engine as _;

    let encoded = data_uri.split(";base64,").nth(1)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?;

    // `thumbnail` rather than `resize`: it is a box filter with a fast path and
    // the result is 96 px square. Nothing here is doing photography.
    let small = decoded.thumbnail(THUMB_PX, THUMB_PX);

    let mut out = Vec::new();
    small
        .to_rgb8()
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut out,
            THUMB_QUALITY,
        ))
        .ok()?;

    Some(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&out)
    ))
}

/// Process-wide and monotonic, so two threads never share a temporary file.
fn next_attempt() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

/// FNV-1a, the same scheme the audio cache names its files with.
fn hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers() -> (Covers, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "vapor-covers-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        (Covers::new(dir.clone()), dir)
    }

    /// A real JPEG, big enough that shrinking it has something to do.
    fn a_big_cover() -> String {
        use base64::Engine as _;
        let img = image::RgbImage::from_fn(900, 900, |x, y| {
            // Noise, not flat colour: a solid square compresses to nothing and
            // would make any size assertion meaningless.
            let v = ((x * 7 + y * 13) % 251) as u8;
            image::Rgb([v, v.wrapping_mul(3), v.wrapping_add(97)])
        });
        let mut bytes = Vec::new();
        img.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut bytes, 92,
        ))
        .expect("encode");
        format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        )
    }

    /// The point of the whole change (PERF-004): a row asks for artwork and
    /// gets something a row can afford.
    #[test]
    fn a_thumbnail_is_far_smaller_than_the_cover_it_came_from() {
        let (c, dir) = covers();
        let full = a_big_cover();
        c.put("/music/big.mp3", &full).unwrap();

        let thumb = c.thumb("/music/big.mp3").expect("a thumbnail");
        assert!(
            thumb.starts_with("data:image/jpeg;base64,"),
            "not a data URI: {}",
            &thumb[..thumb.len().min(40)]
        );
        assert!(
            thumb.len() * 10 < full.len(),
            "thumbnail {} bytes against a {} byte cover — not worth the trip",
            thumb.len(),
            full.len()
        );

        // Cached, so the second row to ask does not re-encode.
        assert!(dir.join(format!("{:016x}.thumb.txt", hash("/music/big.mp3"))).exists());
        assert_eq!(c.thumb("/music/big.mp3").as_deref(), Some(thumb.as_str()));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A track with no cover has no thumbnail — not an empty one.
    /// Writing a cover writes its thumbnail too (PERF-004).
    ///
    /// Otherwise the first person to open Songs on a fresh library generates a
    /// screenful of them before anything can be drawn, which is the pause the
    /// thumbnails exist to remove.
    #[test]
    fn storing_a_cover_also_stores_its_thumbnail() {
        let (c, dir) = covers();
        c.put("/music/big.mp3", &a_big_cover()).unwrap();

        let thumb = dir.join(format!("{:016x}.thumb.txt", hash("/music/big.mp3")));
        assert!(
            thumb.exists(),
            "no thumbnail beside the cover; a row would have to wait for one",
        );
        // And it is the small one, not a copy of the original.
        let written = std::fs::read_to_string(&thumb).unwrap();
        let full = c.get("/music/big.mp3").unwrap();
        assert!(written.len() * 10 < full.len(), "the 'thumbnail' is full size");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn no_cover_means_no_thumbnail() {
        let (c, dir) = covers();
        assert_eq!(c.thumb("/music/absent.mp3"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Bytes that are not an image still draw something.
    ///
    /// A cover this cannot shrink is still a cover, and a row silently missing
    /// its artwork for a reason nobody can see is worse than a slow row.
    #[test]
    fn an_undecodable_cover_falls_back_to_the_original() {
        let (c, dir) = covers();
        c.put("/music/odd.mp3", "data:image/jpeg;base64,AAAA").unwrap();
        assert_eq!(
            c.thumb("/music/odd.mp3").as_deref(),
            Some("data:image/jpeg;base64,AAAA")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cover_survives_a_round_trip() {
        let (c, dir) = covers();
        assert_eq!(c.get("/music/a.mp3"), None);

        c.put("/music/a.mp3", "data:image/jpeg;base64,AAAA")
            .unwrap();
        assert_eq!(
            c.get("/music/a.mp3").as_deref(),
            Some("data:image/jpeg;base64,AAAA")
        );
        // One track's cover is not another's.
        assert_eq!(c.get("/music/b.mp3"), None);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn replacing_a_cover_leaves_one_file() {
        let (c, dir) = covers();
        c.put("/music/a.mp3", "first").unwrap();
        c.put("/music/a.mp3", "second").unwrap();
        assert_eq!(c.get("/music/a.mp3").as_deref(), Some("second"));
        // The temporary file is renamed over the target, not left beside it.
        let files = fs::read_dir(&dir).unwrap().count();
        assert_eq!(files, 1, "a stray temporary file was left behind");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn size_counts_what_is_there() {
        let (c, dir) = covers();
        assert_eq!(c.size(), 0, "a directory that does not exist holds nothing");
        c.put("/music/a.mp3", "0123456789").unwrap();
        c.put("/music/b.mp3", "01234").unwrap();
        assert_eq!(c.size(), 15);
        let _ = fs::remove_dir_all(dir);
    }
}

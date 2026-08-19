//! Playing a track while it is still arriving.
//!
//! ## Why this exists
//!
//! Playback used to need a complete local file: the cache downloads to a
//! temporary and renames, so a track became playable only once every byte of it
//! had landed. That was invisible while the analysis pass was fetching the
//! whole library anyway — everything was already local. Once analysis stopped
//! keeping what it read, every play would have begun with a wait for ten
//! megabytes.
//!
//! ## Why ranges, rather than a pipe
//!
//! The obvious answer is to decode from the front while the rest downloads.
//! That fails on two counts here.
//!
//! **The container.** 324 of the 563 tracks in the library this was written
//! against are `.m4a`, and an MP4's `moov` atom — the sample tables, without
//! which not one frame can be decoded — sits at the *end* unless the file was
//! written for streaming. A front-to-back pipe stalls waiting for bytes that
//! come last.
//!
//! **The mix.** Arming a transition opens the incoming track at its *cue
//! point*, which is usually minutes in, precisely so the whole track need not be
//! decoded first. A pipe cannot answer that at all.
//!
//! Both are the same request: give me these bytes, from here. So this is a
//! `MediaSource` that fetches byte ranges on demand and remembers what it has
//! fetched.
//!
//! ## When the server will not play along
//!
//! Range support is not assumed. The first request asks for one byte; a server
//! that answers `206` supports it and a server that answers `200` does not. In
//! the second case this reports itself as not seekable and falls back to
//! holding the whole body, which is exactly the behaviour there was before —
//! slower to start, still correct.

use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use vapor_dsp::MediaSource;

/// How much is fetched per request.
///
/// Small enough that starting a track does not wait on a megabyte, large enough
/// that a track is not hundreds of requests. Symphonia reads in small bites and
/// this is the unit underneath them.
const CHUNK: u64 = 256 * 1024;

/// Fetches byte ranges of one remote object, and keeps what it fetched.
pub struct RemoteSource {
    fetch: Arc<dyn RangeFetch>,
    /// What is known about the object's size, once the server has said.
    len: Option<u64>,
    /// Whether the server answered a range request with a range.
    seekable: bool,
    /// Where the reader is.
    pos: u64,
    /// What has been fetched, as (offset, bytes). Shared so a second source
    /// over the same track — the mix opens one while playback holds another —
    /// does not fetch it all again.
    have: Arc<Mutex<Chunks>>,
}

/// The bytes held so far, in whatever pieces they arrived.
#[derive(Default)]
pub struct Chunks {
    pieces: Vec<(u64, Vec<u8>)>,
}

impl Chunks {
    /// Copy out of what is held, from `at`, as much as `buf` will take.
    /// Returns how much was filled — zero when the offset is not held.
    fn read_at(&self, at: u64, buf: &mut [u8]) -> usize {
        for (start, bytes) in &self.pieces {
            let end = start + bytes.len() as u64;
            if at >= *start && at < end {
                let offset = (at - start) as usize;
                let n = buf.len().min(bytes.len() - offset);
                buf[..n].copy_from_slice(&bytes[offset..offset + n]);
                return n;
            }
        }
        0
    }

    fn insert(&mut self, at: u64, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        // Kept simple: pieces are whole chunks at chunk boundaries, so overlap
        // means the same chunk twice and the copy already held is as good.
        if self.pieces.iter().any(|(s, _)| *s == at) {
            return;
        }
        self.pieces.push((at, bytes));
    }
}

/// Somewhere bytes can be asked for by range.
///
/// A trait so the tests can answer without a network, and so the shell's WebDAV
/// client is not dragged into this module.
pub trait RangeFetch: Send + Sync {
    /// Fetch `len` bytes from `at`. Returns the bytes and, when the server said
    /// so, the total size of the object.
    fn range(&self, at: u64, len: u64) -> Result<(Vec<u8>, Option<u64>), String>;

    /// Whether the server answers ranges with ranges. Asked once.
    fn supports_ranges(&self) -> bool;

    /// The whole object, for a server that will not do ranges.
    fn whole(&self) -> Result<Vec<u8>, String>;
}

impl RemoteSource {
    pub fn new(fetch: Arc<dyn RangeFetch>, have: Arc<Mutex<Chunks>>) -> Self {
        let seekable = fetch.supports_ranges();
        RemoteSource {
            fetch,
            len: None,
            seekable,
            pos: 0,
            have,
        }
    }

    /// Make sure `self.pos` is covered, fetching the chunk it falls in.
    fn ensure(&mut self) -> std::io::Result<()> {
        {
            let have = self.have.lock().map_err(poisoned)?;
            let mut probe = [0u8; 1];
            if have.read_at(self.pos, &mut probe) == 1 {
                return Ok(());
            }
        }

        // Aligned to chunk boundaries so repeated reads of the same region ask
        // for the same range, and the piece list stays a set of whole chunks.
        let start = (self.pos / CHUNK) * CHUNK;
        let (bytes, total) = if self.seekable {
            self.fetch
                .range(start, CHUNK)
                .map_err(std::io::Error::other)?
        } else {
            // No ranges: take the lot, once. Slower to start and no worse than
            // what playback did before this existed.
            let all = self.fetch.whole().map_err(std::io::Error::other)?;
            let len = all.len() as u64;
            (all, Some(len))
        };

        if let Some(total) = total {
            self.len = Some(total);
        }
        let at = if self.seekable { start } else { 0 };
        self.have.lock().map_err(poisoned)?.insert(at, bytes);
        Ok(())
    }
}

fn poisoned<T>(_: T) -> std::io::Error {
    std::io::Error::other("the byte cache lock was poisoned")
}

impl Read for RemoteSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.len.is_some_and(|len| self.pos >= len) {
            return Ok(0);
        }

        self.ensure()?;
        let have = self.have.lock().map_err(poisoned)?;
        let n = have.read_at(self.pos, buf);
        drop(have);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for RemoteSource {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let at = match to {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::Current(d) => self.pos as i64 + d,
            SeekFrom::End(d) => match self.len {
                Some(len) => len as i64 + d,
                // The size is only known once the server has answered
                // something. Asking for the end before then is the MP4 probe
                // looking for `moov`, so fetch a byte to learn the length.
                None => {
                    self.ensure()?;
                    match self.len {
                        Some(len) => len as i64 + d,
                        None => {
                            return Err(std::io::Error::other("the server did not give a length"))
                        }
                    }
                }
            },
        };
        if at < 0 {
            return Err(std::io::Error::other("seek before the start"));
        }
        self.pos = at as u64;
        Ok(self.pos)
    }
}

impl MediaSource for RemoteSource {
    fn is_seekable(&self) -> bool {
        self.seekable
    }

    fn byte_len(&self) -> Option<u64> {
        self.len
    }
}

/// One track on the WebDAV server, fetched by range.
///
/// `supports_ranges` is asked once, when this is built: the answer is about the
/// server rather than the track, and asking per read would be a request per
/// read.
pub struct RemoteTrack {
    fetcher: std::sync::Arc<crate::webdav::Fetcher>,
    href: String,
    ranges: bool,
}

impl RemoteTrack {
    pub fn new(fetcher: std::sync::Arc<crate::webdav::Fetcher>, href: &str) -> Self {
        let ranges = fetcher.supports_ranges(href);
        RemoteTrack {
            fetcher,
            href: href.to_string(),
            ranges,
        }
    }

    /// Whether this track can be played while it arrives, or must land first.
    pub fn streamable(&self) -> bool {
        self.ranges
    }
}

impl RangeFetch for RemoteTrack {
    fn range(&self, at: u64, len: u64) -> Result<(Vec<u8>, Option<u64>), String> {
        self.fetcher.range(&self.href, at, len)
    }

    fn supports_ranges(&self) -> bool {
        self.ranges
    }

    fn whole(&self) -> Result<Vec<u8>, String> {
        self.fetcher.fetch(&self.href)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server holding one object, counting what was asked of it.
    struct Fake {
        bytes: Vec<u8>,
        ranges: bool,
        asked: Mutex<Vec<(u64, u64)>>,
    }

    impl RangeFetch for Fake {
        fn range(&self, at: u64, len: u64) -> Result<(Vec<u8>, Option<u64>), String> {
            self.asked.lock().expect("asked").push((at, len));
            let start = (at as usize).min(self.bytes.len());
            let end = ((at + len) as usize).min(self.bytes.len());
            Ok((
                self.bytes[start..end].to_vec(),
                Some(self.bytes.len() as u64),
            ))
        }

        fn supports_ranges(&self) -> bool {
            self.ranges
        }

        fn whole(&self) -> Result<Vec<u8>, String> {
            self.asked.lock().expect("asked").push((0, u64::MAX));
            Ok(self.bytes.clone())
        }
    }

    fn fake(len: usize, ranges: bool) -> Arc<Fake> {
        Arc::new(Fake {
            bytes: (0..len).map(|i| (i % 251) as u8).collect(),
            ranges,
            asked: Mutex::new(Vec::new()),
        })
    }

    #[test]
    fn reads_only_the_part_it_was_asked_for() {
        let server = fake(2 * CHUNK as usize, true);
        let mut source = RemoteSource::new(server.clone(), Arc::new(Mutex::new(Chunks::default())));

        let mut buf = [0u8; 16];
        source.read_exact(&mut buf).expect("read");

        assert_eq!(buf[0], 0);
        assert_eq!(
            server.asked.lock().expect("asked").len(),
            1,
            "one read of sixteen bytes cost more than one request",
        );
    }

    /// The case the whole design is for: an MP4 whose tables are at the end.
    #[test]
    fn can_seek_to_the_end_without_reading_everything_before_it() {
        let server = fake(10 * CHUNK as usize, true);
        let mut source = RemoteSource::new(server.clone(), Arc::new(Mutex::new(Chunks::default())));

        let end = source.seek(SeekFrom::End(-8)).expect("seek");
        assert_eq!(end, 10 * CHUNK - 8);

        let mut buf = [0u8; 8];
        source.read_exact(&mut buf).expect("read the tail");

        // The tail's chunk and the one the length was learned from — not the
        // eight chunks in between.
        assert!(
            server.asked.lock().expect("asked").len() <= 2,
            "seeking to the end pulled the whole object",
        );
    }

    #[test]
    fn a_second_read_of_the_same_region_asks_nothing() {
        let server = fake(4 * CHUNK as usize, true);
        let held = Arc::new(Mutex::new(Chunks::default()));
        let mut source = RemoteSource::new(server.clone(), held.clone());

        let mut buf = [0u8; 32];
        source.read_exact(&mut buf).expect("read");
        let after_first = server.asked.lock().expect("asked").len();

        source.seek(SeekFrom::Start(0)).expect("rewind");
        source.read_exact(&mut buf).expect("read again");

        assert_eq!(
            server.asked.lock().expect("asked").len(),
            after_first,
            "the bytes were fetched twice",
        );
    }

    /// A server that ignores `Range` still has to work — slower, not broken.
    #[test]
    fn falls_back_to_the_whole_object_when_ranges_are_refused() {
        let server = fake(3 * CHUNK as usize, false);
        let mut source = RemoteSource::new(server.clone(), Arc::new(Mutex::new(Chunks::default())));

        assert!(
            !source.is_seekable(),
            "claimed to seek against a server that cannot"
        );

        let mut buf = [0u8; 16];
        source.read_exact(&mut buf).expect("read");
        assert_eq!(source.byte_len(), Some(3 * CHUNK));
    }
}

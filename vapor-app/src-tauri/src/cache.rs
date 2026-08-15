//! Local audio cache.
//!
//! The library lives in the user's cloud; playback and analysis need local
//! bytes. This is the layer between the two.
//!
//! ## Content addressing
//!
//! Files are named by a hash of their href rather than by their original path.
//! Two reasons: remote paths contain characters no filesystem agrees on, and a
//! flat directory of fixed-length names avoids rebuilding a remote tree
//! locally just to find one file.
//!
//! ## Writes are atomic
//!
//! Download to a temporary, then rename. A partial file that looks complete is
//! worse than no file: analysis would read it, produce a confident wrong
//! answer, and cache that. The temporary is a sibling because `rename` is only
//! atomic within a filesystem.
//!
//! ## Eviction
//!
//! Least-recently-used, bounded by total bytes. The bound matters on mobile,
//! where an unbounded cache eventually fills the device and the OS starts
//! deleting things the app cares about.

use std::fs;
use std::path::{Path, PathBuf};

/// Default ceiling. Generous on a desktop, deliberately not unbounded.
pub const DEFAULT_MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

pub struct Cache {
    dir: PathBuf,
    max_bytes: u64,
}

#[derive(Debug)]
pub enum CacheError {
    Io(std::io::Error),
    Fetch(String),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::Io(e) => write!(f, "{e}"),
            CacheError::Fetch(m) => write!(f, "{m}"),
        }
    }
}

impl From<std::io::Error> for CacheError {
    fn from(e: std::io::Error) -> Self {
        CacheError::Io(e)
    }
}

impl Cache {
    pub fn new(dir: PathBuf) -> Self {
        Cache {
            dir,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    pub fn with_max_bytes(mut self, max: u64) -> Self {
        self.max_bytes = max;
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Local path for an href, whether or not it exists yet.
    ///
    /// The original extension is kept: Symphonia probes the actual container,
    /// but a correct extension is a useful hint and makes the directory
    /// readable by a person looking at it.
    pub fn path_for(&self, href: &str) -> PathBuf {
        let ext = href
            .rsplit('.')
            .next()
            .filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or("audio");
        self.dir.join(format!("{:016x}.{ext}", hash(href)))
    }

    /// Local path, but only if the file is actually present.
    pub fn get(&self, href: &str) -> Option<PathBuf> {
        let p = self.path_for(href);
        p.is_file().then_some(p)
    }

    pub fn contains(&self, href: &str) -> bool {
        self.path_for(href).is_file()
    }

    /// Store bytes for an href, atomically.
    ///
    /// `fetch` is a parameter rather than an HTTP call inside this function so
    /// the whole layer can be tested without a network.
    pub fn store<F>(&self, href: &str, fetch: F) -> Result<PathBuf, CacheError>
    where
        F: FnOnce() -> Result<Vec<u8>, String>,
    {
        if let Some(existing) = self.get(href) {
            return Ok(existing);
        }

        fs::create_dir_all(&self.dir)?;
        let bytes = fetch().map_err(CacheError::Fetch)?;

        // An empty response is a failed download that returned 200. Writing it
        // would poison the cache with a file that exists and cannot decode.
        if bytes.is_empty() {
            return Err(CacheError::Fetch("empty response".to_string()));
        }

        let target = self.path_for(href);
        let tmp = self.dir.join(format!(".{:016x}.tmp", hash(href)));
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &target)?;

        self.evict_to_fit()?;
        Ok(target)
    }

    /// Total bytes currently held.
    pub fn size(&self) -> u64 {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return 0;
        };
        entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum()
    }

    /// Delete least-recently-used files until the cache fits its bound.
    ///
    /// Access time is what a cache should evict on, but many filesystems mount
    /// with `relatime` or `noatime`, so it is unreliable. Modification time is
    /// used instead — for a write-once cache that is the time it was fetched,
    /// which approximates "least recently useful" well enough and never lies.
    fn evict_to_fit(&self) -> Result<(), CacheError> {
        let mut total = self.size();
        if total <= self.max_bytes {
            return Ok(());
        }

        let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = fs::read_dir(&self.dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                if !meta.is_file() {
                    return None;
                }
                Some((e.path(), meta.len(), meta.modified().ok()?))
            })
            .collect();

        files.sort_by_key(|(_, _, t)| *t);

        for (path, len, _) in files {
            if total <= self.max_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(len);
            }
        }
        Ok(())
    }

    /// Remove one entry.
    pub fn remove(&self, href: &str) -> Result<(), CacheError> {
        match fs::remove_file(self.path_for(href)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Empty the cache. Part of "delete my data" meaning what it says.
    pub fn clear(&self) -> Result<(), CacheError> {
        match fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/// FNV-1a. Names cache files; the only requirements are determinism and a low
/// collision rate, so a cryptographic hash would be cost without benefit.
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

    fn temp_cache() -> (Cache, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "vapor-cache-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        (Cache::new(dir.clone()), dir)
    }

    /// Synthetic audio: a WAV header plus silence. Real enough to be a file of
    /// a known size, without needing anyone's music.
    fn synthetic_wav(samples: usize) -> Vec<u8> {
        let data_len = (samples * 2) as u32;
        let mut v = Vec::with_capacity(44 + data_len as usize);
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_len).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&44100u32.to_le_bytes());
        v.extend_from_slice(&88200u32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_len.to_le_bytes());
        v.resize(44 + data_len as usize, 0);
        v
    }

    #[test]
    fn stores_and_retrieves() {
        let (cache, dir) = temp_cache();
        let bytes = synthetic_wav(1000);

        assert!(cache.get("/a.wav").is_none());
        let path = cache
            .store("/a.wav", || Ok(bytes.clone()))
            .expect("store");
        assert!(path.is_file());
        assert_eq!(cache.get("/a.wav"), Some(path));

        let _ = fs::remove_dir_all(dir);
    }

    /// A second request must not re-download.
    #[test]
    fn a_cached_file_is_not_fetched_again() {
        let (cache, dir) = temp_cache();
        cache
            .store("/a.wav", || Ok(synthetic_wav(100)))
            .expect("first");

        let mut fetched = false;
        cache
            .store("/a.wav", || {
                fetched = true;
                Ok(synthetic_wav(100))
            })
            .expect("second");

        assert!(!fetched, "re-downloaded a file already cached");
        let _ = fs::remove_dir_all(dir);
    }

    /// Different hrefs must not collide onto one file.
    #[test]
    fn distinct_hrefs_get_distinct_paths() {
        let (cache, _dir) = temp_cache();
        let a = cache.path_for("/music/one.mp3");
        let b = cache.path_for("/music/two.mp3");
        assert_ne!(a, b);
    }

    /// Remote paths contain characters filesystems disagree about; the cache
    /// name must survive all of them.
    #[test]
    fn awkward_remote_paths_produce_usable_filenames() {
        let (cache, _dir) = temp_cache();
        for href in [
            "/dav/Bj%C3%B6rk/03 - J%C3%B3ga.flac",
            "/a/b/c/track?with=query.mp3",
            "/deep/../weird/./path.m4a",
            "/no-extension",
        ] {
            let p = cache.path_for(href);
            let name = p.file_name().expect("has a name").to_string_lossy();
            assert!(
                !name.contains('/') && !name.contains('%') && !name.contains('?'),
                "unusable filename for {href}: {name}"
            );
        }
    }

    /// A 200 with no body is a failed download. Writing it would leave a file
    /// that exists and cannot decode — worse than absent, because every later
    /// lookup treats it as cached.
    #[test]
    fn an_empty_response_is_refused() {
        let (cache, dir) = temp_cache();
        let result = cache.store("/a.wav", || Ok(Vec::new()));
        assert!(result.is_err());
        assert!(!cache.contains("/a.wav"), "cached an empty file");
        let _ = fs::remove_dir_all(dir);
    }

    /// A failed fetch must leave nothing behind, including temporaries.
    #[test]
    fn a_failed_fetch_leaves_no_trace() {
        let (cache, dir) = temp_cache();
        let _ = cache.store("/a.wav", || Err("network down".to_string()));

        assert!(!cache.contains("/a.wav"));
        let leftovers: Vec<String> = fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn no_temporary_survives_a_successful_store() {
        let (cache, dir) = temp_cache();
        cache.store("/a.wav", || Ok(synthetic_wav(50))).expect("store");

        let tmps: Vec<String> = fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "left temporaries: {tmps:?}");
        let _ = fs::remove_dir_all(dir);
    }

    /// The bound is the point: an unbounded cache eventually fills a phone.
    #[test]
    fn eviction_keeps_the_cache_under_its_bound() {
        let (cache, dir) = temp_cache();
        // Each file is 44 + 2000 bytes; the bound holds roughly two.
        let cache = cache.with_max_bytes(5_000);

        for i in 0..6 {
            cache
                .store(&format!("/track{i}.wav"), || Ok(synthetic_wav(1000)))
                .expect("store");
            // mtime has second granularity on some filesystems, and eviction
            // orders by it; without a gap the order is arbitrary.
            std::thread::sleep(std::time::Duration::from_millis(1100));
        }

        assert!(
            cache.size() <= 5_000,
            "cache grew past its bound: {} bytes",
            cache.size()
        );
        // The newest must survive — evicting what was just asked for would
        // make the cache useless.
        assert!(cache.contains("/track5.wav"), "evicted the newest entry");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn remove_and_clear_are_idempotent() {
        let (cache, dir) = temp_cache();
        cache.store("/a.wav", || Ok(synthetic_wav(10))).expect("store");

        cache.remove("/a.wav").expect("remove");
        cache.remove("/a.wav").expect("removing twice must not fail");
        assert!(!cache.contains("/a.wav"));

        cache.clear().expect("clear");
        cache.clear().expect("clearing twice must not fail");
        let _ = fs::remove_dir_all(dir);
    }

    /// The cache stores bytes it did not choose; a file it wrote must still be
    /// readable by the analyser, or the layer is pointless.
    #[test]
    fn a_cached_file_is_decodable() {
        let (cache, dir) = temp_cache();
        let path = cache
            .store("/tone.wav", || Ok(synthetic_wav(44_100)))
            .expect("store");

        let decoded = vapor_dsp::decode::decode_to_mono(&path);
        assert!(decoded.is_ok(), "cached file did not decode: {decoded:?}");
        let _ = fs::remove_dir_all(dir);
    }
}

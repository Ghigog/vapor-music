//! Persistence.
//!
//! Lives in the shell because `vapor-core` deliberately owns no I/O — that is
//! what lets the core be tested without a filesystem and reused in the browser,
//! where `~/Library` does not exist and OPFS does.
//!
//! ## Writes are atomic
//!
//! Every save goes to a temporary file in the same directory and is then
//! renamed over the target. `rename` within a filesystem is atomic, so a crash
//! or a power cut during a write leaves either the old file or the new one —
//! never a half-written one.
//!
//! This is not hypothetical for this app: playlists are saved on every
//! mutation, so the window where a naive truncate-and-write could lose the
//! whole collection is hit constantly. The Godot build had exactly this bug.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Where the app keeps its data.
///
/// One directory rather than scattering files, so "delete my data" is a
/// directory removal — which the Your Data screen in the design promises.
pub struct Store {
    dir: PathBuf,
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Serde(serde_json::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "{e}"),
            StoreError::Serde(e) => write!(f, "{e}"),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Serde(e)
    }
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Store { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }

    /// Load a value, or `None` when it has never been written.
    ///
    /// A corrupt file is an error rather than a silent default: quietly
    /// returning an empty playlist collection would look exactly like "you have
    /// no playlists", and the person would have no idea their data was still
    /// on disk and merely unreadable.
    pub fn load<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, StoreError> {
        let path = self.path(name);
        match fs::read_to_string(&path) {
            Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Write a value atomically.
    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<(), StoreError> {
        fs::create_dir_all(&self.dir)?;

        let target = self.path(name);
        // The temporary must share a directory with the target: `rename` is
        // only atomic within a filesystem, and /tmp is frequently a different
        // one.
        let tmp = self.dir.join(format!(".{name}.json.tmp"));

        let json = serde_json::to_string_pretty(value)?;
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &target)?;
        Ok(())
    }

    /// Remove everything. Backs the "delete my data" promise.
    pub fn clear(&self) -> Result<(), StoreError> {
        match fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "vapor-store-test-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        (Store::new(dir.clone()), dir)
    }

    #[test]
    fn round_trips_a_value() {
        let (store, dir) = temp_store();
        let value = vec!["a".to_string(), "b".to_string()];

        store.save("things", &value).expect("save");
        let back: Option<Vec<String>> = store.load("things").expect("load");

        assert_eq!(back, Some(value));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let (store, _dir) = temp_store();
        let back: Option<Vec<String>> = store.load("never-written").expect("load");
        assert_eq!(back, None);
    }

    /// A corrupt file must not read as "empty". Silently defaulting would show
    /// a person an empty library while their data sits unreadable on disk.
    #[test]
    fn corrupt_data_is_an_error_not_a_default() {
        let (store, dir) = temp_store();
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("broken.json"), "{ not json").expect("write");

        let result: Result<Option<Vec<String>>, _> = store.load("broken");
        assert!(result.is_err(), "corrupt data silently became a default");
        let _ = fs::remove_dir_all(dir);
    }

    /// The temporary must not survive a successful save, or the data directory
    /// fills with dotfiles.
    #[test]
    fn no_temporary_is_left_behind() {
        let (store, dir) = temp_store();
        store.save("things", &vec![1, 2, 3]).expect("save");

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();

        assert!(leftovers.is_empty(), "left temporaries: {leftovers:?}");
        let _ = fs::remove_dir_all(dir);
    }

    /// Overwriting must replace, not append or interleave — the failure a
    /// non-atomic write produces when the new value is shorter than the old.
    #[test]
    fn overwriting_replaces_the_whole_file() {
        let (store, dir) = temp_store();
        store
            .save("things", &vec!["long".to_string(), "list".into(), "here".into()])
            .expect("save");
        store.save("things", &vec!["x".to_string()]).expect("save");

        let back: Option<Vec<String>> = store.load("things").expect("load");
        assert_eq!(back, Some(vec!["x".to_string()]));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn clear_removes_everything_and_is_idempotent() {
        let (store, dir) = temp_store();
        store.save("a", &1).expect("save");
        store.save("b", &2).expect("save");

        store.clear().expect("clear");
        assert!(!dir.exists());
        store.clear().expect("clearing twice must not fail");
    }
}

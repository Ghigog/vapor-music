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

/// A file that could not be read and was moved aside rather than replaced.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quarantined {
    /// The store name, as the app asked for it — "playlists", "analysis".
    pub name: String,
    /// Why it could not be read, in the words of whatever failed.
    pub reason: String,
    /// Where the bytes were kept. `None` means even the rename failed, which
    /// is the one case where the data really is at risk from the next save.
    pub kept_at: Option<PathBuf>,
}

impl Quarantined {
    /// What to tell the person, in one sentence.
    pub fn message(&self) -> String {
        match &self.kept_at {
            Some(path) => format!(
                "Your {} file could not be read ({}). It has been kept at {} and \
                 the app started with an empty one, so nothing has been overwritten.",
                self.name,
                self.reason,
                path.display()
            ),
            None => format!(
                "Your {} file could not be read ({}), and it could not be moved \
                 aside either. Copy it somewhere safe before changing anything.",
                self.name, self.reason
            ),
        }
    }
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

    /// Load a value, moving the file aside if it cannot be read.
    ///
    /// [`Store::load`] reports a corrupt file as an error, on the grounds that
    /// quietly returning an empty playlist collection looks exactly like "you
    /// have no playlists". Every caller then wrote `.unwrap_or(None)
    /// .unwrap_or_default()`, which is precisely that silent default — and the
    /// consequence is worse than a wrong screen. The app carries on with an
    /// empty collection, the next mutation saves it, and the atomic write
    /// replaces the unreadable-but-intact file with an empty one. The data was
    /// recoverable right up until the moment the app tried to help.
    ///
    /// So the file is renamed out of the way first. The app still starts with a
    /// default, because refusing to open is a worse answer to "one of fourteen
    /// files is damaged", but the bytes survive and the returned
    /// [`Quarantined`] says where they went so a person can be told.
    pub fn load_or_quarantine<T: DeserializeOwned>(
        &self,
        name: &str,
    ) -> (Option<T>, Option<Quarantined>) {
        match self.load(name) {
            Ok(value) => (value, None),
            Err(e) => {
                let from = self.path(name);
                let to = self.quarantine_path(name);
                let moved = fs::rename(&from, &to).is_ok();
                (
                    None,
                    Some(Quarantined {
                        name: name.to_string(),
                        reason: e.to_string(),
                        kept_at: moved.then(|| to.clone()),
                    }),
                )
            }
        }
    }

    /// A free filename to move a damaged file to.
    ///
    /// Numbered rather than timestamped, and never overwriting: a file that has
    /// failed twice has two copies, because the *first* is the one most likely
    /// to still hold the data.
    fn quarantine_path(&self, name: &str) -> PathBuf {
        for n in 0..1000 {
            let candidate = if n == 0 {
                self.dir.join(format!("{name}.corrupt.json"))
            } else {
                self.dir.join(format!("{name}.corrupt.{n}.json"))
            };
            if !candidate.exists() {
                return candidate;
            }
        }
        self.dir.join(format!("{name}.corrupt.json"))
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

    /// The whole point: a file that cannot be read is *kept*.
    ///
    /// Before this, an unreadable playlists file loaded as an empty collection
    /// and the next mutation saved that empty collection over it. The data was
    /// recoverable right up until the app tried to help.
    #[test]
    fn a_corrupt_file_is_moved_aside_rather_than_replaced() {
        let (store, dir) = temp_store();
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("playlists.json"), b"{ this is not json").expect("write");

        let (loaded, problem) = store.load_or_quarantine::<Vec<String>>("playlists");
        assert!(loaded.is_none());
        let problem = problem.expect("a reported problem");
        assert_eq!(problem.name, "playlists");

        let kept = problem.kept_at.clone().expect("the bytes were kept");
        assert_eq!(
            std::fs::read_to_string(&kept).expect("read"),
            "{ this is not json",
            "the original bytes did not survive"
        );
        // And the original name is now free, so the app can carry on without
        // overwriting anything.
        assert!(!dir.join("playlists.json").exists());

        // The message names the file and where it went — it is shown verbatim.
        let message = problem.message();
        assert!(message.contains("playlists"), "{message}");
        assert!(message.contains(&kept.display().to_string()), "{message}");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Saving after a quarantine must not clobber the kept copy.
    #[test]
    fn the_app_can_carry_on_without_touching_the_kept_copy() {
        let (store, dir) = temp_store();
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("playlists.json"), b"garbage").expect("write");

        let (_, problem) = store.load_or_quarantine::<Vec<String>>("playlists");
        let kept = problem.expect("problem").kept_at.expect("kept");

        store
            .save("playlists", &vec!["new".to_string()])
            .expect("save");

        assert_eq!(std::fs::read_to_string(&kept).expect("read"), "garbage");
        let now: Vec<String> = store.load("playlists").expect("load").expect("some");
        assert_eq!(now, vec!["new".to_string()]);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Failing twice keeps both copies, oldest first — the earliest is the one
    /// most likely to still hold the data.
    #[test]
    fn a_second_failure_does_not_overwrite_the_first_rescue() {
        let (store, dir) = temp_store();
        std::fs::create_dir_all(&dir).expect("dir");

        std::fs::write(dir.join("tags.json"), b"first").expect("write");
        let a = store
            .load_or_quarantine::<Vec<String>>("tags")
            .1
            .expect("first problem")
            .kept_at
            .expect("kept");

        std::fs::write(dir.join("tags.json"), b"second").expect("write");
        let b = store
            .load_or_quarantine::<Vec<String>>("tags")
            .1
            .expect("second problem")
            .kept_at
            .expect("kept");

        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(&a).expect("read"), "first");
        assert_eq!(std::fs::read_to_string(&b).expect("read"), "second");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A file that was never written is not a problem, and must not be
    /// reported as one — that is every store on a first launch.
    #[test]
    fn a_missing_file_is_not_damage() {
        let (store, dir) = temp_store();
        let (loaded, problem) = store.load_or_quarantine::<Vec<String>>("never-written");

        assert!(loaded.is_none());
        assert!(problem.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Valid JSON of the wrong shape is corruption too — this is what a
    /// schema change looks like from the reader's side.
    #[test]
    fn well_formed_json_of_the_wrong_type_is_quarantined() {
        let (store, dir) = temp_store();
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("settings.json"), br#"["not","an","object"]"#).expect("write");

        let (loaded, problem) =
            store.load_or_quarantine::<std::collections::HashMap<String, String>>("settings");
        assert!(loaded.is_none());
        assert!(problem.is_some(), "a shape mismatch went unreported");

        let _ = std::fs::remove_dir_all(dir);
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
            .save(
                "things",
                &vec!["long".to_string(), "list".into(), "here".into()],
            )
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

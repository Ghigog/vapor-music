//! Persistence.
//!
//! Lives in the shell because `vapor-core` deliberately owns no I/O — that is
//! what lets the core be tested without a filesystem and reused in the browser,
//! where `~/Library` does not exist and OPFS does.
//!
//! ## Writes are atomic, and durable
//!
//! Every save goes to a temporary file in the same directory and is then
//! renamed over the target. `rename` within a filesystem is atomic, so a crash
//! or a power cut during a write leaves either the old file or the new one —
//! never a half-written one.
//!
//! This is not hypothetical for this app: playlists are saved on every
//! mutation, so the window where a naive truncate-and-write could lose the
//! whole collection is hit constantly. The Godot build had exactly this bug.
//!
//! Atomicity is about *ordering* and does not by itself give durability — this
//! paragraph used to claim both. `rename` guarantees no reader ever sees a torn
//! file; it does not guarantee the bytes reached the disk before the directory
//! entry did. A power cut in that window leaves the new name pointing at blocks
//! that were never written, which presents as a zero-length or truncated file
//! at the target — the exact damage the atomic write exists to prevent. So the
//! temporary is `fsync`ed before the rename and the directory after it.
//!
//! ## Files carry a version
//!
//! Every file is `{"v":1,"data":…}`. Nothing reads the version yet, because
//! there has only ever been one shape — which is the point. Renaming a field in
//! v1.1 currently makes the file unparseable, and `load_or_quarantine`
//! correctly reads unparseable as damage: the person is greeted by an empty
//! library and a `.corrupt.json` they are told to find. The envelope is what
//! makes that a migration instead. It costs nothing while there is exactly one
//! shape and cannot be added retroactively once there are two.
//!
//! A file written before the envelope existed is read as v1, so no upgrade step
//! is needed.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// The shape version written into every file. See the module docs.
const SCHEMA_VERSION: u32 = 1;

/// What actually goes on disk. Borrows so `save` need not clone the value.
#[derive(Serialize)]
struct Envelope<'a, T> {
    v: u32,
    data: &'a T,
}

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
    /// Written by a later version of the app than this one.
    ///
    /// Distinct from corruption because the file is fine — this build simply
    /// does not know the shape. Reading it as the older shape would silently
    /// discard whatever the newer version added, which is worse than refusing.
    FutureVersion {
        found: u32,
        understood: u32,
    },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "{e}"),
            StoreError::Serde(e) => write!(f, "{e}"),
            StoreError::FutureVersion { found, understood } => write!(
                f,
                "written by a newer version of Vapor Music (file format {found}, \
                 this build understands {understood})"
            ),
        }
    }
}

/// Take the payload out of a versioned file, or accept a pre-envelope one.
///
/// The shape test is deliberately narrow — an object of exactly `v` and `data`,
/// with `v` a number — because a legacy payload is an arbitrary value and could
/// itself be an object. Requiring exactly those two keys means a false positive
/// needs a stored type whose only fields are `v` and `data`, which none of the
/// fourteen stores has.
fn unwrap_envelope(document: serde_json::Value) -> Result<serde_json::Value, StoreError> {
    let versioned = document.as_object().is_some_and(|o| {
        o.len() == 2 && o.contains_key("data") && o.get("v").is_some_and(serde_json::Value::is_u64)
    });

    if !versioned {
        // Written before the envelope existed, which is v1 by definition.
        return Ok(document);
    }

    let serde_json::Value::Object(mut object) = document else {
        unreachable!("`versioned` is only true for an object")
    };

    let found = object
        .get("v")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;

    if found > SCHEMA_VERSION {
        return Err(StoreError::FutureVersion {
            found,
            understood: SCHEMA_VERSION,
        });
    }

    Ok(object
        .remove("data")
        .expect("`versioned` required a data key"))
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
        let text = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let document: serde_json::Value = serde_json::from_str(&text)?;
        let payload = unwrap_envelope(document)?;
        Ok(Some(serde_json::from_value(payload)?))
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

    /// Write a value atomically and durably.
    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<(), StoreError> {
        fs::create_dir_all(&self.dir)?;

        let target = self.path(name);
        // The temporary must share a directory with the target: `rename` is
        // only atomic within a filesystem, and /tmp is frequently a different
        // one.
        let tmp = self.dir.join(format!(".{name}.json.tmp"));

        let json = serde_json::to_string_pretty(&Envelope {
            v: SCHEMA_VERSION,
            data: value,
        })?;

        // Written through a handle rather than `fs::write` so there is
        // something to `sync_all`. Without it the rename can publish a name
        // whose blocks never reached the disk.
        let mut file = fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        drop(file);

        fs::rename(&tmp, &target)?;
        self.sync_dir();
        Ok(())
    }

    /// Make the rename itself durable.
    ///
    /// The rename is atomic, but the directory entry it writes still lives in
    /// the page cache until the directory is synced. Failure is deliberately
    /// ignored: the data is already on disk by this point, and refusing to save
    /// because a directory handle could not be opened would be a worse answer
    /// than a save that is durable slightly later than intended.
    #[cfg(unix)]
    fn sync_dir(&self) {
        if let Ok(dir) = fs::File::open(&self.dir) {
            let _ = dir.sync_all();
        }
    }

    /// Not done on Windows: there is no portable way to open a directory handle
    /// for `sync_all`, and NTFS journals the rename regardless.
    #[cfg(not(unix))]
    fn sync_dir(&self) {}

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

    #[test]
    fn what_is_written_carries_a_version() {
        let (store, dir) = temp_store();
        store.save("things", &vec!["a".to_string()]).expect("save");

        let raw = fs::read_to_string(dir.join("things.json")).expect("read");
        let document: serde_json::Value = serde_json::from_str(&raw).expect("parse");

        assert_eq!(document["v"], 1, "no version on disk: {raw}");
        assert_eq!(document["data"], serde_json::json!(["a"]));
        let _ = fs::remove_dir_all(dir);
    }

    /// The upgrade path for anyone who ran a build from before the envelope.
    /// There is no migration step; the old shape is simply what v1 means.
    #[test]
    fn a_file_written_before_the_envelope_still_loads() {
        let (store, dir) = temp_store();
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("things.json"), r#"["a","b"]"#).expect("write");

        let back: Option<Vec<String>> = store.load("things").expect("load");
        assert_eq!(back, Some(vec!["a".to_string(), "b".to_string()]));
        let _ = fs::remove_dir_all(dir);
    }

    /// A payload that happens to be an object must not be mistaken for an
    /// envelope and unwrapped into its own `data` field.
    #[test]
    fn a_legacy_object_payload_is_not_mistaken_for_an_envelope() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Settings {
            volume: u32,
            shuffled: bool,
        }

        let (store, dir) = temp_store();
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("settings.json"), r#"{"volume":7,"shuffled":true}"#).expect("write");

        let back: Option<Settings> = store.load("settings").expect("load");
        assert_eq!(
            back,
            Some(Settings {
                volume: 7,
                shuffled: true
            })
        );
        let _ = fs::remove_dir_all(dir);
    }

    /// Refused rather than read as the older shape, which would silently drop
    /// whatever the newer version added. `load_or_quarantine` then keeps the
    /// bytes, so a downgrade costs the person a file move and not their data.
    #[test]
    fn a_file_from_a_newer_version_is_refused_not_misread() {
        let (store, dir) = temp_store();
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("things.json"), r#"{"v":99,"data":["a"]}"#).expect("write");

        let result: Result<Option<Vec<String>>, _> = store.load("things");
        let message = result
            .expect_err("a future version was read anyway")
            .to_string();
        assert!(message.contains("newer version"), "{message}");
        assert!(message.contains("99"), "{message}");

        let (loaded, problem) = store.load_or_quarantine::<Vec<String>>("things");
        assert!(loaded.is_none());
        let kept = problem.expect("not quarantined").kept_at.expect("not kept");
        assert_eq!(
            fs::read_to_string(&kept).expect("read"),
            r#"{"v":99,"data":["a"]}"#,
            "the newer file's bytes were not preserved"
        );
        let _ = fs::remove_dir_all(dir);
    }
}

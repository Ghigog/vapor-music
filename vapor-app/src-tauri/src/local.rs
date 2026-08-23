//! A library that is already on this device.
//!
//! The other source is `webdav.rs`, and this deliberately produces the same
//! shape: a list of hrefs, a count of directories walked, a count of things it
//! could not read. Everything downstream — the index, playlists, tags,
//! analysis, the cache — is written against hrefs and does not care which
//! scanner produced them.
//!
//! ## Hrefs are relative to the root, not absolute
//!
//! A file at `/Users/x/Music/Aphex/Xtal.mp3` under a root of `/Users/x/Music`
//! is stored as `/Aphex/Xtal.mp3`. Two reasons, and the second is the one that
//! matters:
//!
//! * It is the same shape a WebDAV href has, so nothing downstream needs to
//!   know which kind of library it is holding.
//! * **Moving the music folder does not orphan the library.** Absolute paths
//!   would put the machine's directory layout inside every playlist, every tag
//!   record and every analysis result; changing the root would invalidate all
//!   of them at once. Relative hrefs make that a one-field edit.
//!
//! Resolving an href back to a file is therefore `root.join(href)`, and that is
//! the only place the absolute path exists.

use std::path::{Path, PathBuf};

use vapor_library::is_audio_path;

/// What a scan found. Mirrors `webdav::ScanResult` field for field.
#[derive(Debug, Default, PartialEq)]
pub struct ScanResult {
    /// Root-relative, leading slash, in the order they were found.
    pub files: Vec<String>,
    /// Directories visited, so progress can be reported honestly.
    pub directories: usize,
    /// Directories that could not be read and were skipped.
    ///
    /// Counted rather than swallowed, for the reason `webdav.rs` gives: a scan
    /// that quietly walked past half a library still says "found 40 tracks",
    /// and nothing distinguishes that from a library with 40 tracks in it.
    pub unreadable: usize,
}

#[derive(Debug)]
pub enum LocalError {
    NotADirectory(PathBuf),
    Io(std::io::Error),
}

impl std::fmt::Display for LocalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocalError::NotADirectory(p) => {
                write!(f, "{} is not a folder this app can read", p.display())
            }
            LocalError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<std::io::Error> for LocalError {
    fn from(e: std::io::Error) -> Self {
        LocalError::Io(e)
    }
}

/// How deep to walk before giving up.
///
/// Not a guess about how people file music — it is a loop guard. A symlink
/// pointing at one of its own ancestors is an infinite tree, and the check
/// below only refuses to *follow* directory symlinks, which does not help if
/// the filesystem itself has a cycle. Sixteen is far past any real layout of
/// artist/album/disc.
const MAX_DEPTH: usize = 16;

/// Walk `root` and return every audio file under it.
pub fn scan(root: &Path) -> Result<ScanResult, LocalError> {
    if !root.is_dir() {
        return Err(LocalError::NotADirectory(root.to_path_buf()));
    }

    let mut result = ScanResult::default();
    walk(root, root, 0, &mut result);

    // Stable across runs, which the filesystem's own order is not: `read_dir`
    // returns entries in whatever order the directory happens to hold them, so
    // without this the library reorders itself for no reason a person can see.
    result.files.sort();
    Ok(result)
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut ScanResult) {
    if depth > MAX_DEPTH {
        out.unreadable += 1;
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // A folder the user cannot read is not a failure of the scan. It is one
        // folder, counted, and the rest of the library still arrives.
        Err(_) => {
            out.unreadable += 1;
            return;
        }
    };

    out.directories += 1;

    for entry in entries.flatten() {
        let path = entry.path();

        // `file_type` here rather than `path.is_dir()`: the latter follows
        // symlinks, and following a directory symlink is how a scan walks into
        // a loop or wanders out of the library entirely.
        let Ok(kind) = entry.file_type() else {
            out.unreadable += 1;
            continue;
        };

        if kind.is_symlink() {
            continue;
        }

        if kind.is_dir() {
            // Skip the dotfiles that every music folder accumulates —
            // `.DS_Store` lives beside them and nothing under a dot directory
            // is a track someone means to play.
            if file_name(&path).starts_with('.') {
                continue;
            }
            walk(root, &path, depth + 1, out);
            continue;
        }

        if !kind.is_file() {
            continue;
        }

        let name = file_name(&path);
        if name.starts_with('.') {
            continue;
        }
        if !is_audio_path(&name) {
            continue;
        }

        if let Some(href) = href_of(root, &path) {
            out.files.push(href);
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// `/Users/x/Music/Aphex/Xtal.mp3` under `/Users/x/Music` → `/Aphex/Xtal.mp3`.
///
/// Returns `None` when the path is somehow not under the root, which `walk`
/// cannot produce but a caller could.
fn href_of(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut href = String::from("/");
    // Built from components rather than the OS string so a Windows library
    // yields `/Aphex/Xtal.mp3` and not `\Aphex\Xtal.mp3`. Hrefs are one shape
    // everywhere; the separator belongs to the filesystem, not to the library.
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    href.push_str(&parts.join("/"));
    Some(href)
}

/// The prefix that marks an href as belonging to a local folder.
///
/// A library can have several sources at once — a WebDAV server and any number
/// of folders — so an href has to say which one it came from. Without it two
/// folders that both contain `/Aphex/Xtal.mp3` are the same track as far as
/// the index, the cache and every playlist are concerned.
///
/// WebDAV hrefs are left exactly as they are, starting with `/`. That is not
/// tidiness — it means a library that already exists keeps working, and no
/// stored playlist, tag or analysis record needs migrating.
const LOCAL: &str = "local:";

/// `("desktop-music", "/Aphex/Xtal.mp3")` → `"local:desktop-music/Aphex/Xtal.mp3"`.
pub fn href(source_id: &str, relative: &str) -> String {
    format!("{LOCAL}{source_id}{relative}")
}

/// The inverse. `None` for a WebDAV href, which is how the two are told apart.
pub fn parse_href(href: &str) -> Option<(&str, &str)> {
    let rest = href.strip_prefix(LOCAL)?;
    // The id cannot contain `/`, so the first one starts the path.
    let cut = rest.find('/')?;
    Some((&rest[..cut], &rest[cut..]))
}

/// Whether this href belongs to a local folder rather than a server.
pub fn is_local(href: &str) -> bool {
    href.starts_with(LOCAL)
}

/// Where each configured folder's files are, by source id.
///
/// The map `Cache` needs. Built from settings every time a cache is
/// constructed rather than cached itself, because a folder can be added or
/// removed while the app is running and a stale map plays the wrong file or
/// none.
pub fn roots(
    folders: &[vapor_library::settings::LocalFolder],
) -> std::collections::HashMap<String, PathBuf> {
    folders
        .iter()
        .map(|f| (f.id.clone(), PathBuf::from(&f.path)))
        .collect()
}

/// Turn an href back into a file on this device.
///
/// The inverse of [`href_of`], and the only place the absolute path exists.
pub fn resolve(root: &Path, href: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in href.split('/').filter(|p| !p.is_empty() && *p != "..") {
        path.push(part);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vapor-local-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    fn touch(root: &Path, relative: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, b"not really audio").expect("write");
    }

    #[test]
    fn finds_audio_at_any_depth_and_ignores_the_rest() {
        let root = temp_root();
        touch(&root, "Xtal.mp3");
        touch(&root, "Aphex/Selected Ambient/Xtal.flac");
        touch(&root, "Aphex/cover.jpg");
        touch(&root, "notes.txt");

        let found = scan(&root).expect("scan");

        assert_eq!(
            found.files,
            vec![
                "/Aphex/Selected Ambient/Xtal.flac".to_string(),
                "/Xtal.mp3".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// The href is what ends up in playlists, tags and analysis. If it carried
    /// the machine's directory layout, moving the music folder would orphan
    /// every one of them.
    #[test]
    fn hrefs_are_relative_to_the_root() {
        let root = temp_root();
        touch(&root, "Artist/Album/Track.m4a");

        let found = scan(&root).expect("scan");

        assert_eq!(found.files, vec!["/Artist/Album/Track.m4a".to_string()]);
        assert!(
            !found.files[0].contains(&root.to_string_lossy().to_string()),
            "the absolute path leaked into the href"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_href_resolves_back_to_the_file_it_came_from() {
        let root = temp_root();
        touch(&root, "Artist/Album/Track.m4a");

        let found = scan(&root).expect("scan");
        let path = resolve(&root, &found.files[0]);

        assert!(path.is_file(), "{} is not a file", path.display());
        let _ = std::fs::remove_dir_all(root);
    }

    /// An href arriving from a stored playlist is data, and `..` in it would
    /// climb out of the library.
    #[test]
    fn resolving_cannot_climb_out_of_the_root() {
        let root = temp_root();
        let escaped = resolve(&root, "/../../../etc/passwd");

        assert!(
            escaped.starts_with(&root),
            "{} escaped {}",
            escaped.display(),
            root.display()
        );
        assert!(escaped.ends_with("etc/passwd"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dotfiles_and_dot_directories_are_skipped() {
        let root = temp_root();
        touch(&root, "Real.mp3");
        touch(&root, ".hidden.mp3");
        touch(&root, ".Trash/Deleted.mp3");

        let found = scan(&root).expect("scan");

        assert_eq!(found.files, vec!["/Real.mp3".to_string()]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_order_is_stable_rather_than_the_filesystem_s() {
        let root = temp_root();
        for name in ["c.mp3", "a.mp3", "b.mp3"] {
            touch(&root, name);
        }

        let found = scan(&root).expect("scan");

        assert_eq!(
            found.files,
            vec![
                "/a.mp3".to_string(),
                "/b.mp3".to_string(),
                "/c.mp3".to_string()
            ]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_path_that_is_not_a_folder_is_refused_by_name() {
        let root = temp_root();
        touch(&root, "Track.mp3");

        let err = scan(&root.join("Track.mp3")).expect_err("a file is not a library");
        assert!(matches!(err, LocalError::NotADirectory(_)));
        assert!(err.to_string().contains("not a folder"), "{err}");
        let _ = std::fs::remove_dir_all(root);
    }

    /// Two folders can hold the same relative path. Without the source in the
    /// href they are one track to the index, the cache and every playlist.
    #[test]
    fn an_href_carries_which_source_it_came_from() {
        let a = href("desktop-music", "/Aphex/Xtal.mp3");
        let b = href("usb-drive", "/Aphex/Xtal.mp3");

        assert_ne!(a, b);
        assert_eq!(parse_href(&a), Some(("desktop-music", "/Aphex/Xtal.mp3")));
        assert_eq!(parse_href(&b), Some(("usb-drive", "/Aphex/Xtal.mp3")));
    }

    /// The reason WebDAV hrefs were left alone: a library that already exists
    /// keeps working, and nothing stored has to be migrated.
    #[test]
    fn a_webdav_href_is_not_mistaken_for_a_local_one() {
        let dav = "/dav/Music/Aphex/Xtal.mp3";

        assert!(!is_local(dav));
        assert_eq!(parse_href(dav), None);
        assert!(is_local(&href("anything", "/x.mp3")));
    }

    /// A directory symlink is the ordinary way a scan walks into a loop or out
    /// of the library. Not followed, and not counted as damage either.
    #[cfg(unix)]
    #[test]
    fn a_directory_symlink_is_not_followed() {
        let root = temp_root();
        touch(&root, "Music/Track.mp3");
        std::os::unix::fs::symlink(&root, root.join("Music/loop")).expect("symlink");

        let found = scan(&root).expect("scan");

        assert_eq!(found.files, vec!["/Music/Track.mp3".to_string()]);
        let _ = std::fs::remove_dir_all(root);
    }
}

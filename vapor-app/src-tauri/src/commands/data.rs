//! Your Data commands — where it is, what it weighs, and deleting it.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// Files that could not be read at startup and were moved aside.
///
/// Empty on every normal launch. Non-empty means the app is running on a
/// default for something the person had data for — an unreadable playlists
/// file is indistinguishable from having no playlists, and the difference
/// matters enormously. The bytes were kept; this is what says so.
#[tauri::command]
pub fn startup_problems(state: State<'_, Shared>) -> Result<Vec<String>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.damaged.iter().map(|d| d.message()).collect())
}

/// Where the app keeps everything, so the Your Data screen can show it.
///
/// Naming the directory is part of the claim: "your data is local" is an
/// assertion until a person can see the path and open it.
#[tauri::command]
pub fn data_location(state: State<'_, Shared>) -> Result<String> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.store.dir().display().to_string())
}

/// Itemise what the app is storing.
///
/// The Your Data screen is where the sovereignty claim gets proved instead of
/// asserted, and a single total proves nothing — a person has to be able to see
/// which file is which, open it, and find it is plain JSON.
#[tauri::command]
pub fn data_breakdown(state: State<'_, Shared>) -> Result<Vec<DataRow>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let dir = app.store.dir();

    let size_of = |name: &str| -> u64 {
        std::fs::metadata(dir.join(name))
            .map(|m| m.len())
            .unwrap_or(0)
    };

    Ok(vec![
        DataRow {
            label: "Music files".to_string(),
            path: if app.settings.remote.is_configured() {
                format!(
                    "{} / {}",
                    app.settings.remote.url.trim_end_matches('/'),
                    app.settings.remote.folder
                )
            } else {
                "No server connected".to_string()
            },
            // The library is the one thing not measured: asking a WebDAV server
            // for the size of every file is a scan, and a scan is not something
            // to run because a screen was opened.
            bytes: 0,
            local: false,
        },
        // Downloads before the cache: they are the deliberate half, and on a
        // device where somebody has kept a few playlists they will be most of
        // the total. Separating them is the difference between "the app is
        // holding two gigabytes" and "you asked it to keep two gigabytes".
        DataRow {
            label: "Downloads".to_string(),
            path: app.cache.downloads_dir().display().to_string(),
            bytes: app.cache.downloads_size(),
            local: true,
        },
        DataRow {
            label: "Offline cache".to_string(),
            path: app.cache.dir().display().to_string(),
            bytes: app.cache.size(),
            local: true,
        },
        // Artwork, separate from the catalogue. It used to be inside
        // `tags.json` and therefore invisible on this screen, which is how a
        // 155 MB file went unnoticed until it killed the phone.
        DataRow {
            label: "Cover art".to_string(),
            path: app.covers.dir().display().to_string(),
            bytes: app.covers.size(),
            local: true,
        },
        DataRow {
            label: "Track tags".to_string(),
            path: dir.join("tags.json").display().to_string(),
            bytes: size_of("tags.json"),
            local: true,
        },
        DataRow {
            label: "Library catalogue".to_string(),
            path: dir.join("analysis.json").display().to_string(),
            bytes: size_of("analysis.json"),
            local: true,
        },
        DataRow {
            label: "Playlists".to_string(),
            path: dir.join("playlists.json").display().to_string(),
            bytes: size_of("playlists.json"),
            local: true,
        },
        DataRow {
            label: "Settings".to_string(),
            path: dir.join("settings.json").display().to_string(),
            bytes: size_of("settings.json"),
            local: true,
        },
    ])
}

/// Open the data directory in the system file manager.
///
/// "Your data is local" is a claim until a person can go and look at it. No
/// Tauri plugin for this, and shelling out to the platform's own opener is
/// three lines — the path is one the app itself chose, never user input, so
/// there is nothing here to inject into.
#[tauri::command]
pub fn reveal_data_folder(state: State<'_, Shared>) -> Result<()> {
    let dir = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.store.dir().to_path_buf()
    };
    std::fs::create_dir_all(&dir)?;

    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };

    std::process::Command::new(opener)
        .arg(&dir)
        .spawn()
        .map_err(|e| Error(format!("Could not open the folder: {e}")))?;
    Ok(())
}

/// Delete everything the app has stored.
///
/// In-memory state is reset too, so the UI reflects the deletion immediately
/// rather than continuing to show data that no longer exists on disk.
#[tauri::command]
pub fn delete_all_data(state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    // Silence first. Deleting the cache from under a playing track would leave
    // the deck holding audio the person just asked to be rid of.
    if let Some(p) = app.player.as_ref() {
        p.stop();
    }
    app.generation += 1;
    app.loading = false;
    app.playing = None;
    app.playback_error = None;
    app.store.clear()?;
    app.cache.clear().map_err(|e| Error(e.to_string()))?;
    // The password lives in the keychain, not the data directory, so clearing
    // one does not clear the other. "Delete my data" must mean both.
    if !app.settings.remote.username.is_empty() {
        let _ = webdav::delete_password(&app.settings.remote.username);
    }
    app.settings = Settings::default();
    app.playlists = PlaylistStore::default();
    app.folders = FolderStore::default();
    app.groups = GroupStore::default();
    app.pinned = std::collections::HashSet::new();
    app.looked.clear();
    app.queue = Queue::default();
    app.rows.clear();
    Ok(())
}

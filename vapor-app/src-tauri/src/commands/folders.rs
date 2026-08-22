//! Folders on this device that the library reads from.
//!
//! The other source is the WebDAV server in `settings.remote`, and these are
//! additive to it rather than an alternative — see `local.rs` for how an href
//! says which source it came from.

use tauri::State;

use crate::*;
use vapor_library::settings::LocalFolder;

/// Add a folder to the library.
///
/// The path comes from the webview, which is not trusted with it: it is checked
/// to be a readable directory before it is stored, and a folder that is already
/// configured is a no-op rather than a duplicate. Nothing here reads the
/// contents — that is `scan_library`, which the caller runs next.
#[tauri::command]
pub fn add_local_folder(path: String, state: State<'_, Shared>) -> Result<Vec<LocalFolder>> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(Error("No folder was chosen.".to_string()));
    }

    let candidate = std::path::Path::new(trimmed);
    if !candidate.is_dir() {
        return Err(Error(format!(
            "{trimmed} is not a folder this app can open."
        )));
    }

    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

    // Same folder twice is the person clicking add on something they already
    // have, which is not an error — it is a no-op with the list as the answer.
    if app.settings.folders.iter().any(|f| f.path == trimmed) {
        return Ok(app.settings.folders.clone());
    }

    // Ids are never reused, even for a folder that was removed and re-added.
    // Every playlist entry, tag and analysis record from the old one names that
    // id, and reusing it would silently point them at the new folder's files.
    let id = new_id("folder");

    app.settings.folders.push(LocalFolder {
        id,
        path: trimmed.to_string(),
        name: String::new(),
    });

    let folders = app.settings.folders.clone();
    app.save_settings()?;
    // The cache resolves local hrefs through these roots; a cache still holding
    // the old set cannot find anything in the folder just added.
    app.rebuild_cache_roots();

    Ok(folders)
}

/// Stop reading a folder.
///
/// The files are not touched — this is the library forgetting where to look,
/// not a delete. Tracks from it disappear at the next scan.
#[tauri::command]
pub fn remove_local_folder(id: String, state: State<'_, Shared>) -> Result<Vec<LocalFolder>> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

    let before = app.settings.folders.len();
    app.settings.folders.retain(|f| f.id != id);
    if app.settings.folders.len() == before {
        return Err(Error("That folder is not in the library.".to_string()));
    }

    let folders = app.settings.folders.clone();
    app.save_settings()?;
    app.rebuild_cache_roots();

    Ok(folders)
}

/// What the library reads from, for the screen that lists it.
#[tauri::command]
pub fn local_folders(state: State<'_, Shared>) -> Result<Vec<LocalFolder>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.settings.folders.clone())
}

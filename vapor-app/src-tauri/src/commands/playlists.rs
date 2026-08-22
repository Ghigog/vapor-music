//! Playlist and folder commands.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

#[tauri::command]
pub fn playlists(state: State<'_, Shared>) -> Result<Vec<vapor_library::Playlist>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.playlists.all().to_vec())
}

#[tauri::command]
pub fn create_playlist(
    name: String,
    folder_id: Option<String>,
    state: State<'_, Shared>,
) -> Result<vapor_library::Playlist> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    // Ids are generated here rather than in the core so the core stays
    // deterministic and testable — see the note on PlaylistStore::create.
    let id = new_id("playlist");
    // A folder that has since been deleted would file the playlist somewhere
    // the rail cannot draw, so an unknown one means the top level.
    let folder = folder_id
        .filter(|f| app.folders.get(f).is_some())
        .unwrap_or_default();
    let created = app.playlists.create_in_folder(id, name, folder).clone();
    app.save_playlists()?;
    Ok(created)
}

#[tauri::command]
pub fn playlist_folders(state: State<'_, Shared>) -> Result<Vec<vapor_library::Folder>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.folders.all().to_vec())
}

#[tauri::command]
pub fn create_folder(name: String, state: State<'_, Shared>) -> Result<vapor_library::Folder> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(Error("A folder needs a name.".to_string()));
    }
    let id = new_id("folder");
    let created = app.folders.create(id, name, "").clone();
    app.save_folders()?;
    Ok(created)
}

#[tauri::command]
pub fn rename_folder(id: String, name: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if name.trim().is_empty() {
        return Err(Error("A folder needs a name.".to_string()));
    }
    let renamed = app.folders.rename(&id, name.trim());
    if renamed {
        app.save_folders()?;
    }
    Ok(renamed)
}

/// Delete a folder. The playlists inside it move to the top level.
///
/// Deleting a container must not delete what it contains — a folder is an
/// organisational convenience, and losing playlists to one would make filing
/// them a risk rather than a tidy-up. `FolderStore::delete` deliberately
/// returns the ids it orphaned instead of cascading, so reassigning them is
/// this layer's job.
#[tauri::command]
pub fn delete_folder(id: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !remove_folder(&mut app, &id) {
        return Ok(false);
    }
    app.save_folders()?;
    app.save_playlists()?;
    app.save_tombstones()?;
    Ok(true)
}

/// File a playlist into a folder, or out of one with an empty `folder_id`.
#[tauri::command]
pub fn set_playlist_folder(
    id: String,
    folder_id: String,
    state: State<'_, Shared>,
) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !folder_id.is_empty() && app.folders.get(&folder_id).is_none() {
        return Err(Error("That folder no longer exists.".to_string()));
    }
    let moved = app.playlists.set_folder(&id, folder_id);
    if moved {
        app.save_playlists()?;
    }
    Ok(moved)
}

#[tauri::command]
pub fn add_tracks_to_playlist(
    id: String,
    hrefs: Vec<String>,
    state: State<'_, Shared>,
) -> Result<usize> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let added = app.playlists.add_tracks(&id, &hrefs);
    // Only write when something changed — the core returns a count precisely so
    // a no-op bulk add does not cost a disk write.
    if added > 0 {
        app.save_playlists()?;
    }
    Ok(added)
}

#[tauri::command]
pub fn rename_playlist(id: String, name: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    // An empty name would leave a row nobody can identify or click.
    if name.trim().is_empty() {
        return Err(Error("A playlist needs a name.".to_string()));
    }
    let renamed = app.playlists.rename(&id, name.trim());
    if renamed {
        app.save_playlists()?;
    }
    Ok(renamed)
}

#[tauri::command]
pub fn delete_playlist(id: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let deleted = app.playlists.delete(&id).is_some();
    if deleted {
        // Written down before it is forgotten (TD-57). Without this the next
        // sync takes the playlist back from a device that had not heard, and
        // the deletion has to be done again on every device in turn.
        app.tombstones.record_playlist(&id, crate::peers::now());
        // The playlist tombstone already stops it being recreated, so the
        // per-track records under it can never be consulted again.
        app.tombstones.forget_tracks_of(&id);
        app.save_playlists()?;
        app.save_tombstones()?;
    }
    Ok(deleted)
}

#[tauri::command]
pub fn remove_playlist_track(id: String, index: usize, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

    // Read before removing: the tombstone is keyed by href, and after
    // `remove_track` the index no longer names anything.
    let href = app
        .playlists
        .get(&id)
        .and_then(|p| p.tracks.get(index).cloned());

    let removed = app.playlists.remove_track(&id, index);
    if removed {
        // The same reason `delete_playlist` writes one, one level down.
        // Without it the next sync takes the track back from a device that had
        // not heard — and unlike a whole playlist reappearing, nothing on
        // screen makes that visible.
        if let Some(href) = href {
            app.tombstones.record_track(&id, href, crate::peers::now());
            app.save_tombstones()?;
        }
        app.save_playlists()?;
    }
    Ok(removed)
}

#[tauri::command]
pub fn reorder_playlist_track(
    id: String,
    from: usize,
    to: usize,
    state: State<'_, Shared>,
) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let moved = app.playlists.reorder_tracks(&id, from, to);
    if moved {
        app.save_playlists()?;
    }
    Ok(moved)
}

/// A playlist's tracks as table rows, in playlist order.
///
/// Rows rather than hrefs: the screen shows title, artist, BPM and key like
/// every other table, and rebuilding that on the frontend from a list of hrefs
/// would be a second implementation of what `apply_tags`/`apply_analysis`
/// already do.
///
/// An href with no matching row is skipped rather than rendered blank — it
/// means the file left the library since it was added, and a row that cannot be
/// played is worse than an absent one. The count difference is visible because
/// the playlist's own length is shown beside it.
#[tauri::command]
pub fn playlist_rows(id: String, state: State<'_, Shared>) -> Result<Vec<Row>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let Some(playlist) = app.playlists.get(&id) else {
        return Ok(Vec::new());
    };

    Ok(rows_in_order(&app.rows, &playlist.tracks)
        .into_iter()
        .cloned()
        .map(|mut row| {
            app.apply_tags(&mut row);
            app.apply_analysis(&mut row);
            row
        })
        .collect())
}

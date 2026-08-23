//! Settings commands — the switches, the server, and its password.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// Daylight, Lamplight, or follow the OS.
///
/// Rejected rather than repaired, unlike a value read off disk: a word that is
/// not one of the three did not come from the control, so writing it would
/// persist a state the UI has no way to show. `sanitised` still has to cope
/// with the same problem on load, because a settings file is a thing people
/// edit, but a live command is not.
#[tauri::command]
pub fn set_appearance(appearance: String, state: State<'_, Shared>) -> Result<Settings> {
    let choice = appearance.trim().to_ascii_lowercase();
    if !APPEARANCES.contains(&choice.as_str()) {
        return Err(Error(format!(
            "unknown appearance {appearance:?}; expected one of {}",
            APPEARANCES.join(", ")
        )));
    }
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.theme = choice;
    app.save_settings()?;
    Ok(app.settings.clone())
}

#[tauri::command]
pub fn set_hide_duplicates(enabled: bool, state: State<'_, Shared>) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.hide_duplicates = enabled;
    app.save_settings()?;
    Ok(app.settings.clone())
}

/// Turn lookups on or off.
///
/// Switching it off forgets what was found as well as stopping further
/// requests: leaving a cache of third-party data behind after someone has said
/// no is not what "off" means to the person pressing it.
#[tauri::command]
pub fn set_metadata_lookup(enabled: bool, state: State<'_, Shared>) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.metadata_lookup_enabled = enabled;
    if !enabled {
        app.looked.clear();
        app.save_looked()?;
        // The downloaded sleeves go too. Forgetting the index and leaving the
        // pictures on the disk would make "off" a claim the data directory
        // contradicts — and `Your data` invites people to go and look.
        let _ = std::fs::remove_dir_all(app.store.dir().join("metadata_images"));
    }
    app.save_settings()?;
    Ok(app.settings.clone())
}

#[tauri::command]
pub fn settings(state: State<'_, Shared>) -> Result<Settings> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.settings.clone())
}

/// Point the app at a server.
///
/// Separate from the password, which never touches this struct — see
/// `save_webdav_password`. Without this command the app had no way to be
/// configured at all: `settings` could report a server and nothing could set
/// one.
#[tauri::command]
pub fn set_remote_config(
    url: String,
    username: String,
    folder: String,
    state: State<'_, Shared>,
) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    apply_remote_config(&mut app, &url, &username, &folder)?;
    Ok(app.settings.clone())
}

/// Whether a password is stored for `username`.
///
/// So the Settings screen can say which state it is in rather than always
/// showing "unchanged", which claims a credential exists whether or not one
/// does. Only the fact is returned; the password stays in `webdav`.
#[tauri::command]
pub fn has_webdav_password(username: String) -> bool {
    webdav::has_password(username.trim())
}

/// Save the WebDAV password to the OS keychain.
///
/// Separate from the rest of settings on purpose: the credential is the one
/// piece of state that must never be written to a settings file, and keeping
/// its command separate makes that hard to undo by accident.
#[tauri::command]
pub fn save_webdav_password(username: String, password: String) -> Result<()> {
    webdav::save_password(&username, &password).map_err(|e| Error(e.to_string()))
}

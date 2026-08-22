//! Artwork commands — covers, portraits, thumbnails, and looking them up.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// The embedded cover for one track, if analysis has read it.
///
/// One call per card rather than a field on every row. Artwork is capped at
/// 2 MB by the tag reader, so a 563-row `library_view` carrying covers would
/// move hundreds of megabytes through IPC on every keystroke.
#[tauri::command]
pub fn track_cover(href: String, state: State<'_, Shared>) -> Result<Option<String>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.covers.get(&href))
}

/// The same cover at row size (PERF-004).
///
/// A table row draws artwork at 48 px, and handing it the full stored cover is
/// what made opening Songs pause — see `Covers::thumb` for the measurements.
/// Now Playing and Liner Notes still ask for the full one, which is the only
/// place the resolution is wanted.
#[tauri::command]
pub fn track_thumb(href: String, state: State<'_, Shared>) -> Result<Option<String>> {
    // The lock is taken to find *where* covers live and dropped before any
    // image is touched. Generating one is about 38 ms — measured — and a
    // screenful of rows asks for 26 at once; holding the state across that
    // would freeze the transport and every other command for a second, which
    // is the fault this change removes rather than one to move somewhere else.
    let dir = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.covers.dir().to_path_buf()
    };
    Ok(covers::Covers::new(dir).thumb(&href))
}

/// A looked-up image as a `data:` URI, read from the file it was cached in.
///
/// Takes a URL rather than an href because one sleeve serves every track on
/// the album, and the file is named after the URL for that reason.
///
/// **Only a URL that a previous lookup actually stored will be served.**
/// Without that check this command is a general "read any file the app can
/// name" primitive reachable from the webview, and its argument comes from the
/// page.
#[tauri::command]
pub fn looked_up_image(url: String, state: State<'_, Shared>) -> Result<Option<String>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let known = app
        .looked
        .values()
        .any(|l| l.artist_image == url || l.album_art == url);
    if !known || url.is_empty() {
        return Ok(None);
    }
    Ok(metadata::image_data_uri(&metadata::image_path(
        app.store.dir(),
        &url,
    )))
}

/// Artwork for one album, resolved. See [`resolve_album_cover`].
#[tauri::command]
pub fn album_cover(album: String, lead: String, state: State<'_, Shared>) -> Result<AlbumArt> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(album_art(&app, &album, &lead))
}

/// Search the services for an album's artwork and use what comes back.
///
/// **Deliberately not gated on `metadata_lookup_enabled`.** That setting
/// governs whether the app may go looking *on its own*; this is someone
/// pressing a button labelled to say what it does, which is the asking that the
/// setting exists to require. Gating it would mean answering "search for the
/// real cover" with "turn on a setting first", for a request that is already
/// the consent.
///
/// The choice is recorded so it survives a restart and so the next track on the
/// album resolves to it too.
#[tauri::command]
pub async fn find_album_art(
    album: String,
    artist: String,
    lead: String,
    state: State<'_, Shared>,
) -> Result<AlbumArt> {
    let dir = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.store.dir().to_path_buf()
    };

    /*
     * On a blocking thread, not in the async body.
     *
     * Two faults in one line. `Lookup::new` builds a
     * `reqwest::blocking::Client`, which cannot be constructed on a runtime
     * worker without panicking the process — see `crate::http`. And the calls
     * themselves are blocking network I/O run directly on the runtime, which
     * is what the comment this replaced claimed to have avoided by dropping
     * the lock: the lock was released and the *runtime* was held instead, so
     * every other command queued behind this one regardless.
     *
     * `crate::http` makes the first survivable wherever it happens. This is
     * what stops it happening here, and gives the runtime its worker back for
     * the seconds the lookup spends on the network.
     */
    let (query_album, query_artist) = (album.clone(), artist.clone());
    let url = tauri::async_runtime::spawn_blocking(move || {
        let lookup = metadata::Lookup::new()?;
        let (url, _genre) = lookup.album(&query_artist, &query_album);
        if !url.is_empty() {
            lookup.download_image(&url, &dir);
        }
        Ok::<String, String>(url)
    })
    .await
    .map_err(|e| Error(e.to_string()))?
    .map_err(Error)?;

    if url.is_empty() {
        return Err(Error(format!(
            "Nothing came back for “{album}”. The album or artist may be \
             spelled differently on the service than in your tags."
        )));
    }

    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.set_album_art(&album, &lead, &url);
    app.save_settings()?;
    Ok(album_art(&app, &album, &lead))
}

/// Forget a hand-chosen cover and go back to what the file carries.
#[tauri::command]
pub fn clear_album_art(album: String, lead: String, state: State<'_, Shared>) -> Result<AlbumArt> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if app.settings.set_album_art(&album, &lead, "") {
        app.save_settings()?;
    }
    Ok(album_art(&app, &album, &lead))
}

/// Whether looked-up artwork outranks the file's own.
#[tauri::command]
pub fn set_prefer_looked_up_art(enabled: bool, state: State<'_, Shared>) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.prefer_looked_up_art = enabled;
    app.save_settings()?;
    Ok(app.settings.clone())
}

/// A looked-up portrait for an artist, as a `data:` URI (TD-53).
///
/// Keyed by name rather than by href, because that is what an artist is here —
/// the Artists tab has a name and a lead track, and the portrait was fetched
/// against whichever track happened to be looked up first. Any track by that
/// artist will do, so the first one that carries a picture answers for all of
/// them.
///
/// `None` is the ordinary case: nothing has been looked up, or lookups are off
/// entirely. The tile draws its placeholder, which is what it does for a track
/// with no embedded cover too.
#[tauri::command]
pub fn artist_portrait(name: String, state: State<'_, Shared>) -> Result<Option<String>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    if name.trim().is_empty() || !app.settings.metadata_lookup_enabled {
        return Ok(None);
    }

    let url = app.rows.iter().find_map(|row| {
        if row.artist != name {
            return None;
        }
        app.looked
            .get(&row.href)
            .filter(|l| !l.artist_image.is_empty())
            .map(|l| l.artist_image.clone())
    });

    Ok(url.and_then(|url| metadata::image_data_uri(&metadata::image_path(app.store.dir(), &url))))
}

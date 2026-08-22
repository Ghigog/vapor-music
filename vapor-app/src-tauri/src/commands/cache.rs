//! Cache commands — what is held locally, and letting go of it.

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::{CacheStatus, Error, Result, Shared};

#[tauri::command]
pub fn cache_status(state: State<'_, Shared>) -> Result<CacheStatus> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let cached = app
        .rows
        .iter()
        .filter(|r| app.cache.contains(&r.href))
        .count();
    Ok(CacheStatus {
        bytes: app.cache.size(),
        max_bytes: app.cache.max_bytes(),
        tracks_cached: cached,
        tracks_total: app.rows.len(),
        location: app.cache.dir().display().to_string(),
    })
}

/// Change how much of the device the cache may use.
///
/// Takes effect for every later fetch, and trims immediately rather than
/// waiting for the next download: someone who has just lowered the bound to
/// reclaim space expects the space back now, not eventually.
#[tauri::command]
pub fn set_cache_max_bytes(bytes: u64, state: State<'_, Shared>) -> Result<u64> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

    app.settings.cache_max_bytes = bytes;
    // The core decides what is too small to be worth having, so the answer is
    // the same here as it would be for a hand-edited settings file.
    app.settings = std::mem::take(&mut app.settings).sanitised();
    let applied = app.settings.cache_max_bytes;

    let dir = app.cache.dir().to_path_buf();
    app.cache = crate::cache::Cache::new(dir, applied);
    app.cache.trim().map_err(|e| Error(e.to_string()))?;
    app.save_settings()?;

    Ok(applied)
}

/// Empty the audio cache, keeping everything else.
///
/// Distinct from "delete everything": the cached audio is the only part of the
/// data directory that is *re-fetchable*, and it is the part that gets large.
/// Someone reclaiming space wants it gone; they do not want to lose ten minutes
/// of analysis, their playlists and their server password with it.
#[tauri::command]
pub fn clear_audio_cache(state: State<'_, Shared>) -> Result<u64> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let freed = app.cache.size();

    app.cache.clear().map_err(|e| Error(e.to_string()))?;
    // Anything mid-flight is now pointing at files that no longer exist.
    app.generation += 1;
    app.loading = false;
    app.armed_next = None;
    // The cued track's decoder is reading a file that has just been deleted.
    // The track that is *playing* keeps its decoder: its file handle is already
    // open, and stopping the music because someone reclaimed disk space would
    // be a worse answer than letting the song finish.
    app.next_stream = None;
    app.drift = None;
    if let Some(p) = app.player.as_ref() {
        p.cancel_transition();
    }

    Ok(freed)
}

/// Drop one track's local copy, keeping its analysis.
///
/// Analysis is small and expensive; the audio is large and cheap to re-fetch.
/// Evicting them together would throw away ten minutes of work to reclaim
/// space that the audio alone accounts for.
#[tauri::command]
pub fn evict_track(href: String, state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.cache.remove(&href).map_err(|e| Error(e.to_string()))
}

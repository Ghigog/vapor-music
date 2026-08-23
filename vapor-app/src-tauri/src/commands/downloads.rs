//! Download commands — keeping a collection's audio on the device.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// Every track whose audio is kept on the device.
#[tauri::command]
pub fn downloaded_tracks(state: State<'_, Shared>) -> Result<Vec<String>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.pinned.iter().cloned().collect())
}

/// Download every track in a playlist or dynamic group, and keep them.
///
/// Reported per track rather than as one long wait: on a slow connection this
/// is minutes, and a button that goes quiet for minutes has failed as far as
/// anyone can tell.
#[tauri::command]
pub async fn download_collection(
    app_handle: tauri::AppHandle,
    kind: String,
    id: String,
    state: State<'_, Shared>,
) -> Result<()> {
    use tauri::Emitter;

    let (hrefs, remote, dir, max, roots) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        (
            collection_tracks(&app, &kind, &id),
            app.settings.remote.clone(),
            app.cache.dir().to_path_buf(),
            app.cache.max_bytes(),
            local::roots(&app.settings.folders),
        )
    };

    if hrefs.is_empty() {
        return Err(Error(
            "There are no tracks in that to download.".to_string(),
        ));
    }

    let shared: Shared = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let cache = cache::Cache::new(dir, max, roots);
        let total = hrefs.len();

        let fetcher = match webdav::Fetcher::new(&remote) {
            Ok(f) => f,
            Err(e) => {
                let _ = app_handle.emit(
                    "download-progress",
                    DownloadProgress {
                        done: 0,
                        total,
                        title: String::new(),
                        finished: true,
                        error: e,
                    },
                );
                return;
            }
        };

        for (i, href) in hrefs.iter().enumerate() {
            let title = shared
                .lock()
                .ok()
                .and_then(|a| {
                    a.rows
                        .iter()
                        .find(|r| &r.href == href)
                        .map(|r| r.title.clone())
                })
                .unwrap_or_default();

            let outcome = cache.download(href, || fetcher.fetch(href));

            if outcome.is_ok() {
                if let Ok(mut app) = shared.lock() {
                    app.pinned.insert(href.clone());
                    let _ = app.save_pinned();
                }
            }

            let _ = app_handle.emit(
                "download-progress",
                DownloadProgress {
                    done: i + 1,
                    total,
                    title,
                    finished: i + 1 == total,
                    error: outcome.err().map(|e| e.to_string()).unwrap_or_default(),
                },
            );
        }
    });

    Ok(())
}

/// Stop keeping a collection's tracks.
///
/// Only where nothing else keeps them: a track in two downloaded playlists
/// stays until both are removed.
#[tauri::command]
pub fn remove_download(kind: String, id: String, state: State<'_, Shared>) -> Result<usize> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let hrefs = collection_tracks(&app, &kind, &id);

    let mut wanted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in app.playlists.all() {
        if !(kind == "playlist" && p.id == id) {
            wanted.extend(p.tracks.iter().cloned());
        }
    }

    let mut removed = 0usize;
    for href in hrefs {
        if wanted.contains(&href) {
            continue;
        }
        if app.pinned.remove(&href) {
            let _ = app.cache.remove_download(&href);
            removed += 1;
        }
    }
    app.save_pinned()?;
    Ok(removed)
}

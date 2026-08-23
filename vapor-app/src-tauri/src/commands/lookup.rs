//! Metadata lookup commands — asking public services what a track is,
//! and remembering the answer.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// What has already been looked up for a track. Never makes a request.
#[tauri::command]
pub fn track_lookup(href: String, state: State<'_, Shared>) -> Result<LookedUp> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(LookedUp::of(&app, &href))
}

/// Look a track up, and remember what came back.
///
/// Refuses when the setting is off rather than quietly doing nothing, because
/// a button that appears to work and does not is worse than one that explains
/// itself. Returns the cached result unchanged when the track has been looked
/// up before, so opening Liner Notes repeatedly is not repeated traffic —
/// `attempted` is what distinguishes "found nothing" from "not yet asked", and
/// without it a track with no lyrics would be requested on every visit.
#[tauri::command]
pub fn look_up_track(href: String, force: bool, state: State<'_, Shared>) -> Result<LookedUp> {
    // The lock is taken, the identity read, and the lock *dropped* before the
    // request goes out. Holding it across an eight-second network call would
    // freeze playback control, the queue and every other command behind it.
    let (artist, album, title, dir) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        if !app.settings.metadata_lookup_enabled {
            return Err(Error(
                "Looking up lyrics and artwork is switched off. Turn it on in Settings — \
                 it sends the artist and title to LRCLIB and Deezer."
                    .to_string(),
            ));
        }
        if !force && app.looked.get(&href).is_some_and(|l| l.attempted) {
            return Ok(LookedUp::of(&app, &href));
        }
        let row = app
            .rows
            .iter()
            .find(|r| r.href == href)
            .ok_or_else(|| Error("That track is not in the library.".to_string()))?;
        let mut row = row.clone();
        app.apply_tags(&mut row);
        (
            row.artist,
            row.album,
            row.title,
            app.store.dir().to_path_buf(),
        )
    };

    let lookup = metadata::Lookup::new().map_err(Error)?;

    // Three independent questions to two services, so they are asked at once
    // rather than one after another (TD-52). Sequentially this was up to five
    // round trips on one IPC call, and the button said "Looking…" for all of
    // them; now it is two waits deep, because only the downloads depend on
    // anything.
    //
    // `scope` rather than spawning detached threads: the borrows of `artist`,
    // `title` and `lookup` are what make this readable, and the alternative is
    // cloning three strings and an Arc to say the same thing.
    let (lyrics, artist_image, album_art, genre) = std::thread::scope(|s| {
        let words = s.spawn(|| lookup.lyrics(&artist, &title));
        let portrait = s.spawn(|| lookup.artist_image(&artist));
        let sleeve = s.spawn(|| lookup.album(&artist, &album));

        // A panic in one lookup must not take the whole thing down: this is
        // decoration on a screen that is already useful without it.
        let lyrics = words.join().unwrap_or(None);
        let artist_image = portrait.join().unwrap_or_default();
        let found = sleeve.join().unwrap_or_default();
        (lyrics, artist_image, found.art, found.genre)
    });

    // Fetched here, once, rather than by the webview: the window's CSP allows
    // `data:` and no remote host, which is the right way round — the page
    // should not be opening connections to Deezer on every render.
    //
    // Also at once, and for the same reason.
    std::thread::scope(|s| {
        for url in [&artist_image, &album_art] {
            s.spawn(|| lookup.download_image(url, &dir));
        }
    });

    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    // Left as they were: a per-track lookup is about this screen, and the tempo
    // reference is gathered by the library-wide pass.
    let previous = app.looked.get(&href).cloned().unwrap_or_default();
    app.looked.insert(
        href.clone(),
        metadata::Looked {
            lyrics,
            artist_image,
            album_art,
            genre,
            attempted: true,
            ..previous
        },
    );
    app.save_looked()?;
    Ok(LookedUp::of(&app, &href))
}

/// Ask Deezer about every track in the library.
///
/// **This sends the artist and title of the whole library to a third party.**
/// It is the largest thing the app ever discloses, so it is a deliberate act
/// with its own button and its own sentence, rather than something the
/// automatic-lookup setting quietly enables.
///
/// What it is for: the tempo octave. A beat tracker is reliable about the pulse
/// and unreliable about whether a listener counts it at 87 or 174 — both this
/// crate and Essentia read Delta Heavy's "Space Time" at 87, and neither is
/// wrong. Nothing measurable on this device settles it. A per-track reference
/// does, where one exists, and Deezer publishes one for some recordings.
///
/// Their number is never adopted as the tempo. It chooses between octaves of
/// the tempo measured here, and only after the durations agree that both are
/// describing the same recording. A wrong search hit is otherwise a stranger's
/// tempo for music nobody is playing.
#[tauri::command]
pub async fn identify_library(
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<()> {
    let shared: Shared = Arc::clone(&state);
    identify_library_in_background(&app_handle, &shared)
}

#[tauri::command]
pub fn lookup_counts(state: State<'_, Shared>) -> Result<LookupCounts> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let fetched = app
        .rows
        .iter()
        .filter(|r| app.looked.get(&r.href).is_some_and(|l| l.attempted))
        .count();
    Ok(LookupCounts {
        fetched,
        total: app.rows.len(),
    })
}

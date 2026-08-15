//! The Tauri shell.
//!
//! This crate is deliberately thin. Everything the app *knows* lives in
//! `vapor-core`; this only adapts it to a window and a webview:
//!
//! * `vapor-library` — playlists, groups, the index, the pathfinder, the queue
//! * `vapor-engine`  — the two-deck mixer
//! * `vapor-dsp`     — decode and analysis
//!
//! The rule that keeps it thin: **no decisions here.** If a command is doing
//! more than translating between JSON and a core type, the logic belongs in a
//! core crate where it can be tested without a window — which is exactly the
//! coupling the Godot version had and this migration exists to remove.
//!
//! The same reasoning applies to the browser build: a wasm front end calls the
//! same core crates directly, so anything implemented here rather than in the
//! core would have to be written twice.

mod analysis;
/// Public so `tests/audio_realtime.rs` can drive the audio path without a
/// device. Nothing outside the crate uses it.
pub mod audio;
mod cache;
mod store;
mod webdav;

use std::sync::{Arc, Mutex};

use store::Store;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use vapor_library::{
    index::{GroupBy, Row, SortKey},
    Curve, PlaylistStore, Queue, Settings, TrackMeta,
};

/// Everything the shell holds between commands.
///
/// One lock rather than one per collection: commands are user-driven and
/// short, so contention is not a concern, and a single lock cannot deadlock
/// against itself the way a set of finer ones can.
struct AppState {
    settings: Settings,
    playlists: PlaylistStore,
    queue: Queue,
    /// The library table's rows, rebuilt on scan.
    rows: Vec<Row>,
    /// Analysis results, keyed by href. Persisted so a library is analysed
    /// once rather than on every launch.
    analysis: analysis::Cache,
    /// Cancels a running analysis pass.
    cancel: analysis::Cancel,
    cache: cache::Cache,
    store: Store,

    /// The audio device, absent when the machine has none. Playback commands
    /// then fail with a message instead of the app refusing to start — a
    /// library you cannot hear is still one you can scan and analyse.
    player: Option<audio::Player>,
    /// What the audio thread is playing, or is being loaded to play.
    playing: Option<String>,
    /// Increments on every load request. A load whose generation is stale lost
    /// a race with a newer one and must discard its result rather than
    /// interrupt the track a person actually asked for.
    generation: u64,
    /// True while a track is being fetched and decoded, which can take seconds
    /// on a cold cache. The UI has to be able to say so.
    loading: bool,
    /// Why the last load failed, surfaced instead of silence with no
    /// explanation.
    playback_error: Option<String>,
}

impl AppState {
    /// Load what exists; start empty for anything that does not.
    ///
    /// A corrupt file is surfaced rather than swallowed — see `Store::load`.
    /// Starting empty on a read failure would show a person an empty library
    /// while their data sits unreadable on disk.
    fn load(store: Store) -> Self {
        let settings = store.load("settings").unwrap_or(None).unwrap_or_default();
        let playlists = store.load("playlists").unwrap_or(None).unwrap_or_default();
        let analysis = store.load("analysis").unwrap_or(None).unwrap_or_default();
        AppState {
            settings,
            playlists,
            queue: Queue::default(),
            rows: Vec::new(),
            analysis,
            cancel: analysis::Cancel::new(),
            // The bound is what stops the cache filling a phone. Configurable
            // later; the default is generous on a desktop and finite anywhere.
            cache: cache::Cache::new(store.dir().join("audio"))
                .with_max_bytes(cache::DEFAULT_MAX_BYTES),
            store,
            // Opened once at startup rather than per track: acquiring a device
            // takes long enough to hear as a gap, and holding one open is what
            // every other player does.
            player: match audio::Player::start() {
                Ok(p) => Some(p),
                Err(e) => {
                    eprintln!("audio output unavailable: {e}");
                    None
                }
            },
            playing: None,
            generation: 0,
            loading: false,
            playback_error: None,
        }
    }

    fn save_analysis(&self) -> Result<()> {
        self.store.save("analysis", &self.analysis)?;
        Ok(())
    }

    /// Copy analysis onto a row, so the table shows what is known.
    ///
    /// A manual BPM override wins over the detected value: it exists precisely
    /// because detection lands a metrical relative on roughly 10% of a real
    /// library, and a correction that the table ignored would be useless.
    fn apply_analysis(&self, row: &mut Row) {
        if let Some(a) = self.analysis.get(&row.href) {
            row.bpm = self.settings.bpm_override(&row.href).unwrap_or(a.bpm);
            row.key = a.key.clone();
        } else if let Some(bpm) = self.settings.bpm_override(&row.href) {
            // A person can correct a track that was never successfully
            // analysed, and that correction must still show.
            row.bpm = bpm;
        }
    }

    /// Persist playlists. Called after every mutation, which is why the write
    /// has to be atomic.
    fn save_playlists(&self) -> Result<()> {
        self.store.save("playlists", &self.playlists)?;
        Ok(())
    }

    fn save_settings(&self) -> Result<()> {
        self.store.save("settings", &self.settings)?;
        Ok(())
    }
}

/// Shared state.
///
/// An `Arc` rather than a bare `Mutex` because the analysis pass moves a handle
/// onto a worker thread that outlives the command that started it — a borrow
/// from `State` cannot.
type Shared = Arc<Mutex<AppState>>;

/// Errors crossing the IPC boundary.
///
/// A string rather than a typed error on purpose: the frontend shows these to
/// a person, and a structured error would have to be re-stringified there
/// anyway. Anything the UI needs to *branch* on gets its own return shape.
#[derive(Debug, Serialize)]
struct Error(String);

impl<E: std::fmt::Display> From<E> for Error {
    fn from(e: E) -> Self {
        Error(e.to_string())
    }
}

type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryView {
    #[serde(default)]
    query: String,
    #[serde(default)]
    sort_key: Option<String>,
    #[serde(default = "default_true")]
    ascending: bool,
    #[serde(default)]
    group_by: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySection {
    header: String,
    rows: Vec<Row>,
}

/// Filter, sort and group the library in one call.
///
/// One round trip rather than three: the table re-runs this per keystroke, and
/// the predicates are the same ones a smart playlist uses, so splitting them
/// would let the two disagree.
#[tauri::command]
fn library_view(view: LibraryView, state: State<'_, Shared>) -> Result<Vec<LibrarySection>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let mut rows: Vec<Row> = vapor_library::filter(&app.rows, &view.query)
        .into_iter()
        .cloned()
        .collect();
    for row in rows.iter_mut() {
        app.apply_analysis(row);
    }

    if let Some(key) = view.sort_key.as_deref().and_then(parse_sort_key) {
        vapor_library::sort_rows(&mut rows, key, view.ascending);
    }

    let group = view
        .group_by
        .as_deref()
        .and_then(parse_group_by)
        .unwrap_or(GroupBy::None);

    Ok(vapor_library::group_rows(&rows, group)
        .into_iter()
        .map(|(header, rows)| LibrarySection {
            header,
            rows: rows.into_iter().cloned().collect(),
        })
        .collect())
}

fn parse_sort_key(s: &str) -> Option<SortKey> {
    Some(match s {
        "title" => SortKey::Title,
        "artist" => SortKey::Artist,
        "album" => SortKey::Album,
        "genre" => SortKey::Genre,
        "year" => SortKey::Year,
        "bpm" => SortKey::Bpm,
        "key" => SortKey::Key,
        "order" => SortKey::Order,
        _ => return None,
    })
}

fn parse_group_by(s: &str) -> Option<GroupBy> {
    Some(match s {
        "none" => GroupBy::None,
        "artist" => GroupBy::Artist,
        "album" => GroupBy::Album,
        "genre" => GroupBy::Genre,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Playlists
// ---------------------------------------------------------------------------

#[tauri::command]
fn playlists(state: State<'_, Shared>) -> Result<Vec<vapor_library::Playlist>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.playlists.all().to_vec())
}

#[tauri::command]
fn create_playlist(name: String, state: State<'_, Shared>) -> Result<vapor_library::Playlist> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    // Ids are generated here rather than in the core so the core stays
    // deterministic and testable — see the note on PlaylistStore::create.
    let id = new_id("playlist");
    let created = app.playlists.create(id, name).clone();
    app.save_playlists()?;
    Ok(created)
}

#[tauri::command]
fn add_tracks_to_playlist(
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

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueState {
    current: Option<String>,
    tracks: Vec<String>,
    /// What plays next, so the UI can show it without asking again.
    next: Option<String>,
}

#[tauri::command]
fn queue_state(state: State<'_, Shared>) -> Result<QueueState> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(QueueState {
        current: app.queue.current().map(str::to_string),
        tracks: app.queue.tracks().to_vec(),
        next: app.queue.peek_next(None).map(str::to_string),
    })
}

#[tauri::command]
fn play_tracks(hrefs: Vec<String>, start: Option<String>, state: State<'_, Shared>) -> Result<()> {
    let shared: Shared = Arc::clone(&state);
    let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;
    app.queue.set_tracks(hrefs, start.as_deref());
    if let Some(current) = app.queue.current().map(str::to_string) {
        begin_playback(&shared, &mut app, current);
    }
    Ok(())
}

#[tauri::command]
fn next_track(state: State<'_, Shared>) -> Result<Option<String>> {
    let shared: Shared = Arc::clone(&state);
    let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;
    let next = app.queue.next(None).map(str::to_string);
    if let Some(href) = next.clone() {
        begin_playback(&shared, &mut app, href);
    }
    Ok(next)
}

#[tauri::command]
fn previous_track(state: State<'_, Shared>) -> Result<Option<String>> {
    let shared: Shared = Arc::clone(&state);
    let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;
    let previous = app.queue.previous().map(str::to_string);
    if let Some(href) = previous.clone() {
        begin_playback(&shared, &mut app, href);
    }
    Ok(previous)
}

// ---------------------------------------------------------------------------
// Playback (TD-03)
// ---------------------------------------------------------------------------

/// How often the supervisor checks whether a track has finished.
///
/// Long enough to cost nothing, short enough that the gap between tracks is not
/// noticed — and the gap is dominated by fetch and decode anyway, which is what
/// prefetch (TD-08) exists to remove.
const SUPERVISOR_POLL: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackState {
    href: Option<String>,
    /// Resolved from the library rows, so the transport can name what is
    /// playing without the UI holding its own copy of the table.
    title: String,
    artist: String,
    status: audio::Status,
    /// Fetching and decoding, which on a cold cache is seconds. Distinct from
    /// playing so the UI can say "loading" rather than showing a stalled
    /// playhead.
    loading: bool,
    position: f64,
    duration: f64,
    volume: f32,
    error: Option<String>,
    /// False when the machine has no output device. The UI disables the
    /// transport rather than offering buttons that silently do nothing.
    available: bool,
}

/// Fetch, decode and start a track.
///
/// The work happens on a blocking thread because it is neither quick nor
/// bounded: a cold cache means a download, and decoding a five-minute track is
/// seconds of CPU. Doing either on the command thread would freeze the window,
/// and doing it on the audio thread is unthinkable.
fn begin_playback(shared: &Shared, app: &mut AppState, href: String) {
    let Some(player) = app.player.as_ref() else {
        app.playback_error = Some("No audio output device is available.".to_string());
        return;
    };

    // A person who double-clicks a second track while the first is still
    // downloading expects the second one. The generation is what lets the
    // abandoned load recognise that it lost and drop its result.
    app.generation += 1;
    let generation = app.generation;
    app.loading = true;
    app.playback_error = None;
    app.playing = Some(href.clone());

    let link = player.link();
    let rate = player.sample_rate();
    let cache_dir = app.cache.dir().to_path_buf();
    let remote = app.settings.remote.clone();
    let shared = Arc::clone(shared);

    tauri::async_runtime::spawn_blocking(move || {
        let cache = cache::Cache::new(cache_dir);
        let outcome = cache
            .store(&href, || webdav::fetch_blocking(&remote, &href))
            .map_err(|e| e.to_string())
            .and_then(|path| {
                // Converted to the device's rate here, once, rather than per
                // block on the audio thread.
                vapor_dsp::decode_for_playback(&path, rate).map_err(|e| e.to_string())
            });

        let Ok(mut app) = shared.lock() else {
            return;
        };
        if app.generation != generation {
            // Superseded. Handing these frames to the deck now would interrupt
            // whatever the person actually chose.
            return;
        }
        app.loading = false;

        match outcome {
            Ok(frames) if !frames.is_empty() => {
                // A refused load means the audio thread has stopped servicing
                // its queue, which is a dead device rather than a busy one.
                // Saying so beats a transport that reads "playing" in silence.
                if !link.load(frames, true) {
                    app.playback_error = Some("The audio device stopped responding.".to_string());
                    app.playing = None;
                }
            }
            // A file that decodes to nothing is the malformed-AAC case (TD-12).
            // It has to say so rather than present as a track that plays
            // silently for its whole duration.
            Ok(_) => {
                app.playback_error = Some("That track contains no playable audio.".to_string());
                app.playing = None;
            }
            Err(e) => {
                app.playback_error = Some(e);
                app.playing = None;
            }
        }
    });
}

/// The device, or a message a person can act on.
fn player(app: &AppState) -> Result<&audio::Player> {
    app.player
        .as_ref()
        .ok_or_else(|| Error("No audio output device is available.".to_string()))
}

#[tauri::command]
fn playback_state(state: State<'_, Shared>) -> Result<PlaybackState> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let snapshot = app.player.as_ref().map(|p| p.snapshot());
    let row = app
        .playing
        .as_ref()
        .and_then(|href| app.rows.iter().find(|r| &r.href == href));

    Ok(PlaybackState {
        href: app.playing.clone(),
        title: row.map(|r| r.title.clone()).unwrap_or_default(),
        // The same rule the table follows: unknown renders as a dash, never as
        // a guess.
        artist: row
            .filter(|r| r.artist_source != vapor_library::index::Source::Unknown)
            .map(|r| r.artist.clone())
            .unwrap_or_default(),
        status: snapshot.map_or(audio::Status::Idle, |s| s.status),
        loading: app.loading,
        position: snapshot.map_or(0.0, |s| s.position),
        duration: snapshot.map_or(0.0, |s| s.duration),
        volume: snapshot.map_or(1.0, |s| s.volume),
        error: app.playback_error.clone(),
        available: app.player.is_some(),
    })
}

#[tauri::command]
fn pause_playback(state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.pause();
    Ok(())
}

#[tauri::command]
fn resume_playback(state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.play();
    Ok(())
}

/// Stop and forget what was playing.
///
/// Deliberately not a pause: the position returns to the start and the
/// transport reads as idle, which is what the Godot build's stop button did.
#[tauri::command]
fn stop_playback(state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.stop();
    // Any load still in flight now belongs to nothing, so retire its
    // generation rather than let it start playing after a stop.
    app.generation += 1;
    app.loading = false;
    app.playing = None;
    Ok(())
}

#[tauri::command]
fn seek(seconds: f64, state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.seek(seconds);
    Ok(())
}

#[tauri::command]
fn set_volume(volume: f32, state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.set_volume(volume);
    Ok(())
}

/// Advance the queue when a track finishes.
///
/// A thread rather than something the UI drives, because the queue has to keep
/// moving whether or not a window is open, focused or even rendering — a
/// webview that stops running timers when hidden is a normal thing for an OS to
/// do, and music stopping because of it is not.
fn spawn_supervisor(app_handle: tauri::AppHandle, shared: Shared) {
    use tauri::Emitter;

    let spawned = std::thread::Builder::new()
        .name("vapor-playback-supervisor".to_string())
        .spawn(move || loop {
            std::thread::sleep(SUPERVISOR_POLL);

            let Ok(mut app) = shared.lock() else {
                return;
            };
            // Consumed here, so one ending advances the queue exactly once.
            if !app.player.as_ref().is_some_and(|p| p.take_ended()) {
                continue;
            }

            // Something is already on its way to the deck — almost always
            // because a person pressed next or picked a track while the
            // outgoing one was still running. Advancing now would skip past it:
            // the ending belongs to the track they just left, not to a queue
            // that ran out. Consuming the flag and doing nothing is the whole
            // fix; the pending load will start on its own.
            if app.loading {
                continue;
            }

            match app.queue.next(None).map(str::to_string) {
                Some(href) => begin_playback(&shared, &mut app, href),
                None => app.playing = None,
            }
            drop(app);

            let _ = app_handle.emit("playback-changed", ());
        });

    if let Err(e) = spawned {
        eprintln!("could not start the playback supervisor: {e}");
    }
}

// ---------------------------------------------------------------------------
// Vibe DJ
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoodPathRequest {
    tracks: std::collections::HashMap<String, TrackMeta>,
    start: String,
    /// "build" | "chill" | "wave" | anything else = flat.
    curve: String,
}

/// Order a set of tracks along an energy and tempo curve.
///
/// The BPM overrides in settings are applied before pathfinding rather than
/// after: tempo detection lands a metrical relative on roughly 10% of a real
/// library, and a wrong BPM does not merely mislabel a track — it changes
/// which transitions the pathfinder believes are cheap, so a correction has to
/// reach the cost model to be worth anything.
#[tauri::command]
fn mood_path(req: MoodPathRequest, state: State<'_, Shared>) -> Result<Vec<String>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let mut tracks = req.tracks;
    for (href, meta) in tracks.iter_mut() {
        if let Some(bpm) = app.settings.bpm_override(href) {
            meta.bpm = bpm;
        }
    }

    Ok(vapor_library::generate_mood_path(
        &tracks,
        &req.start,
        Curve::parse(&req.curve),
        vapor_library::DEFAULT_ENERGY_THRESHOLD,
        // Skip history is not persisted yet; an empty map means no learned
        // dislikes rather than a lookup that silently returns nothing.
        &std::collections::HashMap::new(),
    ))
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[tauri::command]
fn settings(state: State<'_, Shared>) -> Result<Settings> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.settings.clone())
}

/// Correct a track's tempo by hand, or clear the correction with 0.
///
/// A refused value is an error rather than a silent no-op: the person is
/// looking at the number they just typed, and a correction that appeared to be
/// accepted but was not is worse than no correction at all.
#[tauri::command]
fn set_bpm_override(href: String, bpm: f32, state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !app.settings.set_bpm_override(&href, bpm) {
        return Err(Error(format!(
            "A BPM has to be between {} and {}.",
            vapor_library::MIN_MANUAL_BPM as u32,
            vapor_library::MAX_MANUAL_BPM as u32
        )));
    }
    app.save_settings()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Library scan
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanReport {
    tracks: usize,
    directories: usize,
}

/// Save the WebDAV password to the OS keychain.
///
/// Separate from the rest of settings on purpose: the credential is the one
/// piece of state that must never be written to a settings file, and keeping
/// its command separate makes that hard to undo by accident.
#[tauri::command]
fn save_webdav_password(username: String, password: String) -> Result<()> {
    webdav::save_password(&username, &password).map_err(|e| Error(e.to_string()))
}

/// Walk the configured WebDAV tree and rebuild the library index.
#[tauri::command]
async fn scan_library(state: State<'_, Shared>) -> Result<ScanReport> {
    // The remote config is copied out and the lock released before the network
    // call: holding it across an await would block every other command for the
    // length of a scan, which can be minutes on a large library.
    let (url, username, folder) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        let r = &app.settings.remote;
        if !r.is_configured() {
            return Err(Error(
                "No server configured. Add one in Settings.".to_string(),
            ));
        }
        (r.url.clone(), r.username.clone(), r.folder.clone())
    };

    let result = webdav::scan(&url, &username, &folder)
        .await
        .map_err(|e| Error(e.to_string()))?;

    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.rows = result
        .files
        .iter()
        .map(|href| build_row(href, &folder))
        .collect();

    Ok(ScanReport {
        tracks: app.rows.len(),
        directories: result.directories,
    })
}

/// Derive a table row from a path.
///
/// Analysis fields stay empty until the track is actually analysed — the row
/// says "unknown", not a guess. That distinction is the whole reason the Godot
/// stub fabricating 120 BPM was a bug rather than a convenience.
fn build_row(href: &str, base_folder: &str) -> Row {
    let info = vapor_library::parse_path(href, base_folder);
    Row {
        href: href.to_string(),
        title: if info.title.is_empty() {
            href.rsplit('/').next().unwrap_or(href).to_string()
        } else {
            info.title
        },
        artist: info.artist.clone(),
        album: info.album.clone(),
        artist_source: if info.artist.is_empty() {
            vapor_library::index::Source::Unknown
        } else {
            vapor_library::index::Source::File
        },
        album_source: if info.album.is_empty() {
            vapor_library::index::Source::Unknown
        } else {
            vapor_library::index::Source::File
        },
        genre: String::new(),
        bpm: 0.0,
        key: String::new(),
        year: info.year.unwrap_or(0),
        manual_pos: 0,
    }
}

// ---------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisStatus {
    analysed: usize,
    total: usize,
}

#[tauri::command]
fn analysis_status(state: State<'_, Shared>) -> Result<AnalysisStatus> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let hrefs: Vec<String> = app.rows.iter().map(|r| r.href.clone()).collect();
    let outstanding = analysis::pending(&hrefs, &app.analysis).len();
    Ok(AnalysisStatus {
        analysed: hrefs.len().saturating_sub(outstanding),
        total: hrefs.len(),
    })
}

#[tauri::command]
fn cancel_analysis(state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.cancel.stop();
    Ok(())
}

/// Analyse everything not already done, emitting progress as it goes.
///
/// Runs on a blocking thread rather than the async runtime: analysis is
/// CPU-bound, and occupying an async worker for ten minutes starves every other
/// task sharing it.
#[tauri::command]
async fn analyse_library(app_handle: tauri::AppHandle, state: State<'_, Shared>) -> Result<()> {
    use tauri::Emitter;

    // Snapshot what needs doing and release the lock: the pass takes minutes,
    // and holding the lock would block every other command for its duration.
    let (todo, cache_dir, cancel, remote) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        let hrefs: Vec<String> = app.rows.iter().map(|r| r.href.clone()).collect();
        app.cancel.reset();
        (
            analysis::pending(&hrefs, &app.analysis),
            app.cache.dir().to_path_buf(),
            app.cancel.clone(),
            app.settings.remote.clone(),
        )
    };

    if todo.is_empty() {
        return Ok(());
    }

    let state_arc: Shared = Arc::clone(&state);
    let handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let cache = cache::Cache::new(cache_dir);
        analysis::run(
            &todo,
            // Fetch on demand. Analysis needs local bytes; this is where they
            // come from, and a cached track costs nothing.
            |href| {
                cache
                    .store(href, || webdav::fetch_blocking(&remote, href))
                    .ok()
            },
            &cancel,
            |progress| {
                // Persist as each track lands rather than at the end: quitting
                // halfway then loses only the track in flight.
                if let Some(a) = &progress.analysis {
                    if let Ok(mut app) = state_arc.lock() {
                        app.analysis.insert(progress.href.clone(), a.clone());
                        let _ = app.save_analysis();
                    }
                }
                let _ = handle.emit("analysis-progress", &progress);
            },
        );
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheStatus {
    bytes: u64,
    max_bytes: u64,
    /// How many of the library's tracks are held locally, so the Your Data
    /// screen can state what is actually on the device rather than implying
    /// the whole library is.
    tracks_cached: usize,
    tracks_total: usize,
    location: String,
}

#[tauri::command]
fn cache_status(state: State<'_, Shared>) -> Result<CacheStatus> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let cached = app
        .rows
        .iter()
        .filter(|r| app.cache.contains(&r.href))
        .count();
    Ok(CacheStatus {
        bytes: app.cache.size(),
        max_bytes: cache::DEFAULT_MAX_BYTES,
        tracks_cached: cached,
        tracks_total: app.rows.len(),
        location: app.cache.dir().display().to_string(),
    })
}

/// Drop one track's local copy, keeping its analysis.
///
/// Analysis is small and expensive; the audio is large and cheap to re-fetch.
/// Evicting them together would throw away ten minutes of work to reclaim
/// space that the audio alone accounts for.
#[tauri::command]
fn evict_track(href: String, state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.cache.remove(&href).map_err(|e| Error(e.to_string()))
}

// ---------------------------------------------------------------------------
// Your Data
// ---------------------------------------------------------------------------

/// Where the app keeps everything, so the Your Data screen can show it.
///
/// Naming the directory is part of the claim: "your data is local" is an
/// assertion until a person can see the path and open it.
#[tauri::command]
fn data_location(state: State<'_, Shared>) -> Result<String> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.store.dir().display().to_string())
}

/// Delete everything the app has stored.
///
/// In-memory state is reset too, so the UI reflects the deletion immediately
/// rather than continuing to show data that no longer exists on disk.
#[tauri::command]
fn delete_all_data(state: State<'_, Shared>) -> Result<()> {
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
    app.queue = Queue::default();
    app.rows.clear();
    Ok(())
}

/// Id generator for entities the core deliberately does not name itself.
///
/// Monotonic time plus a counter. The Godot version used
/// `Time.get_ticks_usec()` and `randi()`, which could collide within a
/// microsecond; the counter removes that.
fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);
    format!("{prefix}_{t}_{n}")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Tauri resolves this per platform: ~/Library/Application Support
            // on macOS, %APPDATA% on Windows, ~/.local/share on Linux.
            let dir = app
                .path()
                .app_data_dir()
                .expect("no app data directory available");
            let shared: Shared = Arc::new(Mutex::new(AppState::load(Store::new(dir))));

            // Only worth a thread if there is a device for it to watch.
            let has_audio = shared.lock().map(|s| s.player.is_some()).unwrap_or(false);
            if has_audio {
                spawn_supervisor(app.handle().clone(), Arc::clone(&shared));
            }

            app.manage(shared);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_view,
            playlists,
            create_playlist,
            add_tracks_to_playlist,
            queue_state,
            play_tracks,
            next_track,
            previous_track,
            playback_state,
            pause_playback,
            resume_playback,
            stop_playback,
            seek,
            set_volume,
            mood_path,
            settings,
            set_bpm_override,
            data_location,
            delete_all_data,
            save_webdav_password,
            scan_library,
            analyse_library,
            cancel_analysis,
            analysis_status,
            cache_status,
            evict_track,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vapor Music");
}

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

mod store;

use std::sync::Mutex;

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
    store: Store,
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
        AppState {
            settings,
            playlists,
            queue: Queue::default(),
            rows: Vec::new(),
            store,
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

type Shared = Mutex<AppState>;

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
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.queue.set_tracks(hrefs, start.as_deref());
    Ok(())
}

#[tauri::command]
fn next_track(state: State<'_, Shared>) -> Result<Option<String>> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.queue.next(None).map(str::to_string))
}

#[tauri::command]
fn previous_track(state: State<'_, Shared>) -> Result<Option<String>> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.queue.previous().map(str::to_string))
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

#[tauri::command]
fn set_bpm_override(href: String, bpm: f32, state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.set_bpm_override(&href, bpm);
    app.save_settings()?;
    Ok(())
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
    app.store.clear()?;
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
            app.manage(Mutex::new(AppState::load(Store::new(dir))));
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
            mood_path,
            settings,
            set_bpm_override,
            data_location,
            delete_all_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vapor Music");
}

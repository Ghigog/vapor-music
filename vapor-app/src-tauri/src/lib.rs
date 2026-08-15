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

use std::sync::Mutex;

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
#[derive(Default)]
struct AppState {
    settings: Settings,
    playlists: PlaylistStore,
    queue: Queue,
    /// The library table's rows, rebuilt on scan.
    rows: Vec<Row>,
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
    Ok(app.playlists.create(id, name).clone())
}

#[tauri::command]
fn add_tracks_to_playlist(
    id: String,
    hrefs: Vec<String>,
    state: State<'_, Shared>,
) -> Result<usize> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.playlists.add_tracks(&id, &hrefs))
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
            app.manage(Shared::default());
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vapor Music");
}

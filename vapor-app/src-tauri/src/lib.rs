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
/// Public for the same reason as `audio`: the real-time test drives a real
/// streaming deck rather than a stand-in for one.
pub mod decoder;
mod store;
mod sync;
mod tags;
mod webdav;

use std::sync::{Arc, Mutex};

use store::Store;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use vapor_engine::TrackSource;
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
    /// The most recent blend, as (outgoing, incoming), until it is judged.
    ///
    /// A skip is a verdict on a *transition*, so the pair has to outlive the
    /// transition: by the time a person reacts, the mixer has already swapped
    /// decks and forgotten which two tracks were involved.
    last_mix: Option<(String, String)>,
    /// When that blend finished, for the ten-second window.
    last_mix_ended: Option<std::time::Instant>,
    /// Learned dislike of specific transitions, keyed "from\u{1f}to" (TD-14).
    ///
    /// Persisted, because the whole value is that it accumulates. A tuple key
    /// cannot be a JSON object key, so the pair is joined by a unit separator —
    /// a character no href contains.
    skips: std::collections::HashMap<String, f32>,
    /// Embedded tags and artwork, keyed by href (TD-39).
    ///
    /// Persisted beside the analysis and for the same reason: reading them
    /// costs a file open, and the answer does not change unless the file does.
    tags: std::collections::HashMap<String, StoredTags>,
    /// Tracks that were read and could not be used, and why (TD-12).
    ///
    /// Persisted, because the answer does not change between launches and the
    /// alternative is downloading a broken file again to rediscover it. Only
    /// permanent failures land here — "not downloaded yet" is not one.
    failures: std::collections::HashMap<String, String>,
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
    /// The track cued on the other deck for a beat-matched mix, if one is
    /// arranged. Cleared when the mix completes or is abandoned.
    armed_next: Option<String>,

    /// The decoder threads feeding the two decks (TD-09).
    ///
    /// Held here, on the control side, for two reasons. A decoder that nobody
    /// holds is never stopped, and would go on filling a window for a track
    /// that is no longer playing. And dropping one is what joins its thread —
    /// which must happen somewhere allowed to wait, never on the audio thread.
    ///
    /// They swap roles when a transition completes, exactly as the decks do.
    playing_stream: Option<decoder::Streamer>,
    next_stream: Option<decoder::Streamer>,
    /// The drift correction running for the current mix (TD-21). Dropped when
    /// the mix ends, which stops its thread and clears the correction.
    drift: Option<sync::DriftCorrection>,
}

impl AppState {
    /// Load what exists; start empty for anything that does not.
    ///
    /// A corrupt file is surfaced rather than swallowed — see `Store::load`.
    /// Starting empty on a read failure would show a person an empty library
    /// while their data sits unreadable on disk.
    fn load(store: Store) -> Self {
        // `sanitised` on the way in, not on the way out. The core has always
        // had it and nothing called it, so a hand-edited settings file's
        // nonsense reached the app unchecked — including, now, a cache bound
        // too small to hold a track.
        let settings = store
            .load::<Settings>("settings")
            .unwrap_or(None)
            .unwrap_or_default()
            .sanitised();
        let cache_max_bytes = settings.cache_max_bytes;
        let playlists = store.load("playlists").unwrap_or(None).unwrap_or_default();
        let analysis = store.load("analysis").unwrap_or(None).unwrap_or_default();
        let failures = store.load("failures").unwrap_or(None).unwrap_or_default();
        let tags = store.load("tags").unwrap_or(None).unwrap_or_default();
        let skips = store.load("skips").unwrap_or(None).unwrap_or_default();
        AppState {
            settings,
            playlists,
            queue: Queue::default(),
            rows: Vec::new(),
            last_mix: None,
            last_mix_ended: None,
            analysis,
            skips,
            tags,
            failures,
            cancel: analysis::Cancel::new(),
            // The bound is what stops the cache filling a phone, and it is the
            // person's to set — `sanitised` has already refused a value too
            // small to be worth having.
            cache: cache::Cache::new(store.dir().join("audio"), cache_max_bytes),
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
            armed_next: None,
            playing_stream: None,
            next_stream: None,
            drift: None,
        }
    }

    fn save_analysis(&self) -> Result<()> {
        self.store.save("analysis", &self.analysis)?;
        Ok(())
    }

    fn save_skips(&self) -> Result<()> {
        self.store.save("skips", &self.skips)?;
        Ok(())
    }

    fn save_tags(&self) -> Result<()> {
        self.store.save("tags", &self.tags)?;
        Ok(())
    }

    fn save_failures(&self) -> Result<()> {
        self.store.save("failures", &self.failures)?;
        Ok(())
    }

    /// Copy analysis onto a row, so the table shows what is known.
    ///
    /// A manual BPM override wins over the detected value: it exists precisely
    /// because detection lands a metrical relative on roughly 10% of a real
    /// library, and a correction that the table ignored would be useless.
    /// Fill a row's blanks from the file's own tags (TD-39).
    ///
    /// Gaps only. A library filed as `Artist/Album/Track` is a statement about
    /// how it should be organised, and a tag that disagrees is usually the tag
    /// being wrong — a compilation track carrying its original album. So the
    /// derived value wins wherever there is one.
    fn apply_tags(&self, row: &mut Row) {
        let Some(tags) = self.tags.get(&row.href) else {
            return;
        };
        if row.artist_source == vapor_library::index::Source::Unknown {
            if let Some(artist) = &tags.artist {
                row.artist = artist.clone();
                row.artist_source = vapor_library::index::Source::File;
            }
        }
        if row.album_source == vapor_library::index::Source::Unknown {
            if let Some(album) = &tags.album {
                row.album = album.clone();
                row.album_source = vapor_library::index::Source::File;
            }
        }
        if row.genre.is_empty() {
            if let Some(genre) = &tags.genre {
                row.genre = genre.clone();
            }
        }
        if row.year == 0 {
            if let Some(year) = tags.year {
                row.year = year;
            }
        }
    }

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

/// Tags as persisted. A mirror of `tags::Tags` with serde on it, so the reader
/// stays free of a serialisation format it does not care about.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTags {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    album: Option<String>,
    #[serde(default)]
    genre: Option<String>,
    #[serde(default)]
    year: Option<u32>,
    #[serde(default)]
    comment: Option<String>,
    #[serde(default)]
    cover: Option<String>,
}

impl From<tags::Tags> for StoredTags {
    fn from(t: tags::Tags) -> Self {
        StoredTags {
            title: t.title,
            artist: t.artist,
            album: t.album,
            genre: t.genre,
            year: t.year,
            comment: t.comment,
            cover: t.cover,
        }
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
        app.apply_tags(row);
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

#[tauri::command]
fn rename_playlist(id: String, name: String, state: State<'_, Shared>) -> Result<bool> {
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
fn delete_playlist(id: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let deleted = app.playlists.delete(&id).is_some();
    if deleted {
        app.save_playlists()?;
    }
    Ok(deleted)
}

#[tauri::command]
fn remove_playlist_track(id: String, index: usize, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let removed = app.playlists.remove_track(&id, index);
    if removed {
        app.save_playlists()?;
    }
    Ok(removed)
}

#[tauri::command]
fn reorder_playlist_track(
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
fn playlist_rows(id: String, state: State<'_, Shared>) -> Result<Vec<Row>> {
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

/// The library rows for `hrefs`, in the order `hrefs` gives them.
///
/// Separate from the command so the rule it encodes can be tested: an href with
/// no row is **skipped**. A playlist stores hrefs and a file can leave the
/// library after being added, so this is a normal state rather than an error —
/// and a row that cannot be played is worse than an absent one. The count is
/// shown beside the title, so a playlist of 12 displaying 11 rows says so
/// rather than hiding it.
fn rows_in_order<'a>(rows: &'a [Row], hrefs: &[String]) -> Vec<&'a Row> {
    let by_href: std::collections::HashMap<&str, &Row> =
        rows.iter().map(|r| (r.href.as_str(), r)).collect();
    hrefs
        .iter()
        .filter_map(|href| by_href.get(href.as_str()).copied())
        .collect()
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

    record_skip_if_reacting_to_a_blend(&mut app);

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
    /// True while a beat-matched mix is arranged or under way. The one moment
    /// the app is doing what it exists to do, so it should be visible rather
    /// than inferred from two tracks being audible at once.
    mixing: bool,
    /// Peak output level, 0–1. Drives the mark's `energy`.
    level: f32,
    /// Envelope peaks for the playing track, empty until it has been analysed
    /// at the current version.
    waveform: Vec<f32>,
    /// What plays after this, so Now Playing can say so without a second call.
    next_title: String,
    /// Cover art for the playing track as a data URI, when the file carried
    /// one (TD-39).
    cover: Option<String>,
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

    // Already known to be unusable, so say so now rather than after fetching it
    // again and failing in the same way (TD-12). The record is cleared the
    // moment a later analysis pass succeeds on it.
    if let Some(reason) = app.failures.get(&href) {
        app.playback_error = Some(format!("This track cannot be played: {reason}"));
        app.playing = None;
        app.loading = false;
        return;
    }

    // A person who double-clicks a second track while the first is still
    // downloading expects the second one. The generation is what lets the
    // abandoned load recognise that it lost and drop its result.
    app.generation += 1;
    let generation = app.generation;
    app.loading = true;
    app.playback_error = None;
    app.playing = Some(href.clone());
    // A mix arranged before this choice is now for the wrong track. The engine
    // cancels its own side when the load lands; this is the shell's half.
    app.armed_next = None;
    // And its decoder stops with it. Left running it would go on filling a
    // window for a mix that will never happen, competing for disk and CPU with
    // the track the person actually asked for.
    app.next_stream = None;
    app.drift = None;
    player.cancel_transition();

    let link = player.link();
    let rate = player.sample_rate();
    let cache_dir = app.cache.dir().to_path_buf();
    let cache_max = app.cache.max_bytes();
    let remote = app.settings.remote.clone();
    let shared = Arc::clone(shared);

    tauri::async_runtime::spawn_blocking(move || {
        let cache = cache::Cache::new(cache_dir, cache_max);
        let outcome = cache
            .store(&href, || webdav::fetch_blocking(&remote, &href))
            .map_err(|e| e.to_string())
            .and_then(|path| {
                // A decoder thread and a few seconds of window, rather than the
                // whole track in memory (TD-09). Returns once there is enough
                // audio to start, so playback opens with music.
                decoder::Streamer::start(&path, rate, 0)
            });

        let Ok(mut app) = shared.lock() else {
            return;
        };
        if app.generation != generation {
            // Superseded. Handing this to the deck now would interrupt whatever
            // the person actually chose — and dropping the streamer here stops
            // its decoder thread, which is the point of holding one.
            return;
        }
        app.loading = false;

        match outcome {
            Ok(streamer) if !streamer.is_silent() => {
                // A refused load means the audio thread has stopped servicing
                // its queue, which is a dead device rather than a busy one.
                // Saying so beats a transport that reads "playing" in silence.
                if link.load(TrackSource::Stream(streamer.window()), true) {
                    // Held so the decoder keeps running and, more importantly,
                    // so it is stopped when this track is replaced. Dropping
                    // the previous one here — on a control thread — is what
                    // keeps that work off the audio thread.
                    app.playing_stream = Some(streamer);
                } else {
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
                // Learned the hard way; remember it so the next attempt is
                // instant rather than another download (TD-12).
                app.failures
                    .insert(href.clone(), "decodes to no audio".to_string());
                let _ = app.save_failures();
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
        mixing: app.player.as_ref().is_some_and(|p| p.transition_armed()),
        level: snapshot.map_or(0.0, |s| s.level),
        waveform: app
            .playing
            .as_ref()
            .and_then(|href| app.analysis.get(href))
            .map(|a| a.waveform.clone())
            .unwrap_or_default(),
        next_title: app
            .queue
            .peek_next(None)
            .and_then(|href| app.rows.iter().find(|r| r.href == href))
            .map(|r| r.title.clone())
            .unwrap_or_default(),
        cover: app
            .playing
            .as_ref()
            .and_then(|href| app.tags.get(href))
            .and_then(|t| t.cover.clone()),
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

/// How long before a mix begins to start decoding the incoming track.
///
/// Long enough for a fetch and a decode — the track is normally already
/// downloaded by the prefetcher, so this is mostly decode — and short enough
/// that two whole tracks are only in memory near the seam rather than for the
/// whole of every song (TD-09).
const TRANSITION_ARM_LEAD: f64 = 30.0;

/// Choose the mix for a given pair of tracks (TD-27).
///
/// The engine has three and the shell used to pick one for everything, which
/// meant every mix inherited Standard Crossfade's ~3 dB midpoint dip (TD-23)
/// whether or not it suited the pair.
///
/// **Ported from `audio_manager.gd::get_transition_type_between`.**
///
/// An earlier version of this was a two-way branch of my own devising. The
/// original is a weighted choice over six transition types, bucketed by
/// harmonic distance *and* tempo distance, with a genre jump treated as its own
/// case — and seeded by the pair, so the same two tracks always get the same
/// mix. That structure is what belongs here.
///
/// ## What cannot be ported yet
///
/// Three of the six — Echo Out, Reverb Freeze and Tempo Morph — need delay and
/// reverb, which `vapor-engine` does not have (TD-20). They carry most of the
/// weight in the original's buckets, particularly where the keys clash or the
/// tempi are far apart.
///
/// So their weight falls onto the three that exist, by nearest relative:
///
/// | Original | Stands in as | Why |
/// |---|---|---|
/// | Echo Out | Filter Sweep | Both hide the outgoing track behind an effect |
/// | Reverb Freeze | Filter Sweep | Same |
/// | Tempo Morph | Bass Swap | Both are "these two fit, ride it out" |
///
/// A mix that should be an Echo Out is a Filter Sweep today. That is a
/// downgrade rather than a design, it is visible — the Vibe screen names the
/// mix it will perform — and it resolves when TD-20 lands.
fn choose_transition(
    from_key: &str,
    to_key: &str,
    bpm_diff: f32,
    same_genre: bool,
) -> vapor_engine::TransitionType {
    use vapor_engine::TransitionType::{
        BassSwap, EchoOut, ReverbFreeze, StandardCrossfade, TempoMorph,
    };

    // Unanalysed. The original hashes the pair and takes any of the six; with
    // three available and nothing to reason from, the least opinionated one is
    // a better answer than a third of a coin flip.
    if from_key.is_empty() || to_key.is_empty() {
        return StandardCrossfade;
    }

    let key_cost = vapor_library::harmonic_relation_cost(from_key, to_key);
    // The original's "creative" match type: a genre jump is steered the same
    // way as a key clash, because both are a deliberate gear change.
    let clashing = key_cost >= vapor_library::CLASH_COST || !same_genre;

    // The original's buckets, now that all six types exist. It picks between
    // two or three candidates per bucket with a hash of the pair; a single
    // deterministic choice is taken here instead, favouring the type that
    // carries the most weight in each — the variety it adds is not worth a
    // second source of "which mix will this be" for the screen to predict.
    match (clashing, bpm_diff) {
        // Clash or gear change: hide it behind an effect, whatever the tempo.
        (true, d) if d < 8.0 => EchoOut,
        (true, _) => EchoOut,
        // Closely related keys and close tempi: the characteristic DJ move.
        (false, d) if d < 3.0 => BassSwap,
        // Still related, tempi a few BPM apart: bend them together.
        (false, d) if d < 8.0 => TempoMorph,
        // Too far apart to stretch — the engine would refuse a beat-match
        // anyway, so let the outgoing track dissolve rather than collide.
        (false, _) => ReverbFreeze,
    }
}

/// A track's tempo, honouring a manual correction.
fn bpm_of(analysis: &analysis::Analysis, app: &AppState, href: &str) -> f32 {
    app.settings.bpm_override(href).unwrap_or(analysis.bpm)
}

/// Whether two tracks sit in the same genre family.
///
/// `_get_match_type_between` calls a genre jump "creative" and steers it toward
/// an effect-led transition. `is_similar_genre` is already ported, so this asks
/// the original's question with the original's answer.
fn same_genre(app: &AppState, a: &str, b: &str) -> bool {
    let genre_of = |href: &str| {
        app.rows
            .iter()
            .find(|r| r.href == href)
            .map(|r| r.genre.clone())
            .unwrap_or_default()
    };
    let (ga, gb) = (genre_of(a), genre_of(b));
    // Unknown on either side is not evidence of a jump.
    if ga.is_empty() || gb.is_empty() {
        return true;
    }
    vapor_library::is_similar_genre(&ga, &gb)
}

/// Build the mixer's beat grid for a track, honouring a manual tempo.
///
/// A corrected BPM has to reach the grid, not just the table: the correction
/// exists because detection put the track at half or double time, and mixing
/// on the uncorrected value would beat-match to a tempo the person has already
/// said is wrong.
fn beat_grid(analysis: &analysis::Analysis, override_bpm: Option<f32>) -> vapor_engine::BeatGrid {
    let bpm = override_bpm.unwrap_or(analysis.bpm);
    // The tracked grid follows real tempo drift and real downbeat phase. It is
    // only consistent with the detected tempo, though — a corrected BPM means
    // synthesising a grid from it instead, which assumes a beat at zero and a
    // tempo that never wavers. Both are false for real music, and it is still
    // better than aligning to a grid the person has told us is wrong.
    let beats = if override_bpm.is_none() && !analysis.beats.is_empty() {
        analysis.beats.clone()
    } else {
        let period = 60.0 / bpm.max(1.0);
        let count = (analysis.duration as f32 / period) as usize;
        (0..count).map(|i| i as f32 * period).collect()
    };
    vapor_engine::BeatGrid { bpm, beats }
}

/// Everything a beat-matched mix needs, once it is known to be possible.
struct ArmedMix {
    next: String,
    kind: vapor_engine::TransitionType,
    duration: f32,
    incoming_pos: f32,
    ratio: f64,
    outgoing_ratio: f64,
    start_at: f64,
    /// Carried through for the drift correction (TD-21), which needs both grids
    /// and the pair of beats the alignment anchored on. They stay on this side
    /// of the audio boundary throughout — the correction crosses as one scalar.
    out_grid: vapor_engine::BeatGrid,
    in_grid: vapor_engine::BeatGrid,
    cue_in: f32,
    /// Both tracks have vocals, so the outgoing mids duck through the first
    /// half of the mix (TD-21).
    mid_cut: bool,
}

/// Decide whether the next track can be mixed into rather than merely followed.
///
/// Returns `None` for every ordinary reason a mix cannot happen — no analysis
/// yet, tempi too far apart to bridge musically, the moment already passed —
/// and the caller falls back to playing the next track when this one ends.
/// That fallback is the common case, not an error: a queue in title order will
/// mostly hold neighbours whose tempi are nowhere near each other.
fn plan_mix(app: &AppState, position: f64) -> Option<ArmedMix> {
    let current = app.playing.as_ref()?;
    let next = app.queue.peek_next(None)?.to_string();
    // A single-track queue would otherwise try to mix a track into itself.
    if &next == current {
        return None;
    }

    let outgoing = app.analysis.get(current)?;
    let incoming = app.analysis.get(&next)?;

    // The keys at the seam, where analysis produced them — an outro that
    // modulates is what the incoming track actually meets (TD-13).
    let out_key = if outgoing.outro_key.is_empty() {
        &outgoing.key
    } else {
        &outgoing.outro_key
    };
    let in_key = if incoming.intro_key.is_empty() {
        &incoming.key
    } else {
        &incoming.intro_key
    };
    let kind = choose_transition(
        out_key,
        in_key,
        (bpm_of(outgoing, app, current) - bpm_of(incoming, app, &next)).abs(),
        same_genre(app, current, &next),
    );
    // How much room the two tracks leave, snapped to a musical phrase (TD-21).
    // Falls back to the per-type default when either track's segments are
    // missing — an older analysis, or a track with no detectable body.
    let duration = vapor_engine::TransitionType::phrase_duration(
        (outgoing.duration as f32 - outgoing.outro_start).max(0.0),
        incoming.intro_end,
        bpm_of(outgoing, app, current),
    )
    .unwrap_or_else(|| kind.default_duration());
    // Start early enough that the mix finishes as the outgoing track's audible
    // content ends, rather than after it has already fallen silent.
    let start_at = (outgoing.cue_out - duration).max(0.0) as f64;
    if position < start_at - TRANSITION_ARM_LEAD || position > start_at {
        return None;
    }

    let out_grid = beat_grid(outgoing, app.settings.bpm_override(current));
    let in_grid = beat_grid(incoming, app.settings.bpm_override(&next));

    // Both of these are pure and live in the engine; running them here is what
    // keeps beat grids off the audio thread entirely.
    let ratio = vapor_engine::Mixer::tempo_ratio(&out_grid, &in_grid).ok()?;
    let incoming_pos = vapor_engine::Mixer::aligned_incoming_position(
        &out_grid,
        &in_grid,
        start_at as f32,
        incoming.cue_in,
    )
    .ok()?;

    // A Tempo Morph meets in the middle, so both decks are stretched; every
    // other transition leaves the outgoing track alone.
    let (ratio, outgoing_ratio) = if kind.morphs_tempo() {
        let target = (out_grid.bpm + in_grid.bpm) as f64 / 2.0;
        (target / in_grid.bpm as f64, target / out_grid.bpm as f64)
    } else {
        (ratio, 1.0)
    };

    Some(ArmedMix {
        next,
        kind,
        duration,
        incoming_pos,
        ratio,
        outgoing_ratio,
        start_at,
        cue_in: incoming.cue_in,
        out_grid,
        in_grid,
        // Two singers over each other is the clash the duck exists to prevent;
        // ducking a track that has no vocal only makes the mix quieter. Note
        // that `vocal_presence` is an energy threshold rather than a detector —
        // see `vapor_dsp::loudness::VOCAL_PRESENCE_ENERGY`.
        mid_cut: vapor_dsp::loudness::has_vocal_presence(outgoing.energy)
            && vapor_dsp::loudness::has_vocal_presence(incoming.energy),
    })
}

/// Decode the incoming track and arrange the mix (TD-25).
///
/// The decode happens off the supervisor thread because it is seconds of CPU,
/// and the supervisor has to stay responsive enough to notice a track ending.
fn arm_mix(shared: &Shared, app: &mut AppState, mix: ArmedMix) {
    let Some(player) = app.player.as_ref() else {
        return;
    };

    app.armed_next = Some(mix.next.clone());
    let generation = app.generation;

    let link = player.link();
    let rate = player.sample_rate();
    let cache_dir = app.cache.dir().to_path_buf();
    let cache_max = app.cache.max_bytes();
    let remote = app.settings.remote.clone();
    let shared = Arc::clone(shared);

    tauri::async_runtime::spawn_blocking(move || {
        let cache = cache::Cache::new(cache_dir, cache_max);
        let outcome = cache
            .store(&mix.next, || webdav::fetch_blocking(&remote, &mix.next))
            .map_err(|e| e.to_string())
            .and_then(|path| {
                // Decoded from where the mix will actually start, not from the
                // top of the track. A transition cues the incoming track
                // minutes in, and decoding the run-up to it would be the whole
                // cost that streaming exists to avoid.
                let from = (mix.incoming_pos as f64 * rate as f64).max(0.0) as u64;
                decoder::Streamer::start(&path, rate, from)
            });

        let Ok(mut app) = shared.lock() else {
            return;
        };
        // A person chose something else while this was decoding, so the mix is
        // now for the wrong pair of tracks.
        if app.generation != generation || app.armed_next.as_deref() != Some(mix.next.as_str()) {
            return;
        }

        match outcome {
            Ok(streamer) if !streamer.is_silent() => {
                link.preload(TrackSource::Stream(streamer.window()));
                link.schedule_transition(
                    mix.kind,
                    mix.duration,
                    mix.incoming_pos,
                    mix.ratio,
                    mix.outgoing_ratio,
                    mix.start_at,
                    mix.mid_cut,
                );

                // Correct the incoming deck's drift for the length of the mix
                // (TD-21). Needs both decks' windows, so it can only start once
                // the incoming one exists.
                app.drift = app.playing_stream.as_ref().and_then(|outgoing| {
                    sync::DriftCorrection::start(sync::Inputs {
                        link: Arc::clone(&link),
                        outgoing_grid: mix.out_grid,
                        incoming_grid: mix.in_grid,
                        outgoing_window: outgoing.window(),
                        incoming_window: streamer.window(),
                        ratio: mix.ratio,
                        start_time_out: mix.start_at as f32,
                        cue_in: mix.cue_in,
                        // A Tempo Morph is deliberately bending both decks, so
                        // a loop chasing phase through the first half would be
                        // fighting the transition rather than helping it.
                        delay_secs: if mix.kind.morphs_tempo() {
                            (mix.duration * 0.5).min(vapor_engine::pll::MORPH_DELAY_CAP_SECS)
                        } else {
                            0.0
                        },
                    })
                });
                app.next_stream = Some(streamer);
            }
            // Not surfaced. A mix that cannot be arranged is not a failure a
            // person needs to see — the track simply plays to its end and the
            // next one follows, which is what would have happened anyway.
            _ => {
                app.armed_next = None;
                app.next_stream = None;
                app.drift = None;
            }
        }
    });
}

/// How often the prefetcher looks for something worth downloading.
///
/// Unhurried on purpose. It is racing the length of a song, not a frame
/// deadline, and a tight loop over a lock the audio path also wants is a poor
/// trade for arriving thirty seconds earlier than necessary.
const PREFETCH_POLL: std::time::Duration = std::time::Duration::from_secs(2);

/// Wait after a failed fetch, doubling up to [`PREFETCH_BACKOFF_MAX`].
const PREFETCH_BACKOFF: std::time::Duration = std::time::Duration::from_secs(30);
const PREFETCH_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_secs(300);

/// Download the queue's lookahead before it is needed (TD-08).
///
/// Without this the next track starts downloading at the instant the current
/// one ends, so every track change costs a download and a decode in silence.
/// `Queue::lookahead` already returns exactly the right three hrefs — now
/// playing, next, and the one after — and has since the port; nothing consumed
/// them.
///
/// One track at a time, re-deciding after each. A queue that changes mid-fetch
/// therefore wastes at most one download, which is cheaper than the
/// cancellation machinery that would avoid it.
fn spawn_prefetcher(shared: Shared) {
    let spawned = std::thread::Builder::new()
        .name("vapor-prefetch".to_string())
        .spawn(move || {
            let mut failures: u32 = 0;

            loop {
                std::thread::sleep(if failures == 0 {
                    PREFETCH_POLL
                } else {
                    // An unreachable server must not be hammered every two
                    // seconds for as long as the app is open.
                    PREFETCH_BACKOFF
                        .saturating_mul(1 << (failures - 1).min(4))
                        .min(PREFETCH_BACKOFF_MAX)
                });

                let wanted = {
                    let Ok(app) = shared.lock() else {
                        return;
                    };
                    if !app.settings.remote.is_configured() {
                        continue;
                    }
                    // The track being played is fetched by `begin_playback`;
                    // what matters here is everything after it.
                    app.queue
                        .lookahead(None)
                        .into_iter()
                        .find(|href| !app.cache.contains(href))
                        .map(|href| {
                            (
                                href,
                                app.settings.remote.clone(),
                                app.cache.dir().to_path_buf(),
                                app.settings.cache_max_bytes,
                            )
                        })
                };

                let Some((href, remote, dir, max_bytes)) = wanted else {
                    failures = 0;
                    continue;
                };

                // Fetched with the lock released: this is a network round trip
                // measured in seconds, and every command would block behind it.
                let cache = cache::Cache::new(dir, max_bytes);
                match cache.store(&href, || webdav::fetch_blocking(&remote, &href)) {
                    Ok(_) => failures = 0,
                    Err(e) => {
                        failures = failures.saturating_add(1);
                        eprintln!("prefetch of {href} failed: {e}");
                    }
                }
            }
        });

    if let Err(e) = spawned {
        eprintln!("could not start the prefetcher: {e}");
    }
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

            // A mix completed, so the decks have changed roles: what was cued
            // is now what is playing. Nothing needs loading — the audio is
            // already running — the queue just has to agree about where it is.
            if app.player.as_ref().is_some_and(|p| p.take_swapped()) {
                let from = app.playing.clone();
                app.queue.next(None);
                app.playing = app.armed_next.take();
                // The decoders change roles with the decks. Assigning here also
                // drops the one feeding the track that just ended, which joins
                // its thread — on this thread, which is allowed to wait for it.
                app.playing_stream = app.next_stream.take();
                // The mix is over, so the correction has nothing left to
                // correct. Dropping it here joins its thread and returns the
                // deck to its own tempo.
                app.drift = None;
                // Remember what was blended into what, so a skip in the next
                // ten seconds can be attributed to this pair (TD-14).
                if let (Some(from), Some(to)) = (from, app.playing.clone()) {
                    app.last_mix = Some((from, to));
                    app.last_mix_ended = Some(std::time::Instant::now());
                }
                drop(app);
                let _ = app_handle.emit("playback-changed", ());
                continue;
            }

            // Consumed here, so one ending advances the queue exactly once.
            if !app.player.as_ref().is_some_and(|p| p.take_ended()) {
                // Nothing ended, so this is the moment to look ahead: can the
                // next track be mixed into rather than merely followed?
                let position = app.player.as_ref().map_or(0.0, |p| p.snapshot().position);
                let idle = app.loading
                    || app.armed_next.is_some()
                    || app.player.as_ref().is_none_or(|p| {
                        p.transition_armed() || p.snapshot().status != audio::Status::Playing
                    });
                if !idle {
                    if let Some(mix) = plan_mix(&app, position) {
                        arm_mix(&shared, &mut app, mix);
                    }
                }
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

/// A queued track with enough about it to draw a row.
///
/// The queue itself holds hrefs and nothing else — deliberately, so the core
/// stays free of presentation — which means the shell is where an href becomes
/// something a person can read.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueEntry {
    href: String,
    title: String,
    artist: String,
    cover: Option<String>,
    bpm: f32,
    key: String,
    /// True for the track currently playing.
    current: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueView {
    entries: Vec<QueueEntry>,
    /// "off" | "all" | "one".
    repeat: String,
    shuffled: bool,
    /// Index of the playing track, so the screen can scroll to it.
    current: Option<usize>,
    /// Minutes of music still to come, from the analysed durations. Tracks
    /// with no analysis contribute nothing rather than a guess.
    remaining_secs: f64,
}

#[tauri::command]
fn queue_view(state: State<'_, Shared>) -> Result<QueueView> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let current = app.queue.current_index();

    let entries: Vec<QueueEntry> = app
        .queue
        .tracks()
        .iter()
        .enumerate()
        .map(|(i, href)| {
            let row = app.rows.iter().find(|r| &r.href == href);
            let analysis = app.analysis.get(href);
            QueueEntry {
                href: href.clone(),
                cover: app.tags.get(href).and_then(|t| t.cover.clone()),
                title: row
                    .map(|r| r.title.clone())
                    // A track queued from a playlist whose scan has since been
                    // replaced still deserves a name, so fall back to the file.
                    .unwrap_or_else(|| href.rsplit('/').next().unwrap_or(href).to_string()),
                artist: row
                    .filter(|r| r.artist_source != vapor_library::index::Source::Unknown)
                    .map(|r| r.artist.clone())
                    .unwrap_or_default(),
                bpm: app
                    .settings
                    .bpm_override(href)
                    .or_else(|| analysis.map(|a| a.bpm))
                    .unwrap_or(0.0),
                key: analysis.map(|a| a.key.clone()).unwrap_or_default(),
                current: Some(i) == current,
            }
        })
        .collect();

    // Only what is still ahead: a "47 min" that included what you have already
    // heard would be describing the wrong thing.
    let remaining_secs = app
        .queue
        .tracks()
        .iter()
        .skip(current.map_or(0, |c| c))
        .filter_map(|href| app.analysis.get(href))
        .map(|a| a.duration)
        .sum();

    Ok(QueueView {
        entries,
        repeat: match app.queue.repeat() {
            vapor_library::Repeat::Off => "off",
            vapor_library::Repeat::All => "all",
            vapor_library::Repeat::One => "one",
        }
        .to_string(),
        shuffled: app.queue.is_shuffled(),
        current,
        remaining_secs,
    })
}

#[tauri::command]
fn remove_from_queue(href: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.queue.remove(&href))
}

#[tauri::command]
fn move_in_queue(from: usize, to: usize, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.queue.move_track(from, to))
}

#[tauri::command]
fn set_repeat(mode: String, state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.queue.set_repeat(match mode.as_str() {
        "off" => vapor_library::Repeat::Off,
        "one" => vapor_library::Repeat::One,
        _ => vapor_library::Repeat::All,
    });
    Ok(())
}

/// Shuffle the queue, or put it back.
///
/// The permutation is generated here rather than in the core, which owns no
/// randomness on purpose — `randi()` inside a library is what made the
/// GDScript's mood paths reshuffle for no reason.
#[tauri::command]
fn set_shuffled(shuffled: bool, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !shuffled {
        return Ok(app.queue.unshuffle());
    }

    let n = app.queue.tracks().len();
    if n < 2 {
        return Ok(false);
    }
    let mut order: Vec<usize> = (0..n).collect();
    // Fisher-Yates over a cheap PRNG. Nothing here is security-sensitive and
    // pulling in `rand` for one shuffle is not worth the dependency.
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545F4914F6CDD1D)
        | 1;
    for i in (1..n).rev() {
        // xorshift64*
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let j = (seed.wrapping_mul(0x2545F4914F6CDD1D) >> 33) as usize % (i + 1);
        order.swap(i, j);
    }
    Ok(app.queue.shuffle(&order))
}

/// Put a track next without disturbing the rest of the order.
#[tauri::command]
fn play_next(href: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.queue.set_next(&href))
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
        &skip_penalties(&app),
    ))
}

fn transition_name(kind: vapor_engine::TransitionType) -> String {
    use vapor_engine::TransitionType as T;
    match kind {
        T::StandardCrossfade => "crossfade",
        T::BassSwap => "bass swap",
        T::FilterSweep => "filter sweep",
        T::EchoOut => "echo out",
        T::ReverbFreeze => "reverb freeze",
        T::TempoMorph => "tempo morph",
    }
    .to_string()
}

/// Separator for a transition's two hrefs in the skip map.
///
/// A unit separator rather than a slash or a pipe: an href is a URL path and
/// can contain either, and a key that could be produced by two different pairs
/// would teach the pathfinder the wrong lesson.
const SKIP_KEY_SEP: char = '\u{1f}';

/// Cost a single skip adds to a transition.
///
/// **15.0, from `dj_pathfinder.gd::get_skip_penalty`** — the existing value, not
/// a new one. The port kept `WEIGHT_KEY`, `WEIGHT_BPM`, `WEIGHT_ENERGY` and
/// `WEIGHT_GENRE` unchanged, so this sits on the same scale it always did and
/// means what it always meant: one skip outweighs almost any harmonic argument
/// for putting that pair together again.
///
/// The original accumulates without a ceiling, by counting log entries. That is
/// kept too — a pair skipped five times has been rejected five times, and
/// capping it would be second-guessing the person.
const SKIP_PENALTY: f32 = 15.0;

/// How long after a mix ends that pressing next still counts as rejecting it.
///
/// From `audio_manager.gd::_check_and_log_skip`: a skip is logged when next is
/// pressed **during** a transition, or within ten seconds of one finishing —
/// the window in which a person is reacting to the *blend* rather than to the
/// track.
const SKIP_WINDOW_SECS: f64 = 10.0;

fn skip_key(from: &str, to: &str) -> String {
    format!("{from}{SKIP_KEY_SEP}{to}")
}

/// Record a rejected blend when next is pressed in reaction to one (TD-14).
///
/// The signal is the original's and it is deliberately narrow: a skip counts
/// only while a mix is running, or within [`SKIP_WINDOW_SECS`] of one ending.
/// Outside that window, pressing next is a judgement about the *track* — heard
/// too often, wrong mood — and recording it would teach the pathfinder to avoid
/// a transition nobody objected to.
fn record_skip_if_reacting_to_a_blend(app: &mut AppState) {
    let Some((from, to)) = app.last_mix.clone() else {
        return;
    };

    let mixing_now = app.player.as_ref().is_some_and(|p| p.transition_armed());
    let just_ended = app
        .last_mix_ended
        .is_some_and(|t| t.elapsed().as_secs_f64() <= SKIP_WINDOW_SECS);
    if !mixing_now && !just_ended {
        return;
    }

    // One verdict per blend. Pressing next twice in a row is one opinion about
    // one transition, not two.
    app.last_mix = None;
    *app.skips.entry(skip_key(&from, &to)).or_insert(0.0) += SKIP_PENALTY;
    let _ = app.save_skips();
}

/// The skip map in the shape the pathfinder wants.
fn skip_penalties(app: &AppState) -> std::collections::HashMap<(String, String), f32> {
    app.skips
        .iter()
        .filter_map(|(key, penalty)| {
            let (from, to) = key.split_once(SKIP_KEY_SEP)?;
            Some(((from.to_string(), to.to_string()), *penalty))
        })
        .collect()
}

/// Everything the pathfinder needs about the library, built from what has
/// actually been analysed.
///
/// The `mood_path` command takes this map from the caller, which suited a test
/// harness and suits no real screen — the frontend would have to hold a copy of
/// the whole library's analysis to ask a question about it. Building it here
/// keeps that where it already lives.
fn track_meta_pool(app: &AppState) -> std::collections::HashMap<String, TrackMeta> {
    app.rows
        .iter()
        .filter_map(|row| {
            let analysis = app.analysis.get(&row.href)?;
            // An unanalysed track has no tempo and no key, so the cost model
            // cannot place it. Including it with zeros would not make the path
            // longer, it would make it wrong.
            if analysis.bpm <= 0.0 {
                return None;
            }
            Some((
                row.href.clone(),
                TrackMeta {
                    href: row.href.clone(),
                    bpm: app.settings.bpm_override(&row.href).unwrap_or(analysis.bpm),
                    musical_key: analysis.key.clone(),
                    // Real segment keys where analysis produced them (TD-13);
                    // the whole-track key only where the track was too short
                    // for its ends to differ from its middle.
                    intro_key: if analysis.intro_key.is_empty() {
                        analysis.key.clone()
                    } else {
                        analysis.intro_key.clone()
                    },
                    outro_key: if analysis.outro_key.is_empty() {
                        analysis.key.clone()
                    } else {
                        analysis.outro_key.clone()
                    },
                    energy_level: analysis.energy,
                    genre: row.genre.clone(),
                },
            ))
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VibePath {
    hrefs: Vec<String>,
    /// How many library tracks were eligible — analysed, with a tempo. The
    /// screen says "1,284 read", and it should be the true number.
    considered: usize,
    /// How many were passed over for want of analysis. Reported rather than
    /// quietly dropped: a set built from a tenth of the library looks the same
    /// as one built from all of it (TD-43b).
    skipped: usize,
}

/// Order a set of tracks along an energy and tempo curve, from what is known.
#[tauri::command]
fn vibe_path(start: String, curve: String, state: State<'_, Shared>) -> Result<VibePath> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let pool = track_meta_pool(&app);

    if !pool.contains_key(&start) {
        return Err(Error(
            "That track has not been analysed yet, so the DJ has nothing to plan from.".to_string(),
        ));
    }

    let considered = pool.len();
    let skipped = app.rows.len().saturating_sub(considered);
    Ok(VibePath {
        hrefs: vapor_library::generate_mood_path(
            &pool,
            &start,
            Curve::parse(&curve),
            vapor_library::DEFAULT_ENERGY_THRESHOLD,
            // What the app has learned from being skipped (TD-14).
            &skip_penalties(&app),
        ),
        considered,
        skipped,
    })
}

/// What the next blend will do, in the terms the Vibe screen states them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BlendPreview {
    from_title: String,
    to_title: String,
    from_bpm: f32,
    to_bpm: f32,
    from_key: String,
    to_key: String,
    /// Tempo change the incoming deck will be stretched by, as a percentage.
    shift_percent: f32,
    /// Loudness difference, in LU, which is what a gain trim would correct.
    gain_delta: f32,
    /// Whether the engine would actually accept this as a beat-matched mix.
    matchable: bool,
    /// Why not, when it would not — the same distinction the mixer draws.
    reason: String,
    /// Which of the three mixes this pair would get (TD-27).
    transition: String,
}

/// Describe the mix between what is playing and what is next.
///
/// Read-only: this asks the same questions `plan_mix` asks and answers them for
/// a person instead of for the audio thread, so the screen cannot claim a blend
/// the engine would refuse.
#[tauri::command]
fn blend_preview(state: State<'_, Shared>) -> Result<Option<BlendPreview>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let Some(current) = app.playing.clone() else {
        return Ok(None);
    };
    let Some(next) = app.queue.peek_next(None).map(str::to_string) else {
        return Ok(None);
    };
    if next == current {
        return Ok(None);
    }

    let title_of = |href: &str| {
        app.rows
            .iter()
            .find(|r| r.href == href)
            .map(|r| r.title.clone())
            .unwrap_or_default()
    };

    let (Some(out), Some(inc)) = (app.analysis.get(&current), app.analysis.get(&next)) else {
        return Ok(Some(BlendPreview {
            from_title: title_of(&current),
            to_title: title_of(&next),
            from_bpm: 0.0,
            to_bpm: 0.0,
            from_key: String::new(),
            to_key: String::new(),
            shift_percent: 0.0,
            gain_delta: 0.0,
            matchable: false,
            reason: "Not analysed yet".to_string(),
            transition: "crossfade".to_string(),
        }));
    };

    let from_bpm = app.settings.bpm_override(&current).unwrap_or(out.bpm);
    let to_bpm = app.settings.bpm_override(&next).unwrap_or(inc.bpm);

    let out_grid = beat_grid(out, app.settings.bpm_override(&current));
    let in_grid = beat_grid(inc, app.settings.bpm_override(&next));
    let matched = vapor_engine::Mixer::tempo_ratio(&out_grid, &in_grid);

    let (matchable, reason, shift_percent) = match matched {
        Ok(ratio) => (true, String::new(), ((ratio - 1.0) * 100.0) as f32),
        Err(vapor_engine::MatchError::TempoTooFar) => (
            false,
            "Too far apart to beat-match".to_string(),
            if to_bpm > 0.0 {
                (from_bpm / to_bpm - 1.0) * 100.0
            } else {
                0.0
            },
        ),
        Err(vapor_engine::MatchError::NoGrid) => (false, "No usable beat grid".to_string(), 0.0),
    };

    Ok(Some(BlendPreview {
        from_title: title_of(&current),
        to_title: title_of(&next),
        from_bpm,
        to_bpm,
        from_key: out.key.clone(),
        to_key: inc.key.clone(),
        shift_percent,
        gain_delta: inc.lufs - out.lufs,
        matchable,
        reason,
        transition: transition_name(choose_transition(
            if out.outro_key.is_empty() {
                &out.key
            } else {
                &out.outro_key
            },
            if inc.intro_key.is_empty() {
                &inc.key
            } else {
                &inc.intro_key
            },
            (from_bpm - to_bpm).abs(),
            same_genre(&app, &current, &next),
        )),
    }))
}

// ---------------------------------------------------------------------------
// Liner notes
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackDetails {
    href: String,
    title: String,
    artist: String,
    album: String,
    year: u32,
    genre: String,
    /// Absent until the track has been analysed, so the screen can say so
    /// rather than print a column of zeroes.
    analysed: bool,
    bpm: f32,
    bpm_is_manual: bool,
    key: String,
    lufs: f32,
    duration: f64,
    cue_in: f32,
    cue_out: f32,
    energy: f32,
    beats: usize,
    waveform: Vec<f32>,
    /// Where the file is, and whether it is here — the sovereignty claim, per
    /// track.
    href_path: String,
    cached: bool,
    /// Why this track cannot be played, when it cannot (TD-12).
    unplayable: Option<String>,
    cover: Option<String>,
    /// The file's own comment field — the closest thing to the design's
    /// written notes that a file actually carries (TD-41b).
    notes: Option<String>,
    /// True when any of this came from the file's tags rather than its path,
    /// so the screen can stop claiming everything was derived from the name.
    tagged: bool,
}

#[tauri::command]
fn track_details(href: String, state: State<'_, Shared>) -> Result<TrackDetails> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let row = app
        .rows
        .iter()
        .find(|r| r.href == href)
        .ok_or_else(|| Error("That track is not in the library.".to_string()))?;
    let analysis = app.analysis.get(&href);
    let manual = app.settings.bpm_override(&href);

    Ok(TrackDetails {
        href: href.clone(),
        title: row.title.clone(),
        artist: if row.artist_source == vapor_library::index::Source::Unknown {
            String::new()
        } else {
            row.artist.clone()
        },
        album: if row.album_source == vapor_library::index::Source::Unknown {
            String::new()
        } else {
            row.album.clone()
        },
        year: row.year,
        genre: row.genre.clone(),
        analysed: analysis.is_some(),
        bpm: manual.or_else(|| analysis.map(|a| a.bpm)).unwrap_or(0.0),
        bpm_is_manual: manual.is_some(),
        key: analysis.map(|a| a.key.clone()).unwrap_or_default(),
        lufs: analysis.map_or(0.0, |a| a.lufs),
        duration: analysis.map_or(0.0, |a| a.duration),
        cue_in: analysis.map_or(0.0, |a| a.cue_in),
        cue_out: analysis.map_or(0.0, |a| a.cue_out),
        energy: analysis.map_or(0.0, |a| a.energy),
        beats: analysis.map_or(0, |a| a.beats.len()),
        waveform: analysis.map(|a| a.waveform.clone()).unwrap_or_default(),
        href_path: href.clone(),
        cached: app.cache.contains(&href),
        unplayable: app.failures.get(&href).cloned(),
        cover: app.tags.get(&href).and_then(|t| t.cover.clone()),
        notes: app.tags.get(&href).and_then(|t| t.comment.clone()),
        tagged: app.tags.contains_key(&href),
    })
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchResults {
    /// The single best row, shown larger. Absent when nothing matched.
    top: Option<Row>,
    tracks: Vec<Row>,
    /// Distinct artists and albums among the matches, for the "also matching"
    /// chips. Counted so the screen can rank them by weight of evidence.
    artists: Vec<Facet>,
    albums: Vec<Facet>,
    playlists: Vec<vapor_library::Playlist>,
    /// Total matches before the track list was truncated.
    total: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Facet {
    label: String,
    count: usize,
}

/// How many tracks a search returns. Beyond this the list stops being a result
/// and starts being the library again.
const SEARCH_LIMIT: usize = 40;

#[tauri::command]
fn search(query: String, state: State<'_, Shared>) -> Result<SearchResults> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    if query.trim().is_empty() {
        return Ok(SearchResults {
            top: None,
            tracks: Vec::new(),
            artists: Vec::new(),
            albums: Vec::new(),
            playlists: Vec::new(),
            total: 0,
        });
    }

    // The same predicate the table and smart playlists use, so a search and a
    // filter cannot disagree about what matches.
    let matched: Vec<Row> = vapor_library::filter(&app.rows, &query)
        .into_iter()
        .cloned()
        .map(|mut row| {
            app.apply_tags(&mut row);
            app.apply_analysis(&mut row);
            row
        })
        .collect();

    let needle = query.trim().to_lowercase();
    let facet = |pick: fn(&Row) -> &String| {
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for row in &matched {
            let value = pick(row);
            if !value.is_empty() {
                *counts.entry(value.as_str()).or_default() += 1;
            }
        }
        let mut facets: Vec<Facet> = counts
            .into_iter()
            .map(|(label, count)| Facet {
                label: label.to_string(),
                count,
            })
            .collect();
        // Most evidence first, then alphabetically so the order is stable
        // between identical searches rather than following a hash.
        facets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
        facets.truncate(6);
        facets
    };

    let artists = facet(|r| &r.artist);
    let albums = facet(|r| &r.album);

    // The best row is the one whose title starts with what was typed; failing
    // that, the one that merely contains it. Someone typing "salt" wants
    // "Salt Flats" above "Asphalt Sunday".
    let top = matched
        .iter()
        .find(|r| r.title.to_lowercase().starts_with(&needle))
        .or_else(|| {
            matched
                .iter()
                .find(|r| r.title.to_lowercase().contains(&needle))
        })
        .or(matched.first())
        .cloned();

    let playlists: Vec<vapor_library::Playlist> = app
        .playlists
        .all()
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&needle))
        .cloned()
        .collect();

    let total = matched.len();
    let tracks: Vec<Row> = matched
        .into_iter()
        // The top result is shown separately; repeating it immediately below
        // reads as a duplicate rather than as emphasis.
        .filter(|r| top.as_ref().is_none_or(|t| t.href != r.href))
        .take(SEARCH_LIMIT)
        .collect();

    Ok(SearchResults {
        top,
        tracks,
        artists,
        albums,
        playlists,
        total,
    })
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

/// Point the app at a server.
///
/// Separate from the password, which never touches this struct — see
/// `save_webdav_password`. Without this command the app had no way to be
/// configured at all: `settings` could report a server and nothing could set
/// one.
#[tauri::command]
fn set_remote_config(
    url: String,
    username: String,
    folder: String,
    state: State<'_, Shared>,
) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

    // Refused rather than stored, because everything downstream treats this as
    // an origin to hang paths off: a value that is not one produces a scan that
    // finds nothing and reports no error, which reads as "my library is empty"
    // rather than "that is not an address". Pasting an app password in here is
    // the way it actually happens.
    let trimmed = url.trim();
    if !trimmed.is_empty() && !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(Error(format!(
            "\"{trimmed}\" is not a server address — it needs to start with \
             https://. For Koofr that is https://app.koofr.net, and the app \
             password goes in the Password field."
        )));
    }

    // Renaming the account leaves its password behind otherwise: the keychain
    // entry is keyed by username, so a new name writes a new entry and the old
    // one sits there until "delete everything" runs. Best effort — a keychain
    // that will not give up an entry is not a reason to refuse the change.
    let previous = app.settings.remote.username.clone();
    if !previous.is_empty() && previous != username.trim() {
        let _ = webdav::delete_password(&previous);
    }

    app.settings.remote.url = url.trim().to_string();
    app.settings.remote.username = username.trim().to_string();
    app.settings.remote.folder = folder.trim().to_string();
    // An empty folder means the library root, which `sanitised` spells as the
    // default rather than as "".
    app.settings = std::mem::take(&mut app.settings).sanitised();
    app.save_settings()?;

    Ok(app.settings.clone())
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
    let (todo, (cache_dir, cache_max), cancel, remote) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        let hrefs: Vec<String> = app.rows.iter().map(|r| r.href.clone()).collect();
        app.cancel.reset();
        (
            analysis::pending(&hrefs, &app.analysis),
            (app.cache.dir().to_path_buf(), app.cache.max_bytes()),
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
        let cache = cache::Cache::new(cache_dir, cache_max);
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
                if let Ok(mut app) = state_arc.lock() {
                    if let Some(a) = &progress.analysis {
                        // The file is local and just been read, so this is the
                        // cheapest moment there will ever be to read its tags
                        // (TD-39). A separate pass would mean downloading the
                        // library twice.
                        if let Some(path) = cache.get(&progress.href) {
                            let tags = tags::read(&path);
                            if !tags.is_empty() {
                                app.tags.insert(progress.href.clone(), tags.into());
                                let _ = app.save_tags();
                            }
                        }
                        app.analysis.insert(progress.href.clone(), a.clone());
                        let _ = app.save_analysis();
                        // It worked this time, so whatever was wrong before is
                        // no longer true.
                        if app.failures.remove(&progress.href).is_some() {
                            let _ = app.save_failures();
                        }
                    } else if let Some(reason) = &progress.error {
                        // Only the permanent kind. A track that simply was not
                        // downloaded when the pass ran must not be condemned.
                        if !progress.retryable {
                            app.failures.insert(progress.href.clone(), reason.clone());
                            let _ = app.save_failures();
                        }
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
fn set_cache_max_bytes(bytes: u64, state: State<'_, Shared>) -> Result<u64> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

    app.settings.cache_max_bytes = bytes;
    // The core decides what is too small to be worth having, so the answer is
    // the same here as it would be for a hand-edited settings file.
    app.settings = std::mem::take(&mut app.settings).sanitised();
    let applied = app.settings.cache_max_bytes;

    let dir = app.cache.dir().to_path_buf();
    app.cache = cache::Cache::new(dir, applied);
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
fn clear_audio_cache(state: State<'_, Shared>) -> Result<u64> {
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

/// One line of the Your Data table: what it is, where it sits, how big.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DataRow {
    label: String,
    path: String,
    bytes: u64,
    /// False for anything that lives on the server rather than here. The
    /// screen's whole claim is about what is on *this* device, so the
    /// distinction has to be visible rather than implied.
    local: bool,
}

/// Itemise what the app is storing.
///
/// The Your Data screen is where the sovereignty claim gets proved instead of
/// asserted, and a single total proves nothing — a person has to be able to see
/// which file is which, open it, and find it is plain JSON.
#[tauri::command]
fn data_breakdown(state: State<'_, Shared>) -> Result<Vec<DataRow>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let dir = app.store.dir();

    let size_of = |name: &str| -> u64 {
        std::fs::metadata(dir.join(name))
            .map(|m| m.len())
            .unwrap_or(0)
    };

    Ok(vec![
        DataRow {
            label: "Music files".to_string(),
            path: if app.settings.remote.is_configured() {
                format!(
                    "{} / {}",
                    app.settings.remote.url.trim_end_matches('/'),
                    app.settings.remote.folder
                )
            } else {
                "No server connected".to_string()
            },
            // The library is the one thing not measured: asking a WebDAV server
            // for the size of every file is a scan, and a scan is not something
            // to run because a screen was opened.
            bytes: 0,
            local: false,
        },
        DataRow {
            label: "Offline cache".to_string(),
            path: app.cache.dir().display().to_string(),
            bytes: app.cache.size(),
            local: true,
        },
        DataRow {
            label: "Library catalogue".to_string(),
            path: dir.join("analysis.json").display().to_string(),
            bytes: size_of("analysis.json"),
            local: true,
        },
        DataRow {
            label: "Playlists".to_string(),
            path: dir.join("playlists.json").display().to_string(),
            bytes: size_of("playlists.json"),
            local: true,
        },
        DataRow {
            label: "Settings".to_string(),
            path: dir.join("settings.json").display().to_string(),
            bytes: size_of("settings.json"),
            local: true,
        },
    ])
}

/// Open the data directory in the system file manager.
///
/// "Your data is local" is a claim until a person can go and look at it. No
/// Tauri plugin for this, and shelling out to the platform's own opener is
/// three lines — the path is one the app itself chose, never user input, so
/// there is nothing here to inject into.
#[tauri::command]
fn reveal_data_folder(state: State<'_, Shared>) -> Result<()> {
    let dir = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.store.dir().to_path_buf()
    };
    std::fs::create_dir_all(&dir)?;

    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };

    std::process::Command::new(opener)
        .arg(&dir)
        .spawn()
        .map_err(|e| Error(format!("Could not open the folder: {e}")))?;
    Ok(())
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
                // Only useful alongside playback: without a device there is no
                // queue moving forward to run ahead of.
                spawn_prefetcher(Arc::clone(&shared));
            }

            app.manage(shared);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_view,
            playlists,
            create_playlist,
            add_tracks_to_playlist,
            rename_playlist,
            delete_playlist,
            remove_playlist_track,
            reorder_playlist_track,
            playlist_rows,
            queue_state,
            queue_view,
            remove_from_queue,
            move_in_queue,
            set_repeat,
            set_shuffled,
            play_next,
            vibe_path,
            blend_preview,
            track_details,
            search,
            data_breakdown,
            reveal_data_folder,
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
            set_remote_config,
            scan_library,
            analyse_library,
            cancel_analysis,
            analysis_status,
            cache_status,
            set_cache_max_bytes,
            clear_audio_cache,
            evict_track,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vapor Music");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(href: &str, title: &str) -> Row {
        Row {
            href: href.to_string(),
            title: title.to_string(),
            artist: String::new(),
            album: String::new(),
            artist_source: vapor_library::index::Source::Unknown,
            album_source: vapor_library::index::Source::Unknown,
            genre: String::new(),
            bpm: 0.0,
            key: String::new(),
            year: 0,
            manual_pos: 0,
        }
    }

    /// A playlist's order is the playlist's, not the library's. Sorting these
    /// by title would silently discard the one thing a manual playlist is for.
    #[test]
    fn playlist_rows_follow_the_playlist_order() {
        let library = vec![row("/a", "Anna"), row("/b", "Bess"), row("/c", "Cleo")];
        let wanted = vec!["/c".to_string(), "/a".to_string(), "/b".to_string()];

        let got: Vec<&str> = rows_in_order(&library, &wanted)
            .iter()
            .map(|r| r.href.as_str())
            .collect();
        assert_eq!(got, vec!["/c", "/a", "/b"]);
    }

    /// A track whose file has left the library is skipped rather than rendered
    /// as a blank row nobody can play. The playlist keeps the href — this is
    /// only about what is shown — and the difference is visible because the
    /// screen prints the playlist's own length beside the rows.
    #[test]
    fn a_track_missing_from_the_library_is_skipped() {
        let library = vec![row("/a", "Anna"), row("/c", "Cleo")];
        let wanted = vec!["/a".to_string(), "/gone".to_string(), "/c".to_string()];

        let got = rows_in_order(&library, &wanted);
        assert_eq!(got.len(), 2, "the missing track was not skipped");
        assert_eq!(got[0].href, "/a");
        assert_eq!(got[1].href, "/c");
    }

    /// A playlist may hold the same track twice, and both should appear —
    /// deduplicating here would silently disagree with the stored length.
    #[test]
    fn a_repeated_track_appears_each_time() {
        let library = vec![row("/a", "Anna")];
        let wanted = vec!["/a".to_string(), "/a".to_string()];
        assert_eq!(rows_in_order(&library, &wanted).len(), 2);
    }

    #[test]
    fn an_empty_playlist_yields_nothing() {
        let library = vec![row("/a", "Anna")];
        assert!(rows_in_order(&library, &[]).is_empty());
        assert!(rows_in_order(&[], &["/a".to_string()]).is_empty());
    }
}

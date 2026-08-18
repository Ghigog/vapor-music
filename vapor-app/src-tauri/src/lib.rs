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
#[cfg(any(target_os = "android", feature = "android-check"))]
mod android;
/// Public so `tests/audio_realtime.rs` can drive the audio path without a
/// device. Nothing outside the crate uses it.
pub mod audio;
mod cache;
/// Public for the same reason as `audio`: the real-time test drives a real
/// streaming deck rather than a stand-in for one.
pub mod decoder;
mod media;
mod metadata;
mod peers;
mod secrets;
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
    Curve, FolderStore, GroupStore, PlaylistStore, Queue, Settings, TrackMeta,
};

/// Everything the shell holds between commands.
///
/// One lock rather than one per collection: commands are user-driven and
/// short, so contention is not a concern, and a single lock cannot deadlock
/// against itself the way a set of finer ones can.
struct AppState {
    settings: Settings,
    playlists: PlaylistStore,
    /// Folders that playlists are filed into. A folder owns no tracks — a
    /// playlist carries a `folder_id` pointing at one.
    folders: FolderStore,
    /// Smart groups: saved sets of artists, albums and genres.
    ///
    /// A group holds *entities*, not tracks, which is what makes it different
    /// from a playlist — membership is resolved against the library when it is
    /// read, so a group stays current as the library grows. `group.rs` has been
    /// complete and tested since the port and nothing was wired to it.
    groups: GroupStore,
    /// Lyrics and artwork looked up from public services, keyed by href.
    ///
    /// Kept apart from `analysis` and `tags` on purpose: those are what this
    /// device measured and what the file itself carries, and this is what a
    /// stranger said. Merging them would make the screen unable to tell a
    /// person which is which.
    looked: metadata::Cache,
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
    /// Which pass is current. A replaced pass finishes some time after the one
    /// that replaced it starts, and without this its "I have stopped" would
    /// clear the flag belonging to the pass still running.
    analysis_generation: u64,
    /// Whether a pass is in flight, and what it is working on.
    ///
    /// Analysis starts by itself after a scan now, so "did someone press the
    /// button" is no longer the same question as "is it running" — and the
    /// screen used to answer the second by checking the first, which meant an
    /// automatic pass was invisible.
    analysing: bool,
    analysing_title: String,
    /// Where the four-step choice cycle has got to (§3 of the workflow doc).

    /// This installation's identity on the local network (SYNC-001).
    ///
    /// Generated once and persisted. Deliberately not derived from the
    /// hostname, the user or the hardware: it is broadcast in clear on a
    /// shared network, so it must say "a copy of Vapor" and nothing else about
    /// whose it is.
    device_id: String,
    /// Devices this one has paired with (SYNC-002). Persisted.
    trust: vapor_library::sync::Trust,
    /// A pairing in progress, if this device is currently showing a code.
    pairing: Option<vapor_library::sync::Pairing>,
    /// The code on screen. Held separately because `Pairing` deliberately does
    /// not hand its PIN back out — the only thing that may read it is the
    /// person looking at the screen.
    pin: Option<String>,
    /// Who is on the network, kept by the beacon thread.
    peers: peers::Peers,
    /// Files that could not be read at startup and were moved aside.
    ///
    /// Empty on every normal launch. Non-empty means the app is running on a
    /// default for something the person had data for, and they need telling —
    /// silently starting with no playlists is the failure this exists to stop.
    damaged: Vec<store::Quarantined>,
    /// What this device has deleted, so the deletion travels (TD-57).
    ///
    /// Kept beside the stores rather than inside them: a store holds what
    /// exists, and this is a record of what does not.
    tombstones: vapor_library::sync::Tombstones,
    /// The beacon and server threads, while sync is on (TD-58).
    ///
    /// Held so the switch can stop them. `None` means either sync is off or the
    /// ports could not be bound, and both mean there is nothing to stop.
    sync_session: Option<peers::Session>,
    /// Content digests, keyed by href, with the size they were computed at.
    ///
    /// Hashing a library is not free and the answer does not change unless the
    /// file does, so it is memoised beside the analysis and for the same
    /// reason. The size is the invalidation: a file that changed length is a
    /// different file.
    digests: std::collections::HashMap<String, (u64, String)>,
    /// What a running sync is doing, for the dashboard to draw.
    sync: SyncProgress,

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
        // Every load goes through `quarantined`, which moves a file that cannot
        // be read out of the way before the app carries on with a default.
        // Without that, an unreadable-but-intact file is replaced by an empty
        // one the first time anything is saved — the data was recoverable right
        // up until the app tried to help. See `Store::load_or_quarantine`.
        let mut damaged: Vec<store::Quarantined> = Vec::new();
        macro_rules! quarantined {
            ($name:literal) => {{
                let (value, problem) = store.load_or_quarantine($name);
                if let Some(p) = problem {
                    eprintln!("store: {}", p.message());
                    damaged.push(p);
                }
                value
            }};
        }

        let settings = quarantined!("settings")
            .unwrap_or_else(Settings::default)
            .sanitised();
        let cache_max_bytes = settings.cache_max_bytes;
        let playlists = quarantined!("playlists").unwrap_or_default();
        let folders = quarantined!("folders").unwrap_or_default();
        let groups = quarantined!("groups").unwrap_or_default();
        let looked = quarantined!("metadata").unwrap_or_default();
        let trust = quarantined!("trust").unwrap_or_default();
        let tombstones = quarantined!("tombstones").unwrap_or_default();
        let digests = quarantined!("digests").unwrap_or_default();
        // Generated on first launch and kept. A device that renamed itself
        // every start would appear as a new peer each time, and every pairing
        // would have to be redone.
        let device_id: String = quarantined!("device_id").unwrap_or_else(|| new_id("device"));
        let analysis = quarantined!("analysis").unwrap_or_default();
        let failures = quarantined!("failures").unwrap_or_default();
        let tags = quarantined!("tags").unwrap_or_default();
        let skips = quarantined!("skips").unwrap_or_default();
        // The scanned index. Without this the library was rebuilt from the
        // server on every launch: the app opened on "0 tracks" and stayed
        // there until someone found Settings and pressed Scan — a walk of
        // every directory on the server to rediscover a list that had not
        // changed since the last time it was walked.
        let rows = quarantined!("index").unwrap_or_default();
        AppState {
            damaged,
            settings,
            playlists,
            folders,
            groups,
            looked,
            device_id,
            trust,
            pairing: None,
            pin: None,
            peers: Arc::new(Mutex::new(vapor_library::sync::PeerRegistry::new())),
            tombstones,
            sync_session: None,
            digests,
            sync: SyncProgress::default(),
            queue: Queue::default(),
            rows,
            last_mix: None,
            last_mix_ended: None,
            analysis,
            skips,
            tags,
            failures,
            cancel: analysis::Cancel::new(),
            analysis_generation: 0,
            analysing: false,
            analysing_title: String::new(),
            // The bound is what stops the cache filling a phone, and it is the
            // person's to set — `sanitised` has already refused a value too
            // small to be worth having.
            cache: cache::Cache::new(store.dir().join("audio"), cache_max_bytes),
            store,
            // No device yet. `load` reads this app's files; acquiring hardware
            // is [`AppState::open_audio`], called once from `run`.
            player: None,
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

    /// Acquire the audio device, once.
    ///
    /// Separate from [`AppState::load`], which is otherwise "read my files" —
    /// and which every test calls. Opening a device there meant 165 tests each
    /// took a real sound card, and on a Windows runner with no audio endpoint
    /// the first one to try killed the whole test binary with an access
    /// violation before a single result was reported (AND-2). The failure is
    /// inside `cpal`'s WASAPI backend, below anything this crate can guard, so
    /// the fix is to stop asking for a device in a function that has no
    /// business wanting one.
    ///
    /// Opened once rather than per track: acquiring a device takes long enough
    /// to hear as a gap, and holding one open is what every other player does.
    fn open_audio(&mut self) {
        match audio::Player::start() {
            Ok(p) => self.player = Some(p),
            Err(e) => eprintln!("audio output unavailable: {e}"),
        }
    }

    /// Persist the scanned index.
    ///
    /// Written only by a scan, which is the only thing that changes it. The
    /// rows carry what the *path* said; tags and analysis are applied on read
    /// from their own files, so this stays small — a few hundred KB for a
    /// library of thousands — and cannot go stale against them.
    fn save_index(&self) -> Result<()> {
        self.store.save("index", &self.rows)?;
        Ok(())
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

    fn save_folders(&self) -> Result<()> {
        self.store.save("folders", &self.folders)?;
        Ok(())
    }

    fn save_groups(&self) -> Result<()> {
        self.store.save("groups", &self.groups)?;
        Ok(())
    }

    fn save_looked(&self) -> Result<()> {
        self.store.save("metadata", &self.looked)?;
        Ok(())
    }

    fn save_trust(&self) -> Result<()> {
        self.store.save("trust", &self.trust)?;
        Ok(())
    }

    fn save_tombstones(&self) -> Result<()> {
        self.store.save("tombstones", &self.tombstones)?;
        Ok(())
    }

    /// What this device calls itself on the network.
    ///
    /// The folder name of the library, or a plain fallback. **Not** the
    /// hostname: on a personal machine that is usually the owner's name, and
    /// this is broadcast in clear to everyone on the café Wi-Fi.
    fn device_name(&self) -> String {
        let folder = self
            .settings
            .remote
            .folder
            .rsplit('/')
            .find(|s| !s.is_empty())
            .unwrap_or("");
        if folder.is_empty() || folder == "Music" {
            "Vapor".to_string()
        } else {
            format!("Vapor · {folder}")
        }
    }

    /// The content digest of a cached track, computed once.
    ///
    /// `None` for a track this device knows of but has never held, which in a
    /// cloud-first library is most of them. That is a real answer, not a
    /// failure — [`vapor_library::sync::reconcile`] treats a missing digest as
    /// "no opinion" rather than as a disagreement.
    fn digest_of(&mut self, href: &str) -> Option<String> {
        let path = self.cache.get(href)?;
        let size = std::fs::metadata(&path).ok()?.len();

        if let Some((seen_at, digest)) = self.digests.get(href) {
            if *seen_at == size {
                return Some(digest.clone());
            }
        }
        let bytes = std::fs::read(&path).ok()?;
        let digest = vapor_library::sync::digest(&bytes);
        self.digests
            .insert(href.to_string(), (size, digest.clone()));
        Some(digest)
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
    /// Narrow to one album, exactly. Set when a person has opened an album
    /// rather than typed something — a substring search for "Melody" also
    /// matches "All Melody (Reprise)" and every track whose title contains it,
    /// which is not what opening a sleeve means.
    #[serde(default)]
    album: Option<String>,
    /// Narrow to one artist, exactly. Same reasoning.
    #[serde(default)]
    artist: Option<String>,
}

/// One album or artist, as the grid draws it.
///
/// The Albums tab used to render a card per *track*, grouped under an album
/// heading — so "All Melody" was a header with nine tiles under it, none of
/// which was the album. A tab called Albums shows albums.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryEntity {
    /// Album title, or artist name.
    name: String,
    /// The album's artist, or for an artist the number of albums. Empty when
    /// there is nothing true to say.
    subtitle: String,
    tracks: usize,
    /// A track from it, for the cover and for what plays when it is pressed.
    /// Covers are fetched per card rather than embedded in every row: a 2 MB
    /// sleeve on 563 rows is not a payload, it is an outage.
    lead: String,
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

    let mut rows = resolved_rows(&app, &view);

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

/// The library, filtered and with tags and analysis applied.
///
/// Tags are applied *before* the album and artist filters, because until they
/// are the row's album is whatever the path implied — and narrowing on a value
/// the row does not carry yet returns nothing.
fn resolved_rows(app: &AppState, view: &LibraryView) -> Vec<Row> {
    let mut rows: Vec<Row> = vapor_library::filter(&app.rows, &view.query)
        .into_iter()
        .cloned()
        .collect();
    for row in rows.iter_mut() {
        app.apply_tags(row);
        app.apply_analysis(row);
    }
    if let Some(album) = view.album.as_deref() {
        rows.retain(|r| r.album == album && r.album_source.is_known());
    }
    if let Some(artist) = view.artist.as_deref() {
        rows.retain(|r| r.artist == artist && r.artist_source.is_known());
    }
    rows
}

/// The albums or artists in the library, one entry each.
///
/// Grouping rows and drawing a card per row is what the Albums tab did, and it
/// answers a different question: "which tracks are on this album" rather than
/// "which albums do I have". Tracks whose album or artist is unknown are left
/// out entirely — a tab called Albums listing things that are not albums is
/// the complaint this exists to fix. They remain reachable under Songs.
#[tauri::command]
fn library_entities(view: LibraryView, state: State<'_, Shared>) -> Result<Vec<LibraryEntity>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let rows = resolved_rows(&app, &view);

    let by_artist = matches!(view.group_by.as_deref(), Some("artist"));

    // Insertion-ordered: the rows arrive sorted, and a map would scramble them.
    let mut order: Vec<String> = Vec::new();
    let mut members: std::collections::HashMap<String, Vec<&Row>> =
        std::collections::HashMap::new();

    for row in &rows {
        let (name, known) = if by_artist {
            (row.artist.clone(), row.artist_source.is_known())
        } else {
            (row.album.clone(), row.album_source.is_known())
        };
        if !known || name.is_empty() {
            continue;
        }
        // Grouped by identity, displayed by name — and for an album those are
        // not the same thing. Keying on the title alone merges two different
        // records that share one, which every library eventually has: two
        // *Greatest Hits* became one tile, with one cover and one artwork
        // override between them. `album_key` adds the folder, which also keeps
        // two albums that happen to share a directory apart, and still holds a
        // various-artists compilation together.
        let key = if by_artist {
            name.clone()
        } else {
            vapor_library::settings::album_key(&name, &row.href)
        };
        if !members.contains_key(&key) {
            order.push(key.clone());
        }
        members.entry(key).or_default().push(row);
    }

    Ok(order
        .into_iter()
        .filter_map(|key| {
            let tracks = members.get(&key)?;
            let lead = tracks.first()?.href.clone();
            // The display name comes off a member rather than out of the key:
            // the key carries a folder the person never typed and should not
            // read.
            let name = if by_artist {
                key.clone()
            } else {
                tracks.first()?.album.clone()
            };

            let subtitle = if by_artist {
                // How many albums, which is what distinguishes one artist tile
                // from another at a glance.
                let mut albums: Vec<&str> = tracks
                    .iter()
                    .filter(|r| r.album_source.is_known())
                    .map(|r| r.album.as_str())
                    .collect();
                albums.sort_unstable();
                albums.dedup();
                match albums.len() {
                    0 => format!("{} tracks", tracks.len()),
                    1 => "1 album".to_string(),
                    n => format!("{n} albums"),
                }
            } else {
                // The album's artist — or the truth, when a compilation has
                // several. Naming only the first would be a quiet lie.
                let mut artists: Vec<&str> = tracks
                    .iter()
                    .filter(|r| r.artist_source.is_known())
                    .map(|r| r.artist.as_str())
                    .collect();
                artists.sort_unstable();
                artists.dedup();
                match artists.len() {
                    0 => String::new(),
                    1 => artists[0].to_string(),
                    _ => "Various artists".to_string(),
                }
            };

            Some(LibraryEntity {
                name,
                subtitle,
                tracks: tracks.len(),
                lead,
            })
        })
        .collect())
}

/// The embedded cover for one track, if analysis has read it.
///
/// One call per card rather than a field on every row. Artwork is capped at
/// 2 MB by the tag reader, so a 563-row `library_view` carrying covers would
/// move hundreds of megabytes through IPC on every keystroke.
#[tauri::command]
fn track_cover(href: String, state: State<'_, Shared>) -> Result<Option<String>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.tags.get(&href).and_then(|t| t.cover.clone()))
}

// ---------------------------------------------------------------------------
// Lyrics and artwork looked up from public services
// ---------------------------------------------------------------------------
//
// The network half of `metadata_service.gd`. See `metadata.rs` for why this is
// the one part of the app that talks to a stranger, and why it asks first.

/// Serialise a float so it can never arrive as `null`.
///
/// JSON cannot write NaN or infinity, so `serde_json` writes `null` instead.
/// Every float in a reply is typed `number` on the TypeScript side, so a screen
/// doing the obvious thing — `blend.shiftPercent.toFixed(1)` — throws
/// `TypeError` on `null`. A throw during render unmounts the tree and the window
/// goes blank with nothing on it to say why.
///
/// Substituting zero rather than clamping to a huge number: an infinite tempo
/// shift is not "a very large shift", it is the absence of an answer, and every
/// screen already renders zero sensibly. A wrong number that reads as a number
/// is also easier to notice than a hole.
///
/// Applied at the boundary rather than at each use site, so the screens can keep
/// doing the obvious thing. `tests/ipc_numbers.rs` gates that every float in a
/// reply type has it.
pub fn finite<S: serde::Serializer>(value: &f32, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_f32(if value.is_finite() { *value } else { 0.0 })
}

/// [`finite`], for the fields that are `f64` — a playhead position and a
/// duration, which are seconds and want the precision.
pub fn finite64<S: serde::Serializer>(value: &f64, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_f64(if value.is_finite() { *value } else { 0.0 })
}

/// What is known about a track from outside this device.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct LookedUp {
    lyrics: Option<metadata::Lyrics>,
    artist_image: String,
    album_art: String,
    genre: String,
    /// Whether a lookup has been made for this track at all.
    attempted: bool,
    /// Whether the setting permits making one.
    ///
    /// The screen needs this to tell "we looked and found nothing" from "we
    /// have not been allowed to look" — two states that otherwise render as
    /// the same empty panel.
    allowed: bool,
}

impl LookedUp {
    fn of(app: &AppState, href: &str) -> Self {
        let allowed = app.settings.metadata_lookup_enabled;
        match app.looked.get(href) {
            Some(l) => LookedUp {
                lyrics: l.lyrics.clone(),
                artist_image: l.artist_image.clone(),
                album_art: l.album_art.clone(),
                genre: l.genre.clone(),
                attempted: l.attempted,
                allowed,
            },
            None => LookedUp {
                allowed,
                ..Default::default()
            },
        }
    }
}

/// What has already been looked up for a track. Never makes a request.
#[tauri::command]
fn track_lookup(href: String, state: State<'_, Shared>) -> Result<LookedUp> {
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
fn look_up_track(href: String, force: bool, state: State<'_, Shared>) -> Result<LookedUp> {
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
        let (album_art, genre) = sleeve.join().unwrap_or_default();
        (lyrics, artist_image, album_art, genre)
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

/// Whether hardware media keys will reach this build (MIG-023).
///
/// False on macOS when the process is not inside a `.app` bundle: the keys go
/// to the Now Playing *application*, and a bare binary from `tauri dev` is not
/// one. Reported rather than left to be discovered, because souvlaki registers
/// successfully either way and the only other symptom is a keyboard that does
/// nothing.
#[tauri::command]
fn media_keys_available() -> bool {
    media::bundled()
}

/// Progress of the library identification pass.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentifyProgress {
    done: usize,
    total: usize,
    /// The track just finished, so a screen can name what is happening.
    title: String,
    /// How many tempos have been corrected so far — the point of the exercise.
    corrected: usize,
    /// How many tracks Deezer had a genre for.
    genres: usize,
    /// Set on the final message.
    finished: bool,
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
async fn identify_library(app_handle: tauri::AppHandle, state: State<'_, Shared>) -> Result<()> {
    use tauri::Emitter;

    let (todo, remote) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        let todo: Vec<(String, String, String, String, f32, f64)> = app
            .rows
            .iter()
            .filter_map(|row| {
                let analysis = app.analysis.get(&row.href)?;
                let mut r = row.clone();
                app.apply_tags(&mut r);
                Some((
                    r.href.clone(),
                    r.title.clone(),
                    r.artist.clone(),
                    r.album.clone(),
                    analysis.bpm,
                    analysis.duration,
                ))
            })
            .collect();
        (todo, app.settings.remote.clone())
    };
    let _ = remote;

    if todo.is_empty() {
        return Err(Error(
            "Nothing to identify yet — analyse the library first.".to_string(),
        ));
    }

    let shared: Shared = Arc::clone(&state);
    let handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let Ok(lookup) = metadata::Lookup::new() else {
            return;
        };
        let total = todo.len();
        let (mut corrected, mut genres) = (0usize, 0usize);

        for (i, (href, title, artist, album, bpm, duration)) in todo.into_iter().enumerate() {
            let facts = lookup.track_facts(&artist, &title);
            let (_art, genre) = lookup.album(&artist, &album);

            if let Ok(mut app) = shared.lock() {
                let entry = app.looked.entry(href.clone()).or_default();
                entry.attempted = true;
                if !genre.trim().is_empty() {
                    entry.genre = genre.clone();
                    genres += 1;
                }
                if let Some(f) = &facts {
                    entry.deezer_bpm = f.bpm;
                    entry.deezer_duration = f.duration;
                }

                // The correction, and every guard on it. Same recording first,
                // then an octave only, then recorded the way a hand correction
                // is — so the beat grid is re-tracked by the machinery that
                // already exists rather than by a second copy of it.
                if let Some(f) = &facts {
                    if metadata::same_recording(duration, f.duration) {
                        if let Some(fixed) = vapor_library::octave_from_reference(bpm, f.bpm) {
                            if app.settings.bpm_override(&href).is_none()
                                && app.settings.set_bpm_override(&href, fixed)
                            {
                                corrected += 1;
                            }
                        }
                    }
                }
                let _ = app.save_looked();
                let _ = app.save_settings();
            }

            let _ = handle.emit(
                "identify-progress",
                &IdentifyProgress {
                    done: i + 1,
                    total,
                    title: title.clone(),
                    corrected,
                    genres,
                    finished: false,
                },
            );

            // Deezer rate-limits, and a library is a lot of requests. Slower
            // than necessary is better than being cut off halfway.
            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        // Re-track the grids of everything just corrected, through the same
        // path a hand correction uses.
        let corrected_hrefs: Vec<String> = shared
            .lock()
            .map(|app| app.settings.bpm_overrides.keys().cloned().collect())
            .unwrap_or_default();
        retrack_grids(&handle, &shared, corrected_hrefs);

        let _ = handle.emit(
            "identify-progress",
            &IdentifyProgress {
                done: total,
                total,
                title: String::new(),
                corrected,
                genres,
                finished: true,
            },
        );
    });

    Ok(())
}

/// Files that could not be read at startup and were moved aside.
///
/// Empty on every normal launch. Non-empty means the app is running on a
/// default for something the person had data for — an unreadable playlists
/// file is indistinguishable from having no playlists, and the difference
/// matters enormously. The bytes were kept; this is what says so.
#[tauri::command]
fn startup_problems(state: State<'_, Shared>) -> Result<Vec<String>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.damaged.iter().map(|d| d.message()).collect())
}

/// Choose the energy curve the set is conducted along.
///
/// Selecting one *is* the action. There is no "conduct from here" button any
/// more: a curve that has been chosen and not applied is a control that lies
/// about what it did.
///
/// The tail is dropped and re-planned, because the curve is the set's
/// destination and the tracks queued behind it were routes to a different one.
/// What is playing, and what is mixing into it right now, are left alone.
#[tauri::command]
fn set_curve(curve: String, state: State<'_, Shared>) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.curve = vapor_library::Curve::parse(&curve).as_str().to_string();
    app.save_settings()?;

    // Everything after the track playing is a route to the old destination.
    let keep = app.queue.current_index().map(|i| i + 1).unwrap_or(0);
    let head: Vec<String> = app.queue.tracks().iter().take(keep).cloned().collect();
    if !head.is_empty() {
        let playing = app.playing.clone();
        app.queue.set_tracks(head, playing.as_deref());
        extend_set(&mut app);
    }
    Ok(app.settings.clone())
}

/// Turn the DJ on or off.
///
/// Persisted, because it decides what the *supervisor* does — it lived in the
/// frontend as component state until 2026-08-17, which meant the half of the
/// app that chooses what plays next had never heard of it.
#[tauri::command]
fn set_dj_mode(enabled: bool, state: State<'_, Shared>) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.dj_mode = enabled;
    app.save_settings()?;
    Ok(app.settings.clone())
}

/// Turn local-network sync on or off.
///
/// Turning it on starts the beacon and the server there and then, because
/// "restart the app" is not an answer to "I pressed the switch". Turning it off
/// stops them the same way (TD-58): the trust is cleared and the two threads
/// are stopped and joined, so the machine is neither announcing itself nor
/// holding a port by the time this returns.
#[tauri::command]
fn set_sync_enabled(enabled: bool, state: State<'_, Shared>) -> Result<Settings> {
    let shared: Shared = Arc::clone(&state);
    let (start, session) = {
        let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
        let was = app.settings.sync_enabled;
        app.settings.sync_enabled = enabled;
        let session = if enabled {
            None
        } else {
            // Off means off: a device that can no longer be discovered should
            // not still be trusted by one that can.
            app.trust = vapor_library::sync::Trust::new();
            app.pairing = None;
            app.pin = None;
            app.save_trust()?;
            app.sync_session.take()
        };
        app.save_settings()?;
        (enabled && !was, session)
    };

    // Outside the lock. Stopping joins two threads, and one of them takes the
    // peer registry's lock on the way round — holding the app lock across that
    // is a deadlock waiting for the right moment.
    if let Some(session) = session {
        session.stop();
    }

    if start {
        let (id, name, kind, registry) = {
            let app = shared.lock().map_err(|e| Error(e.to_string()))?;
            let kind = if cfg!(any(target_os = "ios", target_os = "android")) {
                vapor_library::sync::DeviceKind::Phone
            } else {
                vapor_library::sync::DeviceKind::Desktop
            };
            let _ = app.store.save("device_id", &app.device_id);
            (
                app.device_id.clone(),
                app.device_name(),
                kind,
                Arc::clone(&app.peers),
            )
        };
        let started = peers::start(
            registry,
            id,
            name,
            kind,
            Arc::new(ServedLibrary(Arc::clone(&shared))),
        );
        if let Ok(mut app) = shared.lock() {
            app.sync_session = started;
        }
    }

    let app = shared.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.settings.clone())
}

/// Set the Vibe Limit — §6's Mix Tuner.
///
/// Refused rather than clamped when it is not a number at all: a slider cannot
/// produce one, so a NaN here means something else is wrong and quietly
/// substituting a value would hide it. Out-of-range *is* clamped, because the
/// ends of the slider are exactly the ends of the band.
#[tauri::command]
fn set_vibe_limit(limit: f32, state: State<'_, Shared>) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !limit.is_finite() {
        return Err(Error("That is not a Vibe Limit.".to_string()));
    }
    app.settings.vibe_limit = limit.clamp(
        vapor_library::settings::MIN_VIBE_LIMIT,
        vapor_library::settings::MAX_VIBE_LIMIT,
    );
    app.save_settings()?;
    Ok(app.settings.clone())
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
fn looked_up_image(url: String, state: State<'_, Shared>) -> Result<Option<String>> {
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

/// Artwork for an album, as a `data:` URI.
///
/// Three sources, in an order that respects both the file and the person:
///
/// 1. **A choice made by hand.** It outranks everything, because it exists
///    precisely for the case where the other two are wrong.
/// 2. **The file's own embedded artwork** — right most of the time, free, and
///    available with no network.
/// 3. **A looked-up cover**, which is the fallback for a file that carries no
///    picture at all.
///
/// `prefer_looked_up_art` swaps 2 and 3 for a library whose tags are known to
/// be poor. It is off by default and should stay that way: album search is
/// fuzzy, and a library-wide preference lets one wrong match replace good art
/// on a record nobody was looking at.
fn resolve_album_cover(app: &AppState, album: &str, lead: &str) -> Option<String> {
    let cached = |url: &str| metadata::image_data_uri(&metadata::image_path(app.store.dir(), url));
    let embedded = || app.tags.get(lead).and_then(|t| t.cover.clone());
    let looked = || {
        app.looked
            .get(lead)
            .map(|l| l.album_art.clone())
            .filter(|u| !u.trim().is_empty())
            .and_then(|u| cached(&u))
    };

    if let Some(chosen) = app.settings.album_art_for(album, lead) {
        // A chosen cover whose bytes have been evicted falls through rather
        // than showing nothing: the choice is still recorded and will resolve
        // again once the picture is re-fetched.
        if let Some(data) = cached(chosen) {
            return Some(data);
        }
    }

    if app.settings.prefer_looked_up_art {
        looked().or_else(embedded)
    } else {
        embedded().or_else(looked)
    }
}

/// An album's cover, and where it came from.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlbumArt {
    /// The picture, as a `data:` URI. `None` when there is none to show.
    src: Option<String>,
    /// Whether this is a cover someone chose by hand rather than the file's own.
    ///
    /// Answered here rather than by the screen comparing keys. The album key's
    /// format is this module's business, and a copy of it in TypeScript would
    /// be a second definition free to drift from the first.
    chosen: bool,
}

fn album_art(app: &AppState, album: &str, lead: &str) -> AlbumArt {
    AlbumArt {
        src: resolve_album_cover(app, album, lead),
        chosen: app.settings.album_art_for(album, lead).is_some(),
    }
}

/// Artwork for one album, resolved. See [`resolve_album_cover`].
#[tauri::command]
fn album_cover(album: String, lead: String, state: State<'_, Shared>) -> Result<AlbumArt> {
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
async fn find_album_art(
    album: String,
    artist: String,
    lead: String,
    state: State<'_, Shared>,
) -> Result<AlbumArt> {
    let dir = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.store.dir().to_path_buf()
    };

    // Outside the lock: this is a network call, and holding the state across it
    // would freeze playback control and every other command behind it.
    let lookup = metadata::Lookup::new().map_err(Error)?;
    let (url, _genre) = lookup.album(&artist, &album);
    if url.is_empty() {
        return Err(Error(format!(
            "Nothing came back for “{album}”. The album or artist may be \
             spelled differently on the service than in your tags."
        )));
    }
    lookup.download_image(&url, &dir);

    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.set_album_art(&album, &lead, &url);
    app.save_settings()?;
    Ok(album_art(&app, &album, &lead))
}

/// Forget a hand-chosen cover and go back to what the file carries.
#[tauri::command]
fn clear_album_art(album: String, lead: String, state: State<'_, Shared>) -> Result<AlbumArt> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if app.settings.set_album_art(&album, &lead, "") {
        app.save_settings()?;
    }
    Ok(album_art(&app, &album, &lead))
}

/// Whether looked-up artwork outranks the file's own.
#[tauri::command]
fn set_prefer_looked_up_art(enabled: bool, state: State<'_, Shared>) -> Result<Settings> {
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
fn artist_portrait(name: String, state: State<'_, Shared>) -> Result<Option<String>> {
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

/// Turn lookups on or off.
///
/// Switching it off forgets what was found as well as stopping further
/// requests: leaving a cache of third-party data behind after someone has said
/// no is not what "off" means to the person pressing it.
#[tauri::command]
fn set_metadata_lookup(enabled: bool, state: State<'_, Shared>) -> Result<Settings> {
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

// ---------------------------------------------------------------------------
// Local sync (SYNC-001 to SYNC-005)
// ---------------------------------------------------------------------------

/// What a running sync is doing, for the dashboard.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncProgress {
    running: bool,
    /// The device being synced with.
    peer: String,
    /// What is moving right now.
    file: String,
    done: usize,
    total: usize,
    /// Bytes moved since this sync started.
    bytes: u64,
    /// Seconds since it started, so the frontend can divide rather than have
    /// a rate pushed at it that is already stale by the time it renders.
    elapsed: f64,
    /// Set when the sync ended badly. Cleared when the next one starts.
    error: String,
}

/// What may be moved. The dashboard's filters (SYNC-005).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncWhat {
    #[serde(default = "yes")]
    tracks: bool,
    #[serde(default = "yes")]
    playlists: bool,
}

fn yes() -> bool {
    true
}

impl Default for SyncWhat {
    fn default() -> Self {
        SyncWhat {
            tracks: true,
            playlists: true,
        }
    }
}

/// The app, as the sync server sees it.
///
/// A thin adapter so `peers.rs` depends on a trait it can be tested against
/// rather than on `AppState`, which cannot be built outside a running app.
struct ServedLibrary(Shared);

impl peers::Library for ServedLibrary {
    fn trust(&self) -> vapor_library::sync::Trust {
        self.0
            .lock()
            .map(|app| app.trust.clone())
            .unwrap_or_default()
    }

    fn pair(
        &self,
        device_id: &str,
        name: &str,
        kind: vapor_library::sync::DeviceKind,
        pin: &str,
    ) -> vapor_library::sync::PairOutcome {
        use vapor_library::sync::PairOutcome;

        let Ok(mut app) = self.0.lock() else {
            return PairOutcome::Refused;
        };
        let Some(pairing) = app.pairing.as_mut() else {
            // Nobody pressed "pair" on this device. A plausible code is not an
            // invitation.
            return PairOutcome::Refused;
        };

        let outcome = pairing.offer(device_id, pin, peers::now());
        match outcome {
            PairOutcome::Paired => {
                app.trust.add(device_id, name, kind, peers::now());
                app.pairing = None;
                let _ = app.save_trust();
            }
            PairOutcome::Refused => app.pairing = None,
            PairOutcome::WrongPin { .. } => {}
        }
        outcome
    }

    fn manifest(&self) -> vapor_library::sync::Manifest {
        self.0
            .lock()
            .map(|mut app| build_manifest(&mut app))
            .unwrap_or_default()
    }

    fn read_track(&self, href: &str) -> Option<Vec<u8>> {
        let path = {
            let app = self.0.lock().ok()?;
            // Two conditions, and both matter. In the library, so a peer
            // cannot name a path; cached, so this serves a file this device
            // actually holds rather than fetching one from the owner's cloud
            // on a stranger's behalf.
            if !app.rows.iter().any(|r| r.href == href) {
                return None;
            }
            app.cache.get(href)?
        };
        std::fs::read(path).ok()
    }

    fn identity(&self) -> (String, String, vapor_library::sync::DeviceKind) {
        let kind = if cfg!(any(target_os = "ios", target_os = "android")) {
            vapor_library::sync::DeviceKind::Phone
        } else {
            vapor_library::sync::DeviceKind::Desktop
        };
        self.0
            .lock()
            .map(|app| (app.device_id.clone(), app.device_name(), kind))
            .unwrap_or_default()
    }
}

/// This device's manifest: what it knows, for another device to compare.
fn build_manifest(app: &mut AppState) -> vapor_library::sync::Manifest {
    use vapor_library::sync::{Manifest, PlaylistRecord, TrackRecord};

    let hrefs: Vec<String> = app.rows.iter().map(|r| r.href.clone()).collect();
    let tracks = hrefs
        .into_iter()
        .map(|href| {
            let digest = app.digest_of(&href).unwrap_or_default();
            let size = app
                .cache
                .get(&href)
                .and_then(|p| std::fs::metadata(p).ok())
                .map_or(0, |m| m.len());
            // The tempo correction is the thing a person actually changes
            // about a track, so it is what makes one device's record newer.
            let updated = u64::from(app.settings.bpm_override(&href).is_some());
            TrackRecord {
                href,
                size,
                digest,
                updated,
            }
        })
        .collect();

    let playlists = app
        .playlists
        .all()
        .iter()
        .map(|p| PlaylistRecord {
            id: p.id.clone(),
            name: p.name.clone(),
            digest: vapor_library::sync::playlist_digest(&p.name, &p.tracks),
            updated: p.tracks.len() as u64,
        })
        .collect();

    let _ = app.store.save("digests", &app.digests);

    Manifest {
        device_id: app.device_id.clone(),
        tracks,
        playlists,
        generated: peers::now(),
    }
}

/// Everything the sync dashboard draws.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncView {
    /// Whether local sync is switched on at all.
    enabled: bool,
    /// This device, as others see it.
    device_id: String,
    device_name: String,
    /// Seen on the network and not yet paired.
    discovered: Vec<vapor_library::sync::Peer>,
    trusted: Vec<vapor_library::sync::TrustedDevice>,
    /// The code this device is currently showing, if any.
    pin: Option<String>,
    pairing_with: Option<String>,
    progress: SyncProgress,
}

#[tauri::command]
fn sync_view(state: State<'_, Shared>) -> Result<SyncView> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !app.settings.sync_enabled {
        return Ok(SyncView {
            enabled: false,
            device_id: app.device_id.clone(),
            device_name: app.device_name(),
            discovered: Vec::new(),
            trusted: Vec::new(),
            pin: None,
            pairing_with: None,
            progress: SyncProgress::default(),
        });
    }
    let discovered = app
        .peers
        .lock()
        .map(|mut registry| registry.live(peers::now()).to_vec())
        .unwrap_or_default();

    Ok(SyncView {
        enabled: true,
        device_id: app.device_id.clone(),
        device_name: app.device_name(),
        discovered,
        trusted: app.trust.all().to_vec(),
        pin: app.pin.clone(),
        pairing_with: app.pairing.as_ref().map(|p| p.peer_id().to_string()),
        progress: app.sync.clone(),
    })
}

/// Show a code, for `peer_id` to type in.
///
/// The code is bound to that one device, so a PIN on screen is not an
/// invitation to everything else on the subnet that can see it.
#[tauri::command]
fn open_pairing(peer_id: String, state: State<'_, Shared>) -> Result<String> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let pin = peers::new_pin();
    app.pairing = Some(vapor_library::sync::Pairing::begin(
        pin.clone(),
        &peer_id,
        peers::now(),
    ));
    app.pin = Some(pin.clone());
    Ok(pin)
}

#[tauri::command]
fn cancel_pairing(state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.pairing = None;
    app.pin = None;
    Ok(())
}

/// Type the code the other device is showing.
#[tauri::command]
fn pair_with(peer_id: String, pin: String, state: State<'_, Shared>) -> Result<String> {
    let (address, me, my_name, kind) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        let registry = app.peers.lock().map_err(|e| Error(e.to_string()))?;
        let peer = registry
            .get(&peer_id)
            .ok_or_else(|| Error("That device is no longer on the network.".to_string()))?;
        (
            peer.address.clone(),
            app.device_id.clone(),
            app.device_name(),
            peer.kind,
        )
    };

    // A crafted advert must not be able to point this device at a host on the
    // internet and have it open a connection there.
    if !peers::is_local(&address) {
        return Err(Error(
            "That device is not on this local network.".to_string(),
        ));
    }

    let (reply, _) = peers::ask(
        &address,
        &peers::Request::Pair {
            device_id: me,
            name: my_name,
            device_kind: if cfg!(any(target_os = "ios", target_os = "android")) {
                vapor_library::sync::DeviceKind::Phone
            } else {
                vapor_library::sync::DeviceKind::Desktop
            },
            pin,
        },
    )
    .map_err(Error)?;

    match reply {
        peers::Reply::Paired { device_id, name } => {
            let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
            app.trust.add(&device_id, &name, kind, peers::now());
            app.save_trust()?;
            Ok(name)
        }
        peers::Reply::Refused { reason } => Err(Error(reason)),
        _ => Err(Error(
            "That device answered with something else.".to_string(),
        )),
    }
}

#[tauri::command]
fn forget_peer(peer_id: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let forgotten = app.trust.forget(&peer_id);
    if forgotten {
        app.save_trust()?;
    }
    Ok(forgotten)
}

/// Sync with a paired device, on a thread, reporting progress as it goes.
#[tauri::command]
fn sync_with(
    peer_id: String,
    what: Option<SyncWhat>,
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<()> {
    use tauri::Emitter as _;

    let shared: Shared = Arc::clone(&state);
    let what = what.unwrap_or_default();

    let (address, name) = {
        let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;
        if app.sync.running {
            return Err(Error("A sync is already running.".to_string()));
        }
        if !app.trust.allows(&peer_id) {
            return Err(Error("That device is not paired.".to_string()));
        }
        let registry = app.peers.lock().map_err(|e| Error(e.to_string()))?;
        let peer = registry
            .get(&peer_id)
            .ok_or_else(|| Error("That device is not on the network.".to_string()))?;
        let found = (peer.address.clone(), peer.name.clone());
        drop(registry);

        if !peers::is_local(&found.0) {
            return Err(Error(
                "That device is not on this local network.".to_string(),
            ));
        }
        app.sync = SyncProgress {
            running: true,
            peer: found.1.clone(),
            ..Default::default()
        };
        found
    };

    let started = std::time::Instant::now();
    let spawned = std::thread::Builder::new()
        .name("vapor-sync".into())
        .spawn(move || {
            let outcome = run_sync(&shared, &address, what, started);
            if let Ok(mut app) = shared.lock() {
                app.sync.running = false;
                app.sync.elapsed = started.elapsed().as_secs_f64();
                if let Err(e) = outcome {
                    app.sync.error = e;
                }
            }
            let _ = app_handle.emit("sync-changed", ());
        });

    if let Err(e) = spawned {
        // The thread never started, so nothing will ever clear the flag it was
        // set under. Left running, the dashboard shows a sync that is not
        // happening and refuses to start another.
        let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.sync.running = false;
        app.sync.error = format!("could not start the sync: {e}");
        return Err(Error(format!("could not start the sync: {e}")));
    }
    let _ = name;
    Ok(())
}

/// The body of a sync. Runs on its own thread and holds the lock only in
/// short bursts — a sync can take minutes, and the app has to stay usable.
fn run_sync(
    shared: &Shared,
    address: &str,
    what: SyncWhat,
    started: std::time::Instant,
) -> std::result::Result<(), String> {
    let me = shared.lock().map_err(|e| e.to_string())?.device_id.clone();

    let (reply, _) = peers::ask(
        address,
        &peers::Request::Manifest {
            device_id: me.clone(),
        },
    )?;
    let theirs = match reply {
        peers::Reply::Manifest(m) => *m,
        peers::Reply::Refused { reason } => return Err(reason),
        _ => return Err("that device answered with something else".to_string()),
    };

    let mine = {
        let mut app = shared.lock().map_err(|e| e.to_string())?;
        build_manifest(&mut app)
    };
    let delta = vapor_library::sync::reconcile(&mine, &theirs);

    // Only what this device is missing is pulled. Pushing would mean writing
    // to someone else's library on their behalf, and the other device runs
    // this same code when its owner asks it to.
    let mut wanted: Vec<String> = Vec::new();
    if what.tracks {
        wanted.extend(delta.fetch.iter().cloned());
        wanted.extend(delta.replace.iter().cloned());
    }
    let total = wanted.len()
        + if what.playlists {
            delta.take_playlists.len()
        } else {
            0
        };

    {
        let mut app = shared.lock().map_err(|e| e.to_string())?;
        app.sync.total = total;
    }

    for (done, href) in wanted.iter().enumerate() {
        {
            let mut app = shared.lock().map_err(|e| e.to_string())?;
            app.sync.done = done;
            app.sync.file = href.rsplit('/').next().unwrap_or(href).to_string();
            app.sync.elapsed = started.elapsed().as_secs_f64();
        }
        if let Err(e) = pull_track(shared, address, &me, href) {
            // One track that will not come across must not end the sync — a
            // library with one unreadable file would otherwise never finish
            // one.
            eprintln!("sync: {href} did not transfer ({e})");
        }
    }

    if what.playlists {
        for id in &delta.take_playlists {
            if let Some(list) = theirs.playlists.iter().find(|p| &p.id == id) {
                let mut app = shared.lock().map_err(|e| e.to_string())?;
                // Names and membership only. A playlist arrives as a list of
                // hrefs that may not all be here yet, and rows_in_order
                // already skips what the library does not hold.
                if app.playlists.get(id).is_none() {
                    app.playlists.create(id.clone(), list.name.clone());
                }
            }
        }
        let app = shared.lock().map_err(|e| e.to_string())?;
        app.save_playlists().map_err(|e| e.0)?;
    }

    let mut app = shared.lock().map_err(|e| e.to_string())?;
    app.sync.done = total;
    app.sync.file.clear();
    let _ = app.store.save("digests", &app.digests);
    Ok(())
}

/// Pull one track, a chunk at a time, resuming from whatever is on disk.
fn pull_track(
    shared: &Shared,
    address: &str,
    me: &str,
    href: &str,
) -> std::result::Result<(), String> {
    let mut collected: Vec<u8> = Vec::new();
    let mut expected_digest;

    loop {
        let have = collected.len() as u64;
        let (reply, body) = peers::ask(
            address,
            &peers::Request::Fetch {
                device_id: me.to_string(),
                href: href.to_string(),
                offset: have,
                len: vapor_library::sync::CHUNK,
            },
        )?;

        let (len, total, digest) = match reply {
            peers::Reply::Bytes { len, total, digest } => (len, total, digest),
            peers::Reply::Error { reason } | peers::Reply::Refused { reason } => {
                return Err(reason)
            }
            _ => return Err("that device answered with something else".to_string()),
        };
        expected_digest = digest;

        if len == 0 {
            if have < total {
                // The peer says there is more and then sends none. Continuing
                // would spin forever asking for the same offset.
                return Err("the transfer stalled".to_string());
            }
            break;
        }
        collected.extend_from_slice(&body);

        {
            let mut app = shared.lock().map_err(|e| e.to_string())?;
            app.sync.bytes += len;
        }

        if collected.len() as u64 >= total {
            break;
        }
    }

    // Verified before it is written, not after. A file that lands in the cache
    // and is then found to be wrong has already been offered to the decoder.
    let actual = vapor_library::sync::digest(&collected);
    if actual != expected_digest {
        return Err("the copy that arrived does not match the original".to_string());
    }

    let app = shared.lock().map_err(|e| e.to_string())?;
    // `store` skips an href it already holds, which is what makes a `replace`
    // need the removal first.
    let _ = app.cache.remove(href);
    app.cache
        .store(href, || Ok(collected))
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// SYNC-006 — the shared document on the WebDAV server
// ---------------------------------------------------------------------------

/// What a round trip to the server changed.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SharedSyncResult {
    playlists_added: usize,
    playlists_extended: usize,
    folders_added: usize,
    tempos_added: usize,
    /// Removed here because another device removed them (TD-57).
    playlists_deleted: usize,
    folders_deleted: usize,
    /// True when there was no document there and this device wrote the first.
    created: bool,
}

/// This device's contribution to the shared document.
fn shared_document(app: &AppState) -> vapor_library::sync::Shared {
    vapor_library::sync::Shared {
        version: vapor_library::sync::SHARED_VERSION,
        written_by: app.device_id.clone(),
        updated: peers::now(),
        playlists: app.playlists.all().to_vec(),
        folders: app.folders.all().to_vec(),
        bpm_overrides: app.settings.bpm_overrides.clone(),
        // Published every time, not only when something was just deleted: a
        // device that has been off for a year still has the playlist, and the
        // document is the only place it will ever hear otherwise.
        deleted: app.tombstones.clone(),
    }
}

/// Pull the shared document, merge it in, and push the result back.
///
/// One round trip does both halves on purpose. Pulling without pushing leaves
/// this device's playlists invisible to every other one; pushing without
/// pulling overwrites theirs. Doing them separately means a person has to know
/// to do both, in the right order.
#[tauri::command]
fn sync_shared_document(
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<SharedSyncResult> {
    let (remote_config, href) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        if !app.settings.remote.is_configured() {
            return Err(Error(
                "No server is configured, so there is nowhere to keep it.".to_string(),
            ));
        }
        (
            app.settings.remote.clone(),
            webdav::shared_document_href(&app.settings.remote.folder),
        )
    };

    // Built outside the lock: it reads the keychain and opens a client.
    let fetcher = webdav::Fetcher::new(&remote_config).map_err(Error)?;
    let existing = fetcher.fetch_optional(&href).map_err(Error)?;

    let mut result = SharedSyncResult {
        created: existing.is_none(),
        ..Default::default()
    };

    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

    if let Some(bytes) = existing {
        let incoming: vapor_library::sync::Shared =
            serde_json::from_slice(&bytes).map_err(|e| {
                // Deliberately not overwritten with a fresh one. A document this
                // build cannot read may be one a newer build wrote, and replacing
                // it would delete another device's playlists to fix a parse error.
                Error(format!(
                    "The file on the server could not be read, so nothing was changed: {e}"
                ))
            })?;

        if incoming.version > vapor_library::sync::SHARED_VERSION {
            return Err(Error(
                "That file was written by a newer version of Vapor. Update this device before \
                 syncing, or it would write back less than it read."
                    .to_string(),
            ));
        }

        let AppState {
            playlists,
            folders,
            settings,
            tombstones,
            ..
        } = &mut *app;
        let report = vapor_library::sync::merge_shared(
            playlists,
            folders,
            &mut settings.bpm_overrides,
            tombstones,
            &incoming,
        );
        result.playlists_added = report.playlists_added;
        result.playlists_extended = report.playlists_extended;
        result.folders_added = report.folders_added;
        result.tempos_added = report.tempos_added;
        result.playlists_deleted = report.playlists_deleted;
        result.folders_deleted = report.folders_deleted;

        if !report.is_empty() {
            app.save_playlists()?;
            app.save_folders()?;
            app.save_settings()?;
            // Tombstones learned from the document are kept, so this device
            // passes the deletion on to the next one it syncs with rather than
            // being the place a deletion stops travelling.
            app.save_tombstones()?;
            // A merged playlist changes what the rows mean, and a corrected
            // tempo changes what the table shows.
            let overrides = app.settings.bpm_overrides.clone();
            for row in app.rows.iter_mut() {
                if let Some(bpm) = overrides.get(&row.href) {
                    row.bpm = *bpm;
                }
            }
        }
    }

    let outgoing = shared_document(&app);
    // A tempo that arrived from another device is a correction like any other,
    // and leaves this device's beat grid tracked at the number it replaced. The
    // whole library is offered rather than only the merged hrefs — `stale_grids`
    // is the predicate for what actually needs work, and the report only carries
    // a count.
    let corrected: Vec<String> = if result.tempos_added > 0 {
        app.settings.bpm_overrides.keys().cloned().collect()
    } else {
        Vec::new()
    };
    drop(app);

    if !corrected.is_empty() {
        retrack_grids(&app_handle, state.inner(), corrected);
    }

    let bytes = serde_json::to_vec_pretty(&outgoing).map_err(|e| Error(e.to_string()))?;
    fetcher.put(&href, bytes).map_err(Error)?;

    Ok(result)
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
fn create_playlist(
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

// ---------------------------------------------------------------------------
// Playlist folders
// ---------------------------------------------------------------------------
//
// A thin organisational layer over playlists, ported from
// `playlist_folder_service.gd`. A folder never owns tracks; a playlist carries
// a `folder_id` pointing at one.
//
// `vapor-library` has had `FolderStore` and `Playlist::folder_id` since the
// port, both tested, and the shell exposed neither — so `folderId` arrived on
// the frontend's `Playlist` type as a field nothing could ever set. That is
// the failure mode the handover names: a parameter carried across without its
// behaviour.
//
// `parent_id` is representable so nesting needs no later migration, but
// nothing here creates a nested folder and the rail draws one level, which is
// what the original did too.

#[tauri::command]
fn playlist_folders(state: State<'_, Shared>) -> Result<Vec<vapor_library::Folder>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.folders.all().to_vec())
}

/* ---- Smart groups ----------------------------------------------------
 *
 * A group is a saved set of artists, albums and genres — not of tracks. That is
 * the whole difference from a playlist: membership is worked out against the
 * library each time it is read, so a group takes in records added later without
 * anyone maintaining it.
 *
 * `vapor_library::group` has held all of this, tested, since the port; the same
 * file's `FolderStore` half was wired up and this half was not, so the feature
 * existed everywhere except in the app. These are the commands it was missing.
 * -------------------------------------------------------------------- */

#[tauri::command]
fn dynamic_groups(state: State<'_, Shared>) -> Result<Vec<vapor_library::DynamicGroup>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.groups.all().to_vec())
}

#[tauri::command]
fn create_group(name: String, state: State<'_, Shared>) -> Result<vapor_library::DynamicGroup> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(Error("A group needs a name.".to_string()));
    }
    let id = new_id("group");
    let created = app.groups.create(id, name).clone();
    app.save_groups()?;
    Ok(created)
}

#[tauri::command]
fn rename_group(id: String, name: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(Error("A group needs a name.".to_string()));
    }
    let renamed = app.groups.rename(&id, name);
    if renamed {
        app.save_groups()?;
    }
    Ok(renamed)
}

#[tauri::command]
fn delete_group(id: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let Some(gone) = app.groups.delete(&id) else {
        return Ok(false);
    };
    // `group.rs` asks the caller to clear any cover override keyed to this id,
    // because a stale image outliving its group is what the GDScript's inline
    // call prevented. There is nothing to clear yet: overrides are keyed by
    // album and href, and a group has neither — it has no artwork of its own.
    // When it gets some, this is where letting go of it belongs.
    let _ = gone;
    app.save_groups()?;
    Ok(true)
}

/// Add an artist, album or genre to a group.
///
/// A track is refused rather than quietly turned into its album: a group holds
/// entities, and resolving one for the caller would make the set contain
/// something nobody put in it.
#[tauri::command]
fn add_to_group(
    id: String,
    entity_type: String,
    value: String,
    state: State<'_, Shared>,
) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let Some(kind) = vapor_library::EntityType::parse(&entity_type) else {
        return Err(Error(format!(
            "A smart group holds artists, albums and genres. \"{entity_type}\" is none of those."
        )));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(Error("There is nothing named here to add.".to_string()));
    }
    let added = app.groups.add_entity(&id, kind, value);
    if added {
        app.save_groups()?;
    }
    Ok(added)
}

#[tauri::command]
fn remove_from_group(
    id: String,
    entity_type: String,
    value: String,
    state: State<'_, Shared>,
) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let Some(kind) = vapor_library::EntityType::parse(&entity_type) else {
        return Ok(false);
    };
    let removed = app.groups.remove_entity(&id, kind, &value);
    if removed {
        app.save_groups()?;
    }
    Ok(removed)
}

#[tauri::command]
fn reorder_groups(from: usize, to: usize, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let moved = app.groups.reorder(from, to);
    if moved {
        app.save_groups()?;
    }
    Ok(moved)
}

/// Every track a group currently resolves to.
///
/// Worked out on read rather than stored, which is the point of the feature: a
/// record added to the library after the group was made belongs to it without
/// anyone saying so.
#[tauri::command]
fn group_tracks(id: String, state: State<'_, Shared>) -> Result<Vec<Row>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let Some(group) = app.groups.get(&id) else {
        return Ok(Vec::new());
    };
    Ok(tracks_in_group(&app, group))
}

fn tracks_in_group(app: &AppState, group: &vapor_library::DynamicGroup) -> Vec<Row> {
    use vapor_library::EntityType;
    app.rows
        .iter()
        .filter(|row| {
            group.entities.iter().any(|e| match e.entity_type {
                EntityType::Artist => row.artist == e.value,
                EntityType::Album => row.album == e.value,
                EntityType::Genre => genre_of(app, &row.href) == e.value,
            })
        })
        .cloned()
        .collect()
}

#[tauri::command]
fn create_folder(name: String, state: State<'_, Shared>) -> Result<vapor_library::Folder> {
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
fn rename_folder(id: String, name: String, state: State<'_, Shared>) -> Result<bool> {
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
fn delete_folder(id: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !remove_folder(&mut app, &id) {
        return Ok(false);
    }
    app.save_folders()?;
    app.save_playlists()?;
    app.save_tombstones()?;
    Ok(true)
}

/// The body of [`delete_folder`], split out so it is reachable from a test —
/// a `#[tauri::command]` takes `State`, which cannot be built outside a
/// running app.
fn remove_folder(app: &mut AppState, id: &str) -> bool {
    if app.folders.get(id).is_none() {
        return false;
    }
    let orphaned = app.folders.delete(id);
    // Only this folder: `delete` orphans what was nested inside rather than
    // cascading, so the children still exist and must not be tombstoned.
    app.tombstones.record_folder(id, peers::now());

    let homeless: Vec<String> = app
        .playlists
        .all()
        .iter()
        .filter(|p| p.folder_id == id || orphaned.contains(&p.folder_id))
        .map(|p| p.id.clone())
        .collect();
    for playlist in homeless {
        app.playlists.set_folder(&playlist, "");
    }
    true
}

/// File a playlist into a folder, or out of one with an empty `folder_id`.
#[tauri::command]
fn set_playlist_folder(id: String, folder_id: String, state: State<'_, Shared>) -> Result<bool> {
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
        // Written down before it is forgotten (TD-57). Without this the next
        // sync takes the playlist back from a device that had not heard, and
        // the deletion has to be done again on every device in turn.
        app.tombstones.record_playlist(&id, peers::now());
        app.save_playlists()?;
        app.save_tombstones()?;
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

/// How far ahead the DJ needs the library described.
///
/// Three: the track playing has to be mixed *out of*, and the exits the Vibe
/// screen offers are Stay, Follow and Switch. Analysing further ahead than the
/// set has been planned is work for a route nobody has chosen yet.
const MIX_LOOKAHEAD: usize = 3;

/// Whether anything in the next few tracks still needs describing.
///
/// The pass is ordered from the queue (see `start_analysis`), so restarting it
/// is how the upcoming tracks get to the front. Asking about a window rather
/// than only the current track is what stops the DJ arriving at a record it
/// cannot mix.
fn needs_analysis_soon(app: &AppState, lookahead: usize) -> bool {
    let from = app.queue.current_index().unwrap_or(0);
    app.queue
        .tracks()
        .iter()
        .skip(from)
        .take(lookahead + 1)
        .any(|href| needs_analysis(app, href))
}

#[tauri::command]
fn play_tracks(
    app_handle: tauri::AppHandle,
    hrefs: Vec<String>,
    start: Option<String>,
    state: State<'_, Shared>,
) -> Result<()> {
    let shared: Shared = Arc::clone(&state);

    let jump_the_queue = {
        let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;
        app.queue.set_tracks(hrefs, start.as_deref());
        let current = app.queue.current().map(str::to_string);
        if let Some(current) = current.clone() {
            begin_playback(&shared, &mut app, current);
        }
        // The track being started, and the few behind it.
        //
        // This used to ask only about the current track, which meant a queue
        // whose *next* records were undescribed never re-ordered the pass — so
        // the DJ reached a track it knew nothing about and could not mix into
        // it. `plan_mix` needs the incoming track analysed before it arrives,
        // not while it is arriving.
        //
        // Still bounded, for the reason the old comment gave: restarting costs
        // the track in flight, and paying that to re-order a queue that is
        // already described would be worse than leaving the pass alone.
        needs_analysis_soon(&app, MIX_LOOKAHEAD)
    };

    if jump_the_queue {
        // Lock released above: `start_analysis` takes it.
        start_analysis(&app_handle, &shared)?;
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
    #[serde(serialize_with = "finite64")]
    position: f64,
    #[serde(serialize_with = "finite64")]
    duration: f64,
    #[serde(serialize_with = "finite")]
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
    #[serde(serialize_with = "finite")]
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
/// Everything this device knows about a track's genre.
///
/// Three sources, and the DJ used to see none of them. `app.rows` carries the
/// genre the *scan* found, which is empty for every track in a folder-organised
/// library — the tag is read later, and `apply_tags` merges it in only on the
/// way out to a screen. So the pool the DJ reasons over had an empty genre for
/// all 563 tracks, `same_genre` answered "yes, similar" to every pair, and a
/// genre was never once part of a decision.
///
/// The looked-up genre counts too: it is the only source for a library whose
/// files carry no tags, which is most of them here — 46 of 534.
fn genre_of(app: &AppState, href: &str) -> String {
    let scanned = app
        .rows
        .iter()
        .find(|r| r.href == href)
        .map(|r| r.genre.clone())
        .unwrap_or_default();
    if !scanned.trim().is_empty() {
        return scanned;
    }
    if let Some(tagged) = app.tags.get(href).and_then(|t| t.genre.clone()) {
        if !tagged.trim().is_empty() {
            return tagged;
        }
    }
    app.looked
        .get(href)
        .map(|l| l.genre.clone())
        .filter(|g| !g.trim().is_empty())
        .unwrap_or_default()
}

fn same_genre(app: &AppState, a: &str, b: &str) -> bool {
    let (ga, gb) = (genre_of(app, a), genre_of(app, b));
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
    // A tracked grid follows real tempo drift and real downbeat phase, and is
    // what mixing wants — but only if it was tracked at the tempo now in force.
    // Correcting a track from 256 to 128 leaves a grid built on every eighth,
    // and aligning to that is aligning to the error the correction was made to
    // fix. `retrack_after_correction` re-runs the tracker against the corrected
    // number, and until it lands this falls back to a synthetic grid: a beat at
    // zero, a tempo that never wavers, both false for real music and both still
    // better than a grid at a tempo the person has said is wrong.
    let beats = if analysis.beats_are_for(bpm) {
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
/// What a candidate of a given kind costs, lower being better.
///
/// One definition, used by both the three suggestions on screen and the pick
/// the set actually takes, so the two cannot drift apart.
///
/// Built on [`vapor_library::transition_cost`] — the model ported from the
/// Godot build, which weighs key, tempo, energy *and genre relatedness*. The
/// scoring here used to be a separate ad-hoc formula per kind that mentioned
/// genre nowhere at all, so two tracks from unrelated genres scored exactly as
/// well as two from the same one. That is what made the suggestions feel
/// arbitrary and repetitive: with genre absent, the only things left were key
/// and tempo, and the same handful of tracks win those against everything.
///
/// Each kind then adds what it is *for* on top, because a Switch that scored
/// like a Match would simply be a Match:
///
/// * **Match** — the smoothest harmonic step, so the shared cost is enough.
/// * **Fresh** — §2's target of about 15 BPM and 0.25 of energy of movement,
///   so distance *from that target* is the penalty rather than distance itself.
/// * **Switch** — the effect masks the key, so rhythm and energy carry it.
fn candidate_cost(app: &AppState, from: &TrackMeta, to: &TrackMeta, kind: Exit) -> f32 {
    let base = vapor_library::transition_cost(from, to, app.settings.vibe_limit, 0.0);
    let bpm_diff = (from.bpm - to.bpm).abs();
    let energy_diff = (from.energy_level - to.energy_level).abs();

    match kind {
        Exit::Stay => base,
        Exit::Follow => base + (bpm_diff - 15.0).abs() + (energy_diff - 0.25).abs() * 40.0,
        Exit::Switch => base + energy_diff * 20.0,
    }
}

/// The track the DJ would pick to follow the one playing.
///
/// The same choice `mix_candidates` marks with `aiChoice`, so the screen and the
/// set never disagree about what is coming: it prefers whichever match kind the
/// cycle is on, and falls back to the best of any kind rather than giving up —
/// a Switch is not always available, and a set that stops because no track was
/// different enough is worse than one that carries on with a near match.
fn dj_pick(app: &AppState) -> Option<String> {
    let current = app.playing.clone()?;
    let pool = track_meta_pool(app);
    let from = pool.get(&current)?;

    // Never repeat something already in the set. Without this the DJ can pick
    // the track it just played and loop two tracks forever.
    let queued: std::collections::HashSet<&str> =
        app.queue.tracks().iter().map(String::as_str).collect();

    // Plain cheapest transition. This is the *fallback* for when the planner
    // could not run — an unanalysed library, or a pool too small to search —
    // so it deliberately has no opinion about where the set is going. When the
    // planner does run, `extend_set` never reaches here.
    let mut best: Option<(f32, &str)> = None;
    for (href, to) in &pool {
        if href == &current || queued.contains(href.as_str()) {
            continue;
        }
        let similar = same_genre(app, &current, href);
        let score = candidate_cost(app, from, to, exit_between(from, to, similar));
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, href));
        }
    }
    best.map(|(_, href)| href.to_string())
}

/// How far ahead the set is planned.
///
/// The planner searches ten; queueing all of them is what makes the screen able
/// to say what is coming rather than only what is next. Short enough that
/// choosing a different exit re-plans something recent rather than discarding
/// half an hour of decisions.
const PLAN_AHEAD: usize = 10;

/// Keep the set going: append the DJ's pick when nothing follows the current
/// track.
///
/// This is what makes the Vibe DJ a DJ rather than a screen of suggestions.
/// Until now nothing ever added to the queue — `mix_candidates` displayed
/// choices and `plan_mix` read `peek_next`, so a queue of one had nothing to
/// mix into, repeat-all wrapped it onto itself, and the same track played
/// forever while the screen said "0 to come".
///
/// Returns whether the queue grew, so the caller can tell the UI.
fn extend_set(app: &mut AppState) -> bool {
    if !app.settings.dj_mode {
        return false;
    }
    // `has_more` rather than `peek_next().is_some()`, and the difference is the
    // whole of a set: under repeat-all `peek_next` wraps to the beginning, so at
    // the end of a queue it answers with a track that has already played. Read
    // that way the DJ is told the set is fine and stops extending it.
    if app.queue.has_more() {
        return false;
    }

    // Plan the set, rather than picking one track at a time.
    //
    // `dj_pick` below only knows which single transition is cheapest; it has no
    // idea where the set is going. The planner does — A* over transition cost
    // *plus* how far each step sits from where the curve wants energy and tempo
    // to be by then. Until now it only ran from a button nobody was told to
    // press, so a set that was supposed to arc somewhere just wandered.
    let Some(current) = app.playing.clone() else {
        return false;
    };
    let pool = track_meta_pool(app);
    if pool.contains_key(&current) {
        let planned = vapor_library::generate_mood_path(
            &pool,
            &current,
            vapor_library::Curve::parse(&app.settings.curve),
            app.settings.vibe_limit,
            &skip_penalties(app),
        );
        // Skip the head: `generate_mood_path` starts from the track playing.
        let added = planned
            .iter()
            .skip(1)
            .take(PLAN_AHEAD)
            .filter(|href| app.queue.append(href))
            .count();
        if added > 0 {
            return true;
        }
    }

    // Nothing to plan from — an unanalysed library, or a pool too small to
    // search. One cheap transition still beats the music stopping.
    let Some(pick) = dj_pick(app) else {
        return false;
    };
    app.queue.append(&pick)
}

fn plan_mix(app: &AppState, position: f64) -> Option<ArmedMix> {
    let current = app.playing.as_ref()?;
    let next = app.queue.peek_next(None)?.to_string();
    // A single-track queue would otherwise try to mix a track into itself.
    if &next == current {
        return None;
    }

    let outgoing = app.analysis.get(current)?;
    let incoming = app.analysis.get(&next)?;

    // Described is not the same as usable.
    //
    // A record can carry an analysis whose tempo came back as nothing — a
    // track too short or too quiet for the beat tracker to find a pulse in.
    // `track_meta_pool` has always refused those, so the Vibe screen never
    // offered one; the mix planner did not, so the one path that could still
    // reach an undescribed tempo was the queue. Beat-matching against 0 BPM is
    // not a mix, and there is nothing to hear in the result but the fault.
    if outgoing.bpm <= 0.0 || incoming.bpm <= 0.0 {
        return None;
    }

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
fn spawn_supervisor(app_handle: tauri::AppHandle, shared: Shared, controls: Arc<media::Controls>) {
    use tauri::Emitter;

    let spawned = std::thread::Builder::new()
        .name("vapor-playback-supervisor".to_string())
        .spawn(move || loop {
            std::thread::sleep(SUPERVISOR_POLL);

            let Ok(mut app) = shared.lock() else {
                return;
            };

            // The one place that already knows, four times a second, what is
            // playing and whether it still is. `publish_from` drops anything
            // that has not meaningfully changed, so this is not four round
            // trips a second — see `media::worth_sending` — and it hops to the
            // main thread, which is where the media APIs have to be called
            // from and where this was not calling them.
            let showing = now_playing(&app);
            controls.publish_from(&app_handle, showing);

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
                    // Feed the set before looking for a mix: `plan_mix` reads
                    // `peek_next`, so with nothing queued there is nothing to
                    // plan and the track simply ends.
                    if extend_set(&mut app) {
                        drop(app);
                        let _ = app_handle.emit("playback-changed", ());
                        continue;
                    }
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

// ---------------------------------------------------------------------------
// System media controls (MIG-023)
// ---------------------------------------------------------------------------

/// Do what a hardware key or a Control Center button asked for.
///
/// Deliberately routed through the same functions the commands use rather than
/// through the commands themselves — a `#[tauri::command]` takes `State`,
/// which exists only inside an invocation. Two code paths that must agree
/// about what "next" means is exactly how they stop agreeing.
fn handle_media_press(shared: &Shared, app_handle: &tauri::AppHandle, press: media::Press) {
    use tauri::Emitter as _;

    let Ok(mut app) = shared.lock() else { return };

    match press {
        media::Press::Play => {
            if let Some(p) = app.player.as_ref() {
                p.play();
            }
        }
        media::Press::Pause => {
            if let Some(p) = app.player.as_ref() {
                p.pause();
            }
        }
        media::Press::Toggle => {
            if let Some(p) = app.player.as_ref() {
                if p.snapshot().status == audio::Status::Playing {
                    p.pause();
                } else {
                    p.play();
                }
            }
        }
        media::Press::Next => {
            record_skip_if_reacting_to_a_blend(&mut app);
            if let Some(href) = app.queue.next(None).map(str::to_string) {
                begin_playback(shared, &mut app, href);
            }
        }
        media::Press::Previous => {
            if let Some(href) = app.queue.previous().map(str::to_string) {
                begin_playback(shared, &mut app, href);
            }
        }
    }

    drop(app);
    // The screen has to follow the keys. Without this the transport keeps
    // showing the previous track until something else happens to refresh it.
    let _ = app_handle.emit("playback-changed", ());
}

/// The native window, for the one platform that needs one.
///
/// Windows hangs SMTC off a window handle; macOS and Linux ignore it entirely,
/// so asking for one there would be a `tauri::Window` lookup that can only
/// fail. Written as two functions rather than one with a `cfg!` inside,
/// because the Windows body does not compile elsewhere.
#[cfg(target_os = "windows")]
fn window_handle(handle: &tauri::AppHandle) -> Option<*mut std::ffi::c_void> {
    use tauri::Manager as _;
    handle
        .get_webview_window("main")
        // `HWND.0` is already `*mut c_void` in the `windows` version Tauri
        // pins, so a cast here is one clippy rejects — and clippy is only run
        // on Windows for this function, which is how it survived.
        .and_then(|w| w.hwnd().ok())
        .map(|hwnd| hwnd.0)
}

#[cfg(not(target_os = "windows"))]
fn window_handle(_handle: &tauri::AppHandle) -> Option<*mut std::ffi::c_void> {
    None
}

/// What the system should be showing, from what the app is doing.
fn now_playing(app: &AppState) -> media::NowPlaying {
    let snapshot = app.player.as_ref().map(|p| p.snapshot());
    let row = app
        .playing
        .as_ref()
        .and_then(|href| app.rows.iter().find(|r| &r.href == href))
        .cloned()
        .map(|mut row| {
            app.apply_tags(&mut row);
            row
        });

    media::NowPlaying {
        title: row.as_ref().map(|r| r.title.clone()).unwrap_or_default(),
        artist: row.as_ref().map(|r| r.artist.clone()).unwrap_or_default(),
        album: row.as_ref().map(|r| r.album.clone()).unwrap_or_default(),
        duration: snapshot.as_ref().map_or(0.0, |s| s.duration),
        playing: snapshot
            .as_ref()
            .is_some_and(|s| s.status == audio::Status::Playing),
        position: snapshot.as_ref().map_or(0.0, |s| s.position),
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
        app.settings.vibe_limit,
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
            let genre = genre_of(app, &row.href);
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
                    // A hand correction wins outright. Otherwise the genre gets
                    // to resolve which octave the detected tempo is in: a beat
                    // tracker is reliable about the pulse and unreliable about
                    // whether a listener counts it at 87 or 174, and nothing
                    // else the app measures can tell those apart. See
                    // `vapor_library::octave_correct`.
                    bpm: app.settings.bpm_override(&row.href).unwrap_or_else(|| {
                        vapor_library::octave_correct(analysis.bpm, &genre).unwrap_or(analysis.bpm)
                    }),
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
                    // Loudness, not `analysis.energy`. That field is mean RMS
                    // over peak RMS — a *consistency* ratio, which reads a
                    // relentless track as high and one with a breakdown as
                    // low. Measured on this library it puts ballads above drum
                    // & bass, and it was deciding the Build and Chill curves,
                    // the energy term in the transition cost, and whether two
                    // tracks count as a match. See
                    // `vapor_library::intensity_from_lufs`.
                    energy_level: vapor_library::intensity_from_lufs(analysis.lufs),
                    // Not `row.genre`: that is only what the *scan* found, and
                    // in a folder-organised library it is empty for everything.
                    genre,
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
            app.settings.vibe_limit,
            // What the app has learned from being skipped (TD-14).
            &skip_penalties(&app),
        ),
        considered,
        skipped,
    })
}

/// The three ways out of the track that is playing.
///
/// **Intentions, not similarity classes.** The old model sorted candidates into
/// Match, Fresh and Switch by how alike they were, then picked one per step from
/// a rotating cycle and marked it "AI choice". That fights the planner: the
/// cycle's pick and the set's own next track were computed by different code and
/// could disagree about what happens next.
///
/// Here the planner owns the set and these are the ways a person can steer it.
/// `Follow` is not a recommendation, it is what happens if nobody touches
/// anything — which is why there is no badge any more, and no cycle to reason
/// about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Exit {
    /// Hold roughly where the set is now, without advancing the curve.
    Stay,
    /// The planner's next track. The default, always.
    Follow,
    /// Branch off: audibly different, still mixable. Re-plans the tail toward
    /// the same destination the curve already had.
    Switch,
}

impl Exit {
    fn label(self) -> &'static str {
        match self {
            Exit::Stay => "STAY",
            Exit::Follow => "FOLLOW",
            Exit::Switch => "SWITCH",
        }
    }
}

/// How far apart two tracks have to be to count as a Switch.
///
/// Taken from the distribution of the owner's own library rather than invented:
/// across 4,000 random pairs the median tempo gap is 25 BPM and the median
/// intensity gap 0.14, with the 90th percentiles at 59 BPM and 0.35. These sit
/// around the 75th–90th, so a Switch is genuinely one of the more distant
/// jumps available rather than merely "not a match".
const SWITCH_INTENSITY: f32 = 0.30;
const SWITCH_BPM: f32 = 45.0;

/// And how close they have to be to count as a Match: below the median of both.
const MATCH_INTENSITY: f32 = 0.15;
const MATCH_BPM: f32 = 8.0;

/// Which of the three exits one track is from another.
///
/// **Distance, not a genre label.** This used to return `Switch` if and only if
/// the two genres differed, and `same_genre` treats an unknown genre as
/// similar — so on a library carrying 46 genre tags across 534 tracks the
/// branch was dead and the screen could never offer a third choice. Drum & bass
/// into Sade is 25 BPM and 0.35 of intensity apart, the 90th percentile of this
/// library, and it was being called a Match.
///
/// A known difference of genre still forces a Switch. It is good evidence when
/// it is there; it simply is not the only evidence, and it was never present.
fn exit_between(from: &TrackMeta, to: &TrackMeta, similar_genre: bool) -> Exit {
    let bpm_diff = (from.bpm - to.bpm).abs();
    let intensity_diff = (from.energy_level - to.energy_level).abs();

    if !similar_genre || intensity_diff >= SWITCH_INTENSITY || bpm_diff >= SWITCH_BPM {
        return Exit::Switch;
    }
    if bpm_diff >= MATCH_BPM || intensity_diff >= MATCH_INTENSITY {
        return Exit::Follow;
    }
    Exit::Stay
}

/// One option for what plays next.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MixCandidate {
    href: String,
    title: String,
    artist: String,
    #[serde(serialize_with = "finite")]
    bpm: f32,
    key: String,
    /// "stay" | "follow" | "switch".
    exit: Exit,
    /// The word on the card: STAY, FOLLOW, SWITCH.
    label: String,
    /// The mix the engine would actually perform to get there.
    transition: String,
    /// Whether this is what is actually queued next.
    ///
    /// What is actually queued next: §4 moves the selection to a manual
    /// override and leaves the badge where it was, so an override reads as one
    /// rather than as the DJ having chosen it all along.
    selected: bool,
    /// The sleeve, as the design's alternates carry one.
    cover: Option<String>,
}

/// The three ways out of the playing track, and which one the DJ would take.
///
/// §2–§4 of `docs/ai_dj_workflow.md`. The curve decides where the set is going;
/// this decides the next step. Both existed in the original — `play_harmonic_
/// shuffle` planned the arc and this chose each transition — and the rewrite
/// shipped only the first, so the screen could plan a set but never show the
/// choice it was making or let anyone overrule it.
///
/// One candidate per kind, each the best of its kind rather than the best
/// overall, because the point is to offer three genuinely different exits.
#[tauri::command]
fn mix_candidates(state: State<'_, Shared>) -> Result<Vec<MixCandidate>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(mix_candidates_for(&app))
}

/// The body of [`mix_candidates`], reachable from a test.
///
/// A `#[tauri::command]` takes `State`, which cannot be built outside a running
/// app, so logic left in a command body is logic no test can see.
fn mix_candidates_for(app: &AppState) -> Vec<MixCandidate> {
    let Some(current) = app.playing.clone() else {
        return Vec::new();
    };
    let pool = track_meta_pool(app);
    let Some(from) = pool.get(&current) else {
        return Vec::new();
    };

    // Everything analysed except the track playing and what it has already
    // been through: offering the track you just heard is not an option.
    let played: std::collections::HashSet<&str> = app
        .queue
        .tracks()
        .iter()
        .take(app.queue.current_index().unwrap_or(0))
        .map(String::as_str)
        .collect();

    // The three exits are filled by different questions, which is the whole
    // point of the redesign: Follow is *the plan's* next track, not the most
    // similar one. Asking "which candidate is most Follow-like" is what let the
    // suggestion and the set disagree.
    let queued = app.queue.peek_next(None).map(str::to_string);

    // Everything that could be either card. Gathered first so Stay and Switch
    // are chosen in order rather than in one pass: Switch has to know what Stay
    // took, or on a small library both land on the same track and the screen is
    // back to two cards.
    let candidates: Vec<&TrackMeta> = pool
        .iter()
        .filter(|(href, _)| {
            href.as_str() != current
                && !played.contains(href.as_str())
                // Already the Follow card.
                && queued.as_deref() != Some(href.as_str())
        })
        .map(|(_, to)| to)
        .collect();

    // Stay is a question every candidate can answer — "how little does the
    // level move" — so it is asked of all of them rather than only of the ones
    // `exit_between` puts in the Stay band. A library holding nothing within
    // 8 BPM of what is playing still has a closest track, and showing two cards
    // because the third did not clear a threshold is the screen withholding an
    // answer it has.
    let stay = candidates.iter().copied().min_by(|a, b| {
        let score = |t: &TrackMeta| {
            (from.energy_level - t.energy_level).abs() * 100.0
                + candidate_cost(app, from, t, Exit::Stay)
        };
        score(a).total_cmp(&score(b))
    });

    // Switch is not "the most different track" — it is a real exit the engine
    // can still perform, so among the candidates that are genuinely a departure
    // it is judged on transition cost.
    let departing = candidates
        .iter()
        .copied()
        .filter(|t| stay.is_none_or(|s| s.href != t.href))
        .filter(|t| exit_between(from, t, same_genre(app, &current, &t.href)) == Exit::Switch)
        .min_by(|a, b| {
            candidate_cost(app, from, a, Exit::Switch).total_cmp(&candidate_cost(
                app,
                from,
                b,
                Exit::Switch,
            ))
        });

    // And when nothing clears the thresholds there is still a furthest track,
    // which is a more honest card than no card: the alternative is a screen
    // that silently offers two exits and gives no reason, which is what it did.
    let switch = departing.or_else(|| {
        candidates
            .iter()
            .copied()
            .filter(|t| stay.is_none_or(|s| s.href != t.href))
            .max_by(|a, b| {
                let far = |t: &TrackMeta| {
                    (from.energy_level - t.energy_level).abs() * 100.0 + (from.bpm - t.bpm).abs()
                };
                far(a).total_cmp(&far(b))
            })
    });

    // Follow is the plan's next track. When there is no plan yet — nothing has
    // been queued behind what is playing — there is still an answer to "where
    // would the DJ go from here", and it is the same question `candidate_cost`
    // asks. Without this the screen opened on two cards and grew a third the
    // moment anything was queued, which reads as a bug in the DJ rather than an
    // absent plan.
    //
    // Stay and Switch already do exactly this: both fall back rather than
    // withhold a card that nothing cleared a threshold for. Follow was the one
    // exit that could still come back empty.
    let follow = queued
        .as_deref()
        // A queue with nothing after the current track wraps under repeat-all,
        // so `peek_next` answers with the record already playing. Offered as
        // Follow that is a card saying "next, this again".
        .filter(|h| *h != current.as_str())
        .and_then(|h| pool.get(h))
        // Two cards pointing at one track is the same failure wearing a second
        // label, which the planned case already refuses to do.
        .filter(|t| stay.is_none_or(|s| s.href != t.href))
        .filter(|t| switch.is_none_or(|s| s.href != t.href))
        .or_else(|| {
            candidates
                .iter()
                .copied()
                .filter(|t| stay.is_none_or(|s| s.href != t.href))
                .filter(|t| switch.is_none_or(|s| s.href != t.href))
                .min_by(|a, b| {
                    candidate_cost(app, from, a, Exit::Follow).total_cmp(&candidate_cost(
                        app,
                        from,
                        b,
                        Exit::Follow,
                    ))
                })
        });
    let chosen: Vec<(&TrackMeta, Exit)> = [
        stay.map(|t| (t, Exit::Stay)),
        follow.map(|t| (t, Exit::Follow)),
        switch.map(|t| (t, Exit::Switch)),
    ]
    .into_iter()
    .flatten()
    .collect();

    chosen
        .into_iter()
        .map(|(to, exit)| {
            let row = app.rows.iter().find(|r| r.href == to.href);
            MixCandidate {
                href: to.href.clone(),
                title: row.map(|r| r.title.clone()).unwrap_or_default(),
                artist: row
                    .filter(|r| r.artist_source != vapor_library::index::Source::Unknown)
                    .map(|r| r.artist.clone())
                    .unwrap_or_default(),
                bpm: to.bpm,
                key: to.musical_key.clone(),
                exit,
                label: exit.label().to_string(),
                transition: transition_name(choose_transition(
                    &from.musical_key,
                    &to.musical_key,
                    (from.bpm - to.bpm).abs(),
                    same_genre(app, &current, &to.href),
                )),
                selected: queued.as_deref() == Some(to.href.as_str()),
                cover: app.tags.get(&to.href).and_then(|t| t.cover.clone()),
            }
        })
        .collect()
}

/// Take one of the three exits, and re-plan the set behind it.
///
/// The curve owns the destination and the exit owns the next step, so
/// overriding one step must not throw away the arc: the tail is re-searched
/// from the chosen track along the same curve. Being 60% through a Build stays
/// 60% through a Build — the route changes, not where it is going.
#[tauri::command]
fn choose_next(href: String, curve: String, state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

    if !app.queue.set_next(&href) {
        return Err(Error("That track is no longer in the queue.".to_string()));
    }

    let pool = track_meta_pool(&app);
    if !pool.contains_key(&href) {
        // Unanalysed: it can still play next, there is simply nothing to plan
        // a route from.
        return Ok(());
    }

    let tail = vapor_library::generate_mood_path(
        &pool,
        &href,
        Curve::parse(&curve),
        app.settings.vibe_limit,
        &skip_penalties(&app),
    );

    // Everything up to and including the track playing is history and stays
    // put; the tail is what the DJ is still free to arrange.
    let played: Vec<String> = app
        .queue
        .tracks()
        .iter()
        .take(app.queue.current_index().unwrap_or(0) + 1)
        .cloned()
        .collect();
    let mut next: Vec<String> = played;
    for h in tail {
        if !next.contains(&h) {
            next.push(h);
        }
    }
    let current = app.queue.current().map(str::to_string);
    app.queue.set_tracks(next, current.as_deref());
    Ok(())
}

/// What the next blend will do, in the terms the Vibe screen states them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BlendPreview {
    from_title: String,
    to_title: String,
    #[serde(serialize_with = "finite")]
    from_bpm: f32,
    #[serde(serialize_with = "finite")]
    to_bpm: f32,
    from_key: String,
    to_key: String,
    /// Tempo change the incoming deck will be stretched by, as a percentage.
    #[serde(serialize_with = "finite")]
    shift_percent: f32,
    /// Loudness difference, in LU, which is what a gain trim would correct.
    #[serde(serialize_with = "finite")]
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
///
/// The number is stored here and returns immediately; the beat grid it implies
/// is re-tracked in the background by [`retrack_after_correction`], because a
/// correction that only changed the label would leave mixing aligned to the
/// tempo it was made to reject.
#[tauri::command]
fn set_bpm_override(
    href: String,
    bpm: f32,
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !app.settings.set_bpm_override(&href, bpm) {
        return Err(Error(format!(
            "A BPM has to be between {} and {}.",
            vapor_library::MIN_MANUAL_BPM as u32,
            vapor_library::MAX_MANUAL_BPM as u32
        )));
    }
    app.save_settings()?;
    drop(app);

    retrack_grids(&app_handle, state.inner(), vec![href]);
    Ok(())
}

/// What the UI is told about a re-tracking job.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RetrackProgress {
    href: String,
    /// The tempo being tracked against.
    bpm: f32,
    /// Absent while the job runs, then the count of beats found.
    beats: Option<usize>,
    /// Why it could not be done. A track that is not downloaded yet is the
    /// common one, and it is not a failure of the correction — the number is
    /// stored either way.
    error: Option<String>,
}

/// Which of `hrefs` hold a beat grid tracked at a tempo no longer in force.
///
/// The predicate covers every way the two can drift apart, rather than only the
/// correction that was just typed: a tempo arriving over sync, a correction made
/// before grids were re-tracked at all, and a correction *cleared* — clearing
/// puts the detected tempo back in force, which leaves the corrected grid as
/// wrong as the one it replaced.
fn stale_grids(app: &AppState, hrefs: &[String]) -> Vec<(String, f32)> {
    hrefs
        .iter()
        .filter_map(|href| {
            // Never analysed is not stale. Whenever the pass reaches it, it
            // reads the correction and tracks against that from the start.
            let analysis = app.analysis.get(href)?;
            let target = app.settings.bpm_override(href).unwrap_or(analysis.bpm);
            (!analysis.beats_are_for(target)).then(|| (href.clone(), target))
        })
        .collect()
}

/// Re-track beat grids against the tempos now in force.
///
/// Runs on one blocking thread, sequentially, for the reason `analysis::run`
/// gives: this is decode-bound, and saturating every core to fix a beat grid
/// would make the app stutter while music is playing. The corrections
/// themselves are already saved, so nothing waits on this — a failure costs the
/// grid quality and not the number.
fn retrack_grids(app_handle: &tauri::AppHandle, shared: &Shared, hrefs: Vec<String>) {
    use tauri::Emitter;

    let Ok(app) = shared.lock() else { return };
    let todo = stale_grids(&app, &hrefs);
    if todo.is_empty() {
        return;
    }
    let (cache_dir, cache_max) = (app.cache.dir().to_path_buf(), app.cache.max_bytes());
    let remote = app.settings.remote.clone();
    drop(app);

    for (href, bpm) in &todo {
        let _ = app_handle.emit(
            "bpm-retrack",
            &RetrackProgress {
                href: href.clone(),
                bpm: *bpm,
                beats: None,
                error: None,
            },
        );
    }

    let state_arc: Shared = Arc::clone(shared);
    let handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let cache = cache::Cache::new(cache_dir, cache_max);
        // One session for the batch, as the analysis pass does: one keychain
        // read and one connection rather than one of each per track.
        let fetcher = webdav::Fetcher::new(&remote).ok();

        for (href, bpm) in todo {
            let path = fetcher
                .as_ref()
                .and_then(|f| cache.store(&href, || f.fetch(&href)).ok());

            let outcome = match path {
                Some(path) => vapor_dsp::retrack_beats_file(&path, bpm).map_err(|e| e.to_string()),
                None => Err("not available locally".to_string()),
            };

            let progress = match outcome {
                Ok(beats) => {
                    let count = beats.len();
                    if let Ok(mut app) = state_arc.lock() {
                        // Re-read rather than reusing the entry from before the
                        // decode: an analysis pass may have rewritten it while
                        // this ran, and only the grid belongs to this job.
                        if let Some(entry) = app.analysis.get_mut(&href) {
                            entry.beats = beats;
                            entry.beats_bpm = bpm;
                        }
                        let _ = app.save_analysis();
                    }
                    RetrackProgress {
                        href: href.clone(),
                        bpm,
                        beats: Some(count),
                        error: None,
                    }
                }
                Err(e) => RetrackProgress {
                    href: href.clone(),
                    bpm,
                    beats: None,
                    error: Some(e),
                },
            };

            let _ = handle.emit("bpm-retrack", &progress);
        }
    });
}

// ---------------------------------------------------------------------------
// Library scan
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanReport {
    tracks: usize,
    directories: usize,
    /// Folders that could not be read and were walked past (TD-49).
    unreadable: usize,
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
    apply_remote_config(&mut app, &url, &username, &folder)?;
    Ok(app.settings.clone())
}

/// The body of [`set_remote_config`], separated from the Tauri wrapper.
///
/// A `#[tauri::command]` takes `State`, which cannot be built outside a running
/// app — so a command whose logic lives entirely in its own body is a command
/// that cannot be integration-tested. Both defects this function carries were
/// found by a person rather than by a test, for exactly that reason.
pub(crate) fn apply_remote_config(
    app: &mut AppState,
    url: &str,
    username: &str,
    folder: &str,
) -> Result<()> {
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

    // The keychain entry is keyed by username, so a rename has to move it.
    //
    // This used to *delete* the old entry, which combined with the UI's "an
    // empty password box means leave it alone" rule to destroy the credential
    // on any rename — correcting a username to the email address the server
    // actually wants would silently log you out. Best effort either way: a
    // keychain that will not give up an entry is not a reason to refuse the
    // change.
    let previous = app.settings.remote.username.clone();
    if !previous.is_empty() && previous != username.trim() {
        let _ = webdav::move_password(&previous, username.trim());
    }

    app.settings.remote.url = trimmed.to_string();
    app.settings.remote.username = username.trim().to_string();
    app.settings.remote.folder = folder.trim().to_string();
    // An empty folder means the library root, which `sanitised` spells as the
    // default rather than as "".
    app.settings = std::mem::take(&mut app.settings).sanitised();
    app.save_settings()
}

/// Whether a password is stored for `username`.
///
/// So the Settings screen can say which state it is in rather than always
/// showing "unchanged", which claims a credential exists whether or not one
/// does. Only the fact is returned; the password stays in `webdav`.
#[tauri::command]
fn has_webdav_password(username: String) -> bool {
    webdav::has_password(username.trim())
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
async fn scan_library(
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<ScanReport> {
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

    let report = {
        let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.rows = result
            .files
            .iter()
            .map(|href| build_row(href, &folder))
            .collect();

        // Saved here rather than at exit: a scan is the only thing that
        // changes the index, and writing it now means a crash mid-analysis
        // still leaves a library to come back to.
        app.save_index()?;

        ScanReport {
            tracks: app.rows.len(),
            directories: result.directories,
            unreadable: result.unreadable,
        }
    };

    // Analyse what was just found, without being asked.
    //
    // A scan produces rows that know a filename and nothing else — no tempo,
    // no key, so no Vibe DJ and no blends. That used to wait behind a button on
    // the Settings screen, which is a strange place to have to go to make the
    // library work, and an easy one never to find. Starting here is also the
    // only way the automatic pass can know there is new work.
    //
    // `pending` skips everything already done, so a rescan of a known library
    // costs nothing. The lock is released above first: `start_analysis` takes
    // it, and this mutex is not reentrant.
    let shared: Shared = Arc::clone(&state);
    start_analysis(&app_handle, &shared)?;

    Ok(report)
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
    /// Whether a pass is running right now, whoever started it.
    running: bool,
    /// The track it is on. Empty between tracks and when nothing is running.
    current: String,
}

#[tauri::command]
fn analysis_status(state: State<'_, Shared>) -> Result<AnalysisStatus> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let hrefs: Vec<String> = app.rows.iter().map(|r| r.href.clone()).collect();
    let outstanding = analysis::pending(&hrefs, &app.analysis).len();
    Ok(AnalysisStatus {
        analysed: hrefs.len().saturating_sub(outstanding),
        total: hrefs.len(),
        running: app.analysing,
        current: app.analysing_title.clone(),
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
    let shared: Shared = Arc::clone(&state);
    start_analysis(&app_handle, &shared)
}

/// Whether the track is one analysis still owes an answer for.
///
/// Used to decide if starting playback is worth restarting a pass for: if the
/// track already has its tempo and key, the running pass is doing more good
/// where it is.
fn needs_analysis(app: &AppState, href: &str) -> bool {
    app.analysis
        .get(href)
        .is_none_or(|a| a.version < analysis::ANALYSIS_VERSION)
}

/// Begin a pass, replacing whatever pass was running.
///
/// ## Why this exists rather than being the command's body
///
/// Analysis used to start only when someone pressed a button on the Settings
/// screen, which meant a freshly scanned library sat there knowing nothing but
/// filenames until they found it. Scanning and playing both start a pass now,
/// so this is called from three places and lives on its own.
///
/// ## Each pass carries its own cancel flag
///
/// The old code reset one shared flag, which made restarting unsafe: the pass
/// already running holds a clone, and resetting the flag it is watching would
/// un-cancel it, leaving two passes analysing the same library at once. A new
/// flag per pass means stopping the old one stays stopped.
///
/// Ordering comes from the play queue, so the track being listened to is
/// described first — see `analysis::pending_first`.
fn start_analysis(app_handle: &tauri::AppHandle, shared: &Shared) -> Result<()> {
    use tauri::Emitter;

    // Snapshot what needs doing and release the lock: the pass takes minutes,
    // and holding the lock would block every other command for its duration.
    let (todo, (cache_dir, cache_max), cancel, remote, generation) = {
        let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;
        let hrefs: Vec<String> = app.rows.iter().map(|r| r.href.clone()).collect();

        // What the person is listening to, then what follows it.
        let priority: Vec<String> = app
            .queue
            .tracks()
            .iter()
            .skip(app.queue.current_index().unwrap_or(0))
            .cloned()
            .collect();

        // Stop the pass that is running, and hand the new one a fresh flag.
        app.cancel.stop();
        let cancel = analysis::Cancel::new();
        app.cancel = cancel.clone();
        app.analysis_generation += 1;

        (
            analysis::pending_first(&hrefs, &app.analysis, &priority),
            (app.cache.dir().to_path_buf(), app.cache.max_bytes()),
            app.cancel.clone(),
            app.settings.remote.clone(),
            app.analysis_generation,
        )
    };

    if todo.is_empty() {
        return Ok(());
    }

    // Announced before the thread starts, so a screen opened immediately after
    // a scan already knows a pass is under way rather than waiting for the
    // first track to finish.
    if let Ok(mut app) = shared.lock() {
        app.analysing = true;
        app.analysing_title = String::new();
    }

    let state_arc: Shared = Arc::clone(shared);
    let handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let cache = cache::Cache::new(cache_dir, cache_max);

        // One session for the whole pass: one keychain read and one connection,
        // rather than one of each per track. See `webdav::Fetcher`.
        //
        // A pass that cannot read the credential has no way to fetch anything,
        // so it ends here rather than failing every track in turn and marking
        // the whole library unreadable.
        let Ok(fetcher) = webdav::Fetcher::new(&remote) else {
            return;
        };

        analysis::run(
            &todo,
            // Fetch on demand. Analysis needs local bytes; this is where they
            // come from, and a cached track costs nothing.
            //
            // Also where the screen learns what is being worked on: this runs
            // *before* the track is analysed, whereas the progress callback
            // below runs after. Naming the finished one would leave the screen
            // a track behind, and on a cold cache a track can take seconds.
            |href| {
                if let Ok(mut app) = state_arc.lock() {
                    app.analysing_title = app
                        .rows
                        .iter()
                        .find(|r| r.href == href)
                        .map(|r| r.title.clone())
                        .unwrap_or_default();
                }
                cache.store(href, || fetcher.fetch(href)).ok()
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

        // Only if this is still the current pass: a pass that was replaced
        // finishes after its replacement started, and clearing the flag here
        // unconditionally would report "not running" while one still is.
        if let Ok(mut app) = state_arc.lock() {
            if app.analysis_generation == generation {
                app.analysing = false;
                app.analysing_title = String::new();
            }
        }
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
    app.folders = FolderStore::default();
    app.groups = GroupStore::default();
    app.looked.clear();
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
            if let Ok(mut state) = shared.lock() {
                state.open_audio();
            }

            // Only worth a thread if there is a device for it to watch.
            let has_audio = shared.lock().map(|s| s.player.is_some()).unwrap_or(false);
            if has_audio {
                // Registered here, on the main thread, because that is where
                // macOS wants its remote command centre built (MIG-023).
                //
                // Only when there is a device: media keys that answer on a
                // machine with no audio output would be a set of controls for
                // something that cannot happen.
                let for_press = Arc::clone(&shared);
                let handle_for_press = app.handle().clone();
                let controls = media::Controls::attach(window_handle(app.handle()), move |press| {
                    handle_media_press(&for_press, &handle_for_press, press);
                });

                spawn_supervisor(
                    app.handle().clone(),
                    Arc::clone(&shared),
                    Arc::clone(&controls),
                );
                // Only useful alongside playback: without a device there is no
                // queue moving forward to run ahead of.
                spawn_prefetcher(Arc::clone(&shared));
            }

            // Local sync (SYNC-001, SYNC-004). Independent of audio: a device
            // with no speaker is exactly the sort of thing worth syncing to.
            //
            // Both are best-effort. A locked-down network, or a second copy of
            // the app already holding the port, costs a person discovery and
            // nothing else — the app still opens, plays and scans.
            if shared
                .lock()
                .map(|s| s.settings.sync_enabled)
                .unwrap_or(false)
            {
                let (id, name, kind, registry) = {
                    let state = shared.lock().expect("fresh state");
                    let kind = if cfg!(any(target_os = "ios", target_os = "android")) {
                        vapor_library::sync::DeviceKind::Phone
                    } else {
                        vapor_library::sync::DeviceKind::Desktop
                    };
                    // Persisted on first launch, so a device keeps its identity
                    // and its pairings survive a restart.
                    let _ = state.store.save("device_id", &state.device_id);
                    (
                        state.device_id.clone(),
                        state.device_name(),
                        kind,
                        Arc::clone(&state.peers),
                    )
                };
                let started = peers::start(
                    registry,
                    id,
                    name,
                    kind,
                    Arc::new(ServedLibrary(Arc::clone(&shared))),
                );
                if let Ok(mut state) = shared.lock() {
                    state.sync_session = started;
                }
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
            playlist_folders,
            create_folder,
            dynamic_groups,
            create_group,
            rename_group,
            delete_group,
            add_to_group,
            remove_from_group,
            reorder_groups,
            group_tracks,
            rename_folder,
            delete_folder,
            set_playlist_folder,
            track_lookup,
            look_up_track,
            looked_up_image,
            album_cover,
            find_album_art,
            clear_album_art,
            set_prefer_looked_up_art,
            artist_portrait,
            set_metadata_lookup,
            set_vibe_limit,
            set_curve,
            set_dj_mode,
            set_sync_enabled,
            startup_problems,
            identify_library,
            media_keys_available,
            sync_view,
            open_pairing,
            cancel_pairing,
            pair_with,
            forget_peer,
            sync_with,
            sync_shared_document,
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
            has_webdav_password,
            set_remote_config,
            scan_library,
            analyse_library,
            cancel_analysis,
            library_entities,
            mix_candidates,
            choose_next,
            track_cover,
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

    /// A real `AppState` on a throwaway directory.
    ///
    /// Integration rather than unit: this exercises load, mutate and save
    /// against the actual `Store`, which is where the shell's own bugs live.
    /// A counter rather than a timestamp in the name — macOS resolves the
    /// clock coarsely enough that two tests starting together collide.
    fn app() -> (AppState, std::path::PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "vapor-app-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        (AppState::load(Store::new(dir.clone())), dir)
    }

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

    // -----------------------------------------------------------------------
    // Corrected tempos and the grids they imply
    // -----------------------------------------------------------------------

    /// An analysed track at `bpm`, with a tracked grid to match.
    fn analysed_at(bpm: f32, duration: f64) -> analysis::Analysis {
        let period = 60.0 / bpm;
        let count = (duration as f32 / period) as usize;
        analysis::Analysis {
            bpm,
            beats_bpm: bpm,
            // Offset, so a synthesised grid starting at zero is distinguishable
            // from this one rather than accidentally equal to it.
            beats: (0..count).map(|i| 0.31 + i as f32 * period).collect(),
            duration,
            version: analysis::ANALYSIS_VERSION,
            ..Default::default()
        }
    }

    /// The tracked grid is what mixing wants, and it is used when it applies.
    #[test]
    fn an_uncorrected_track_mixes_on_its_tracked_grid() {
        let a = analysed_at(128.0, 300.0);
        let grid = beat_grid(&a, None);
        assert_eq!(grid.bpm, 128.0);
        assert_eq!(grid.beats, a.beats, "the tracked grid was discarded");
    }

    /// The core of the correction: a grid tracked at the rejected tempo is not
    /// used at that tempo's corrected value. Detection said 256, the person
    /// said 128, and the beats on file are still every eighth — aligning to
    /// them would beat-match to the error.
    #[test]
    fn a_correction_refuses_the_grid_tracked_at_the_old_tempo() {
        let a = analysed_at(256.0, 300.0);
        let grid = beat_grid(&a, Some(128.0));
        assert_eq!(grid.bpm, 128.0);
        assert_ne!(grid.beats, a.beats, "mixed on the grid that was rejected");
        // The interim synthetic grid, until the re-track lands.
        assert!((grid.beats[1] - grid.beats[0] - 60.0 / 128.0).abs() < 1e-4);
    }

    /// And once it has landed, the corrected tempo uses the re-tracked grid —
    /// which is the whole point, since the synthetic one has no drift and no
    /// real downbeat phase.
    #[test]
    fn a_retracked_grid_is_used_at_the_corrected_tempo() {
        let mut a = analysed_at(256.0, 300.0);
        let retracked = analysed_at(128.0, 300.0).beats;
        a.beats = retracked.clone();
        a.beats_bpm = 128.0;

        let grid = beat_grid(&a, Some(128.0));
        assert_eq!(grid.beats, retracked);

        // ...and is refused for the detected tempo, which is the same rule in
        // the other direction. Clearing a correction has to re-track too.
        let cleared = beat_grid(&a, None);
        assert_eq!(cleared.bpm, 256.0);
        assert_ne!(cleared.beats, retracked);
    }

    /// A cache entry written before `beatsBpm` existed has a zero there, and
    /// zero must mean "tracked at the detected tempo" rather than "tracked at
    /// nothing". Reading it the other way would throw away every grid in an
    /// existing library and mix the lot on synthetic ones.
    #[test]
    fn a_grid_from_before_the_field_existed_is_still_trusted() {
        let mut a = analysed_at(128.0, 300.0);
        a.beats_bpm = 0.0;

        assert_eq!(a.beats_tracked_at(), 128.0);
        assert_eq!(beat_grid(&a, None).beats, a.beats);
    }

    /// What the background job picks up, in each direction.
    #[test]
    fn stale_grids_finds_corrections_made_cleared_and_synced() {
        let (mut app, _dir) = app();

        app.analysis
            .insert("/fresh".into(), analysed_at(128.0, 300.0));
        app.analysis
            .insert("/corrected".into(), analysed_at(256.0, 300.0));
        let mut done = analysed_at(256.0, 300.0);
        done.beats_bpm = 128.0;
        app.analysis.insert("/done".into(), done);

        app.settings.set_bpm_override("/corrected", 128.0);
        app.settings.set_bpm_override("/done", 128.0);

        let all = vec![
            "/fresh".to_string(),
            "/corrected".to_string(),
            "/done".to_string(),
            "/never-analysed".to_string(),
        ];
        let stale = stale_grids(&app, &all);
        assert_eq!(
            stale,
            vec![("/corrected".to_string(), 128.0)],
            "expected only the correction that has not been re-tracked"
        );

        // Clearing the correction on the one already re-tracked makes *it*
        // stale: the detected tempo is back in force and its grid is at 128.
        app.settings.set_bpm_override("/done", 0.0);
        let stale = stale_grids(&app, &all);
        assert!(
            stale.contains(&("/done".to_string(), 256.0)),
            "clearing a correction left a grid at the corrected tempo, got {stale:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Starting up on damaged data
    // -----------------------------------------------------------------------

    /// The data-loss path, end to end.
    ///
    /// A playlists file that cannot be parsed used to load as an empty
    /// collection; the app then carried on, and the first mutation saved that
    /// empty collection over the original. This asserts the whole sequence —
    /// start, mutate, save — leaves the original bytes on disk.
    #[test]
    fn a_damaged_playlists_file_survives_a_launch_and_a_save() {
        let (app, dir) = app();
        drop(app);
        std::fs::create_dir_all(&dir).expect("dir");
        let original = br#"{"playlists": [ truncated..."#;
        std::fs::write(dir.join("playlists.json"), original).expect("write");

        let mut app = AppState::load(Store::new(dir.clone()));

        // The app started, and it knows something is wrong.
        assert_eq!(app.damaged.len(), 1, "{:?}", app.damaged);
        assert_eq!(app.damaged[0].name, "playlists");

        // Now do the thing that used to destroy it.
        app.playlists.create("p1", "A New Playlist");
        app.save_playlists().expect("save");

        let kept = app.damaged[0].kept_at.clone().expect("kept somewhere");
        assert_eq!(
            std::fs::read(&kept).expect("read"),
            original,
            "the original playlists file was lost"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Reading the app's files must not take the sound card.
    ///
    /// It used to. `AppState::load` called `Player::start()`, so every one of
    /// these tests opened a real audio device — and on a Windows runner with no
    /// audio endpoint the first one to try killed the test binary with an
    /// access violation, before any test reported a result (AND-2). The crash
    /// is inside `cpal`'s WASAPI backend and cannot be caught from here; not
    /// asking is the fix.
    ///
    /// Pinned rather than left to the comment, because "does this function
    /// touch hardware" is invisible at every call site.
    #[test]
    fn loading_does_not_open_an_audio_device() {
        let (app, dir) = app();
        assert!(
            app.player.is_none(),
            "load acquired an audio device; that belongs in open_audio"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A normal launch reports nothing. If this ever fails, the banner is
    /// about to be shown to everyone for no reason.
    #[test]
    fn a_clean_launch_reports_no_damage() {
        let (app, dir) = app();
        assert!(app.damaged.is_empty(), "{:?}", app.damaged);
        drop(app);

        // And a launch over real, valid data is also clean.
        let mut app = AppState::load(Store::new(dir.clone()));
        app.playlists.create("p1", "Real");
        app.save_playlists().expect("save");
        app.save_settings().expect("save");
        drop(app);

        let reopened = AppState::load(Store::new(dir.clone()));
        assert!(reopened.damaged.is_empty(), "{:?}", reopened.damaged);
        assert_eq!(reopened.playlists.len(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Several damaged files are all reported, not just the first — a person
    /// told about one and then surprised by another has been told nothing.
    #[test]
    fn every_damaged_file_is_reported() {
        let (app, dir) = app();
        drop(app);
        std::fs::create_dir_all(&dir).expect("dir");
        for name in ["playlists", "folders", "tags"] {
            std::fs::write(dir.join(format!("{name}.json")), b"not json").expect("write");
        }

        let app = AppState::load(Store::new(dir.clone()));
        let mut names: Vec<&str> = app.damaged.iter().map(|d| d.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["folders", "playlists", "tags"]);

        // And each message is something a person can act on: it names the file
        // and where the bytes went.
        for d in &app.damaged {
            let m = d.message();
            assert!(m.contains(&d.name), "{m}");
            assert!(m.contains("kept at"), "{m}");
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    // -----------------------------------------------------------------------
    // The DJ conducting a set
    // -----------------------------------------------------------------------

    /// An analysed track, so the cost model can place it.
    fn analysed_track(bpm: f32, key: &str, energy: f32) -> analysis::Analysis {
        analysis::Analysis {
            bpm,
            beats_bpm: bpm,
            key: key.to_string(),
            energy,
            duration: 240.0,
            lufs: -9.0,
            version: analysis::ANALYSIS_VERSION,
            ..Default::default()
        }
    }

    /// A library of analysed tracks with one of them playing and nothing queued
    /// after it — exactly the state a person is in after pressing play on a
    /// single track.
    fn conducting() -> (AppState, std::path::PathBuf) {
        let (mut app, dir) = app();
        let tracks = [
            ("/a.mp3", 174.0, "4A", 0.8),
            ("/b.mp3", 172.0, "4A", 0.8),
            ("/c.mp3", 128.0, "8B", 0.5),
            ("/d.mp3", 90.0, "11B", 0.2),
        ];
        for (href, bpm, key, energy) in tracks {
            app.rows.push(row(href, href));
            app.analysis
                .insert(href.to_string(), analysed_track(bpm, key, energy));
        }
        app.queue.set_tracks(vec!["/a.mp3".to_string()], None);
        app.playing = Some("/a.mp3".to_string());
        (app, dir)
    }

    /// The bug, stated plainly: a set of one track never grew, so the track
    /// repeated forever and the screen said "0 to come".
    #[test]
    fn the_dj_extends_a_set_that_has_nothing_queued_after_it() {
        let (mut app, dir) = conducting();
        assert_eq!(app.queue.tracks().len(), 1);

        assert!(extend_set(&mut app), "the DJ added nothing");

        // The planner fills the set rather than adding one track at a time —
        // it used to append exactly one, which is why the queue read "0 to
        // come" the moment that track started.
        assert!(
            app.queue.tracks().len() > 2,
            "only {} queued; the planner did not run",
            app.queue.tracks().len()
        );
        let next = app.queue.peek_next(None).expect("something to come");
        assert_ne!(next, "/a.mp3", "the DJ queued the track already playing");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// And it keeps going, without ever repeating itself — a DJ that loops two
    /// tracks is not conducting a set.
    #[test]
    fn the_dj_keeps_the_set_going_without_repeating() {
        let (mut app, dir) = conducting();

        for _ in 0..3 {
            // Advance as the supervisor does when a track ends.
            if let Some(next) = app.queue.next(None).map(str::to_string) {
                app.playing = Some(next);
            }
            extend_set(&mut app);
        }

        let queued = app.queue.tracks();
        let unique: std::collections::HashSet<&String> = queued.iter().collect();
        assert_eq!(
            unique.len(),
            queued.len(),
            "the DJ repeated a track: {queued:?}"
        );
        assert!(queued.len() >= 3, "the set stopped growing: {queued:?}");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// With the DJ off it is a plain queue and must stay one. The switch was
    /// frontend state, so the backend used to ignore it entirely.
    #[test]
    fn the_dj_adds_nothing_when_it_is_switched_off() {
        let (mut app, dir) = conducting();
        app.settings.dj_mode = false;

        assert!(!extend_set(&mut app));
        assert_eq!(app.queue.tracks().len(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A queue someone built themselves is not interfered with while it still
    /// has somewhere to go.
    #[test]
    fn the_dj_leaves_a_queue_that_already_has_a_next_track_alone() {
        let (mut app, dir) = conducting();
        app.queue
            .set_tracks(vec!["/a.mp3".to_string(), "/d.mp3".to_string()], None);
        app.playing = Some("/a.mp3".to_string());

        assert!(!extend_set(&mut app));
        assert_eq!(app.queue.tracks().len(), 2);
        assert_eq!(app.queue.peek_next(None), Some("/d.mp3"));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Nothing analysed means nothing to choose from, and the DJ must say so by
    /// doing nothing rather than by panicking.
    #[test]
    fn the_dj_does_nothing_with_an_unanalysed_library() {
        let (mut app, dir) = app();
        app.rows.push(row("/x.mp3", "x"));
        app.queue.set_tracks(vec!["/x.mp3".to_string()], None);
        app.playing = Some("/x.mp3".to_string());

        assert!(!extend_set(&mut app));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Choosing a blend from the screen has to work, and the candidates come
    /// from the whole library rather than from the queue — so every one of them
    /// used to be refused with "that track is no longer in the queue".
    #[test]
    fn choosing_a_candidate_queues_it_even_though_it_was_not_in_the_queue() {
        let (mut app, dir) = conducting();
        assert!(!app.queue.tracks().iter().any(|t| t == "/c.mp3"));

        assert!(app.queue.set_next("/c.mp3"), "the choice was refused");
        assert_eq!(app.queue.peek_next(None), Some("/c.mp3"));

        let _ = std::fs::remove_dir_all(dir);
    }

    // -----------------------------------------------------------------------
    // Genre reaching the DJ at all
    // -----------------------------------------------------------------------

    /// The scan leaves `row.genre` empty in a folder-organised library, so the
    /// genre lives in the file's tags — and the DJ read the row.
    #[test]
    fn the_dj_sees_a_genre_that_only_the_file_tags_carry() {
        let (mut app, dir) = app();
        app.rows.push(row("/a.mp3", "A"));
        assert_eq!(genre_of(&app, "/a.mp3"), "", "nothing to find yet");

        app.tags.insert(
            "/a.mp3".to_string(),
            tags::Tags {
                genre: Some("Drum & Bass".to_string()),
                ..Default::default()
            }
            .into(),
        );
        assert_eq!(genre_of(&app, "/a.mp3"), "Drum & Bass");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// And the looked-up genre, which for this library is the only source for
    /// the 488 tracks whose files carry no tag at all.
    #[test]
    fn a_looked_up_genre_counts_when_the_file_has_none() {
        let (mut app, dir) = app();
        app.rows.push(row("/a.mp3", "A"));
        app.looked.insert(
            "/a.mp3".to_string(),
            metadata::Looked {
                genre: "House".to_string(),
                attempted: true,
                ..Default::default()
            },
        );

        assert_eq!(genre_of(&app, "/a.mp3"), "House");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The scan's own genre wins when it has one — it came from the library
    /// index rather than from a stranger.
    #[test]
    fn the_scanned_genre_outranks_a_looked_up_one() {
        let (mut app, dir) = app();
        let mut r = row("/a.mp3", "A");
        r.genre = "Techno".to_string();
        app.rows.push(r);
        app.looked.insert(
            "/a.mp3".to_string(),
            metadata::Looked {
                genre: "Pop".to_string(),
                attempted: true,
                ..Default::default()
            },
        );

        assert_eq!(genre_of(&app, "/a.mp3"), "Techno");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The behaviour you would expect and did not get: of two tracks that are
    /// otherwise equally good, the DJ takes the one in the same genre.
    ///
    /// It could not, because the scoring never mentioned genre — so with key
    /// and tempo identical, the winner was whichever the map happened to yield
    /// first. That is what made the suggestions feel arbitrary.
    #[test]
    fn genre_decides_between_two_otherwise_identical_candidates() {
        let (mut app, dir) = app();

        let tracks = [
            ("/playing.mp3", "Drum & Bass"),
            ("/same-genre.mp3", "Drum & Bass"),
            ("/other-genre.mp3", "Classical"),
        ];
        for (href, genre) in tracks {
            let mut r = row(href, href);
            r.genre = genre.to_string();
            app.rows.push(r);
            // Identical in every way the cost model can see except genre.
            app.analysis
                .insert(href.to_string(), analysed_track(174.0, "4A", 0.8));
        }
        app.queue.set_tracks(vec!["/playing.mp3".to_string()], None);
        app.playing = Some("/playing.mp3".to_string());

        let pick = dj_pick(&app).expect("a pick");
        assert_eq!(
            pick, "/same-genre.mp3",
            "the DJ ignored genre and took {pick}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// And the suggestions on screen agree with it, because both go through
    /// one cost function.
    #[test]
    fn the_screen_and_the_set_score_candidates_the_same_way() {
        let (mut app, dir) = app();
        for (href, genre, bpm) in [
            ("/playing.mp3", "House", 128.0),
            ("/near.mp3", "House", 129.0),
            ("/far.mp3", "Folk", 129.0),
        ] {
            let mut r = row(href, href);
            r.genre = genre.to_string();
            app.rows.push(r);
            app.analysis
                .insert(href.to_string(), analysed_track(bpm, "8A", 0.6));
        }
        app.playing = Some("/playing.mp3".to_string());
        let pool = track_meta_pool(&app);
        let from = pool.get("/playing.mp3").expect("playing");

        let near = candidate_cost(&app, from, pool.get("/near.mp3").unwrap(), Exit::Stay);
        let far = candidate_cost(&app, from, pool.get("/far.mp3").unwrap(), Exit::Stay);
        assert!(
            near < far,
            "a same-genre candidate did not score better: {near} vs {far}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    // -----------------------------------------------------------------------
    // The three exits
    // -----------------------------------------------------------------------

    /// Drum & bass into Sade, at the numbers those two actually measure. It was
    /// a Match, because Switch was reachable only through a genre difference
    /// and this library has 46 genre tags across 534 tracks.
    #[test]
    fn a_distant_pair_is_a_switch_without_needing_a_genre() {
        let (mut app, dir) = app();
        for (href, bpm, lufs) in [("/dnb.mp3", 87.8f32, -10.2f32), ("/sade.mp3", 113.1, -19.0)] {
            app.rows.push(row(href, href));
            let mut a = analysed_track(bpm, "4A", 0.7);
            a.lufs = lufs;
            app.analysis.insert(href.to_string(), a);
        }
        let pool = track_meta_pool(&app);
        let (dnb, sade) = (
            pool.get("/dnb.mp3").unwrap(),
            pool.get("/sade.mp3").unwrap(),
        );

        // Neither has a genre, so the old rule called this similar.
        assert_eq!(exit_between(dnb, sade, true), Exit::Switch);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Stay is for holding the level, so near-identical intensity and tempo.
    #[test]
    fn a_close_pair_is_a_stay() {
        let (mut app, dir) = app();
        for href in ["/a.mp3", "/b.mp3"] {
            app.rows.push(row(href, href));
            let mut a = analysed_track(128.0, "8A", 0.6);
            a.lufs = -12.0;
            app.analysis.insert(href.to_string(), a);
        }
        let pool = track_meta_pool(&app);
        assert_eq!(
            exit_between(
                pool.get("/a.mp3").unwrap(),
                pool.get("/b.mp3").unwrap(),
                true
            ),
            Exit::Stay
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The consolidation: Follow is *the plan's* next track, not a similarity
    /// class, so the card and the set can no longer disagree about what is
    /// coming.
    #[test]
    fn follow_is_whatever_the_set_has_queued_next() {
        let (mut app, dir) = conducting();
        assert!(extend_set(&mut app), "the set did not plan");

        let queued = app
            .queue
            .peek_next(None)
            .expect("a planned next")
            .to_string();
        let follow = mix_candidates_for(&app)
            .into_iter()
            .find(|c| c.exit == Exit::Follow)
            .expect("a Follow card");

        assert_eq!(follow.href, queued);
        assert!(follow.selected, "Follow is what happens if nobody acts");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A group is a set of entities, and its tracks are worked out on read.
    ///
    /// That is the whole difference from a playlist: nothing is stored per
    /// track, so a record added to the library afterwards belongs to the group
    /// without anyone touching it.
    #[test]
    fn a_group_takes_in_tracks_added_after_it_was_made() {
        use vapor_library::EntityType;
        let (mut app, dir) = app();
        app.rows.push(row("/one.mp3", "One"));
        app.rows[0].artist = "Aphex Twin".to_string();

        let id = app.groups.create("g1", "Braindance").id.clone();
        assert!(app.groups.add_entity(&id, EntityType::Artist, "Aphex Twin"));

        let group = app.groups.get(&id).expect("the group").clone();
        assert_eq!(tracks_in_group(&app, &group).len(), 1);

        // A record that did not exist when the group was made.
        app.rows.push(row("/two.mp3", "Two"));
        app.rows[1].artist = "Aphex Twin".to_string();
        assert_eq!(
            tracks_in_group(&app, &group).len(),
            2,
            "the group did not pick up a track added after it",
        );

        // And nothing by anyone else wanders in.
        app.rows.push(row("/three.mp3", "Three"));
        app.rows[2].artist = "Boards of Canada".to_string();
        assert_eq!(tracks_in_group(&app, &group).len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Adding the same entity twice is a no-op, not a second row — dragging an
    /// artist onto a group you already dropped them on should do nothing.
    #[test]
    fn adding_an_entity_twice_does_not_duplicate_it() {
        use vapor_library::EntityType;
        let (mut app, dir) = app();
        let id = app.groups.create("g1", "Braindance").id.clone();

        assert!(app.groups.add_entity(&id, EntityType::Artist, "Aphex Twin"));
        assert!(!app.groups.add_entity(&id, EntityType::Artist, "Aphex Twin"));
        assert_eq!(app.groups.get(&id).expect("the group").entities.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A mix needs a tempo on both sides, not merely a row in the cache.
    ///
    /// A track too short or too quiet for the beat tracker comes back analysed
    /// with a tempo of nothing. `track_meta_pool` has always refused those, so
    /// they never reached the Vibe screen — but the queue is a second way in,
    /// and the planner took whatever the queue handed it. Beat-matching against
    /// 0 BPM is not a mix.
    #[test]
    fn a_mix_is_not_planned_into_a_track_with_no_tempo() {
        let (mut app, dir) = conducting();

        // The planner is willing while both ends are described.
        app.queue.set_tracks(
            vec!["/a.mp3".to_string(), "/b.mp3".to_string()],
            Some("/a.mp3"),
        );
        // Position 0: the fixture's `cue_out` is 0, so that is where this
        // pair's arming window sits.
        assert!(
            plan_mix(&app, 0.0).is_some(),
            "the fixture cannot plan a mix at all, so this proves nothing",
        );

        // Same track, analysed, no tempo found.
        app.analysis
            .insert("/b.mp3".to_string(), analysed_track(0.0, "4A", 0.8));
        assert!(
            plan_mix(&app, 0.0).is_none(),
            "planned a beat-match against a tempo of zero",
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The DJ has to know the library before it arrives at it.
    ///
    /// The pass is ordered from the queue, so restarting it is how upcoming
    /// tracks reach the front. That restart used to be asked for only when the
    /// track *being started* was undescribed — so a queue whose next records
    /// were unknown never re-ordered, and the DJ reached a track it could not
    /// mix into.
    #[test]
    fn the_next_few_tracks_are_what_decides_to_reorder_the_pass() {
        let (mut app, dir) = conducting();
        app.queue.set_tracks(
            vec![
                "/a.mp3".to_string(),
                "/b.mp3".to_string(),
                "/c.mp3".to_string(),
            ],
            Some("/a.mp3"),
        );

        // Everything in the window is described: nothing to re-order for.
        assert!(!needs_analysis_soon(&app, MIX_LOOKAHEAD));

        // A track two ahead is not, and that is the case the old condition
        // could not see, because the track playing was fine.
        app.analysis.remove("/c.mp3");
        assert!(
            needs_analysis_soon(&app, MIX_LOOKAHEAD),
            "an undescribed track in the window did not ask for the pass",
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Three exits before anything has been planned.
    ///
    /// The screen opened on two cards — Stay and Switch — and grew a third the
    /// moment something was queued, which reads as the DJ malfunctioning rather
    /// than as an absent plan. Follow was the plan's next track and nothing
    /// else, so with no plan there was no card; Stay and Switch had both been
    /// given fallbacks already and Follow had not.
    ///
    /// Deliberately without `extend_set`: the existing three-exit test plans
    /// first, which is exactly why this never showed up.
    #[test]
    fn three_exits_are_offered_before_the_set_has_been_planned() {
        let (app, dir) = conducting();
        // Not `peek_next().is_none()`: a one-track queue wraps under repeat-all
        // and answers with the track playing, which is precisely the state that
        // used to produce a Follow card offering the current record back.
        assert_eq!(
            app.queue.peek_next(None),
            Some("/a.mp3"),
            "the fixture is meant to have no real next track",
        );

        let cards = mix_candidates_for(&app);
        let exits: Vec<Exit> = cards.iter().map(|c| c.exit).collect();
        assert_eq!(
            exits,
            vec![Exit::Stay, Exit::Follow, Exit::Switch],
            "got {} card(s) with no plan yet",
            cards.len(),
        );

        // Nothing is queued, so nothing is what happens if nobody acts. The
        // card is an answer, not a claim about the set.
        assert!(
            cards.iter().all(|c| !c.selected),
            "a card claimed to be queued when the set had not been planned",
        );

        let mut hrefs: Vec<&str> = cards.iter().map(|c| c.href.as_str()).collect();
        hrefs.sort_unstable();
        hrefs.dedup();
        assert_eq!(hrefs.len(), 3, "the same track appeared under two exits");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Three exits, every time there are three tracks to fill them with.
    ///
    /// The screen showed two cards, repeatedly, and the reason was that Stay
    /// and Switch were each drawn only from the band `exit_between` put a
    /// candidate in — so a library with nothing inside 8 BPM of the track
    /// playing simply had no Stay card and said nothing about why. The four
    /// fixture tracks here span 174 to 90 BPM: none of them is a Stay by the
    /// thresholds, and all three cards must still appear.
    #[test]
    fn all_three_exits_are_offered_even_when_nothing_sits_in_the_band() {
        let (mut app, dir) = conducting();
        extend_set(&mut app);

        let cards = mix_candidates_for(&app);
        let exits: Vec<Exit> = cards.iter().map(|c| c.exit).collect();
        assert_eq!(
            exits,
            vec![Exit::Stay, Exit::Follow, Exit::Switch],
            "got {} card(s): {:?}",
            cards.len(),
            cards.iter().map(|c| c.href.as_str()).collect::<Vec<_>>()
        );

        // And they are three different records. Two cards pointing at one track
        // is the same failure wearing a third label.
        let mut hrefs: Vec<&str> = cards.iter().map(|c| c.href.as_str()).collect();
        hrefs.sort_unstable();
        hrefs.dedup();
        assert_eq!(hrefs.len(), 3, "the same track appeared under two exits");

        // Stay holds the level and Switch leaves it, whichever band they came
        // from — that ordering is the only thing the labels promise.
        // Read from the same pool the cards were chosen from, rather than from
        // `Analysis` — intensity is derived from LUFS on the way into
        // `TrackMeta`, so the raw analysis does not carry it.
        let pool = track_meta_pool(&app);
        let level = |href: &str| pool.get(href).map_or(0.0_f32, |t| t.energy_level);
        let here = level("/a.mp3");
        let stay_gap = (here - level(&cards[0].href)).abs();
        let switch_gap = (here - level(&cards[2].href)).abs();
        assert!(
            stay_gap <= switch_gap,
            "Stay ({stay_gap}) moved the level further than Switch ({switch_gap})"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The planner fills the set, not one track at a time.
    #[test]
    fn the_set_is_planned_several_tracks_ahead() {
        let (mut app, dir) = conducting();
        extend_set(&mut app);
        assert!(
            app.queue.tracks().len() > 2,
            "only {} queued — the planner did not run",
            app.queue.tracks().len()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The symptom, end to end: a drum & bass track must not be called a
    /// match for chill hip hop.
    ///
    /// Both read at 87 BPM in the same key with near-identical energy, and both
    /// readings are correct — Essentia agrees on 87 for the drum & bass. Nothing
    /// the app measures separates them, so the DJ called it a MATCH and it was
    /// right to, on the numbers it had. The genre is what says one of those 87s
    /// is really 174.
    #[test]
    fn drum_and_bass_is_not_a_match_for_hip_hop_at_the_same_reading() {
        let (mut app, dir) = app();

        for (href, genre) in [("/dnb.mp3", "Drum & Bass"), ("/hiphop.mp3", "Hip Hop")] {
            let mut r = row(href, href);
            r.genre = genre.to_string();
            app.rows.push(r);
            // What analysis actually produced for both.
            app.analysis
                .insert(href.to_string(), analysed_track(87.0, "4A", 0.7));
        }
        app.playing = Some("/dnb.mp3".to_string());

        let pool = track_meta_pool(&app);
        let dnb = pool.get("/dnb.mp3").expect("dnb");
        let hip = pool.get("/hiphop.mp3").expect("hip hop");

        // The genre resolved the octave the beat tracker could not.
        assert_eq!(dnb.bpm, 174.0, "the drum & bass tempo was not corrected");
        assert_eq!(hip.bpm, 87.0, "the hip hop tempo was wrongly moved");

        let kind = exit_between(dnb, hip, same_genre(&app, "/dnb.mp3", "/hiphop.mp3"));
        assert_ne!(
            kind,
            Exit::Stay,
            "a drum & bass track is still being offered as a match for hip hop"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The other half of the DnB-versus-chill complaint, without genre.
    ///
    /// Two tracks at the same tempo and key, one loud and one quiet, must not
    /// be offered as a match. They were, because `energy` measured consistency
    /// rather than intensity and put them 0.03 apart — under every threshold.
    /// Loudness puts them 0.26 apart.
    #[test]
    fn a_loud_track_is_not_a_match_for_a_quiet_one() {
        let (mut app, dir) = app();

        for (href, lufs) in [("/loud.mp3", -9.0f32), ("/quiet.mp3", -19.0)] {
            app.rows.push(row(href, href));
            let mut a = analysed_track(87.0, "4A", 0.7);
            a.lufs = lufs;
            app.analysis.insert(href.to_string(), a);
        }
        app.playing = Some("/loud.mp3".to_string());

        let pool = track_meta_pool(&app);
        let loud = pool.get("/loud.mp3").expect("loud");
        let quiet = pool.get("/quiet.mp3").expect("quiet");

        assert!(
            loud.energy_level - quiet.energy_level > 0.2,
            "intensity did not separate them: {} vs {}",
            loud.energy_level,
            quiet.energy_level
        );
        assert_ne!(
            exit_between(loud, quiet, true),
            Exit::Stay,
            "a loud track is still a match for a quiet one at the same tempo"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A hand correction still outranks the genre. The person looking at the
    /// track is the only one who actually knows.
    #[test]
    fn a_hand_corrected_tempo_beats_the_genre_guess() {
        let (mut app, dir) = app();
        let mut r = row("/dnb.mp3", "d");
        r.genre = "Drum & Bass".to_string();
        app.rows.push(r);
        app.analysis
            .insert("/dnb.mp3".to_string(), analysed_track(87.0, "4A", 0.7));
        app.settings.set_bpm_override("/dnb.mp3", 88.0);

        let pool = track_meta_pool(&app);
        assert_eq!(pool.get("/dnb.mp3").expect("track").bpm, 88.0);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A track with no genre is left exactly as measured — which is most of
    /// this library, and the reason this is an improvement rather than a fix.
    #[test]
    fn a_track_without_a_genre_keeps_its_measured_tempo() {
        let (mut app, dir) = app();
        app.rows.push(row("/x.mp3", "x"));
        app.analysis
            .insert("/x.mp3".to_string(), analysed_track(87.0, "4A", 0.7));

        let pool = track_meta_pool(&app);
        assert_eq!(pool.get("/x.mp3").expect("track").bpm, 87.0);

        let _ = std::fs::remove_dir_all(dir);
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

    // -----------------------------------------------------------------------
    // The remote configuration — where both reported credential defects lived
    // -----------------------------------------------------------------------

    #[test]
    fn a_server_address_must_be_one() {
        let (mut app, dir) = app();

        // A Koofr app password pasted into the address field: the way it
        // actually happens, and previously stored without complaint.
        let refused = apply_remote_config(&mut app, "4wg9ie7xi8v7nbi6", "someone", "Music");
        assert!(refused.is_err(), "a non-address was accepted");
        assert!(
            app.settings.remote.url.is_empty(),
            "the refused value was stored anyway"
        );

        assert!(apply_remote_config(
            &mut app,
            "https://app.koofr.net",
            "someone",
            "/dav/Koofr/Music"
        )
        .is_ok());
        assert_eq!(app.settings.remote.url, "https://app.koofr.net");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_address_is_allowed_because_it_means_not_configured_yet() {
        let (mut app, dir) = app();
        assert!(apply_remote_config(&mut app, "", "", "").is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_configuration_survives_a_restart() {
        let (mut app, dir) = app();
        apply_remote_config(&mut app, "https://example.com", "someone", "/dav/Music")
            .expect("save");
        drop(app);

        // A second load of the same directory is what a relaunch is.
        let reloaded = AppState::load(Store::new(dir.clone()));
        assert_eq!(reloaded.settings.remote.url, "https://example.com");
        assert_eq!(reloaded.settings.remote.username, "someone");
        assert_eq!(reloaded.settings.remote.folder, "/dav/Music");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The password must never reach the settings file. A test asserts it
    /// cannot be serialised; this asserts it is not there in practice either.
    #[test]
    fn no_password_is_ever_written_to_disk() {
        let (mut app, dir) = app();
        apply_remote_config(&mut app, "https://example.com", "someone", "/dav/Music")
            .expect("save");

        let written = std::fs::read_to_string(dir.join("settings.json")).expect("settings file");
        assert!(
            !written.to_lowercase().contains("password"),
            "the settings file mentions a password: {written}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn whitespace_around_a_field_is_not_stored() {
        let (mut app, dir) = app();
        apply_remote_config(
            &mut app,
            "  https://example.com  ",
            "  someone  ",
            "  /dav/Music  ",
        )
        .expect("save");

        assert_eq!(app.settings.remote.url, "https://example.com");
        assert_eq!(app.settings.remote.username, "someone");
        assert_eq!(app.settings.remote.folder, "/dav/Music");

        let _ = std::fs::remove_dir_all(dir);
    }

    // -----------------------------------------------------------------------
    // Playlists, through the state rather than through the commands
    // -----------------------------------------------------------------------

    #[test]
    fn playlists_survive_a_restart() {
        let (mut app, dir) = app();
        app.playlists.create("p1", "Late Night");
        app.playlists.add_track("p1", "/a.m4a");
        app.save_playlists().expect("save");
        drop(app);

        let reloaded = AppState::load(Store::new(dir.clone()));
        let playlist = reloaded.playlists.get("p1").expect("playlist survived");
        assert_eq!(playlist.name, "Late Night");
        assert_eq!(playlist.tracks, vec!["/a.m4a".to_string()]);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Deleting a container must not delete what it contains. A folder is an
    /// organisational convenience; if filing a playlist into one risked losing
    /// it, nobody would file anything.
    #[test]
    fn deleting_a_folder_keeps_the_playlists_that_were_in_it() {
        let (mut app, dir) = app();
        app.folders.create("f1", "Sets", "");
        app.playlists.create_in_folder("p1", "Late Night", "f1");
        app.playlists.add_track("p1", "/a.m4a");

        assert!(remove_folder(&mut app, "f1"));

        let playlist = app.playlists.get("p1").expect("playlist survived");
        assert_eq!(playlist.tracks, vec!["/a.m4a".to_string()]);
        // Back at the top level, where the rail can still draw it. A playlist
        // pointing at a folder that no longer exists would be invisible.
        assert_eq!(playlist.folder_id, "");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A nested folder's playlists are the case that needs the reassignment to
    /// follow what `FolderStore::delete` reports rather than only the id asked
    /// for — those playlists point at the *child*, which was never deleted.
    #[test]
    fn a_nested_folders_playlists_come_home_too() {
        let (mut app, dir) = app();
        app.folders.create("parent", "Sets", "");
        app.folders.create("child", "Warmups", "parent");
        app.playlists.create_in_folder("p1", "Openers", "child");

        assert!(remove_folder(&mut app, "parent"));

        assert_eq!(app.playlists.get("p1").expect("survived").folder_id, "");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// TD-57: a deletion is only carried if it was written down, and only
    /// travels if it survives a restart. Both halves in one test, because a
    /// tombstone that is recorded and then lost on quit is the same bug.
    #[test]
    fn a_deleted_folder_leaves_a_record_that_survives_a_restart() {
        let (mut app, dir) = app();
        app.folders.create("f1", "Sets", "");
        app.folders.create("child", "Warmups", "f1");

        assert!(remove_folder(&mut app, "f1"));
        app.save_folders().expect("save folders");
        app.save_tombstones().expect("save tombstones");

        assert!(app.tombstones.folder_deleted("f1"));
        // The nested folder was orphaned, not deleted — tombstoning it would
        // delete a folder that still exists on every other device.
        assert!(!app.tombstones.folder_deleted("child"));
        drop(app);

        let reloaded = AppState::load(Store::new(dir.clone()));
        assert!(
            reloaded.tombstones.folder_deleted("f1"),
            "the record of the deletion did not survive a restart"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// And the record has to reach the document, or no other device ever hears.
    #[test]
    fn the_shared_document_carries_what_was_deleted() {
        let (mut app, dir) = app();
        app.tombstones.record_playlist("p1", 100);
        app.tombstones.record_folder("f1", 100);

        let document = shared_document(&app);
        assert!(document.deleted.playlist_deleted("p1"));
        assert!(document.deleted.folder_deleted("f1"));

        // And through JSON, since that is how it actually travels.
        let text = serde_json::to_string(&document).expect("write");
        let back: vapor_library::sync::Shared = serde_json::from_str(&text).expect("read");
        assert_eq!(back.deleted, document.deleted);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn folders_survive_a_restart() {
        let (mut app, dir) = app();
        app.folders.create("f1", "Sets", "");
        app.playlists.create_in_folder("p1", "Late Night", "f1");
        app.save_folders().expect("save folders");
        app.save_playlists().expect("save playlists");
        drop(app);

        let reloaded = AppState::load(Store::new(dir.clone()));
        assert_eq!(reloaded.folders.get("f1").expect("folder").name, "Sets");
        // The two files are written separately, so a playlist keeping its
        // `folder_id` across a restart is a claim worth making explicitly.
        assert_eq!(
            reloaded.playlists.get("p1").expect("playlist").folder_id,
            "f1"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_playlist_holding_a_missing_track_still_lists_the_rest() {
        let library = vec![row("/a", "Anna"), row("/c", "Cleo")];
        let wanted = vec!["/a".to_string(), "/gone".to_string(), "/c".to_string()];
        assert_eq!(rows_in_order(&library, &wanted).len(), 2);
    }

    // -----------------------------------------------------------------------
    // Analysis records
    // -----------------------------------------------------------------------

    /// A record written at an older version must be re-analysed rather than
    /// trusted, or a library ends up with some tracks measured one way and
    /// some another with no way to tell which.
    #[test]
    fn stale_analysis_records_are_not_trusted() {
        use crate::analysis::{Analysis, ANALYSIS_VERSION};

        let (mut app, dir) = app();
        app.analysis.insert(
            "/a.m4a".to_string(),
            Analysis {
                bpm: 128.0,
                key: "8A".to_string(),
                version: ANALYSIS_VERSION - 1,
                ..Default::default()
            },
        );

        let stale = app
            .analysis
            .get("/a.m4a")
            .map(|a| a.version < ANALYSIS_VERSION)
            .unwrap_or(false);
        assert!(stale, "an older record was treated as current");

        let _ = std::fs::remove_dir_all(dir);
    }

    // -----------------------------------------------------------------------
    // Settings that a hand-edited file could make nonsense of
    // -----------------------------------------------------------------------

    #[test]
    fn an_absurd_cache_bound_is_corrected_on_load() {
        let (app, dir) = app();
        // Whatever the default is, it has to be big enough to hold a track —
        // a bound below one track evicts everything the moment it arrives.
        assert!(
            app.settings.cache_max_bytes > 50_000_000,
            "the default cache bound is {} bytes",
            app.settings.cache_max_bytes
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_corrupt_settings_file_does_not_silently_start_empty() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "vapor-corrupt-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("settings.json"), "{ not json at all").expect("write");

        // Loading must not panic. Starting empty on a read failure would show
        // someone an empty library while their data sits unreadable on disk —
        // the behaviour `Store::load` exists to avoid.
        let app = AppState::load(Store::new(dir.clone()));
        assert!(app.settings.remote.url.is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The library survives a restart.
    ///
    /// The scanned index was the one thing built at scan and never written, so
    /// every launch opened on "0 tracks" and stayed there until someone found
    /// Settings and pressed Scan — re-walking every directory on the server to
    /// rediscover a list that had not changed.
    ///
    /// Loaded through a *second* `AppState` over the same directory, because
    /// that is the boundary the app crosses: one process saves, the next one
    /// reads. Asserting on the first one's own field would prove nothing.
    #[test]
    fn a_scanned_library_is_still_there_next_launch() {
        let (mut first, dir) = app();
        first.rows = vec![
            row("/dav/Koofr/Music/a.mp3", "A"),
            row("/dav/Koofr/Music/b.mp3", "B"),
        ];
        first.save_index().expect("the index should save");

        let second = AppState::load(Store::new(dir.clone()));

        assert_eq!(
            second.rows.len(),
            2,
            "the library was empty on the next launch"
        );
        assert_eq!(second.rows[0].title, "A");
        assert_eq!(second.rows[1].href, "/dav/Koofr/Music/b.mp3");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// "Delete everything" includes the index, or the next launch restores a
    /// library the person asked to be rid of.
    #[test]
    fn deleting_everything_does_not_leave_the_index_behind() {
        let (mut app_state, dir) = app();
        app_state.rows = vec![row("/dav/Koofr/Music/a.mp3", "A")];
        app_state.save_index().expect("the index should save");

        app_state.store.clear().expect("clearing should succeed");

        let next = AppState::load(Store::new(dir.clone()));
        assert!(
            next.rows.is_empty(),
            "the library came back after being deleted"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    fn meta(bpm: f32, key: &str, energy: f32) -> TrackMeta {
        TrackMeta {
            href: format!("/{bpm}-{key}"),
            bpm,
            musical_key: key.to_string(),
            intro_key: key.to_string(),
            outro_key: key.to_string(),
            energy_level: energy,
            genre: String::new(),
        }
    }

    /// The thresholds are the original's, from `_get_match_type_between`.
    #[test]
    fn a_close_neighbour_in_the_same_genre_is_a_match() {
        let from = meta(128.0, "8A", 0.5);
        let to = meta(130.0, "9A", 0.55);

        assert_eq!(exit_between(&from, &to, true), Exit::Stay);
    }

    #[test]
    fn eight_bpm_apart_is_fresh_rather_than_a_match() {
        let from = meta(128.0, "8A", 0.5);
        // Exactly the boundary: the original uses >=, so this is Fresh.
        let to = meta(136.0, "8A", 0.5);

        assert_eq!(exit_between(&from, &to, true), Exit::Follow);
    }

    #[test]
    fn a_fifth_of_energy_apart_is_fresh_even_at_the_same_tempo() {
        let from = meta(128.0, "8A", 0.4);
        let to = meta(128.0, "8A", 0.6);

        assert_eq!(exit_between(&from, &to, true), Exit::Follow);
    }

    /// A genre jump outranks everything: it is a Switch however close the
    /// tempo and energy are.
    #[test]
    fn a_different_genre_is_a_switch_however_close_the_rest_is() {
        let from = meta(128.0, "8A", 0.5);
        let to = meta(128.0, "8A", 0.5);

        assert_eq!(exit_between(&from, &to, false), Exit::Switch);
    }
}

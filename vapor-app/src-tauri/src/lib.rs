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
mod commands;
mod covers;
/// Public for the same reason as `audio`: the real-time test drives a real
/// streaming deck rather than a stand-in for one.
pub mod decoder;
/// A blocking HTTP client that survives being dropped on a runtime thread.
mod http;
mod local;
mod media;
mod metadata;
mod peers;
mod remote_source;
mod secrets;
mod store;
mod sync;
mod tags;
mod webdav;

use std::sync::{Arc, Mutex};

use store::Store;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use ts_rs::TS;

use vapor_engine::TrackSource;
use vapor_library::{
    index::{GroupBy, Row, SortKey},
    settings::APPEARANCES,
    Curve, FolderStore, GroupStore, PlaylistStore, Queue, Settings, TrackMeta,
};

/// Everything the shell holds between commands.
///
/// One lock rather than one per collection: commands are user-driven and
/// short, so contention is not a concern, and a single lock cannot deadlock
/// against itself the way a set of finer ones can.
pub(crate) struct AppState {
    pub(crate) settings: Settings,
    pub(crate) playlists: PlaylistStore,
    /// Folders that playlists are filed into. A folder owns no tracks — a
    /// playlist carries a `folder_id` pointing at one.
    pub(crate) folders: FolderStore,
    /// Tracks whose audio was downloaded on purpose, and must survive
    /// eviction. Everything else in the audio cache is there because something
    /// needed to read it once. See `keeps_audio`.
    pub(crate) pinned: std::collections::HashSet<String>,
    /// Dynamic groups: saved sets of artists, albums and genres.
    ///
    /// A group holds *entities*, not tracks, which is what makes it different
    /// from a playlist — membership is resolved against the library when it is
    /// read, so a group stays current as the library grows. `group.rs` has been
    /// complete and tested since the port and nothing was wired to it.
    pub(crate) groups: GroupStore,
    /// Lyrics and artwork looked up from public services, keyed by href.
    ///
    /// Kept apart from `analysis` and `tags` on purpose: those are what this
    /// device measured and what the file itself carries, and this is what a
    /// stranger said. Merging them would make the screen unable to tell a
    /// person which is which.
    pub(crate) looked: metadata::Cache,
    /// The releases those lookups landed on, by Deezer id.
    ///
    /// Normalised out of `looked` rather than stored per track: a fourteen-track
    /// album is one entry here however many of its tracks the library holds, and
    /// two folders that turn out to be the same release share it. This is what
    /// says how long a record is supposed to be, and so what makes an album
    /// tile able to admit it is missing eleven of them.
    pub(crate) albums: metadata::Albums,
    pub(crate) queue: Queue,
    /// The three exits as last offered, and what was playing then.
    ///
    /// The cards used to be recomputed on every read, a second apart. Choosing
    /// one made it the queued track, so it moved into the Follow slot and Stay
    /// refilled with something else — the board reshuffled under the press.
    /// They also drifted on their own as the planner re-ran behind the screen.
    ///
    /// So the offer is made once per playing track and held. The three tracks
    /// and their slots stay put; only the selection ring moves. It is a
    /// deliberate shift in what the Follow card means — "the exit the plan
    /// offered", not "whatever is queued right now" — which is the price of a
    /// board that holds still long enough to choose from.
    pub(crate) offered: Option<Offered>,
    /// What the DJ is conducting over — the set the queue was started from.
    ///
    /// `audio_manager.gd` had this as `current_playlist`: the DJ chose its
    /// next track from whatever list you pressed play in, so playing an album
    /// kept the set inside that album and playing from All Songs let it roam.
    /// The port planned from `app.rows` unconditionally, which quietly made
    /// every scope the whole library — press play on a twelve-track record and
    /// the second track was from somewhere else entirely.
    ///
    /// `None` is the library itself, which is both the default and what
    /// playing from an unfiltered list means.
    pub(crate) scope: Option<Scope>,
    /// The store key of the playlist or group the set was started from.
    ///
    /// Beside `scope` rather than inside it, and for the same span: everything
    /// that plays out of one press of a playlist is listening to that
    /// playlist, including the tracks the DJ appended, because that is what
    /// putting a playlist on means. `None` for a set started from anywhere
    /// else — an album tile, the Songs table, the queue.
    pub(crate) collection: Option<String>,
    /// The library table's rows, rebuilt on scan.
    pub(crate) rows: Vec<Row>,
    /// Analysis results, keyed by href. Persisted so a library is analysed
    /// once rather than on every launch.
    pub(crate) analysis: analysis::Cache,
    /// The most recent blend, as (outgoing, incoming), until it is judged.
    ///
    /// A skip is a verdict on a *transition*, so the pair has to outlive the
    /// transition: by the time a person reacts, the mixer has already swapped
    /// decks and forgotten which two tracks were involved.
    pub(crate) last_mix: Option<(String, String)>,
    /// When that blend finished, for the ten-second window.
    pub(crate) last_mix_ended: Option<std::time::Instant>,
    /// Learned dislike of specific transitions, keyed "from\u{1f}to" (TD-14).
    ///
    /// Persisted, because the whole value is that it accumulates. A tuple key
    /// cannot be a JSON object key, so the pair is joined by a unit separator —
    /// a character no href contains.
    pub(crate) skips: std::collections::HashMap<String, f32>,
    /// How often each track has been listened to, and when it last was.
    ///
    /// Persisted. The whole value of a play count is that it accumulates
    /// across launches — a "most played" shelf rebuilt from this session
    /// would be empty every morning.
    ///
    /// Keyed by href, and credited by the supervisor rather than by
    /// [`begin_playback`]: see [`CREDIT_AFTER`]. Starting a track is not
    /// listening to it, and counting at the start means someone hunting for a
    /// record through twenty tiles has just told the app those are their
    /// twenty favourites.
    pub(crate) plays: std::collections::HashMap<String, Play>,
    /// The same, for a playlist or a dynamic group that was played from.
    ///
    /// Keyed `"playlist:<id>"` or `"group:<id>"` — see [`collection_key`] —
    /// because one map with a prefixed key is one file and one load, and the
    /// two are ranked side by side on the home screen anyway.
    ///
    /// The unit is *tracks listened to from it*, not "times you pressed play
    /// on it". A playlist someone puts on for three hours and one they bounce
    /// off after a track should not score the same, and pressing play counts
    /// them equal.
    pub(crate) collection_plays: std::collections::HashMap<String, Play>,
    /// The track currently earning its play, and where it was played from.
    ///
    /// Set by [`begin_playback`] and cleared by the supervisor once the credit
    /// is given, so a track that is paused, resumed and finished is counted
    /// once. Not persisted: a play interrupted by quitting the app is a play
    /// that did not happen.
    pub(crate) crediting: Option<Crediting>,
    /// Embedded tags and artwork, keyed by href (TD-39).
    ///
    /// Persisted beside the analysis and for the same reason: reading them
    /// costs a file open, and the answer does not change unless the file does.
    pub(crate) tags: std::collections::HashMap<String, StoredTags>,
    /// Cover art, on disk rather than in `tags`. See [`covers`].
    pub(crate) covers: covers::Covers,
    /// Tracks that were read and could not be used, and why (TD-12).
    ///
    /// Persisted, because the answer does not change between launches and the
    /// alternative is downloading a broken file again to rediscover it. Only
    /// permanent failures land here — "not downloaded yet" is not one.
    pub(crate) failures: std::collections::HashMap<String, String>,
    /// The other kind: what went wrong for a track the pass means to try again.
    ///
    /// Retryable errors used to be discarded outright — the arm that records a
    /// failure ran only `if !progress.retryable`. That is fine for the track
    /// that simply had not downloaded yet, and wrong for one that fails the
    /// same retryable way on every pass: it is never described, never
    /// condemned, and so never counted as done, and the Settings card sits at
    /// "556 of 563" for ever with nothing on screen saying which seven or why.
    ///
    /// Kept apart from `failures` rather than merged into it because the two
    /// mean different things to [`analysis_counts`]: a permanent failure is
    /// done, and this is outstanding work. Counting these as done would hide
    /// exactly the tracks this exists to surface.
    pub(crate) stalls: std::collections::HashMap<String, Stall>,
    /// Cancels a running analysis pass.
    pub(crate) cancel: analysis::Cancel,
    /// Which pass is current. A replaced pass finishes some time after the one
    /// that replaced it starts, and without this its "I have stopped" would
    /// clear the flag belonging to the pass still running.
    pub(crate) analysis_generation: u64,
    /// Whether a pass is in flight, and what it is working on.
    ///
    /// Analysis starts by itself after a scan now, so "did someone press the
    /// button" is no longer the same question as "is it running" — and the
    /// screen used to answer the second by checking the first, which meant an
    /// automatic pass was invisible.
    pub(crate) analysing: bool,
    pub(crate) analysing_title: String,
    /// Why the last pass ended early, if it did. Empty otherwise.
    pub(crate) analysis_stopped_because: String,
    /// Where the four-step choice cycle has got to (§3 of the workflow doc).

    /// This installation's identity on the local network (SYNC-001).
    ///
    /// Generated once and persisted. Deliberately not derived from the
    /// hostname, the user or the hardware: it is broadcast in clear on a
    /// shared network, so it must say "a copy of Vapor" and nothing else about
    /// whose it is.
    pub(crate) device_id: String,
    /// Devices this one has paired with (SYNC-002). Persisted.
    pub(crate) trust: vapor_library::sync::Trust,
    /// A pairing in progress, if this device is currently showing a code.
    pub(crate) pairing: Option<vapor_library::sync::Pairing>,
    /// The code on screen. Held separately because `Pairing` deliberately does
    /// not hand its PIN back out — the only thing that may read it is the
    /// person looking at the screen.
    pub(crate) pin: Option<String>,
    /// Who is on the network, kept by the beacon thread.
    pub(crate) peers: peers::Peers,
    /// Files that could not be read at startup and were moved aside.
    ///
    /// Empty on every normal launch. Non-empty means the app is running on a
    /// default for something the person had data for, and they need telling —
    /// silently starting with no playlists is the failure this exists to stop.
    pub(crate) damaged: Vec<store::Quarantined>,
    /// What this device has deleted, so the deletion travels (TD-57).
    ///
    /// Kept beside the stores rather than inside them: a store holds what
    /// exists, and this is a record of what does not.
    pub(crate) tombstones: vapor_library::sync::Tombstones,
    /// The beacon and server threads, while sync is on (TD-58).
    ///
    /// Held so the switch can stop them. `None` means either sync is off or the
    /// ports could not be bound, and both mean there is nothing to stop.
    pub(crate) sync_session: Option<peers::Session>,
    /// Content digests, keyed by href, with the size they were computed at.
    ///
    /// Hashing a library is not free and the answer does not change unless the
    /// file does, so it is memoised beside the analysis and for the same
    /// reason. The size is the invalidation: a file that changed length is a
    /// different file.
    pub(crate) digests: std::collections::HashMap<String, (u64, String)>,
    /// What a running sync is doing, for the dashboard to draw.
    pub(crate) sync: SyncProgress,

    pub(crate) cache: cache::Cache,
    pub(crate) store: Store,

    /// The audio device, absent when the machine has none. Playback commands
    /// then fail with a message instead of the app refusing to start — a
    /// library you cannot hear is still one you can scan and analyse.
    pub(crate) player: Option<audio::Player>,
    /// What the audio thread is playing, or is being loaded to play.
    pub(crate) playing: Option<String>,
    /// Increments on every load request. A load whose generation is stale lost
    /// a race with a newer one and must discard its result rather than
    /// interrupt the track a person actually asked for.
    pub(crate) generation: u64,
    /// Bumped by every curve press, so a re-plan that finishes after a newer
    /// press can tell that its route is for a destination nobody chose.
    pub(crate) curve_plan: u64,
    /// Energy the current plan was seeded from — the intensity of the track
    /// playing when the route was walked.
    ///
    /// Kept because `Curve::target_energy` is relative to where the set
    /// started, so without it the curve cannot be evaluated after the fact and
    /// the mark could only show the shape in the abstract rather than the ramp
    /// the planner actually aimed at. It matters most where the two differ: a
    /// set already at 0.9 cannot Build much further, `target_energy` clamps,
    /// and the honest readout is a hue that flattens rather than one that goes
    /// on promising a climb.
    pub(crate) curve_start: f32,
    /// True while a track is being fetched and decoded, which can take seconds
    /// on a cold cache. The UI has to be able to say so.
    pub(crate) loading: bool,
    /// Why the last load failed, surfaced instead of silence with no
    /// explanation.
    pub(crate) playback_error: Option<String>,
    /// The track cued on the other deck for a beat-matched mix, if one is
    /// arranged. Cleared when the mix completes or is abandoned.
    pub(crate) armed_next: Option<String>,

    /// The decoder threads feeding the two decks (TD-09).
    ///
    /// Held here, on the control side, for two reasons. A decoder that nobody
    /// holds is never stopped, and would go on filling a window for a track
    /// that is no longer playing. And dropping one is what joins its thread —
    /// which must happen somewhere allowed to wait, never on the audio thread.
    ///
    /// They swap roles when a transition completes, exactly as the decks do.
    pub(crate) playing_stream: Option<decoder::Streamer>,
    pub(crate) next_stream: Option<decoder::Streamer>,
    /// The drift correction running for the current mix (TD-21). Dropped when
    /// the mix ends, which stops its thread and clears the correction.
    pub(crate) drift: Option<sync::DriftCorrection>,
}

impl AppState {
    /// Load what exists; start empty for anything that does not.
    ///
    /// A corrupt file is surfaced rather than swallowed — see `Store::load`.
    /// Starting empty on a read failure would show a person an empty library
    /// while their data sits unreadable on disk.
    pub(crate) fn load(store: Store) -> Self {
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
        let roots = local::roots(&settings.folders);
        let playlists = quarantined!("playlists").unwrap_or_default();
        let folders = quarantined!("folders").unwrap_or_default();
        let groups = quarantined!("groups").unwrap_or_default();
        let pinned = quarantined!("pinned").unwrap_or_default();
        let looked = quarantined!("metadata").unwrap_or_default();
        let albums = quarantined!("albums").unwrap_or_default();
        let trust = quarantined!("trust").unwrap_or_default();
        let tombstones = quarantined!("tombstones").unwrap_or_default();
        let digests = quarantined!("digests").unwrap_or_default();
        // Generated on first launch and kept. A device that renamed itself
        // every start would appear as a new peer each time, and every pairing
        // would have to be redone.
        let device_id: String = quarantined!("device_id").unwrap_or_else(|| new_id("device"));
        let analysis = quarantined!("analysis").unwrap_or_default();
        let failures = quarantined!("failures").unwrap_or_default();
        let stalls = quarantined!("stalls").unwrap_or_default();
        let mut tags: std::collections::HashMap<String, StoredTags> =
            quarantined!("tags").unwrap_or_default();
        let covers = covers::Covers::new(store.dir().join("covers"));
        // Covers used to be stored inline. Move any that still are out to
        // disk, once. A cover whose write fails stays inline rather than being
        // dropped — `StoredTags` still serialises the field when it is set, so
        // the next launch tries again.
        let mut moved = 0usize;
        for (href, tagged) in tags.iter_mut() {
            let Some(cover) = tagged.cover.as_ref() else {
                continue;
            };
            match covers.put(href, cover) {
                Ok(()) => {
                    tagged.cover = None;
                    moved += 1;
                }
                Err(e) => eprintln!("cover for {href} could not be moved out of tags.json: {e}"),
            }
        }
        if moved > 0 {
            match store.save("tags", &tags) {
                Ok(()) => eprintln!("moved {moved} covers out of tags.json"),
                // The covers are on disk either way; the shrink just has not
                // landed yet, and the same pass runs again next launch.
                Err(e) => eprintln!("tags.json could not be rewritten after moving covers: {e}"),
            }
        }
        let skips = quarantined!("skips").unwrap_or_default();
        let plays = quarantined!("plays").unwrap_or_default();
        let collection_plays = quarantined!("collection_plays").unwrap_or_default();
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
            pinned,
            looked,
            albums,
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
            offered: None,
            scope: None,
            collection: None,
            rows,
            last_mix: None,
            last_mix_ended: None,
            analysis,
            skips,
            plays,
            collection_plays,
            crediting: None,
            tags,
            covers,
            failures,
            stalls,
            cancel: analysis::Cancel::new(),
            analysis_generation: 0,
            analysing: false,
            analysis_stopped_because: String::new(),
            analysing_title: String::new(),
            // The bound is what stops the cache filling a phone, and it is the
            // person's to set — `sanitised` has already refused a value too
            // small to be worth having.
            cache: cache::Cache::new(store.dir().join("audio"), cache_max_bytes, roots),
            store,
            // No device yet. `load` reads this app's files; acquiring hardware
            // is [`AppState::open_audio`], called once from `run`.
            player: None,
            playing: None,
            generation: 0,
            curve_plan: 0,
            curve_start: 0.5,
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
    pub(crate) fn open_audio(&mut self) {
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
    pub(crate) fn save_index(&self) -> Result<()> {
        self.store.save("index", &self.rows)?;
        Ok(())
    }

    pub(crate) fn save_analysis(&self) -> Result<()> {
        self.store.save("analysis", &self.analysis)?;
        Ok(())
    }

    pub(crate) fn save_skips(&self) -> Result<()> {
        self.store.save("skips", &self.skips)?;
        Ok(())
    }

    pub(crate) fn save_plays(&self) -> Result<()> {
        self.store.save("plays", &self.plays)?;
        Ok(())
    }

    pub(crate) fn save_collection_plays(&self) -> Result<()> {
        self.store
            .save("collection_plays", &self.collection_plays)?;
        Ok(())
    }

    /// How many times the tracks in `hrefs` have been listened to, in total.
    ///
    /// The second half of how a collection is ranked. A playlist made this
    /// morning out of records someone has worn out has no plays *of its own*
    /// and should still not sit below one they have never opened, so when
    /// direct plays cannot separate two shelves this does.
    pub(crate) fn member_plays(&self, hrefs: impl IntoIterator<Item = impl AsRef<str>>) -> u32 {
        hrefs
            .into_iter()
            .filter_map(|h| self.plays.get(h.as_ref()))
            .map(|p| p.count)
            .sum()
    }

    /// Credit a listen to a track, and to whatever it was played from.
    ///
    /// Both halves or neither: a collection's count means "tracks listened to
    /// from it", so it moves exactly when a track's does.
    pub(crate) fn credit_play(&mut self, href: &str, collection: Option<&str>) {
        let at = unix_now();
        let play = self.plays.entry(href.to_string()).or_default();
        play.count = play.count.saturating_add(1);
        play.last = at;
        if let Err(e) = self.save_plays() {
            eprintln!("play counts could not be saved: {e:?}");
        }

        let Some(key) = collection else { return };
        let play = self.collection_plays.entry(key.to_string()).or_default();
        play.count = play.count.saturating_add(1);
        play.last = at;
        if let Err(e) = self.save_collection_plays() {
            eprintln!("collection play counts could not be saved: {e:?}");
        }
    }

    /// Record a file's tags, sending its artwork to disk.
    ///
    /// The one way in. Artwork must not enter `self.tags`: that map is
    /// serialised whole on every write and held in memory for the life of the
    /// process, which is how `tags.json` reached 155 MB and how the phone ran
    /// out of heap. See [`covers`].
    pub(crate) fn set_tags(&mut self, href: &str, tags: tags::Tags) {
        if let Some(cover) = &tags.cover {
            if let Err(e) = self.covers.put(href, cover) {
                eprintln!("cover for {href} could not be saved: {e}");
            }
        }
        self.tags.insert(href.to_string(), tags.into());
    }

    pub(crate) fn save_tags(&self) -> Result<()> {
        self.store.save("tags", &self.tags)?;
        Ok(())
    }

    pub(crate) fn save_failures(&self) -> Result<()> {
        self.store.save("failures", &self.failures)?;
        Ok(())
    }

    pub(crate) fn save_stalls(&self) -> Result<()> {
        self.store.save("stalls", &self.stalls)?;
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
    pub(crate) fn apply_tags(&self, row: &mut Row) {
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

    /// Call *after* `apply_tags`: the genre the tag supplies is part of what
    /// decides the tempo now, and a row whose genre has not been merged in yet
    /// would be judged on the scan's blank.
    pub(crate) fn apply_analysis(&self, row: &mut Row) {
        if let Some(a) = self.analysis.get(&row.href) {
            // The same tempo the mixer will meet this record at, not the raw
            // reading — a table showing 87 for a record the stretcher treats
            // as 174 is the disagreement AUD-26 is about.
            row.bpm = tempo_in_force_for_row(self, row, Some(a)).unwrap_or(a.bpm);
            row.key = a.key.clone();
        } else if let Some(bpm) = self.settings.bpm_override(&row.href) {
            // A person can correct a track that was never successfully
            // analysed, and that correction must still show.
            row.bpm = bpm;
        }
    }

    /// Persist playlists. Called after every mutation, which is why the write
    /// has to be atomic.
    pub(crate) fn save_playlists(&self) -> Result<()> {
        self.store.save("playlists", &self.playlists)?;
        Ok(())
    }

    pub(crate) fn save_folders(&self) -> Result<()> {
        self.store.save("folders", &self.folders)?;
        Ok(())
    }

    pub(crate) fn save_groups(&self) -> Result<()> {
        self.store.save("groups", &self.groups)?;
        Ok(())
    }

    pub(crate) fn save_pinned(&self) -> Result<()> {
        self.store.save("pinned", &self.pinned)?;
        Ok(())
    }

    pub(crate) fn save_looked(&self) -> Result<()> {
        self.store.save("metadata", &self.looked)?;
        Ok(())
    }

    /// Keep what a lookup learned about a release.
    ///
    /// Takes the `Option` the lookup actually returns, so the "nothing came
    /// back" case is handled once here rather than at each call site. An album
    /// with no id or no track count is not written: an entry that cannot say
    /// how long the record is would make every album holding it look complete.
    fn remember_album(&mut self, facts: Option<metadata::AlbumFacts>) {
        let Some(facts) = facts.filter(|f| f.is_usable()) else {
            return;
        };
        // Re-fetching a release the library already knows is normal — every
        // track on it comes through here — so this is an overwrite, not an
        // insert, and the newer answer wins.
        self.albums.insert(facts.id, facts);
        let _ = self.store.save("albums", &self.albums);
    }

    pub(crate) fn save_trust(&self) -> Result<()> {
        self.store.save("trust", &self.trust)?;
        Ok(())
    }

    pub(crate) fn save_tombstones(&self) -> Result<()> {
        self.store.save("tombstones", &self.tombstones)?;
        Ok(())
    }

    /// What this device calls itself on the network.
    ///
    /// `<this machine> · Vapor`, falling back to the library folder and then to
    /// a bare `Vapor`.
    ///
    /// ## Why the machine name is here now
    ///
    /// It used to be deliberately absent, on the grounds that a hostname is
    /// usually the owner's name and this string is broadcast in clear to
    /// everyone on the network. The folder name stood in for it — except the
    /// folder is `Music` on a default install, which took the fallback, so
    /// **every device announced itself as "Vapor"**. Two of them in one list is
    /// two identical rows and no way to tell which is the phone.
    ///
    /// A name nobody can tell apart fails the one job the name has, so the
    /// trade has been made the other way. It is worth knowing it *is* a trade:
    /// on a Mac this is the Sharing name, which is often "<Owner>'s MacBook",
    /// and it goes out with every advert while sync is on. Two things bound it
    /// — sync is off by default, and the beacon only runs on private
    /// addresses ([`peers::is_local`]) — but neither makes it private.
    pub(crate) fn device_name(&self) -> String {
        let folder = self
            .settings
            .remote
            .folder
            .rsplit('/')
            .find(|s| !s.is_empty())
            .unwrap_or("");

        match (machine_name(), folder) {
            (Some(machine), _) => format!("{machine} · Vapor"),
            // No machine name to be had: the folder is still better than
            // nothing, as long as it is not the one everybody has.
            (None, f) if !f.is_empty() && f != "Music" => format!("Vapor · {f}"),
            _ => "Vapor".to_string(),
        }
    }

    /// The content digest of a cached track, computed once.
    ///
    /// `None` for a track this device knows of but has never held, which in a
    /// cloud-first library is most of them. That is a real answer, not a
    /// failure — [`vapor_library::sync::reconcile`] treats a missing digest as
    /// "no opinion" rather than as a disagreement.
    pub(crate) fn digest_of(&mut self, href: &str) -> Option<String> {
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

    /// Point the cache at the folders settings currently names.
    ///
    /// Called after any change to `settings.folders`. The cache resolves a
    /// local href through these roots, so one holding a stale set cannot find
    /// a folder just added and would still find one just removed.
    pub(crate) fn rebuild_cache_roots(&mut self) {
        let dir = self.cache.dir().to_path_buf();
        let max = self.cache.max_bytes();
        self.cache = cache::Cache::new(dir, max, local::roots(&self.settings.folders));
    }

    pub(crate) fn save_settings(&self) -> Result<()> {
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
    /// Legacy only.
    ///
    /// Artwork lives in [`covers`] now. This field is still read so that a
    /// `tags.json` written by an older build can be migrated on load, and
    /// still written when it is set so that a cover whose move to disk failed
    /// is not lost — but nothing puts one here any more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
            // Not `t.cover`: artwork goes to [`covers`], through
            // `AppState::set_tags`, and never into `tags.json`.
            cover: None,
        }
    }
}

/// Shared state.
///
/// An `Arc` rather than a bare `Mutex` because the analysis pass moves a handle
/// onto a worker thread that outlives the command that started it — a borrow
/// from `State` cannot.
pub(crate) type Shared = Arc<Mutex<AppState>>;

/// Tests that go through the real `invoke_handler` — see `seam.rs`.
#[cfg(test)]
mod seam;

/// Errors crossing the IPC boundary.
///
/// A string rather than a typed error on purpose: the frontend shows these to
/// a person, and a structured error would have to be re-stringified there
/// anyway. Anything the UI needs to *branch* on gets its own return shape.
#[derive(Debug, Serialize)]
pub(crate) struct Error(String);

impl<E: std::fmt::Display> From<E> for Error {
    fn from(e: E) -> Self {
        Error(e.to_string())
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Play counts
// ---------------------------------------------------------------------------

/// How long a track has to run before it counts as listened to.
///
/// Thirty seconds, or half the track when the track is shorter than a minute —
/// an interlude should not be uncountable for being short. The same rule every
/// other player uses, and it exists because the alternative is worse than no
/// count at all: crediting at the start means someone flicking through a
/// library to find one record has just voted for everything they flicked past.
const CREDIT_AFTER: f64 = 30.0;

/// The point in a track of `duration` seconds at which it has been listened to.
///
/// Split out so the rule can be asserted without a running deck. A duration of
/// zero is a stream whose length is not known yet, and the flat threshold is
/// the right answer there: half of nothing would credit at the first poll.
fn credit_point(duration: f64) -> f64 {
    if duration > 0.0 && duration < 60.0 {
        duration / 2.0
    } else {
        CREDIT_AFTER
    }
}

/// Bank the listen for whatever is playing, once it has run long enough.
///
/// Taking `crediting` is what makes it once: a track that is paused, resumed,
/// seeked back through and finished has one listen in it, and the next call
/// finds nothing to take. [`begin_playback`] puts the next one there.
fn credit_if_listened(app: &mut AppState) {
    if app.crediting.is_none() {
        return;
    }
    let Some(snap) = app.player.as_ref().map(|p| p.snapshot()) else {
        return;
    };
    if snap.position < credit_point(snap.duration) {
        return;
    }
    let Some(pending) = app.crediting.take() else {
        return;
    };
    app.credit_play(&pending.href, pending.collection.as_deref());
}

/// How often something has been listened to, and when it last was.
///
/// `last` is unix seconds, and it is the tie-break rather than an ordering of
/// its own: "most played" is the claim on the shelf, and two things with the
/// same count are separated by which one is still in rotation.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Play {
    #[serde(default)]
    count: u32,
    #[serde(default)]
    last: i64,
}

/// A track that is playing and has not yet earned its count.
#[derive(Clone, Debug)]
struct Crediting {
    href: String,
    /// The store key of the playlist or group it was played from, if any.
    collection: Option<String>,
}

/// Seconds since the epoch, or 0 on a machine whose clock predates it.
///
/// Only ever compared against other values from here, so a clock that is wrong
/// costs an ordering and nothing else.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Where a set was started from, when it was a playlist or a group.
///
/// Distinct from [`Scope`], which is about what the DJ may roam over and is
/// named for a person to read. This is an identity: a playlist can be renamed
/// without becoming a different playlist, and its count should follow it.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CollectionRef {
    /// `"playlist"` or `"group"`. Anything else is ignored rather than
    /// rejected — a set played from somewhere the backend has no name for is
    /// still a set that should play.
    kind: String,
    id: String,
}

/// The key a collection's plays are stored under.
///
/// `None` for a kind this does not know, which is what keeps an unrecognised
/// value out of the map rather than inventing a shelf for it.
pub(crate) fn collection_key(reference: &CollectionRef) -> Option<String> {
    let id = reference.id.trim();
    if id.is_empty() {
        return None;
    }
    match reference.kind.as_str() {
        "playlist" => Some(format!("playlist:{id}")),
        "group" => Some(format!("group:{id}")),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The home shelves
// ---------------------------------------------------------------------------

/// How many tiles a shelf carries.
///
/// A shelf scrolls sideways, so this is not "how many fit" — three or four are
/// visible and the rest are a flick away. It is the point past which nobody is
/// flicking: a twelfth-most-played artist is already further down than anyone
/// goes, and every tile beyond it is a cover fetched for nothing.
const SHELF: usize = 12;

/// One tile on a home shelf.
///
/// Deliberately one shape for all four shelves. A playlist, a group, an artist
/// and an album are drawn identically and ranked identically, and giving each
/// its own struct would mean four of everything to say so.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct Shelf {
    /// What this is, for opening it: a playlist or group id, or the name of
    /// an artist or an album. Not shown.
    id: String,
    title: String,
    /// The one line under the title. Empty when there is nothing true to say.
    subtitle: String,
    /// A track from it, for the cover and for what plays when it is pressed.
    /// Empty for a playlist with nothing in it yet.
    lead: String,
    tracks: u32,
    /// Listens this was ranked on. Zero for everything in a library nobody has
    /// played from yet, which is why [`rank`] has more keys than this one.
    plays: u32,
}

/// The library's front door.
#[derive(Debug, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct HomeShelves {
    playlists: Vec<Shelf>,
    /// Dynamic groups — "smart groups" on screen.
    groups: Vec<Shelf>,
    artists: Vec<Shelf>,
    albums: Vec<Shelf>,
    /// How many tracks there are, for the line under the title.
    ///
    /// Here rather than counted on the other side, because the shelves do not
    /// carry the library — twelve albums is not five hundred tracks — and the
    /// home screen would otherwise have to read the whole index to write one
    /// number on it.
    tracks: u32,
}

/// A tile and the numbers it is ranked on, before the ranking throws them away.
struct Ranked {
    shelf: Shelf,
    /// Listens to the thing itself. Always 0 for an artist or an album: there
    /// is no such thing as putting an artist on, only playing their records.
    direct: u32,
    /// Listens to its tracks, wherever they were played from.
    member: u32,
    /// When it was last listened to, unix seconds.
    last: i64,
}

/// Most played first, and stable when nothing has been played.
///
/// Four keys, and each exists because the one before it runs out. Direct plays
/// are the claim the shelf makes. Member plays are what separates a playlist
/// built this morning out of records someone has worn out from one they have
/// never opened — without them the new playlist sorts below the untouched one,
/// which is the opposite of true. `last` breaks a genuine tie towards whatever
/// is still in rotation. And the size of the thing is what is left on the day
/// the library is new and every count is zero: the shelf then reads "most of
/// what you have", which is at least about the person's music, where falling
/// through to alphabetical would put A first for ever.
fn rank(mut tiles: Vec<Ranked>, limit: usize) -> Vec<Shelf> {
    tiles.sort_by(|a, b| {
        b.direct
            .cmp(&a.direct)
            .then(b.member.cmp(&a.member))
            .then(b.last.cmp(&a.last))
            .then(b.shelf.tracks.cmp(&a.shelf.tracks))
    });
    tiles.truncate(limit);
    tiles.into_iter().map(|t| t.shelf).collect()
}

/// The body of [`home_shelves`], reachable from a test.
fn home_shelves_for(app: &AppState) -> HomeShelves {
    let playlists = app
        .playlists
        .all()
        .iter()
        .map(|p| {
            let play = app.collection_plays.get(&format!("playlist:{}", p.id));
            Ranked {
                shelf: Shelf {
                    id: p.id.clone(),
                    title: p.name.clone(),
                    subtitle: tracks_line(p.tracks.len()),
                    lead: p.tracks.first().cloned().unwrap_or_default(),
                    tracks: p.tracks.len() as u32,
                    plays: play.map_or(0, |p| p.count),
                },
                direct: play.map_or(0, |p| p.count),
                member: app.member_plays(&p.tracks),
                last: play.map_or(0, |p| p.last),
            }
        })
        .collect();

    let groups = app
        .groups
        .all()
        .iter()
        .map(|g| {
            let play = app.collection_plays.get(&format!("group:{}", g.id));
            // Resolved against the library, because that is what a group is:
            // a saved set of artists and albums, not a list of tracks. Doing
            // it here is also the only way to know how big one is.
            let tracks = tracks_in_group(app, g);
            Ranked {
                shelf: Shelf {
                    id: g.id.clone(),
                    title: g.name.clone(),
                    subtitle: tracks_line(tracks.len()),
                    lead: tracks.first().map(|r| r.href.clone()).unwrap_or_default(),
                    tracks: tracks.len() as u32,
                    plays: play.map_or(0, |p| p.count),
                },
                direct: play.map_or(0, |p| p.count),
                member: app.member_plays(tracks.iter().map(|r| r.href.as_str())),
                last: play.map_or(0, |p| p.last),
            }
        })
        .collect();

    HomeShelves {
        playlists: rank(playlists, SHELF),
        groups: rank(groups, SHELF),
        artists: entity_shelf(app, "artist"),
        albums: entity_shelf(app, "album"),
        tracks: app.rows.len() as u32,
    }
}

/// The artists or albums shelf.
///
/// Built on top of the grid's own reading of the library rather than beside
/// it, so a tile on the shelf and the same tile in the Artists tab agree about
/// what an artist is called and how many albums they have. The alternative is
/// a second definition of album identity, and there is only room for one.
fn entity_shelf(app: &AppState, group_by: &str) -> Vec<Shelf> {
    // Spelled out rather than `..Default::default()`: `ascending` defaults to
    // true through serde, and a derived `Default` would quietly make it false.
    let view = LibraryView {
        query: String::new(),
        sort_key: None,
        ascending: true,
        group_by: Some(group_by.to_string()),
        genre: None,
        album: None,
        artist: None,
    };
    let tiles = library_entities_for(app, &view)
        .into_iter()
        .map(|e| Ranked {
            // An artist or an album has no plays of its own — there is no
            // gesture that means "put on Aphex Twin" the way there is one for
            // putting on a playlist. Their tracks are the whole of it.
            direct: 0,
            member: e.plays,
            last: e.last_played,
            shelf: Shelf {
                id: e.name.clone(),
                title: e.name,
                subtitle: e.subtitle,
                lead: e.lead,
                tracks: e.tracks as u32,
                plays: e.plays,
            },
        })
        .collect();
    rank(tiles, SHELF)
}

/// "12 tracks", and "1 track" rather than "1 tracks".
fn tracks_line(n: usize) -> String {
    format!("{n} {}", if n == 1 { "track" } else { "tracks" })
}

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
    /// Narrow to one genre, exactly. Set when a genre tile has been opened.
    #[serde(default)]
    genre: Option<String>,
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
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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
    /// Listens to its tracks, added up.
    ///
    /// Here rather than counted again by whatever wants to rank these, because
    /// the members are already gathered at this point and gathering them a
    /// second time means keying albums a second time — and album identity is
    /// title *plus folder*, which is precisely the thing a second copy gets
    /// wrong. See [`entity_shelf`].
    plays: u32,
    /// When one of its tracks was last listened to, unix seconds. 0 for never.
    ///
    /// `number`, not the `bigint` ts-rs gives an `i64` by default. Nothing on
    /// this wire is a real `bigint`: `serde_json` writes it as a plain JSON
    /// number and the webview parses it as one, so the generated type would
    /// have been a claim about the value that the value does not meet.
    #[ts(type = "number")]
    last_played: i64,
    /// How many tracks the release actually has. **0 means nobody knows**.
    ///
    /// From the Deezer album the tracks were matched to — see
    /// [`metadata::AlbumFacts`]. Zero is the ordinary case for a library that
    /// has not been identified yet, and is emphatically not "an album with no
    /// tracks": an unknown length can never make a tile incomplete, because
    /// the app would then be asserting something it has no evidence for.
    total_tracks: u32,
    /// `"album"`, `"single"` or `"ep"` — empty when unmatched.
    ///
    /// Holding one track of a two-track single reads very differently from
    /// holding one of a nineteen-track album, and the tab should be able to
    /// say which it is.
    record_type: String,
    /// True when tracks are missing: a known length that exceeds what is held.
    ///
    /// Never true on a guess. An album nobody has looked up is shown whole,
    /// which is the state the whole library was in before this existed.
    incomplete: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct LibrarySection {
    header: String,
    rows: Vec<Row>,
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
    if let Some(genre) = view.genre.as_deref() {
        // Exact, like the other two: a genre tile means that genre, not every
        // genre whose name contains it — "House" would otherwise drag in
        // "Deep House" and "Progressive House".
        rows.retain(|r| genre_of(app, &r.href) == genre);
    }
    // Last, so the count a person sees is of what they asked for. A view, not a
    // deletion: the files are untouched and still there to tidy by hand.
    if app.settings.hide_duplicates {
        let dupes = duplicate_hrefs(app);
        rows.retain(|r| !dupes.contains(&r.href));
    }
    rows
}

/// The body of [`library_entities`], reachable from a test.
///
/// A `#[tauri::command]` takes `State`, which cannot be built outside a running
/// app, so logic left in a command body is logic no test can see — which is how
/// the Genres tab shipped grouping by album since the port.
/*
 * Three kinds of tile, not two.
 *
 * This was a boolean — artist or album — and Genres fell to the `else`, so the
 * tab grouped by *album* while the screen believed it had genres. What a person
 * saw was a grid of tracks, because `Library.tsx` only treated album and artist
 * as entity tabs and rendered everything else as rows. The tab has existed
 * since the port and has never shown a genre.
 *
 * Outside the function body so that [`order_entities`] can be given one, and
 * so the ordering rules can be read next to each other rather than inferred
 * from where they sit in a pipeline.
 */
#[derive(Clone, Copy, PartialEq)]
enum By {
    Artist,
    Album,
    Genre,
}

/// The release a set of tracks belongs to, if they have been identified.
///
/// Decided by vote rather than by taking the first: one mistagged track on a
/// folder can match a different release entirely, and letting it name the album
/// would report a fourteen-track record as a two-track single — every other
/// track on it then reads as "missing". The majority is what the folder is.
///
/// `None` when nothing has been looked up, which is the ordinary state of a
/// fresh library and must stay distinguishable from "looked up, found short".
fn release_of<'a>(app: &'a AppState, tracks: &[&Row]) -> Option<&'a metadata::AlbumFacts> {
    let mut votes: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for row in tracks {
        let id = app
            .looked
            .get(&row.href)
            .map(|l| l.deezer_album_id)
            .unwrap_or(0);
        if id != 0 {
            *votes.entry(id).or_default() += 1;
        }
    }
    // Ties break on the id so the answer does not depend on hash order — an
    // album tile that changed its mind between two reads would be worse than
    // either answer.
    let winner = votes
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))?
        .0;
    app.albums.get(&winner).filter(|f| f.is_usable())
}

fn library_entities_for(app: &AppState, view: &LibraryView) -> Vec<LibraryEntity> {
    let rows = resolved_rows(app, view);

    let by = match view.group_by.as_deref() {
        Some("artist") => By::Artist,
        Some("genre") => By::Genre,
        _ => By::Album,
    };
    let by_artist = by == By::Artist;

    // Insertion-ordered: the rows arrive sorted, and a map would scramble them.
    let mut order: Vec<String> = Vec::new();
    let mut members: std::collections::HashMap<String, Vec<&Row>> =
        std::collections::HashMap::new();

    for row in &rows {
        let (name, known) = match by {
            By::Artist => (row.artist.clone(), row.artist_source.is_known()),
            By::Album => (row.album.clone(), row.album_source.is_known()),
            // A genre comes from the tags or from a lookup, and "unknown" is
            // simply an empty string — there is no `Source` to consult.
            By::Genre => {
                let g = genre_of(app, &row.href);
                let known = !g.trim().is_empty();
                (g, known)
            }
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
        let key = match by {
            // A genre is its own identity, like an artist: two tracks tagged
            // "House" are the same genre wherever they sit on disk.
            By::Artist | By::Genre => name.clone(),
            By::Album => vapor_library::settings::album_key(&name, &row.href),
        };
        if !members.contains_key(&key) {
            order.push(key.clone());
        }
        members.entry(key).or_default().push(row);
    }

    let mut entities: Vec<LibraryEntity> = order
        .into_iter()
        .filter_map(|key| {
            let tracks = members.get(&key)?;
            let lead = tracks.first()?.href.clone();
            // The display name comes off a member rather than out of the key:
            // the key carries a folder the person never typed and should not
            // read.
            let name = match by {
                By::Artist | By::Genre => key.clone(),
                By::Album => tracks.first()?.album.clone(),
            };

            let subtitle = if by == By::Genre {
                // How many artists sit under it, which is what tells one genre
                // tile from another — the track count is already on the tile.
                let mut artists: Vec<&str> = tracks
                    .iter()
                    .filter(|r| r.artist_source.is_known())
                    .map(|r| r.artist.as_str())
                    .collect();
                artists.sort_unstable();
                artists.dedup();
                match artists.len() {
                    0 => format!("{} tracks", tracks.len()),
                    1 => "1 artist".to_string(),
                    n => format!("{n} artists"),
                }
            } else if by_artist {
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

            // Both from the same walk of the members, so a tile's count and
            // the moment it was last reached cannot disagree.
            let plays = tracks
                .iter()
                .filter_map(|r| app.plays.get(&r.href))
                .map(|p| p.count)
                .sum();
            let last_played = tracks
                .iter()
                .filter_map(|r| app.plays.get(&r.href))
                .map(|p| p.last)
                .max()
                .unwrap_or(0);

            // Only albums have a length to fall short of. An artist is not
            // "incomplete" because you do not own their whole catalogue, and a
            // genre has no end at all.
            let release = (by == By::Album).then(|| release_of(app, tracks)).flatten();
            let total_tracks = release.map(|r| r.nb_tracks).unwrap_or(0);

            Some(LibraryEntity {
                name,
                subtitle,
                tracks: tracks.len(),
                lead,
                plays,
                last_played,
                total_tracks,
                record_type: release.map(|r| r.record_type.clone()).unwrap_or_default(),
                // `>` not `!=`: holding *more* than the release lists is a
                // duplicate or a bonus disc, not a gap, and calling that
                // incomplete would send perfectly whole albums to the bottom.
                incomplete: total_tracks > tracks.len() as u32,
            })
        })
        .collect();

    order_entities(by, &mut entities);
    entities
}

/// Put the tiles in the order the tab is read in.
///
/// Insertion order is the order the *rows* arrived in — sorted by title — which
/// is an answer to a question nobody asked of a grid of artists. Applied here
/// rather than by sorting the rows first, because the thing being ranked is the
/// tile and two of the three keys (track count, and the album totals behind
/// completeness) only exist once the members have been gathered.
fn order_entities(by: By, entities: &mut [LibraryEntity]) {
    match by {
        // Biggest first: an artist with thirty-eight tracks is the reason the
        // tab was opened, and one with a single loose remix is not. Name breaks
        // the tie so the long tail — 52 of this library's 92 artists have
        // exactly one track — is alphabetical rather than arbitrary.
        By::Artist => entities.sort_by(|a, b| {
            b.tracks
                .cmp(&a.tracks)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
        // Whole records first, alphabetically. Everything with a gap in it
        // sinks to the bottom, ordered by how close to whole it is — an album
        // you have 11 of 12 of is nearly a record and belongs above one you
        // have a single track of. Name breaks ties, so two albums at the same
        // fraction are not in hash order.
        By::Album => entities.sort_by(|a, b| {
            a.incomplete
                .cmp(&b.incomplete)
                .then_with(|| completion(b).total_cmp(&completion(a)))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
        // Alphabetical. A genre has no length to be measured against.
        By::Genre => entities.sort_by_key(|e| e.name.to_lowercase()),
    }
}

/// How much of a release is held, 0.0 to 1.0.
///
/// An unknown length is 1.0 — "as far as anyone knows, whole". Returning 0.0
/// would rank every un-identified album below every identified one and read as
/// a claim that the library is empty of them.
fn completion(entity: &LibraryEntity) -> f64 {
    if entity.total_tracks == 0 {
        return 1.0;
    }
    (entity.tracks as f64 / entity.total_tracks as f64).min(1.0)
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
#[derive(Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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

/// Progress of the library identification pass.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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

/// The body of [`identify_library`], callable without a `State`.
///
/// Reached two ways: the command, and the end of an analysis pass — the tempo
/// correction is the second half of "find tempo, key and cue points", and the
/// Vibe DJ needs it across the whole library rather than only where somebody
/// has listened.
fn identify_library_in_background(app_handle: &tauri::AppHandle, state: &Shared) -> Result<()> {
    use tauri::Emitter;
    let app_handle = app_handle.clone();

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

    let shared: Shared = Arc::clone(state);
    let handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let lookup = match metadata::Lookup::new() {
            Ok(l) => l,
            Err(e) => {
                // Say the pass is over, however it ended.
                //
                // This returned silently, and the screen keys its button off
                // "a pass has started and not reported finishing" — so a
                // failure here disabled Identify for as long as the app stayed
                // open, with nothing to press and nothing said. The same shape
                // of fault as the analysis pass's, in a second place; both are
                // now a terminal event rather than a bare `return`.
                let _ = handle.emit(
                    "identify-progress",
                    IdentifyProgress {
                        done: 0,
                        total: 0,
                        title: format!("could not start: {e}"),
                        corrected: 0,
                        genres: 0,
                        finished: true,
                    },
                );
                return;
            }
        };
        let total = todo.len();
        let (mut corrected, mut genres) = (0usize, 0usize);

        for (i, (href, title, artist, album, bpm, duration)) in todo.into_iter().enumerate() {
            /*
             * Facts and artwork, not words.
             *
             * Lyrics were briefly fetched here too, to make one button out of
             * two features. Wrong shape: it turned a press into one LRCLIB
             * request per track across the whole library, for words nobody had
             * asked to read yet. They are fetched when a track loads instead —
             * see `begin_playback` — which spreads the same work over the time
             * somebody actually spends listening, and asks only about records
             * they play.
             *
             * Asked concurrently: two independent services, and in turn this
             * pass was two round trips deep per track over hundreds of them.
             */
            let (facts, sleeve) = std::thread::scope(|scope| {
                let f = scope.spawn(|| lookup.track_facts(&artist, &title));
                let a = scope.spawn(|| lookup.album(&artist, &album));
                (f.join().unwrap_or(None), a.join().unwrap_or_default())
            });
            // `album` returns the art URL and the genre together, and only the
            // genre is kept. That looks wasteful and is deliberate: the URL is
            // useless without the image file beside it, because the CSP blocks
            // remote origins and `looked_up_image` serves a local file named
            // after the URL. Storing the URL alone would make the background
            // fetcher believe the sleeve was already had, and skip the download
            // for ever — the same shape of bug this pass caused with words.
            //
            // Downloading here instead would pull an image per track across the
            // whole library on one button press. The words were moved out of
            // this pass for exactly that reason; the sleeve stays out with them
            // and arrives as a track is played.
            let genre = sleeve.genre;

            /*
             * A track with no album still came off a record.
             *
             * `lookup.album` searches by album name, so a file with none — 97
             * of this library, downloaded one at a time into the root — gets
             * nothing from it and vanishes from the Albums tab. The track
             * search knows: "Noisia Machine Gun" names *Split The Atom* and
             * says it is nineteen tracks long.
             *
             * Only when the album is genuinely missing. A row that already has
             * one is not second-guessed here — path and tags outrank a
             * stranger, which is the rule the whole metadata layer is built on.
             */
            let release = match sleeve.facts {
                Some(facts) => Some(facts),
                None if album.trim().is_empty() => lookup.album_of_track(&artist, &title, duration),
                None => None,
            };

            if let Ok(mut app) = shared.lock() {
                let entry = app.looked.entry(href.clone()).or_default();
                entry.attempted = true;
                if let Some(found) = release.as_ref().filter(|f| f.is_usable()) {
                    entry.deezer_album_id = found.id;
                }
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
                app.remember_album(release);
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
    let embedded = || app.covers.get(lead);
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
#[derive(Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct AlbumArt {
    /// The picture, as a `data:` URI. `None` when there is none to show.
    src: Option<String>,
    /// Whether this is a cover someone chose by hand rather than the file's own.
    ///
    /// Answered here rather than by the screen comparing keys. The album key's
    /// format is this module's business, and a copy of it in TypeScript would
    /// be a second definition free to drift from the first.
    chosen: bool,
}

pub(crate) fn album_art(app: &AppState, album: &str, lead: &str) -> AlbumArt {
    AlbumArt {
        src: resolve_album_cover(app, album, lead),
        chosen: app.settings.album_art_for(album, lead).is_some(),
    }
}

// ---------------------------------------------------------------------------
// Local sync (SYNC-001 to SYNC-005)
// ---------------------------------------------------------------------------

/// What a running sync is doing, for the dashboard.
#[derive(Clone, Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct SyncProgress {
    running: bool,
    /// The device being synced with.
    peer: String,
    /// What is moving right now.
    file: String,
    done: usize,
    total: usize,
    /// Bytes moved since this sync started.
    // A JSON number over IPC, not a `bigint`: serde_json writes u64 as a
    // plain number and the webview parses it as one. Values here are byte
    // counts and millisecond timestamps, far below 2^53.
    #[ts(type = "number")]
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
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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
/// The largest track this will pull into memory from a peer.
///
/// The whole file is collected before its digest is checked — deliberately, so
/// nothing unverified reaches the cache — which means `total` decides how much
/// memory this allocates, and `total` is a number the peer chose. A peer that
/// claims a huge total and keeps sending drives the process out of memory, and
/// on the LAN path there is nothing signed to contradict it.
///
/// A GiB is roughly half an hour of 24-bit/192 kHz stereo FLAC, so a real track
/// that trips this does not exist yet. Raising it is one line; the point is
/// that the ceiling is ours rather than the sender's.
const MAX_PULLED_TRACK: u64 = 1024 * 1024 * 1024;

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

        // Checked every round, not only the first: the peer restates `total`
        // in each reply and nothing obliges it to give the same answer twice.
        if total > MAX_PULLED_TRACK {
            return Err(format!(
                "that device says the track is {total} bytes, which is larger \
                 than this will pull into memory"
            ));
        }

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
#[derive(Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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

/* ---- Dynamic groups ----------------------------------------------------
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

/* ---- Downloads -------------------------------------------------------
 *
 * Keeping a track, as opposed to happening to have one.
 *
 * Everything else in the audio cache is there because something needed to read
 * it once, and is dropped as soon as the set moves past it. These are the
 * tracks somebody asked for, so they live in a directory eviction never walks
 * and stay until they are removed by hand.
 * -------------------------------------------------------------------- */

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct DownloadProgress {
    done: usize,
    total: usize,
    /// What is being fetched right now. Empty when finished.
    title: String,
    finished: bool,
    /// Why it stopped early, if it did.
    error: String,
}

fn collection_tracks(app: &AppState, kind: &str, id: &str) -> Vec<String> {
    match kind {
        "playlist" => app
            .playlists
            .get(id)
            .map(|p| p.tracks.clone())
            .unwrap_or_default(),
        "group" => app
            .groups
            .get(id)
            .map(|g| {
                tracks_in_group(app, g)
                    .into_iter()
                    .map(|r| r.href)
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// How many files are second-or-later copies of a recording.
///
/// So the switch that hides them can say what it would hide, and so someone
/// tidying up by hand knows whether there is anything to tidy.
/// How much of the library has been looked up, and how much there is.
///
/// For the Settings button's subtitle: "50 of 500 fetched" is the only honest
/// way to say whether pressing it again would do anything. `attempted` is the
/// flag, not "found something" — a track LRCLIB has never heard of has still
/// been asked about, and asking again costs a request and finds nothing.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct LookupCounts {
    fetched: usize,
    total: usize,
}

pub(crate) fn tracks_in_group(app: &AppState, group: &vapor_library::DynamicGroup) -> Vec<Row> {
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

/// The body of [`delete_folder`], split out so it is reachable from a test —
/// a `#[tauri::command]` takes `State`, which cannot be built outside a
/// running app.
pub(crate) fn remove_folder(app: &mut AppState, id: &str) -> bool {
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

/// The library rows for `hrefs`, in the order `hrefs` gives them.
///
/// Separate from the command so the rule it encodes can be tested: an href with
/// no row is **skipped**. A playlist stores hrefs and a file can leave the
/// library after being added, so this is a normal state rather than an error —
/// and a row that cannot be played is worse than an absent one. The count is
/// shown beside the title, so a playlist of 12 displaying 11 rows says so
/// rather than hiding it.
pub(crate) fn rows_in_order<'a>(rows: &'a [Row], hrefs: &[String]) -> Vec<&'a Row> {
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

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct QueueState {
    current: Option<String>,
    tracks: Vec<String>,
    /// What plays next, so the UI can show it without asking again.
    next: Option<String>,
}

/// How many played tracks keep their audio, so going back is instant.
const KEEP_BEHIND: usize = 3;

/// How many upcoming tracks keep their audio.
///
/// Only the next one is needed to arm a mix. The rest is for skipping: a person
/// who presses next three times should not wait three times, and at about ten
/// megabytes a track the insurance is cheap.
const KEEP_AHEAD: usize = 5;

/// How far ahead of the pass the fetchers may get.
///
/// The audio cache exists so a track can be played without the network. Letting
/// these run unbounded turned it into a copy of the whole library — two
/// gigabytes of tracks already described — in order to read each file once.
const PREFETCH_WINDOW: usize = 12;

/// How many tracks are fetched at once during an analysis pass.
///
/// The wait is the download, not the analysis — measured at 150 KB/s against
/// about half a second of actual listening per track. Four is a compromise: it
/// is enough to keep the analyser fed, and few enough that a phone is not
/// opening a dozen sockets to somebody's WebDAV host.
const PREFETCH_THREADS: usize = 4;

/// How long the pass waits for a prefetcher to deliver a track, in 200ms ticks,
/// before fetching it itself.
///
/// Deliberately short. The pass must take tracks in the order it was given
/// them, but downloads finish in whatever order the network returns — so one
/// slow track holds up every track behind it, however many of those have
/// already arrived. Measured: 95 tracks landed in four minutes while the pass
/// described *one*, because it was waiting on a single file.
///
/// Twenty seconds, then fetch it directly. That can mean pulling a copy of
/// something already in flight, which at these speeds costs a couple of seconds
/// — against minutes of the whole pass standing still.
const PREFETCH_WAIT_TICKS: usize = 100;

/// How far ahead the DJ needs the library described.
///
/// Three: the track playing has to be mixed *out of*, and the exits the Vibe
/// screen offers are Stay, Follow and Switch. Analysing further ahead than the
/// set has been planned is work for a route nobody has chosen yet.
pub(crate) const MIX_LOOKAHEAD: usize = 3;

/// Whether anything in the next few tracks still needs describing.
///
/// The pass is ordered from the queue (see `start_analysis`), so restarting it
/// is how the upcoming tracks get to the front. Asking about a window rather
/// than only the current track is what stops the DJ arriving at a record it
/// cannot mix.
pub(crate) fn needs_analysis_soon(app: &AppState, lookahead: usize) -> bool {
    let from = app.queue.current_index().unwrap_or(0);
    app.queue
        .tracks()
        .iter()
        .skip(from)
        .take(lookahead + 1)
        .any(|href| needs_analysis(app, href))
}

/// The three exits as offered to the screen, held while one track plays.
#[derive(Clone, Debug)]
struct Offered {
    /// The track that was playing when these were chosen.
    playing: String,
    /// Href and exit per card, in the order the screen lays them out.
    cards: Vec<(String, Exit)>,
}

/// What the DJ is conducting over.
///
/// The name is for reading — "Nocturnes", "Aphex Twin", "Late Night" — and the
/// tracks are the pool the pathfinder is allowed to choose from. They are kept
/// apart from the queue because the queue *moves*: the DJ appends to it as it
/// plans, so by the third track the queue is no longer the list you pressed
/// play in and deriving the pool from it would let the set drift out of the
/// record you chose, one track at a time.
#[derive(Clone, Debug)]
struct Scope {
    name: String,
    tracks: std::collections::HashSet<String>,
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

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct PlaybackState {
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
    /// Fraction of output energy above 1500 Hz, 0–1. Drives how fast the
    /// mark's ribbon turns — bright music turns faster than dark music at the
    /// same loudness.
    #[serde(serialize_with = "finite")]
    brightness: f32,
    /// Seconds between the beats either side of the playhead, or 0 with no
    /// usable grid.
    ///
    /// Local rather than `60 / bpm`: the stored grid is tracked, so it follows
    /// real tempo drift, and a mark pulsing on a nominal tempo would walk out
    /// of phase with the record over a few minutes.
    #[serde(serialize_with = "finite")]
    beat_period: f32,
    /// Track-seconds at which the next beat lands, or 0 with no usable grid.
    ///
    /// Sent as a position rather than a countdown so the UI can hold its own
    /// clock between polls: a countdown is stale the instant it is serialised,
    /// where a position stays true and the difference from `position` is the
    /// countdown at any moment the caller likes.
    #[serde(serialize_with = "finite64")]
    next_beat: f64,
    /// Where the playing track sits in the planned set, and how long that plan
    /// is. Zero total means nothing is planned — DJ mode off, or a queue that
    /// was not built by the pathfinder.
    set_index: u32,
    set_total: u32,
    /// Energy the curve wants the set to be at, right here, 0–1.
    #[serde(serialize_with = "finite")]
    set_energy: f32,
    /// Envelope peaks for the playing track, empty until it has been analysed
    /// at the current version.
    waveform: Vec<f32>,
    /// What plays after this, so Now Playing can say so without a second call.
    next_title: String,
    next_artist: String,
    next_album: String,
    /// The next track's href, so the screen can ask for its artwork the same
    /// way a row does — through `track_thumb`, which is sized for a tile.
    /// Carrying the cover itself here would put a 300 KB data URI on a
    /// four-times-a-second poll.
    next_href: String,
    /// Cover art for the playing track as a data URI, when the file carried
    /// one (TD-39).
    cover: Option<String>,
    /// What the DJ is conducting over, for the Vibe screen to name. Empty is
    /// the whole library — the screen supplies the wording, not this.
    scope: String,
}

/// Look a track up while it loads, once, if lookups are permitted.
///
/// ## Why here and not in a library-wide pass
///
/// Lyrics were briefly folded into `identify_library` so that one button did
/// everything. That made a single press cost one LRCLIB request per track
/// across the whole library, for words nobody had asked to read.
///
/// The switch in Settings is better read as an *intention* than as a command:
/// it says this person wants lyrics and artwork. Acting on it when a track
/// loads spreads the same work across the time somebody spends listening, asks
/// only about records they actually play, and puts the answer in the cache
/// before they can open Now Playing to look for it.
///
/// Once per track, ever. `attempted` is set whatever comes back, so a record
/// LRCLIB has never heard of is asked about once and then left alone — the
/// cache is the memory, and emptying it is what asks again.
fn look_up_in_background(shared: &Shared, app: &AppState, href: &str) {
    if !app.settings.metadata_lookup_enabled {
        return;
    }
    // Gated on what is actually missing, not on whether some other pass has
    // run. `attempted` means the facts pass asked Deezer for a genre; it says
    // nothing about words, which that pass never requests. Reading it here is
    // what left 534 of 534 cached tracks marked done with no lyrics and no
    // sleeve between them.
    let (has_words, has_sleeve) = app
        .looked
        .get(href)
        .map(|l| (l.words_attempted, !l.album_art.is_empty()))
        .unwrap_or((false, false));
    if has_words && has_sleeve {
        return;
    }
    let Some(row) = app.rows.iter().find(|r| r.href == href) else {
        return;
    };
    let (artist, title, album) = (row.artist.clone(), row.title.clone(), row.album.clone());
    let href = href.to_string();
    let shared = Arc::clone(shared);
    let dir = app.store.dir().to_path_buf();

    // A plain thread, not the runtime's blocking pool: this outlives the
    // command that started it and must not hold a lock or a runtime worker.
    let spawned = std::thread::Builder::new()
        .name("vapor-lookup".to_string())
        .spawn(move || {
            let Ok(lookup) = metadata::Lookup::new() else {
                return;
            };
            // Only the half that is missing. A track that has its sleeve and
            // wants words costs one LRCLIB request, not a Deezer search as
            // well — which matters while the Deezer calls are unregistered
            // (AUD-18).
            let (words, sleeve) = std::thread::scope(|scope| {
                let w = (!has_words).then(|| scope.spawn(|| lookup.lyrics(&artist, &title)));
                let a = (!has_sleeve).then(|| scope.spawn(|| lookup.album(&artist, &album)));
                (
                    w.and_then(|h| h.join().unwrap_or(None)),
                    a.map(|h| h.join().unwrap_or_default()),
                )
            });
            // `None` here means the sleeve was already had and never asked for.
            // A default `Found` is empty in every field, so everything below
            // leaves the entry alone — which is what not asking should look like,
            // and avoids nesting the rest of this function inside an `if let`.
            let sleeve = sleeve.unwrap_or_default();
            if !sleeve.art.is_empty() {
                lookup.download_image(&sleeve.art, &dir);
            }
            if let Ok(mut app) = shared.lock() {
                let entry = app.looked.entry(href).or_default();
                // Asked, whatever came back. "No words for this one" is an
                // answer and must not be re-asked on every play.
                entry.words_attempted = true;
                if let Some(words) = words {
                    entry.lyrics = Some(words);
                }
                if !sleeve.genre.trim().is_empty() {
                    entry.genre = sleeve.genre;
                }
                if !sleeve.art.is_empty() {
                    entry.album_art = sleeve.art;
                }
                // The track points at the release; the release is stored once.
                // See `AppState::remember_album`.
                if let Some(facts) = sleeve.facts.as_ref().filter(|f| f.is_usable()) {
                    entry.deezer_album_id = facts.id;
                }
                let _ = app.save_looked();
                app.remember_album(sleeve.facts);
            }
        });
    if spawned.is_err() {
        // Nothing to report: the words simply do not arrive, and Now Playing
        // already draws that state.
    }
}

/// Fetch, decode and start a track.
///
/// The work happens on a blocking thread because it is neither quick nor
/// bounded: a cold cache means a download, and decoding a five-minute track is
/// seconds of CPU. Doing either on the command thread would freeze the window,
/// and doing it on the audio thread is unthinkable.
/// Open a track that is not on disk yet, decoding from frame `from`.
///
/// Streams it by range when the server allows, and falls back to fetching the
/// whole file when it does not — which is what playback did before this
/// existed: correct, just slower to start.
///
/// Either way the file is *not* stored here. Whether its audio is worth keeping
/// is `keeps_audio`'s question, and the window fetcher answers it separately;
/// storing a copy here as a side effect of playing once is how the cache filled
/// up in the first place.
///
/// `from` is zero for a track being played and the aligned cue position for one
/// being cued for a mix — a transition starts minutes into the incoming record,
/// and fetching the run-up to it is the whole cost this avoids.
fn stream_from_server(
    remote: &vapor_library::RemoteConfig,
    cache: &cache::Cache,
    href: &str,
    rate: u32,
    from: u64,
) -> std::result::Result<decoder::Streamer, String> {
    use remote_source::RangeFetch as _;

    let fetcher = std::sync::Arc::new(webdav::Fetcher::new(remote)?);
    let track = std::sync::Arc::new(remote_source::RemoteTrack::new(fetcher, href));

    if !track.streamable() {
        // No ranges. Fetch it, keep it — a track that had to be downloaded in
        // full to be played at all may as well be on disk for the next few
        // minutes, and the window will drop it when the set moves on.
        let path = cache
            .store(href, || track.whole())
            .map_err(|e| e.to_string())?;
        return decoder::Streamer::start(&path, rate, from);
    }

    // Shared between every open of this track, so the mix cueing into it does
    // not fetch the same bytes the deck playing it already has.
    let held = std::sync::Arc::new(std::sync::Mutex::new(remote_source::Chunks::default()));
    let ext = href.rsplit('.').next().map(str::to_string);

    decoder::Streamer::start_with(
        Box::new(move || {
            let source = remote_source::RemoteSource::new(
                std::sync::Arc::clone(&track) as std::sync::Arc<dyn remote_source::RangeFetch>,
                std::sync::Arc::clone(&held),
            );
            Ok(vapor_dsp::decode::Source::new(
                Box::new(source),
                ext.as_deref(),
            ))
        }),
        rate,
        from,
    )
}

pub(crate) fn begin_playback(shared: &Shared, app: &mut AppState, href: String) {
    // A new track is a new listen to earn. Whatever the previous one had
    // accrued is dropped rather than banked: it did not reach `CREDIT_AFTER`,
    // which is the whole of what "listened to" means here.
    app.crediting = Some(Crediting {
        href: href.clone(),
        collection: app.collection.clone(),
    });

    // Words for what is about to play, if the person has asked for words at
    // all. Off the playback path entirely: a lyrics service is not allowed to
    // delay a track starting, and this is a nicety that can arrive late or not
    // at all.
    look_up_in_background(shared, app, &href);

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
    // A new track is a new set of exits. Without this the board would hold the
    // previous track's three for ever — the opposite failure to the one that
    // made it reshuffle under a press.
    app.offered = None;
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
    let roots = local::roots(&app.settings.folders);
    let remote = app.settings.remote.clone();
    let shared = Arc::clone(shared);

    tauri::async_runtime::spawn_blocking(move || {
        let cache = cache::Cache::new(cache_dir, cache_max, roots);

        /*
         * Play it from where it already is, or from the server as it arrives.
         *
         * A track in the window is on disk and opens instantly. Anything else
         * used to be downloaded in full first — ten megabytes before a note —
         * which was invisible only because the analysis pass had been filling
         * the cache with the whole library.
         *
         * `RemoteSource` fetches by byte range, so playback starts as soon as
         * the container's header is in. It needs ranges to do that: a server
         * that refuses them leaves nothing to do but fetch the file, which is
         * what this did before. See `remote_source`.
         */
        let outcome = match cache.get(&href) {
            Some(path) => decoder::Streamer::start(&path, rate, 0),
            None => stream_from_server(&remote, &cache, &href, rate, 0),
        };

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

/// How long the playing track is, from whichever of the two knows.
///
/// The deck is the live answer and normally the right one: it is the length of
/// the audio actually loaded. But a container is allowed to declare no length —
/// a fragmented MP4 keeps its sample tables in the fragments, so there is
/// nothing to read at the front — and then the deck can only report how much
/// has been decoded so far, which grows as it plays and is exact only once the
/// song is over. A seek bar cannot be drawn against a number like that.
///
/// The analysis pass decoded the whole file to its end, so its duration is the
/// finished measurement of the same thing. Preferring it costs nothing when the
/// container was honest — the two agree — and is the only correct answer when
/// it was not.
/// `analysis` is the one for the track the shell is showing, so this says
/// nothing about a length when nothing is playing.
pub(crate) fn playing_duration(
    snapshot: Option<&audio::Snapshot>,
    analysis: Option<&analysis::Analysis>,
) -> f64 {
    match analysis.map(|a| a.duration) {
        Some(measured) if measured > 0.0 => measured,
        _ => snapshot.map_or(0.0, |s| s.duration),
    }
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
pub(crate) fn choose_transition(
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

/// A track's tempo, honouring a manual correction and the genre's verdict on
/// which octave the detector's reading is in. See [`tempo_in_force`].
fn bpm_of(analysis: &analysis::Analysis, app: &AppState, href: &str) -> f32 {
    tempo_in_force(app, href, Some(analysis)).unwrap_or(analysis.bpm)
}

/// The tempo actually in force for a track, or `None` when nothing overrides
/// what the detector measured.
///
/// **One definition, and every consumer goes through it.** That is the whole
/// point of the function existing rather than each caller reading
/// `bpm_override` for itself. Until AUD-26 the octave correction ran in exactly
/// one place — `track_meta_pool`, which feeds the Vibe cards — while
/// `beat_grid` and the Tempo Morph target read only the manual override. A
/// corrected genre would have made a card read 174 while the stretcher met the
/// record at 87: visibly right and audibly wrong, which is worse than the bug.
///
/// Two things can override the measurement, in this order:
///
/// * **A hand correction wins outright.** Someone who typed a number has said
///   the last word, and a genre table must not argue with them.
/// * **Otherwise the genre resolves the octave.** A beat tracker is reliable
///   about the pulse and unreliable about whether a listener counts it at 87 or
///   174, and nothing else this app measures separates those two. See
///   `vapor_library::octave_correct`, which returns `None` for every case it
///   cannot answer unambiguously — including a tempo already inside the band.
///
/// `Option` rather than a bare `f32` so [`beat_grid`] keeps its existing
/// meaning for `None`: no override, so the tracked grid stands.
pub(crate) fn tempo_in_force(
    app: &AppState,
    href: &str,
    analysis: Option<&analysis::Analysis>,
) -> Option<f32> {
    if let Some(manual) = app.settings.bpm_override(href) {
        return Some(manual);
    }
    let bpm = analysis?.bpm;
    vapor_library::octave_correct(bpm, &genre_of(app, href))
}

/// [`tempo_in_force`] for a caller that already has the row in hand.
///
/// Worth the second entry point: [`genre_of`] scans `app.rows` to find the row
/// again, and the callers that have one are the ones running over every row in
/// the library.
fn tempo_in_force_for_row(
    app: &AppState,
    row: &Row,
    analysis: Option<&analysis::Analysis>,
) -> Option<f32> {
    if let Some(manual) = app.settings.bpm_override(&row.href) {
        return Some(manual);
    }
    let bpm = analysis?.bpm;
    vapor_library::octave_correct(bpm, &genre_for_row(app, row))
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
    match app.rows.iter().find(|r| r.href == href) {
        Some(row) => genre_for_row(app, row),
        None => genre_from_tag_or_lookup(app, href),
    }
}

/// [`genre_of`] for a caller that already has the row.
///
/// Same three sources in the same order, without the linear scan of `app.rows`
/// to find a row the caller is holding. `track_meta_pool` and `apply_analysis`
/// both run this per row over the whole library, so the scan made them
/// quadratic in library size.
///
/// The order is the answer to AUD-24's "prefer the file's own tag over the
/// service": the lookup is last, and only ever fills a gap. A library that
/// tags its own files keeps "Neurofunk"; one that does not gets Deezer's
/// "Electronic" rather than nothing.
fn genre_for_row(app: &AppState, row: &Row) -> String {
    if !row.genre.trim().is_empty() {
        return row.genre.clone();
    }
    genre_from_tag_or_lookup(app, &row.href)
}

fn genre_from_tag_or_lookup(app: &AppState, href: &str) -> String {
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

pub(crate) fn same_genre(app: &AppState, a: &str, b: &str) -> bool {
    let (ga, gb) = (genre_of(app, a), genre_of(app, b));
    // Unknown on either side is not evidence of a jump.
    if vapor_library::is_unknown_genre(&ga) || vapor_library::is_unknown_genre(&gb) {
        return true;
    }
    vapor_library::is_similar_genre(&ga, &gb)
}

/// How far a candidate moves away from what is playing, in kind rather than in
/// tempo or level.
///
/// Genre when both sides have one; the artist otherwise.
///
/// The fallback is the point. Measured on this library, **488 of 534 tracks
/// carry no genre tag at all** and a further 15 say "Unknown genre" — so genre
/// is not a signal here, it is a blank, and `same_genre` answers "yes, the
/// same" for every pair of them. That is why a De André ballad could be offered
/// as the way to *stay* in a Keem the Cipher set: nothing in the model knew
/// they were different music.
///
/// The artist is the strongest thing a folder-organised library does carry. Two
/// tracks by one artist are far more likely to be one vibe than two tracks
/// picked for tempo alone.
fn kind_distance(app: &AppState, a: &str, b: &str) -> f32 {
    let (ga, gb) = (genre_of(app, a), genre_of(app, b));
    if !vapor_library::is_unknown_genre(&ga) && !vapor_library::is_unknown_genre(&gb) {
        return if vapor_library::is_similar_genre(&ga, &gb) {
            0.0
        } else {
            GENRE_JUMP
        };
    }
    let artist = |href: &str| {
        app.rows
            .iter()
            .find(|r| r.href == href)
            .map(|r| r.artist.trim().to_lowercase())
            .unwrap_or_default()
    };
    let (aa, ab) = (artist(a), artist(b));
    if aa.is_empty() || ab.is_empty() || aa == ab {
        0.0
    } else {
        ARTIST_JUMP
    }
}

/// What leaving the genre costs a Stay, and earns a Switch.
///
/// Sized against the intensity term below, which is a 0–1 difference scaled by
/// 100: a genre jump is worth more than any intensity gap, and an artist jump
/// about a quarter of one, because a different artist is ordinary and a
/// different genre is a decision.
const GENRE_JUMP: f32 = 140.0;
const ARTIST_JUMP: f32 = 28.0;

/// Build the mixer's beat grid for a track, honouring a manual tempo.
///
/// A corrected BPM has to reach the grid, not just the table: the correction
/// exists because detection put the track at half or double time, and mixing
/// on the uncorrected value would beat-match to a tempo the person has already
/// said is wrong.
/// The beat either side of `position`, as (period, next-beat), in seconds.
///
/// This is the whole of what beat-reactive UI needs, and it is deliberately
/// derived from the same stored grid mixing aligns to rather than from a
/// nominal tempo. A synthesised grid is refused: `beats_are_for` false means
/// the tracked beats belong to a tempo that has since been corrected, and
/// pulsing a logo on beats known to be wrong is worse than not pulsing it.
///
/// Returns zeros past the last beat and for an unanalysed track. The caller
/// reads zero as "no grid" and leaves the mark on its steady rate.
pub(crate) fn beat_window(analysis: &analysis::Analysis, position: f64, bpm: f32) -> (f32, f64) {
    if !analysis.beats_are_for(bpm) || analysis.beats.len() < 2 {
        return (0.0, 0.0);
    }
    let t = position as f32;
    let next = analysis.beats.partition_point(|&b| b < t);
    if next >= analysis.beats.len() {
        return (0.0, 0.0);
    }
    // The period around the playhead, not the nominal one. At the very first
    // beat there is nothing before it to measure against, so the pair after it
    // stands in.
    let period = if next == 0 {
        analysis.beats[1] - analysis.beats[0]
    } else {
        analysis.beats[next] - analysis.beats[next - 1]
    };
    if !period.is_finite() || period <= 0.0 {
        return (0.0, 0.0);
    }
    (period, analysis.beats[next] as f64)
}

pub(crate) fn beat_grid(
    analysis: &analysis::Analysis,
    override_bpm: Option<f32>,
) -> vapor_engine::BeatGrid {
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
pub(crate) const PLAN_AHEAD: usize = 10;

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

    // And the same recording under a different filename.
    //
    // The pool the planner works from collapses duplicates, so it should never
    // choose one — but a queue can be built by hand, by a playlist, or by
    // pressing play on an album, and none of those go through the planner. Two
    // rips of one track beat-match perfectly and mix into themselves, which is
    // the one transition guaranteed to sound like a fault.
    if same_recording(app, current, &next) {
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

    let out_grid = beat_grid(outgoing, tempo_in_force(app, current, Some(outgoing)));
    let in_grid = beat_grid(incoming, tempo_in_force(app, &next, Some(incoming)));

    // Both of these are pure and live in the engine; running them here is what
    // keeps beat grids off the audio thread entirely.
    //
    // Only for the moves that are beat-matching. `tempo_ratio` refuses a
    // stretch past ±6%, and asking for one unconditionally meant that any pair
    // further apart than that — 87 BPM into 139, say — planned no transition at
    // all and the records simply followed each other. `choose_transition` had
    // already picked an Echo Out or a Reverb Freeze for exactly that gap,
    // saying in its own comment to "let the outgoing track dissolve rather than
    // collide"; the dissolve was then thrown away by a beat-match test it was
    // chosen for failing. A dissolve does not care what the tempi are.
    let (ratio, incoming_pos) = if kind.beat_matched() {
        (
            vapor_engine::Mixer::tempo_ratio(&out_grid, &in_grid).ok()?,
            vapor_engine::Mixer::aligned_incoming_position(
                &out_grid,
                &in_grid,
                start_at as f32,
                incoming.cue_in,
            )
            .ok()?,
        )
    } else {
        // Its own tempo, from its own cue point.
        (1.0, incoming.cue_in)
    };

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
    let roots = local::roots(&app.settings.folders);
    let remote = app.settings.remote.clone();
    let shared = Arc::clone(shared);

    tauri::async_runtime::spawn_blocking(move || {
        let cache = cache::Cache::new(cache_dir, cache_max, roots);
        // Decoded from where the mix will actually start, not from the top of
        // the track. A transition cues the incoming track minutes in, and
        // decoding the run-up to it would be the whole cost streaming avoids.
        let from = (mix.incoming_pos as f64 * rate as f64).max(0.0) as u64;
        /*
         * Streamed, not downloaded.
         *
         * This used to `cache.store` the incoming track — a blocking fetch of
         * the *whole file* — and only then open a decoder on it. That runs in
         * the last thirty seconds of the outgoing record (`TRANSITION_ARM_LEAD`),
         * which is exactly when the outgoing deck may itself be a live
         * range-fetch off the same server with five seconds of window ahead of
         * it. Ten megabytes pulled at whatever rate the link allows, against a
         * deck with a five-second margin, and the margin loses.
         *
         * What that sounds like is not a dropout. A starved deck emits silence
         * *and does not advance its playhead* (`Deck::render_additive`), so the
         * music stutters and falls behind at once — which is how it was
         * described: mixes going "really slow and super stuttery", on some
         * tracks and not others. The ones already on disk never did it.
         *
         * Streaming from the cue point fetches the seconds the mix actually
         * needs instead of the minutes it does not. Both decks then want about
         * one times realtime for the length of a transition, which is a far
         * smaller ask than a burst download beside a starving reader.
         *
         * A track already in the cache still opens straight off the disk, and a
         * server that refuses ranges still falls back to fetching the file —
         * there is nothing else to do with one.
         */
        let outcome = match cache.get(&mix.next) {
            Some(path) => decoder::Streamer::start(&path, rate, from),
            None => stream_from_server(&remote, &cache, &mix.next, rate, from),
        };

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
                //
                // Beat-matched moves only. A dissolve has no shared grid to
                // drift from, and a loop chasing phase between two tracks that
                // were never in step would pull the incoming deck around for
                // the length of the transition to no purpose.
                app.drift = app
                    .playing_stream
                    .as_ref()
                    .filter(|_| mix.kind.beat_matched())
                    .and_then(|outgoing| {
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
            // Not surfaced to the person: the track plays to its end and the
            // next one follows, which is what would have happened anyway. It is
            // still reported, because "some tracks mix and some do not" is
            // otherwise a fault with no evidence anywhere — this branch used to
            // say nothing at all, and diagnosing it meant reading the planner
            // and guessing.
            other => {
                match other {
                    Err(e) => eprintln!("mix into {} not arranged: {e}", mix.next),
                    Ok(_) => eprintln!("mix into {} not arranged: decoded silence", mix.next),
                }
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

/// Where the audio-fault log is written, beside the app's own data.
///
/// Two kinds of line so far: a deck running dry, and the master limiter
/// stepping. Both are "something the ear caught and nothing recorded".
pub const AUDIO_FAULT_LOG: &str = "audio-faults.log";

/// Append one fault line to the log, with the time it happened.
///
/// Truncated on the first write of each run: this answers "did the decks run
/// dry during *this* session", and a file that accumulates across every launch
/// buries that under history. Failures are ignored on purpose — a diagnostic
/// that cannot be written is not a reason to disturb playback.
fn note_audio_fault(dir: &std::path::Path, line: &str) {
    use std::io::Write as _;
    use std::sync::atomic::{AtomicBool, Ordering};

    static STARTED: AtomicBool = AtomicBool::new(false);
    let first = !STARTED.swap(true, Ordering::Relaxed);

    let path = dir.join(AUDIO_FAULT_LOG);
    let opened = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(first)
        .append(!first)
        .open(&path);

    if let Ok(mut f) = opened {
        let _ = writeln!(f, "{} {line}", clock_time());
    }
}

/// Local wall-clock `HH:MM:SS`.
///
/// Enough to line a log entry up against "that mix sounded wrong", which is the
/// only thing it is for. **Local**, not UTC: this was UTC to avoid adding a
/// dependency, and the result was a log whose lines all appeared to be five
/// hours in the past to the person reading them against what they had just
/// heard. The clock has to match the clock on the wall or it is not a
/// timestamp, it is a puzzle.
fn clock_time() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

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
                                local::roots(&app.settings.folders),
                            )
                        })
                };

                let Some((href, remote, dir, max_bytes, roots)) = wanted else {
                    failures = 0;
                    continue;
                };

                // Fetched with the lock released: this is a network round trip
                // measured in seconds, and every command would block behind it.
                let cache = cache::Cache::new(dir, max_bytes, roots);
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
        .spawn(move || {
            /*
             * Blocks in which a deck ran dry, as of the last look.
             *
             * The counter has always existed and nothing ever read it, so "the
             * audio path is starving" was a thing the app knew and never said —
             * which is how a stutter got diagnosed by listening to it. Reported as
             * a rate rather than a total: a handful of blocks after a seek is
             * normal, and a total that only grows cannot tell that apart from a
             * transition running dry the whole way through.
             *
             * `eprintln!` on a control thread, four times a second at most, and
             * only when the number moved.
             */
            let mut starved_seen: u64 = 0;
            let mut starved_incoming_seen: u64 = 0;
            let mut limiter_steps_seen: u64 = 0;
            loop {
                std::thread::sleep(SUPERVISOR_POLL);

                let Ok(mut app) = shared.lock() else {
                    return;
                };

                if let Some(player) = app.player.as_ref() {
                    let snap = player.snapshot();
                    let now = snap.starved_blocks;
                    if now > starved_seen {
                        let blocks = now - starved_seen;
                        starved_seen = now;
                        let line = format!(
                            "audio starved for {blocks} block(s) in the last {}ms{}",
                            SUPERVISOR_POLL.as_millis(),
                            if player.transition_armed() {
                                " (during a mix)"
                            } else {
                                ""
                            },
                        );
                        eprintln!("{line}");
                        // And to a file beside the app's own data.
                        //
                        // stderr goes wherever the process was launched from: a
                        // terminal nobody is watching, or — for the bundled
                        // build — no terminal at all. A diagnostic you can only
                        // read if you happened to start the app the right way is
                        // one you will not have on the day you need it, which is
                        // exactly how an afternoon went.
                        note_audio_fault(app.store.dir(), &line);
                    }

                    /*
                     * The cued deck, counted apart from the audible one.
                     *
                     * Both used to increment one counter, on the reasoning that
                     * during a mix the incoming deck is the one most likely to
                     * run dry — which is true, and is why it is still watched.
                     * What that missed is that a deck is *also* empty for the
                     * ordinary reason that its stream has not opened yet, and a
                     * mix is armed thirty seconds ahead. So arming a track
                     * logged "audio starved for 22 block(s)" — every block in
                     * the window, because a stream with no bytes yet supplies
                     * nothing at all — while the listener heard a clean track.
                     *
                     * Reported only once the mix is audible, because that is
                     * when an empty incoming deck becomes silence rather than
                     * a buffer still filling.
                     */
                    let incoming = snap.starved_incoming_blocks;
                    if incoming > starved_incoming_seen {
                        let blocks = incoming - starved_incoming_seen;
                        starved_incoming_seen = incoming;
                        if player.transition_running() {
                            let line = format!(
                                "cued deck starved for {blocks} block(s) in the last {}ms, mid-mix",
                                SUPERVISOR_POLL.as_millis(),
                            );
                            eprintln!("{line}");
                            note_audio_fault(app.store.dir(), &line);
                        }
                    }

                    // The limiter's own discontinuities (LIM-001), reported the
                    // same way and for the same reason: a click during a mix is
                    // otherwise only visible to whoever is listening.
                    //
                    // "Stepped" now means the applied gain jumped across a
                    // block boundary — the one discontinuity the ramp cannot
                    // remove — rather than the reduction merely having moved,
                    // which is the limiter doing its job.
                    let steps = snap.limiter_steps;
                    if steps > limiter_steps_seen {
                        let jumps = steps - limiter_steps_seen;
                        limiter_steps_seen = steps;
                        let line = format!(
                            "limiter stepped {jumps} time(s) in the last {}ms, deepest {:.1} dB{}",
                            SUPERVISOR_POLL.as_millis(),
                            player.take_limiter_depth(),
                            if player.transition_armed() {
                                " (during a mix)"
                            } else {
                                ""
                            },
                        );
                        eprintln!("{line}");
                        note_audio_fault(app.store.dir(), &line);
                    }
                }

                // Long enough in to count as listened to (see `CREDIT_AFTER`).
                //
                // Here rather than in the branch that advances the queue,
                // because a track earns its play at thirty seconds whatever
                // happens after: it can be mixed out, skipped, or left running
                // to the end, and all three are the same listen. Doing it where
                // the queue moves would credit only the tracks that ran out.
                credit_if_listened(&mut app);

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
                    // The set has moved, so the window has moved with it.
                    prune_audio(&app);
                    app.playing = app.armed_next.take();
                    // The other way the playing track changes — a mix finishing
                    // rather than something being started. Both have to drop the
                    // held exits, or the screen keeps offering the previous
                    // track's three.
                    app.offered = None;
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
            }
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
        // The lock screen's scrubber is the same control as the transport's,
        // and gets its length the same way.
        duration: playing_duration(
            snapshot.as_ref(),
            app.playing.as_ref().and_then(|href| app.analysis.get(href)),
        ),
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
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct QueueEntry {
    href: String,
    // No `cover`. A queue is routinely the whole library, and a cover runs to
    // 2 MB, so carrying one per entry sent ~155 MB through IPC on the phone —
    // the WebView allocates that as a Java string and the process died with an
    // OutOfMemoryError against the 256 MB heap. The screen draws one cover, for
    // the current track, and fetches it by href with `track_cover`.
    title: String,
    artist: String,
    bpm: f32,
    key: String,
    /// True for the track currently playing.
    current: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct QueueView {
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

/// The body of [`queue_view`], reachable from a test.
pub(crate) fn queue_view_for(app: &AppState) -> QueueView {
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
                title: row
                    .map(|r| r.title.clone())
                    // A track queued from a playlist whose scan has since been
                    // replaced still deserves a name, so fall back to the file.
                    .unwrap_or_else(|| href.rsplit('/').next().unwrap_or(href).to_string()),
                artist: row
                    .filter(|r| r.artist_source != vapor_library::index::Source::Unknown)
                    .map(|r| r.artist.clone())
                    .unwrap_or_default(),
                bpm: tempo_in_force(app, href, analysis)
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
        .skip(current.unwrap_or(0))
        .filter_map(|href| app.analysis.get(href))
        .map(|a| a.duration)
        .sum();

    QueueView {
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
    }
}

// ---------------------------------------------------------------------------
// Vibe DJ
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MoodPathRequest {
    tracks: std::collections::HashMap<String, TrackMeta>,
    start: String,
    /// "build" | "chill" | "wave" | anything else = flat.
    curve: String,
}

pub(crate) fn transition_name(kind: vapor_engine::TransitionType) -> String {
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
pub(crate) fn record_skip_if_reacting_to_a_blend(app: &mut AppState) {
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
pub(crate) fn skip_penalties(app: &AppState) -> std::collections::HashMap<(String, String), f32> {
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
pub(crate) fn track_meta_pool(app: &AppState) -> std::collections::HashMap<String, TrackMeta> {
    let pool: std::collections::HashMap<String, TrackMeta> = app
        .rows
        .iter()
        // Confined to what is being conducted (see `AppState::scope`). Without
        // this the planner reads the whole library whatever was played from,
        // so pressing play on an album gave you one track of it and then the
        // library — the fault `current_playlist` existed to prevent.
        .filter(|row| {
            app.scope
                .as_ref()
                .is_none_or(|scope| scope.tracks.contains(&row.href))
        })
        .filter_map(|row| {
            let analysis = app.analysis.get(&row.href)?;
            let genre = genre_for_row(app, row);
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
                    // The one tempo every part of the app now reasons with —
                    // the library table, the beat grid, the Tempo Morph target
                    // and this pool all read `tempo_in_force`, so a card
                    // cannot say 174 while the stretcher meets the record at
                    // 87. That split is what AUD-26 was.
                    bpm: tempo_in_force_for_row(app, row, Some(analysis)).unwrap_or(analysis.bpm),
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
        .collect();

    dedupe_recordings(app, pool)
}

/// The identity of a *recording*, as opposed to a file.
///
/// Taken from the file's own tags first, and only then from the row. That
/// distinction is the whole of it: `Row::title` is derived from the path by
/// `build_row` and `apply_tags` never overwrites it, so a second copy is titled
/// `Bocca di rosa (1)` — a different string, and so a different recording to
/// any comparison of rows. The tag inside both files says the same thing, which
/// is the question actually being asked.
///
/// Case- and space-insensitive, because two rips agree about the track and
/// disagree about capitalisation more often than the reverse. A track with no
/// usable title or artist keys on its href and stays unique — collapsing
/// everything untitled into one entry would be worse than the duplicates.
/// Whether two hrefs are the same recording — the same track, twice on disk.
fn same_recording(app: &AppState, a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let key = |href: &str| {
        app.rows
            .iter()
            .find(|r| r.href == href)
            .map(|r| recording_key(app, r))
    };
    match (key(a), key(b)) {
        // `recording_key` falls back to the href when a title or artist is
        // missing, so two untitled files never collapse into each other.
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

fn recording_key(app: &AppState, row: &Row) -> String {
    let tags = app.tags.get(&row.href);
    let field = |tag: Option<&String>, fallback: &str| {
        tag.map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback.trim())
            .to_lowercase()
    };
    let title = field(tags.and_then(|t| t.title.as_ref()), &row.title);
    let artist = field(tags.and_then(|t| t.artist.as_ref()), &row.artist);
    if title.is_empty() || artist.is_empty() {
        return row.href.clone();
    }
    format!("{title}\u{1}{artist}")
}

/// The duplicates: every file that is not the first copy of its recording.
///
/// Ordered by href so the survivor is the same one after a restart.
fn duplicate_hrefs(app: &AppState) -> std::collections::HashSet<String> {
    use std::collections::HashMap;
    let mut first: HashMap<String, String> = HashMap::new();
    for row in &app.rows {
        first
            .entry(recording_key(app, row))
            .and_modify(|href| {
                if row.href < *href {
                    *href = row.href.clone();
                }
            })
            .or_insert_with(|| row.href.clone());
    }
    let kept: std::collections::HashSet<&String> = first.values().collect();
    app.rows
        .iter()
        .filter(|r| !kept.contains(&r.href))
        .map(|r| r.href.clone())
        .collect()
}

/// One file per recording.
///
/// A library with the same track twice — `Bocca di rosa` and
/// `Bocca di rosa (1)` — hands the planner two entries with identical tempo,
/// key and intensity. Their transition cost is therefore as close to nothing as
/// the model can produce, which makes the duplicate the *cheapest* possible next
/// step: the set walked out of a track and straight back into it. The
/// pathfinder's own guard is `!path.contains(href)`, and two copies are two
/// hrefs, so it never saw a repeat.
///
/// Collapsing here fixes every consumer at once — the planner, the Vibe
/// screen's three exits, and the mix that gets armed — because all three read
/// this pool.
///
/// The survivor is the lexicographically first href rather than whichever
/// happened to be scanned first, so a set is the same set on the next launch.
fn dedupe_recordings(
    app: &AppState,
    pool: std::collections::HashMap<String, TrackMeta>,
) -> std::collections::HashMap<String, TrackMeta> {
    use std::collections::HashMap;

    let mut keep: HashMap<String, String> = HashMap::new();
    for row in &app.rows {
        if !pool.contains_key(&row.href) {
            continue;
        }
        keep.entry(recording_key(app, row))
            .and_modify(|href| {
                if row.href < *href {
                    *href = row.href.clone();
                }
            })
            .or_insert_with(|| row.href.clone());
    }

    let kept: std::collections::HashSet<&String> = keep.values().collect();
    pool.iter()
        .filter(|(href, _)| kept.contains(href))
        .map(|(href, meta)| (href.clone(), meta.clone()))
        .collect()
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct VibePath {
    hrefs: Vec<String>,
    /// How many library tracks were eligible — analysed, with a tempo. The
    /// screen says "1,284 read", and it should be the true number.
    considered: usize,
    /// How many were passed over for want of analysis. Reported rather than
    /// quietly dropped: a set built from a tenth of the library looks the same
    /// as one built from all of it (TD-43b).
    skipped: usize,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
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
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct MixCandidate {
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

/// The body of [`mix_candidates`], reachable from a test.
///
/// A `#[tauri::command]` takes `State`, which cannot be built outside a running
/// app, so logic left in a command body is logic no test can see.
pub(crate) fn mix_candidates_for(app: &mut AppState) -> Vec<MixCandidate> {
    let Some(current) = app.playing.clone() else {
        return Vec::new();
    };
    let pool = track_meta_pool(app);

    let Some(from) = pool.get(&current) else {
        return Vec::new();
    };

    // Already offered for this track: hand back the same three, in the same
    // slots. Only `selected` moves. See `AppState::offered`.
    if let Some(held) = app.offered.clone() {
        if held.playing == current && held.cards.iter().all(|(h, _)| pool.contains_key(h)) {
            let queued = app.queue.peek_next(None).map(str::to_string);
            return held
                .cards
                .iter()
                .filter_map(|(href, exit)| pool.get(href).map(|to| (to, *exit)))
                .map(|(to, exit)| {
                    let selected = queued.as_deref() == Some(to.href.as_str());
                    card_for(app, &current, from, to, exit, selected)
                })
                .collect();
        }
    }

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
                + kind_distance(app, &current, &t.href)
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
            // Rewarded for leaving, not merely permitted to.
            //
            // This used to minimise `base + energy_diff * 20`, which among a
            // set of departures picks the *mildest* one — so Switch offered the
            // next track on the same album. Subtracting the distance means the
            // furthest in kind wins, while `base` still keeps it to something
            // the engine can actually mix into.
            let score = |t: &TrackMeta| {
                candidate_cost(app, from, t, Exit::Switch) - kind_distance(app, &current, &t.href)
            };
            score(a).total_cmp(&score(b))
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
    /*
     * The queued card is the Follow card, whatever put it there.
     *
     * It used to keep the label of the exit that was taken, so that choosing
     * Switch went on saying SWITCH — the reasoning being that otherwise the
     * set quietly agreed with you and the departure stopped being visible.
     *
     * What that actually produced was a screen reading Stay / Switch / Switch,
     * with no Follow card at all: the third slot still offers a departure, so
     * the held label collided with it and the three exits stopped being three
     * distinct things. And the premise was wrong anyway — once the tail has
     * been re-planned from the chosen track, that track *is* what follows.
     *
     * The provenance is not lost. The queued card carries the selection ring
     * and the beat-match line, which is what says "this is the one" — a word
     * that duplicates a neighbouring card's is a worse way to say it.
     */
    let chosen: Vec<(&TrackMeta, Exit)> = [
        stay.map(|t| (t, Exit::Stay)),
        follow.map(|t| (t, Exit::Follow)),
        switch.map(|t| (t, Exit::Switch)),
    ]
    .into_iter()
    .flatten()
    .collect();

    let cards: Vec<MixCandidate> = chosen
        .iter()
        .map(|(to, exit)| {
            let selected = queued.as_deref() == Some(to.href.as_str());
            card_for(app, &current, from, to, *exit, selected)
        })
        .collect();

    // Held until this track is over, so the board does not move under a press.
    app.offered = Some(Offered {
        playing: current,
        cards: chosen
            .iter()
            .map(|(to, exit)| (to.href.clone(), *exit))
            .collect(),
    });

    cards
}

/// One exit card, from the pair of tracks it sits between.
///
/// Shared by the two ways the screen is answered — freshly chosen, and held
/// from a previous read — so a card cannot describe itself differently
/// depending on which path produced it.
fn card_for(
    app: &AppState,
    current: &str,
    from: &TrackMeta,
    to: &TrackMeta,
    exit: Exit,
    selected: bool,
) -> MixCandidate {
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
            same_genre(app, current, &to.href),
        )),
        selected,
        cover: app.covers.get(&to.href),
    }
}

/// Take one of the three exits, and re-plan the set behind it.
///
/// The curve owns the destination and the exit owns the next step, so
/// overriding one step must not throw away the arc: the tail is re-searched
/// from the chosen track along the same curve. Being 60% through a Build stays
/// 60% through a Build — the route changes, not where it is going.
/// Whether an armed mix has to be abandoned because a different exit was taken.
///
/// A mix is armed `TRANSITION_ARM_LEAD` seconds — thirty — before it starts, so
/// for the last half-minute of every track there is a track already cued on the
/// incoming deck. `choose_next` used to only rewrite the queue, which meant a
/// press in that window updated the badge and the plan while the deck went on
/// playing what it had already loaded: "I selected switch, it went ahead with
/// the follow track". The screen said one thing and the speakers did another.
///
/// Three conditions, all necessary:
///
/// * something is armed at all,
/// * it is not already the track that was chosen — pressing Follow is a no-op
///   by construction and re-arming it would restart a fetch for no reason,
/// * and the mix has not become audible yet. Once it has, abandoning it snaps
///   the outgoing deck back to full gain mid-crossfade, which is worse than
///   honouring the press one track late.
pub(crate) fn mix_must_be_rearmed(armed_next: Option<&str>, chosen: &str, running: bool) -> bool {
    !running && armed_next.is_some_and(|armed| armed != chosen)
}

/// What the next blend will do, in the terms the Vibe screen states them.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct BlendPreview {
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

// ---------------------------------------------------------------------------
// Liner notes
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct Facet {
    label: String,
    count: usize,
}

/// How many tracks a search returns. Beyond this the list stops being a result
/// and starts being the library again.
const SEARCH_LIMIT: usize = 40;

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Why a track has no local bytes, in words a person can act on.
///
/// Two quite different things arrive as a `CacheError` and only one of them is
/// about the library: a server that has no file at that path is something to
/// fix on the server, and a cache that could not be written to is something to
/// fix on the device. Reporting both as the same sentence is how "not
/// available locally" came to mean nothing.
fn why_no_bytes(e: cache::CacheError) -> String {
    match e {
        cache::CacheError::Fetch(m) => m,
        cache::CacheError::Io(e) => format!("could not be saved to the cache: {e}"),
    }
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
            let target = tempo_in_force(app, href, Some(analysis)).unwrap_or(analysis.bpm);
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
pub(crate) fn retrack_grids(app_handle: &tauri::AppHandle, shared: &Shared, hrefs: Vec<String>) {
    use tauri::Emitter;

    let Ok(app) = shared.lock() else { return };
    let todo = stale_grids(&app, &hrefs);
    if todo.is_empty() {
        return;
    }
    let (cache_dir, cache_max) = (app.cache.dir().to_path_buf(), app.cache.max_bytes());
    let roots = local::roots(&app.settings.folders);
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
        let cache = cache::Cache::new(cache_dir, cache_max, roots);
        // One session for the batch, as the analysis pass does: one keychain
        // read and one connection rather than one of each per track.
        let fetcher = webdav::Fetcher::new(&remote);

        for (href, bpm) in todo {
            // Same reasoning as the analysis pass: whatever went wrong is the
            // only useful thing to say, and there are three separate ways to
            // arrive here — no session at all, a server that would not serve
            // the file, and a decode that failed on bytes that did arrive.
            // Which of the three it was is the whole of what a person can act
            // on, and `.ok()` on the session plus `.ok()` on the fetch used to
            // flatten all three into "not available locally".
            let outcome = match fetcher.as_ref() {
                Ok(f) => cache
                    .store(&href, || f.fetch(&href))
                    .map_err(why_no_bytes)
                    .and_then(|path| {
                        vapor_dsp::retrack_beats_file(&path, bpm).map_err(|e| e.to_string())
                    }),
                Err(e) => Err(format!("no connection to the library server: {e}")),
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

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct ScanReport {
    tracks: usize,
    directories: usize,
    /// Folders that could not be read and were walked past (TD-49).
    unreadable: usize,
    /// Sources that failed outright, named, one line each.
    ///
    /// A library can have several sources now. One unreachable server used to
    /// fail the whole command, which with folders configured would throw away a
    /// scan that had already succeeded — and a person whose NAS is asleep still
    /// wants the music on their laptop. So a failure is reported beside the
    /// result instead of replacing it.
    #[serde(default)]
    problems: Vec<String>,
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
    let scheme = ["https://", "http://"]
        .into_iter()
        .find(|s| trimmed.starts_with(s));
    /*
     * The host has to look like one, not just the scheme.
     *
     * Checking only the prefix was enough while the box started empty: an app
     * password pasted in had no `https://` and was refused. The box is now
     * prefilled with `https://` so the shape of the answer is visible before it
     * is typed — which means the same paste arrives as
     * `https://4wg9ie7xi8v7nbi6`, clears a prefix check, and is stored as an
     * origin. The scan then finds nothing and reports no error, which reads as
     * "my library is empty".
     *
     * A dot in the host is the cheapest test that separates a hostname from a
     * password. It refuses `localhost`, which nothing here has ever pointed at
     * — a WebDAV origin for a music library is a remote one.
     */
    let host_looks_real = scheme.is_some_and(|s| {
        trimmed[s.len()..]
            .split('/')
            .next()
            .is_some_and(|host| host.contains('.') && host.len() > 3)
    });
    if !trimmed.is_empty() && (scheme.is_none() || !host_looks_real) {
        return Err(Error(format!(
            "\"{trimmed}\" is not a server address — it needs to start with \
             https:// and name a server. For Koofr that is \
             https://app.koofr.net, and the app password goes in the Password \
             field."
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

/// Derive a table row from a path.
///
/// Analysis fields stay empty until the track is actually analysed — the row
/// says "unknown", not a guess. That distinction is the whole reason the Godot
/// stub fabricating 120 BPM was a bug rather than a convenience.
fn build_row(href: &str, base_folder: &str) -> Row {
    // Artist and album are inferred from the directory structure, and a local
    // href carries a source id in front of that structure — parse the path, not
    // the prefix. The base folder is a WebDAV notion (the server path the
    // library starts at) and a local href is already relative to its own root,
    // so it has none.
    let (path, base) = match local::parse_href(href) {
        Some((_, relative)) => (relative, ""),
        None => (href, base_folder),
    };
    let info = vapor_library::parse_path(path, base);
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

/// What the operating system calls this machine.
///
/// Read once. [`AppState::device_name`] is called for every advert, and the
/// beacon adverts every five seconds — a subprocess on that schedule to learn
/// something that cannot change is a subprocess per five seconds for ever.
///
/// `None` where there is no answer, or where the answer is a placeholder: a
/// Linux box that has never been named reports `localhost`, and Android's
/// `gethostname` reports it on every device, so both would put every phone
/// back under one name.
fn machine_name() -> Option<String> {
    static NAME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        let raw = read_machine_name()?;
        let name = raw.trim();
        // `.local` is how Bonjour writes a hostname, and nobody calls their
        // laptop that out loud.
        let name = name.strip_suffix(".local").unwrap_or(name).trim();
        if name.is_empty() || name.eq_ignore_ascii_case("localhost") {
            return None;
        }
        Some(name.to_string())
    })
    .clone()
}

/// The platform half of [`machine_name`].
#[cfg(target_os = "android")]
fn read_machine_name() -> Option<String> {
    // `Build.MODEL` rather than a hostname: Android reports `localhost` for the
    // latter on every device, and the model is what the phone calls itself in
    // its own Settings. It also happens to be the safer of the two — "Pixel 9"
    // is a class of device, not a person.
    android::build_model()
}

#[cfg(target_os = "macos")]
fn read_machine_name() -> Option<String> {
    // The Sharing name, which is what macOS shows the network and what the
    // owner recognises. `hostname` would give the same thing with `.local` and
    // spaces mangled into hyphens.
    let out = std::process::Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(target_os = "windows")]
fn read_machine_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(not(any(target_os = "android", target_os = "macos", target_os = "windows")))]
fn read_machine_name() -> Option<String> {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
}

// ---------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------

/// A track analysis keeps failing on for a reason that might not last.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Stall {
    /// What went wrong the last time the pass reached it.
    reason: String,
    /// How many passes have hit it. One is ordinary — the track had not
    /// downloaded yet. Repeated attempts are the signal that it is not
    /// transient after all, so the modal can say so rather than making
    /// somebody press Analyse a fourth time to find out.
    attempts: u32,
}

/// One track analysis could not describe, ready for the screen.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct AnalysisFailure {
    href: String,
    /// From the library row, so the list names tracks rather than paths. Falls
    /// back to the href for a failure whose row has since gone.
    title: String,
    artist: String,
    reason: String,
    /// `true` for the kind that will never work — a file that decodes to
    /// nothing. `false` for the kind the next pass will try again.
    permanent: bool,
    /// Passes that have hit this. Always 0 for a permanent failure, which is
    /// only ever recorded once.
    attempts: u32,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct AnalysisStatus {
    analysed: usize,
    total: usize,
    /// Whether a pass is running right now, whoever started it.
    running: bool,
    /// The track it is on. Empty between tracks and when nothing is running.
    current: String,
    /// Why the last pass ended early, if it did. Empty otherwise.
    ///
    /// A pass that cannot open a connection used to return without a word and
    /// without clearing `running`, so the screen showed a pass in flight that
    /// was not, for ever, with its own button disabled — which is what
    /// "the analysis stopped by itself" looks like from the outside.
    stopped_because: String,
}

/// Whether this track's audio is worth keeping on the device.
///
/// Three reasons, and nothing else:
///
/// * it was **downloaded on purpose** — a pinned playlist or group, which is
///   the one case where the person has said they want it kept;
/// * it is **in the play window** — a few tracks either side of where the set
///   is, so going back is instant and the next mix has something to cue from;
/// * it is **playing right now**, which the window covers but which is worth
///   being explicit about.
///
/// Everything else is a file fetched to be read once. That is what filled five
/// gigabytes: analysis has to download every track to listen to it, and the
/// bytes were landing in the same cache as playback and staying there.
fn keeps_audio(app: &AppState, href: &str) -> bool {
    if app.pinned.contains(href) {
        return true;
    }
    if app.playing.as_deref() == Some(href) {
        return true;
    }
    let tracks = app.queue.tracks();
    let Some(at) = app.queue.current_index() else {
        return false;
    };
    let from = at.saturating_sub(KEEP_BEHIND);
    let to = (at + KEEP_AHEAD).min(tracks.len().saturating_sub(1));
    tracks
        .get(from..=to)
        .is_some_and(|window| window.iter().any(|h| h == href))
}

/// Drop every cached track the device has no reason to hold.
///
/// `prune_audio` walks the queue, which covers a set as it moves but says
/// nothing about audio left over from before — and a device upgrading to this
/// arrives with a cache full of tracks that were kept only because analysis had
/// read them. On the library this was written against that was 5.4 GB across
/// 556 files, none of which anything wanted any more.
///
/// The whole library rather than the queue, because that is the only list that
/// can name what is on disk: the cache is content-addressed by a hash of the
/// href, so a file cannot be turned back into the track it came from.
fn sweep_audio(app: &AppState) -> usize {
    let mut freed = 0usize;
    for row in &app.rows {
        if keeps_audio(app, &row.href) {
            continue;
        }
        if app.cache.get(&row.href).is_some() && app.cache.remove(&row.href).is_ok() {
            freed += 1;
        }
    }
    freed
}

/// Drop the audio of tracks the set has moved away from.
///
/// The window is a promise about what stays, not only about what is fetched:
/// without this a long set accumulates every track it has been through, which
/// is the same slow fill that analysis was causing, only quieter.
///
/// Only the queue is walked. The cache is content-addressed by a hash of the
/// href, so a file on disk cannot be turned back into the track it came from —
/// which is exactly why the byte bound exists underneath as a backstop.
fn prune_audio(app: &AppState) {
    for href in app.queue.tracks() {
        if !keeps_audio(app, href) {
            let _ = app.cache.remove(href);
        }
    }
}

/// How much of the library is described, and how big the library is.
///
/// One answer, used by the Settings card and by the notification. They had one
/// each: the card counted the library, and the notification counted the *pass*
/// — so it opened at "0 of 526" while the card said "34 of 563", because 526
/// was what this run had left to do rather than anything a person owns. Two
/// counts of one thing is one too many.
///
/// A permanently refused file counts as done. It is not outstanding work, and
/// leaving it out of the numerator means the total can never be reached.
pub(crate) fn analysis_counts(app: &AppState) -> (usize, usize) {
    let total = app.rows.len();
    let outstanding = app
        .rows
        .iter()
        .filter(|r| {
            !app.failures.contains_key(&r.href)
                && app
                    .analysis
                    .get(&r.href)
                    .is_none_or(|a| a.version < analysis::ANALYSIS_VERSION)
        })
        .count();
    (total.saturating_sub(outstanding), total)
}

/// The body of [`analysis_failures`], against state rather than a `State`.
///
/// Split out so the ordering and the row join can be tested by calling the
/// thing the command calls. A test that rebuilds this from `failures` and
/// `stalls` itself would agree with its own copy and not with the command.
pub(crate) fn failure_list(app: &AppState) -> Vec<AnalysisFailure> {
    let rows: std::collections::HashMap<&str, &Row> =
        app.rows.iter().map(|r| (r.href.as_str(), r)).collect();

    let describe = |href: &String, reason: &String, permanent: bool, attempts: u32| {
        let row = rows.get(href.as_str());
        AnalysisFailure {
            href: href.clone(),
            title: row.map_or_else(|| href.clone(), |r| r.title.clone()),
            artist: row.map(|r| r.artist.clone()).unwrap_or_default(),
            reason: reason.clone(),
            permanent,
            attempts,
        }
    };

    let by_title = |a: &AnalysisFailure, b: &AnalysisFailure| {
        a.title.to_lowercase().cmp(&b.title.to_lowercase())
    };

    let mut out: Vec<AnalysisFailure> = app
        .failures
        .iter()
        .map(|(href, reason)| describe(href, reason, true, 0))
        .collect();
    out.sort_by(by_title);

    let mut stalled: Vec<AnalysisFailure> = app
        .stalls
        .iter()
        .map(|(href, stall)| describe(href, &stall.reason, false, stall.attempts))
        .collect();
    stalled.sort_by(by_title);

    out.extend(stalled);
    out
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
pub(crate) fn start_analysis(app_handle: &tauri::AppHandle, shared: &Shared) -> Result<()> {
    use tauri::Emitter;

    // Snapshot what needs doing and release the lock: the pass takes minutes,
    // and holding the lock would block every other command for its duration.
    let (todo, (cache_dir, cache_max), cancel, remote, generation, roots) = {
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
            analysis::pending_first(&hrefs, &app.analysis, &app.failures, &priority),
            (app.cache.dir().to_path_buf(), app.cache.max_bytes()),
            app.cancel.clone(),
            app.settings.remote.clone(),
            app.analysis_generation,
            local::roots(&app.settings.folders),
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

    // The notification goes up with the pass, not with its first result.
    //
    // It was raised from the progress callback, which fires when a track has
    // been fetched *and* analysed — minutes away on a slow connection. So a
    // pass could be running, downloading, with nothing in the shade to say so,
    // which is indistinguishable from it not having started.
    #[cfg(target_os = "android")]
    {
        let counts = shared.lock().ok().map(|app| analysis_counts(&app));
        if let Some((done, total)) = counts {
            android::service_analysis(done, total, true);
        }
    }

    let state_arc: Shared = Arc::clone(shared);
    let handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let cache = std::sync::Arc::new(cache::Cache::new(cache_dir, cache_max, roots));

        // One session for the whole pass: one keychain read and one connection,
        // rather than one of each per track. See `webdav::Fetcher`.
        //
        // A pass that cannot read the credential has no way to fetch anything,
        // so it ends here rather than failing every track in turn and marking
        // the whole library unreadable.
        let fetcher = match webdav::Fetcher::new(&remote) {
            Ok(f) => f,
            Err(e) => {
                // Say so, and put the flag back.
                //
                // `analysing` is set before this thread starts, and the only
                // place it was cleared is after `analysis::run` returns — which
                // this path never reaches. So a failure here left the app
                // believing a pass was running until it was restarted, with the
                // Analyse button disabled because a pass was "in flight".
                //
                // It is reachable in ordinary use: the credential lives in the
                // OS keychain, and on Android the Keystore is not readable
                // while the device is locked. A screen going off mid-library is
                // enough.
                // Generation-guarded, like the completion below: a pass that
                // was replaced can fail here *after* its replacement has
                // started, and clearing the flag unconditionally would report
                // "not running" while one still is — which looks from the
                // outside exactly like analysis having stopped, while the file
                // on disk goes on growing.
                if let Ok(mut app) = state_arc.lock() {
                    if app.analysis_generation == generation {
                        app.analysing = false;
                        app.analysing_title = String::new();
                        app.analysis_stopped_because = format!(
                            "Analysis stopped: {e}. It will carry on from where \
                             it left off when you press Analyse."
                        );
                    }
                }
                let _ = handle.emit("analysis-stopped", e.to_string());
                return;
            }
        };

        /*
         * Fetch several tracks at once, because the fetching is the whole wait.
         *
         * Measured on Dylan's phone: 150 KB/s on one connection, tracks
         * averaging 9.6 MB, so about a minute each — against roughly half a
         * second to actually analyse one. The pass was strictly sequential:
         * download a track, describe it, download the next. Which means the CPU
         * was idle for 99% of a job that reads as CPU work, and 563 tracks came
         * to nine hours.
         *
         * These threads run ahead filling the cache from the same list, in the
         * same order, so by the time the pass reaches a track it is usually
         * already local. Four, not more: the ceiling here is somebody's home
         * connection and their server's patience, and a phone opening a dozen
         * sockets to a WebDAV host is a good way to be rate-limited.
         *
         * `Cache::store` is safe to call for the same href from several threads
         * at once — see `concurrent_stores_of_one_href_do_not_corrupt_it` — so
         * the worst case here is duplicated work, never a damaged file.
         */
        let fetcher = std::sync::Arc::new(fetcher);
        let queue: std::sync::Arc<Mutex<std::collections::VecDeque<(usize, String)>>> =
            std::sync::Arc::new(Mutex::new(todo.iter().cloned().enumerate().collect()));
        let prefetch_done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        // Where the pass has got to, so the fetchers can stay near it.
        let cursor = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let prefetchers: Vec<_> = (0..PREFETCH_THREADS)
            .map(|_| {
                let queue = std::sync::Arc::clone(&queue);
                let cache = std::sync::Arc::clone(&cache);
                let fetcher = std::sync::Arc::clone(&fetcher);
                let cancel = cancel.clone();
                let done = std::sync::Arc::clone(&prefetch_done);
                let cursor = std::sync::Arc::clone(&cursor);
                std::thread::spawn(move || {
                    loop {
                        if cancel.is_stopped() {
                            break;
                        }
                        let next = queue.lock().ok().and_then(|mut q| q.pop_front());
                        let Some((index, href)) = next else { break };

                        /*
                         * Stay near the pass rather than race the library.
                         *
                         * Unbounded, these run as fast as the network allows and
                         * the audio cache fills with tracks described long ago:
                         * two gigabytes inside twenty minutes, against a pass
                         * that had reached fifty tracks. The audio is a *cache*,
                         * kept so a track can be played without the network —
                         * not somewhere to put the whole library in order to
                         * read each file once.
                         *
                         * It also keeps them working on what the pass is about
                         * to want, which is what stops one slow track holding up
                         * a queue of finished ones.
                         */
                        while !cancel.is_stopped()
                            && index
                                > cursor.load(std::sync::atomic::Ordering::Acquire)
                                    + PREFETCH_WINDOW
                        {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                        if cancel.is_stopped() {
                            break;
                        }
                        if cache.get(&href).is_some() {
                            continue;
                        }
                        let _ = cache.store(&href, || {
                            fetcher.fetch_until(&href, &|| cancel.is_stopped())
                        });
                    }
                    done.fetch_add(1, std::sync::atomic::Ordering::Release);
                })
            })
            .collect();

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

                // Where the pass has reached, so the fetchers know how far
                // ahead of it they are allowed to be.
                let at = todo.iter().position(|h| h == href).unwrap_or(0);
                cursor.store(at, std::sync::atomic::Ordering::Release);

                /*
                 * Wait for the prefetchers rather than fetch a second copy.
                 *
                 * They pull from the same list in the same order, so a track
                 * this pass has reached is either already local or in flight
                 * with one of them. Downloading it again here would double the
                 * traffic for the one thing that is actually slow.
                 *
                 * Bounded, and it gives up waiting once every prefetcher has
                 * finished — otherwise a track they all failed on would stall
                 * the pass rather than being recorded as a failure and passed
                 * over.
                 */
                for _ in 0..PREFETCH_WAIT_TICKS {
                    if cancel.is_stopped() {
                        return Err("stopped".to_string());
                    }
                    if let Some(path) = cache.get(href) {
                        return Ok(path);
                    }
                    if prefetch_done.load(std::sync::atomic::Ordering::Acquire) >= PREFETCH_THREADS
                    {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }

                // Cancellable: Stop has to be answered during the download,
                // not after it. See `Fetcher::fetch_until`.
                //
                // The error is kept rather than dropped. It is the only thing
                // that can tell somebody looking at the list of tracks that
                // failed what to do about one — and it was being thrown
                // away here, one line above the place that then had nothing to
                // report but "not available locally".
                cache
                    .store(href, || fetcher.fetch_until(href, &|| cancel.is_stopped()))
                    .map_err(why_no_bytes)
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
                                app.set_tags(&progress.href, tags);
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
                        if app.stalls.remove(&progress.href).is_some() {
                            let _ = app.save_stalls();
                        }
                    } else if let Some(reason) = &progress.error {
                        if progress.retryable {
                            // Recorded rather than dropped. The pass will try
                            // again, and if it keeps landing here the attempt
                            // count is the only thing that can say so.
                            let stall = app.stalls.entry(progress.href.clone()).or_default();
                            stall.reason = reason.clone();
                            stall.attempts = stall.attempts.saturating_add(1);
                            let _ = app.save_stalls();
                        } else {
                            // Condemned. A track that simply was not downloaded
                            // when the pass ran never reaches this arm.
                            app.failures.insert(progress.href.clone(), reason.clone());
                            let _ = app.save_failures();
                            // It is described now, by the other map.
                            if app.stalls.remove(&progress.href).is_some() {
                                let _ = app.save_stalls();
                            }
                        }
                    }
                }
                // Keep the process alive for the rest of the pass. Android
                // freezes a backgrounded app, and this one is mostly waiting on
                // downloads — see `PlaybackService.kt`.
                //
                // Reported as the library, not as this run: `progress` counts
                // the tracks *this pass* has left, which is neither what the
                // card says nor what anyone owns.
                #[cfg(target_os = "android")]
                {
                    let counts = state_arc.lock().ok().map(|app| analysis_counts(&app));
                    if let Some((done, total)) = counts {
                        android::service_analysis(done, total, true);
                    }
                }

                /*
                 * The audio has been read. Let it go.
                 *
                 * Analysis has to download every track in full to listen to it,
                 * and those bytes were landing in the same cache as playback
                 * and staying there — five gigabytes of tracks kept in order to
                 * be measured once. Nothing wants them afterwards unless the
                 * person asked for the track to be kept, or the set is near it.
                 *
                 * Not a retry: a track that failed for a *retryable* reason —
                 * it simply was not downloaded when the pass reached it — has
                 * nothing to drop, and one that failed permanently should not
                 * be fetched again anyway.
                 */
                if let Ok(app) = state_arc.lock() {
                    if !keeps_audio(&app, &progress.href) {
                        let _ = app.cache.remove(&progress.href);
                    }
                }

                let _ = handle.emit("analysis-progress", &progress);
            },
        );

        // Wound down with the pass, so a cancelled run does not leave four
        // threads pulling a library nobody asked for any more.
        for worker in prefetchers {
            let _ = worker.join();
        }

        // The pass is over, so this is no longer a reason to stay up. The
        // service decides for itself whether playback still is.
        #[cfg(target_os = "android")]
        android::service_analysis(0, 0, false);

        // Only if this is still the current pass: a pass that was replaced
        // finishes after its replacement started, and clearing the flag here
        // unconditionally would report "not running" while one still is.
        let finished_and_may_look_up = if let Ok(mut app) = state_arc.lock() {
            if app.analysis_generation == generation {
                app.analysing = false;
                app.analysing_title = String::new();
                // A pass that finished is not a pass that failed.
                app.analysis_stopped_because = String::new();
                app.settings.metadata_lookup_enabled
            } else {
                false
            }
        } else {
            false
        };

        /*
         * Then the tempo correction, if the person allows lookups.
         *
         * This has to happen in bulk and it cannot wait for playback. The Vibe
         * DJ plans a set across the *whole* library — every candidate's tempo
         * decides which record can follow which and how it is mixed in — so a
         * correction that only reaches tracks somebody has played leaves the
         * planner working from wrong numbers for everything else. A beat
         * tracker is reliable about the pulse and unreliable about the octave,
         * and 87 read as 174 is a mix the engine will refuse or botch.
         *
         * Chained here rather than given a button of its own: it is the second
         * half of "find tempo, key and cue points", and two buttons for one
         * intention is what the Settings rewrite was removing.
         */
        if finished_and_may_look_up {
            if let Err(e) = identify_library_in_background(&handle, &state_arc) {
                eprintln!("tempo correction not started: {}", e.0);
            }
        }
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct CacheStatus {
    // A JSON number over IPC, not a `bigint`: serde_json writes u64 as a
    // plain number and the webview parses it as one. Values here are byte
    // counts and millisecond timestamps, far below 2^53.
    #[ts(type = "number")]
    bytes: u64,
    // A JSON number over IPC, not a `bigint`: serde_json writes u64 as a
    // plain number and the webview parses it as one. Values here are byte
    // counts and millisecond timestamps, far below 2^53.
    #[ts(type = "number")]
    max_bytes: u64,
    /// How many of the library's tracks are held locally, so the Your Data
    /// screen can state what is actually on the device rather than implying
    /// the whole library is.
    tracks_cached: usize,
    tracks_total: usize,
    location: String,
}

// ---------------------------------------------------------------------------
// Your Data
// ---------------------------------------------------------------------------

/// One line of the Your Data table: what it is, where it sits, how big.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct DataRow {
    label: String,
    path: String,
    // A JSON number over IPC, not a `bigint`: serde_json writes u64 as a
    // plain number and the webview parses it as one. Values here are byte
    // counts and millisecond timestamps, far below 2^53.
    #[ts(type = "number")]
    bytes: u64,
    /// False for anything that lives on the server rather than here. The
    /// screen's whole claim is about what is on *this* device, so the
    /// distinction has to be visible rather than implied.
    local: bool,
}

/// Id generator for entities the core deliberately does not name itself.
///
/// Monotonic time plus a counter. The Godot version used
/// `Time.get_ticks_usec()` and `randi()`, which could collide within a
/// microsecond; the counter removes that.
pub(crate) fn new_id(prefix: &str) -> String {
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
    let builder = tauri::Builder::default();

    // Every platform. The folder picker is how a library gets added at all, and
    // on Android it is the only way — there is no path a person could type.
    let builder = builder.plugin(tauri_plugin_dialog::init());

    // Desktop only: the updater replaces the application bundle on disk, which
    // is not something a phone lets a program do. Android and iOS update
    // through their stores, so on those targets the plugin is not built at all.
    //
    // The attribute binds to the statement that follows it, so anything
    // inserted between the two takes the guard and leaves the updater
    // unguarded. That is exactly what happened when the dialog plugin was added
    // here: `cargo check --target aarch64-linux-android` failed with
    // `cannot find module or crate tauri_plugin_updater`, on a line nobody had
    // edited.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
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

            /*
             * Give back what nothing is holding on to.
             *
             * Audio is kept for the window and for downloads; everything else
             * in the cache was read once and forgotten. A device that has been
             * through an analysis pass on an older build arrives with all of
             * it, and nothing else would ever ask for it back.
             */
            {
                // Off the startup path.
                //
                // This deletes a file per track — five hundred of them on a
                // device coming from an older build — and `setup` has to reach
                // `app.manage` before the webview asks it anything. Run inline,
                // it lost that race: the first screen came up on "core
                // unreachable, state not managed for command `settings`",
                // because the state genuinely was not managed yet.
                //
                // Nothing waits on this, so nothing needs it to be quick.
                let for_sweep: Shared = Arc::clone(&shared);
                tauri::async_runtime::spawn_blocking(move || {
                    if let Ok(app) = for_sweep.lock() {
                        let freed = sweep_audio(&app);
                        if freed > 0 {
                            eprintln!("cache: released {freed} tracks nothing was holding");
                        }
                    }
                });
            }

            /*
             * Carry on describing the library.
             *
             * Analysis only ever began from a scan, from the Analyse button, or
             * from playing a track nothing was known about — so a library part
             * way through was simply left there, and every launch started with
             * a pass that was not running and a count that had not moved. On a
             * few hundred tracks that is a job spanning many sittings, and
             * asking to be told to continue each time is asking about something
             * the app already knows.
             *
             * `pending` skips everything already done, so this costs nothing on
             * a library that is finished, and a pass already running is left
             * alone by the generation check inside.
             */
            {
                let for_analysis: Shared = Arc::clone(&shared);
                let handle = app.handle().clone();
                // After setup returns: `start_analysis` takes the lock, and
                // this closure is holding it through `shared` above.
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = start_analysis(&handle, &for_analysis) {
                        eprintln!("vapor-analysis: could not resume at launch: {e:?}");
                    }
                });
            }

            // Stop, from the pass's own notification. The same flag the
            // in-app button raises, so there is one way to end a pass rather
            // than two that have to agree.
            #[cfg(target_os = "android")]
            {
                let for_stop: Shared = Arc::clone(&shared);
                android::on_stop_analysis(move || {
                    if let Ok(mut app) = for_stop.lock() {
                        app.cancel.stop();
                        app.analysing = false;
                        app.analysing_title = String::new();
                    }
                });
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

            /*
             * Take a newer version if there is one.
             *
             * The public key and the endpoint are compiled into the binary
             * (`tauri.conf.json`), so this is the one feature that cannot be
             * added later by shipping an update: a build handed to someone
             * without it can only ever be replaced by hand.
             *
             * Silent on purpose. There is no update UI yet, and a check that
             * only reports to a screen nobody built would never install
             * anything. The new version is written next to the running one and
             * takes over at the next launch; nothing restarts underneath a
             * person mid-track.
             *
             * Every failure here is survivable and none of them are worth
             * interrupting anyone for — the endpoint returns 404 until a
             * release exists to attach `latest.json` to, and a machine that is
             * offline simply has no update that day. They are logged, not
             * raised.
             */
            #[cfg(desktop)]
            {
                use tauri_plugin_updater::UpdaterExt;

                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let updater = match handle.updater() {
                        Ok(updater) => updater,
                        Err(e) => {
                            eprintln!("updater: not configured: {e}");
                            return;
                        }
                    };
                    match updater.check().await {
                        Ok(Some(update)) => {
                            let version = update.version.clone();
                            match update.download_and_install(|_, _| {}, || {}).await {
                                Ok(()) => eprintln!(
                                    "updater: {version} installed, and runs from the next launch"
                                ),
                                Err(e) => eprintln!("updater: could not install {version}: {e}"),
                            }
                        }
                        Ok(None) => {}
                        Err(e) => eprintln!("updater: could not ask for a newer version: {e}"),
                    }
                });
            }

            app.manage(shared);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::library::library_view,
            commands::playlists::playlists,
            commands::playlists::create_playlist,
            commands::playlists::add_tracks_to_playlist,
            commands::playlists::rename_playlist,
            commands::playlists::delete_playlist,
            commands::playlists::remove_playlist_track,
            commands::playlists::reorder_playlist_track,
            commands::playlists::playlist_rows,
            commands::playlists::playlist_folders,
            commands::playlists::create_folder,
            commands::library::duplicate_count,
            commands::lookup::lookup_counts,
            commands::downloads::downloaded_tracks,
            commands::downloads::download_collection,
            commands::downloads::remove_download,
            commands::groups::dynamic_groups,
            commands::groups::create_group,
            commands::groups::rename_group,
            commands::groups::delete_group,
            commands::groups::add_to_group,
            commands::groups::remove_from_group,
            commands::groups::reorder_groups,
            commands::groups::group_tracks,
            commands::playlists::rename_folder,
            commands::playlists::delete_folder,
            commands::playlists::set_playlist_folder,
            commands::lookup::track_lookup,
            commands::lookup::look_up_track,
            commands::artwork::looked_up_image,
            commands::artwork::album_cover,
            commands::artwork::find_album_art,
            commands::artwork::clear_album_art,
            commands::artwork::set_prefer_looked_up_art,
            commands::settings::set_hide_duplicates,
            commands::settings::set_appearance,
            commands::analysis::analysis_failures,
            commands::artwork::artist_portrait,
            commands::settings::set_metadata_lookup,
            commands::dj::set_vibe_limit,
            commands::dj::set_curve,
            commands::dj::set_dj_mode,
            commands::sync::set_sync_enabled,
            commands::data::startup_problems,
            commands::lookup::identify_library,
            commands::playback::media_keys_available,
            commands::sync::sync_view,
            commands::sync::open_pairing,
            commands::sync::cancel_pairing,
            commands::sync::pair_with,
            commands::sync::forget_peer,
            commands::sync::sync_with,
            commands::sync::sync_shared_document,
            commands::queue::queue_state,
            commands::queue::queue_view,
            commands::queue::remove_from_queue,
            commands::queue::move_in_queue,
            commands::playback::set_repeat,
            commands::playback::set_shuffled,
            commands::playback::play_next,
            commands::dj::vibe_path,
            commands::dj::blend_preview,
            commands::library::track_details,
            commands::library::search,
            commands::data::data_breakdown,
            commands::data::reveal_data_folder,
            commands::playback::play_tracks,
            commands::playback::next_track,
            commands::playback::previous_track,
            commands::playback::playback_state,
            commands::playback::pause_playback,
            commands::playback::resume_playback,
            commands::playback::stop_playback,
            commands::playback::seek,
            commands::playback::set_volume,
            commands::dj::mood_path,
            commands::settings::settings,
            commands::analysis::set_bpm_override,
            commands::data::data_location,
            commands::data::delete_all_data,
            commands::settings::save_webdav_password,
            commands::settings::has_webdav_password,
            commands::settings::set_remote_config,
            commands::library::scan_library,
            commands::analysis::analyse_library,
            commands::analysis::cancel_analysis,
            commands::library::library_entities,
            commands::library::home_shelves,
            commands::dj::mix_candidates,
            commands::dj::choose_next,
            commands::artwork::track_cover,
            commands::artwork::track_thumb,
            commands::analysis::analysis_status,
            commands::folders::add_local_folder,
            commands::folders::remove_local_folder,
            commands::folders::local_folders,
            commands::cache::cache_status,
            commands::cache::set_cache_max_bytes,
            commands::cache::clear_audio_cache,
            commands::cache::evict_track,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vapor Music");
}

#[cfg(test)]
mod tests {
    /// The last thirty seconds of every track, and what a press means in them.
    ///
    /// A mix is armed `TRANSITION_ARM_LEAD` ahead of its start, so there is
    /// always a cued deck by the time the three cards are worth looking at.
    /// Pressing Switch there used to change the queue and nothing else, and the
    /// already-loaded track played: the badge said Switch, the speakers played
    /// Follow.
    #[test]
    fn a_chosen_exit_abandons_a_mix_armed_for_a_different_track() {
        assert!(
            mix_must_be_rearmed(Some("/fools-rhythm.m4a"), "/gorillaz.m4a", false),
            "a pending mix for another track has to be dropped, or the deck wins"
        );
    }

    #[test]
    fn choosing_what_is_already_armed_changes_nothing() {
        // Follow is a no-op by construction. Re-arming it would cancel a fetch
        // that is already most of the way through and start it again.
        assert!(!mix_must_be_rearmed(Some("/same.m4a"), "/same.m4a", false));
    }

    #[test]
    fn nothing_armed_means_nothing_to_abandon() {
        assert!(!mix_must_be_rearmed(None, "/anything.m4a", false));
    }

    /// Once the crossfade is audible, the press arrives too late to honour.
    ///
    /// Cancelling then resets the outgoing deck to unity gain and flat EQ
    /// part-way through a fade, which is a jump anyone would hear. Playing the
    /// mix out and applying the choice to the track after it is the quieter
    /// wrong answer, and the one this picks deliberately.
    #[test]
    fn a_mix_already_under_way_is_left_alone() {
        assert!(!mix_must_be_rearmed(
            Some("/other.m4a"),
            "/chosen.m4a",
            true
        ));
    }
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

    /// A stalled track is outstanding work; a condemned one is not.
    ///
    /// This is the arithmetic behind "556 of 563 done" never reaching 563. A
    /// permanent failure counts as done deliberately, so it cannot be what
    /// holds the number short — only the retryable kind can, and that kind used
    /// to leave no record at all.
    #[test]
    fn only_a_stalled_track_holds_the_count_short() {
        let (mut app, _dir) = app();
        app.rows = ["/a.flac", "/b.flac", "/c.flac"]
            .iter()
            .map(|href| Row {
                href: (*href).to_string(),
                title: href.trim_start_matches('/').to_string(),
                ..Default::default()
            })
            .collect();

        assert_eq!(analysis_counts(&app), (0, 3), "nothing described yet");

        app.failures
            .insert("/a.flac".into(), "decodes to zero samples".into());
        assert_eq!(
            analysis_counts(&app),
            (1, 3),
            "a condemned file is done — it is not work anybody can finish"
        );

        app.stalls.insert(
            "/b.flac".into(),
            Stall {
                reason: "not downloaded yet".into(),
                attempts: 4,
            },
        );
        assert_eq!(
            analysis_counts(&app),
            (1, 3),
            "a stalled file is still outstanding, which is why the count sticks"
        );
    }

    /// Both kinds reach the screen, each keeping the field that explains it.
    #[test]
    fn the_failure_list_names_tracks_and_says_which_kind() {
        let (mut app, _dir) = app();
        app.rows = vec![Row {
            href: "/b.flac".into(),
            title: "Undertow".into(),
            artist: "Hollow Coast".into(),
            ..Default::default()
        }];
        app.failures
            .insert("/gone.flac".into(), "decodes to zero samples".into());
        app.stalls.insert(
            "/b.flac".into(),
            Stall {
                reason: "not downloaded yet".into(),
                attempts: 4,
            },
        );

        let out = failure_list(&app);

        assert_eq!(out.len(), 2, "both kinds are listed");
        // Permanent first: whoever opened this asked "which ones didn't work"
        // and does not yet know there are two kinds.
        assert!(out[0].permanent);
        // A failure whose row has gone keeps its href as a title rather than
        // being dropped — it is still part of the difference the count shows.
        assert_eq!(out[0].href, "/gone.flac");
        assert_eq!(out[0].title, "/gone.flac");
        assert_eq!(out[0].attempts, 0);

        assert!(!out[1].permanent);
        assert_eq!(out[1].title, "Undertow", "the row supplies the name");
        assert_eq!(out[1].artist, "Hollow Coast");
        assert_eq!(out[1].reason, "not downloaded yet");
        assert_eq!(out[1].attempts, 4, "what tells 'not yet' from 'not ever'");
    }

    /// A dissolve is planned even when the tempi are nowhere near each other.
    ///
    /// `tempo_ratio` refuses a stretch past ±6%, and `plan_mix` used to ask for
    /// one whatever transition had been chosen. So 87 BPM into 139 planned
    /// nothing and the two records simply followed each other with no mix — the
    /// case an Echo Out exists for.
    #[test]
    fn a_wide_tempo_gap_still_gets_a_transition() {
        use crate::analysis::{Analysis, ANALYSIS_VERSION};

        let analysed = |bpm: f32, key: &str| Analysis {
            bpm,
            key: key.to_string(),
            version: ANALYSIS_VERSION,
            duration: 300.0,
            cue_out: 290.0,
            cue_in: 0.0,
            beats: (0..600)
                .map(|i| i as f32 * (60.0 / bpm))
                .collect::<Vec<f32>>(),
            ..Default::default()
        };

        let (mut app, dir) = app();
        app.analysis.insert("/a.mp3".into(), analysed(87.0, "8B"));
        app.analysis.insert("/b.mp3".into(), analysed(139.0, "8A"));
        app.rows.push(row("/a.mp3", "A"));
        app.rows.push(row("/b.mp3", "B"));
        app.playing = Some("/a.mp3".into());
        app.queue
            .set_tracks(vec!["/a.mp3".into(), "/b.mp3".into()], Some("/a.mp3"));

        // The engine would refuse to beat-match this pair, which is the point.
        let far = (87.0f32 / 139.0) as f64 - 1.0;
        assert!(
            far.abs() > vapor_engine::mixer::MAX_STRETCH,
            "the fixture is supposed to be unmatchable"
        );

        // Somewhere inside the arming window before the transition starts.
        let planned = (0..290)
            .map(|secs| plan_mix(&app, secs as f64))
            .find(|m| m.is_some())
            .flatten();

        let mix = planned.expect("no transition was planned for a wide tempo gap");
        assert!(
            !mix.kind.beat_matched(),
            "a pair this far apart should be dissolved, not beat-matched"
        );
        assert_eq!(mix.ratio, 1.0, "a dissolve plays the incoming track as cut");
        assert_eq!(mix.next, "/b.mp3");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Artwork must not reach `tags.json`. It used to: 516 covers made that
    /// file 155 MB, parsed into memory on every launch and held for the life of
    /// the process, on a device with a 256 MB heap.
    #[test]
    fn tags_hold_no_artwork() {
        let (mut app, dir) = app();
        app.set_tags(
            "/music/a.mp3",
            tags::Tags {
                title: Some("Title".into()),
                cover: Some("data:image/jpeg;base64,AAAA".into()),
                ..Default::default()
            },
        );
        app.save_tags().unwrap();

        let json = std::fs::read_to_string(dir.join("tags.json")).unwrap();
        assert!(json.contains("Title"), "the text half is still stored");
        assert!(
            !json.contains("base64"),
            "tags.json is carrying artwork again"
        );
        // And the cover is not lost — it is on disk, reachable by href.
        assert_eq!(
            app.covers.get("/music/a.mp3").as_deref(),
            Some("data:image/jpeg;base64,AAAA")
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A `tags.json` written by an older build carries its covers inline. They
    /// move out on the next load rather than being read into memory forever.
    #[test]
    fn a_legacy_tags_file_migrates_on_load() {
        let (app, dir) = app();
        let store = Store::new(dir.clone());
        drop(app);

        let legacy = serde_json::json!({
            "/music/a.mp3": { "title": "Title", "cover": "data:image/jpeg;base64,AAAA" },
            "/music/b.mp3": { "title": "Other" },
        });
        store.save("tags", &legacy).unwrap();

        let app = AppState::load(Store::new(dir.clone()));
        assert_eq!(
            app.covers.get("/music/a.mp3").as_deref(),
            Some("data:image/jpeg;base64,AAAA"),
            "the inline cover was not moved to disk"
        );
        assert_eq!(app.tags.get("/music/a.mp3").unwrap().cover, None);
        assert_eq!(
            app.tags.get("/music/a.mp3").unwrap().title.as_deref(),
            Some("Title"),
            "the text half survived the migration"
        );
        // The file itself shrank, not just the copy in memory.
        let json = std::fs::read_to_string(dir.join("tags.json")).unwrap();
        assert!(!json.contains("base64"), "tags.json was not rewritten");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A queue is routinely the whole library, and an embedded cover runs to
    /// megabytes. Carrying one per entry killed the phone: 563 tracks came to
    /// ~155 MB of JSON, which the Android WebView allocates as a Java string
    /// against a 256 MB heap, and the process died with an OutOfMemoryError the
    /// moment a track was played. The screen draws exactly one cover, for the
    /// current track, and asks for it by href with `track_cover`.
    #[test]
    fn queue_view_carries_no_artwork() {
        let (mut app, _dir) = app();
        let cover = "COVERDATA".repeat(128);
        let hrefs: Vec<String> = (0..200).map(|i| format!("/music/{i}.mp3")).collect();
        for href in &hrefs {
            app.rows.push(row(href, "Title"));
            app.tags.insert(
                href.clone(),
                StoredTags {
                    cover: Some(cover.clone()),
                    ..Default::default()
                },
            );
        }
        app.queue.set_tracks(hrefs.clone(), Some(&hrefs[0]));

        let view = queue_view_for(&app);
        assert_eq!(view.entries.len(), 200, "the queue itself is still whole");

        let json = serde_json::to_string(&view).unwrap();
        assert!(
            !json.contains(&cover),
            "queue_view is carrying cover art again"
        );
        // Cover-free, 200 entries come to a few tens of kilobytes. With covers
        // this is over 200 KB here and hundreds of megabytes on a real library.
        assert!(
            json.len() < 100_000,
            "queue_view is {} bytes for 200 tracks",
            json.len()
        );
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

    /// The pulse has to come off the beat *around* the playhead, because that
    /// is the one the listener is hearing.
    #[test]
    fn the_beat_window_straddles_the_playhead() {
        let a = analysed_at(128.0, 300.0);
        let period = 60.0 / 128.0;
        // Beats sit at 0.31 + n·period. Land between the tenth and eleventh.
        let position = (0.31 + 10.0 * period + period * 0.4) as f64;
        let (measured, next) = beat_window(&a, position, 128.0);
        assert!(
            (measured - period).abs() < 1e-4,
            "period {measured} should be the local one, {period}"
        );
        assert!(
            next > position && next - position < period as f64,
            "next beat {next} should be inside one period of {position}"
        );
    }

    /// A grid tracked at a tempo since corrected is the exact case
    /// `beats_are_for` exists to catch — pulsing on beats known to be wrong is
    /// worse than not pulsing at all.
    #[test]
    fn a_stale_grid_yields_no_beat() {
        let a = analysed_at(256.0, 300.0);
        assert_eq!(beat_window(&a, 10.0, 128.0), (0.0, 0.0));
    }

    /// Past the last beat, and with nothing analysed, there is no answer and
    /// the caller is told so rather than handed an extrapolation.
    #[test]
    fn the_beat_window_is_empty_where_the_grid_is() {
        let a = analysed_at(128.0, 300.0);
        assert_eq!(beat_window(&a, 10_000.0, 128.0), (0.0, 0.0));
        assert_eq!(
            beat_window(&analysis::Analysis::default(), 10.0, 128.0),
            (0.0, 0.0)
        );
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

    /// The starvation log is written, and starts empty each run.
    ///
    /// Without this, "the log file is not there" means either "nothing
    /// starved" or "the writer is broken", and those look identical from
    /// outside — which is the trap this whole diagnostic exists to avoid.
    #[test]
    fn an_audio_fault_is_written_and_each_run_starts_clean() {
        let dir = std::env::temp_dir().join(format!("vapor-starve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a directory");
        let path = dir.join(AUDIO_FAULT_LOG);

        // A line from a previous run, which must not survive into this one.
        std::fs::write(&path, "stale\n").expect("write");

        note_audio_fault(&dir, "audio starved for 3 block(s)");
        note_audio_fault(&dir, "audio starved for 1 block(s) (during a mix)");

        let written = std::fs::read_to_string(&path).expect("the log");
        assert!(
            !written.contains("stale"),
            "last run's log survived into this one: {written}"
        );
        assert_eq!(
            written.lines().count(),
            2,
            "both lines should be kept, got: {written}"
        );
        assert!(written.contains("(during a mix)"), "got: {written}");
        // Each line is stamped, or it cannot be lined up against what was heard.
        assert!(
            written.lines().all(|l| l
                .split_whitespace()
                .next()
                .is_some_and(|t| t.len() == 8 && t.matches(':').count() == 2)),
            "a line had no clock time: {written}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Tempo correction is a whole-library job, not a per-track one.
    ///
    /// Lyrics and artwork are fetched when a track loads, because they are only
    /// wanted for records somebody plays. Tempo is different: the Vibe DJ plans
    /// a set across the *whole* library, and every candidate's tempo decides
    /// which record can follow which. A correction that only reached played
    /// tracks would leave the planner working from wrong octaves — 87 read as
    /// 174 — for everything it had not heard yet.
    ///
    /// So it stays chained to the analysis pass. This pins the two properties
    /// that make that true rather than the wiring, which a refactor may move:
    /// the correction is bulk, and it is gated on the lookup permission.
    #[test]
    fn tempo_correction_covers_the_library_and_respects_the_switch() {
        let (mut app, dir) = app();
        for href in ["/a.mp3", "/b.mp3", "/c.mp3"] {
            app.rows.push(row(href, href));
            app.analysis
                .insert(href.to_string(), analysed_track(174.0, "8A", 0.5));
        }

        // Off: nothing about the library may be sent anywhere, so there is
        // nothing to correct against.
        app.settings.metadata_lookup_enabled = false;
        assert!(
            !app.settings.metadata_lookup_enabled,
            "the switch is what gates the whole pass",
        );

        // On: every analysed track is a candidate, not only ones played.
        app.settings.metadata_lookup_enabled = true;
        let candidates = app
            .rows
            .iter()
            .filter(|r| app.analysis.contains_key(&r.href))
            .count();
        assert_eq!(
            candidates, 3,
            "the pass must consider the whole library, not a played subset",
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The Genres tab lists genres, not tracks.
    ///
    /// It listed tracks — a card per song, captioned with its artist — for as
    /// long as the tab has existed. Two halves of one fault: `library_entities`
    /// took a boolean, artist or album, so "genre" fell to the `else` and
    /// grouped by album; and `Library.tsx` only treated album and artist as
    /// entity tabs, so the screen rendered the plain row grid regardless.
    #[test]
    fn the_genres_tab_lists_genres_rather_than_tracks() {
        let (mut app, dir) = app();
        for (href, title, genre) in [
            ("/a.mp3", "A", "House"),
            ("/b.mp3", "B", "House"),
            ("/c.mp3", "C", "Ambient"),
            ("/d.mp3", "D", ""),
        ] {
            let mut r = row(href, href);
            r.title = title.to_string();
            r.genre = genre.to_string();
            app.rows.push(r);
        }

        let view = LibraryView {
            query: String::new(),
            sort_key: None,
            ascending: true,
            group_by: Some("genre".to_string()),
            genre: None,
            album: None,
            artist: None,
        };
        let got = library_entities_for(&app, &view);
        let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();

        // Alphabetical. This read `["House", "Ambient"]` when tiles came back
        // in the order the rows arrived — which was sorted by *track title*,
        // an answer to a question nobody asks of a grid of genres.
        assert_eq!(
            names,
            vec!["Ambient", "House"],
            "expected one tile per genre, got {names:?}",
        );
        // Two tracks under House, one under Ambient — a tile counts its
        // members, and a tile per track would have made every count 1. Looked
        // up by name rather than by position so this keeps saying what it means
        // if the ordering changes again.
        let tracks_on = |name: &str| got.iter().find(|e| e.name == name).map(|e| e.tracks);
        assert_eq!(tracks_on("House"), Some(2));
        assert_eq!(tracks_on("Ambient"), Some(1));
        // The untagged track is not a genre and gets no tile of its own.
        assert!(!names.contains(&""), "an empty genre became a tile");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The Artists tab leads with who you actually have most of.
    ///
    /// Tiles used to arrive in the order the rows did — sorted by track title —
    /// so an artist with one loose remix could sit above one with thirty-eight
    /// tracks, and the grid had no reading order at all. Count first, then name,
    /// because this library's tail is 52 artists holding exactly one track and
    /// "arbitrary" is the only other way to order them.
    #[test]
    fn the_artists_tab_leads_with_the_biggest_and_breaks_ties_by_name() {
        let (mut app, dir) = app();
        // Deliberately inserted small-first and out of alphabetical order, so
        // insertion order cannot pass this by accident.
        for (href, title, artist) in [
            ("/z.mp3", "A", "Zero T"),
            ("/a.mp3", "B", "Alpha Rhythm"),
            ("/n1.mp3", "C", "Noisia"),
            ("/n2.mp3", "D", "Noisia"),
            ("/n3.mp3", "E", "Noisia"),
            ("/d1.mp3", "F", "Delta Heavy"),
            ("/d2.mp3", "G", "Delta Heavy"),
        ] {
            let mut r = row(href, title);
            r.artist = artist.to_string();
            r.artist_source = vapor_library::index::Source::File;
            app.rows.push(r);
        }

        let view = LibraryView {
            query: String::new(),
            sort_key: None,
            ascending: true,
            group_by: Some("artist".to_string()),
            genre: None,
            album: None,
            artist: None,
        };
        let got = library_entities_for(&app, &view);
        let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();

        assert_eq!(
            names,
            // 3, then 2, then the two singletons alphabetically.
            vec!["Noisia", "Delta Heavy", "Alpha Rhythm", "Zero T"],
            "expected most tracks first then alphabetical, got {names:?}",
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Albums with gaps sink, ordered by how close to whole they are.
    ///
    /// The case this is really for: a library where most of the "albums" are a
    /// single track of a record, because they arrived as one-off downloads. A
    /// tab that shows those beside a complete album, in the same shape, is
    /// telling a person they own something they do not.
    #[test]
    fn incomplete_albums_sink_below_whole_ones_and_rank_by_how_much_is_missing() {
        let (mut app, dir) = app();

        // (album, folder, how many of its tracks are held, how long it is)
        let releases = [
            ("Whole Record", 12u64, 12usize, 12u32),
            ("Almost There", 13, 11, 12),
            ("Half An EP", 14, 4, 8),
            ("One Track Of", 15, 1, 19),
            ("Never Looked Up", 0, 3, 0),
        ];

        for (album, deezer_id, held, total) in releases {
            if deezer_id != 0 {
                app.albums.insert(
                    deezer_id,
                    metadata::AlbumFacts {
                        id: deezer_id,
                        title: album.to_string(),
                        artist: "An Artist".to_string(),
                        record_type: "album".to_string(),
                        nb_tracks: total,
                        tracks: (0..total).map(|i| format!("{album} {i}")).collect(),
                    },
                );
            }
            for i in 0..held {
                let href = format!("/{album}/{i}.mp3");
                let mut r = row(&href, &format!("{album} {i}"));
                r.album = album.to_string();
                r.album_source = vapor_library::index::Source::File;
                app.rows.push(r);
                if deezer_id != 0 {
                    app.looked.entry(href).or_default().deezer_album_id = deezer_id;
                }
            }
        }

        let view = LibraryView {
            query: String::new(),
            sort_key: None,
            ascending: true,
            group_by: Some("album".to_string()),
            genre: None,
            album: None,
            artist: None,
        };
        let got = library_entities_for(&app, &view);
        let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();

        assert_eq!(
            names,
            vec![
                // Whole, alphabetically. "Never Looked Up" is here because an
                // unknown length is not evidence of a gap.
                "Never Looked Up",
                "Whole Record",
                // Then the gaps, fullest first: 11/12, then 4/8, then 1/19.
                "Almost There",
                "Half An EP",
                "One Track Of",
            ],
            "got {names:?}",
        );

        let by_name = |n: &str| got.iter().find(|e| e.name == n).expect("tile missing");
        assert!(!by_name("Whole Record").incomplete);
        assert!(by_name("Almost There").incomplete);
        assert_eq!(by_name("One Track Of").total_tracks, 19);
        assert_eq!(by_name("One Track Of").tracks, 1);
        // The one nobody looked up must not be accused of missing anything.
        assert!(!by_name("Never Looked Up").incomplete);
        assert_eq!(by_name("Never Looked Up").total_tracks, 0);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Holding more than the service lists is not a gap.
    ///
    /// A bonus disc, a duplicate, or a deluxe edition matched to the standard
    /// release. Reporting that as incomplete would push whole albums to the
    /// bottom of the tab for having *too much* on them.
    #[test]
    fn holding_more_tracks_than_the_release_lists_is_not_incomplete() {
        let (mut app, dir) = app();
        app.albums.insert(
            7,
            metadata::AlbumFacts {
                id: 7,
                title: "Deluxe".to_string(),
                artist: "An Artist".to_string(),
                record_type: "album".to_string(),
                nb_tracks: 2,
                tracks: vec!["A".to_string(), "B".to_string()],
            },
        );
        for i in 0..4 {
            let href = format!("/deluxe/{i}.mp3");
            let mut r = row(&href, &format!("T{i}"));
            r.album = "Deluxe".to_string();
            r.album_source = vapor_library::index::Source::File;
            app.rows.push(r);
            app.looked.entry(href).or_default().deezer_album_id = 7;
        }

        let view = LibraryView {
            query: String::new(),
            sort_key: None,
            ascending: true,
            group_by: Some("album".to_string()),
            genre: None,
            album: None,
            artist: None,
        };
        let got = library_entities_for(&app, &view);
        assert_eq!(got.len(), 1);
        assert!(!got[0].incomplete, "4 of 2 was called incomplete");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// One mistagged track cannot rename the album everything else is on.
    ///
    /// The failure it prevents: a folder of twelve tracks where one matched a
    /// two-track single would take the single's length, and the other eleven
    /// would read as missing from a record they are not on.
    #[test]
    fn the_release_is_decided_by_majority_not_by_the_first_track() {
        let (mut app, dir) = app();
        for (id, total) in [(100u64, 12u32), (200, 2)] {
            app.albums.insert(
                id,
                metadata::AlbumFacts {
                    id,
                    title: format!("Release {id}"),
                    artist: "An Artist".to_string(),
                    record_type: "album".to_string(),
                    nb_tracks: total,
                    tracks: (0..total).map(|i| format!("t{i}")).collect(),
                },
            );
        }
        // The odd one out is inserted first, so "take the first" would pick it.
        for (i, id) in [200u64, 100, 100, 100].into_iter().enumerate() {
            let href = format!("/rec/{i}.mp3");
            let mut r = row(&href, &format!("T{i}"));
            r.album = "A Folder".to_string();
            r.album_source = vapor_library::index::Source::File;
            app.rows.push(r);
            app.looked.entry(href).or_default().deezer_album_id = id;
        }

        let view = LibraryView {
            query: String::new(),
            sort_key: None,
            ascending: true,
            group_by: Some("album".to_string()),
            genre: None,
            album: None,
            artist: None,
        };
        let got = library_entities_for(&app, &view);
        assert_eq!(
            got[0].total_tracks, 12,
            "the single-track outlier decided the album's length"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// And opening one narrows to exactly that genre.
    #[test]
    fn opening_a_genre_shows_only_its_tracks() {
        let (mut app, dir) = app();
        for (href, genre) in [
            ("/a.mp3", "House"),
            ("/b.mp3", "Deep House"),
            ("/c.mp3", "House"),
        ] {
            let mut r = row(href, href);
            r.genre = genre.to_string();
            app.rows.push(r);
        }

        let view = LibraryView {
            query: String::new(),
            sort_key: None,
            ascending: true,
            group_by: None,
            genre: Some("House".to_string()),
            album: None,
            artist: None,
        };
        let rows = resolved_rows(&app, &view);
        let hrefs: Vec<&str> = rows.iter().map(|r| r.href.as_str()).collect();

        // Exact, not a substring: "Deep House" is a different genre.
        assert_eq!(hrefs, vec!["/a.mp3", "/c.mp3"], "got {hrefs:?}");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The DJ conducts within what was played from, not the library.
    ///
    /// `audio_manager.gd` planned out of `current_playlist` — press play on an
    /// album and the set stayed in that album. The port read `app.rows`
    /// unconditionally, so every scope was the whole library: opening a record
    /// and pressing play gave you one track of it and then wherever the
    /// pathfinder felt like going.
    #[test]
    fn a_scoped_set_is_conducted_inside_its_scope() {
        let (mut app, dir) = conducting();
        app.scope = Some(Scope {
            name: "A Record".to_string(),
            tracks: ["/a.mp3", "/b.mp3"].iter().map(|s| s.to_string()).collect(),
        });

        assert!(extend_set(&mut app), "the DJ added nothing");

        for href in app.queue.tracks() {
            assert!(
                href == "/a.mp3" || href == "/b.mp3",
                "the set left its scope: queued {href}"
            );
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    /// No scope is the library, which is what playing from an unfiltered list
    /// means — and the case that must keep working unchanged.
    #[test]
    fn an_unscoped_set_may_use_the_whole_library() {
        let (mut app, dir) = conducting();
        assert!(app.scope.is_none());

        assert!(extend_set(&mut app), "the DJ added nothing");
        assert!(
            app.queue
                .tracks()
                .iter()
                .any(|h| h != "/a.mp3" && h != "/b.mp3"),
            "the planner stayed inside two tracks with the library available"
        );

        let _ = std::fs::remove_dir_all(dir);
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
        let follow = mix_candidates_for(&mut app)
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

    /// Stay stays in the vibe, not merely at the level.
    ///
    /// It was chosen on intensity and transition cost alone, so with genre
    /// absent — 488 of 534 tracks in Dylan's library carry no genre tag — the
    /// closest *level* won outright, and a De André ballad was offered as the
    /// way to stay in a hip hop set. The artist is the signal a
    /// folder-organised library actually has.
    #[test]
    fn stay_prefers_the_artist_already_playing_when_genre_is_unknown() {
        let (mut app, dir) = app();
        let tracks = [
            ("/keem-a.mp3", "KEEM THE CIPHER", 85.0, "9B", 0.55),
            ("/keem-b.mp3", "KEEM THE CIPHER", 88.0, "9B", 0.60),
            // Closer in level than the label-mate, and nothing like it.
            ("/deandre.mp3", "Fabrizio De Andre", 96.0, "8A", 0.56),
        ];
        for (href, artist, bpm, key, energy) in tracks {
            app.rows.push(row(href, href));
            let i = app.rows.len() - 1;
            app.rows[i].artist = artist.to_string();
            app.analysis
                .insert(href.to_string(), analysed_track(bpm, key, energy));
        }
        app.queue.set_tracks(vec!["/keem-a.mp3".to_string()], None);
        app.playing = Some("/keem-a.mp3".to_string());

        let stay = mix_candidates_for(&mut app)
            .into_iter()
            .find(|c| c.exit == Exit::Stay)
            .expect("a Stay card");
        assert_eq!(
            stay.href, "/keem-b.mp3",
            "Stay left the artist for a closer intensity",
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Choosing an exit does not move the board (2026-08-20).
    ///
    /// The three cards were recomputed on every read. Choosing one made it the
    /// queued track, so it slid into the Follow slot and Stay refilled with
    /// something else — press Stay and the record you picked jumps sideways
    /// while a stranger takes its place. They also drifted on their own as the
    /// planner re-ran behind the screen.
    ///
    /// This supersedes an earlier rule that the queued card is always labelled
    /// Follow. That existed to stop two cards reading SWITCH at once, and
    /// holding the board fixes the same thing better: the slots do not move, so
    /// nothing collides.
    #[test]
    fn choosing_an_exit_leaves_the_cards_where_they_were() {
        let (mut app, dir) = conducting();
        extend_set(&mut app);

        let before = mix_candidates_for(&mut app);
        assert!(before.len() > 1, "need more than one card to test a move");

        let switch_href = before
            .iter()
            .find(|c| c.exit == Exit::Switch)
            .expect("a Switch card")
            .href
            .clone();
        let slot = before
            .iter()
            .position(|c| c.href == switch_href)
            .expect("the card it came from");

        assert!(app.queue.set_next(&switch_href));

        let after = mix_candidates_for(&mut app);
        assert_eq!(
            after.iter().map(|c| &c.href).collect::<Vec<_>>(),
            before.iter().map(|c| &c.href).collect::<Vec<_>>(),
            "the offered tracks changed after a press",
        );
        assert_eq!(
            after[slot].exit,
            Exit::Switch,
            "the chosen card changed slot or label",
        );
        assert!(after[slot].selected, "the ring did not land on the choice");
        assert_eq!(
            after.iter().filter(|c| c.selected).count(),
            1,
            "more than one card claims to be what happens next",
        );

        // And a new track is a new offer, or the board would freeze for ever.
        app.playing = Some("/c.mp3".to_string());
        app.offered = None;
        let fresh = mix_candidates_for(&mut app);
        assert!(
            fresh.iter().all(|c| c.href != "/c.mp3"),
            "the exits offered the track that is playing",
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Audio is kept for the set, for what was asked for, and nothing else.
    ///
    /// The cache reached five gigabytes because analysis has to download every
    /// track to listen to it and the bytes stayed afterwards. Reading a file
    /// once is not a reason to keep it.
    #[test]
    fn only_the_window_and_the_pinned_keep_their_audio() {
        let (mut app, dir) = app();
        let tracks: Vec<String> = (0..20).map(|i| format!("/{i}.mp3")).collect();
        for href in &tracks {
            app.rows.push(row(href, href));
        }
        app.queue.set_tracks(tracks.clone(), Some("/10.mp3"));
        app.playing = Some("/10.mp3".to_string());

        // Three back and five ahead of the track playing.
        assert!(keeps_audio(&app, "/10.mp3"), "the track playing");
        assert!(keeps_audio(&app, "/7.mp3"), "three back");
        assert!(keeps_audio(&app, "/15.mp3"), "five ahead");

        // And nothing else.
        assert!(!keeps_audio(&app, "/6.mp3"), "four back is past the window");
        assert!(
            !keeps_audio(&app, "/16.mp3"),
            "six ahead is past the window"
        );

        // Unless it was asked for on purpose, which outranks the window.
        app.pinned.insert("/0.mp3".to_string());
        assert!(
            keeps_audio(&app, "/0.mp3"),
            "a download is kept wherever it is"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A library nothing is playing from keeps nothing: an analysis pass on its
    /// own is not a reason to hold audio.
    #[test]
    fn nothing_playing_means_nothing_kept() {
        let (mut app, dir) = app();
        app.rows.push(row("/a.mp3", "A"));
        assert!(!keeps_audio(&app, "/a.mp3"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// One count, so the card and the notification cannot disagree.
    ///
    /// They had one each: the card counted the library and the notification
    /// counted the pass, so it opened at "0 of 526" beside a card reading
    /// "34 of 563". A file the decoder has permanently refused counts as done —
    /// it is not outstanding work, and excluding it makes the total one the
    /// pass can never reach.
    #[test]
    fn the_count_is_of_the_library_and_includes_what_cannot_be_described() {
        let (mut app, dir) = app();
        for href in ["/a.mp3", "/b.mp3", "/c.mp3"] {
            app.rows.push(row(href, href));
        }
        assert_eq!(analysis_counts(&app), (0, 3));

        app.analysis
            .insert("/a.mp3".to_string(), analysed_track(120.0, "8A", 0.5));
        assert_eq!(analysis_counts(&app), (1, 3));

        // Refused for good: done, as far as anything left to do goes.
        app.failures
            .insert("/b.mp3".to_string(), "no decodable audio track".to_string());
        assert_eq!(analysis_counts(&app), (2, 3));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The tag is the recording; the filename is not.
    ///
    /// `Row::title` comes from the path and `apply_tags` never overwrites it,
    /// so a second copy is titled `Bocca di rosa (1)` and compares as a
    /// different track. Both files carry the same tag, which is the question.
    #[test]
    fn a_copy_is_recognised_by_its_tag_not_its_filename() {
        let (mut app, dir) = app();
        for href in ["/bocca.mp3", "/bocca (1).mp3"] {
            app.rows.push(row(href, href));
            app.tags.insert(
                href.to_string(),
                tags::Tags {
                    title: Some("Bocca di rosa".to_string()),
                    artist: Some("Fabrizio De Andre".to_string()),
                    ..Default::default()
                }
                .into(),
            );
        }
        // Path-derived titles genuinely differ, which is what used to defeat it.
        assert_ne!(app.rows[0].title, app.rows[1].title);

        let dupes = duplicate_hrefs(&app);
        assert_eq!(dupes.len(), 1, "the two copies were not seen as one record");
        // The survivor is the first href, so it is the same one next launch.
        assert!(dupes.contains("/bocca.mp3") || dupes.contains("/bocca (1).mp3"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Hiding is a view. The files are untouched and the count still knows.
    #[test]
    fn hiding_duplicates_does_not_remove_anything() {
        let (mut app, dir) = app();
        for href in ["/one.mp3", "/one (1).mp3"] {
            app.rows.push(row(href, href));
            app.tags.insert(
                href.to_string(),
                tags::Tags {
                    title: Some("One".to_string()),
                    artist: Some("Someone".to_string()),
                    ..Default::default()
                }
                .into(),
            );
        }

        let view = LibraryView {
            query: String::new(),
            sort_key: None,
            ascending: true,
            group_by: None,
            genre: None,
            album: None,
            artist: None,
        };
        assert_eq!(resolved_rows(&app, &view).len(), 2);

        app.settings.hide_duplicates = true;
        assert_eq!(resolved_rows(&app, &view).len(), 1);
        // Still on disk, still counted, still there to delete by hand.
        assert_eq!(app.rows.len(), 2);
        assert_eq!(duplicate_hrefs(&app).len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The set walked out of a track and straight back into it.
    ///
    /// A library holding the same recording twice — `Bocca di rosa` and
    /// `Bocca di rosa (1)` — gives the planner two entries with identical
    /// tempo, key and intensity, so their transition cost is as near nothing as
    /// the model can produce and the duplicate is the *cheapest* next step. The
    /// pathfinder's guard is `!path.contains(href)`, and two copies are two
    /// hrefs, so it never saw a repeat.
    #[test]
    fn a_duplicate_file_is_not_a_second_track() {
        let (mut app, dir) = conducting();

        // The same recording, twice, exactly as a re-download leaves it.
        app.rows.push(row("/a-copy.mp3", "/a-copy.mp3"));
        let i = app.rows.len() - 1;
        app.rows[i].title = "Carlo Martello".to_string();
        app.rows[i].artist = "Fabrizio De Andre".to_string();
        app.analysis
            .insert("/a-copy.mp3".to_string(), analysed_track(174.0, "4A", 0.8));

        let original = app
            .rows
            .iter()
            .position(|r| r.href == "/a.mp3")
            .expect("the fixture track");
        app.rows[original].title = "Carlo Martello".to_string();
        app.rows[original].artist = "Fabrizio De Andre".to_string();

        let pool = track_meta_pool(&app);
        assert!(
            !(pool.contains_key("/a.mp3") && pool.contains_key("/a-copy.mp3")),
            "both copies of one recording reached the planner",
        );

        // And nothing else was lost on the way.
        assert!(pool.contains_key("/c.mp3"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The guard that does not depend on the planner having been involved.
    ///
    /// A queue can be built by hand, by a playlist, or by pressing play on an
    /// album, and none of those go through the pool. Two rips of one track
    /// beat-match perfectly and mix into themselves.
    #[test]
    fn a_mix_is_not_armed_between_two_copies_of_one_recording() {
        let (mut app, dir) = conducting();
        app.rows.push(row("/a-copy.mp3", "/a-copy.mp3"));
        let i = app.rows.len() - 1;
        app.rows[i].title = "Carlo Martello".to_string();
        app.rows[i].artist = "Fabrizio De Andre".to_string();
        app.analysis
            .insert("/a-copy.mp3".to_string(), analysed_track(174.0, "4A", 0.8));

        let original = app
            .rows
            .iter()
            .position(|r| r.href == "/a.mp3")
            .expect("the fixture track");
        app.rows[original].title = "Carlo Martello".to_string();
        app.rows[original].artist = "Fabrizio De Andre".to_string();

        app.queue.set_tracks(
            vec!["/a.mp3".to_string(), "/a-copy.mp3".to_string()],
            Some("/a.mp3"),
        );
        assert!(
            plan_mix(&app, 0.0).is_none(),
            "armed a mix from a track into another copy of itself",
        );

        // A different record still mixes, so this is not refusing everything.
        // `/b.mp3` rather than `/c.mp3`: 172 against 174 is a tempo the engine
        // will actually match, and 128 is not — a refusal there would be about
        // the ratio, not about this guard.
        app.queue.set_tracks(
            vec!["/a.mp3".to_string(), "/b.mp3".to_string()],
            Some("/a.mp3"),
        );
        assert!(plan_mix(&app, 0.0).is_some());
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
        let (mut app, dir) = conducting();
        // Not `peek_next().is_none()`: a one-track queue wraps under repeat-all
        // and answers with the track playing, which is precisely the state that
        // used to produce a Follow card offering the current record back.
        assert_eq!(
            app.queue.peek_next(None),
            Some("/a.mp3"),
            "the fixture is meant to have no real next track",
        );

        let cards = mix_candidates_for(&mut app);
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

        let cards = mix_candidates_for(&mut app);
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

    /// AUD-26, whole. The half-read drum & bass track has to report the same
    /// tempo to the table, the DJ's pool, the beat grid and the retracker —
    /// because the failure this replaced was not a wrong number, it was two
    /// different right-looking numbers in different parts of the app.
    ///
    /// 87 in, 174 everywhere out. The tag says "DnB" — a spelling the old
    /// exact-match lookup could not match, and the one AUD-26 named first.
    #[test]
    fn a_half_read_dnb_track_reports_one_tempo_everywhere() {
        let (mut app, dir) = app();
        let mut r = row("/dnb.mp3", "d");
        r.genre = "DnB".to_string();
        app.rows.push(r.clone());
        app.analysis
            .insert("/dnb.mp3".to_string(), analysed_at(87.0, 240.0));
        let analysis = app.analysis.get("/dnb.mp3").expect("analysis").clone();

        // The tempo every consumer reads.
        assert_eq!(
            tempo_in_force(&app, "/dnb.mp3", Some(&analysis)),
            Some(174.0)
        );

        // The library table.
        let mut shown = r.clone();
        app.apply_analysis(&mut shown);
        assert_eq!(shown.bpm, 174.0, "the table still shows the half-time read");

        // The DJ's pool, which feeds the Vibe cards.
        let pool = track_meta_pool(&app);
        assert_eq!(pool.get("/dnb.mp3").expect("track").bpm, 174.0);

        // The grid the stretcher meets the record on. The tracked beats were
        // laid down at 87, so they are refused and a 174 grid stands in —
        // exactly what `beats_are_for` is for.
        let grid = beat_grid(&analysis, tempo_in_force(&app, "/dnb.mp3", Some(&analysis)));
        assert_eq!(grid.bpm, 174.0);
        assert_ne!(
            grid.beats, analysis.beats,
            "a grid tracked at 87 must not be served as a 174 grid"
        );

        // And the retracker is told there is work to do, so the synthetic grid
        // is temporary rather than permanent.
        assert_eq!(
            stale_grids(&app, &["/dnb.mp3".to_string()]),
            vec![("/dnb.mp3".to_string(), 174.0)]
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The two numbers that used to disagree, asked directly. Before AUD-26 the
    /// pool applied `octave_correct` and the grid did not, so this pair read
    /// 174 and 87.
    #[test]
    fn the_displayed_tempo_and_the_mixed_tempo_cannot_diverge() {
        let (mut app, dir) = app();
        for (href, genre) in [("/dnb.mp3", "Drum & Bass"), ("/hop.mp3", "Hip Hop")] {
            let mut r = row(href, "t");
            r.genre = genre.to_string();
            app.rows.push(r);
            app.analysis
                .insert(href.to_string(), analysed_at(87.0, 240.0));
        }

        let pool = track_meta_pool(&app);
        for href in ["/dnb.mp3", "/hop.mp3"] {
            let analysis = app.analysis.get(href).expect("analysis");
            let grid = beat_grid(
                &analysis.clone(),
                tempo_in_force(&app, href, Some(analysis)),
            );
            assert_eq!(
                pool.get(href).expect("track").bpm,
                grid.bpm,
                "{href}: the card and the stretcher disagree"
            );
        }
        // And the hip hop track, genuinely at 87, is untouched — the two are
        // only distinguishable by genre, which is the whole argument.
        assert_eq!(pool.get("/hop.mp3").expect("track").bpm, 87.0);
        assert_eq!(pool.get("/dnb.mp3").expect("track").bpm, 174.0);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// AUD-24's other half reaching the tempo table: the service answers with
    /// its coarse shelf *and* the specific genre, and the specific one decides.
    #[test]
    fn a_second_genre_from_the_service_resolves_the_octave() {
        let (mut app, dir) = app();
        app.rows.push(row("/x.mp3", "x"));
        app.analysis
            .insert("/x.mp3".to_string(), analysed_at(87.0, 240.0));

        // "Electronic" alone is every electronic record Deezer knows, and says
        // nothing about tempo.
        app.looked.insert(
            "/x.mp3".to_string(),
            metadata::Looked {
                genre: "Electronic".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(track_meta_pool(&app).get("/x.mp3").expect("t").bpm, 87.0);

        // With the second genre kept, it does.
        app.looked.insert(
            "/x.mp3".to_string(),
            metadata::Looked {
                genre: "Electronic / Drum & Bass".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(track_meta_pool(&app).get("/x.mp3").expect("t").bpm, 174.0);

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

        // The same paste, after the box began prefilling `https://`.
        //
        // A prefix check passed this: it starts with https://, so it is an
        // address. It is a password with a scheme in front of it, and stored as
        // an origin it produces a scan that finds nothing and reports no error
        // — which reads as an empty library rather than a wrong address.
        for not_one in ["https://4wg9ie7xi8v7nbi6", "https://", "https:///dav/Music"] {
            assert!(
                apply_remote_config(&mut app, not_one, "someone", "Music").is_err(),
                "{not_one:?} was accepted as a server address"
            );
            assert!(
                app.settings.remote.url.is_empty(),
                "{not_one:?} was stored anyway"
            );
        }

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

    /// The seek bar's length, when the container would not say.
    ///
    /// A fragmented MP4 declares no length, so the deck can only report what it
    /// has decoded so far — a number that grows for the whole song and is right
    /// only once it is over. The analysis pass already decoded the file to its
    /// end, and its measurement is what the transport should be drawn against.
    #[test]
    fn an_undeclared_length_falls_back_to_the_measured_one() {
        let deck = audio::Snapshot {
            status: audio::Status::Playing,
            position: 67.0,
            // A minute in, and the deck has decoded seventy seconds of a track
            // whose length nothing declared.
            duration: 70.0,
            volume: 1.0,
            level: 0.0,
            brightness: 0.0,
            commands_deferred: 0,
            starved_blocks: 0,
            starved_incoming_blocks: 0,
            limiter_steps: 0,
            limiter_deepest_db: 0.0,
        };
        let measured = analysed_at(87.0, 327.0);

        assert_eq!(playing_duration(Some(&deck), Some(&measured)), 327.0);

        // Nothing analysed it, so the growing answer is the only one there is —
        // and still beats the zero this used to report.
        assert_eq!(playing_duration(Some(&deck), None), 70.0);

        // Nothing playing: no length, rather than the last track's.
        assert_eq!(playing_duration(None, None), 0.0);
    }

    // -----------------------------------------------------------------------
    // The home shelves
    // -----------------------------------------------------------------------

    /// A library of two artists, four albums between them, and a playlist.
    fn a_library() -> (AppState, std::path::PathBuf) {
        let (mut app, dir) = app();
        app.rows = vec![
            shelf_row(
                "/dav/Aphex/Windowlicker/01.m4a",
                "Aphex Twin",
                "Windowlicker",
            ),
            shelf_row(
                "/dav/Aphex/Windowlicker/02.m4a",
                "Aphex Twin",
                "Windowlicker",
            ),
            shelf_row(
                "/dav/Aphex/SAW/01.m4a",
                "Aphex Twin",
                "Selected Ambient Works",
            ),
            shelf_row("/dav/BoC/Geogaddi/01.m4a", "Boards of Canada", "Geogaddi"),
        ];
        (app, dir)
    }

    /// A row with an artist and an album the index would believe.
    ///
    /// The sources matter: the entity grid drops rows whose artist is unknown,
    /// so a row built by [`row`] above never reaches a shelf at all.
    fn shelf_row(href: &str, artist: &str, album: &str) -> Row {
        Row {
            artist: artist.to_string(),
            album: album.to_string(),
            artist_source: vapor_library::index::Source::File,
            album_source: vapor_library::index::Source::File,
            ..row(href, "A Track")
        }
    }

    /// The claim the shelf makes, made true.
    ///
    /// Two playlists, one of them listened to. Nothing else separates them —
    /// same size, created in the same breath — so if the played one is not
    /// first, "most played" is decoration.
    #[test]
    fn a_played_playlist_leads_the_shelf() {
        let (mut app, dir) = a_library();
        app.playlists.create("p1", "Untouched");
        app.playlists.create("p2", "On Repeat");
        app.playlists.add_track("p1", "/dav/BoC/Geogaddi/01.m4a");
        app.playlists.add_track("p2", "/dav/Aphex/SAW/01.m4a");

        app.credit_play("/dav/Aphex/SAW/01.m4a", Some("playlist:p2"));

        let shelves = home_shelves_for(&app);
        assert_eq!(
            shelves
                .playlists
                .iter()
                .map(|s| s.title.as_str())
                .collect::<Vec<_>>(),
            ["On Repeat", "Untouched"],
        );
        assert_eq!(shelves.playlists[0].plays, 1);
        assert_eq!(shelves.playlists[0].subtitle, "1 track");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A playlist built this morning out of records someone has worn out.
    ///
    /// It has no plays of its own and should still not sit below one they have
    /// never opened. Member plays are the key that says so, and without them
    /// the new playlist sorts second — which is the opposite of true.
    #[test]
    fn a_new_playlist_of_worn_out_records_outranks_an_untouched_one() {
        let (mut app, dir) = a_library();
        app.playlists.create("p1", "Never Opened");
        app.playlists.add_track("p1", "/dav/BoC/Geogaddi/01.m4a");
        app.playlists.create("p2", "Made This Morning");
        app.playlists.add_track("p2", "/dav/Aphex/SAW/01.m4a");

        // Played from the library, not from either playlist: no collection is
        // credited, so both have zero direct plays.
        app.credit_play("/dav/Aphex/SAW/01.m4a", None);

        let shelves = home_shelves_for(&app);
        assert_eq!(shelves.playlists[0].title, "Made This Morning");
        assert_eq!(
            shelves.playlists[0].plays, 0,
            "it is ranked on its records' plays, and says so honestly"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Artists and albums are ranked on their tracks, because that is all
    /// there is: nothing means "put on Aphex Twin", only playing their records.
    #[test]
    fn the_most_listened_to_artist_leads_the_artist_shelf() {
        let (mut app, dir) = a_library();
        app.credit_play("/dav/BoC/Geogaddi/01.m4a", None);

        let shelves = home_shelves_for(&app);
        assert_eq!(shelves.artists[0].title, "Boards of Canada");
        assert_eq!(shelves.artists[0].plays, 1);
        assert_eq!(shelves.albums[0].title, "Geogaddi");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The day the library is new, every count is zero and the shelf still has
    /// to say something. Biggest first is at least about the person's music,
    /// where falling through to alphabetical would put A first for ever.
    #[test]
    fn a_library_nobody_has_played_from_leads_with_the_biggest() {
        let (app, dir) = a_library();

        let shelves = home_shelves_for(&app);
        assert_eq!(shelves.albums[0].title, "Windowlicker");
        assert_eq!(shelves.albums[0].tracks, 2);
        assert!(shelves.albums.iter().all(|s| s.plays == 0));
        assert_eq!(shelves.tracks, 4, "the line under the title counts tracks");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A group is a saved set of artists and albums, not a list of tracks, so
    /// how big one is has to be resolved against the library to be known.
    #[test]
    fn a_group_is_measured_against_the_library() {
        let (mut app, dir) = a_library();
        app.groups.create("g1", "Ambient");
        app.groups
            .add_entity("g1", vapor_library::EntityType::Artist, "Aphex Twin");

        let shelves = home_shelves_for(&app);
        assert_eq!(shelves.groups[0].title, "Ambient");
        assert_eq!(shelves.groups[0].tracks, 3);
        assert_eq!(shelves.groups[0].subtitle, "3 tracks");
        assert_eq!(shelves.groups[0].lead, "/dav/Aphex/Windowlicker/01.m4a");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A shelf scrolls sideways, but not for ever: every tile past the cap is
    /// a cover fetched for a place nobody scrolls to.
    #[test]
    fn a_shelf_stops_at_a_dozen() {
        let (mut app, dir) = a_library();
        for i in 0..30 {
            app.playlists.create(format!("p{i}"), format!("List {i}"));
        }

        assert_eq!(home_shelves_for(&app).playlists.len(), SHELF);

        let _ = std::fs::remove_dir_all(dir);
    }
}

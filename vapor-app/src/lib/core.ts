/**
 * Typed client for the Rust core.
 *
 * Every call into `vapor-core` goes through here. The point is that the shapes
 * are declared once: a command signature changing in `src-tauri/src/lib.rs`
 * should break the build here rather than fail at runtime in a screen.
 *
 * The `invoke` indirection also leaves room for the browser build. On the web
 * there is no Tauri IPC — the same core crates are compiled to wasm and called
 * directly — so this module is the only place that has to know which it is.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Types — mirror the serde shapes in src-tauri/src/lib.rs
// ---------------------------------------------------------------------------

/** Where a derived field came from, so the UI can distinguish fact from guess. */
export type Source = "cache" | "file" | "folder" | "unknown";

export interface Row {
  href: string;
  title: string;
  artist: string;
  album: string;
  artistSource: Source;
  albumSource: Source;
  /** Empty when unknown. */
  genre: string;
  /** 0 when unknown — not "0 BPM". */
  bpm: number;
  /** Camelot key, empty when unknown. */
  key: string;
  year: number;
  manualPos: number;
}

export interface LibrarySection {
  /** Empty when ungrouped; "—" for rows whose grouping field is unknown. */
  header: string;
  rows: Row[];
}

export type SortKey =
  | "title"
  | "artist"
  | "album"
  | "genre"
  | "year"
  | "bpm"
  | "key"
  | "order";

export type GroupBy = "none" | "artist" | "album" | "genre";

export interface LibraryView {
  query?: string;
  sortKey?: SortKey;
  ascending?: boolean;
  groupBy?: GroupBy;
  /** Narrow to exactly this album. Set by opening one, not by typing. */
  album?: string;
  /** Narrow to exactly this artist. */
  artist?: string;
}

/**
 * One album or artist, as the Library grid draws it.
 *
 * The grid used to render a card per track grouped under an album heading,
 * which answers "what is on this album" rather than "which albums do I have".
 */
export interface LibraryEntity {
  name: string;
  /** The album's artist, or how many albums an artist has. */
  subtitle: string;
  tracks: number;
  /** A track from it — what plays when pressed, and whose cover is shown. */
  lead: string;
}

export interface Playlist {
  id: string;
  name: string;
  customCoverPath: string;
  tracks: string[];
  /** The folder it is filed in, or "" for the top level. */
  folderId: string;
}

/**
 * A folder of playlists.
 *
 * An organisational layer only — a folder never owns tracks, a playlist points
 * at one. `parentId` makes nesting representable so it needs no later
 * migration, but nothing creates a nested folder and the rail draws one level.
 */
export interface Folder {
  id: string;
  name: string;
  parentId: string;
}

export interface QueueState {
  current: string | null;
  tracks: string[];
  /** What plays next, so the UI need not ask again. */
  next: string | null;
}

/** What the audio thread is doing. "loading" is a separate flag — fetching and
 *  decoding is the shell's business, not the device's. */
export type PlaybackStatus = "idle" | "playing" | "paused";

export interface PlaybackState {
  href: string | null;
  /** Resolved from the library rows; empty when nothing is playing. */
  title: string;
  artist: string;
  status: PlaybackStatus;
  /** Fetching and decoding — seconds on a cold cache. */
  loading: boolean;
  position: number;
  duration: number;
  volume: number;
  error: string | null;
  /** False when the machine has no output device at all. */
  available: boolean;
  /** True while a beat-matched mix is arranged or under way (TD-25). */
  mixing: boolean;
  /** Peak output level, 0–1. Drives the mark's `energy` on Now Playing. */
  level: number;
  /** Fraction of output energy above 1500 Hz, 0–1. Drives how fast the mark
   *  turns: bright music turns faster than dark music at the same loudness. */
  brightness: number;
  /** Seconds between the beats either side of the playhead. 0 = no usable
   *  grid, and the mark stays on its steady rate rather than guessing. */
  beatPeriod: number;
  /** Track-seconds at which the next beat lands. A position rather than a
   *  countdown, so it stays true between polls and the UI can run its own
   *  clock off it. 0 with no usable grid. */
  nextBeat: number;
  /** Where the playing track sits in the planned set. `setTotal` 0 means
   *  nothing is planned — shuffle, or a queue the pathfinder did not build. */
  setIndex: number;
  setTotal: number;
  /** Energy the curve wants the set to be at here, 0–1. Drives the mark's
   *  hue, so the logo says where the set has got to. */
  setEnergy: number;
  /** Envelope peaks for the playing track. Empty until it has been analysed
   *  at the current version — draw a plain bar rather than inventing one. */
  waveform: number[];
  /** What plays after this, so Now Playing needs no second call. */
  nextTitle: string;
  nextArtist: string;
  nextAlbum: string;
  /**
   * The next track's href, for its artwork.
   *
   * The cover is fetched separately, through the same tile-sized path a row
   * uses — carrying a 300 KB data URI on a four-times-a-second poll is the
   * mistake `trackThumb` exists to avoid.
   */
  nextHref: string;
  /** Cover art as a data URI, when the file carried one. */
  cover: string | null;
  /**
   * What the DJ is conducting over — a playlist, album, artist, genre or
   * smart group name. Empty means the whole library.
   *
   * Set by whatever list `playTracks` was called from, and it is the pool the
   * planner actually chooses within, not a caption. See `AppState::scope`.
   */
  scope: string;
}

export interface TrackMeta {
  href: string;
  bpm: number;
  musicalKey: string;
  outroKey: string;
  introKey: string;
  energyLevel: number;
  genre: string;
}

export type Curve = "build" | "chill" | "wave" | "flat";

/**
 * The three ways out of the track that is playing.
 *
 * `follow` is the set: whatever the planner already has queued next. The other
 * two are departures from it — `stay` holds this energy, `switch` leaves it.
 * There is no fourth state and no rotation between them; the set follows itself
 * until someone says otherwise.
 */
export type Exit = "stay" | "follow" | "switch";

/** One option for what plays next. */
export interface MixCandidate {
  href: string;
  title: string;
  artist: string;
  bpm: number;
  key: string;
  exit: Exit;
  /** The word on the card: STAY, FOLLOW, SWITCH. */
  label: string;
  /** The mix the engine would actually perform to get there. */
  transition: string;
  /** Whether this is what is actually queued next. */
  selected: boolean;
  /** The sleeve, as the design's alternates carry one. */
  cover: string | null;
}

export interface RemoteConfig {
  url: string;
  username: string;
  folder: string;
}

export interface Settings {
  remote: RemoteConfig;
  baseFontSize: number;
  uiScale: number;
  themeMode: "preset" | "custom";
  theme: string;
  customBaseColor: string;
  customAccentColor: string;
  headphoneProfile: string;
  headphoneCalibrationEnabled: boolean;
  /** Manual corrections, keyed by href. See the note on setBpmOverride. */
  bpmOverrides: Record<string, number>;
  /** Album artwork chosen by hand, keyed by album identity (title + folder). */
  albumArt: Record<string, string>;
  /** Whether a looked-up cover outranks the file's own embedded artwork. */
  preferLookedUpArt: boolean;
  /**
   * Whether the library hides second and later copies of a recording.
   *
   * Off by default: these are your own files, and a library quietly showing
   * fewer tracks than are on disk is one you cannot trust. Hiding is a view,
   * not a deletion. The Vibe DJ excludes duplicates regardless — two copies of
   * one track are identical in tempo, key and intensity, so a set free to use
   * both will mix a record into itself.
   */
  hideDuplicates: boolean;
  /** Ceiling on the local audio cache, in bytes. */
  cacheMaxBytes: number;
  /**
   * Whether the app may look up lyrics and artwork from public services.
   *
   * Off by default. Everything else the app knows is worked out on the device;
   * a lookup sends the artist and title of what is playing to a third party.
   */
  metadataLookupEnabled: boolean;
  /**
   * The Vibe Limit: the energy difference between consecutive tracks past
   * which the pathfinder starts paying a steep penalty (ai_dj_workflow.md §6).
   *
   * Strict keeps a set at one intensity; loose permits drops and climbs.
   */
  vibeLimit: number;
  /**
   * Whether this device announces itself on the local network.
   *
   * Off by default, for the same reason `metadataLookupEnabled` is: a beacon
   * every five seconds tells everyone on the network that this machine is
   * here, on whatever network it happens to be joined to.
   */
  syncEnabled: boolean;
  /**
   * Whether the DJ conducts the set, or the queue plays in order.
   *
   * Persisted and read by the backend, which is the half that decides what
   * plays next. It was component state until 2026-08-17, so the supervisor had
   * never heard of it and the "DJ" could only re-order a queue someone else had
   * already built — a set of one track repeated forever.
   */
  djMode: boolean;
  /**
   * Where the set is going, as `pathfinder.rs` names the curves.
   *
   * Persisted for the same reason `djMode` is: the playback thread extends the
   * set when the queue runs short, and it does that whether or not the Vibe
   * screen is open.
   */
  curve: Curve;
}

/** The band the Vibe Limit is offered over. Matches `settings.rs`. */
export const MIN_VIBE_LIMIT = 0.1;
export const MAX_VIBE_LIMIT = 1.0;

/**
 * Data files that could not be read at startup and were moved aside.
 *
 * Empty on every normal launch. Each string is a sentence to show a person
 * verbatim: which file, why, and where the bytes were kept. The app starts on
 * a default either way — refusing to open because one of fourteen files is
 * damaged is a worse answer — so this is the only thing that distinguishes
 * "you have no playlists" from "your playlists could not be read".
 */
export function startupProblems(): Promise<string[]> {
  return invoke<string[]>("startup_problems");
}

/**
 * Choose the curve, and re-plan the set along it.
 *
 * Saved rather than passed per-call, because the playback thread extends the
 * set on its own when the queue runs short and has to know where the set is
 * going without a screen being open. Setting it truncates everything after the
 * track playing — that tail was a route to the old destination — and plans a
 * new one.
 */
export function setCurve(curve: Curve): Promise<Settings> {
  return invoke<Settings>("set_curve", { curve });
}

/** Set the Vibe Limit. Out-of-range values are clamped to the band. */
export function setVibeLimit(limit: number): Promise<Settings> {
  return invoke<Settings>("set_vibe_limit", { limit });
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(cmd, args);
}

/**
 * Filter, sort and group in one round trip.
 *
 * One call rather than three because the table re-runs this per keystroke, and
 * because the filter predicate is the same one a smart playlist uses — running
 * them separately would let the two disagree about membership.
 */
export function libraryView(view: LibraryView = {}): Promise<LibrarySection[]> {
  return invoke<LibrarySection[]>("library_view", { view });
}

/** The albums or artists in the library, one entry each. */
export function libraryEntities(view: LibraryView = {}): Promise<LibraryEntity[]> {
  return invoke<LibraryEntity[]>("library_entities", { view });
}

/**
 * The embedded cover for one track.
 *
 * Fetched per card rather than carried on every row: artwork is capped at 2 MB
 * and a library of several hundred tracks would otherwise move hundreds of
 * megabytes through IPC on every keystroke.
 */
export function trackCover(href: string): Promise<string | null> {
  return invoke<string | null>("track_cover", { href });
}

/**
 * The same cover at tile size (PERF-004).
 *
 * A song row draws artwork at 48 px and the queue's tile at 56 px, and handing
 * either the full stored cover is what made opening Songs pause: 516 covers
 * averaging 281 KB, a screenful of which is about 7 MB through IPC to draw
 * postage stamps. This is a 128 px version, generated once and cached beside
 * the original.
 */
export function trackThumb(href: string): Promise<string | null> {
  return invoke<string | null>("track_thumb", { href });
}

/**
 * The three ways out of the track playing.
 *
 * One candidate per kind — each the best of its kind rather than the best
 * overall — so the three are genuinely different exits rather than three
 * shades of the same one.
 */
export function mixCandidates(): Promise<MixCandidate[]> {
  return invoke<MixCandidate[]>("mix_candidates");
}

/**
 * Take one of them, and re-plan the set behind it.
 *
 * The curve owns the destination and the match type owns the next step, so
 * overriding one step re-searches the tail along the same curve rather than
 * abandoning the arc.
 */
export function chooseNext(href: string, curve: Curve): Promise<void> {
  return invoke<void>("choose_next", { href, curve });
}

export function playlists(): Promise<Playlist[]> {
  return invoke<Playlist[]>("playlists");
}

export function createPlaylist(
  name: string,
  folderId?: string,
): Promise<Playlist> {
  return invoke<Playlist>("create_playlist", { name, folderId: folderId ?? null });
}

// --- Lyrics and artwork from public services --------------------------------

/** One line of time-aligned lyrics. */
export interface LyricLine {
  /** Seconds from the start of the track. */
  time: number;
  text: string;
}

export interface Lyrics {
  /** Whether `lines` carry usable timings. */
  synced: boolean;
  lines: LyricLine[];
  /** The unaligned text, empty when a synced version was available. */
  plain: string;
}

/**
 * What is known about a track from outside this device.
 *
 * Kept apart from analysis and tags, which are what this device measured and
 * what the file itself carries. A screen has to be able to say which is which.
 */
export interface LookedUp {
  lyrics: Lyrics | null;
  artistImage: string;
  albumArt: string;
  genre: string;
  /** Whether a lookup has been made for this track at all. */
  attempted: boolean;
  /** Whether the setting permits making one. */
  allowed: boolean;
}

/** What has already been looked up. Never makes a request. */
export function trackLookup(href: string): Promise<LookedUp> {
  return invoke<LookedUp>("track_lookup", { href });
}

/** Look a track up, and remember what came back. Rejects when switched off. */
export function lookUpTrack(href: string, force = false): Promise<LookedUp> {
  return invoke<LookedUp>("look_up_track", { href, force });
}

/**
 * A looked-up image as a data URI, from the file it was cached in.
 *
 * Takes the URL rather than an href because one sleeve serves every track on
 * the album. Only a URL a previous lookup stored is served.
 */
export function lookedUpImage(url: string): Promise<string | null> {
  return invoke<string | null>("looked_up_image", { url });
}

/**
 * Artwork for an album, resolved.
 *
 * Three sources in order: a cover chosen by hand, then the file's own embedded
 * picture, then a looked-up one for a file that carries none.
 * `preferLookedUpArt` swaps the last two.
 *
 * `lead` is any track on the album — an album's identity is its title plus the
 * folder its tracks live in, so any of them resolves the same cover.
 */
export interface AlbumArt {
  /** The picture, as a data URI. `null` when there is none to show. */
  src: string | null;
  /** Whether this is a cover chosen by hand rather than the file's own.
   *  Answered by the backend, because the album key's format is its business. */
  chosen: boolean;
}

export function albumCover(album: string, lead: string): Promise<AlbumArt> {
  return invoke<AlbumArt>("album_cover", { album, lead });
}

/**
 * Search the services for an album's cover and use what comes back.
 *
 * Sends the artist and album to Deezer. Not gated on `metadataLookupEnabled`:
 * that setting governs whether the app may go looking on its own, and pressing
 * this is the asking it exists to require. Rejects when nothing matched, which
 * is a real outcome and worth showing.
 */
export function findAlbumArt(
  album: string,
  artist: string,
  lead: string,
): Promise<AlbumArt> {
  return invoke<AlbumArt>("find_album_art", { album, artist, lead });
}

/** Forget a hand-chosen cover and go back to what the file carries. */
export function clearAlbumArt(album: string, lead: string): Promise<AlbumArt> {
  return invoke<AlbumArt>("clear_album_art", { album, lead });
}

/** Emitted on `identify-progress`, once per track and once at the end. */
export interface IdentifyProgress {
  done: number;
  total: number;
  /** The track just finished. */
  title: string;
  /** Tempos corrected so far — the point of the exercise. */
  corrected: number;
  /** Tracks Deezer had a genre for. */
  genres: number;
  finished: boolean;
}

/**
 * Ask Deezer about every track in the library.
 *
 * **Sends the artist and title of the whole library to a third party.** It is
 * the largest disclosure the app ever makes, which is why it is a button with
 * its own sentence rather than something the automatic-lookup setting quietly
 * turns on.
 *
 * What it is for is the tempo octave: a beat tracker is reliable about the
 * pulse and unreliable about whether a listener counts it at 87 or 174, and
 * nothing measurable on this device settles it. Deezer's number is never
 * adopted as the tempo — it only chooses between octaves of the tempo measured
 * here, and only once the durations agree the two are the same recording.
 *
 * Returns as soon as the pass starts; watch `identify-progress`.
 */
export function identifyLibrary(): Promise<void> {
  return invoke<void>("identify_library");
}

/** Turn the DJ on or off. Persisted; the backend acts on it. */
export function setDjMode(enabled: boolean): Promise<Settings> {
  return invoke<Settings>("set_dj_mode", { enabled });
}

/** Whether looked-up artwork outranks the file's own, library-wide. */
export function setPreferLookedUpArt(enabled: boolean): Promise<Settings> {
  return invoke<Settings>("set_prefer_looked_up_art", { enabled });
}

export function setHideDuplicates(enabled: boolean): Promise<Settings> {
  return invoke<Settings>("set_hide_duplicates", { enabled });
}

/**
 * A looked-up portrait for an artist, as a data URI.
 *
 * Keyed by name: any track by that artist will do, so the first one carrying a
 * picture answers for all of them. `null` whenever nothing was looked up or
 * lookups are off, which is the ordinary case.
 */
export function artistPortrait(name: string): Promise<string | null> {
  return invoke<string | null>("artist_portrait", { name });
}

/** Turn lookups on or off. Off also forgets everything already found. */
export function setMetadataLookup(enabled: boolean): Promise<Settings> {
  return invoke<Settings>("set_metadata_lookup", { enabled });
}

// --- Sync with another device on the same network (SYNC-001 to SYNC-005) ----

export type DeviceKind = "desktop" | "phone" | "tablet" | "unknown";

/** A device seen on the network. */
export interface SyncPeer {
  id: string;
  name: string;
  kind: DeviceKind;
  /** Host and port, as this device heard it — never as the peer claimed it. */
  address: string;
  lastSeen: number;
}

/** A device this one has agreed to sync with. */
export interface TrustedDevice {
  id: string;
  name: string;
  kind: DeviceKind;
  pairedAt: number;
}

export interface SyncProgress {
  running: boolean;
  peer: string;
  /** What is moving right now. */
  file: string;
  done: number;
  total: number;
  bytes: number;
  /** Seconds since it started — the rate is divided here, not pushed stale. */
  elapsed: number;
  error: string;
}

export interface SyncView {
  /** Whether local sync is switched on at all. Off is the shipped default. */
  enabled: boolean;
  deviceId: string;
  deviceName: string;
  /** On the network, paired or not. */
  discovered: SyncPeer[];
  trusted: TrustedDevice[];
  /** The code this device is showing, if it is showing one. */
  pin: string | null;
  pairingWith: string | null;
  progress: SyncProgress;
}

/** What may move. Both default to true. */
export interface SyncWhat {
  tracks: boolean;
  playlists: boolean;
}

/**
 * Whether hardware media keys will reach this build.
 *
 * False on macOS when the app is running from `tauri dev` rather than a built
 * `.app` — macOS routes media keys to the Now Playing *application*, and a
 * bare binary is not one.
 */
export function mediaKeysAvailable(): Promise<boolean> {
  return invoke<boolean>("media_keys_available");
}

/** Turn local-network sync on or off. Off also forgets who was paired. */
export function setSyncEnabled(enabled: boolean): Promise<Settings> {
  return invoke<Settings>("set_sync_enabled", { enabled });
}

export function syncView(): Promise<SyncView> {
  return invoke<SyncView>("sync_view");
}

/**
 * Show a pairing code for one device to type in.
 *
 * Bound to that device, so a code on screen is not an invitation to everything
 * else on the network that can see it.
 */
export function openPairing(peerId: string): Promise<string> {
  return invoke<string>("open_pairing", { peerId });
}

export function cancelPairing(): Promise<void> {
  return invoke<void>("cancel_pairing");
}

/** Type the code the other device is showing. Resolves to its name. */
export function pairWith(peerId: string, pin: string): Promise<string> {
  return invoke<string>("pair_with", { peerId, pin });
}

export function forgetPeer(peerId: string): Promise<boolean> {
  return invoke<boolean>("forget_peer", { peerId });
}

/** Start a sync. Returns as soon as it has begun; watch `syncView` for the rest. */
export function syncWith(peerId: string, what?: SyncWhat): Promise<void> {
  return invoke<void>("sync_with", { peerId, what: what ?? null });
}

/** What a round trip to the shared document on the server changed. */
export interface SharedSyncResult {
  playlistsAdded: number;
  playlistsExtended: number;
  foldersAdded: number;
  temposAdded: number;
  /** Removed here because another device removed them (TD-57). A deletion is
   *  the one edit an additive merge cannot carry, so it travels as a record of
   *  its own and is applied on arrival. */
  playlistsDeleted: number;
  foldersDeleted: number;
  /** True when there was none there and this device wrote the first. */
  created: boolean;
}

/**
 * Pull the shared document from the server, merge it in, and push it back.
 *
 * One call does both halves: pulling without pushing leaves this device's
 * playlists invisible to every other one, and pushing without pulling
 * overwrites theirs.
 */
export function syncSharedDocument(): Promise<SharedSyncResult> {
  return invoke<SharedSyncResult>("sync_shared_document");
}

// --- Smart groups -----------------------------------------------------------

/**
 * What a smart group can hold.
 *
 * Entities, not tracks. That is what makes a group different from a playlist:
 * membership is resolved against the library whenever it is read, so a record
 * added later belongs without anyone maintaining a list. A track dropped on a
 * group is refused for the same reason — it is not one of these.
 */
export type EntityType = "artist" | "album" | "genre";

export interface Entity {
  entityType: EntityType;
  value: string;
}

export interface DynamicGroup {
  id: string;
  name: string;
  entities: Entity[];
}

// --- Downloads --------------------------------------------------------------

/**
 * Keeping a track, as opposed to happening to have one.
 *
 * Everything else in the audio cache is there because something needed to read
 * it once and is dropped when the set moves past it. These are the tracks you
 * asked for: they live outside the evicted directory and stay until removed.
 */
export interface DownloadProgress {
  done: number;
  total: number;
  /** What is being fetched now. Empty when finished. */
  title: string;
  finished: boolean;
  /** Why it stopped early, if it did. */
  error: string;
}

export type Collection = "playlist" | "group";

/** Every track whose audio is kept on this device. */
export function downloadedTracks(): Promise<string[]> {
  return invoke<string[]>("downloaded_tracks");
}

/** Fetch and keep every track in a playlist or smart group.
 *
 *  Returns as soon as the work has started; follow `download-progress`. */
export function downloadCollection(
  kind: Collection,
  id: string,
): Promise<void> {
  return invoke<void>("download_collection", { kind, id });
}

/** Stop keeping them. Returns how many were actually released — a track another
 *  download still wants is kept. */
export function removeDownload(
  kind: Collection,
  id: string,
): Promise<number> {
  return invoke<number>("remove_download", { kind, id });
}

/** How many files are second-or-later copies of a recording. */
export function duplicateCount(): Promise<number> {
  return invoke<number>("duplicate_count");
}

export function dynamicGroups(): Promise<DynamicGroup[]> {
  return invoke<DynamicGroup[]>("dynamic_groups");
}

export function createGroup(name: string): Promise<DynamicGroup> {
  return invoke<DynamicGroup>("create_group", { name });
}

export function renameGroup(id: string, name: string): Promise<boolean> {
  return invoke<boolean>("rename_group", { id, name });
}

export function deleteGroup(id: string): Promise<boolean> {
  return invoke<boolean>("delete_group", { id });
}

/** Idempotent: adding an entity a group already holds changes nothing. */
export function addToGroup(
  id: string,
  entityType: EntityType,
  value: string,
): Promise<boolean> {
  return invoke<boolean>("add_to_group", { id, entityType, value });
}

export function removeFromGroup(
  id: string,
  entityType: EntityType,
  value: string,
): Promise<boolean> {
  return invoke<boolean>("remove_from_group", { id, entityType, value });
}

export function reorderGroups(from: number, to: number): Promise<boolean> {
  return invoke<boolean>("reorder_groups", { from, to });
}

/** Every track the group resolves to right now. */
export function groupTracks(id: string): Promise<Row[]> {
  return invoke<Row[]>("group_tracks", { id });
}

// --- Playlist folders -------------------------------------------------------

export function playlistFolders(): Promise<Folder[]> {
  return invoke<Folder[]>("playlist_folders");
}

export function createFolder(name: string): Promise<Folder> {
  return invoke<Folder>("create_folder", { name });
}

export function renameFolder(id: string, name: string): Promise<boolean> {
  return invoke<boolean>("rename_folder", { id, name });
}

/** Delete a folder. The playlists inside it move back to the top level. */
export function deleteFolder(id: string): Promise<boolean> {
  return invoke<boolean>("delete_folder", { id });
}

/** File a playlist into a folder, or out of one with an empty `folderId`. */
export function setPlaylistFolder(
  id: string,
  folderId: string,
): Promise<boolean> {
  return invoke<boolean>("set_playlist_folder", { id, folderId });
}

/** Returns how many were actually added — duplicates are skipped. */
export function addTracksToPlaylist(id: string, hrefs: string[]): Promise<number> {
  return invoke<number>("add_tracks_to_playlist", { id, hrefs });
}

/** Whether a password is stored for this account. The password itself never
 *  crosses — only whether it is there, so Settings can say which state it is
 *  in instead of always claiming "unchanged". */
export function hasWebdavPassword(username: string): Promise<boolean> {
  return invoke<boolean>("has_webdav_password", { username });
}

export function renamePlaylist(id: string, name: string): Promise<boolean> {
  return invoke<boolean>("rename_playlist", { id, name });
}

export function deletePlaylist(id: string): Promise<boolean> {
  return invoke<boolean>("delete_playlist", { id });
}

export function removePlaylistTrack(id: string, index: number): Promise<boolean> {
  return invoke<boolean>("remove_playlist_track", { id, index });
}

export function reorderPlaylistTrack(
  id: string,
  from: number,
  to: number,
): Promise<boolean> {
  return invoke<boolean>("reorder_playlist_track", { id, from, to });
}

/**
 * A playlist's tracks as table rows, in playlist order.
 *
 * Rows rather than hrefs so the screen can show what every other table shows
 * without rebuilding tag and analysis lookup on this side. Tracks whose files
 * have left the library are omitted, so this can be shorter than the playlist.
 */
export function playlistRows(id: string): Promise<Row[]> {
  return invoke<Row[]>("playlist_rows", { id });
}

export function queueState(): Promise<QueueState> {
  return invoke<QueueState>("queue_state");
}

/**
 * Replace the queue and start playing.
 *
 * Returns as soon as the queue is set, not when audio starts: the track still
 * has to be fetched and decoded, which is seconds on a cold cache. Watch
 * `playbackState().loading` for that.
 */
/**
 * Play `hrefs`, starting at `start`, and conduct within them.
 *
 * `scope` names the list this came from and is what confines the DJ to it —
 * an album, a playlist, a genre. Omitted, the set is the library and the DJ
 * may roam it, which is what playing from an unfiltered list means.
 */
export function playTracks(
  hrefs: string[],
  start?: string,
  scope?: string,
): Promise<void> {
  return invoke<void>("play_tracks", {
    hrefs,
    start: start ?? null,
    scope: scope ?? null,
  });
}

export function nextTrack(): Promise<string | null> {
  return invoke<string | null>("next_track");
}

export function previousTrack(): Promise<string | null> {
  return invoke<string | null>("previous_track");
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

export function playbackState(): Promise<PlaybackState> {
  return invoke<PlaybackState>("playback_state");
}

export function pausePlayback(): Promise<void> {
  return invoke<void>("pause_playback");
}

export function resumePlayback(): Promise<void> {
  return invoke<void>("resume_playback");
}

/** Stop and return to the start — not a pause. */
export function stopPlayback(): Promise<void> {
  return invoke<void>("stop_playback");
}

export function seek(seconds: number): Promise<void> {
  return invoke<void>("seek", { seconds });
}

/** Master volume, 0 to 1. Applied after the mixer, so a transition's own gain
 *  automation is unaffected. */
export function setVolume(volume: number): Promise<void> {
  return invoke<void>("set_volume", { volume });
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

export interface QueueEntry {
  href: string;
  title: string;
  artist: string;
  /** 0 when unknown — not "0 BPM". */
  bpm: number;
  key: string;
  current: boolean;
}

export type RepeatMode = "off" | "all" | "one";

export interface QueueView {
  entries: QueueEntry[];
  repeat: RepeatMode;
  shuffled: boolean;
  current: number | null;
  /** Seconds still to come. Unanalysed tracks contribute nothing rather than
   *  a guess, so this is a floor, not an estimate. */
  remainingSecs: number;
}

export function queueView(): Promise<QueueView> {
  return invoke<QueueView>("queue_view");
}

export function removeFromQueue(href: string): Promise<boolean> {
  return invoke<boolean>("remove_from_queue", { href });
}

/** Reorder by index. Returns false for an out-of-range or pointless move. */
export function moveInQueue(from: number, to: number): Promise<boolean> {
  return invoke<boolean>("move_in_queue", { from, to });
}

/** Put a track next without disturbing the rest of the order. */
export function playNext(href: string): Promise<boolean> {
  return invoke<boolean>("play_next", { href });
}

/** What happens at the end of the queue. Default is "all", the behaviour the
 *  queue had before there was any choice. */
export function setRepeat(mode: RepeatMode): Promise<void> {
  return invoke<void>("set_repeat", { mode });
}

/** Shuffle or restore the order. Returns false when there was nothing to do —
 *  a queue of fewer than two tracks, or an unshuffle when not shuffled. */
export function setShuffled(shuffled: boolean): Promise<boolean> {
  return invoke<boolean>("set_shuffled", { shuffled });
}

// ---------------------------------------------------------------------------
// Vibe DJ
// ---------------------------------------------------------------------------

export interface VibePath {
  hrefs: string[];
  /** Library tracks eligible to be planned with — analysed, with a tempo. */
  considered: number;
  /** How many were passed over for want of analysis. */
  skipped: number;
}

/**
 * Order the library along an energy and tempo curve, starting from one track.
 *
 * Built from the analysis the backend already holds, rather than from a map the
 * frontend would have to assemble — see `moodPath`, which takes the map and is
 * kept for tests.
 */
export function vibePath(start: string, curve: Curve): Promise<VibePath> {
  return invoke<VibePath>("vibe_path", { start, curve });
}

export interface BlendPreview {
  fromTitle: string;
  toTitle: string;
  fromBpm: number;
  toBpm: number;
  fromKey: string;
  toKey: string;
  /** Tempo stretch the incoming deck takes, as a percentage. */
  shiftPercent: number;
  /** Loudness difference in LU. */
  gainDelta: number;
  /** Whether the engine would actually accept this as a beat-matched mix. */
  matchable: boolean;
  /** Why not, when it would not. */
  reason: string;
  /** Which of the three mixes this pair would get (TD-27). */
  transition: string;
}

/** Describe the mix between what is playing and what is next, or null when
 *  there is no pair to describe. */
export function blendPreview(): Promise<BlendPreview | null> {
  return invoke<BlendPreview | null>("blend_preview");
}

// ---------------------------------------------------------------------------
// Liner notes
// ---------------------------------------------------------------------------

export interface TrackDetails {
  href: string;
  title: string;
  artist: string;
  album: string;
  year: number;
  genre: string;
  /** False until analysed — show that rather than a column of zeroes. */
  analysed: boolean;
  bpm: number;
  bpmIsManual: boolean;
  key: string;
  lufs: number;
  duration: number;
  cueIn: number;
  cueOut: number;
  energy: number;
  beats: number;
  waveform: number[];
  hrefPath: string;
  cached: boolean;
  /** Why this track cannot be played, when it cannot (TD-12). */
  unplayable: string | null;
  cover: string | null;
  /** The file's own comment field — the nearest thing to sleeve notes a file
   *  actually carries. */
  notes: string | null;
  /** True when any of this came from tags rather than the path. */
  tagged: boolean;
}

export function trackDetails(href: string): Promise<TrackDetails> {
  return invoke<TrackDetails>("track_details", { href });
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

export interface Facet {
  label: string;
  count: number;
}

export interface SearchResults {
  /** The single best row, shown larger. Excluded from `tracks`. */
  top: Row | null;
  tracks: Row[];
  artists: Facet[];
  albums: Facet[];
  playlists: Playlist[];
  /** Total matches before truncation. */
  total: number;
}

export function search(query: string): Promise<SearchResults> {
  return invoke<SearchResults>("search", { query });
}

// ---------------------------------------------------------------------------
// Connecting a library
// ---------------------------------------------------------------------------

export interface ScanReport {
  tracks: number;
  /** Directories visited, so a slow scan reports progress honestly. */
  directories: number;
  /**
   * Folders the scan could not read and walked past.
   *
   * Reported rather than swallowed: a scan that skipped half a library still
   * says "found 40 tracks", and that is indistinguishable from a library with
   * 40 tracks in it.
   */
  unreadable: number;
}

export interface AnalysisStatus {
  analysed: number;
  total: number;
  /**
   * Whether a pass is running right now, whoever started it.
   *
   * Analysis begins by itself after a scan, so "did someone press Analyse" is
   * no longer the same question as "is it running" — and the screen used to
   * answer the second by checking the first, which left an automatic pass
   * invisible.
   */
  running: boolean;
  /** The track it is on. Empty between tracks and when nothing is running. */
  current: string;
  /** Why the last pass ended early, if it did. Empty otherwise. */
  stoppedBecause: string;
}

export interface Analysis {
  bpm: number;
  key: string;
  introKey: string;
  outroKey: string;
  /** Perceived energy, 0–1, from integrated loudness. Not the stored
   *  `energy` field, which is a consistency ratio — see
   *  `vapor_library::intensity_from_lufs`. */
  energy: number;
  beats: number[];
  cueIn: number;
  cueOut: number;
  lufs: number;
  duration: number;
  version: number;
}

/** Emitted on the `analysis-progress` event, once per track. */
export interface AnalysisProgress {
  done: number;
  total: number;
  /** The track just finished, so one row can update rather than the table. */
  href: string;
  analysis: Analysis | null;
  /** Present when this track failed. The pass continues either way. */
  error: string | null;
}

/**
 * Emitted on the `bpm-retrack` event when a corrected tempo has its beat grid
 * rebuilt.
 *
 * A correction changes where the beats are, not just the number on the row, so
 * the backend re-runs beat tracking against it. That needs the audio, so it can
 * take seconds on a track that is not downloaded yet — and until it lands the
 * track mixes on a grid assuming a beat at zero and a tempo that never wavers.
 * One of these arrives when the job starts (`beats` and `error` both null) and
 * one when it finishes.
 */
export interface BpmRetrack {
  href: string;
  /** The tempo being tracked against — the correction, or the detected tempo
   *  again when a correction has just been cleared. */
  bpm: number;
  /** Beats found, once it is done. */
  beats: number | null;
  /** Why it could not be done. The correction itself is saved either way. */
  error: string | null;
}

export interface CacheStatus {
  bytes: number;
  maxBytes: number;
  /** How many of the library's tracks are actually on the device. */
  tracksCached: number;
  tracksTotal: number;
  location: string;
}

/**
 * Point the app at a server. Returns the settings as stored.
 *
 * The password is deliberately not a parameter — it goes to the OS keychain
 * via `saveWebdavPassword` and never enters the settings file.
 */
export function setRemoteConfig(
  url: string,
  username: string,
  folder: string,
): Promise<Settings> {
  return invoke<Settings>("set_remote_config", { url, username, folder });
}

/** Store the password in the OS keychain, keyed by username. */
export function saveWebdavPassword(
  username: string,
  password: string,
): Promise<void> {
  return invoke<void>("save_webdav_password", { username, password });
}

/** Walk the server and rebuild the index. Minutes on a large library. */
export function scanLibrary(): Promise<ScanReport> {
  return invoke<ScanReport>("scan_library");
}

/**
 * Analyse everything not already done.
 *
 * Returns as soon as the pass starts, not when it finishes — it is roughly
 * 0.5 s per track. Progress arrives on the `analysis-progress` event.
 */
export function analyseLibrary(): Promise<void> {
  return invoke<void>("analyse_library");
}

export function analysisStatus(): Promise<AnalysisStatus> {
  return invoke<AnalysisStatus>("analysis_status");
}

export function cancelAnalysis(): Promise<void> {
  return invoke<void>("cancel_analysis");
}

// ---------------------------------------------------------------------------
// Your data
// ---------------------------------------------------------------------------

export function cacheStatus(): Promise<CacheStatus> {
  return invoke<CacheStatus>("cache_status");
}

/** Drop one track's local copy, keeping its analysis. */
export function evictTrack(href: string): Promise<void> {
  return invoke<void>("evict_track", { href });
}

/** Where the app keeps everything — the claim "your data is local", shown. */
export function dataLocation(): Promise<string> {
  return invoke<string>("data_location");
}

export interface DataRow {
  label: string;
  path: string;
  bytes: number;
  /** False for anything on the server rather than this device. */
  local: boolean;
}

/** Itemise what is stored, so the sovereignty claim can be checked rather than
 *  taken on trust. */
export function dataBreakdown(): Promise<DataRow[]> {
  return invoke<DataRow[]>("data_breakdown");
}

/** Open the data directory in the system file manager. */
export function revealDataFolder(): Promise<void> {
  return invoke<void>("reveal_data_folder");
}

/** Delete everything stored, including the keychain entry. */
export function deleteAllData(): Promise<void> {
  return invoke<void>("delete_all_data");
}

/** Order a set of tracks along an energy/tempo curve. */
export function moodPath(
  tracks: Record<string, TrackMeta>,
  start: string,
  curve: Curve,
): Promise<string[]> {
  return invoke<string[]>("mood_path", { req: { tracks, start, curve } });
}

export function settings(): Promise<Settings> {
  return invoke<Settings>("settings");
}

/**
 * Correct a track's BPM by hand.
 *
 * This exists because tempo detection agrees with the previous analysis on
 * ~81% of a real library and the residual is metrical error — half, double or
 * three-quarter time. Rather than block on solving that, the app lets a person
 * fix it. Pass 0 to clear.
 */
export function setBpmOverride(href: string, bpm: number): Promise<void> {
  return invoke<void>("set_bpm_override", { href, bpm });
}

/**
 * Change how much of the device the audio cache may use.
 *
 * Returns the bound actually applied, which may be larger than asked for: the
 * core refuses a cache too small to hold a track, since that fetches, evicts
 * and re-fetches the same audio while reporting itself as working.
 */
export function setCacheMaxBytes(bytes: number): Promise<number> {
  return invoke<number>("set_cache_max_bytes", { bytes });
}

/**
 * Empty the audio cache, keeping analysis, playlists and settings.
 *
 * Returns the bytes freed. Distinct from `deleteAllData`: cached audio is the
 * only re-fetchable part of the data directory and the only part that gets
 * large, so reclaiming space should not cost ten minutes of analysis.
 */
export function clearAudioCache(): Promise<number> {
  return invoke<number>("clear_audio_cache");
}

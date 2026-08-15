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
}

export interface Playlist {
  id: string;
  name: string;
  customCoverPath: string;
  tracks: string[];
  folderId: string;
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
  /** Envelope peaks for the playing track. Empty until it has been analysed
   *  at the current version — draw a plain bar rather than inventing one. */
  waveform: number[];
  /** What plays after this, so Now Playing needs no second call. */
  nextTitle: string;
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
  /** Ceiling on the local audio cache, in bytes. */
  cacheMaxBytes: number;
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

export function playlists(): Promise<Playlist[]> {
  return invoke<Playlist[]>("playlists");
}

export function createPlaylist(name: string): Promise<Playlist> {
  return invoke<Playlist>("create_playlist", { name });
}

/** Returns how many were actually added — duplicates are skipped. */
export function addTracksToPlaylist(id: string, hrefs: string[]): Promise<number> {
  return invoke<number>("add_tracks_to_playlist", { id, hrefs });
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
export function playTracks(hrefs: string[], start?: string): Promise<void> {
  return invoke<void>("play_tracks", { hrefs, start: start ?? null });
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

export interface QueueView {
  entries: QueueEntry[];
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

// ---------------------------------------------------------------------------
// Vibe DJ
// ---------------------------------------------------------------------------

export interface VibePath {
  hrefs: string[];
  /** Library tracks eligible to be planned with — analysed, with a tempo. */
  considered: number;
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
}

export interface AnalysisStatus {
  analysed: number;
  total: number;
}

export interface Analysis {
  bpm: number;
  key: string;
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

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

export function playTracks(hrefs: string[], start?: string): Promise<void> {
  return invoke<void>("play_tracks", { hrefs, start: start ?? null });
}

export function nextTrack(): Promise<string | null> {
  return invoke<string | null>("next_track");
}

export function previousTrack(): Promise<string | null> {
  return invoke<string | null>("previous_track");
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

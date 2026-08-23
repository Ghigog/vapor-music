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
// Types — generated from the Rust structs, not written here
// ---------------------------------------------------------------------------
//
// These used to be hand-written to "mirror the serde shapes in
// src-tauri/src/lib.rs". Nothing enforced the mirror, and it broke: `Playlist`
// went over the wire as `folder_id` while the rail filtered on `folderId`, so
// every playlist read as filed nowhere and the sidebar rendered a heading with
// nothing under it.
//
// `ts-rs` derives these from the Rust definitions, reading the same
// `#[serde(rename_all)]` attributes serde itself reads, so the two cannot
// disagree. They are checked in, and `npm run types:check` fails if the
// committed files are not what the current Rust produces. Do not edit them by
// hand — edit the struct.
// Imported as well as re-exported: `export ... from` forwards a name without
// binding it locally, and the command signatures below use these.
import type { Settings } from "./generated/Settings";
import type { Appearance } from "./theme";
import type { RemoteConfig } from "./generated/RemoteConfig";
import type { LocalFolder } from "./generated/LocalFolder";
export type { LocalFolder };
import type { DownloadProgress } from "./generated/DownloadProgress";
import type { IdentifyProgress } from "./generated/IdentifyProgress";
import type { Source } from "./generated/Source";
import type { Row } from "./generated/Row";
import type { Playlist } from "./generated/Playlist";
import type { Folder } from "./generated/Folder";
import type { TrackMeta } from "./generated/TrackMeta";
import type { EntityType } from "./generated/EntityType";
import type { Entity } from "./generated/Entity";
import type { DynamicGroup } from "./generated/DynamicGroup";
import type { AlbumArt } from "./generated/AlbumArt";
import type { AnalysisStatus } from "./generated/AnalysisStatus";
import type { AnalysisFailure } from "./generated/AnalysisFailure";
import type { BlendPreview } from "./generated/BlendPreview";
import type { CacheStatus } from "./generated/CacheStatus";
import type { DataRow } from "./generated/DataRow";
import type { DeviceKind } from "./generated/DeviceKind";
import type { Exit } from "./generated/Exit";
import type { Facet } from "./generated/Facet";
import type { HomeShelves } from "./generated/HomeShelves";
import type { LibraryEntity } from "./generated/LibraryEntity";
import type { LibraryPage } from "./generated/LibraryPage";
import type { LibrarySection } from "./generated/LibrarySection";
import type { LookedUp } from "./generated/LookedUp";
import type { LookupCounts } from "./generated/LookupCounts";
import type { LyricLine } from "./generated/LyricLine";
import type { Lyrics } from "./generated/Lyrics";
import type { MixCandidate } from "./generated/MixCandidate";
import type { PlaybackState } from "./generated/PlaybackState";
import type { QueueEntry } from "./generated/QueueEntry";
import type { QueueState } from "./generated/QueueState";
import type { QueueView } from "./generated/QueueView";
import type { ScanReport } from "./generated/ScanReport";
import type { SearchResults } from "./generated/SearchResults";
import type { Shelf } from "./generated/Shelf";
import type { SharedSyncResult } from "./generated/SharedSyncResult";
import type { SyncProgress } from "./generated/SyncProgress";
import type { SyncView } from "./generated/SyncView";
import type { TrackDetails } from "./generated/TrackDetails";
import type { TrustedDevice } from "./generated/TrustedDevice";
import type { VibePath } from "./generated/VibePath";

/**
 * Narrow the settings' curve to the four the UI draws.
 *
 * `Settings.curve` is a `String` in Rust, not the `Curve` enum, because it also
 * accepts the spellings the Godot build wrote. The hand-written type here used
 * to claim it was the union, which was a guess that happened to hold; the
 * generated type says `string`, which is the truth. So it is narrowed once,
 * here, rather than asserted at each use.
 */
export function asCurve(value: string): Curve {
  return value === "build" || value === "chill" || value === "wave"
    ? value
    : "flat";
}

export type {
  IdentifyProgress,
  DownloadProgress,
  RemoteConfig,
  Settings,
  Source,
  Row,
  Playlist,
  Folder,
  TrackMeta,
  EntityType,
  Entity,
  DynamicGroup,
  AlbumArt,
  AnalysisStatus,
  AnalysisFailure,
  BlendPreview,
  CacheStatus,
  DataRow,
  DeviceKind,
  Exit,
  Facet,
  HomeShelves,
  LibraryEntity,
  LibraryPage,
  LibrarySection,
  LookedUp,
  LookupCounts,
  LyricLine,
  Lyrics,
  MixCandidate,
  PlaybackState,
  QueueEntry,
  QueueState,
  QueueView,
  ScanReport,
  SearchResults,
  Shelf,
  SharedSyncResult,
  SyncProgress,
  SyncView,
  TrackDetails,
  TrustedDevice,
  VibePath,
};

export type SortKey =
  "title" | "artist" | "album" | "genre" | "year" | "bpm" | "key" | "order";

export type GroupBy = "none" | "artist" | "album" | "genre";

export interface LibraryView {
  query?: string;
  sortKey?: SortKey;
  ascending?: boolean;
  groupBy?: GroupBy;
  /** Narrow to one genre, exactly — set when a genre tile is opened. */
  genre?: string;
  /** Narrow to exactly this album. Set by opening one, not by typing. */
  album?: string;
  /** Narrow to exactly this artist. */
  artist?: string;
}

/** What the audio thread is doing. "loading" is a separate flag — fetching and
 *  decoding is the shell's business, not the device's. */
export type PlaybackStatus = "idle" | "playing" | "paused";

export type Curve = "build" | "chill" | "wave" | "flat";

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

async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return tauriInvoke<T>(cmd, args);
}

/**
 * Which slice of an ordered library view to actually fetch.
 *
 * Over the flat result, not over each section: grouping puts headings into one
 * ordered list of rows, and that list is what a scroll position is in terms of.
 */
export interface RowWindow {
  /** First row to fetch, counting from the start of the ordered result. */
  offset?: number;
  /** How many rows to fetch. Omitted is "the rest"; `0` is a count. */
  limit?: number;
}

/**
 * Filter, sort and group in one round trip, and fetch one window of the result.
 *
 * One call rather than three because the table re-runs this per keystroke, and
 * because the filter predicate is the same one a smart playlist uses — running
 * them separately would let the two disagree about membership.
 *
 * The window is AUD-13. Measured at 50,000 rows: the whole view is 17,649,902
 * bytes of JSON and 153 ms of `JSON.parse` on the UI thread, per keystroke; the
 * 300-row window the table actually renders into is 106,032 bytes and 0.27 ms.
 * The ordering stays here — a caller that sorted its own window would sort each
 * window separately and show a different order per screenful, which is a worse
 * bug than the payload.
 */
export function libraryPage(
  view: LibraryView = {},
  window: RowWindow = {},
): Promise<LibraryPage> {
  return invoke<LibraryPage>("library_view", { view, window });
}

/**
 * The whole of a library view, in one go.
 *
 * Still here, and still the right call for the three things that genuinely need
 * every row: queueing a table from a track, resolving an entity a drag is
 * carrying, and playing an album. Those are presses, not keystrokes, so paying
 * the full payload once for one is the trade. Anything that re-reads as
 * somebody types wants `libraryPage`.
 */
export function libraryView(view: LibraryView = {}): Promise<LibrarySection[]> {
  return libraryPage(view).then((page) => page.sections);
}

/** The albums or artists in the library, one entry each. */
export function libraryEntities(
  view: LibraryView = {},
): Promise<LibraryEntity[]> {
  return invoke<LibraryEntity[]>("library_entities", { view });
}

/**
 * What the library screen opens on: four shelves, most played first.
 *
 * One call rather than one per shelf. They are one screen, and four round
 * trips is four chances to paint a half-built page.
 */
export function homeShelves(): Promise<HomeShelves> {
  return invoke<HomeShelves>("home_shelves");
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
  return invoke<Playlist>("create_playlist", {
    name,
    folderId: folderId ?? null,
  });
}

// --- Lyrics and artwork from public services --------------------------------

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
 * Which theme the app draws in.
 *
 * Lands in `Settings.theme`, which held a Godot theme-resource name until the
 * appearance control existed and which nothing in this app ever read. See
 * `lib/theme.ts` for what the three words mean and how they reach the DOM.
 */
export function setAppearance(appearance: Appearance): Promise<Settings> {
  return invoke<Settings>("set_appearance", { appearance });
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

/** A device seen on the network. */
export interface SyncPeer {
  id: string;
  name: string;
  kind: DeviceKind;
  /** Host and port, as this device heard it — never as the peer claimed it. */
  address: string;
  lastSeen: number;
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

// --- Dynamic groups -----------------------------------------------------------

// --- Downloads --------------------------------------------------------------

export type Collection = "playlist" | "group";

/** Every track whose audio is kept on this device. */
export function downloadedTracks(): Promise<string[]> {
  return invoke<string[]>("downloaded_tracks");
}

/** Fetch and keep every track in a playlist or dynamic group.
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
export function removeDownload(kind: Collection, id: string): Promise<number> {
  return invoke<number>("remove_download", { kind, id });
}

/** How many files are second-or-later copies of a recording. */

/**
 * Tracks that have been asked about, against the library size.
 *
 * `fetched` counts tracks *asked about*, not tracks something was found for —
 * a track LRCLIB has never heard of has still been asked, and asking again
 * costs a request and finds nothing. It is what decides whether pressing Fetch
 * would do anything.
 */
export function lookupCounts(): Promise<LookupCounts> {
  return invoke<LookupCounts>("lookup_counts");
}

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
export function addTracksToPlaylist(
  id: string,
  hrefs: string[],
): Promise<number> {
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

export function removePlaylistTrack(
  id: string,
  index: number,
): Promise<boolean> {
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
 *
 * `collection` says which playlist or group it was, when it was one, and is
 * what earns that playlist the listen. A name is not enough for this: a
 * playlist can be renamed without becoming a different playlist, and its count
 * should follow it. Omitted, nothing is credited — which is correct for an
 * album, an artist, or the library at large, since there is no gesture that
 * means "put on Aphex Twin", only playing their records.
 */
export function playTracks(
  hrefs: string[],
  start?: string,
  scope?: string,
  collection?: { kind: Collection; id: string },
): Promise<void> {
  return invoke<void>("play_tracks", {
    hrefs,
    start: start ?? null,
    scope: scope ?? null,
    collection: collection ?? null,
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

export type RepeatMode = "off" | "all" | "one";

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

/** Describe the mix between what is playing and what is next, or null when
 *  there is no pair to describe. */
export function blendPreview(): Promise<BlendPreview | null> {
  return invoke<BlendPreview | null>("blend_preview");
}

// ---------------------------------------------------------------------------
// Liner notes
// ---------------------------------------------------------------------------

export function trackDetails(href: string): Promise<TrackDetails> {
  return invoke<TrackDetails>("track_details", { href });
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

export function search(query: string): Promise<SearchResults> {
  return invoke<SearchResults>("search", { query });
}

// ---------------------------------------------------------------------------
// Connecting a library
// ---------------------------------------------------------------------------

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

/**
 * Walk every configured source and rebuild the index.
 *
 * Folders on this device and the server, merged. `ScanReport.problems` names
 * any source that failed — one unreachable server is a partial library and a
 * message, not a failed scan, because the music on this laptop is still here.
 */
export function scanLibrary(): Promise<ScanReport> {
  return invoke<ScanReport>("scan_library");
}

/**
 * Read music from a folder on this device, alongside anything else configured.
 *
 * The path comes from the native picker. The backend checks it is a readable
 * directory before storing it, and adding one already configured is a no-op —
 * so a double click is not an error. Returns the folders as stored.
 *
 * Nothing is read here: run {@link scanLibrary} next.
 */
export function addLocalFolder(path: string): Promise<LocalFolder[]> {
  return invoke<LocalFolder[]>("add_local_folder", { path });
}

/**
 * Stop reading a folder. The files are untouched — this is the library
 * forgetting where to look, and the tracks leave at the next scan.
 */
export function removeLocalFolder(id: string): Promise<LocalFolder[]> {
  return invoke<LocalFolder[]>("remove_local_folder", { id });
}

/** The folders on this device the library reads from. */
export function localFolders(): Promise<LocalFolder[]> {
  return invoke<LocalFolder[]>("local_folders");
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

/**
 * Every track analysis could not describe, permanent failures first.
 *
 * Empty for a healthy library. A non-empty list with the count short of the
 * total is the case this exists for: the tracks in it are the difference.
 */
export function analysisFailures(): Promise<AnalysisFailure[]> {
  return invoke<AnalysisFailure[]>("analysis_failures");
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

/**
 * Empty the cover art, keeping analysis, playlists and settings.
 *
 * Returns the bytes freed. Sibling to `clearAudioCache` and not the same
 * bargain: audio comes back the next time a track is played, artwork comes
 * back when the library is analysed again. Say so wherever this is offered
 * (AUD-12).
 */
export function clearCoverArt(): Promise<number> {
  return invoke<number>("clear_cover_art");
}

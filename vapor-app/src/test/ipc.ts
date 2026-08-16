/**
 * A working fake of the Rust backend, for tests and for looking at the UI.
 *
 * ## Why this is committed
 *
 * This had already been hand-rolled and thrown away three times — once per
 * session that wanted to see a screen — each time with slightly different
 * shapes. A fake that is rebuilt from memory drifts from the thing it fakes,
 * and then the tests written against it are testing the drift.
 *
 * ## What it is
 *
 * Not a mock that records calls and returns `undefined`. It holds real state
 * and behaves: adding a track to a playlist makes the playlist longer, saving a
 * password makes `has_webdav_password` true, scanning produces rows that then
 * appear in `library_view`. Screens can therefore be driven the way a person
 * drives them, and a test can assert on what the screen *shows* rather than on
 * which function was called.
 *
 * ## Keeping it honest
 *
 * The shapes must match `src-tauri/src/lib.rs`. `tests/command_bindings.rs`
 * proves every command has a binding in `core.ts` but says nothing about
 * payload shape, so drift here surfaces as component tests failing in confusing
 * ways rather than as a clear error. When a command's shape changes, change it
 * here in the same commit.
 */
import type * as core from "../lib/core";

/** Every command the backend exposes, as the frontend calls it. */
export type Command = string;

export interface FakeOptions {
  /** Start already connected to a server, as a returning user would be. */
  connected?: boolean;
  /** Start with a password in the keychain. */
  withPassword?: boolean;
  /** Tracks in the library. Defaults to a small mixed set. */
  rows?: core.Row[];
  playlists?: core.Playlist[];
}

/** A library row with sensible defaults, so a test states only what it cares
 *  about. */
export function makeRow(over: Partial<core.Row> = {}): core.Row {
  return {
    href: "/dav/Koofr/Music/track.m4a",
    title: "A Track",
    artist: "An Artist",
    album: "An Album",
    artistSource: "folder",
    albumSource: "folder",
    genre: "Electronic",
    bpm: 128,
    key: "8A",
    year: 2020,
    manualPos: 0,
    ...over,
  };
}

const DEFAULT_ROWS: core.Row[] = [
  makeRow({
    href: "/dav/Koofr/Music/windowlicker.m4a",
    title: "Windowlicker",
    artist: "Aphex Twin",
    // Deliberately not the same as the title: a fixture where a track and its
    // album share a name makes every text query ambiguous.
    album: "Windowlicker EP",
    bpm: 124.3,
    key: "8A",
  }),
  makeRow({
    href: "/dav/Koofr/Music/xtal.m4a",
    title: "Xtal",
    artist: "Aphex Twin",
    album: "Selected Ambient Works",
    bpm: 101.7,
    key: "5A",
  }),
  makeRow({
    href: "/dav/Koofr/Music/roygbiv.m4a",
    title: "Roygbiv",
    artist: "Boards of Canada",
    album: "Music Has the Right",
    bpm: 91.2,
    key: "4A",
  }),
  // Deliberately unanalysed: 0 BPM and no key is a real state the UI must
  // render as "—" rather than as "0".
  makeRow({
    href: "/dav/Koofr/Music/unanalysed.mp3",
    title: "Not Yet Analysed",
    artist: "Someone",
    album: "Somewhere",
    bpm: 0,
    key: "",
    artistSource: "unknown",
  }),
];

/**
 * A fake backend.
 *
 * `invoke` is the function to hand to the app; everything else on here is for
 * the test to arrange state and to assert on what the backend was asked.
 */
export class FakeBackend {
  /** Every command invoked, in order, for the rare assertion that needs it. */
  readonly calls: { cmd: Command; args: Record<string, unknown> }[] = [];

  /** Commands set to reject, so unhappy paths can be driven. */
  private failures = new Map<Command, string>();

  private settings: core.Settings;
  private password: string | null;
  private rows: core.Row[];
  private scanned: boolean;
  private playlists: core.Playlist[];
  private queue: string[] = [];
  private current: string | null = null;
  private status: core.PlaybackStatus = "idle";
  private analysed = 0;
  private nextId = 1;
  private repeat: core.RepeatMode = "off";
  private shuffled = false;

  constructor(options: FakeOptions = {}) {
    const connected = options.connected ?? true;
    this.settings = {
      remote: {
        url: connected ? "https://app.koofr.net" : "",
        username: connected ? "someone@example.com" : "",
        folder: connected ? "/dav/Koofr/Music" : "Music",
      },
      baseFontSize: 16,
      uiScale: 1,
      themeMode: "preset",
      theme: "daylight",
      customBaseColor: "#ffffff",
      customAccentColor: "#000000",
      headphoneProfile: "",
      headphoneCalibrationEnabled: false,
      bpmOverrides: {},
      cacheMaxBytes: 8_000_000_000,
    };
    this.password = options.withPassword ?? connected ? "app-password" : null;
    this.rows = options.rows ?? DEFAULT_ROWS;
    // A connected library is one that has been scanned; a fresh one has not.
    this.scanned = connected;
    this.playlists = options.playlists ?? [];
    this.analysed = connected ? this.rows.length : 0;
  }

  /** Make `cmd` reject with `message` until cleared. */
  fail(cmd: Command, message: string) {
    this.failures.set(cmd, message);
  }

  clearFailure(cmd: Command) {
    this.failures.delete(cmd);
  }

  /** Whether a command was invoked at all. */
  called(cmd: Command): boolean {
    return this.calls.some((c) => c.cmd === cmd);
  }

  /** The arguments of the last invocation of `cmd`. */
  lastArgs(cmd: Command): Record<string, unknown> | undefined {
    return [...this.calls].reverse().find((c) => c.cmd === cmd)?.args;
  }

  /** Read the fake's own state, for assertions about effects. */
  get state() {
    return {
      settings: this.settings,
      hasPassword: this.password !== null,
      playlists: this.playlists,
      queue: this.queue,
      current: this.current,
      status: this.status,
      scanned: this.scanned,
    };
  }

  readonly invoke = async <T>(
    cmd: Command,
    args?: Record<string, unknown>,
  ): Promise<T> => {
    this.calls.push({ cmd, args: args ?? {} });

    const failure = this.failures.get(cmd);
    if (failure) throw new Error(failure);

    return this.handle(cmd, args ?? {}) as T;
  };

  private handle(cmd: Command, a: Record<string, unknown>): unknown {
    switch (cmd) {
      // --- settings and connection -------------------------------------
      case "settings":
        return this.settings;

      case "set_remote_config": {
        const url = String(a.url ?? "").trim();
        // The real command refuses a value that is not an origin, because
        // everything downstream hangs paths off it.
        if (url !== "" && !/^https?:\/\//.test(url)) {
          throw new Error(`"${url}" is not a server address`);
        }
        const previous = this.settings.remote.username;
        const next = String(a.username ?? "").trim();
        // A rename *moves* the credential; it used to delete it.
        if (previous !== "" && previous !== next && this.password !== null) {
          // Nothing to do in the fake beyond keeping it — that is the point.
        }
        this.settings = {
          ...this.settings,
          remote: { url, username: next, folder: String(a.folder ?? "").trim() },
        };
        return this.settings;
      }

      case "save_webdav_password":
        this.password = String(a.password ?? "");
        return null;

      case "has_webdav_password":
        return this.password !== null && String(a.username ?? "") !== "";

      case "scan_library": {
        if (this.password === null) {
          throw new Error(
            "No password saved for this account. Type it in the Password field above and press Save before scanning.",
          );
        }
        // A folder that is not the real one finds nothing — the case that
        // produced "0 tracks" and no explanation.
        if (!this.settings.remote.folder.startsWith("/dav/")) {
          this.scanned = true;
          return { tracks: 0, directories: 1 };
        }
        this.scanned = true;
        return { tracks: this.rows.length, directories: 3 };
      }

      // --- library ------------------------------------------------------
      case "library_view": {
        if (!this.scanned) return [];
        const view = (a.view ?? {}) as core.LibraryView;
        let rows = [...this.rows];
        const q = (view.query ?? "").trim().toLowerCase();
        if (q) {
          rows = rows.filter((r) =>
            `${r.title} ${r.artist} ${r.album}`.toLowerCase().includes(q),
          );
        }
        const key = view.sortKey ?? "title";
        rows.sort((x, y) => {
          const pick = (r: core.Row) =>
            key === "bpm" ? r.bpm : String(r[key as keyof core.Row] ?? "");
          const l = pick(x);
          const r = pick(y);
          const cmp = typeof l === "number" ? l - (r as number) : String(l).localeCompare(String(r));
          return view.ascending === false ? -cmp : cmp;
        });
        if ((view.groupBy ?? "none") === "none") return [{ header: "", rows }];
        return [{ header: "All", rows }];
      }

      case "search": {
        const q = String(a.query ?? "").trim().toLowerCase();
        const tracks = q
          ? this.rows.filter((r) =>
              `${r.title} ${r.artist}`.toLowerCase().includes(q),
            )
          : [];
        return {
          top: tracks[0] ?? null,
          tracks,
          artists: [],
          albums: [],
          playlists: [],
          total: tracks.length,
        };
      }

      // --- playlists ----------------------------------------------------
      case "playlists":
        return this.playlists;

      case "create_playlist": {
        const made: core.Playlist = {
          id: `playlist-${this.nextId++}`,
          name: String(a.name ?? ""),
          customCoverPath: "",
          tracks: [],
          folderId: "",
        };
        this.playlists = [...this.playlists, made];
        return made;
      }

      case "add_tracks_to_playlist": {
        const id = String(a.id ?? "");
        const hrefs = (a.hrefs ?? []) as string[];
        let added = 0;
        this.playlists = this.playlists.map((p) => {
          if (p.id !== id) return p;
          const tracks = [...p.tracks];
          for (const h of hrefs) {
            if (!tracks.includes(h)) {
              tracks.push(h);
              added++;
            }
          }
          return { ...p, tracks };
        });
        return added;
      }

      case "rename_playlist": {
        const id = String(a.id ?? "");
        const name = String(a.name ?? "");
        if (!name.trim()) throw new Error("A playlist needs a name.");
        let hit = false;
        this.playlists = this.playlists.map((p) => {
          if (p.id !== id) return p;
          hit = true;
          return { ...p, name };
        });
        return hit;
      }

      case "delete_playlist": {
        const id = String(a.id ?? "");
        const before = this.playlists.length;
        this.playlists = this.playlists.filter((p) => p.id !== id);
        return this.playlists.length < before;
      }

      case "remove_playlist_track": {
        const id = String(a.id ?? "");
        const index = Number(a.index ?? -1);
        let hit = false;
        this.playlists = this.playlists.map((p) => {
          if (p.id !== id || index < 0 || index >= p.tracks.length) return p;
          hit = true;
          const tracks = [...p.tracks];
          tracks.splice(index, 1);
          return { ...p, tracks };
        });
        return hit;
      }

      case "reorder_playlist_track": {
        const id = String(a.id ?? "");
        const from = Number(a.from ?? -1);
        const to = Number(a.to ?? -1);
        let hit = false;
        this.playlists = this.playlists.map((p) => {
          if (p.id !== id) return p;
          if (from < 0 || from >= p.tracks.length) return p;
          if (to < 0 || to >= p.tracks.length) return p;
          hit = true;
          const tracks = [...p.tracks];
          const [moved] = tracks.splice(from, 1);
          if (moved !== undefined) tracks.splice(to, 0, moved);
          return { ...p, tracks };
        });
        return hit;
      }

      case "playlist_rows": {
        const p = this.playlists.find((x) => x.id === String(a.id ?? ""));
        if (!p) return [];
        // Hrefs with no row are skipped, exactly as the backend does.
        return p.tracks
          .map((h) => this.rows.find((r) => r.href === h))
          .filter((r): r is core.Row => r !== undefined);
      }

      // --- playback -----------------------------------------------------
      case "play_tracks": {
        this.queue = (a.hrefs ?? []) as string[];
        this.current = (a.start as string | null) ?? this.queue[0] ?? null;
        this.status = this.current ? "playing" : "idle";
        return null;
      }

      case "next_track": {
        const at = this.current ? this.queue.indexOf(this.current) : -1;
        this.current = this.queue[at + 1] ?? null;
        this.status = this.current ? "playing" : "idle";
        return this.current;
      }

      case "previous_track": {
        const at = this.current ? this.queue.indexOf(this.current) : 0;
        this.current = this.queue[Math.max(0, at - 1)] ?? null;
        return this.current;
      }

      case "pause_playback":
        this.status = "paused";
        return null;

      case "resume_playback":
        this.status = this.current ? "playing" : "idle";
        return null;

      case "stop_playback":
        this.status = "idle";
        this.current = null;
        return null;

      case "set_bpm_override": {
        // Stored *and applied*, as the backend does: a correction that did not
        // reach the rows would make the table look like it had ignored it.
        const href = String(a.href ?? "");
        const bpm = Number(a.bpm ?? 0);
        const overrides = { ...this.settings.bpmOverrides };
        if (bpm > 0) {
          // The real command refuses an implausible tempo rather than clamping.
          if (bpm < 40 || bpm > 250) {
            throw new Error(`A tempo of ${bpm} is not plausible.`);
          }
          overrides[href] = bpm;
        } else {
          delete overrides[href];
        }
        this.settings = { ...this.settings, bpmOverrides: overrides };
        this.rows = this.rows.map((r) =>
          r.href === href ? { ...r, bpm: overrides[href] ?? 0 } : r,
        );
        return null;
      }

      case "seek":
      case "set_volume":
      case "evict_track":
      case "cancel_analysis":
        return null;

      case "playback_state": {
        const row = this.rows.find((r) => r.href === this.current);
        return {
          href: this.current,
          title: row?.title ?? "",
          artist: row?.artist ?? "",
          status: this.status,
          loading: false,
          position: 0,
          duration: row ? 240 : 0,
          volume: 1,
          error: null,
          available: true,
          mixing: false,
          level: 0,
          waveform: [],
          nextTitle: "",
          cover: null,
        };
      }

      case "queue_state":
        return {
          current: this.current,
          tracks: this.queue,
          next: this.current
            ? (this.queue[this.queue.indexOf(this.current) + 1] ?? null)
            : null,
        };

      case "queue_view":
        return {
          entries: this.queue.map((href) => {
            const row = this.rows.find((r) => r.href === href);
            return {
              href,
              title: row?.title ?? href,
              artist: row?.artist ?? "",
              cover: null,
              bpm: row?.bpm ?? 0,
              key: row?.key ?? "",
              current: href === this.current,
            };
          }),
          repeat: this.repeat,
          shuffled: this.shuffled,
          current: this.current ? this.queue.indexOf(this.current) : null,
          remainingSecs: this.queue.length * 240,
        };

      case "remove_from_queue": {
        const href = String(a.href ?? "");
        const before = this.queue.length;
        this.queue = this.queue.filter((h) => h !== href);
        return this.queue.length < before;
      }

      case "move_in_queue": {
        const from = Number(a.from ?? -1);
        const to = Number(a.to ?? -1);
        if (from < 0 || from >= this.queue.length) return false;
        if (to < 0 || to >= this.queue.length) return false;
        const next = [...this.queue];
        const [moved] = next.splice(from, 1);
        if (moved !== undefined) next.splice(to, 0, moved);
        this.queue = next;
        return true;
      }

      case "play_next": {
        const href = String(a.href ?? "");
        const at = this.current ? this.queue.indexOf(this.current) : -1;
        this.queue = this.queue.filter((h) => h !== href);
        this.queue.splice(at + 1, 0, href);
        return true;
      }

      case "set_repeat":
        this.repeat = String(a.mode ?? "off") as core.RepeatMode;
        return null;

      case "set_shuffled":
        this.shuffled = Boolean(a.shuffled);
        return this.shuffled;

      case "vibe_path": {
        // Only analysed tracks can be planned with; the rest are reported as
        // skipped rather than silently dropped (TD-43b).
        const usable = this.rows.filter((r) => r.bpm > 0);
        return {
          hrefs: usable.map((r) => r.href),
          considered: usable.length,
          skipped: this.rows.length - usable.length,
        };
      }

      case "blend_preview": {
        if (!this.current) return null;
        const at = this.queue.indexOf(this.current);
        const from = this.rows.find((r) => r.href === this.current);
        const to = this.rows.find((r) => r.href === this.queue[at + 1]);
        if (!from || !to) return null;
        const matchable =
          from.bpm > 0 && to.bpm > 0 && Math.abs(from.bpm - to.bpm) / from.bpm <= 0.06;
        return {
          fromTitle: from.title,
          toTitle: to.title,
          fromBpm: from.bpm,
          toBpm: to.bpm,
          fromKey: from.key,
          toKey: to.key,
          shiftPercent: from.bpm > 0 ? ((to.bpm - from.bpm) / from.bpm) * 100 : 0,
          gainDelta: 0,
          matchable,
          reason: matchable ? "" : "Their tempos are too far apart to bridge.",
          transition: "Standard Crossfade",
        };
      }

      case "track_details": {
        const row = this.rows.find((r) => r.href === String(a.href ?? ""));
        if (!row) throw new Error("That track is not in the library.");
        return {
          href: row.href,
          title: row.title,
          artist: row.artist,
          album: row.album,
          year: row.year,
          genre: row.genre,
          analysed: row.bpm > 0,
          bpm: row.bpm,
          bpmIsManual: false,
          key: row.key,
          lufs: -9.4,
          duration: 240,
          cueIn: 0.2,
          cueOut: 238,
          energy: 0.6,
          beats: 512,
          waveform: [],
          hrefPath: row.href,
          cached: true,
          unplayable: null,
          cover: null,
          notes: null,
          tagged: false,
        };
      }

      case "data_breakdown":
        return [
          { label: "Audio cache", path: "/tmp/vapor-music/cache", bytes: 1_200_000_000, local: true },
          { label: "Analysis", path: "/tmp/vapor-music/analysis.json", bytes: 240_000, local: true },
        ];

      case "reveal_data_folder":
        return null;

      // --- analysis, cache, data ---------------------------------------
      case "analyse_library":
        this.analysed = this.rows.length;
        return null;

      case "analysis_status":
        return { analysed: this.analysed, total: this.scanned ? this.rows.length : 0 };

      case "cache_status":
        return {
          bytes: 1_200_000_000,
          maxBytes: this.settings.cacheMaxBytes,
          tracksCached: this.scanned ? this.rows.length : 0,
          tracksTotal: this.scanned ? this.rows.length : 0,
          location: "/tmp/vapor-music/cache",
        };

      case "set_cache_max_bytes":
        this.settings = { ...this.settings, cacheMaxBytes: Number(a.bytes ?? 0) };
        return this.settings;

      case "clear_audio_cache":
        return 1_200_000_000;

      case "data_location":
        return "/tmp/vapor-music";

      case "delete_all_data":
        this.password = null;
        this.rows = [];
        this.playlists = [];
        this.scanned = false;
        return null;

      case "mood_path":
        return { tracks: [], skipped: 0 };

      case "track_meta":
        return null;

      default:
        // Loud rather than `undefined`: an unknown command is a fake that has
        // drifted from the backend, and returning nothing turns that into a
        // confusing render failure three layers away.
        throw new Error(
          `FakeBackend has no case for "${cmd}" — add it, and check the shape ` +
            `against src-tauri/src/lib.rs`,
        );
    }
  }
}

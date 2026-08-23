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
 * A stub with a memory, in other words — not a second copy of the backend. See
 * below for where that line is drawn.
 *
 * ## What it deliberately does not do
 *
 * It holds state; it does not derive answers. Filtering, sorting, grouping rows
 * into albums, resolving a group to its tracks, scoring the DJ's three cards,
 * ruling on whether an address is an address or a tempo plausible — all of that
 * is the backend's, and `vapor-core` tests it. This file used to carry a second
 * implementation of each, in comments that described themselves as "mirroring
 * the backend" and "kept in step deliberately".
 *
 * They were not kept in step. The copies disagreed — keys sorted alphabetically
 * here and musically there, the address rule came apart twice, `MAX_STRETCH`
 * existed as a number in two languages — and because every test ran against
 * this file rather than against Rust, the tests agreed with the copy.
 *
 * So a command here returns state, or a fixture, or whatever the test asked for
 * with `answers()`. A screen's test then says what the screen does with an
 * answer, and `calls`/`lastArgs` say what it asked for. Both of those are the
 * screen's own behaviour, which is the only thing a screen's test can be about.
 *
 * ## Keeping it honest
 *
 * Shapes are no longer this file's problem: the response types in
 * `src/lib/generated/` are derived from the Rust structs by `ts-rs`, the canned
 * replies here are constrained to them with `satisfies`, and
 * `npm run types:check` fails when they drift. Behaviour across the boundary is
 * covered by `src-tauri/src/seam.rs`, which invokes real commands through the
 * real IPC.
 *
 * If you find yourself writing a rule in here, that is the signal it belongs in
 * a test in `vapor-core` instead.
 */
import type * as core from "../lib/core";

/** Every command the backend exposes, as the frontend calls it. */
export type Command = string;

export interface FakeOptions {
  /** Start already connected to a server, as a returning user would be. */
  connected?: boolean;
  /** Folders on this device the library reads from. Not playlist folders. */
  localFolders?: core.LocalFolder[];
  /** Start with a password in the keychain. */
  withPassword?: boolean;
  /** Tracks in the library. Defaults to a small mixed set. */
  rows?: core.Row[];
  /**
   * The albums and artists those tracks group into.
   *
   * Given rather than worked out. Grouping is `vapor_library`'s, and a fake
   * that derives it a second way is a fake that can disagree with the real one.
   * Defaults to the entities `DEFAULT_ROWS` produces; supply `rows` without
   * this and the grid is empty, which is the prompt to say what it should hold.
   */
  albums?: core.LibraryEntity[];
  artists?: core.LibraryEntity[];
  playlists?: core.Playlist[];
  /** Playlist folders. Not to be confused with `unreadableFolders` below. */
  folders?: core.Folder[];
  groups?: core.DynamicGroup[];
  /** Start with lyric and artwork lookups permitted. Off by default, as ships. */
  metadataLookup?: boolean;
  /** What a lookup would find, by href. Anything absent finds nothing. */
  lyrics?: Record<string, core.Lyrics>;
  /** Start with local-network sync switched on. Off by default, as ships. */
  syncEnabled?: boolean;
  /** Whether the DJ conducts. On by default, as ships. */
  djMode?: boolean;
  /** The stored theme choice. `auto` by default, as ships. */
  appearance?: "auto" | "daylight" | "lamplight";
  /** Tracks analysis could not describe. Empty by default, as a sound one is. */
  analysisFailures?: core.AnalysisFailure[];
  /**
   * Data files the backend could not read at startup, already turned into the
   * sentences it would show. Empty by default, which is every normal launch.
   */
  damaged?: string[];
  /** What an artwork search would find, by album title. */
  albumArtSearch?: Record<string, string>;
  /** Devices on the network. Empty by default, as a lone machine is. */
  peers?: core.SyncPeer[];
  /** Devices already paired with. */
  trustedPeers?: core.TrustedDevice[];
  /** Playlists already in the shared document on the server (SYNC-006). */
  sharedDocument?: core.Playlist[];
  /**
   * Folders the scan cannot read and walks past (TD-49). The real backend
   * counts them and reports them; this is how a test reaches that sentence.
   */
  unreadableFolders?: number;
  /**
   * A keychain that accepts a password and does not keep it — TD-50 exactly.
   * `save_webdav_password` succeeds and `has_webdav_password` stays false.
   *
   * Modelled rather than imagined: this is the state the app shipped in, and
   * the screen reported it as success. A store that can only succeed is a
   * store no test can catch lying.
   */
  keychainSilentlyFails?: boolean;
  /** Cached bytes on disk. */
  cacheBytes?: number;
  /** Cover art bytes on disk. */
  coverBytes?: number;
  /**
   * A pass is running right now, on this track.
   *
   * Modelled because the screen has to report a pass it did not start — which
   * is every pass now that a scan begins one.
   */
  analysing?: { title: string } | undefined;
  /** Clearing the cache reports bytes freed but leaves them on disk. */
  cacheResistsClearing?: boolean;
  /**
   * Whether tracks have embedded artwork.
   *
   * Off by default: a freshly scanned library has none until analysis has read
   * the files, and the grid has to look right in that state too.
   */
  covers?: boolean;
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

/**
 * An album or artist tile, with the counts a test usually has no opinion about.
 *
 * `plays` and `lastPlayed` exist so the home shelves can rank these, and almost
 * every test that names an entity is about something else entirely. Stating
 * them at nine call sites to say "zero, as a library nobody has played from"
 * is nine chances to say it differently.
 */
export function makeEntity(
  over: Partial<core.LibraryEntity> = {},
): core.LibraryEntity {
  return {
    name: "An Album",
    subtitle: "An Artist",
    tracks: 1,
    lead: "/dav/Koofr/Music/track.m4a",
    plays: 0,
    lastPlayed: 0,
    // A library nobody has identified, which is what the backend reports until
    // the lookup pass has run. Zero means "no idea how long this record is",
    // so the default tile is whole — a fake that defaulted to incomplete would
    // put every test's album under a heading it never asked for.
    totalTracks: 0,
    recordType: "",
    incomplete: false,
    ...over,
  };
}

/**
 * The canned answer to `library_view`, in the shape the command now returns.
 *
 * `library_view` answers with a window plus the length of the result it was cut
 * from (AUD-13), so a test arranging a narrowed reply has three fields to state
 * where it used to state one. Two of them are the same at almost every call
 * site — the arranged rows are the whole of the arranged result, starting at
 * its beginning — and restating that at each one is another chance to get
 * `total` and `offset` to disagree with `sections`.
 *
 * `total` counts the rows given unless a test says otherwise, which is what a
 * test wanting the header's count to exceed the window in hand would do.
 */
export function makePage(
  sections: core.LibrarySection[],
  over: Partial<core.LibraryPage> = {},
): core.LibraryPage {
  return {
    sections,
    total: sections.reduce((n, section) => n + section.rows.length, 0),
    offset: 0,
    ...over,
  };
}

/**
 * The default rows, by title, in the order given.
 *
 * The instance method of the same intent reads whatever rows a test arranged;
 * this one is for arranging them in the first place, which has to happen at
 * `useBackend` and therefore before there is an instance to ask.
 *
 * A test that cares about on-screen order states it here rather than through
 * `answers()`. The two are not interchangeable any more: a canned answer is
 * returned whole, so a screen that asks for a window gets the whole thing back
 * and every row in it looks like it was in the window.
 */
export function defaultRowsNamed(...titles: string[]): core.Row[] {
  return titles.map((title) => {
    const row = DEFAULT_ROWS.find((r) => r.title === title);
    if (!row) throw new Error(`no default row titled ${title}`);
    return row;
  });
}

/** An album or artist as a home shelf draws it. */
function shelfOf(entity: core.LibraryEntity): core.Shelf {
  return {
    id: entity.name,
    title: entity.name,
    subtitle: entity.subtitle,
    lead: entity.lead,
    tracks: entity.tracks,
    plays: entity.plays,
  };
}

/** "12 tracks", and "1 track" rather than "1 tracks" — as the backend says it. */
function tracksLine(n: number): string {
  return `${n} ${n === 1 ? "track" : "tracks"}`;
}

/**
 * A fake sleeve: a real data URI, so the UI renders an actual `<img>` rather
 * than being tested against a string it never receives.
 */
const A_SLEEVE =
  "data:image/gif;base64,R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw==";

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
 * The albums and artists a scan of `DEFAULT_ROWS` produces.
 *
 * Written down rather than derived. Grouping rows into entities is
 * `vapor_library`'s job — including the part that is easy to get subtly wrong,
 * where an album's identity is its title *plus* the folder its tracks live in,
 * so two records called "Greatest Hits" stay two records. This file used to
 * reimplement that rule and describe itself as "mirroring the backend", which
 * is the arrangement that lets the two drift apart in silence.
 *
 * A test that needs different entities scripts them with
 * `backend.answers("library_entities", ...)`.
 *
 * "Not Yet Analysed" has `artistSource: "unknown"`, so the backend omits it
 * from both lists — an unknown artist is not an artist named "unknown".
 */
const DEFAULT_ALBUMS: core.LibraryEntity[] = [
  makeEntity({
    name: "Windowlicker EP",
    subtitle: "Aphex Twin",
    lead: "/dav/Koofr/Music/windowlicker.m4a",
  }),
  makeEntity({
    name: "Selected Ambient Works",
    subtitle: "Aphex Twin",
    lead: "/dav/Koofr/Music/xtal.m4a",
  }),
  makeEntity({
    name: "Music Has the Right",
    subtitle: "Boards of Canada",
    lead: "/dav/Koofr/Music/roygbiv.m4a",
  }),
];

const DEFAULT_ARTISTS: core.LibraryEntity[] = [
  makeEntity({
    name: "Aphex Twin",
    subtitle: "2 albums",
    tracks: 2,
    lead: "/dav/Koofr/Music/windowlicker.m4a",
  }),
  makeEntity({
    name: "Boards of Canada",
    subtitle: "1 album",
    lead: "/dav/Koofr/Music/roygbiv.m4a",
  }),
];

/**
 * The three cards the Vibe screen is offered.
 *
 * Canned, not chosen. Which track earns each exit is `candidate_cost`,
 * `exit_between` and the pathfinder in the backend, scoring intensity and
 * transition cost against the whole pool. This file used to approximate that
 * in about seventy lines whose own comments said they "stood in for" the real
 * scoring — an approximation nothing compared against the thing it approximated
 * and which no screen test was actually about.
 *
 * A screen's test needs three cards with distinct exits, one of them queued.
 * That is what this is. A test that needs different cards — none at all, two
 * that clash, an unanalysed library — scripts them with
 * `backend.answers("mix_candidates", ...)`.
 */
function defaultCandidates(covers: boolean): core.MixCandidate[] {
  const card = (
    row: core.Row,
    exit: core.Exit,
    transition: string,
    selected: boolean,
  ): core.MixCandidate => ({
    href: row.href,
    title: row.title,
    artist: row.artist,
    bpm: row.bpm,
    key: row.key,
    exit,
    label: exit.toUpperCase(),
    transition,
    selected,
    cover: covers ? A_SLEEVE : null,
  });
  const [windowlicker, xtal, roygbiv] = DEFAULT_ROWS;
  return [
    card(xtal!, "stay", "Bass Swap", false),
    card(roygbiv!, "follow", "Filter Sweep", true),
    card(windowlicker!, "switch", "Echo Out", false),
  ];
}

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

  /**
   * Answers a test has scripted, by command.
   *
   * The counterpart to `fail`. Where this file used to compute a reply — sort
   * the rows, resolve a group's members, pick mix candidates — it now returns
   * what the test asked it to, because that computation is the backend's and
   * `vapor-library` already tests it. Re-deriving it here produced a second
   * implementation that could disagree with the first, and did: keys sorted
   * alphabetically here and musically there.
   *
   * A screen's test is then about the screen. What it asked for is in `calls`
   * and `lastArgs`; what it does with the answer is on the page.
   */
  private scripted = new Map<Command, unknown>();

  private settings: core.Settings;
  private password: string | null;
  private rows: core.Row[];
  private albums: core.LibraryEntity[];
  private artists: core.LibraryEntity[];
  private scanned: boolean;
  private playlists: core.Playlist[];
  private folders: core.Folder[];
  private groups: core.DynamicGroup[];
  private pinned = new Set<string>();
  /** Startup damage sentences the backend would report. Empty by default. */
  private damaged: string[] = [];
  /** Covers chosen by hand, keyed as the backend keys them. */
  private albumArt: Record<string, string> = {};
  /** What a search would find, by album title. Anything absent finds nothing. */
  private albumArtSearch: Record<string, string> = {};

  /** The embedded cover for a track, honouring `covers` like every other
   *  cover-bearing command. */
  private coverFor(href: string): string | null {
    return this.rows.some((r) => r.href === href) && this.covers
      ? A_SLEEVE
      : null;
  }
  private queue: string[] = [];
  private current: string | null = null;
  /** What the DJ is conducting over — the name `play_tracks` was given. */
  private scope = "";
  private status: core.PlaybackStatus = "idle";
  private analysed = 0;
  private nextId = 1;
  private repeat: core.RepeatMode = "off";
  private shuffled = false;
  private unreadableFolders: number;
  private keychainSilentlyFails: boolean;
  private analysisFailures: core.AnalysisFailure[];
  private cacheBytes: number;
  private coverBytes: number;
  private cacheResistsClearing: boolean;
  private covers: boolean;
  private analysing: boolean;
  private analysingTitle: string;
  private deleted = false;
  /** Tracks a lookup has been made for. */
  private attempted = new Set<string>();
  /** Devices on the fake network. */
  private peers: core.SyncPeer[];
  private trusted: core.TrustedDevice[];
  private pairing: { peerId: string; pin: string } | null = null;
  private syncing: core.SyncProgress | null = null;
  /** The shared document on the fake server, or null when there is none. */
  private sharedDocument: core.Playlist[] | null;

  private syncView(): core.SyncView {
    if (!this.settings.syncEnabled) {
      return {
        enabled: false,
        deviceId: "this-device",
        deviceName: "Vapor",
        discovered: [],
        trusted: [],
        pin: null,
        pairingWith: null,
        progress: {
          running: false,
          peer: "",
          file: "",
          done: 0,
          total: 0,
          bytes: 0,
          elapsed: 0,
          error: "",
        },
      };
    }
    return {
      enabled: true,
      deviceId: "this-device",
      deviceName: "Vapor",
      discovered: this.peers,
      trusted: this.trusted,
      pin: this.pairing?.pin ?? null,
      pairingWith: this.pairing?.peerId ?? null,
      progress: this.syncing ?? {
        running: false,
        peer: "",
        file: "",
        done: 0,
        total: 0,
        bytes: 0,
        elapsed: 0,
        error: "",
      },
    };
  }
  private readonly lyricsFor: Record<string, core.Lyrics>;

  /**
   * What a lookup has found, or would find.
   *
   * A track only has lyrics here if the test said so, because "the service
   * has nothing for this track" is the common case and the screen has to say
   * something sensible in it.
   */
  private lookedUp(href: string): core.LookedUp {
    const attempted = this.attempted.has(href);
    const found = attempted ? this.lyricsFor[href] : undefined;
    return {
      lyrics: found ?? null,
      artistImage:
        attempted && found ? "https://example.invalid/artist.jpg" : "",
      albumArt: attempted && found ? "https://example.invalid/album.jpg" : "",
      genre: attempted && found ? "Electronic" : "",
      attempted,
      allowed: this.settings.metadataLookupEnabled,
    };
  }

  constructor(options: FakeOptions = {}) {
    const connected = options.connected ?? true;
    this.settings = {
      // Folders on this device, alongside the server. Empty by default, so
      // every existing test still describes a server-only library.
      //
      // Named `localFolders` in the options because `folders` already means
      // playlist folders here, and two things called folders in one fake is
      // how a fixture starts lying.
      folders: options.localFolders ?? [],
      remote: {
        url: connected ? "https://app.koofr.net" : "",
        username: connected ? "someone@example.com" : "",
        folder: connected ? "/dav/Koofr/Music" : "Music",
      },
      baseFontSize: 16,
      uiScale: 1,
      themeMode: "preset",
      theme: options.appearance ?? "auto",
      customBaseColor: "#ffffff",
      customAccentColor: "#000000",
      headphoneProfile: "",
      headphoneCalibrationEnabled: false,
      bpmOverrides: {},
      albumArt: {},
      preferLookedUpArt: false,
      hideDuplicates: false,
      cacheMaxBytes: 8_000_000_000,
      metadataLookupEnabled: options.metadataLookup ?? false,
      vibeLimit: 0.5,
      syncEnabled: options.syncEnabled ?? false,
      djMode: options.djMode ?? true,
      curve: "build",
    };
    this.analysisFailures = options.analysisFailures ?? [];
    this.damaged = options.damaged ?? [];
    this.albumArtSearch = options.albumArtSearch ?? {};
    this.lyricsFor = options.lyrics ?? {};
    this.peers = options.peers ?? [];
    this.trusted = options.trustedPeers ?? [];
    this.sharedDocument = options.sharedDocument ?? null;
    this.keychainSilentlyFails = options.keychainSilentlyFails ?? false;
    // A store that never keeps anything cannot already be holding something.
    // Seeding a password here made `keychainSilentlyFails` unobservable: the
    // screen asked "is one stored?", got yes, and reported the success it was
    // supposed to be incapable of reporting.
    //
    // Parenthesised deliberately — `a ?? b ? c : d` is `(a ?? b) ? c : d`,
    // which is the intent here but reads as though it were not.
    this.password = this.keychainSilentlyFails
      ? null
      : (options.withPassword ?? connected)
        ? "app-password"
        : null;
    this.rows = options.rows ?? DEFAULT_ROWS;
    // The default entities describe the default rows, so they only stand in
    // when the default rows are what is being used.
    const own = options.rows !== undefined;
    this.albums = options.albums ?? (own ? [] : DEFAULT_ALBUMS);
    this.artists = options.artists ?? (own ? [] : DEFAULT_ARTISTS);
    // A connected library is one that has been scanned; a fresh one has not.
    this.scanned = connected;
    this.playlists = options.playlists ?? [];
    this.folders = options.folders ?? [];
    this.groups = options.groups ?? [];
    this.analysed = connected ? this.rows.length : 0;
    this.unreadableFolders = options.unreadableFolders ?? 0;
    this.cacheBytes = options.cacheBytes ?? 1_200_000_000;
    this.coverBytes = options.coverBytes ?? 149_000_000;
    this.cacheResistsClearing = options.cacheResistsClearing ?? false;
    this.covers = options.covers ?? false;
    this.analysing = options.analysing !== undefined;
    this.analysingTitle = options.analysing?.title ?? "";
  }

  /** Make `cmd` reject with `message` until cleared. */
  /**
   * Reply to `cmd` with `value`, from now on.
   *
   * Call it again to change the answer mid-test, which is how a test drives a
   * search: arrange the second answer, type, and assert the table shows it.
   */
  answers(cmd: Command, value: unknown) {
    this.scripted.set(cmd, value);
  }

  /** Stop answering `cmd` from a script and go back to the default. */
  clearAnswer(cmd: Command) {
    this.scripted.delete(cmd);
  }

  /**
   * The default rows whose titles are listed, in the order given.
   *
   * For arranging the answer a search would have produced, without a test
   * having to restate a whole row to do it.
   */
  rowsNamed(...titles: string[]): core.Row[] {
    return titles.map((title) => {
      const row = this.rows.find((r) => r.title === title);
      if (!row) throw new Error(`no default row titled ${title}`);
      return row;
    });
  }

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

  /** How many times it was invoked — for asking whether a screen re-read. */
  timesCalled(cmd: Command): number {
    return this.calls.filter((c) => c.cmd === cmd).length;
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
      folders: this.folders,
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

  /**
   * The tracks a playlist or group holds.
   *
   * A playlist keeps its own list, so that half is just state. A group does
   * not — it resolves against the index, and that resolution is
   * `vapor_library::group`, which this used to carry a second copy of. The
   * whole library stands in for it here: what a download screen owes is
   * downloading what it is handed and counting it, not deciding membership.
   */
  private collectionTracks(kind: string, id: string): string[] {
    if (kind === "playlist") {
      return this.playlists.find((p) => p.id === id)?.tracks ?? [];
    }
    if (kind === "group") {
      return this.groups.some((x) => x.id === id)
        ? this.rows.map((r) => r.href)
        : [];
    }
    return [];
  }

  /**
   * `satisfies`, not a cast.
   *
   * The response types come from `ts-rs`, generated from the Rust structs, so
   * constraining a canned response to one makes it impossible for this file to
   * answer in a shape the real backend would not. That is the specific failure
   * this mock had: it returned camelCase because the frontend expected
   * camelCase, while Rust sent snake_case, and every test agreed with the mock.
   *
   * A cast would silence the check. `satisfies` keeps the literal's own type and
   * still fails the build when the Rust changes underneath it.
   */
  private handle(cmd: Command, a: Record<string, unknown>): unknown {
    // A scripted answer wins over the default one, so a test can say what the
    // backend replies without this file having to work it out.
    if (this.scripted.has(cmd)) return this.scripted.get(cmd);

    switch (cmd) {
      // --- settings and connection -------------------------------------
      case "settings":
        return this.settings;

      case "set_remote_config": {
        // No address validation here. The real command refuses anything that is
        // not an origin, checking the host and not merely the scheme, and this
        // file used to restate that rule with a matching regex and a comment
        // explaining that the two had to be kept in step by hand. They came
        // apart twice.
        //
        // Refusal is the backend's, and the seam tests cover it. A screen's
        // test that needs a refusal asks for one with
        // `backend.fail("set_remote_config", "...")`, which is what it means to
        // test that the screen shows the reason it was given.
        this.settings = {
          ...this.settings,
          remote: {
            url: String(a.url ?? "").trim(),
            username: String(a.username ?? "").trim(),
            folder: String(a.folder ?? "").trim(),
          },
        };
        return this.settings;
      }

      case "save_webdav_password":
        // Succeeds either way. That is the point: the command returning
        // without throwing has never been evidence that anything was stored.
        if (!this.keychainSilentlyFails)
          this.password = String(a.password ?? "");
        return null;

      case "has_webdav_password":
        return this.password !== null && String(a.username ?? "") !== "";

      case "local_folders":
        return this.settings.folders;

      // The native folder picker, which `@tauri-apps/plugin-dialog` reaches
      // through the same IPC as everything else. A canned path, because a fake
      // cannot open a real picker and the screen's behaviour is what is under
      // test: what it does with a chosen folder, not how the folder was chosen.
      //
      // The monkey found this one. It clicks "Add a folder" like anything else
      // on screen, and an unhandled command throws — which is exactly what the
      // monkey is for.
      case "plugin:dialog|open":
        return "/Users/someone/Music";

      case "add_local_folder": {
        const path = String(a.path ?? "").trim();
        // Already configured is a no-op returning the list, matching the
        // backend: clicking add on a folder you already have is not an error.
        if (this.settings.folders.some((f) => f.path === path)) {
          return this.settings.folders;
        }
        this.settings.folders = [
          ...this.settings.folders,
          { id: `folder-${this.settings.folders.length + 1}`, path, name: "" },
        ];
        return this.settings.folders;
      }

      case "remove_local_folder": {
        const id = String(a.id ?? "");
        this.settings.folders = this.settings.folders.filter(
          (f) => f.id !== id,
        );
        return this.settings.folders;
      }

      case "scan_library": {
        if (this.password === null) {
          throw new Error(
            "No password saved for this account. Type it in the Password field above and press Save before scanning.",
          );
        }
        // A folder that is not on the server is an error, not an empty
        // library (TD-49). It used to return zero tracks here, matching a
        // backend that swallowed the 404 — and "Found 0 tracks" is also what
        // a genuinely empty library says, so nothing distinguished them.
        if (!this.settings.remote.folder.startsWith("/dav/")) {
          throw new Error(
            `The server has no folder at ${this.settings.remote.folder}. ` +
              `Check the Folder field — it is the full path on the server, ` +
              `not the name of the folder.`,
          );
        }
        this.scanned = true;
        // Scanning starts an analysis pass, without being asked. The real
        // shell spawns one at the end of `scan_library`; here it completes at
        // once, which is the part the screens can observe — that a scanned
        // library describes itself without anyone pressing Analyse.
        this.analysed = this.rows.length;
        return {
          tracks: this.rows.length,
          directories: 3,
          unreadable: this.unreadableFolders,
        };
      }

      // --- library ------------------------------------------------------
      case "library_view": {
        if (!this.scanned) return makePage([]);
        // The window is applied; it is not derived. Slicing an ordered list at
        // the offset the caller named is what the transport does (AUD-13), and
        // a stub that answered a request for rows 300-600 with all 50,000 would
        // make every windowed screen look correct here and be wrong in the app
        // — which is the failure a fake exists to catch, not to produce.
        //
        // Still no filtering, sorting, scoping or de-duplication. All of it is
        // `vapor_library::index`, which tests it — including the parts this
        // used to get wrong: Camelot keys sort musically rather than
        // alphabetically, unknowns sink whichever way the sort runs, and ties
        // break on title. A screen's test asserted against none of that.
        //
        // What comes back is what the test arranged, in the window it asked
        // for. That makes a screen's test say what the screen does with an
        // answer, and `calls` says what it asked for.
        const w = (a.window ?? {}) as { offset?: number; limit?: number };
        const offset = Math.min(Math.max(w.offset ?? 0, 0), this.rows.length);
        const end = Math.min(
          offset + (w.limit ?? this.rows.length),
          this.rows.length,
        );
        return makePage([{ header: "", rows: this.rows.slice(offset, end) }], {
          total: this.rows.length,
          offset,
        });
      }

      case "library_entities": {
        if (!this.scanned) return [];
        const view = (a.view ?? {}) as core.LibraryView;
        return view.groupBy === "artist" ? this.artists : this.albums;
      }

      case "home_shelves": {
        if (!this.scanned) {
          return {
            playlists: [],
            groups: [],
            artists: [],
            albums: [],
            tracks: 0,
          } satisfies core.HomeShelves;
        }
        // In the order they were given, not in play order. Which of these is
        // most played is `home_shelves_for` in the backend, ranked on four
        // keys and tested there; a fake that sorted them a second way would be
        // a second opinion about the same question. What a screen's test is
        // about is that four shelves are drawn, in their sections, and that
        // pressing a tile opens or plays the right thing.
        return {
          playlists: this.playlists.map((list) => ({
            id: list.id,
            title: list.name,
            subtitle: tracksLine(list.tracks.length),
            lead: list.tracks[0] ?? "",
            tracks: list.tracks.length,
            plays: 0,
          })),
          groups: this.groups.map((group) => ({
            id: group.id,
            title: group.name,
            // A group resolves to the whole library here, as `group_tracks`
            // above answers, for the reason given there.
            subtitle: tracksLine(this.rows.length),
            lead: this.rows[0]?.href ?? "",
            tracks: this.rows.length,
            plays: 0,
          })),
          artists: this.artists.map(shelfOf),
          albums: this.albums.map(shelfOf),
          tracks: this.rows.length,
        } satisfies core.HomeShelves;
      }

      case "mix_candidates": {
        // Nothing playing means nothing to mix out of, which is the one part
        // of this the screen actually branches on.
        if (!this.rows.some((r) => r.href === this.current)) return [];
        return defaultCandidates(this.covers);
      }

      case "choose_next": {
        const href = String(a.href ?? "");
        const at = this.current ? this.queue.indexOf(this.current) : -1;
        this.queue = this.queue.filter((h) => h !== href);
        this.queue.splice(at + 1, 0, href);
        return null;
      }

      // Both sizes answer the same way here. The real difference is bytes —
      // 128 px against the stored original (PERF-004) — and a fixture has no
      // bytes to speak of, so what a test can check is that rows ask for the
      // small one at all.
      case "lookup_counts": {
        // Counts tracks *asked about*, matching the real command — a track
        // nothing was found for has still been asked, and asking again costs a
        // request and finds nothing.
        return {
          fetched: this.rows.filter((r) => this.attempted.has(r.href)).length,
          total: this.rows.length,
        } satisfies core.LookupCounts;
      }

      case "track_cover":
      case "track_thumb": {
        const href = String(a.href ?? "");
        return this.rows.some((r) => r.href === href) && this.covers
          ? A_SLEEVE
          : null;
      }

      case "search": {
        // Searching is ranked in the backend across tracks, artists, albums and
        // playlists. A substring match over two fields is not that, and no test
        // here is about ranking — the ones that are about search script it.
        return {
          top: null,
          tracks: [],
          artists: [],
          albums: [],
          playlists: [],
          total: 0,
        } satisfies core.SearchResults;
      }

      case "playlists":
        return this.playlists;

      case "create_playlist": {
        const wanted = a.folderId == null ? "" : String(a.folderId);
        const made: core.Playlist = {
          id: `playlist-${this.nextId++}`,
          name: String(a.name ?? ""),
          customCoverPath: "",
          tracks: [],
          folderId: this.folders.some((f) => f.id === wanted) ? wanted : "",
        };
        this.playlists = [...this.playlists, made];
        return made;
      }

      case "track_lookup":
        return this.lookedUp(String(a.href ?? ""));

      case "artist_portrait": {
        if (!this.settings.metadataLookupEnabled) return null;
        const name = String(a.name ?? "");
        // Any track by that artist that has been looked up answers for it.
        const found = this.rows.some(
          (r) =>
            r.artist === name &&
            this.attempted.has(r.href) &&
            this.lyricsFor[r.href],
        );
        return found ? A_SLEEVE : null;
      }

      case "looked_up_image": {
        const url = String(a.url ?? "");
        // Only a URL a lookup actually stored, as the backend guards.
        const known = Object.keys(this.lyricsFor).some((href) => {
          const l = this.lookedUp(href);
          return url !== "" && (l.artistImage === url || l.albumArt === url);
        });
        return known ? A_SLEEVE : null;
      }

      case "look_up_track": {
        const href = String(a.href ?? "");
        if (!this.settings.metadataLookupEnabled) {
          throw new Error(
            "Looking up lyrics and artwork is switched off. Turn it on in Settings.",
          );
        }
        if (a.force !== true && this.attempted.has(href)) {
          return this.lookedUp(href);
        }
        this.attempted.add(href);
        return this.lookedUp(href);
      }

      case "set_curve": {
        const curve = String(a.curve ?? "");
        this.settings = { ...this.settings, curve: curve as core.Curve };
        // Everything after the track playing was a route to the old
        // destination, so it goes; the backend re-plans, and here the queue
        // simply shortens, which is what the screen has to survive.
        const at = this.current ? this.queue.indexOf(this.current) : -1;
        if (at >= 0) this.queue = this.queue.slice(0, at + 1);
        return this.settings;
      }

      case "set_vibe_limit": {
        const limit = Number(a.limit);
        if (!Number.isFinite(limit))
          throw new Error("That is not a Vibe Limit.");
        this.settings = {
          ...this.settings,
          vibeLimit: Math.min(Math.max(limit, 0.1), 1),
        };
        return this.settings;
      }

      case "set_metadata_lookup": {
        this.settings = {
          ...this.settings,
          metadataLookupEnabled: a.enabled === true,
        };
        // Off forgets what was found, as the backend does.
        if (!this.settings.metadataLookupEnabled) this.attempted.clear();
        return this.settings;
      }

      case "sync_view":
        return this.syncView();

      case "media_keys_available":
        return true;

      // Album artwork. The fake keeps the same three-source order as the
      // backend so a screen can be driven through all of it: a hand-chosen
      // cover, then the file's embedded picture, then a looked-up one.
      case "album_cover": {
        const key = `${a.album as string}\u001f${a.lead as string}`;
        const chosen = this.albumArt[key];
        return {
          src: chosen ?? this.coverFor(a.lead as string),
          chosen: chosen !== undefined,
        } satisfies core.AlbumArt;
      }

      case "find_album_art": {
        const found = this.albumArtSearch[a.album as string];
        if (!found) {
          throw new Error(
            `Nothing came back for \u201c${a.album as string}\u201d. The album or ` +
              `artist may be spelled differently on the service than in your tags.`,
          );
        }
        this.albumArt[`${a.album as string}\u001f${a.lead as string}`] = found;
        return { src: found, chosen: true } satisfies core.AlbumArt;
      }

      case "clear_album_art": {
        delete this.albumArt[`${a.album as string}\u001f${a.lead as string}`];
        return {
          src: this.coverFor(a.lead as string),
          chosen: false,
        } satisfies core.AlbumArt;
      }

      case "set_prefer_looked_up_art":
        this.settings = {
          ...this.settings,
          preferLookedUpArt: a.enabled === true,
        };
        return this.settings;

      // Empty on every normal launch. A test that wants the damage banner sets
      // `damaged` on the options — see `FakeOptions`.
      case "set_hide_duplicates":
        this.settings = {
          ...this.settings,
          hideDuplicates: a.enabled === true,
        };
        return this.settings;

      case "startup_problems":
        return this.damaged;

      // The pass runs in the background and reports by event; the fake has
      // nothing to report, which is the correct quiescent behaviour.
      case "identify_library":
        return undefined;

      case "set_dj_mode":
        this.settings = { ...this.settings, djMode: a.enabled === true };
        return this.settings;

      // Rejects an unknown word exactly as the command does, so a test that
      // types one gets the failure rather than a fake that shrugs.
      case "analysis_failures":
        return this.analysisFailures;

      case "set_appearance": {
        const choice = String(a.appearance ?? "")
          .trim()
          .toLowerCase();
        if (!["auto", "daylight", "lamplight"].includes(choice)) {
          throw new Error(`unknown appearance ${JSON.stringify(a.appearance)}`);
        }
        this.settings = { ...this.settings, theme: choice };
        return this.settings;
      }

      case "set_sync_enabled": {
        this.settings = { ...this.settings, syncEnabled: a.enabled === true };
        if (!this.settings.syncEnabled) this.trusted = [];
        return this.settings;
      }

      case "open_pairing": {
        // Fixed, so a test can type it. The real one is six random digits
        // from the OS.
        this.pairing = { peerId: String(a.peerId ?? ""), pin: "482913" };
        return this.pairing.pin;
      }

      case "cancel_pairing":
        this.pairing = null;
        return null;

      case "pair_with": {
        const peerId = String(a.peerId ?? "");
        const peer = this.peers.find((p) => p.id === peerId);
        if (!peer) throw new Error("That device is no longer on the network.");
        // The fake's other device is showing 482913, the same one `open_pairing`
        // shows — enough for a screen test to get the code right or wrong.
        if (String(a.pin ?? "") !== "482913") {
          throw new Error("wrong code — 2 left");
        }
        this.trusted = [
          ...this.trusted.filter((t) => t.id !== peerId),
          { id: peer.id, name: peer.name, kind: peer.kind, pairedAt: 1 },
        ];
        return peer.name;
      }

      case "forget_peer": {
        const peerId = String(a.peerId ?? "");
        const had = this.trusted.some((t) => t.id === peerId);
        this.trusted = this.trusted.filter((t) => t.id !== peerId);
        return had;
      }

      case "sync_with": {
        const peerId = String(a.peerId ?? "");
        const peer = this.trusted.find((t) => t.id === peerId);
        if (!peer) throw new Error("That device is not paired.");
        if (this.syncing?.running)
          throw new Error("A sync is already running.");
        // Finished immediately: what a screen test needs is the shape of a
        // completed sync, and a fake that takes real time is a fake that makes
        // every test that touches it slow and flaky.
        this.syncing = {
          running: false,
          peer: peer.name,
          file: "",
          done: 2,
          total: 2,
          bytes: 8_000_000,
          elapsed: 4,
          error: "",
        };
        return null;
      }

      case "sync_shared_document": {
        if (!this.settings.remote.url || !this.settings.remote.username) {
          throw new Error(
            "No server is configured, so there is nowhere to keep it.",
          );
        }
        // Additive, as the real merge is: a playlist the server has and this
        // device does not, and nothing removed.
        const created = !this.sharedDocument;
        const incoming = this.sharedDocument ?? [];
        let added = 0;
        for (const list of incoming) {
          if (!this.playlists.some((p) => p.id === list.id)) {
            this.playlists = [...this.playlists, list];
            added += 1;
          }
        }
        this.sharedDocument = this.playlists;
        return {
          playlistsAdded: added,
          playlistsExtended: 0,
          foldersAdded: 0,
          temposAdded: 0,
          playlistsDeleted: 0,
          foldersDeleted: 0,
          created,
        } satisfies core.SharedSyncResult;
      }

      // --- Dynamic groups ---------------------------------------------------
      //
      // A group holds entities and resolves to tracks on read, so the fake
      // resolves them the same way the backend does rather than storing a
      // track list — a fake that kept a list would let a screen pass here and
      // fail against a library that had grown.
      // --- Downloads ------------------------------------------------------
      //
      // Kept per href, the way the backend keeps them: a track in two
      // downloaded playlists survives removing one of them, and a fake that
      // stored a flag per collection would let a screen pass here and lose
      // somebody's music in the real app.
      case "downloaded_tracks":
        return [...this.pinned];

      case "download_collection": {
        const hrefs = this.collectionTracks(String(a.kind), String(a.id));
        if (hrefs.length === 0) {
          throw new Error("There are no tracks in that to download.");
        }
        hrefs.forEach((h) => this.pinned.add(h));
        return null;
      }

      case "remove_download": {
        const hrefs = this.collectionTracks(String(a.kind), String(a.id));
        const wanted = new Set(
          this.playlists
            .filter((p) => !(a.kind === "playlist" && p.id === a.id))
            .flatMap((p) => p.tracks),
        );
        let removed = 0;
        for (const h of hrefs) {
          if (wanted.has(h)) continue;
          if (this.pinned.delete(h)) removed += 1;
        }
        return removed;
      }

      case "duplicate_count": {
        // Keyed on title and artist, not on the filename — a second copy is
        // called "Thing (1)" and would otherwise count as its own record.
        const seen = new Set<string>();
        let dupes = 0;
        for (const r of this.rows) {
          const key = `${r.title.trim().toLowerCase()}\u0001${r.artist.trim().toLowerCase()}`;
          if (seen.has(key)) dupes += 1;
          else seen.add(key);
        }
        return dupes;
      }

      case "dynamic_groups":
        return this.groups;

      case "create_group": {
        const name = String(a.name ?? "").trim();
        if (!name) throw new Error("A group needs a name.");
        const made: core.DynamicGroup = {
          id: `group-${this.nextId++}`,
          name,
          entities: [],
        };
        this.groups = [made, ...this.groups];
        return made;
      }

      case "rename_group": {
        const name = String(a.name ?? "").trim();
        if (!name) throw new Error("A group needs a name.");
        const g = this.groups.find((x) => x.id === a.id);
        if (!g) return false;
        g.name = name;
        return true;
      }

      case "delete_group": {
        const before = this.groups.length;
        this.groups = this.groups.filter((x) => x.id !== a.id);
        return this.groups.length !== before;
      }

      case "add_to_group": {
        const kind = String(a.entityType ?? "");
        if (!["artist", "album", "genre"].includes(kind)) {
          throw new Error(
            `A dynamic group holds artists, albums and genres. "${kind}" is none of those.`,
          );
        }
        const value = String(a.value ?? "").trim();
        if (!value) throw new Error("There is nothing named here to add.");
        const g = this.groups.find((x) => x.id === a.id);
        if (!g) return false;
        // Idempotent, as the store is.
        if (
          g.entities.some((e) => e.entityType === kind && e.value === value)
        ) {
          return false;
        }
        g.entities = [
          ...g.entities,
          { entityType: kind as core.EntityType, value },
        ];
        return true;
      }

      case "remove_from_group": {
        const g = this.groups.find((x) => x.id === a.id);
        if (!g) return false;
        const before = g.entities.length;
        g.entities = g.entities.filter(
          (e) => !(e.entityType === a.entityType && e.value === a.value),
        );
        return g.entities.length !== before;
      }

      case "reorder_groups": {
        const from = Number(a.from);
        const to = Number(a.to);
        if (
          from < 0 ||
          to < 0 ||
          from >= this.groups.length ||
          to >= this.groups.length
        ) {
          return false;
        }
        const next = [...this.groups];
        const [moved] = next.splice(from, 1);
        if (moved) next.splice(to, 0, moved);
        this.groups = next;
        return true;
      }

      case "group_tracks": {
        // Which tracks a group resolves to is `vapor_library::group`, matching
        // entities against the index — a union across artists, albums and
        // genres, with rules about unknown fields that this restated and could
        // get wrong. The screen's half is drawing what it is handed and saying
        // how many, so it is handed the whole library and a test that cares
        // scripts a narrower answer.
        return this.groups.some((x) => x.id === a.id) ? this.rows : [];
      }

      case "playlist_folders":
        return this.folders;

      case "create_folder": {
        const name = String(a.name ?? "").trim();
        if (!name) throw new Error("A folder needs a name.");
        const made: core.Folder = {
          id: `folder-${this.nextId++}`,
          name,
          parentId: "",
        };
        // Newest first, as the store inserts them.
        this.folders = [made, ...this.folders];
        return made;
      }

      case "rename_folder": {
        const name = String(a.name ?? "").trim();
        if (!name) throw new Error("A folder needs a name.");
        const id = String(a.id ?? "");
        if (!this.folders.some((f) => f.id === id)) return false;
        this.folders = this.folders.map((f) =>
          f.id === id ? { ...f, name } : f,
        );
        return true;
      }

      case "delete_folder": {
        const id = String(a.id ?? "");
        if (!this.folders.some((f) => f.id === id)) return false;
        const orphaned = this.folders
          .filter((f) => f.parentId === id)
          .map((f) => f.id);
        this.folders = this.folders
          .filter((f) => f.id !== id)
          .map((f) => (f.parentId === id ? { ...f, parentId: "" } : f));
        // Deleting a container does not delete what it contains.
        const gone = new Set([id, ...orphaned]);
        this.playlists = this.playlists.map((p) =>
          gone.has(p.folderId) ? { ...p, folderId: "" } : p,
        );
        return true;
      }

      case "set_playlist_folder": {
        const id = String(a.id ?? "");
        const folderId = String(a.folderId ?? "");
        if (folderId && !this.folders.some((f) => f.id === folderId)) {
          throw new Error("That folder no longer exists.");
        }
        if (!this.playlists.some((p) => p.id === id)) return false;
        this.playlists = this.playlists.map((p) =>
          p.id === id ? { ...p, folderId } : p,
        );
        return true;
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
        this.scope = (a.scope as string | null) ?? "";
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
          // No plausibility range here. The real command refuses a tempo
          // outside its bounds rather than clamping, and those bounds are a
          // Rust constant — restating them meant two numbers that had to be
          // changed together and nothing saying so. A test about the refusal
          // asks for one with `backend.fail`.
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
        const nextHref = this.current
          ? (this.queue[this.queue.indexOf(this.current) + 1] ?? null)
          : null;
        const nextRow = this.rows.find((r) => r.href === nextHref);
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
          // Six fields the mock had simply never grown, and nothing noticed
          // because its response was unchecked. Anything reading them got
          // `undefined` here and a number in the real app — which is how a
          // component can pass its tests and still divide by nothing.
          brightness: 0,
          beatPeriod: 0,
          nextBeat: 0,
          setIndex: 0,
          setTotal: 0,
          setEnergy: 0,
          waveform: [],
          nextTitle: nextRow?.title ?? "",
          nextArtist: nextRow?.artist ?? "",
          nextAlbum: nextRow?.album ?? "",
          nextHref: nextRow?.href ?? "",
          cover: null,
          scope: this.scope,
        } satisfies core.PlaybackState;
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
        } satisfies core.VibePath;
      }

      case "blend_preview": {
        if (!this.current) return null;
        const at = this.queue.indexOf(this.current);
        const from = this.rows.find((r) => r.href === this.current);
        const to = this.rows.find((r) => r.href === this.queue[at + 1]);
        if (!from || !to) return null;
        // Whether two tracks can be bridged is `MAX_STRETCH` and the engine's
        // own arithmetic. This used to carry its own copy of the 6% figure,
        // which is a constant living in two languages waiting to disagree.
        // Scripted when a test is about what the panel says.
        return {
          fromTitle: from.title,
          toTitle: to.title,
          fromBpm: from.bpm,
          toBpm: to.bpm,
          fromKey: from.key,
          toKey: to.key,
          shiftPercent: 0,
          gainDelta: 0,
          matchable: true,
          reason: "",
          transition: "Standard Crossfade",
        } satisfies core.BlendPreview;
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
          // Honours `covers` like every other cover-bearing command. It was
          // hard-coded to null, so this screen could not tell a track with
          // artwork from one without.
          cover: this.covers ? A_SLEEVE : null,
          notes: null,
          tagged: false,
        } satisfies core.TrackDetails;
      }

      case "data_breakdown":
        return [
          {
            label: "Audio cache",
            path: "/tmp/vapor-music/cache",
            bytes: this.cacheBytes,
            local: this.cacheBytes > 0,
          },
          {
            label: "Cover art",
            path: "/tmp/vapor-music/covers",
            bytes: this.coverBytes,
            local: this.coverBytes > 0,
          },
          {
            label: "Analysis",
            path: "/tmp/vapor-music/analysis.json",
            bytes: this.deleted ? 0 : 240_000,
            local: !this.deleted,
          },
        ];

      case "reveal_data_folder":
        return null;

      // --- analysis, cache, data ---------------------------------------
      case "analyse_library":
        this.analysed = this.rows.length;
        return null;

      case "analysis_status":
        return {
          analysed: this.analysed,
          total: this.scanned ? this.rows.length : 0,
          running: this.analysing,
          current: this.analysingTitle,
          stoppedBecause: "",
        };

      case "cache_status":
        return {
          bytes: this.cacheBytes,
          maxBytes: this.settings.cacheMaxBytes,
          tracksCached: this.scanned ? this.rows.length : 0,
          tracksTotal: this.scanned ? this.rows.length : 0,
          location: "/tmp/vapor-music/cache",
        };

      case "set_cache_max_bytes":
        this.settings = {
          ...this.settings,
          cacheMaxBytes: Number(a.bytes ?? 0),
        };
        return this.settings;

      case "clear_audio_cache": {
        const freed = this.cacheBytes;
        // The real command deletes files and returns what it freed. It used to
        // be modelled as a number with no effect, so the screen's claim that
        // the cache was empty could never be contradicted here.
        if (!this.cacheResistsClearing) this.cacheBytes = 0;
        return freed;
      }

      case "clear_cover_art": {
        const freed = this.coverBytes;
        // Modelled with an effect for the same reason as `clear_audio_cache`
        // above: a screen that says the artwork is gone should be able to be
        // wrong here.
        this.coverBytes = 0;
        return freed;
      }

      case "data_location":
        return "/tmp/vapor-music";

      case "delete_all_data":
        this.password = null;
        this.rows = [];
        this.playlists = [];
        this.scanned = false;
        this.cacheBytes = 0;
        this.coverBytes = 0;
        this.deleted = true;
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

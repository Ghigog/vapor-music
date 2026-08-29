/**
 * Library — the home screen.
 *
 * Built from the "Library — home" screen in the Daylight design. The album
 * grid is the default view; the flat track table is the separate Songs screen.
 *
 * The header is the design's central claim made literal: a green dot and
 * "N tracks · on this device · no account". That line is the reason `--sov`
 * exists and the only place on this screen it is allowed to appear.
 */

import { useEffect, useMemo, useState } from "react";
import * as core from "../lib/core";
import { Cover } from "../components/Cover";
import { useEntityDrag } from "../components/entityDrag";
import { Home, forgetHomeShelves } from "./Home";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import { Songs } from "./Songs";
import type { AlbumTrack, GroupBy, LibraryEntity, LibrarySection, Row } from "../lib/core";

/**
 * Tabs map onto the index's grouping, so the UI cannot invent a category.
 *
 * "Songs" was called "All" and showed the same card grid ungrouped, while a
 * separate Songs screen in the sidebar held the actual table. The Daylight
 * design has no such screen — its Library tabs are Albums, Songs, Artists,
 * Playlists, and the flat list is one of them (docs/FINDINGS.md). It is one
 * of them here now too, and it renders the table.
 */
/**
 * Which tab is showing.
 *
 * Playlists were briefly one of these. They are a collection you pick from
 * rather than a way of grouping the library, and they have their own tab in the
 * bar now, which opens a list instead of a screen — five pills also wrapped
 * onto two rows at a phone's width.
 */
/**
 * Which tab of the library is showing.
 *
 * Exported because App holds it. It is a place — going back from a track's
 * liner notes should return to the tab you left, and when this was local state
 * it could not, because opening liner notes unmounts Library and a remount
 * starts at the default. You left from Songs and came back to Albums.
 *
 * "home" is the one that is not a grouping. It is what the screen opens on and
 * what almost every visit wants — see `Home` — and the four that group the
 * library are what is left for the rare visit that is looking for a particular
 * record.
 */
export type Tab = "home" | GroupBy;

const TABS: ReadonlyArray<{ id: Tab; label: string }> = [
  { id: "home", label: "Home" },
  { id: "album", label: "Albums" },
  { id: "artist", label: "Artists" },
  { id: "genre", label: "Genres" },
  { id: "none", label: "Songs" },
];

type Load =
  | { kind: "loading" }
  | { kind: "ready"; sections: LibrarySection[] }
  | { kind: "error"; message: string };

/** Which tabs list entities rather than tracks. */
/**
 * Tabs that list *things* rather than tracks.
 *
 * Genres belongs here and did not: the tab rendered the plain row grid, so
 * "Genres" showed a card per track — every song in the library, captioned with
 * its artist. It had been that way since the port. `library_entities` now
 * groups by genre as well, and this is the other half of it.
 */
function isEntityTab(group: GroupBy): group is "album" | "artist" | "genre" {
  return group === "album" || group === "artist" || group === "genre";
}

/**
 * Whole records, then the ones with gaps in them.
 *
 * A lot of this library arrived as one-off downloads — a single track of a
 * nineteen-track album, filed under that album's name. Shown in the same shape
 * as a complete record, those tiles claim something untrue, and there are
 * enough of them to bury the albums actually owned.
 *
 * The backend has already put them in order and marked them; this only draws
 * the line. It returns a single unheaded group for every other tab, and for an
 * Albums tab where nothing is missing — a heading over the whole list would be
 * a label, not a division.
 */
function splitByCompleteness(
  entities: LibraryEntity[],
): { header: string; items: LibraryEntity[] }[] {
  const whole = entities.filter((e) => !e.incomplete);
  const partial = entities.filter((e) => e.incomplete);
  if (partial.length === 0) return [{ header: "", items: whole }];
  // `whole` can be empty — a library where every album is a stray track — and
  // an empty grid above the heading would be a hole on the screen.
  return [
    ...(whole.length > 0 ? [{ header: "", items: whole }] : []),
    { header: "Incomplete", items: partial },
  ];
}

/**
 * The album or artist being looked inside.
 *
 * Exported because App holds it: it is a place in the app, so it belongs in the
 * history entry with the others. See `opened` below.
 */
export type Opened = {
  kind: "album" | "artist" | "genre";
  name: string;
  /** Any track on it — enough to resolve artwork, since an album's identity
   *  is its title plus the folder its tracks live in. */
  lead: string;
  /** The album's artist, for an artwork search. Empty for an artist tile. */
  artist: string;
};

/**
 * The last few library reads, kept across unmounts.
 *
 * Library is unmounted whenever a drill-down covers it — liner notes, a
 * playlist, an album — so returning to it used to mean a fresh round trip and
 * a spinner every time, for a list that had not changed. What is on screen is
 * painted from here first and corrected when the answer arrives, so going back
 * is instant and still ends up truthful.
 *
 * Bounded for the reason `lib/artwork.ts` bounds its own: a 563-row section
 * list is around a hundred kilobytes, and remembering every query someone
 * typed is how a cache becomes a leak. Insertion order is close enough to
 * least-recently-used for a handful of entries.
 */
const READS = 8;
const viewCache = new Map<string, LibrarySection[]>();
const entityCache = new Map<string, LibraryEntity[]>();

/**
 * Throw away the remembered reads.
 *
 * Called by whatever changes what a read would return — a scan, a corrected
 * BPM. Without this the cache would paint a list that is known to be wrong,
 * and although the background fetch corrects it within a frame, painting a
 * deleted track even briefly is the kind of thing that reads as a bug.
 */
export function forgetLibraryReads() {
  viewCache.clear();
  entityCache.clear();
  // The shelves are a read of the same library and go stale on the same
  // events. They are remembered in their own screen, so they are dropped from
  // here rather than kept in the maps above — a `HomeShelves` is not a
  // `LibrarySection[]` and would need a third cache to pretend otherwise.
  forgetHomeShelves();
}

function cacheKey(groupBy: GroupBy, query: string) {
  return `${groupBy}\u0000${query}`;
}

function remember<T>(cache: Map<string, T>, key: string, value: T) {
  cache.delete(key);
  cache.set(key, value);
  for (const oldest of cache.keys()) {
    if (cache.size <= READS) break;
    cache.delete(oldest);
  }
}

export function Library({
  onOpen,
  opened: controlledOpened,
  onOpenedChange,
  tab: controlledTab,
  onTabChange,
  onOpenPlaylist,
  onOpenGroup,
}: {
  /** Opens a track's liner notes — the table's double-click. */
  onOpen?: ((href: string) => void) | undefined;
  /**
   * The drill-down, when the caller owns it.
   *
   * It began as local state, which meant the back gesture could not see it:
   * opening an album and pressing back left the app entirely, because as far
   * as the history stack was concerned nothing had happened. App owns it now
   * for the same reason it already owned Liner Notes and playlists.
   *
   * Both props are optional and Library keeps its own copy when they are
   * absent, so it still stands up on its own in a test.
   */
  opened?: Opened | null | undefined;
  onOpenedChange?: ((opened: Opened | null) => void) | undefined;
  /** The open tab, when the caller owns it — same arrangement as `opened`,
   *  and optional for the same reason: Library still stands alone in a test. */
  tab?: Tab | undefined;
  onTabChange?: ((tab: Tab) => void) | undefined;
  /**
   * Open a playlist or a smart group from a home shelf.
   *
   * Both are drill-downs App owns, like liner notes — they render in front of
   * this screen rather than inside it, so Library can only ask. Optional, and
   * absent the tiles are inert: the shelves still draw, which is what a test
   * that renders Library on its own gets.
   */
  onOpenPlaylist?: ((id: string) => void) | undefined;
  onOpenGroup?: ((id: string) => void) | undefined;
}) {
  const [query, setQuery] = useState("");
  /**
   * The query the reads actually use.
   *
   * Debouncing lived inside the fetch effects, which meant *every* read waited
   * 120ms — including the one on mount, and the one after a tab press, neither
   * of which is a burst of keystrokes. Returning to the library paid it on top
   * of the round trip. Only typing waits now.
   */
  const [settledQuery, setSettledQuery] = useState("");
  const [ownOpened, setOwnOpened] = useState<Opened | null>(null);
  const opened = onOpenedChange ? (controlledOpened ?? null) : ownOpened;
  const setOpened = (next: Opened | null) => {
    if (onOpenedChange) onOpenedChange(next);
    else setOwnOpened(next);
  };
  const [entities, setEntities] = useState<LibraryEntity[] | null>(null);
  const [ownTab, setOwnTab] = useState<Tab>("home");
  const tab = onTabChange ? (controlledTab ?? "home") : ownTab;
  const setTab = (next: Tab) => {
    if (onTabChange) onTabChange(next);
    else setOwnTab(next);
  };
  /**
   * The grouping the reads below use.
   *
   * Home is not one, and the two effects that read the library skip it
   * entirely — a shelf of twelve albums does not need five hundred rows
   * fetched to draw it. Albums stands in only so this stays a `GroupBy`; on
   * home nothing looks at it.
   */
  const groupBy: GroupBy = tab === "home" ? "album" : tab;
  const [load, setLoad] = useState<Load>({ kind: "loading" });
  /** A failure to start playback belongs on screen, not in the console. */
  const [playError, setPlayError] = useState<string | null>(null);

  /**
   * A scan or a correction while this screen is open.
   *
   * The reads are keyed on query and tab, so neither effect re-runs when the
   * *library* changes underneath them. `nonce` is a dependency that exists to
   * be changed, which is what makes them run again.
   */
  const [nonce, setNonce] = useState(0);
  useEffect(() => {
    const handler = () => {
      forgetLibraryReads();
      setNonce((n) => n + 1);
    };
    window.addEventListener("vapor:library-changed", handler);
    return () => window.removeEventListener("vapor:library-changed", handler);
  }, []);

  // 120ms is below the threshold where typing feels laggy but high enough to
  // collapse a burst of keystrokes into one round trip.
  useEffect(() => {
    if (query === settledQuery) return;
    const t = setTimeout(() => setSettledQuery(query), 120);
    return () => clearTimeout(t);
  }, [query, settledQuery]);

  useEffect(() => {
    // Home reads its own shelves and nothing else. Without this the front
    // door would pull the whole index across on every visit to draw four
    // rows of pictures — and searching from it hands the query to the Songs
    // table below, which does its own reading.
    if (tab === "home") return;
    let cancelled = false;
    const key = cacheKey(groupBy, settledQuery);

    // Paint what was there before, if anything, rather than a spinner. The
    // request still goes out — this is the previous answer, not the final one.
    const known = viewCache.get(key);
    setLoad(known ? { kind: "ready", sections: known } : { kind: "loading" });

    core
      .libraryView({ query: settledQuery, groupBy, sortKey: "title", ascending: true })
      .then((sections) => {
        remember(viewCache, key, sections);
        if (!cancelled) setLoad({ kind: "ready", sections });
      })
      .catch((e: unknown) => {
        // A failed refresh must not blank a list that is already on screen:
        // stale is better than empty, and the next attempt corrects it.
        if (!cancelled && !known) setLoad({ kind: "error", message: messageOf(e) });
      });

    return () => {
      cancelled = true;
    };
  }, [settledQuery, groupBy, nonce, tab]);

  /** The albums or artists for the current tab. */
  useEffect(() => {
    if (tab === "home" || !isEntityTab(groupBy)) {
      setEntities(null);
      return;
    }
    let cancelled = false;
    const key = cacheKey(groupBy, settledQuery);

    const known = entityCache.get(key);
    setEntities(known ?? null);

    core
      .libraryEntities({ query: settledQuery, groupBy, sortKey: "title", ascending: true })
      .then((list) => {
        remember(entityCache, key, list);
        if (!cancelled) setEntities(list);
      })
      .catch(() => {
        // The grid falls back to the row view's error, which is the same
        // request against the same index.
        if (!cancelled && !known) setEntities([]);
      });
    return () => {
      cancelled = true;
    };
  }, [settledQuery, groupBy, nonce, tab]);

  /**
   * Play a card, queueing everything currently on screen behind it.
   *
   * The same rule as the Songs table: what you can see is what goes in the
   * queue, in the order you can see it. Library previously had no click
   * handling at all — the cards were an `<article>` with no interaction, so
   * the home screen, the first thing anyone sees, could not start a track.
   */
  async function play(href: string, section?: LibrarySection) {
    /*
     * A grouped tab queues the group, not the screen.
     *
     * Genres are the case: the cards are laid out under a heading per genre,
     * and queueing everything visible meant pressing a house record and
     * getting the whole library behind it, conducted across all of it. The
     * heading is the scope, so it is what goes in.
     */
    const source =
      section ??
      (load.kind === "ready" ? { header: "", rows: load.sections.flatMap((s) => s.rows) } : null);
    try {
      await core.playTracks(
        source?.rows.map((r) => r.href) ?? [],
        href,
        source?.header || undefined,
      );
    } catch (e: unknown) {
      setPlayError(messageOf(e));
    }
  }

  /**
   * Play an album or artist from its first track.
   *
   * Queues that entity's tracks only. Queueing everything on screen would mean
   * pressing one album and getting the whole library behind it.
   */
  async function playEntity(entity: LibraryEntity) {
    try {
      const sections = await core.libraryView({
        groupBy: "none",
        sortKey: "title",
        ascending: true,
        ...(groupBy === "album" ? { album: entity.name } : { artist: entity.name }),
      });
      const hrefs = sections[0]?.rows.map((r) => r.href) ?? [];
      await core.playTracks(hrefs, entity.lead, entity.name);
    } catch (e: unknown) {
      setPlayError(messageOf(e));
    }
  }

  /**
   * How big the library is, for the line under the title.
   *
   * Two sources because the two views read different things. A grouping tab
   * has the rows in hand and counts them, which is also why the number narrows
   * as you type — the count is of what you are looking at. Home never fetches
   * rows, so its count comes back with the shelves; it is the whole library,
   * because that is what home is showing you the front of.
   */
  const rowCount = useMemo(() => {
    if (load.kind !== "ready") return 0;
    return load.sections.reduce((n, s) => n + s.rows.length, 0);
  }, [load]);
  const [homeTracks, setHomeTracks] = useState(0);
  const trackCount = tab === "home" ? homeTracks : rowCount;

  return (
    <div className="library">
      <header className="library__head">
        <div className="library__sov">
          <span className="library__sov-dot" aria-hidden="true" />
          <span className="numeric">
            {trackCount.toLocaleString()} tracks · on this device · no account
          </span>
        </div>
        <h1 className="library__title">Your library</h1>
      </header>

      <div className="library__search glass">
        <SearchIcon />
        <input
          className="library__search-input"
          type="search"
          value={query}
          placeholder="Search your library"
          onChange={(e) => {
            setQuery(e.target.value);
            // Searching means "show me something else".
            setOpened(null);
          }}
          // The library is local, so there is nothing to autocomplete against
          // and nothing to send anywhere.
          autoComplete="off"
          spellCheck={false}
        />
      </div>

      <div className="library__tabs" role="tablist">
        {TABS.map((item) => (
          <button
            key={item.id}
            role="tab"
            aria-selected={tab === item.id}
            className={
              "library__tab" + (tab === item.id ? " library__tab--on" : "")
            }
            onClick={() => {
              setTab(item.id);
              // As does changing tab.
              setOpened(null);
            }}
          >
            {item.label}
          </button>
        ))}
      </div>

      {/* Inside an album or an artist: the same table, narrowed to it. */}
      {opened ? (
        <div className="library__body">
          <div className="library__opened-head">
            <div className="library__crumb">
              <button className="library__back" onClick={() => setOpened(null)}>
                {/* Named for where pressing it lands, not for what is open.
                    Closing this returns to the tab underneath, and an album
                    opened from a home shelf goes back to the shelf — a crumb
                    reading "Albums" there would be pointing at a tab the
                    press does not visit. */}
                ‹{" "}
                {tab === "home"
                  ? "Home"
                  : opened.kind === "album"
                    ? "Albums"
                    : opened.kind === "artist"
                      ? "Artists"
                      : "Genres"}
              </button>
              <h2 className="library__opened">{opened.name}</h2>
            </div>
            {opened.kind === "album" && (
              <AlbumArtwork album={opened.name} artist={opened.artist} lead={opened.lead} />
            )}
          </div>
          {opened.kind === "album" ? (
            /* An album knows how long it is supposed to be, so it can show the
               tracks it is missing. Everything else falls straight through to
               the table — an artist has no length to fall short of. */
            <AlbumTracks
              album={opened.name}
              lead={opened.lead}
              onOpen={onOpen}
              onError={setPlayError}
            />
          ) : (
            <Songs
              onOpen={onOpen}
              query=""
              filter={
                opened.kind === "artist"
                  ? { artist: opened.name }
                  : { genre: opened.name }
              }
              // Playing from inside an opened record conducts within it.
              scope={opened.name}
            />
          )}
        </div>
      ) : tab === "home" ? (
        /* Typing on home is a search, not a filter of the shelves.
         *
         * A shelf holds the first dozen of something ranked by plays, so
         * narrowing one would answer "you have no such album" for an album
         * that is right there in the library, thirteenth. Search is its own
         * result, which is what every other player does with the same field
         * and the same shelves. The table is the same one the Songs tab
         * shows, and it does its own reading. */
        query.trim() ? (
          <Songs onOpen={onOpen} query={query} />
        ) : (
          <Home
            onOpenPlaylist={onOpenPlaylist ?? (() => {})}
            onOpenGroup={onOpenGroup ?? (() => {})}
            onOpenEntity={setOpened}
            onTracks={setHomeTracks}
          />
        )
      ) : groupBy === "none" ? (
        /* The flat list is the table, not an ungrouped grid of cards: the whole
           point of it is the columns — artist, album, tempo, key — and sorting
           by them. It gets the search field above rather than one of its own. */
        <Songs onOpen={onOpen} query={query} />
      ) : isEntityTab(groupBy) ? (
        <div className="library__body">
          <ErrorNotice error={playError} onDismiss={() => setPlayError(null)} />

          {entities === null && <p className="label">reading library</p>}

          {entities !== null && entities.length === 0 && (
            <EmptyLibrary query={query} kind={groupBy} />
          )}

          {entities !== null &&
            entities.length > 0 &&
            splitByCompleteness(entities).map(({ header, items }) => (
              <section key={header || "whole"} className="library__section">
                {header && (
                  <h2 className="library__section-head label">
                    {header} <span className="library__section-count">{items.length}</span>
                  </h2>
                )}
                <div className="library__grid">
                  {items.map((entity) => (
                    <EntityCard
                      /* Name *and* subtitle. Two albums can share a title — the
                         backend already keeps them apart by artist, so keying on
                         name alone collided them back together and React warned
                         about duplicate keys on exactly the case the
                         album-identity test covers. */
                      key={`${entity.name}\u0000${entity.subtitle}`}
                      entity={entity}
                      kind={groupBy}
                      onOpen={() =>
                        setOpened({
                          kind: groupBy,
                          name: entity.name,
                          lead: entity.lead,
                          artist: groupBy === "album" ? entity.subtitle : entity.name,
                        })
                      }
                      onPlay={() => void playEntity(entity)}
                    />
                  ))}
                </div>
              </section>
            ))}
        </div>
      ) : (
        <div className="library__body">
          <ErrorNotice error={playError} onDismiss={() => setPlayError(null)} />

          {load.kind === "loading" && <p className="label">reading library</p>}

          {load.kind === "error" && (
            <div className="library__empty">
              <p className="library__empty-title">Could not read the library</p>
              <ErrorNotice error={load.message} />
            </div>
          )}

          {load.kind === "ready" && trackCount === 0 && <EmptyLibrary query={query} />}

          {load.kind === "ready" &&
            trackCount > 0 &&
            load.sections.map((section) => (
              <section key={section.header || "all"} className="library__section">
                {section.header && (
                  <h2 className="library__section-head label">{section.header}</h2>
                )}
                <div className="library__grid">
                  {section.rows.map((row) => (
                    <Card
                      key={row.href}
                      row={row}
                      onPlay={() => void play(row.href, section)}
                    />
                  ))}
                </div>
              </section>
            ))}
        </div>
      )}
    </div>
  );
}

/**
 * Two empty states, not one.
 *
 * "No library yet" and "nothing matched that search" are different problems
 * with different fixes, and collapsing them into one message tells a person
 * with a full library that their music is missing.
 */
function EmptyLibrary({
  query,
  kind,
}: {
  query: string;
  kind?: "album" | "artist" | "genre";
}) {
  if (query.trim()) {
    return (
      <div className="library__empty">
        <p className="library__empty-title">Nothing matched “{query}”</p>
        <p className="library__empty-body">Try a different word.</p>
      </div>
    );
  }
  // An entity tab can be empty while the library is not: nothing has an album
  // yet because nothing has been analysed, and the files are not filed in
  // folders either. Saying "no music yet" there would be false.
  if (kind) {
    return (
      <div className="library__empty">
        <p className="library__empty-title">
          {kind === "album" ? "No albums yet" : "No artists yet"}
        </p>
        <p className="library__empty-body">
          Tracks are filed under an {kind} once their folder or their tags say
          which one. Everything you have is under Songs.
        </p>
      </div>
    );
  }

  return (
    <div className="library__empty">
      <p className="library__empty-title">No music yet</p>
      <p className="library__empty-body">
        Connect your storage in Settings and Vapor will index it here. Nothing
        leaves your device.
      </p>
    </div>
  );
}

/**
 * The embedded sleeve for a track, fetched on its own.
 *
 * One request per card rather than a field on every row: artwork is capped at
 * 2 MB by the tag reader, and 563 rows carrying covers would move hundreds of
 * megabytes through IPC on each keystroke.
 *
 * Absent is the normal case, not a failure — a track has no cover until
 * analysis has read the file, and a freshly scanned library has none at all.
 * So there is no error state here: the gradient placeholder *is* the answer.
 */
/**
 * An opened album's cover, and the toggle between where it can come from.
 *
 * A file's embedded artwork can simply be wrong — the owner's copy of one album
 * carries an unrelated picture — and no amount of reading the file better fixes
 * that. The only thing that knows the picture is wrong is the person looking at
 * it, so this is the one place where they can say so and have the app go and
 * ask a service.
 *
 * The picture is the control: tapping it swaps between the file's own artwork
 * and a Deezer lookup. It used to be a "Find artwork" button and three lines of
 * body text sitting beside the square, which on a phone was most of the screen
 * spent explaining a thing you could have simply been shown.
 *
 * Searching still sends the artist and album to Deezer, and the label under the
 * picture says which of the two you are looking at. It is deliberately not
 * behind the automatic-lookup setting: tapping *is* the asking that setting
 * exists to require, and answering "turn on a setting first" to "find the real
 * cover" would be a worse app.
 */
function AlbumArtwork({
  album,
  artist,
  lead,
}: {
  album: string;
  artist: string;
  lead: string;
}) {
  const [src, setSrc] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [chosen, setChosen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    setError(null);
    core
      .albumCover(album, lead)
      .then((art) => {
        if (cancelled) return;
        setSrc(art.src);
        setChosen(art.chosen);
      })
      .catch(() => {
        if (!cancelled) setSrc(null);
      });
    return () => {
      cancelled = true;
    };
  }, [album, lead]);

  async function find() {
    setBusy(true);
    setError(null);
    try {
      const art = await core.findAlbumArt(album, artist, lead);
      setSrc(art.src);
      setChosen(art.chosen);
    } catch (e: unknown) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  async function revert() {
    setBusy(true);
    setError(null);
    try {
      const art = await core.clearAlbumArt(album, lead);
      setSrc(art.src);
      setChosen(art.chosen);
    } catch (e: unknown) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="albumart">
      <button
        className="albumart__art"
        onClick={() => void (chosen ? revert() : find())}
        disabled={busy}
        // The whole explanation, in the place it applies to. What this used to
        // say took three lines of body text beside a 132px square and pushed
        // the track list off a phone screen.
        title={
          chosen
            ? "Using artwork from Deezer. Tap for the artwork inside the files."
            : "Using the artwork inside the files. Tap to search Deezer for it."
        }
        aria-label={
          chosen
            ? `Cover of ${album}, from Deezer. Use the artwork inside the files instead.`
            : `Cover of ${album}, from the files. Search Deezer for artwork instead.`
        }
      >
        {src ? (
          <img className="albumart__img" src={src} alt={`Cover of ${album}`} />
        ) : (
          <div className="card__art-sheen" aria-hidden="true" />
        )}
        {/* Which of the two you are looking at. Two words, over the picture, so
            saying it costs no layout. */}
        <span className="albumart__source">
          {busy ? "Searching…" : chosen ? "Deezer" : "From file"}
        </span>
      </button>
      <ErrorNotice error={error} onDismiss={() => setError(null)} />
    </div>
  );
}

/**
 * One album or artist.
 *
 * Two verbs, so both are reachable: pressing the card opens it, and the play
 * button on the sleeve starts it. Collapsing those into one gesture would mean
 * either you cannot see what is on an album without playing it, or you cannot
 * play it without going in first.
 *
 * A third thing you can do with it is take it somewhere — onto a group in the
 * rail, which is what a group is made of, or onto a playlist, which resolves
 * it to the tracks on it. See `useEntityDrag`.
 */
/**
 * An opened album, drawn as the record rather than as the files.
 *
 * The whole point is the gaps. A library holding 1 of 19 of *Split The Atom*
 * used to draw a one-row table, which looks exactly like a complete album with
 * one track on it — there was nothing on the screen to say the other eighteen
 * existed. Now they are all there, and the ones not held are greyed and inert.
 *
 * Falls back to the ordinary table when the album was never matched to a
 * release, which is every album until the identify pass has run. Inventing a
 * tracklist from the files to hand would be the one list guaranteed to have no
 * gaps in it.
 */
function AlbumTracks({
  album,
  lead,
  onOpen,
  onError,
}: {
  album: string;
  lead: string;
  onOpen?: ((href: string) => void) | undefined;
  onError: (message: string) => void;
}) {
  const [tracks, setTracks] = useState<AlbumTrack[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    setTracks(null);
    core
      .albumTracklist(album, lead)
      // An empty answer is "never looked up", which the render below reads as
      // "fall back to the table" — the same branch a failure takes, because a
      // person can do exactly the same thing about both.
      .then((list) => !cancelled && setTracks(list))
      .catch(() => !cancelled && setTracks([]));
    return () => {
      cancelled = true;
    };
  }, [album, lead]);

  if (tracks === null) return <p className="label">reading album</p>;
  if (tracks.length === 0) {
    return <Songs onOpen={onOpen} query="" filter={{ album }} scope={album} />;
  }

  const held = tracks.filter((t) => t.href);
  const missing = tracks.length - held.length;

  async function play(href: string) {
    try {
      // Only what is actually here goes in the queue, in album order. The
      // missing rows are on the screen to be seen, not to be played past.
      await core.playTracks(held.map((t) => t.href), href, album);
    } catch (e: unknown) {
      onError(messageOf(e));
    }
  }

  return (
    <div className="tracklist">
      {missing > 0 && (
        <p className="tracklist__gap label">
          {held.length} of {tracks.length} tracks — {missing} not in your library
        </p>
      )}
      <ol className="tracklist__list">
        {tracks.map((track, i) => (
          <li
            /* Position is not unique: a held track the release does not list
               comes back as 0, and there can be several. Index is stable here
               because the list is replaced wholesale, never reordered. */
            key={`${i}\u0000${track.title}`}
            className={"tracklist__row" + (track.href ? "" : " tracklist__row--absent")}
          >
            <span className="tracklist__no">{track.position || "—"}</span>
            {track.href ? (
              <button
                type="button"
                className="tracklist__title"
                onClick={() => void play(track.href)}
              >
                {track.title}
              </button>
            ) : (
              /* Not a disabled button. There is no action here to disable —
                 the track is not in the library — and a disabled control
                 invites a person to work out what would enable it. A plain
                 span with the reason attached says the true thing. */
              <span className="tracklist__title tracklist__title--absent">
                {track.title}
                {/* Said in text, not conveyed by the grey alone. Colour is not
                    available to everyone, and "not in your library" is the
                    entire reason this row cannot be pressed. The class hides it
                    from sight without hiding it from assistive tech —
                    `display: none` would take it out of the accessibility tree
                    too, which is the opposite of the point. */}
                <span className="tracklist__absent-note"> — not in your library</span>
              </span>
            )}
          </li>
        ))}
      </ol>
    </div>
  );
}

function EntityCard({
  entity,
  kind,
  onOpen,
  onPlay,
}: {
  entity: LibraryEntity;
  kind: "album" | "artist" | "genre";
  onOpen: () => void;
  onPlay: () => void;
}) {
  const noun = kind;
  const pickUp = useEntityDrag(kind, entity.name);
  return (
    <div
      className={"card card--entity" + (kind === "artist" ? " card--round" : "")}
      {...pickUp}
    >
      <button
        type="button"
        className="card__open"
        onClick={onOpen}
        aria-label={`Open the ${noun} ${entity.name}`}
      >
        <Cover
          href={entity.lead}
          label={entity.name}
          {...(kind === "artist" ? { artist: entity.name } : {})}
        />
      </button>
      <button
        type="button"
        className="card__play"
        onClick={onPlay}
        aria-label={`Play ${entity.name}`}
      >
        <span className="icon icon--play" aria-hidden="true" />
      </button>
      <div className="card__meta">
        <span className="card__title" title={entity.name}>
          {entity.name}
        </span>
        <span className="card__sub" title={entity.subtitle}>
          {entity.subtitle ||
            `${entity.tracks} ${entity.tracks === 1 ? "track" : "tracks"}`}
        </span>
        {/* How much of it is actually here.
         *
         * Under the heading rather than instead of it: "Incomplete" says which
         * pile a record is in, and this says how far off it is — 4 of 8 is a
         * different proposition from 1 of 19, and the sort order alone cannot
         * tell you which you are looking at. `recordType` is named because
         * missing one track of a two-track single is not the same failure as
         * missing eighteen of an album. */}
        {entity.incomplete && (
          <span className="card__gap">
            {entity.tracks} of {entity.totalTracks}
            {entity.recordType === "single" || entity.recordType === "ep"
              ? ` on this ${entity.recordType.toUpperCase()}`
              : ""}
          </span>
        )}
      </div>
    </div>
  );
}

function Card({ row, onPlay }: { row: Row; onPlay: () => void }) {
  // Unknown fields are rendered as a quiet dash rather than "Unknown Artist" —
  // the index uses the same convention for group headers.
  const artist = row.artistSource === "unknown" ? "—" : row.artist;

  return (
    /* A real button, not an `<article>` with an onClick bolted on: it is
     * focusable, it answers Enter and Space without any code here, and a
     * screen reader announces it as something that can be pressed. The name
     * says what pressing it does, because "Adrenaline" alone does not.
     *
     * Single click, unlike the Songs table's double click. A grid of cards is
     * a set of targets rather than a list with a selection, so there is no
     * second gesture for selecting that a single click would collide with. */
    <button
      type="button"
      className="card"
      onClick={onPlay}
      aria-label={`Play ${row.title}${artist === "—" ? "" : ` by ${artist}`}`}
    >
      <div className="card__art">
        <div className="card__art-sheen" aria-hidden="true" />
        {row.bpm > 0 && (
          <span className="card__badge numeric">{Math.round(row.bpm)}</span>
        )}
      </div>
      <div className="card__meta">
        <span className="card__title" title={row.title}>
          {row.title}
        </span>
        <span className="card__sub" title={artist}>
          {artist}
        </span>
      </div>
    </button>
  );
}

function SearchIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.6" />
      <path
        d="M10.5 10.5L14 14"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
      />
    </svg>
  );
}

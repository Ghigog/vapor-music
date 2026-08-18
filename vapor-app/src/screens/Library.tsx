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
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import { Songs } from "./Songs";
import { PlaylistRail } from "../components/PlaylistRail";
import type { GroupBy, LibraryEntity, LibrarySection, Row } from "../lib/core";

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
 * Playlists are not a way of grouping the library — they are a different list
 * entirely — so this is `GroupBy` plus one, rather than a fifth `GroupBy` that
 * the backend would then have to be taught to reject.
 */
type Tab = GroupBy | "playlist";

const TABS: ReadonlyArray<{ id: Tab; label: string }> = [
  { id: "album", label: "Albums" },
  { id: "artist", label: "Artists" },
  { id: "genre", label: "Genres" },
  { id: "none", label: "Songs" },
  { id: "playlist", label: "Playlists" },
];

type Load =
  | { kind: "loading" }
  | { kind: "ready"; sections: LibrarySection[] }
  | { kind: "error"; message: string };

/** Which tabs list entities rather than tracks. */
function isEntityTab(group: GroupBy): group is "album" | "artist" {
  return group === "album" || group === "artist";
}

/**
 * The album or artist being looked inside.
 *
 * Exported because App holds it: it is a place in the app, so it belongs in the
 * history entry with the others. See `opened` below.
 */
export type Opened = {
  kind: "album" | "artist";
  name: string;
  /** Any track on it — enough to resolve artwork, since an album's identity
   *  is its title plus the folder its tracks live in. */
  lead: string;
  /** The album's artist, for an artwork search. Empty for an artist tile. */
  artist: string;
};

export function Library({
  onOpen,
  onOpenPlaylist,
  opened: controlledOpened,
  onOpenedChange,
}: {
  /** Opens a track's liner notes — the table's double-click. */
  onOpen?: ((href: string) => void) | undefined;
  /** Opens a playlist from the Playlists tab. */
  onOpenPlaylist?: ((id: string) => void) | undefined;
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
}) {
  const [query, setQuery] = useState("");
  const [ownOpened, setOwnOpened] = useState<Opened | null>(null);
  const opened = onOpenedChange ? (controlledOpened ?? null) : ownOpened;
  const setOpened = (next: Opened | null) => {
    if (onOpenedChange) onOpenedChange(next);
    else setOwnOpened(next);
  };
  const [entities, setEntities] = useState<LibraryEntity[] | null>(null);
  const [tab, setTab] = useState<Tab>("album");
  /** What to ask the library for, or null on a tab that is not asking it. */
  const groupBy: GroupBy | null = tab === "playlist" ? null : tab;
  const [load, setLoad] = useState<Load>({ kind: "loading" });
  /** A failure to start playback belongs on screen, not in the console. */
  const [playError, setPlayError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    // Debounced because the table re-runs the whole filter/sort/group pipeline
    // per keystroke. 120ms is below the threshold where typing feels laggy but
    // high enough to collapse a burst of keystrokes into one round trip.
    if (!groupBy) return;

    const t = setTimeout(() => {
      core
        .libraryView({ query, groupBy, sortKey: "title", ascending: true })
        .then((sections) => {
          if (!cancelled) setLoad({ kind: "ready", sections });
        })
        .catch((e: unknown) => {
          if (!cancelled) setLoad({ kind: "error", message: messageOf(e) });
        });
    }, 120);

    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query, groupBy]);

  /** The albums or artists for the current tab. */
  useEffect(() => {
    if (!groupBy || !isEntityTab(groupBy)) {
      setEntities(null);
      return;
    }
    let cancelled = false;
    const t = setTimeout(() => {
      core
        .libraryEntities({ query, groupBy, sortKey: "title", ascending: true })
        .then((list) => {
          if (!cancelled) setEntities(list);
        })
        .catch(() => {
          // The grid falls back to the row view's error, which is the same
          // request against the same index.
          if (!cancelled) setEntities([]);
        });
    }, 120);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query, groupBy]);

  /**
   * Play a card, queueing everything currently on screen behind it.
   *
   * The same rule as the Songs table: what you can see is what goes in the
   * queue, in the order you can see it. Library previously had no click
   * handling at all — the cards were an `<article>` with no interaction, so
   * the home screen, the first thing anyone sees, could not start a track.
   */
  async function play(href: string) {
    const visible =
      load.kind === "ready" ? load.sections.flatMap((s) => s.rows.map((r) => r.href)) : [];
    try {
      await core.playTracks(visible, href);
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
      await core.playTracks(hrefs, entity.lead);
    } catch (e: unknown) {
      setPlayError(messageOf(e));
    }
  }

  const trackCount = useMemo(() => {
    if (load.kind !== "ready") return 0;
    return load.sections.reduce((n, s) => n + s.rows.length, 0);
  }, [load]);

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
                ‹ {opened.kind === "album" ? "Albums" : "Artists"}
              </button>
              <h2 className="library__opened">{opened.name}</h2>
            </div>
            {opened.kind === "album" && (
              <AlbumArtwork album={opened.name} artist={opened.artist} lead={opened.lead} />
            )}
          </div>
          <Songs
            onOpen={onOpen}
            query=""
            filter={
              opened.kind === "album"
                ? { album: opened.name }
                : { artist: opened.name }
            }
          />
        </div>
      ) : tab === "playlist" ? (
        /* The rail the sidebar carries, as a tab.
         *
         * The same component rather than a second list: it already creates,
         * renames, files into folders and takes dropped tracks, and a phone
         * has no sidebar to reach any of that from. */
        <div className="library__body">
          <PlaylistRail activeId={null} onOpen={onOpenPlaylist ?? (() => {})} />
        </div>
      ) : groupBy === "none" ? (
        /* The flat list is the table, not an ungrouped grid of cards: the whole
           point of it is the columns — artist, album, tempo, key — and sorting
           by them. It gets the search field above rather than one of its own. */
        <Songs onOpen={onOpen} query={query} />
      ) : groupBy && isEntityTab(groupBy) ? (
        <div className="library__body">
          <ErrorNotice error={playError} onDismiss={() => setPlayError(null)} />

          {entities === null && <p className="label">reading library</p>}

          {entities !== null && entities.length === 0 && (
            <EmptyLibrary query={query} kind={groupBy} />
          )}

          {entities !== null && entities.length > 0 && (
            <div className="library__grid">
              {entities.map((entity) => (
                <EntityCard
                  key={entity.name}
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
          )}
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
                    <Card key={row.href} row={row} onPlay={() => void play(row.href)} />
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
  kind?: "album" | "artist";
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

function Cover({
  href,
  label,
  artist,
}: {
  href: string;
  label: string;
  /**
   * When this tile is an artist, their name — so a looked-up portrait can
   * stand in where the lead track has no embedded art (TD-53).
   *
   * The file's own artwork still wins. An artist tile falling back to a
   * picture of the *album* the lead track came from is the case this fixes:
   * it looked like the app knew who the artist was, and it did not.
   */
  artist?: string;
}) {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    core
      .trackCover(href)
      .then(async (data) => {
        if (cancelled) return;
        if (data || !artist) {
          setSrc(data);
          return;
        }
        const portrait = await core.artistPortrait(artist).catch(() => null);
        if (!cancelled) setSrc(portrait);
      })
      .catch(() => {
        if (!cancelled) setSrc(null);
      });
    return () => {
      cancelled = true;
    };
  }, [href, artist]);

  return (
    <div className="card__art">
      {src ? (
        <img className="card__img" src={src} alt={`Cover of ${label}`} />
      ) : (
        <div className="card__art-sheen" aria-hidden="true" />
      )}
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
 */
function EntityCard({
  entity,
  kind,
  onOpen,
  onPlay,
}: {
  entity: LibraryEntity;
  kind: "album" | "artist";
  onOpen: () => void;
  onPlay: () => void;
}) {
  const noun = kind === "album" ? "album" : "artist";
  return (
    <div className={"card card--entity" + (kind === "artist" ? " card--round" : "")}>
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
        ▶
      </button>
      <div className="card__meta">
        <span className="card__title" title={entity.name}>
          {entity.name}
        </span>
        <span className="card__sub" title={entity.subtitle}>
          {entity.subtitle ||
            `${entity.tracks} ${entity.tracks === 1 ? "track" : "tracks"}`}
        </span>
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

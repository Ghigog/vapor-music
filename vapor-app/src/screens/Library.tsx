/**
 * Library — the whole of it, on one screen.
 *
 * There used to be five: a tab bar reading Home, Albums, Artists, Genres,
 * Songs, each a different way of laying the same library out, and four of them
 * were places you went once and never again. What almost every visit wants is
 * the same three things in the same order — who, what, and then everything —
 * so that is what this is now, top to bottom: the artists and albums most
 * played, the genres most liked, and the full track table under them.
 *
 * The header is the design's central claim made literal: a green dot and
 * "N tracks · on this device · no account". That line is the reason `--sov`
 * exists and the only place on this screen it is allowed to appear.
 *
 * The page scrolls as one. That is why `Songs` is handed `scroller` rather
 * than left to bring its own scroll region: a virtualized table with an inner
 * scroller inside a scrolling column is two scrollbars for one list, and the
 * shelves would stop moving halfway down while the rows carried on.
 */

import { useEffect, useRef, useState } from "react";
import * as core from "../lib/core";
import { Cover } from "../components/Cover";
import { useEntityDrag } from "../components/entityDrag";
import { Shelves, forgetShelves } from "./Shelves";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import { Songs } from "./Songs";
import type { AlbumTrack, LibraryEntity } from "../lib/core";

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
 * The last few entity reads, kept across unmounts.
 *
 * Library is unmounted whenever a drill-down covers it — liner notes, a
 * playlist, an album — so returning to it used to mean a fresh round trip and
 * a spinner every time, for a list that had not changed. What is on screen is
 * painted from here first and corrected when the answer arrives, so going back
 * is instant and still ends up truthful.
 *
 * Bounded for the reason `lib/artwork.ts` bounds its own: remembering every
 * read anyone ever asked for is how a cache becomes a leak. Insertion order is
 * close enough to least-recently-used for a handful of entries.
 */
const READS = 8;
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
  entityCache.clear();
  // The shelves are a read of the same library and go stale on the same
  // events. They are remembered in their own screen, so they are dropped from
  // here rather than kept in the map above — a `HomeShelves` is not a
  // `LibraryEntity[]` and would need a second cache to pretend otherwise.
  forgetShelves();
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
  /**
   * The picture behind an opened album or artist, for the blurred backdrop.
   *
   * Held here rather than read a second time: `AlbumArtwork` and
   * `ArtistArtwork` already fetch the one picture there is to show, and a
   * second `albumCover`/`artistPortrait` call just to paint the wash behind
   * it would be the same request twice for two halves of one picture.
   */
  const [heroArt, setHeroArt] = useState<string | null>(null);
  /**
   * What an opened artist is filed under, for the line beneath their name.
   *
   * Reported up by `ArtistTracks` rather than read again: it has already
   * fetched every track of theirs, and each row carries its genres. A second
   * round trip to count what is in hand would be the same read twice.
   */
  const [artistGenres, setArtistGenres] = useState<string[]>([]);
  const [ownOpened, setOwnOpened] = useState<Opened | null>(null);
  const opened = onOpenedChange ? (controlledOpened ?? null) : ownOpened;
  const setOpened = (next: Opened | null) => {
    // The wash belongs to what is open. Left standing across a change it
    // paints the previous record's colours behind the new one's name for as
    // long as the new picture takes to arrive. The genre line is the same
    // hazard one line down: it would name the last artist under this one's
    // name until the read lands.
    setHeroArt(null);
    setArtistGenres([]);
    if (onOpenedChange) onOpenedChange(next);
    else setOwnOpened(next);
  };
  /** The genres, ranked by how much this library's owner likes what is in
   *  them. `library_entities` does the ranking; this only draws it. */
  const [genres, setGenres] = useState<LibraryEntity[] | null>(null);
  /**
   * How big the library is, for the line under the title.
   *
   * Its own read, and the cheapest one there is: a window of nothing is a
   * count, which the backend answers without sorting or grouping a row. It
   * used to be reported upwards by whichever view was showing, which meant it
   * read 0 whenever the page opened straight into an album — a state the back
   * gesture reaches every time somebody leaves liner notes.
   */
  const [trackCount, setTrackCount] = useState(0);
  /**
   * The one scroll region on this screen.
   *
   * The table below is virtualized and has to know what it is scrolling
   * inside; handing it this is what lets the shelves, the genres and fifty
   * thousand rows share a single scrollbar. See `Songs`.
   */
  const scrollerRef = useRef<HTMLDivElement>(null);
  /** A failure to start playback belongs on screen, not in the console. */
  const [playError, setPlayError] = useState<string | null>(null);

  /**
   * A scan or a correction while this screen is open.
   *
   * The reads are keyed on the query, so they do not re-run when the *library*
   * changes underneath them. `nonce` is a dependency that exists to be
   * changed, which is what makes them run again.
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

  /**
   * How many tracks there are.
   *
   * A window of zero rows, which the backend short-circuits into a count: no
   * sort, no grouping, no payload. Not narrowed by the search field, because
   * the line it feeds says "on this device" — it is a claim about what is
   * here, not about what matched.
   */
  useEffect(() => {
    let cancelled = false;
    core
      .libraryPage({}, { limit: 0 })
      .then((page) => {
        if (!cancelled) setTrackCount(page.total);
      })
      // A count that could not be read stays as it was. Nothing on this screen
      // depends on it, and a red box over the title would be out of all
      // proportion to a number.
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [nonce]);

  /**
   * The genres, most liked first.
   *
   * Narrowed by the search field like everything else, though the row is
   * hidden while a search is running — the read is keyed on the query anyway,
   * so this is one cache entry rather than a special case.
   */
  useEffect(() => {
    let cancelled = false;
    const key = `genre\u0000${settledQuery}`;

    // Paint what was there before, if anything, rather than nothing. The
    // request still goes out — this is the previous answer, not the final one.
    const known = entityCache.get(key);
    setGenres(known ?? null);

    core
      .libraryEntities({ query: settledQuery, groupBy: "genre" })
      .then((list) => {
        remember(entityCache, key, list);
        if (!cancelled) setGenres(list);
      })
      .catch(() => {
        // No error state: the row is one of three things on a page whose other
        // two are already answering, and a red box where the genres would be
        // says less than their absence does.
        if (!cancelled && !known) setGenres([]);
      });
    return () => {
      cancelled = true;
    };
  }, [settledQuery, nonce]);

  /** Play one album from an artist's shelf, conducted within it. */
  async function playAlbumEntity(entity: LibraryEntity) {
    try {
      const sections = await core.libraryView({
        groupBy: "none",
        sortKey: "title",
        ascending: true,
        album: entity.name,
      });
      const hrefs = sections[0]?.rows.map((r) => r.href) ?? [];
      await core.playTracks(hrefs, entity.lead, entity.name);
    } catch (e: unknown) {
      setPlayError(messageOf(e));
    }
  }

  return (
    <div className="library">
      {/*
        The picture behind everything above the list.

        It was a rounded card around the opened record's name only, which drew
        a second panel inside a screen that already has one — the colours of
        the record stopped at a border a few hundred pixels down, with the
        title and the search field sitting outside it on the plain page. It
        reaches the top of the window now and fades out over the header, so
        what you opened colours the whole top of the app.

        Only when there is a picture to show. A flat placeholder tint here
        (tried once, see history) still ended in an edge of its own a few
        hundred pixels down — the plain page beneath it is the one background
        that never draws a line against anything.

        Decorative: the same picture is legible, unblurred, in the tile below
        it, so it is hidden from assistive tech.
      */}
      {opened && heroArt && (
        <div className="library__wash" aria-hidden="true">
          <img className="library__wash-img" src={heroArt} alt="" />
        </div>
      )}
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

      <div className="library__scroll" ref={scrollerRef}>
        {/* Inside an album, an artist or a genre: the same page, narrowed. */}
        {opened ? (
          <div className="library__body">
            <div
              className={
                "library__opened-head" +
                (opened.kind === "artist" ? " library__opened-head--artist" : "")
              }
            >
              <div className="library__crumb">
                <h2 className="library__opened">{opened.name}</h2>
                {/*
                  The way to the artist, from the album.

                  There was none: an album named its artist nowhere, so the
                  only route to them was back out to the library and finding
                  them among the shelves again. Under the title rather than
                  beside it, because it is the album's subtitle — the same
                  shape the tile on a shelf has.
                */}
                {opened.kind === "album" && opened.artist && (
                  <button
                    className="library__opened-artist"
                    onClick={() =>
                      setOpened({
                        kind: "artist",
                        name: opened.artist,
                        lead: opened.lead,
                        artist: opened.artist,
                      })
                    }
                  >
                    {opened.artist}
                  </button>
                )}
                {/*
                  What an artist is filed under, in the album's subtitle slot.

                  An artist's genre used to be a column repeated down every one
                  of their tracks, which said the same thing eighty times and
                  said it nowhere a person looks for it. It belongs to the
                  artist, so it goes under their name — once.

                  Each name is the same destination the Genres row on this
                  screen gives it, not just a fact about who you are looking
                  at — there was no reason to make it a second, mute way of
                  saying what a press already says elsewhere.

                  Silent until the read lands, and silent for an artist whose
                  tracks carry no genre at all: an empty line reserving its own
                  height under the title reads as something that failed to
                  load.
                */}
                {opened.kind === "artist" && artistGenres.length > 0 && (
                  <p className="library__opened-genres">
                    {artistGenres.map((genre, i) => (
                      <span key={genre}>
                        {i > 0 && " · "}
                        <button
                          type="button"
                          className="library__opened-genre"
                          onClick={() =>
                            setOpened({ kind: "genre", name: genre, lead: "", artist: "" })
                          }
                        >
                          {genre}
                        </button>
                      </span>
                    ))}
                  </p>
                )}
              </div>
              {opened.kind === "album" ? (
                <AlbumArtwork
                  album={opened.name}
                  artist={opened.artist}
                  lead={opened.lead}
                  onArt={setHeroArt}
                />
              ) : opened.kind === "artist" ? (
                <ArtistArtwork name={opened.name} onArt={setHeroArt} />
              ) : null}
            </div>
            {opened.kind === "album" ? (
              /* An album knows how long it is supposed to be, so it can show
                 the tracks it is missing. Everything else falls straight
                 through to the table — an artist has no length to fall short
                 of. */
              <AlbumTracks
                album={opened.name}
                lead={opened.lead}
                onOpen={onOpen}
                onError={setPlayError}
              />
            ) : opened.kind === "artist" ? (
              /* An artist is their records, in the order they made them.
                 The sortable table is gone from here — see `ArtistTracks`. */
              <>
                <ArtistAlbums
                  name={opened.name}
                  onOpen={(album) => setOpened(album)}
                  onPlay={(entity) => void playAlbumEntity(entity)}
                />
                <ArtistTracks
                  name={opened.name}
                  onError={setPlayError}
                  onGenres={setArtistGenres}
                />
              </>
            ) : (
              <Songs
                onOpen={onOpen}
                query=""
                filter={{ genre: opened.name }}
                // Playing from inside an opened record conducts within it.
                scope={opened.name}
                scroller={scrollerRef}
              />
            )}
          </div>
        ) : query.trim() ? (
          /* Typing is a search, not a filter of the shelves.
           *
           * A shelf holds the first dozen of something ranked by plays, so
           * narrowing one would answer "you have no such album" for an album
           * that is right there in the library, thirteenth. Search is its own
           * result, which is what every other player does with the same field
           * and the same shelves. The table below does its own reading. */
          <div className="library__body">
            <Songs onOpen={onOpen} query={query} scroller={scrollerRef} />
          </div>
        ) : (
          <div className="library__body">
            <ErrorNotice error={playError} onDismiss={() => setPlayError(null)} />

            <Shelves onOpenEntity={setOpened} />

            <GenreRow
              genres={genres}
              onOpen={(genre) =>
                setOpened({
                  kind: "genre",
                  name: genre.name,
                  lead: genre.lead,
                  artist: "",
                })
              }
            />

            <section className="library__songs">
              <h2 className="shelf__head label">Songs</h2>
              {/* Most liked first: plays for a track, skips against it. An
                  alphabetical list of every track answers a question almost
                  nobody arrives with, and the headings still re-sort it. */}
              <Songs
                onOpen={onOpen}
                query=""
                defaultSort="score"
                scroller={scrollerRef}
              />
            </section>
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * The genres, as a row of pills that scrolls sideways.
 *
 * Ranked by how much this library's owner likes what is filed under each —
 * plays for, skips against, added up over the tracks. Alphabetical said
 * nothing at all about their music: it put Acid Jazz first for ever in a
 * library whose owner has played four house records a day since March.
 *
 * Silent when there are none, which is the ordinary state of a library that
 * has not been analysed yet. A heading over an empty row would claim the
 * genres are missing rather than unread.
 */
function GenreRow({
  genres,
  onOpen,
}: {
  genres: LibraryEntity[] | null;
  onOpen: (genre: LibraryEntity) => void;
}) {
  if (!genres || genres.length === 0) return null;

  return (
    <section className="genres">
      <h2 className="genres__head label">Genres</h2>
      <div className="genres__row">
        {genres.map((genre) => (
          <button
            key={genre.name}
            type="button"
            className="genres__pill"
            onClick={() => onOpen(genre)}
            aria-label={`Open the genre ${genre.name}`}
          >
            <span className="genres__name">{genre.name}</span>
            <span className="genres__count numeric">{genre.tracks}</span>
          </button>
        ))}
      </div>
    </section>
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
  onArt,
}: {
  album: string;
  artist: string;
  lead: string;
  /** The picture, whenever it changes — for the blurred backdrop behind the
   *  whole header. Not a second fetch: this is the same one drawn below. */
  onArt?: ((src: string | null) => void) | undefined;
}) {
  const [src, setSrc] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [chosen, setChosen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onArt?.(src);
    // Only `src` itself is the backdrop's business — `onArt` is a setter and
    // re-running this because the caller re-rendered would fire it needlessly.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [src]);

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

/**
 * An opened artist's portrait.
 *
 * Read-only, unlike `AlbumArtwork`: there is no per-artist equivalent of
 * `find_album_art` / `clear_album_art` to hand-correct a wrong one, so this is
 * only ever what `artist_portrait` already looks up — the same picture `Cover`
 * falls back to on an artist tile (TD-53), shown here at a size worth looking
 * at rather than a 78px circle.
 */
function ArtistArtwork({
  name,
  onArt,
}: {
  name: string;
  /** The picture, whenever it changes — for the blurred backdrop behind the
   *  whole header. Not a second fetch: this is the same one drawn below. */
  onArt?: ((src: string | null) => void) | undefined;
}) {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    onArt?.(src);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [src]);

  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    core
      .artistPortrait(name)
      .then((portrait) => {
        if (!cancelled) setSrc(portrait);
      })
      .catch(() => {
        if (!cancelled) setSrc(null);
      });
    return () => {
      cancelled = true;
    };
  }, [name]);

  return (
    <div className="artistart">
      <div className="artistart__art">
        {src ? (
          <img className="artistart__img" src={src} alt={`Portrait of ${name}`} />
        ) : (
          <div className="card__art-sheen" aria-hidden="true" />
        )}
      </div>
    </div>
  );
}

/**
 * An artist's albums, as a shelf above their tracks.
 *
 * The same shelf Home draws — a row that scrolls sideways rather than a grid
 * that wraps — narrowed to one artist instead of ranked across the library.
 * `library_entities` already does the narrowing (`resolved_rows` filters on
 * `view.artist` before anything is grouped), so this is one more read of the
 * same endpoint the Albums tab uses, not a new one.
 */
function ArtistAlbums({
  name,
  onOpen,
  onPlay,
}: {
  name: string;
  onOpen: (opened: Opened) => void;
  onPlay: (entity: LibraryEntity) => void;
}) {
  const [albums, setAlbums] = useState<LibraryEntity[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    setAlbums(null);
    core
      .libraryEntities({ groupBy: "album", artist: name, sortKey: "title", ascending: true })
      .then((list) => {
        if (!cancelled) setAlbums(list);
      })
      .catch(() => {
        if (!cancelled) setAlbums([]);
      });
    return () => {
      cancelled = true;
    };
  }, [name]);

  // No loading state and no empty state: the track list below is about to
  // paint regardless, and a shelf that flashes "reading" or "no albums"
  // above a table that is already answering the same question is noise.
  if (!albums || albums.length === 0) return null;

  return (
    <section className="shelf">
      <h2 className="shelf__head label">Albums</h2>
      <div className="shelf__row shelf__row--of-4">
        {albums.map((a) => (
          <EntityCard
            key={`${a.name} ${a.subtitle}`}
            entity={a}
            kind="album"
            className="shelf__tile"
            onOpen={() =>
              onOpen({ kind: "album", name: a.name, lead: a.lead, artist: name })
            }
            onPlay={() => onPlay(a)}
          />
        ))}
      </div>
    </section>
  );
}

/**
 * An artist's tracks, as their records rather than as a table.
 *
 * The sortable table used to sit here, and every column on it answered a
 * question the page had already answered. Artist repeats the heading on every
 * row. Album and genre are what the page is: an artist's albums are the
 * headings, and their genre belongs beside their name, not down a column of
 * their own. BPM and key are the DJ's numbers and have no bearing on reading
 * what somebody released.
 *
 * So: no columns, no sort controls. Tracks grouped by album, albums in the
 * order they came out — `year`, which is what the file's tags or the folder
 * name say. Albums whose year nobody knows sink to the end rather than
 * claiming 1970; that rule is `sort_rows`, not this screen's.
 *
 * One read for the whole artist, unwindowed. The flat table is windowed
 * because it is fifty thousand rows; an artist is dozens, and paging a list
 * that fits on two screens costs a round trip per scroll to save nothing.
 */
function ArtistTracks({
  name,
  onError,
  onGenres,
}: {
  name: string;
  onError: (message: string) => void;
  /** What this artist is filed under, for the line under their name. */
  onGenres: (genres: string[]) => void;
}) {
  const [sections, setSections] = useState<core.LibrarySection[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** Bumped by Retry, which is the only thing that re-reads. */
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setSections(null);
    setError(null);
    core
      .libraryView({
        artist: name,
        groupBy: "album",
        sortKey: "year",
        ascending: true,
      })
      .then((got) => {
        if (cancelled) return;
        setSections(got);
        onGenres(topGenres(got));
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(messageOf(e));
      });
    return () => {
      cancelled = true;
    };
    // `onGenres` is a setter and stable; listing it would re-read the artist
    // whenever the screen above re-renders for any other reason.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [name, revision]);

  if (error) {
    return (
      <ErrorNotice error={error} onRetry={() => setRevision((r) => r + 1)} />
    );
  }
  if (sections === null) return <p className="label">reading tracks</p>;

  /**
   * Play `href`, queueing the artist behind it in the order shown.
   *
   * Every track, not the album the press landed in: the list is one list, and
   * a queue that stopped at the end of a record would be a different answer
   * from the one on screen. `name` is the scope, so the DJ conducts within
   * the artist — the same thing an opened album does with its own.
   */
  async function play(href: string) {
    const hrefs = (sections ?? []).flatMap((s) => s.rows.map((r) => r.href));
    try {
      await core.playTracks(hrefs, href, name);
    } catch (e: unknown) {
      onError(messageOf(e));
    }
  }

  return (
    <div className="artisttracks">
      {sections.map((section) => (
        <section className="artisttracks__album" key={section.header}>
          <h3 className="artisttracks__head">
            {/* The header is the album title, or "—" for tracks whose album
                nobody knows — one heading for all of them, which is what
                `group_rows` sends. */}
            <span className="artisttracks__name">{section.header}</span>
            {yearOf(section) > 0 && (
              <span className="artisttracks__year numeric">
                {yearOf(section)}
              </span>
            )}
          </h3>
          <ol className="artisttracks__list">
            {section.rows.map((row) => (
              <li className="artisttracks__row" key={row.href}>
                {/* The same control an opened album's tracklist uses: a real
                    button, so it focuses and answers Enter and Space. */}
                <button
                  type="button"
                  className="tracklist__title"
                  onClick={() => void play(row.href)}
                >
                  {row.title}
                </button>
              </li>
            ))}
          </ol>
        </section>
      ))}
    </div>
  );
}

/**
 * What an artist is filed under, most of their music first.
 *
 * Counted over their tracks rather than taken from the first one: a rock
 * artist with two ambient B-sides is a rock artist, and the first track
 * alphabetically has no claim to speak for the rest. Three at most — a line
 * naming nine genres is a list, and the point of putting it under the name is
 * that it can be read at a glance.
 *
 * A track can be filed under several, and each of them counts. Ties keep the
 * order they were met in, which for equal counts is as good an answer as
 * there is and at least a stable one.
 */
function topGenres(sections: core.LibrarySection[], limit = 3): string[] {
  const counts = new Map<string, number>();
  for (const section of sections) {
    for (const row of section.rows) {
      for (const genre of row.genres) {
        counts.set(genre, (counts.get(genre) ?? 0) + 1);
      }
    }
  }
  return [...counts]
    .sort((a, b) => b[1] - a[1])
    .slice(0, limit)
    .map(([genre]) => genre);
}

/**
 * When a section's album came out, or 0 when nothing says.
 *
 * The earliest year its tracks carry rather than the first row's: a folder
 * can hold one mistagged file, and one wrong number should not re-date the
 * record. 0 means unknown and is not drawn — a heading reading "0" would be
 * a claim, and so would the year of a reissue nobody entered.
 */
function yearOf(section: core.LibrarySection): number {
  return section.rows.reduce(
    (year, row) => (row.year > 0 && (year === 0 || row.year < year) ? row.year : year),
    0,
  );
}

function EntityCard({
  entity,
  kind,
  onOpen,
  onPlay,
  className,
}: {
  entity: LibraryEntity;
  kind: "album" | "artist" | "genre";
  onOpen: () => void;
  onPlay: () => void;
  /** Extra classes onto the root, for the shelf this card also draws on
   *  (`shelf__tile`) — the grid needs nothing extra, so this is optional. */
  className?: string | undefined;
}) {
  const noun = kind;
  const pickUp = useEntityDrag(kind, entity.name);
  return (
    <div
      className={
        "card card--entity" +
        (kind === "artist" ? " card--round" : "") +
        (className ? ` ${className}` : "")
      }
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

/**
 * The top of the library: two shelves, most played first.
 *
 * The album grid used to be what Library opened on, which asked the wrong
 * question. Almost nobody arrives at their own music looking for a particular
 * record — they arrive wanting something *on*. Tidal and Spotify both answer
 * this the same way and for the same reason: shelves of what you actually
 * listen to, in the order you actually listen to it.
 *
 * Two shelves now, artists then albums. Playlists and smart groups were the
 * first two and are not here any more: both are already in the sidebar, on
 * every screen, where they are also the drop target for a dragged track — so
 * the shelves were a second, worse copy of a list that never left the window.
 * What replaced them, below this component, is the library itself: the genres
 * and then every track, ranked by what has been played and what has been
 * skipped. The front door now shows the music rather than the folders.
 *
 * The ranking is `home_shelves_for` in the backend, and is tested there. This
 * screen draws what it is handed.
 */

import { useEffect, useState } from "react";
import * as core from "../lib/core";
import { Cover } from "../components/Cover";
import { useEntityDrag } from "../components/entityDrag";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import type { HomeShelves, Shelf } from "../lib/core";
import type { Opened } from "./Library";

/**
 * The last shelves read, kept across unmounts.
 *
 * This is unmounted whenever a drill-down covers it — a playlist, an album,
 * liner notes — so coming back used to mean a spinner for a page that had not
 * changed. What is on screen is painted from here first and corrected when the
 * answer arrives. One entry, not a map: there is only one library.
 */
let remembered: HomeShelves | null = null;

/** Throw it away. Called by whatever changes what a read would return. */
export function forgetShelves() {
  remembered = null;
}

/**
 * How many tiles a shelf shows without scrolling.
 *
 * Four: the label is a name rather than a sentence, and a wider shelf shows
 * more of the library at a glance. It drops by one on a phone, which is
 * `shelves.css`'s half of this.
 */
const PER_ROW = 4;

export function Shelves({ onOpenEntity }: { onOpenEntity: (opened: Opened) => void }) {
  const [shelves, setShelves] = useState<HomeShelves | null>(remembered);
  const [error, setError] = useState<string | null>(null);
  /** A failure to start playback belongs on screen, not in the console. */
  const [playError, setPlayError] = useState<string | null>(null);

  /**
   * A scan, or a listen that changes the order.
   *
   * The shelves are ranked on play counts, so they go stale as a side effect
   * of the app being used — unlike the album grid, which only changes when the
   * library does. Re-read on the same event the grid re-reads on, and again
   * whenever this screen is returned to, which is what the empty dependency
   * list below amounts to.
   */
  const [nonce, setNonce] = useState(0);
  useEffect(() => {
    const handler = () => {
      forgetShelves();
      setNonce((n) => n + 1);
    };
    window.addEventListener("vapor:library-changed", handler);
    return () => window.removeEventListener("vapor:library-changed", handler);
  }, []);

  useEffect(() => {
    let cancelled = false;
    core
      .homeShelves()
      .then((next) => {
        remembered = next;
        if (!cancelled) {
          setShelves(next);
          setError(null);
        }
      })
      .catch((e: unknown) => {
        // A failed refresh must not blank shelves that are already on screen:
        // stale is better than empty, and the next attempt corrects it.
        if (!cancelled && !remembered) setError(messageOf(e));
      });
    return () => {
      cancelled = true;
    };
  }, [nonce]);

  /** Play an artist or an album, from its first track, conducted within it. */
  async function playEntity(kind: "artist" | "album", tile: Shelf) {
    try {
      const sections = await core.libraryView({
        groupBy: "none",
        sortKey: "title",
        ascending: true,
        ...(kind === "album" ? { album: tile.id } : { artist: tile.id }),
      });
      const hrefs = sections[0]?.rows.map((r) => r.href) ?? [];
      await core.playTracks(hrefs, tile.lead, tile.title);
    } catch (e: unknown) {
      setPlayError(messageOf(e));
    }
  }

  if (error && !shelves) {
    return (
      <div className="library__body">
        <div className="library__empty">
          <p className="library__empty-title">Could not read the library</p>
          <ErrorNotice error={error} />
        </div>
      </div>
    );
  }

  if (!shelves) {
    return (
      <div className="library__body">
        <p className="label">reading library</p>
      </div>
    );
  }

  /*
   * A library with nothing in it at all.
   *
   * Two empty shelves under two headings is a page that looks broken. One
   * sentence saying why is not.
   */
  const anything = shelves.artists.length > 0 || shelves.albums.length > 0;
  if (!anything) {
    return (
      <div className="library__body">
        <div className="library__empty">
          <p className="library__empty-title">No music yet</p>
          <p className="library__empty-body">
            Connect your storage in Settings and Vapor will index it here.
            Nothing leaves your device.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="library__body">
      <ErrorNotice error={playError} onDismiss={() => setPlayError(null)} />

      <ShelfRow
        title="Artists"
        tiles={shelves.artists}
        per={PER_ROW}
        round
        onOpen={(tile) =>
          onOpenEntity({
            kind: "artist",
            name: tile.title,
            lead: tile.lead,
            artist: tile.title,
          })
        }
        onPlay={(tile) => void playEntity("artist", tile)}
        opens="artist"
        pickUp="artist"
      />

      <ShelfRow
        title="Albums"
        tiles={shelves.albums}
        per={PER_ROW}
        onOpen={(tile) =>
          onOpenEntity({
            kind: "album",
            name: tile.title,
            lead: tile.lead,
            // The album's artist, for the artwork search inside it.
            artist: tile.subtitle,
          })
        }
        onPlay={(tile) => void playEntity("album", tile)}
        opens="album"
        pickUp="album"
      />
    </div>
  );
}

/**
 * One shelf: a heading and a row that scrolls sideways.
 *
 * Sideways rather than wrapping, because a shelf is a claim about the first
 * few — the twelfth most played album is not what the row is for, it is just
 * where the row runs out. Wrapping would give the twelfth the same weight as
 * the first and turn four shelves into four grids, which is the screen this
 * replaced.
 */
function ShelfRow({
  title,
  tiles,
  per,
  round,
  opens,
  pickUp,
  onOpen,
  onPlay,
}: {
  title: string;
  tiles: Shelf[];
  /** Tiles visible before it scrolls. See `PER_ROW`. */
  per: 4;
  /** Artists are round, as they are everywhere else in the app. */
  round?: boolean;
  /** The noun for the accessible name: "Open the album Geogaddi". */
  opens: string;
  /** The kind these tiles can be picked up as — onto a group in the rail, or
   *  a playlist, which resolves it to the tracks on it. */
  pickUp: "artist" | "album";
  onOpen: (tile: Shelf) => void;
  onPlay: (tile: Shelf) => void;
}) {
  // An empty shelf means an empty library, which the caller says once, above.
  // A heading with nothing under it would say it a third time.
  if (tiles.length === 0) return null;

  return (
    <section className="shelf">
      <h2 className="shelf__head label">{title}</h2>
      <div className={`shelf__row shelf__row--of-${per}`}>
        {tiles.map((tile) => (
          <ShelfTile
            /* Id *and* subtitle. Two albums can share a title, and an album
               tile's id is its title — so keying on it alone collides them
               and React warns about a duplicate key, which is the same bug
               the album grid had and for the same reason. */
            key={`${tile.id}\u0000${tile.subtitle}`}
            tile={tile}
            round={round}
            opens={opens}
            pickUp={pickUp}
            onOpen={() => onOpen(tile)}
            onPlay={() => onPlay(tile)}
          />
        ))}
      </div>
    </section>
  );
}

/**
 * One tile on a shelf.
 *
 * Its own component only so that it can call a hook: `useEntityDrag` cannot be
 * called inside the `map` above, and the alternative — wiring the drag by hand
 * on every shelf — is how the mouse path and the touch path drift apart.
 */
function ShelfTile({
  tile,
  round,
  opens,
  pickUp,
  onOpen,
  onPlay,
}: {
  tile: Shelf;
  /* `| undefined` because `exactOptionalPropertyTypes` is on: the shelves pass
     it through whether or not they have one. */
  round?: boolean | undefined;
  opens: string;
  pickUp: "artist" | "album";
  onOpen: () => void;
  onPlay: () => void;
}) {
  const grab = useEntityDrag(pickUp, tile.title);

  return (
    <div
      className={"card card--entity shelf__tile" + (round ? " card--round" : "")}
      {...grab}
    >
      <button
        type="button"
        className="card__open"
        onClick={onOpen}
        aria-label={`Open the ${opens} ${tile.title}`}
      >
        <Cover
          href={tile.lead}
          label={tile.title}
          {...(round ? { artist: tile.title } : {})}
        />
      </button>
      <button
        type="button"
        className="card__play"
        onClick={onPlay}
        aria-label={`Play ${tile.title}`}
      >
        <span className="icon icon--play" aria-hidden="true" />
      </button>
      <div className="card__meta">
        <span className="card__title" title={tile.title}>
          {tile.title}
        </span>
        <span className="card__sub" title={tile.subtitle}>
          {tile.subtitle}
        </span>
      </div>
    </div>
  );
}

/**
 * The library's front door: four shelves, most played first.
 *
 * The album grid used to be what Library opened on, which asked the wrong
 * question. Almost nobody arrives at their own music looking for a particular
 * record — they arrive wanting something *on*, and the thing they reach for is
 * a playlist they already have. Tidal and Spotify both answer this the same
 * way and for the same reason: shelves of what you actually listen to, in the
 * order you actually listen to it, with the browse-by-album view kept for the
 * rare visit where you do know exactly what you want.
 *
 * Four shelves, in the order someone reaches for them: playlists, smart
 * groups, artists, albums. Smart groups are second because they are the thing
 * this app has and the others do not — a saved set of artists and albums that
 * fills itself — and burying a feature nobody else offers under two rows of
 * things everybody offers is how it stays undiscovered.
 *
 * The ranking is `home_shelves_for` in the backend, on four keys, and is
 * tested there. This screen draws what it is handed.
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
 * Home is unmounted whenever a drill-down covers it — a playlist, an album,
 * liner notes — so coming back used to mean a spinner for a page that had not
 * changed. What is on screen is painted from here first and corrected when the
 * answer arrives. One entry, not a map: there is only one home.
 */
let remembered: HomeShelves | null = null;

/** Throw it away. Called by whatever changes what a read would return. */
export function forgetHomeShelves() {
  remembered = null;
}

/**
 * How many tiles a shelf shows without scrolling.
 *
 * Playlists and groups get three because their titles are sentences someone
 * wrote — "Late night, driving" needs the width. Artists and albums get four:
 * the label is a name, and a wider shelf shows more of the library at a glance.
 * Both drop by one on a phone, which is `home.css`'s half of this.
 */
const PER_ROW = { collection: 3, entity: 4 } as const;

export function Home({
  onOpenPlaylist,
  onOpenGroup,
  onOpenEntity,
  onTracks,
}: {
  onOpenPlaylist: (id: string) => void;
  onOpenGroup: (id: string) => void;
  onOpenEntity: (opened: Opened) => void;
  /**
   * How many tracks there are, for the line above this screen.
   *
   * Reported upwards because the header belongs to Library and the number
   * arrives here: home never reads rows, so it has nothing else to count, and
   * making Library ask for the shelves as well to write one number on itself
   * would be a second round trip for a page that is already painted.
   */
  onTracks: (tracks: number) => void;
}) {
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
      forgetHomeShelves();
      setNonce((n) => n + 1);
    };
    window.addEventListener("vapor:library-changed", handler);
    return () => window.removeEventListener("vapor:library-changed", handler);
  }, []);

  useEffect(() => {
    let cancelled = false;
    // What was on screen last time, said again before the request goes out —
    // otherwise the header reads "0 tracks" over shelves that are already
    // drawn, for as long as the round trip takes.
    if (remembered) onTracks(remembered.tracks);
    core
      .homeShelves()
      .then((next) => {
        remembered = next;
        if (!cancelled) {
          setShelves(next);
          setError(null);
          onTracks(next.tracks);
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
  }, [nonce, onTracks]);

  /**
   * Play a whole collection, and credit it.
   *
   * The tracks are fetched here rather than carried on the tile: a shelf of
   * twelve playlists would otherwise ship every href in all twelve to draw
   * twelve covers.
   */
  async function playCollection(kind: core.Collection, tile: Shelf) {
    try {
      const hrefs =
        kind === "playlist"
          ? (await core.playlistRows(tile.id)).map((r) => r.href)
          : (await core.groupTracks(tile.id)).map((r) => r.href);
      if (hrefs.length === 0) return;
      // Scoped to the collection, so the DJ conducts inside it, and credited
      // to it, so playing from here is what puts it at the front next time.
      await core.playTracks(hrefs, hrefs[0], tile.title, {
        kind,
        id: tile.id,
      });
    } catch (e: unknown) {
      setPlayError(messageOf(e));
    }
  }

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
   * Four empty shelves under four headings is a page that looks broken. One
   * sentence saying why is not.
   */
  const anything =
    shelves.playlists.length > 0 ||
    shelves.groups.length > 0 ||
    shelves.artists.length > 0 ||
    shelves.albums.length > 0;
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
        title="Playlists"
        empty="Playlists you make will show up here, most played first."
        tiles={shelves.playlists}
        per={PER_ROW.collection}
        onOpen={(tile) => onOpenPlaylist(tile.id)}
        onPlay={(tile) => void playCollection("playlist", tile)}
        opens="playlist"
      />

      <ShelfRow
        title="Smart groups"
        empty="A smart group is a set of artists and albums that fills itself. Make one from the Groups tab."
        tiles={shelves.groups}
        per={PER_ROW.collection}
        onOpen={(tile) => onOpenGroup(tile.id)}
        onPlay={(tile) => void playCollection("group", tile)}
        opens="smart group"
      />

      <ShelfRow
        title="Artists"
        tiles={shelves.artists}
        per={PER_ROW.entity}
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
        per={PER_ROW.entity}
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
  empty,
  opens,
  pickUp,
  onOpen,
  onPlay,
}: {
  title: string;
  tiles: Shelf[];
  /** Tiles visible before it scrolls. See `PER_ROW`. */
  per: 3 | 4;
  /** Artists are round, as they are everywhere else in the app. */
  round?: boolean;
  /**
   * What to say when there are none, for the two shelves that can be empty in
   * a library that is otherwise full. An artist shelf with nothing on it means
   * the library is empty, which is said once, above.
   */
  empty?: string;
  /** The noun for the accessible name: "Open the playlist Late night". */
  opens: string;
  /**
   * The kind these tiles can be picked up as, for the two shelves that hold
   * entities. A playlist and a group are collections rather than things a
   * group can hold, so their shelves leave this unset and their tiles do not
   * drag.
   */
  pickUp?: "artist" | "album";
  onOpen: (tile: Shelf) => void;
  onPlay: (tile: Shelf) => void;
}) {
  if (tiles.length === 0 && !empty) return null;

  return (
    <section className="shelf">
      <h2 className="shelf__head label">{title}</h2>
      {tiles.length === 0 ? (
        <p className="shelf__empty">{empty}</p>
      ) : (
        <div className={`shelf__row shelf__row--of-${per}`}>
          {tiles.map((tile) => (
            <ShelfTile
              /* Id *and* subtitle. Two albums can share a title, and an
                 album tile's id is its title — so keying on it alone
                 collides them and React warns about a duplicate key, which
                 is the same bug the album grid had and for the same
                 reason. */
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
      )}
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
  /* `| undefined` on the optional two because `exactOptionalPropertyTypes` is
     on: the shelves pass them through whether or not they have one. */
  round?: boolean | undefined;
  opens: string;
  pickUp?: "artist" | "album" | undefined;
  onOpen: () => void;
  onPlay: () => void;
}) {
  // Always called, as a hook must be; the props it returns are only spread on
  // for the shelves that hold something a group could take.
  const grab = useEntityDrag(pickUp ?? "artist", tile.title);

  return (
    <div
      className={"card card--entity shelf__tile" + (round ? " card--round" : "")}
      {...(pickUp ? grab : {})}
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

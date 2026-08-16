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
import type { GroupBy, LibrarySection, Row } from "../lib/core";

/**
 * Tabs map onto the index's grouping, so the UI cannot invent a category.
 *
 * "Songs" was called "All" and showed the same card grid ungrouped, while a
 * separate Songs screen in the sidebar held the actual table. The Daylight
 * design has no such screen — its Library tabs are Albums, Songs, Artists,
 * Playlists, and the flat list is one of them (docs/DESIGN_DRIFT.md). It is one
 * of them here now too, and it renders the table.
 */
const TABS: ReadonlyArray<{ id: GroupBy; label: string }> = [
  { id: "album", label: "Albums" },
  { id: "artist", label: "Artists" },
  { id: "genre", label: "Genres" },
  { id: "none", label: "Songs" },
];

type Load =
  | { kind: "loading" }
  | { kind: "ready"; sections: LibrarySection[] }
  | { kind: "error"; message: string };

export function Library({
  onOpen,
}: {
  /** Opens a track's liner notes — the table's double-click. */
  onOpen?: ((href: string) => void) | undefined;
}) {
  const [query, setQuery] = useState("");
  const [groupBy, setGroupBy] = useState<GroupBy>("album");
  const [load, setLoad] = useState<Load>({ kind: "loading" });
  /** A failure to start playback belongs on screen, not in the console. */
  const [playError, setPlayError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    // Debounced because the table re-runs the whole filter/sort/group pipeline
    // per keystroke. 120ms is below the threshold where typing feels laggy but
    // high enough to collapse a burst of keystrokes into one round trip.
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
          onChange={(e) => setQuery(e.target.value)}
          // The library is local, so there is nothing to autocomplete against
          // and nothing to send anywhere.
          autoComplete="off"
          spellCheck={false}
        />
      </div>

      <div className="library__tabs" role="tablist">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            role="tab"
            aria-selected={groupBy === tab.id}
            className={
              "library__tab" + (groupBy === tab.id ? " library__tab--on" : "")
            }
            onClick={() => setGroupBy(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* The flat list is the table, not an ungrouped grid of cards: the whole
          point of it is the columns — artist, album, tempo, key — and sorting
          by them. It gets the search field above rather than one of its own. */}
      {groupBy === "none" ? (
        <Songs onOpen={onOpen} query={query} />
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
function EmptyLibrary({ query }: { query: string }) {
  if (query.trim()) {
    return (
      <div className="library__empty">
        <p className="library__empty-title">Nothing matched “{query}”</p>
        <p className="library__empty-body">Try a different word.</p>
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

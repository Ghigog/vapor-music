/**
 * Songs — the flat track table.
 *
 * This is the screen that justified choosing React. The Godot version
 * hand-wrote list virtualization in `track_table.gd` — slot plans, offset
 * arrays, visible-range maths and a node pool, roughly 400 lines that had to
 * be debugged and re-tuned whenever the row shape changed. Here it is a hook.
 *
 * Two properties the hand-rolled version had to work for and this gets from
 * the design of the library:
 *
 * * Only visible rows exist in the DOM, so a 10,000-track library costs the
 *   same as a 20-track one.
 * * Row height is declared once. The Godot version derived it in three places
 *   (slot plan, offsets, hit-testing) and they could disagree.
 *
 * Sorting, filtering and grouping are not implemented here — they come from
 * `vapor-library`'s index over IPC, so this screen and a smart playlist cannot
 * form different opinions about the same query.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import * as core from "../lib/core";
import type { Row, SortKey } from "../lib/core";

/** Row height in px, from the design. Declared once; the virtualizer and the
 *  stylesheet both read it from here rather than each holding a copy. */
const ROW_HEIGHT = 66;
const ROW_GAP = 4;

interface Column {
  id: SortKey;
  label: string;
  /** Right-aligned mono columns carry data; the rest carry names. */
  numeric?: boolean;
}

const COLUMNS: readonly Column[] = [
  { id: "title", label: "Title" },
  { id: "artist", label: "Artist" },
  { id: "album", label: "Album" },
  { id: "bpm", label: "BPM", numeric: true },
  { id: "key", label: "Key", numeric: true },
];

type Load =
  | { kind: "loading" }
  | { kind: "ready"; rows: Row[] }
  | { kind: "error"; message: string };

export function Songs() {
  const [query, setQuery] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("title");
  const [ascending, setAscending] = useState(true);
  const [load, setLoad] = useState<Load>({ kind: "loading" });
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());

  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    const t = setTimeout(() => {
      core
        .libraryView({ query, sortKey, ascending, groupBy: "none" })
        .then((sections) => {
          if (cancelled) return;
          // groupBy "none" returns exactly one section.
          setLoad({ kind: "ready", rows: sections[0]?.rows ?? [] });
        })
        .catch((e: unknown) => {
          if (!cancelled) setLoad({ kind: "error", message: String(e) });
        });
    }, 120);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query, sortKey, ascending]);

  const rows = useMemo(() => (load.kind === "ready" ? load.rows : []), [load]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT + ROW_GAP,
    // A few rows above and below the viewport, so a fast scroll does not
    // reveal blank space before React commits.
    overscan: 6,
  });

  function toggleSort(id: SortKey) {
    if (id === sortKey) setAscending((a) => !a);
    else {
      setSortKey(id);
      setAscending(true);
    }
  }

  function toggleSelected(href: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (!next.delete(href)) next.add(href);
      return next;
    });
  }

  async function playFrom(href: string) {
    await core.playTracks(
      rows.map((r) => r.href),
      href,
    );
  }

  async function addSelectedTo(playlistId: string) {
    // Order matters: the selection is a Set, but the playlist should receive
    // tracks in the order they appear on screen, not in insertion order.
    const hrefs = rows.filter((r) => selected.has(r.href)).map((r) => r.href);
    await core.addTracksToPlaylist(playlistId, hrefs);
    setSelected(new Set());
  }

  return (
    <div className="songs">
      <header className="songs__head">
        <h1 className="songs__title">Songs</h1>
        <div className="songs__search glass">
          <input
            className="songs__search-input"
            type="search"
            value={query}
            placeholder="Filter"
            onChange={(e) => setQuery(e.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
        </div>
      </header>

      <div className="songs__cols" role="row">
        {COLUMNS.map((col) => (
          <button
            key={col.id}
            role="columnheader"
            aria-sort={
              sortKey === col.id
                ? ascending
                  ? "ascending"
                  : "descending"
                : "none"
            }
            className={
              "songs__col songs__col--" +
              col.id +
              (col.numeric ? " songs__col--num" : "") +
              (sortKey === col.id ? " songs__col--on" : "")
            }
            onClick={() => toggleSort(col.id)}
          >
            {col.label}
            {sortKey === col.id && (
              <span aria-hidden="true">{ascending ? " ↑" : " ↓"}</span>
            )}
          </button>
        ))}
      </div>

      {selected.size > 0 && (
        <SelectionBar
          count={selected.size}
          onClear={() => setSelected(new Set())}
          onAddTo={addSelectedTo}
        />
      )}

      <div className="songs__scroll" ref={scrollRef}>
        {load.kind === "loading" && <p className="label">reading library</p>}

        {load.kind === "error" && (
          <p className="songs__error numeric">{load.message}</p>
        )}

        {load.kind === "ready" && rows.length === 0 && (
          <p className="songs__empty">
            {query.trim() ? `Nothing matched “${query}”` : "No tracks yet"}
          </p>
        )}

        {/* The spacer carries the full scroll height; only the rows inside the
            visible window are actually rendered. */}
        <div
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
          {virtualizer.getVirtualItems().map((item) => {
            const row = rows[item.index];
            if (!row) return null;
            return (
              <div
                key={row.href}
                className={
                  "songrow" + (selected.has(row.href) ? " songrow--on" : "")
                }
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  right: 0,
                  height: ROW_HEIGHT,
                  transform: `translateY(${item.start}px)`,
                }}
                onDoubleClick={() => void playFrom(row.href)}
                onClick={(e) => {
                  // Plain click selects; double-click plays. Modifier-click is
                  // the multi-select gesture people expect from a table.
                  if (e.metaKey || e.ctrlKey) toggleSelected(row.href);
                  else setSelected(new Set([row.href]));
                }}
              >
                <SongRow row={row} />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function SongRow({ row }: { row: Row }) {
  const artist = row.artistSource === "unknown" ? "—" : row.artist;
  const album = row.albumSource === "unknown" ? "—" : row.album;

  return (
    <>
      <div className="songrow__art" aria-hidden="true">
        <div className="songrow__art-sheen" />
      </div>
      <div className="songrow__names">
        <span className="songrow__title" title={row.title}>
          {row.title}
        </span>
        <span className="songrow__artist" title={artist}>
          {artist}
        </span>
      </div>
      <span className="songrow__album" title={album}>
        {album}
      </span>
      {/*
        Unknown analysis renders as a dash, never as 0 or 120. The Godot stub
        used to fabricate 120 BPM / 8A, which is indistinguishable from a real
        result once it reaches the table — the whole point of showing "—" is
        that a person can tell the difference.
      */}
      <span className="songrow__bpm numeric">
        {row.bpm > 0 ? Math.round(row.bpm) : "—"}
      </span>
      <span className="songrow__key numeric">{row.key || "—"}</span>
    </>
  );
}

function SelectionBar({
  count,
  onClear,
  onAddTo,
}: {
  count: number;
  onClear: () => void;
  onAddTo: (playlistId: string) => void;
}) {
  const [playlists, setPlaylists] = useState<core.Playlist[]>([]);

  useEffect(() => {
    core.playlists().then(setPlaylists).catch(() => setPlaylists([]));
  }, []);

  return (
    <div className="selbar glass">
      <span className="numeric">{count} selected</span>
      <div className="selbar__actions">
        <select
          className="selbar__select"
          value=""
          onChange={(e) => {
            if (e.target.value) onAddTo(e.target.value);
          }}
        >
          <option value="">Add to…</option>
          {playlists.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <button className="selbar__clear" onClick={onClear}>
          Clear
        </button>
      </div>
    </div>
  );
}

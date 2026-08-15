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
import { ErrorNotice, messageOf } from "../components/ErrorNotice";

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

export function Songs({
  onOpen,
}: {
  onOpen?: ((href: string) => void) | undefined;
}) {
  const [query, setQuery] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("title");
  const [ascending, setAscending] = useState(true);
  const [load, setLoad] = useState<Load>({ kind: "loading" });
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());

  /** Which hrefs carry a manual tempo, so a correction can be marked as one.
   *  The table shows a number; this is what says where it came from. */
  const [overrides, setOverrides] = useState<Record<string, number>>({});
  /** The href whose BPM cell is being edited, if any. */
  const [editing, setEditing] = useState<string | null>(null);
  const [bpmError, setBpmError] = useState<string | null>(null);
  /** Bumped after a correction lands, to re-read rows with it applied. */
  const [revision, setRevision] = useState(0);

  /** The row the keyboard is on. Separate from `selected`, which is what a
   *  bulk action applies to — moving focus should not silently change what a
   *  person is about to add to a playlist. */
  const [focused, setFocused] = useState(0);

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
          if (!cancelled) setLoad({ kind: "error", message: messageOf(e) });
        });
    }, 120);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query, sortKey, ascending, revision]);

  useEffect(() => {
    core
      .settings()
      .then((s) => setOverrides(s.bpmOverrides ?? {}))
      // Not worth surfacing: without this the corrections still apply, they
      // just are not marked as corrections.
      .catch(() => setOverrides({}));
  }, [revision]);

  /**
   * Commit a hand-typed tempo. An empty box clears the correction.
   *
   * The rows are re-read rather than patched in place: the backend decides
   * whether an override wins over a detected value, and a table that applied
   * that rule itself would be a second opinion waiting to disagree.
   */
  async function commitBpm(href: string, raw: string) {
    setEditing(null);
    const trimmed = raw.trim();
    const bpm = trimmed === "" ? 0 : Number(trimmed);
    if (!Number.isFinite(bpm)) {
      setBpmError("That is not a number.");
      return;
    }
    try {
      await core.setBpmOverride(href, bpm);
      setBpmError(null);
      setRevision((r) => r + 1);
    } catch (e: unknown) {
      setBpmError(messageOf(e));
    }
  }

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

      {bpmError && (
        // Dismissed by fixing it or by clicking away, not by a timer: a
        // message about the number you just typed should not vanish while you
        // are still reading it.
        <ErrorNotice error={bpmError} onDismiss={() => setBpmError(null)} />
      )}

      {selected.size > 0 && (
        <SelectionBar
          count={selected.size}
          onClear={() => setSelected(new Set())}
          onAddTo={addSelectedTo}
        />
      )}

      <div
        className="songs__scroll"
        ref={scrollRef}
        tabIndex={0}
        role="listbox"
        aria-label="Tracks"
        aria-activedescendant={rows[focused] ? `row-${focused}` : undefined}
        onKeyDown={(e) => {
          if (rows.length === 0) return;
          // Arrow keys move a cursor; Enter plays it; space selects. The Godot
          // version had none of this, and a table this size is unusable without
          // it (TD-33).
          const move = (to: number) => {
            e.preventDefault();
            const next = Math.max(0, Math.min(rows.length - 1, to));
            setFocused(next);
            virtualizer.scrollToIndex(next, { align: "auto" });
          };
          switch (e.key) {
            case "ArrowDown":
              return move(focused + 1);
            case "ArrowUp":
              return move(focused - 1);
            case "PageDown":
              return move(focused + 10);
            case "PageUp":
              return move(focused - 10);
            case "Home":
              return move(0);
            case "End":
              return move(rows.length - 1);
            case "Enter": {
              e.preventDefault();
              const row = rows[focused];
              if (row) void playFrom(row.href);
              return;
            }
            case " ": {
              e.preventDefault();
              const row = rows[focused];
              if (row) toggleSelected(row.href);
              return;
            }
            default:
              return;
          }
        }}
      >
        {load.kind === "loading" && <p className="label">reading library</p>}

        {load.kind === "error" && (
          <ErrorNotice
            error={load.message}
            onRetry={() => setRevision((r) => r + 1)}
          />
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
                id={`row-${item.index}`}
                role="option"
                aria-selected={selected.has(row.href)}
                className={
                  "songrow" +
                  (selected.has(row.href) ? " songrow--on" : "") +
                  (item.index === focused ? " songrow--cursor" : "")
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
                  setFocused(item.index);
                  if (e.metaKey || e.ctrlKey) toggleSelected(row.href);
                  else setSelected(new Set([row.href]));
                }}
              >
                <SongRow
                  row={row}
                  onOpen={onOpen}
                  overridden={row.href in overrides}
                  editing={editing === row.href}
                  onEdit={() => setEditing(row.href)}
                  onCommit={(raw) => void commitBpm(row.href, raw)}
                  onCancel={() => setEditing(null)}
                />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function SongRow({
  row,
  onOpen,
  overridden,
  editing,
  onEdit,
  onCommit,
  onCancel,
}: {
  row: Row;
  onOpen?: ((href: string) => void) | undefined;
  overridden: boolean;
  editing: boolean;
  onEdit: () => void;
  onCommit: (raw: string) => void;
  onCancel: () => void;
}) {
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
      {onOpen && (
        <button
          className="songrow__info"
          aria-label={`Liner notes for ${row.title}`}
          title="Liner notes"
          onClick={(e) => {
            // The row's own click plays and selects; this is a different verb.
            e.stopPropagation();
            onOpen(row.href);
          }}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          i
        </button>
      )}
      <BpmCell
        bpm={row.bpm}
        overridden={overridden}
        editing={editing}
        onEdit={onEdit}
        onCommit={onCommit}
        onCancel={onCancel}
      />
      <span className="songrow__key numeric">{row.key || "—"}</span>
    </>
  );
}

/**
 * The BPM cell, which is also where a tempo gets corrected (TD-10).
 *
 * Detection lands a metrical relative — half, double or three-quarter time — on
 * roughly 10% of a real library. That was accepted rather than solved, on the
 * condition that a person could fix it; `bpm_overrides` has been honoured by
 * the table and the pathfinder for a while, but nothing set it, so the escape
 * hatch was unreachable. This is the hatch.
 *
 * Double-click to edit, matching the table's existing "double-click acts"
 * idiom, and `stopPropagation` so it does not also start playing the row.
 * An unanalysed track shows "—" and is still editable — a person can correct a
 * tempo that was never detected at all, which the backend explicitly supports.
 */
function BpmCell({
  bpm,
  overridden,
  editing,
  onEdit,
  onCommit,
  onCancel,
}: {
  bpm: number;
  overridden: boolean;
  editing: boolean;
  onEdit: () => void;
  onCommit: (raw: string) => void;
  onCancel: () => void;
}) {
  if (editing) {
    return <BpmEditor bpm={bpm} onCommit={onCommit} onCancel={onCancel} />;
  }

  return (
    <span
      className={
        "songrow__bpm numeric" + (overridden ? " songrow__bpm--manual" : "")
      }
      title={
        overridden
          ? "Corrected by hand. Double-click to change, or clear the box to go back to the detected tempo."
          : "Double-click to correct the tempo"
      }
      onDoubleClick={(e) => {
        // Without this the row's own double-click starts playing the track.
        e.stopPropagation();
        onEdit();
      }}
    >
      {/*
        Unknown analysis renders as a dash, never as 0 or 120. The Godot stub
        used to fabricate 120 BPM / 8A, which is indistinguishable from a real
        result once it reaches the table — the whole point of showing "—" is
        that a person can tell the difference.
      */}
      {bpm > 0 ? Math.round(bpm) : "—"}
    </span>
  );
}

/**
 * The open editor.
 *
 * Separate from `BpmCell` because it needs a ref, and a hook cannot live behind
 * the conditional that decides whether the editor is open at all.
 *
 * Finishing is one-shot. Enter and Escape both unmount the input, and removing
 * a focused element can fire `blur` on the way out — so without the guard,
 * Escape would cancel and then immediately commit the value it was meant to
 * discard, which is the opposite of what was asked for. The same guard stops
 * Enter committing twice.
 */
function BpmEditor({
  bpm,
  onCommit,
  onCancel,
}: {
  bpm: number;
  onCommit: (raw: string) => void;
  onCancel: () => void;
}) {
  const done = useRef(false);
  const finish = (action: () => void) => {
    if (done.current) return;
    done.current = true;
    action();
  };

  return (
    <input
      className="songrow__bpm-input numeric"
      type="text"
      inputMode="decimal"
      defaultValue={bpm > 0 ? String(Math.round(bpm)) : ""}
      placeholder="BPM"
      aria-label="Corrected BPM"
      autoFocus
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onFocus={(e) => e.currentTarget.select()}
      // Blur commits rather than discards: clicking away from a box you have
      // typed into should keep what you typed, not throw it away silently.
      onBlur={(e) => {
        const value = e.currentTarget.value;
        finish(() => onCommit(value));
      }}
      onKeyDown={(e) => {
        const value = e.currentTarget.value;
        if (e.key === "Enter") finish(() => onCommit(value));
        else if (e.key === "Escape") finish(onCancel);
        // Arrow keys belong to the box while it has focus, not to the table.
        e.stopPropagation();
      }}
    />
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

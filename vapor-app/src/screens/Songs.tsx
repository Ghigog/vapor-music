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
import { listen } from "@tauri-apps/api/event";
import * as core from "../lib/core";
import { startTrackDrag } from "../components/PlaylistRail";
import { TrackSheet } from "../components/TrackSheet";
import { useLongPress } from "../lib/longPress";
import type { Row, SortKey } from "../lib/core";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";

/** Row height in px, from the design. Declared once; the virtualizer and the
 *  stylesheet both read it from here rather than each holding a copy. */
const ROW_HEIGHT = 66;
const ROW_GAP = 4;

interface Column {
  id: SortKey;
  label: string;
  /** Which grid cell this control sits in. Title and artist share cell 2,
   *  because the row stacks them in one cell too — see COLUMNS. */
  cell: number;
  /** Right-aligned mono columns carry data; the rest carry names. */
  numeric?: boolean;
}

/**
 * The header controls, and the cell each occupies.
 *
 * The row renders five grid children: art, names, album, bpm, key — with title
 * and artist *stacked inside* the names cell, which is what the design shows.
 * The header therefore cannot have five columns of its own, and previously
 * resolved that by hiding the artist control entirely: you could not sort by
 * artist from the header at all, and the two grids agreed only by convention.
 *
 * Now both sort controls live in cell 2, side by side over the stack they sort.
 * One column definition, five cells, every control reachable.
 */
/*
 * Cell 1 is the selection checkbox and cell 2 the artwork; neither is sortable,
 * so the header starts at 3. These numbers are grid columns and must stay in
 * step with `grid-template-columns` in songs.css and with the children
 * `SongRow` renders.
 */
const COLUMNS: readonly Column[] = [
  { id: "title", label: "Title", cell: 3 },
  { id: "artist", label: "Artist", cell: 3 },
  { id: "album", label: "Album", cell: 4 },
  { id: "bpm", label: "BPM", cell: 5, numeric: true },
  { id: "key", label: "Key", cell: 6, numeric: true },
];

type Load =
  | { kind: "loading" }
  | { kind: "ready"; rows: Row[] }
  | { kind: "error"; message: string };

/**
 * The flat track table.
 *
 * Rendered on its own, it brings a heading and a filter box. Rendered inside
 * Library — which is where it now lives, as the "Songs" tab — it brings
 * neither: Library already has a title and a search field, and a screen with
 * two search boxes that filter the same list is a screen nobody can use.
 *
 * `query` is therefore optional. Supplied, it is the search field above; left
 * out, the table owns its own.
 */
export function Songs({
  onOpen,
  query: externalQuery,
  filter,
}: {
  onOpen?: ((href: string) => void) | undefined;
  query?: string | undefined;
  /**
   * Narrow to one album or artist, exactly.
   *
   * Set when a sleeve has been opened, not when something has been typed:
   * a substring search for "All Melody" also matches "All Melody (Reprise)"
   * and any track whose title contains it, which is not what opening an album
   * means.
   */
  filter?: { album?: string; artist?: string } | undefined;
}) {
  const embedded = externalQuery !== undefined;
  const [ownQuery, setOwnQuery] = useState("");
  const query = externalQuery ?? ownQuery;
  const setQuery = setOwnQuery;
  const [sortKey, setSortKey] = useState<SortKey>("title");
  const [ascending, setAscending] = useState(true);
  const [load, setLoad] = useState<Load>({ kind: "loading" });
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  /**
   * Where a shift-click ranges from: the last row selected on purpose.
   *
   * Kept apart from the keyboard cursor deliberately. If the cursor were the
   * anchor, arrowing down after a shift-click would quietly move the far end
   * of the range, and the next shift-click would select something other than
   * what the person had marked.
   */
  const [anchor, setAnchor] = useState<number | null>(null);

  /** Which hrefs carry a manual tempo, so a correction can be marked as one.
   *  The table shows a number; this is what says where it came from. */
  const [overrides, setOverrides] = useState<Record<string, number>>({});
  /** The href whose BPM cell is being edited, if any. */
  const [editing, setEditing] = useState<string | null>(null);
  const [bpmError, setBpmError] = useState<string | null>(null);
  /** Bumped after a correction lands, to re-read rows with it applied. */
  const [revision, setRevision] = useState(0);
  /** Hrefs whose beat grid is being re-tracked against a corrected tempo.
   *  The number is stored the moment it is typed; the grid takes longer, and
   *  saying so is better than a cell that looks finished before it is. */
  const [regridding, setRegridding] = useState<Set<string>>(new Set());

  /** The row the keyboard is on. Separate from `selected`, which is what a
   *  bulk action applies to — moving focus should not silently change what a
   *  person is about to add to a playlist. */
  const [focused, setFocused] = useState(0);
  /** The track whose facts are being read, opened by press-and-hold. */
  const [sheetFor, setSheetFor] = useState<Row | null>(null);
  /*
   * Which row the finger went down on.
   *
   * One `useLongPress` for the whole table rather than one per row: a hook
   * cannot be called inside the map that renders them, and only one press is
   * ever in flight. The row is stashed on pointerdown so the timer, when it
   * fires, knows what it fired on.
   */
  const pressedRow = useRef<Row | null>(null);
  const hold = useLongPress(() => {
    if (pressedRow.current) setSheetFor(pressedRow.current);
  });

  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    const t = setTimeout(() => {
      core
        .libraryView({
          query,
          sortKey,
          ascending,
          groupBy: "none",
          ...(filter?.album ? { album: filter.album } : {}),
          ...(filter?.artist ? { artist: filter.artist } : {}),
        })
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
  }, [query, sortKey, ascending, revision, filter?.album, filter?.artist]);

  useEffect(() => {
    core
      .settings()
      .then((s) => setOverrides(s.bpmOverrides ?? {}))
      // Not worth surfacing: without this the corrections still apply, they
      // just are not marked as corrections.
      .catch(() => setOverrides({}));
  }, [revision]);

  // A correction re-tracks the beat grid in the background, which needs the
  // audio and so can take seconds. Two events per track — one at the start, one
  // at the end — rather than polling.
  useEffect(() => {
    const unlisten = listen<core.BpmRetrack>("bpm-retrack", (e) => {
      const { href, beats, error } = e.payload;
      const running = beats === null && error === null;
      setRegridding((current) => {
        const next = new Set(current);
        if (running) next.add(href);
        else next.delete(href);
        return next;
      });
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

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

  /**
   * Extend the selection from the anchor to `index`.
   *
   * The anchor is the last row selected deliberately — by checkbox or by
   * modifier-click — not the keyboard cursor, so arrowing around after a
   * shift-click does not silently move the far end of the range.
   *
   * Shift-click had never been implemented at all: the row handled `metaKey`
   * and `ctrlKey` and nothing else, which left selecting a run of tracks
   * possible only one modifier-click at a time.
   */
  function selectRange(index: number) {
    const from = anchor ?? index;
    const lo = Math.min(from, index);
    const hi = Math.max(from, index);
    setSelected((prev) => {
      const next = new Set(prev);
      for (let i = lo; i <= hi; i++) {
        const r = rows[i];
        if (r) next.add(r.href);
      }
      return next;
    });
  }

  /** Select deliberately, and remember where a later shift-click ranges from. */
  function selectAt(index: number, href: string) {
    toggleSelected(href);
    setAnchor(index);
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
    // The sidebar rail shows each playlist's length; without this it keeps
    // showing the count from before the add.
    window.dispatchEvent(new Event("vapor:playlists-changed"));
    setSelected(new Set());
  }

  return (
    <div className="songs">
      {!embedded && (
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
      )}

      {/*
        Sort controls, not a table header (TD-46).

        These were a `role="row"` of `role="columnheader"` buttons sitting
        above a `listbox` of `option`s. A `row` must live inside a `table`,
        `grid` or `treegrid` and there is none, so a screen reader was told
        about a table header with no table.

        Of the two ways out, this is the one that keeps what already works.
        Making the whole table a `grid` would buy cell-level navigation and
        cost the listbox model TD-33 built deliberately — where the cursor is
        separate from the selection, so an arrow key cannot silently change
        what a person is about to add to a playlist. A sortable list is a
        listbox with a set of sort buttons above it, and that is what this is.

        `aria-sort` goes with the role: it is only defined on a `columnheader`
        or `rowheader`. The state it carried is now in each button's own
        accessible name, which is where a button's state belongs.
      */}
      <div className="songs__cols" role="group" aria-label="Sort the tracks">
        {/* Grouped by cell so the header grid has exactly as many children as
            the row grid, rather than five controls fighting over four cells. */}
        {[3, 4, 5, 6].map((cell) => {
          const inCell = COLUMNS.filter((c) => c.cell === cell);
          return (
            <div
              key={cell}
              className={
                "songs__cell" +
                (inCell[0]?.numeric ? " songs__cell--num" : "")
              }
              // The narrow layout drops cells 4-6, and it cannot select them by
              // the inline `gridColumn` below. Same numbers as COLUMNS.
              data-cell={cell}
              style={{ gridColumn: cell }}
            >
              {inCell.map((col) => (
                <button
                  key={col.id}
                  // Which control is doing the sorting, for anyone who cannot
                  // see which one is lit.
                  aria-pressed={sortKey === col.id}
                  className={
                    "songs__col" + (sortKey === col.id ? " songs__col--on" : "")
                  }
                  onClick={() => toggleSort(col.id)}
                >
                  {col.label}
                  {sortKey === col.id && (
                    <>
                      <span aria-hidden="true">{ascending ? " ↑" : " ↓"}</span>
                      {/* The arrow is the sighted reader's copy of this. Both
                          are needed: an arrow glyph read aloud is "up arrow",
                          which does not say what it is sorting. */}
                      <span className="a11y-only">
                        , sorted {ascending ? "ascending" : "descending"}
                      </span>
                    </>
                  )}
                </button>
              ))}
            </div>
          );
        })}
      </div>

      {bpmError && (
        // Dismissed by fixing it or by clicking away, not by a timer: a
        // message about the number you just typed should not vanish while you
        // are still reading it.
        <ErrorNotice error={bpmError} onDismiss={() => setBpmError(null)} />
      )}

      {/* Overlaid rather than inserted above the table.
       *
       * In the flow it pushed every row down the moment a row was selected —
       * so the row you had just clicked moved out from under the pointer, and
       * the second click of a double-click landed on this bar instead. Double
       * -clicking a BPM cell opened the "Add to…" dropdown rather than the
       * editor, and the tempo correction (TD-10) was unreachable by the
       * gesture that is supposed to reach it. Found by an end-to-end test;
       * invisible in jsdom, where nothing has a position. */}
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
                {...hold.handlers}
                onPointerDown={(e) => {
                  pressedRow.current = row;
                  hold.handlers.onPointerDown(e);
                }}
                draggable
                onDragStart={(e) => {
                  // Starting a drag is a press and hold too. Without this the
                  // sheet opens on the way to a playlist.
                  hold.cancel();
                  // A drag on a selected row takes the whole selection; on an
                  // unselected one it takes just that row, which keeps the
                  // single-track case direct. The table has had multi-select
                  // since TD-33 and dragging six rows one at a time to the
                  // same playlist is not a feature anyone wants.
                  const hrefs = selected.has(row.href)
                    ? rows.filter((r) => selected.has(r.href)).map((r) => r.href)
                    : [row.href];
                  startTrackDrag(e, hrefs);
                }}
                /* Double-click opens the track, the way double-clicking a file
                 * opens it. It used to play — but playing is now the single
                 * click, and the liner notes were reachable only through a
                 * small button that appears on hover over the artwork, which
                 * is not a place anyone looks for them. */
                onDoubleClick={() => onOpen?.(row.href)}
                onClick={(e) => {
                  /*
                   * One click plays, like tapping a row on a phone.
                   *
                   * This used to select, and selecting was the only thing a
                   * plain click did — on a screen whose entire purpose is
                   * playing something. Selection has moved to the checkbox,
                   * where it is visible and where a shift-click can extend it.
                   *
                   * `detail` counts the clicks in the sequence: the second
                   * click of a double-click reports 2, and ignoring it stops
                   * the track restarting from 0:00 on the way to opening the
                   * liner notes.
                   */
                  if (e.detail > 1) return;
                  // A press-and-hold has already opened the sheet; the click
                  // that follows it must not also start the track.
                  if (hold.swallowClick()) return;
                  setFocused(item.index);

                  if (e.shiftKey) {
                    selectRange(item.index);
                    return;
                  }
                  if (e.metaKey || e.ctrlKey) {
                    selectAt(item.index, row.href);
                    return;
                  }
                  void playFrom(row.href);
                }}
              >
                <SelectBox
                  checked={selected.has(row.href)}
                  title={row.title}
                  onPick={(shift) => {
                    if (shift) selectRange(item.index);
                    else selectAt(item.index, row.href);
                  }}
                />
                <SongRow
                  row={row}
                  onOpen={onOpen}
                  overridden={row.href in overrides}
                  regridding={regridding.has(row.href)}
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

      {sheetFor && (
        <TrackSheet
          title={sheetFor.title}
          artist={sheetFor.artistSource === "unknown" ? "" : sheetFor.artist}
          album={sheetFor.albumSource === "unknown" ? "" : sheetFor.album}
          bpm={sheetFor.bpm}
          musicalKey={sheetFor.key}
          {...(onOpen
            ? { onOpen: () => onOpen(sheetFor.href) }
            : {})}
          onClose={() => setSheetFor(null)}
        />
      )}
    </div>
  );
}

/**
 * The row's selection control.
 *
 * A real `<input type="checkbox">`, so it announces its state, answers Space,
 * and looks like what it is. Selection used to have no visible control at all —
 * it was a side effect of clicking the row, which meant there was nothing on
 * screen to tell you selecting was possible, and nothing to click to undo it.
 *
 * The click is stopped here rather than being allowed to reach the row: the
 * row plays now, and ticking a box to mark a track must not also start it.
 */
function SelectBox({
  checked,
  title,
  onPick,
}: {
  checked: boolean;
  title: string;
  onPick: (shift: boolean) => void;
}) {
  return (
    <input
      type="checkbox"
      className="songrow__pick"
      checked={checked}
      aria-label={`Select ${title}`}
      onClick={(e) => {
        e.stopPropagation();
        // Shift extends from the last deliberate selection. Read from the
        // click rather than from a keydown listener, so it works on the first
        // interaction with no prior focus.
        onPick(e.shiftKey);
      }}
      onDoubleClick={(e) => e.stopPropagation()}
      // React warns about a checked input with no handler; the click above is
      // what actually drives it, because it is the only place the modifier is
      // available.
      onChange={() => {}}
    />
  );
}

function SongRow({
  row,
  onOpen,
  overridden,
  regridding,
  editing,
  onEdit,
  onCommit,
  onCancel,
}: {
  row: Row;
  onOpen?: ((href: string) => void) | undefined;
  overridden: boolean;
  regridding: boolean;
  editing: boolean;
  onEdit: () => void;
  onCommit: (raw: string) => void;
  onCancel: () => void;
}) {
  const artist = row.artistSource === "unknown" ? "—" : row.artist;
  const album = row.albumSource === "unknown" ? "—" : row.album;
  /*
   * Tempo and key for the narrow layout, which has no columns for them.
   *
   * Built the way the cells build them: an unanalysed track contributes
   * nothing rather than a fabricated 120 BPM, and a row with neither shows no
   * sub-line at all instead of "— · —".
   */
  const meta = [row.bpm > 0 ? `${Math.round(row.bpm)} BPM` : null, row.key]
    .filter(Boolean)
    .join(" · ");

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
        {/* Shown only below the tablet breakpoint; the wide table has columns
            for both of these and would be saying it twice. */}
        {meta && <span className="songrow__meta">{meta}</span>}
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
        regridding={regridding}
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
  regridding,
  editing,
  onEdit,
  onCommit,
  onCancel,
}: {
  bpm: number;
  overridden: boolean;
  regridding: boolean;
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
        "songrow__bpm numeric" +
        (overridden ? " songrow__bpm--manual" : "") +
        (regridding ? " songrow__bpm--regridding" : "")
      }
      // The correction is saved the instant it is typed; the beat grid it
      // implies has to be tracked against the audio, which takes seconds. A
      // cell that looked finished during that window would be claiming the mix
      // is aligned before it is.
      title={
        regridding
          ? "Saved. Finding the beats at this tempo — mixing uses an approximate grid until it is done."
          : overridden
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

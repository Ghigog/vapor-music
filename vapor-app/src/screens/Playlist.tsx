/**
 * One playlist (TD-44).
 *
 * The screen that was missing: playlists could be created only through a
 * command nothing called, added to only through a dropdown, and never seen.
 * `vapor-library`'s `playlist.rs` has had rename, delete, remove and reorder
 * all along — this is the surface for them.
 *
 * ## Rows come from the backend, in playlist order
 *
 * A playlist stores hrefs. Turning those into the title/artist/BPM/key a table
 * shows means the tag and analysis lookup every other table already does, so
 * `playlist_rows` does it there rather than a second copy of it here. Tracks
 * whose files have left the library are dropped from the result, which is why
 * the count beside the title can exceed the rows shown — said plainly rather
 * than rendered as blank rows nobody can play.
 *
 * Reordering follows the Queue's approach for the same reasons: HTML5 drag
 * events rather than a library, applied optimistically, with explicit buttons
 * so the keyboard is not left out.
 *
 * ## Why this screen and `SmartGroup` are written to match (POL-6)
 *
 * They are one feature — a collection you open, name and keep — and they read
 * as two: different title sizes, a download button on one, Play and Delete
 * buttons on one, and a rename gesture on one that a phone cannot perform. The
 * title treatment, the download button and the tap-the-name rename are now the
 * same on both.
 *
 * Play went and stayed gone: playing is what pressing a row does, and there
 * was no reason for a second control saying so. Delete came back, because
 * taking it away left nothing in the app that could delete a playlist —
 * `delete_playlist` had no caller at all. It is on the group screen too now,
 * which never had one, so the two screens still match. It asks first: a
 * destructive control sitting in a header beside a rename you tap is an easy
 * thing to hit by accident on a phone.
 *
 * ## Removing a track is a gesture, and a button the keyboard can reach
 *
 * Hold a row and drag it to the panel that rises at the bottom. The row's ✕
 * was a 24px target revealed on hover, which on a phone means revealed by
 * nothing at all.
 *
 * Hover was the wrong trigger; a button was not. A drag is the only way to
 * express "remove" with a pointer, and a keyboard cannot drag — so the gesture
 * alone left desktop keyboard users, on three of the four platforms this
 * ships on, with no way to take a track out. The button is back in the row's
 * actions, `a11y-only` so no ✕ returns to the row for a pointer, and revealed
 * when focus reaches it so a keyboard user can see what they are about to
 * press.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import * as core from "../lib/core";
import { DownloadButton } from "../components/DownloadButton";
import { Confirm } from "../components/Confirm";
import { Empty } from "../components/States";
import { ErrorNotice } from "../components/ErrorNotice";
import { overlayRoot } from "../components/overlay";
import { writeDrag } from "../lib/drag";
import { useLongPress } from "../lib/longPress";

/** Tell the rail its counts have moved. */
function announce() {
  window.dispatchEvent(new Event("vapor:playlists-changed"));
}

/** A row being carried by a finger, and what the preview says. */
interface Carried {
  index: number;
  label: string;
}

export function Playlist({
  id,
  onOpen,
  onGone,
}: {
  id: string;
  onOpen: (href: string) => void;
  /** The playlist was deleted, so there is nothing left to show. */
  onGone: () => void;
}) {
  const [meta, setMeta] = useState<core.Playlist | null>(null);
  const [rows, setRows] = useState<core.Row[] | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [renaming, setRenaming] = useState(false);
  /** The Delete in the header has been pressed, and is waiting to be meant. */
  const [confirming, setConfirming] = useState(false);
  const [name, setName] = useState("");
  const [dragging, setDragging] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  /** The row a finger picked up, once the hold has armed and moved. */
  const [carried, setCarried] = useState<Carried | null>(null);
  const [at, setAt] = useState({ x: 0, y: 0 });
  /** Whether whatever is being carried is over the Remove panel. */
  const [overBin, setOverBin] = useState(false);
  const bin = useRef<HTMLDivElement>(null);
  /**
   * Which row the finger went down on.
   *
   * One `useLongPress` for the whole list rather than one per row — a hook
   * cannot be called inside the map that renders them, and only one press is
   * ever in flight. The same shape the Songs table uses.
   */
  const pressed = useRef<Carried | null>(null);
  /**
   * The press that just ended picked a row up, so the click behind it is not
   * a tap on that row.
   *
   * Not `useLongPress`'s own `swallowClick`, which is set by any press that
   * outlives the hold delay — here that includes a slow mouse click on a row,
   * which has to keep playing the track. Only an actual pick-up swallows.
   * Cleared by the next press, so a swallow that finds no click cannot eat the
   * tap after it.
   */
  const liftedFrom = useRef(false);

  const refresh = useCallback(async () => {
    try {
      const [all, list] = await Promise.all([core.playlists(), core.playlistRows(id)]);
      setMeta(all.find((p) => p.id === id) ?? null);
      setRows(list);
      setError(null);
    } catch (e) {
      setError(e);
    }
  }, [id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const remove = useCallback(
    async (index: number) => {
      // Optimistic, then reconciled — the same reasoning as the Queue's reorder.
      setRows((r) => r && r.filter((_, i) => i !== index));
      try {
        await core.removePlaylistTrack(id, index);
        announce();
      } catch (e) {
        setError(e);
      }
      void refresh();
    },
    [id, refresh],
  );

  /** Delete the playlist, once the confirmation has been answered. */
  async function destroy() {
    setConfirming(false);
    try {
      await core.deletePlaylist(id);
      // The rail lists these; one it never hears about stays on screen.
      announce();
      onGone();
    } catch (e) {
      setError(e);
    }
  }

  const hold = useLongPress(
    // Held and released in place does nothing here: a row has no sheet.
    () => {},
    (x, y) => {
      const held = pressed.current;
      if (!held) return;
      liftedFrom.current = true;
      setCarried(held);
      setAt({ x, y });
    },
  );

  /**
   * The finger, for as long as it is carrying a row.
   *
   * On `window` rather than on the row, because the gesture ends over a panel
   * at the other end of the screen. The panel is found by its box rather than
   * by `elementFromPoint`, which would answer with the preview following the
   * finger.
   */
  useEffect(() => {
    if (!carried) return;

    const inBin = (x: number, y: number) => {
      const box = bin.current?.getBoundingClientRect();
      return (
        !!box && x >= box.left && x <= box.right && y >= box.top && y <= box.bottom
      );
    };

    const onMove = (e: PointerEvent) => {
      e.preventDefault();
      setAt({ x: e.clientX, y: e.clientY });
      setOverBin(inBin(e.clientX, e.clientY));
    };
    const onUp = (e: PointerEvent) => {
      const hit = inBin(e.clientX, e.clientY);
      setCarried(null);
      setOverBin(false);
      if (hit) void remove(carried.index);
    };
    const onCancel = () => {
      setCarried(null);
      setOverBin(false);
    };

    window.addEventListener("pointermove", onMove, { passive: false });
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
    };
  }, [carried, remove]);

  async function rename() {
    const trimmed = name.trim();
    setRenaming(false);
    if (!trimmed || trimmed === meta?.name) return;
    try {
      await core.renamePlaylist(id, trimmed);
      announce();
      void refresh();
    } catch (e) {
      setError(e);
    }
  }

  async function move(from: number, to: number) {
    if (from === to) return;
    setRows((r) => {
      if (!r) return r;
      const next = [...r];
      const [moved] = next.splice(from, 1);
      if (moved) next.splice(to, 0, moved);
      return next;
    });
    try {
      await core.reorderPlaylistTrack(id, from, to);
    } catch (e) {
      setError(e);
    }
    void refresh();
  }

  function play(startHref?: string) {
    if (!rows || rows.length === 0) return;
    void core
      .playTracks(
        rows.map((r) => r.href),
        startHref,
        // The DJ conducts within the playlist, not out of it.
        meta?.name,
        // And the playlist earns the listen. By id rather than by the name
        // beside it, so renaming a playlist does not reset its count.
        { kind: "playlist", id },
      )
      .catch(setError);
  }

  if (!meta && rows === null) return null;
  if (!meta) {
    return <Empty title="That playlist is gone" body="It may have been deleted." />;
  }

  // The playlist holds hrefs; some may no longer be in the library.
  const missing = meta.tracks.length - (rows?.length ?? 0);
  /** Something is being carried, by either input path. */
  const lifting = carried !== null || dragging !== null;

  return (
    <section className="screen playlist">
      <header className="playlist__head">
        {renaming ? (
          <input
            className="playlist__rename"
            autoFocus
            value={name}
            aria-label="Playlist name"
            onChange={(e) => setName(e.target.value)}
            onBlur={() => void rename()}
            onKeyDown={(e) => {
              if (e.key === "Enter") void rename();
              if (e.key === "Escape") setRenaming(false);
            }}
          />
        ) : (
          /* A heading you press to rename, the same as a group's. It was a
             double-click before, which is a gesture a phone does not have —
             so a playlist could not be renamed on the device this is for. */
          <h1 className="playlist__title">
            <button
              className="playlist__title-edit"
              title="Rename"
              onClick={() => {
                setName(meta.name);
                setRenaming(true);
              }}
            >
              {meta.name}
            </button>
          </h1>
        )}

        <div className="playlist__meta label">
          <span className="numeric">{meta.tracks.length}</span>
          {meta.tracks.length === 1 ? " track" : " tracks"}
          {missing > 0 && (
            <span className="playlist__missing">
              {" · "}
              <span className="numeric">{missing}</span> not in the library
            </span>
          )}
        </div>

        {/* Keeping it, which is a different question from playing it. */}
        <DownloadButton kind="playlist" id={id} hrefs={meta.tracks} />

        {/* The only way to delete a playlist anywhere in the app. */}
        <button
          type="button"
          className="playlist__delete"
          onClick={() => setConfirming(true)}
        >
          Delete
        </button>
      </header>

      {/* Conditionally: `ErrorNotice` renders whatever it is given, so an
          unguarded one is a permanent red bar saying nothing went wrong. */}
      {error != null && (
        <ErrorNotice error={error} onDismiss={() => setError(null)} />
      )}

      {rows && rows.length === 0 ? (
        <Empty
          title="Nothing in here yet"
          body="Drag tracks, albums or artists onto this playlist in the bar at the bottom."
        />
      ) : (
        <ol className="playlist__list">
          {rows?.map((row, index) => (
            <li
              key={`${row.href}-${index}`}
              className={
                "playlist__row" +
                (dragging === index || carried?.index === index
                  ? " playlist__row--dragging"
                  : "") +
                (over === index && dragging !== null && dragging !== index
                  ? " playlist__row--over"
                  : "")
              }
              draggable
              {...hold.handlers}
              onPointerDown={(e) => {
                pressed.current = { index, label: row.title };
                liftedFrom.current = false;
                hold.handlers.onPointerDown(e);
              }}
              onDragStart={(e) => {
                // Starting a native drag is the same opening gesture as a
                // hold; without this the timer is still running when the drag
                // is already under way.
                hold.cancel();
                setDragging(index);
                // Also carries the track payload, so a row can be dragged out
                // to another playlist in the rail as well as reordered here.
                writeDrag(e.dataTransfer, {
                  kind: "track",
                  values: [row.href],
                  label: row.title,
                });
                // Both, and after `writeDrag`, which sets plain "copy". This
                // row is two drags at once: a move within this list, and a
                // copy onto a rail. `effectAllowed` has to permit whichever
                // `dropEffect` the target asks for, and a target asking for
                // one the drag did not allow is a drop the browser refuses.
                e.dataTransfer.effectAllowed = "copyMove";
              }}
              onDragOver={(e) => {
                if (dragging === null) return;
                e.preventDefault();
                e.dataTransfer.dropEffect = "move";
                setOver(index);
              }}
              onDragEnd={() => {
                setDragging(null);
                setOver(null);
              }}
              onDrop={(e) => {
                e.preventDefault();
                if (dragging !== null) void move(dragging, index);
                setDragging(null);
                setOver(null);
              }}
            >
              <span className="playlist__grip" aria-hidden="true">
                ⠿
              </span>
              <span className="playlist__index numeric">{index + 1}</span>
              <button
                className="playlist__open"
                onClick={() => {
                  // A hold that became a drag can end in a click on whatever
                  // was under the finger; without this, taking a track out
                  // plays it on the way.
                  if (liftedFrom.current) {
                    liftedFrom.current = false;
                    return;
                  }
                  play(row.href);
                }}
              >
                <span className="playlist__song">{row.title}</span>
                <span className="playlist__artist">{row.artist || "—"}</span>
              </button>
              <span className="playlist__bpm numeric">
                {row.bpm > 0 ? Math.round(row.bpm) : "—"}
              </span>
              <span className="playlist__key numeric">{row.key || "—"}</span>
              {/* Labelled, not just titled. The glyph is the visible content,
                  so without an explicit label a screen reader announces
                  "up arrow" and "multiplication x" — `title` is a tooltip, and
                  a tooltip is not a name. Each label names the track too, or
                  four rows of "Move up" are indistinguishable. */}
              <div className="playlist__row-actions">
                <button
                  aria-label={`Move ${row.title} up`}
                  title="Move up"
                  disabled={index === 0}
                  onClick={() => void move(index, index - 1)}
                >
                  ↑
                </button>
                <button
                  aria-label={`Move ${row.title} down`}
                  title="Move down"
                  disabled={!rows || index === rows.length - 1}
                  onClick={() => void move(index, index + 1)}
                >
                  ↓
                </button>
                <button
                  aria-label={`Liner notes for ${row.title}`}
                  title="Liner notes"
                  onClick={() => onOpen(row.href)}
                >
                  ⓘ
                </button>
                {/* Not drawn for a pointer — that control is the drag, and a
                    hover-revealed ✕ is what POL-6 took out. `a11y-only` keeps
                    it in the accessibility tree and in the tab order, and the
                    stylesheet gives it back its box once focus lands on it. */}
                <button
                  className="playlist__remove a11y-only"
                  aria-label={`Remove ${row.title} from this playlist`}
                  title="Remove from this playlist"
                  onClick={() => void remove(index)}
                >
                  ✕
                </button>
              </div>
            </li>
          ))}
        </ol>
      )}

      {/*
        Where a row goes to be taken out.

        Only while something is being carried: a bin sitting there permanently
        is a thing to fall onto by accident. It takes both input paths — the
        native drag a mouse starts, and the finger the effect above follows.
      */}
      {lifting && (
        <div
          ref={bin}
          className={"playlist__bin" + (overBin ? " playlist__bin--over" : "")}
          onDragOver={(e) => {
            if (dragging === null) return;
            e.preventDefault();
            e.dataTransfer.dropEffect = "move";
            setOverBin(true);
          }}
          onDragLeave={() => setOverBin(false)}
          onDrop={(e) => {
            e.preventDefault();
            const index = dragging;
            setDragging(null);
            setOver(null);
            setOverBin(false);
            if (index !== null) void remove(index);
          }}
        >
          Remove
        </div>
      )}

      {confirming && (
        <Confirm
          title={`Delete ${meta.name}?`}
          body="The playlist goes. The tracks stay in your library."
          confirmLabel="Delete"
          onConfirm={() => void destroy()}
          onCancel={() => setConfirming(false)}
        />
      )}

      {/* What follows the finger. The class is the drag layer's, which also
          carries the rule that stops the page scrolling under a carried row. */}
      {carried &&
        createPortal(
          <div
            className="draglayer"
            style={{ left: at.x, top: at.y }}
            aria-hidden="true"
          >
            <span className="draglayer__label">{carried.label}</span>
          </div>,
          overlayRoot(),
        )}
    </section>
  );
}

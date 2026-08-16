/**
 * Playlists in the sidebar (TD-44), and the target for dragging tracks onto
 * them (TD-31).
 *
 * ## Why the sidebar and not a tab
 *
 * The design puts Playlists among the Library's tabs, and that is where you go
 * to *browse* them. But a drop target has to be visible from wherever the drag
 * starts, and drags start in the Songs table — a different screen. A tab cannot
 * be dropped onto from another screen; a rail that is always on screen can.
 *
 * This is also what the Godot build does: `sidebar_playlist_item.gd` implements
 * `_can_drop_data`/`_drop_data` for a payload of `{type: "track", href}`, and
 * `track_drag_button.gd` produces exactly that. The payload here is the same
 * idea in the platform's own vocabulary — a JSON array of hrefs on
 * `application/x-vapor-tracks`.
 *
 * ## Dropping a selection, not just a row
 *
 * The Godot original drags one track. Here a drag that starts on a *selected*
 * row carries the whole selection, because the table already has multi-select
 * (TD-33) and dragging six rows one at a time to the same playlist is not a
 * feature anyone wants. A drag on an unselected row carries just that row,
 * which is what makes the single-track case still feel direct.
 */
import { useCallback, useEffect, useState } from "react";
import * as core from "../lib/core";

/** The drag payload: a JSON array of hrefs. */
export const TRACK_DRAG_TYPE = "application/x-vapor-tracks";

/** Read the dragged hrefs, or `null` when this drag is not ours. */
export function draggedTracks(e: React.DragEvent): string[] | null {
  const raw = e.dataTransfer.getData(TRACK_DRAG_TYPE);
  if (!raw) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as string[]) : null;
  } catch {
    return null;
  }
}

/**
 * Start a track drag carrying `hrefs`.
 *
 * `text/plain` is set as well because Firefox refuses to begin a drag without
 * a standard type present — the same reason the Queue's reorder sets it.
 */
export function startTrackDrag(e: React.DragEvent, hrefs: string[]) {
  e.dataTransfer.effectAllowed = "copy";
  e.dataTransfer.setData(TRACK_DRAG_TYPE, JSON.stringify(hrefs));
  e.dataTransfer.setData("text/plain", hrefs.join("\n"));
}

export function PlaylistRail({
  activeId,
  onOpen,
}: {
  activeId: string | null;
  onOpen: (id: string) => void;
}) {
  const [playlists, setPlaylists] = useState<core.Playlist[]>([]);
  const [over, setOver] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  /** What the last drop did, so a drag that lands has an answer. */
  const [flash, setFlash] = useState<string | null>(null);

  const refresh = useCallback(() => {
    core
      .playlists()
      .then(setPlaylists)
      .catch(() => setPlaylists([]));
  }, []);

  useEffect(refresh, [refresh]);

  // Rebuilt when anything else changes a playlist — adding from the Songs
  // selection bar, or removing a track inside one — so the counts here are not
  // quietly stale.
  useEffect(() => {
    const handler = () => refresh();
    window.addEventListener("vapor:playlists-changed", handler);
    return () => window.removeEventListener("vapor:playlists-changed", handler);
  }, [refresh]);

  useEffect(() => {
    if (!flash) return;
    const t = setTimeout(() => setFlash(null), 2600);
    return () => clearTimeout(t);
  }, [flash]);

  async function create() {
    const trimmed = name.trim();
    if (!trimmed) {
      setCreating(false);
      return;
    }
    try {
      const made = await core.createPlaylist(trimmed);
      setName("");
      setCreating(false);
      refresh();
      onOpen(made.id);
    } catch {
      setCreating(false);
    }
  }

  async function drop(e: React.DragEvent, playlist: core.Playlist) {
    e.preventDefault();
    setOver(null);
    const hrefs = draggedTracks(e);
    if (!hrefs || hrefs.length === 0) return;

    const added = await core.addTracksToPlaylist(playlist.id, hrefs).catch(() => 0);
    refresh();

    // Said plainly, including when nothing happened: dropping six tracks onto a
    // playlist that already has them looks identical to a drop that missed.
    setFlash(
      added === 0
        ? `Already in ${playlist.name}`
        : `Added ${added} to ${playlist.name}`,
    );
  }

  return (
    <div className="rail">
      <div className="rail__head">
        <span className="rail__title label">Playlists</span>
        {/* The glyph is the content, so the name has to be explicit — see
            the note on the row actions in Playlist.tsx. */}
        <button
          className="rail__new"
          aria-label="New playlist"
          title="New playlist"
          onClick={() => setCreating(true)}
        >
          +
        </button>
      </div>

      {creating && (
        <input
          className="rail__input"
          autoFocus
          value={name}
          placeholder="Playlist name"
          onChange={(e) => setName(e.target.value)}
          onBlur={() => void create()}
          onKeyDown={(e) => {
            if (e.key === "Enter") void create();
            if (e.key === "Escape") {
              setName("");
              setCreating(false);
            }
          }}
        />
      )}

      <ul className="rail__list">
        {playlists.map((p) => (
          <li key={p.id}>
            <button
              className={
                "rail__item" +
                (activeId === p.id ? " rail__item--on" : "") +
                (over === p.id ? " rail__item--over" : "")
              }
              onClick={() => onOpen(p.id)}
              onDragOver={(e) => {
                // Only claim the drop when the payload is ours, or the cursor
                // promises something this cannot accept.
                if (!e.dataTransfer.types.includes(TRACK_DRAG_TYPE)) return;
                e.preventDefault();
                e.dataTransfer.dropEffect = "copy";
                setOver(p.id);
              }}
              onDragLeave={() => setOver((cur) => (cur === p.id ? null : cur))}
              onDrop={(e) => void drop(e, p)}
            >
              <span className="rail__name">{p.name}</span>
              <span className="rail__count numeric">{p.tracks.length}</span>
            </button>
          </li>
        ))}
      </ul>

      {playlists.length === 0 && !creating && (
        <p className="rail__empty">
          None yet. Drag tracks here once you make one.
        </p>
      )}

      {flash && <div className="rail__flash">{flash}</div>}
    </div>
  );
}

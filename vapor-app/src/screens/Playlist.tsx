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
 */
import { useCallback, useEffect, useState } from "react";
import * as core from "../lib/core";
import { DownloadButton } from "../components/DownloadButton";
import { Empty } from "../components/States";
import { ErrorNotice } from "../components/ErrorNotice";
import { startTrackDrag } from "../components/PlaylistRail";

/** Tell the rail its counts have moved. */
function announce() {
  window.dispatchEvent(new Event("vapor:playlists-changed"));
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
  const [name, setName] = useState("");
  const [dragging, setDragging] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);

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

  async function remove(index: number) {
    // Optimistic, then reconciled — the same reasoning as the Queue's reorder.
    setRows((r) => r && r.filter((_, i) => i !== index));
    try {
      await core.removePlaylistTrack(id, index);
      announce();
    } catch (e) {
      setError(e);
    }
    void refresh();
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

  async function destroy() {
    try {
      await core.deletePlaylist(id);
      announce();
      onGone();
    } catch (e) {
      setError(e);
    }
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

  return (
    <section className="screen playlist">
      <header className="playlist__head">
        {renaming ? (
          <input
            className="playlist__rename"
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            onBlur={() => void rename()}
            onKeyDown={(e) => {
              if (e.key === "Enter") void rename();
              if (e.key === "Escape") setRenaming(false);
            }}
          />
        ) : (
          <h1
            className="playlist__title"
            title="Double-click to rename"
            onDoubleClick={() => {
              setName(meta.name);
              setRenaming(true);
            }}
          >
            {meta.name}
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

        <div className="playlist__actions">
          <button
            className="playlist__play"
            disabled={!rows || rows.length === 0}
            onClick={() => play()}
          >
            Play
          </button>
          <button
            className="playlist__delete"
            aria-label={`Delete the playlist ${meta.name}`}
            onClick={() => void destroy()}
          >
            Delete
          </button>
        </div>
      </header>

      {/* Conditionally: `ErrorNotice` renders whatever it is given, so an
          unguarded one is a permanent red bar saying nothing went wrong. */}
      {error != null && (
        <ErrorNotice error={error} onDismiss={() => setError(null)} />
      )}

      {rows && rows.length === 0 ? (
        <Empty
          title="Nothing in here yet"
          body="Drag tracks from Songs onto this playlist in the sidebar, or use Add to… from a selection."
        />
      ) : (
        <ol className="playlist__list">
          {rows?.map((row, index) => (
            <li
              key={`${row.href}-${index}`}
              className={
                "playlist__row" +
                (dragging === index ? " playlist__row--dragging" : "") +
                (over === index && dragging !== null && dragging !== index
                  ? " playlist__row--over"
                  : "")
              }
              draggable
              onDragStart={(e) => {
                setDragging(index);
                e.dataTransfer.effectAllowed = "move";
                // Also carries the track payload, so a row can be dragged out
                // to another playlist in the rail as well as reordered here.
                startTrackDrag(e, [row.href]);
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
              <button className="playlist__open" onClick={() => play(row.href)}>
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
                  four rows of "Remove" are indistinguishable. */}
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
                <button
                  aria-label={`Remove ${row.title}`}
                  title="Remove"
                  onClick={() => void remove(index)}
                >
                  ✕
                </button>
              </div>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

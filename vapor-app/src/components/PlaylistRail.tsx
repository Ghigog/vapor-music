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
 * ## Why `dragDropEnabled` is false
 *
 * None of this fires inside the app window unless
 * `src-tauri/tauri.conf.json` sets `dragDropEnabled: false`. The default is
 * true, which makes wry subclass the WKWebView and override
 * `draggingEntered`/`performDragOperation`; Tauri's handler returns `true`
 * unconditionally (`tauri-runtime-wry`), so `super` is never called and
 * WebKit never turns the AppKit drag session into DOM `dragover`/`drop`. The
 * drag starts and the ghost follows the cursor, and then nothing accepts it.
 * Tauri documents this for Windows; macOS behaves the same way. Nothing here
 * listens for a file dropped onto the window, so switching it off costs
 * nothing.
 *
 * It is invisible in `npm run test` and `npm run e2e`, both of which run the
 * frontend in a plain browser where the drop works.
 *
 * ## Dropping a selection, not just a row
 *
 * The Godot original drags one track. Here a drag that starts on a *selected*
 * row carries the whole selection, because the table already has multi-select
 * (TD-33) and dragging six rows one at a time to the same playlist is not a
 * feature anyone wants. A drag on an unselected row carries just that row,
 * which is what makes the single-track case still feel direct.
 *
 * ## Folders
 *
 * `playlist_folder_service.gd` is ported — `FolderStore` and the `folder_id`
 * on a playlist have been in `vapor-library`, tested, since the migration —
 * and until now the shell exposed neither, so `folderId` reached this side as
 * a field nothing could set. One level is drawn, as the original drew: nesting
 * is representable so it needs no migration later, but a tree of playlist
 * folders is a filing cabinet nobody asked for.
 *
 * A playlist is filed by dragging it onto a folder, which is the same gesture
 * as dropping tracks onto a playlist, on a different payload type. Dropping on
 * "All playlists" takes it back out.
 */
import { useCallback, useEffect, useState } from "react";
import * as core from "../lib/core";
import * as drag from "../lib/drag";
import { messageOf } from "./ErrorNotice";
import { Confirm } from "./Confirm";

/** A playlist being filed: its id, as plain text under our own type. */
export const PLAYLIST_DRAG_TYPE = "application/x-vapor-playlist";

/** What a new-name box is being opened for. */
type Creating = null | { kind: "playlist"; folderId: string } | { kind: "folder" };

export function PlaylistRail({
  activeId,
  onOpen,
}: {
  activeId: string | null;
  onOpen: (id: string) => void;
}) {
  const [playlists, setPlaylists] = useState<core.Playlist[]>([]);
  const [folders, setFolders] = useState<core.Folder[]>([]);
  const [over, setOver] = useState<string | null>(null);
  const [creating, setCreating] = useState<Creating>(null);
  /** The folder a "delete this?" question is open about. */
  const [deleting, setDeleting] = useState<core.Folder | null>(null);
  const [name, setName] = useState("");
  /** Folders currently collapsed, by id. Open is the default. */
  const [shut, setShut] = useState<ReadonlySet<string>>(new Set());
  /** What the last drop did, so a drag that lands has an answer. */
  const [flash, setFlash] = useState<string | null>(null);

  /**
   * Whether the last read actually reached the backend.
   *
   * This used to `.catch(() => setPlaylists([]))`, so a failed call rendered as
   * "None yet" — a person who had just made a playlist was told they had none.
   * The app restarts under `tauri dev` on every backend edit, and a call in
   * flight across a restart fails, which is exactly when someone is most likely
   * to be looking. Nothing was ever lost; the rail simply reported an error as
   * an answer.
   *
   * On failure the previous list is kept rather than cleared. Stale is a better
   * lie than empty: it is at least what was true a moment ago, and the next
   * refresh corrects it.
   */
  const [reachable, setReachable] = useState(true);

  const refresh = useCallback(() => {
    core
      .playlists()
      .then((got) => {
        setPlaylists(got);
        setReachable(true);
      })
      .catch(() => setReachable(false));
    core
      .playlistFolders()
      .then(setFolders)
      .catch(() => {});
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
    const what = creating;
    if (!trimmed || !what) {
      setCreating(null);
      setName("");
      return;
    }
    try {
      if (what.kind === "folder") {
        await core.createFolder(trimmed);
        setName("");
        setCreating(null);
        refresh();
        return;
      }
      const made = await core.createPlaylist(trimmed, what.folderId);
      setName("");
      setCreating(null);
      refresh();
      onOpen(made.id);
    } catch (e: unknown) {
      setCreating(null);
      setFlash(messageOf(e));
    }
  }

  /**
   * Take a drop.
   *
   * The payload is read and acted on by `lib/drag`, not here: an album dropped
   * on a playlist has to become the tracks on it, and that resolution is the
   * library's job rather than the rail's. The message it hands back — including
   * the one for a drop that added nothing — is what goes in the flash.
   */
  async function drop(e: React.DragEvent, playlist: core.Playlist) {
    e.preventDefault();
    setOver(null);
    const payload = drag.readDrag(e.dataTransfer);
    if (!payload) return;

    try {
      setFlash(
        await drag.dropOn(payload, {
          menu: "playlist",
          id: playlist.id,
          name: playlist.name,
        }),
      );
    } catch (err: unknown) {
      setFlash(messageOf(err));
    }
    refresh();
  }

  /** File the dragged playlist into `folder`, or out of one when it is null. */
  async function fileInto(e: React.DragEvent, folder: core.Folder | null) {
    e.preventDefault();
    setOver(null);
    const id = e.dataTransfer.getData(PLAYLIST_DRAG_TYPE);
    if (!id) return;

    const playlist = playlists.find((p) => p.id === id);
    // Already where it was dropped: say so rather than reporting a move.
    if (playlist && playlist.folderId === (folder?.id ?? "")) {
      setFlash(
        folder ? `Already in ${folder.name}` : "Already outside any folder",
      );
      return;
    }

    try {
      await core.setPlaylistFolder(id, folder?.id ?? "");
      refresh();
      setFlash(
        folder
          ? `Moved ${playlist?.name ?? "playlist"} to ${folder.name}`
          : `Moved ${playlist?.name ?? "playlist"} out of its folder`,
      );
    } catch (err: unknown) {
      setFlash(messageOf(err));
    }
  }

  /**
   * Delete a folder.
   *
   * Confirmed because it is not undoable, but not alarming: the playlists come
   * back out to the top level rather than going with it, and the prompt says
   * so — otherwise a person declines a safe action for fear of an unsafe one.
   */
  async function removeFolder(folder: core.Folder) {
    setDeleting(null);
    try {
      await core.deleteFolder(folder.id);
      refresh();
      setFlash(`Deleted ${folder.name}`);
    } catch (e: unknown) {
      setFlash(messageOf(e));
    }
  }

  function toggle(id: string) {
    setShut((cur) => {
      const next = new Set(cur);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }

  /** The rows for one folder, or the top level when `folderId` is "". */
  function itemsIn(folderId: string) {
    return playlists.filter((p) => p.folderId === folderId);
  }

  const item = (p: core.Playlist) => (
    <li key={p.id}>
      <button
        className={
          "rail__item" +
          (activeId === p.id ? " rail__item--on" : "") +
          (over === p.id ? " rail__item--over" : "")
        }
        /* The touch path finds its targets by these rather than by React
           handlers, because it is driven by pointer events on the window and
           never sees a `drop` at all. On a window wide enough to show the rail
           they point at the same rows the mouse does. */
        data-drop-id={p.id}
        data-drop-name={p.name}
        draggable
        onDragStart={(e) => {
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData(PLAYLIST_DRAG_TYPE, p.id);
          // Firefox refuses to start a drag without a standard type present.
          e.dataTransfer.setData("text/plain", p.name);
        }}
        onClick={() => onOpen(p.id)}
        onDragOver={(e) => {
          // Only claim the drop when the payload is ours, or the cursor
          // promises something this cannot accept. A playlist takes every
          // kind, so the kind itself does not have to be checked here — a
          // group is the one that refuses, and it asks `accepts`.
          if (!drag.kindOf(e.dataTransfer)) return;
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
  );

  return (
    <div className="rail">
      <div className="rail__head">
        <span className="rail__title label">Playlists</span>
        {/* The glyph is the content, so the name has to be explicit — see
            the note on the row actions in Playlist.tsx. */}
        <span className="rail__actions">
          <button
            className="rail__new"
            aria-label="New folder"
            title="New folder"
            onClick={() => setCreating({ kind: "folder" })}
          >
            <span className="icon icon--group" aria-hidden="true" />
          </button>
          <button
            className="rail__new"
            aria-label="New playlist"
            title="New playlist"
            onClick={() => setCreating({ kind: "playlist", folderId: "" })}
          >
            +
          </button>
        </span>
      </div>

      {creating && (
        <input
          className="rail__input"
          autoFocus
          value={name}
          aria-label={creating.kind === "folder" ? "Folder name" : "Playlist name"}
          placeholder={
            creating.kind === "folder" ? "Folder name" : "Playlist name"
          }
          onChange={(e) => setName(e.target.value)}
          onBlur={() => void create()}
          onKeyDown={(e) => {
            if (e.key === "Enter") void create();
            if (e.key === "Escape") {
              setName("");
              setCreating(null);
            }
          }}
        />
      )}

      <div className="rail__list">
        {folders.map((f) => {
          const inside = itemsIn(f.id);
          const open = !shut.has(f.id);
          return (
            <section
              key={f.id}
              className={
                "rail__folder" + (over === f.id ? " rail__folder--over" : "")
              }
              onDragOver={(e) => {
                if (!e.dataTransfer.types.includes(PLAYLIST_DRAG_TYPE)) return;
                e.preventDefault();
                e.dataTransfer.dropEffect = "move";
                setOver(f.id);
              }}
              onDragLeave={() => setOver((cur) => (cur === f.id ? null : cur))}
              onDrop={(e) => void fileInto(e, f)}
            >
              <div className="rail__folder-head">
                <button
                  className="rail__folder-name"
                  aria-expanded={open}
                  onClick={() => toggle(f.id)}
                >
                  <span className="rail__chevron" aria-hidden="true">
                    {open ? "⌄" : "›"}
                  </span>
                  <span className="rail__name">{f.name}</span>
                  <span className="rail__count numeric">{inside.length}</span>
                </button>
                {/* The glyph is the content, so the name has to be explicit. */}
                <button
                  className="rail__folder-del"
                  aria-label={`Delete folder ${f.name}`}
                  title={`Delete folder ${f.name}`}
                  onClick={() => setDeleting(f)}
                >
                  ×
                </button>
              </div>

              {open && (
                <ul className="rail__sublist" data-menu="playlist">
                  {inside.map(item)}
                  {inside.length === 0 && (
                    <li className="rail__empty rail__empty--in">
                      Drag a playlist here.
                    </li>
                  )}
                </ul>
              )}
            </section>
          );
        })}

        {/* The way back out. Without a target for "no folder", a playlist
            dragged into one could never be dragged out again. */}
        {folders.length > 0 && (
          <p
            className={
              "rail__loose" + (over === "" ? " rail__loose--over" : "")
            }
            onDragOver={(e) => {
              if (!e.dataTransfer.types.includes(PLAYLIST_DRAG_TYPE)) return;
              e.preventDefault();
              e.dataTransfer.dropEffect = "move";
              setOver("");
            }}
            onDragLeave={() => setOver((cur) => (cur === "" ? null : cur))}
            onDrop={(e) => void fileInto(e, null)}
          >
            Not in a folder
          </p>
        )}

        <ul className="rail__sublist" data-menu="playlist">
          {itemsIn("").map(item)}
        </ul>
      </div>

      {playlists.length === 0 && folders.length === 0 && !creating && (
        <p className="rail__empty">
          {reachable
            ? "Create a playlist: +"
            : "Could not read your playlists. They are still on disk."}
        </p>
      )}

      {flash && <div className="rail__flash">{flash}</div>}

      {deleting && (
        <Confirm
          title={`Delete the folder "${deleting.name}"?`}
          body={(() => {
            const inside = playlists.filter(
              (p) => p.folderId === deleting.id,
            ).length;
            return inside === 0
              ? "The folder is empty."
              : `The ${inside} ${inside === 1 ? "playlist" : "playlists"} inside will move back to the top level, not be deleted.`;
          })()}
          onConfirm={() => void removeFolder(deleting)}
          onCancel={() => setDeleting(null)}
        />
      )}
    </div>
  );
}

/**
 * Dynamic groups in the sidebar.
 *
 * They existed only in the mobile tab bar, so on a desktop window the feature
 * was unreachable: the rail listed playlists and nothing else, and there was no
 * other way in.
 *
 * A separate component from `PlaylistRail` rather than a second section inside
 * it, because almost none of that file applies here. There are no folders to
 * file a group into, and what it takes on a drop is not what a playlist takes.
 * What is left is a heading, a list, and a way to make one.
 *
 * ## It is a drop target after all
 *
 * This said it was not one, and its own empty state told you to "drag an
 * artist, album or genre onto it" — an instruction for a gesture nothing
 * implemented. Both halves were missing: no handlers here, and no drag source
 * anywhere that produced an artist, an album or a genre. `lib/drag` had always
 * had the rule for it (`accepts` refuses a bare track, `dropOn` calls
 * `add_to_group`), and no caller could reach that branch.
 *
 * The refusal is the reason to reject the drag in `dragover` rather than at the
 * drop: a group that lights up under a track and then explains itself is worse
 * than one that never lights up. The message is still there for the touch path,
 * which has no cursor to say it with.
 */
import { useCallback, useEffect, useState } from "react";

import * as core from "../lib/core";
import * as drag from "../lib/drag";
import { messageOf } from "./ErrorNotice";

/** Tell every rail that the groups have changed. */
export function groupsChanged() {
  window.dispatchEvent(new Event("vapor:groups-changed"));
}

export function GroupRail({
  activeId,
  onOpen,
}: {
  activeId: string | null;
  onOpen: (id: string) => void;
}) {
  const [groups, setGroups] = useState<core.DynamicGroup[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  /** The group a drag is currently over, for the highlight. */
  const [over, setOver] = useState<string | null>(null);
  /** What the last drop did, so a drag that lands has an answer. */
  const [flash, setFlash] = useState<string | null>(null);

  /** Whether the last read reached the backend — see `PlaylistRail`, which had
   *  the same fault: a failed call rendered as "None yet". */
  const [reachable, setReachable] = useState(true);

  const refresh = useCallback(() => {
    core
      .dynamicGroups()
      .then((got) => {
        setGroups(got);
        setReachable(true);
      })
      // Keep what was there. Stale beats claiming the person has none.
      .catch(() => setReachable(false));
  }, []);

  useEffect(() => {
    refresh();
    const handler = () => refresh();
    window.addEventListener("vapor:groups-changed", handler);
    return () => window.removeEventListener("vapor:groups-changed", handler);
  }, [refresh]);

  useEffect(() => {
    if (!flash) return;
    const t = setTimeout(() => setFlash(null), 2600);
    return () => clearTimeout(t);
  }, [flash]);

  async function drop(e: React.DragEvent, group: core.DynamicGroup) {
    e.preventDefault();
    setOver(null);
    const payload = drag.readDrag(e.dataTransfer);
    if (!payload) return;

    try {
      setFlash(
        await drag.dropOn(payload, {
          menu: "group",
          id: group.id,
          name: group.name,
        }),
      );
    } catch (err: unknown) {
      setFlash(messageOf(err));
    }
    refresh();
  }

  async function create() {
    const trimmed = name.trim();
    setName("");
    setCreating(false);
    if (!trimmed) return;
    try {
      const made = await core.createGroup(trimmed);
      refresh();
      onOpen(made.id);
    } catch {
      // The rail is a list, not a place to report a failed write; the next
      // refresh shows the truth either way.
      refresh();
    }
  }

  return (
    <div className="rail">
      <div className="rail__head">
        <span className="rail__title label">Dynamic groups</span>
        <span className="rail__actions">
          <button
            className="rail__new"
            aria-label="New group"
            title="New group"
            onClick={() => setCreating(true)}
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
          aria-label="Group name"
          placeholder="Group name"
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

      <div className="rail__list">
        {/* `data-menu` and `data-drop-id` are how the touch layer finds these;
            it works off pointer events on the window and never sees a `drop`. */}
        <ul className="rail__sublist" data-menu="group">
          {groups.map((g) => (
            <li key={g.id}>
              <button
                className={
                  "rail__item" +
                  (activeId === g.id ? " rail__item--on" : "") +
                  (over === g.id ? " rail__item--over" : "")
                }
                data-drop-id={g.id}
                data-drop-name={g.name}
                onClick={() => onOpen(g.id)}
                onDragOver={(e) => {
                  // A track is turned away here rather than at the drop, so it
                  // never lights up under something it would only refuse. The
                  // cursor says "no" on its own once we decline to preventDefault.
                  const kind = drag.kindOf(e.dataTransfer);
                  if (!kind || !drag.accepts(kind, "group")) return;
                  e.preventDefault();
                  e.dataTransfer.dropEffect = "copy";
                  setOver(g.id);
                }}
                onDragLeave={() => setOver((cur) => (cur === g.id ? null : cur))}
                onDrop={(e) => void drop(e, g)}
              >
                <span className="rail__name">{g.name}</span>
                <span className="rail__count numeric">{g.entities.length}</span>
              </button>
            </li>
          ))}
        </ul>
      </div>

      {groups.length === 0 && !creating && (
        <p className="rail__empty">
          {reachable
            ? "Create a group +"
            : "Could not read your groups. They are still on disk."}
        </p>
      )}

      {flash && <div className="rail__flash">{flash}</div>}
    </div>
  );
}

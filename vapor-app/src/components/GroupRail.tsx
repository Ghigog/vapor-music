/**
 * Dynamic groups in the sidebar.
 *
 * They existed only in the mobile tab bar, so on a desktop window the feature
 * was unreachable: the rail listed playlists and nothing else, and there was no
 * other way in.
 *
 * A separate component from `PlaylistRail` rather than a second section inside
 * it, because almost none of that file applies here. A group is not a list of
 * tracks you drop onto — it holds artists, albums and genres and works out its
 * own membership — so it is not a drag target, and there are no folders to file
 * it into. What is left is a heading, a list, and a way to make one.
 */
import { useCallback, useEffect, useState } from "react";

import * as core from "../lib/core";

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
        <ul className="rail__sublist">
          {groups.map((g) => (
            <li key={g.id}>
              <button
                className={
                  "rail__item" + (activeId === g.id ? " rail__item--on" : "")
                }
                onClick={() => onOpen(g.id)}
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
    </div>
  );
}

/**
 * One smart group.
 *
 * A group holds artists, albums and genres; the tracks are worked out from the
 * library every time this opens, which is the difference from a playlist. So
 * there is no track list to edit here — you edit what the group *is*, and the
 * tracks follow.
 */
import { useCallback, useEffect, useState } from "react";
import * as core from "../lib/core";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";

export function SmartGroup({
  id,
  onOpen,
  onGone,
}: {
  id: string;
  /** Opens a track's liner notes. */
  onOpen?: ((href: string) => void) | undefined;
  /** The group was deleted, so there is nothing left to show. */
  onGone: () => void;
}) {
  const [group, setGroup] = useState<core.DynamicGroup | null>(null);
  const [tracks, setTracks] = useState<core.Row[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");

  const refresh = useCallback(async () => {
    try {
      const all = await core.dynamicGroups();
      const found = all.find((g) => g.id === id) ?? null;
      setGroup(found);
      if (!found) {
        onGone();
        return;
      }
      setTracks(await core.groupTracks(id));
    } catch (e: unknown) {
      setError(messageOf(e));
    }
  }, [id, onGone]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function commitName() {
    const next = name.trim();
    setNaming(false);
    if (!next || next === group?.name) return;
    try {
      await core.renameGroup(id, next);
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
    }
  }

  async function drop(entityType: core.EntityType, value: string) {
    try {
      await core.removeFromGroup(id, entityType, value);
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
    }
  }

  if (!group) return null;

  return (
    <div className="group">
      <ErrorNotice error={error} onDismiss={() => setError(null)} />

      <header className="group__head">
        {naming ? (
          <input
            className="group__rename"
            autoFocus
            value={name}
            aria-label="Group name"
            onChange={(e) => setName(e.target.value)}
            onBlur={() => void commitName()}
            onKeyDown={(e) => {
              if (e.key === "Enter") void commitName();
              if (e.key === "Escape") setNaming(false);
            }}
          />
        ) : (
          <button
            className="group__title"
            title="Rename"
            onClick={() => {
              setName(group.name);
              setNaming(true);
            }}
          >
            {group.name}
          </button>
        )}
        <p className="label">
          {group.entities.length === 0
            ? "empty"
            : `${group.entities.length} in the set · ${tracks.length} tracks`}
        </p>
      </header>

      {/*
        What the group is, rather than what it currently contains. Removing one
        of these is what changes the group; the list below simply follows.
      */}
      {group.entities.length === 0 ? (
        <p className="group__empty">
          Nothing in this group yet. Drag an artist, album or genre onto it — a
          group holds those rather than single tracks, which is what keeps it up
          to date as the library grows.
        </p>
      ) : (
        <ul className="group__chips">
          {group.entities.map((e) => (
            <li key={`${e.entityType}:${e.value}`}>
              <span className="group__chip">
                <span className="group__chip-kind label">{e.entityType}</span>
                <span className="group__chip-value">{e.value}</span>
                <button
                  className="group__chip-drop"
                  aria-label={`Remove ${e.value} from ${group.name}`}
                  onClick={() => void drop(e.entityType, e.value)}
                >
                  ×
                </button>
              </span>
            </li>
          ))}
        </ul>
      )}

      <ul className="group__tracks">
        {tracks.map((t) => (
          <li key={t.href}>
            <button
              className="group__track"
              onClick={() => void core.playTracks(tracks.map((r) => r.href), t.href)}
              onDoubleClick={() => onOpen?.(t.href)}
            >
              <span className="group__track-title">{t.title}</span>
              <span className="group__track-artist">{t.artist}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * Queue.
 *
 * What is playing, what follows, and the ability to change your mind about the
 * order. Reordering is the whole point of the screen — a list you can only read
 * is the transport bar with extra steps.
 *
 * ## Drag and drop without a library
 *
 * The design's answer is `dnd-kit` (TD-31). This uses the platform's own
 * HTML5 drag events instead, which for a single vertical list of rows is a few
 * handlers rather than a dependency and a sensor model. It gives keyboard
 * reordering nothing, so that is provided separately with explicit buttons —
 * which a pointer-only implementation would have needed anyway.
 *
 * The reorder is applied optimistically and reconciled from the backend's
 * answer, because the queue is authoritative and the playhead can move
 * underneath a drag.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as core from "../lib/core";
import { Empty } from "../components/States";

export function Queue({ onOpen }: { onOpen: (href: string) => void }) {
  const [view, setView] = useState<core.QueueView | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const busy = useRef(false);

  const refresh = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    try {
      setView(await core.queueView());
    } catch {
      // Left as-is; the next event or poll will correct it.
    } finally {
      busy.current = false;
    }
  }, []);

  useEffect(() => {
    void refresh();
    // The queue changes when a track ends or a mix completes, both of which
    // the supervisor announces. Polling as well would be redundant.
    const unlisten = listen("playback-changed", () => void refresh());
    return () => {
      void unlisten.then((f) => f());
    };
  }, [refresh]);

  async function move(from: number, to: number) {
    if (from === to) return;
    // Optimistic: the drop should land where it was dropped, not a round trip
    // later. The refresh below is what makes it true.
    setView((v) => {
      if (!v) return v;
      const entries = [...v.entries];
      const [moved] = entries.splice(from, 1);
      // The backend refuses an out-of-range move; the optimistic copy has to
      // agree rather than splicing `undefined` into the list.
      if (!moved) return v;
      entries.splice(to, 0, moved);
      return { ...v, entries };
    });
    await core.moveInQueue(from, to);
    await refresh();
  }

  async function remove(href: string) {
    await core.removeFromQueue(href);
    await refresh();
  }

  if (!view) return null;

  if (view.entries.length === 0) {
    return (
      <Empty
        title="Nothing queued"
        body="Play something from Songs or Search and it will line up here."
      />
    );
  }

  const current = view.entries.find((e) => e.current);
  const upNext = view.entries.filter((e) => !e.current);

  return (
    <div className="queue">
      <header className="queue__head">
        <div className="queue__head-row">
          <div>
            <h1 className="queue__title">Queue</h1>
            <p className="queue__sub label">
              {upNext.length} to come
              {view.remainingSecs > 0 && ` · ${minutes(view.remainingSecs)}`}
            </p>
          </div>
          <div className="queue__modes">
            <button
              className={
                "queue__mode" + (view.shuffled ? " queue__mode--on" : "")
              }
              aria-pressed={view.shuffled}
              title={view.shuffled ? "Back to the original order" : "Shuffle"}
              onClick={() => {
                void core.setShuffled(!view.shuffled).then(refresh);
              }}
            >
              ⤨ Shuffle
            </button>
            {/* Cycles rather than three buttons: it is one setting with three
                values, and the label says which. */}
            <button
              className={
                "queue__mode" +
                (view.repeat !== "all" ? " queue__mode--on" : "")
              }
              title="What happens at the end of the queue"
              onClick={() => {
                const next: core.RepeatMode =
                  view.repeat === "all"
                    ? "one"
                    : view.repeat === "one"
                      ? "off"
                      : "all";
                void core.setRepeat(next).then(refresh);
              }}
            >
              {view.repeat === "one"
                ? "↻ Repeat one"
                : view.repeat === "off"
                  ? "→ Play to end"
                  : "↻ Repeat all"}
            </button>
          </div>
        </div>
      </header>

      {current && (
        <section className="queue__now glass">
          <span className="queue__now-art" aria-hidden="true">
            {current.cover && <img src={current.cover} alt="" />}
          </span>
          <span className="queue__now-text">
            <span className="label">now playing</span>
            <span className="queue__now-title">{current.title}</span>
            <span className="queue__now-artist">{current.artist || "—"}</span>
          </span>
        </section>
      )}

      <h2 className="label queue__heading">up next</h2>

      <ul className="queue__list">
        {view.entries.map((entry, index) => {
          if (entry.current) return null;
          return (
            <li
              key={entry.href}
              className={
                "queue__item" +
                (dragging === index ? " queue__item--dragging" : "") +
                (over === index && dragging !== null && dragging !== index
                  ? " queue__item--over"
                  : "")
              }
              draggable
              onDragStart={(e) => {
                setDragging(index);
                e.dataTransfer.effectAllowed = "move";
                // Firefox refuses to start a drag without payload.
                e.dataTransfer.setData("text/plain", entry.href);
              }}
              onDragOver={(e) => {
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
              <span className="queue__grip" aria-hidden="true">
                ⠿
              </span>
              <button
                className="queue__row"
                onDoubleClick={() => onOpen(entry.href)}
                onClick={() => {
                  // Play from here, keeping the rest of the order intact.
                  void core
                    .playTracks(
                      view.entries.map((x) => x.href),
                      entry.href,
                    )
                    .then(refresh);
                }}
              >
                <span className="queue__row-text">
                  <span className="queue__row-title">{entry.title}</span>
                  <span className="queue__row-artist">
                    {entry.artist || "—"}
                  </span>
                </span>
                <span className="queue__row-facts numeric">
                  {entry.bpm > 0 ? Math.round(entry.bpm) : "—"}
                  {entry.key && ` · ${entry.key}`}
                </span>
              </button>

              {/* Explicit controls, because a drag handle is unreachable from a
                  keyboard and this is the only way to reorder the queue. */}
              <span className="queue__actions">
                <button
                  className="queue__action"
                  aria-label={`Move ${entry.title} up`}
                  disabled={index === 0}
                  onClick={() => void move(index, index - 1)}
                >
                  ↑
                </button>
                <button
                  className="queue__action"
                  aria-label={`Move ${entry.title} down`}
                  disabled={index === view.entries.length - 1}
                  onClick={() => void move(index, index + 1)}
                >
                  ↓
                </button>
                <button
                  className="queue__action queue__action--remove"
                  aria-label={`Remove ${entry.title} from the queue`}
                  onClick={() => void remove(entry.href)}
                >
                  ×
                </button>
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** "47 min", or "1 h 12 min" once that stops being readable. */
function minutes(seconds: number): string {
  const total = Math.round(seconds / 60);
  if (total < 60) return `${total} min`;
  return `${Math.floor(total / 60)} h ${total % 60} min`;
}

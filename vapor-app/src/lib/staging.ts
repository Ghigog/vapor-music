/**
 * Rows that arrive and leave visibly.
 *
 * ## Why this exists
 *
 * Pressing a curve used to swap the whole tail of the queue in one render: ten
 * rows vanished and ten different rows were already in their place, with a
 * pause in between where the app was working. Nothing about that reads as a
 * decision being made — it reads as a list that glitched — and the pause is
 * what people described as the app freezing, because the only feedback that
 * anything had happened arrived at the end of it.
 *
 * The set is decided a track at a time now, and announced a track at a time,
 * so new rows genuinely do arrive one after another. React mounts them as they
 * come; what it will not do is keep a removed row on screen long enough to
 * animate away, because by then it is gone from the list it was drawn from.
 * That is the whole job here: hold departed rows in place for one animation,
 * and say which rows are new so the caller can bring them in.
 *
 * Keyed on a string id rather than on an index, because the queue reorders.
 */

import { useEffect, useRef, useState } from "react";

/** How long a row takes to leave. Must match `--queue-leave` in queue.css. */
export const LEAVE_MS = 320;
/** Delay between one row and the next in a staggered group, in ms. */
export const STAGGER_MS = 45;

export interface Staged<T> {
  item: T;
  /** Stable identity, and the React key. */
  id: string;
  /** On its way out: still drawn, no longer in the list it came from. */
  leaving: boolean;
  /** Arrived on this pass, so it can be brought in rather than appearing. */
  entering: boolean;
  /**
   * Place in the group leaving or entering together, for the stagger.
   *
   * The staircase the design asks for is this number times [`STAGGER_MS`].
   * Counted within the group rather than taken from the row's index in the
   * list, so one row arriving at the end of a long queue is not held behind
   * nine delays it has nothing to do with.
   */
  rank: number;
}

/**
 * Track `items` through their arrivals and departures.
 *
 * Returns everything to draw, departed rows included, each still holding the
 * position it had, so the rows around one that is fading do not jump.
 */
export function useStaging<T>(
  items: T[],
  idOf: (item: T) => string,
): Staged<T>[] {
  const [, redraw] = useState(0);
  /*
   * Held in a ref and re-rendered by hand rather than kept in state.
   *
   * The alternative is an effect that writes derived state, which renders
   * twice for every queue update and — because effects run after paint — puts
   * one frame on screen with the new row already in its final position. That
   * frame is exactly what the animation exists to replace.
   */
  const shown = useRef<Staged<T>[]>([]);
  const timers = useRef(new Map<string, number>());

  const live = new Set(items.map(idOf));
  const before = shown.current;
  const known = new Set(before.filter((s) => !s.leaving).map((s) => s.id));

  let arriving = 0;
  const next: Staged<T>[] = items.map((item) => {
    const id = idOf(item);
    const entering = !known.has(id);
    return { item, id, leaving: false, entering, rank: entering ? arriving++ : 0 };
  });

  // Everything that has gone, put back where it was so nothing under it jumps.
  let departing = 0;
  before.forEach((was, at) => {
    if (live.has(was.id)) return;
    const row = was.leaving
      ? was
      : { ...was, leaving: true, entering: false, rank: departing++ };
    next.splice(Math.min(at, next.length), 0, row);
    if (!was.leaving) hold(row.id, row.rank);
  });

  /** Keep a departed row on screen for its animation, then drop it. */
  function hold(id: string, rank: number) {
    window.clearTimeout(timers.current.get(id));
    timers.current.set(
      id,
      window.setTimeout(() => {
        timers.current.delete(id);
        shown.current = shown.current.filter((s) => s.id !== id);
        redraw((n) => n + 1);
      }, LEAVE_MS + rank * STAGGER_MS),
    );
  }

  shown.current = next;

  // Nothing is left half-faded, and no timer fires into a screen that has gone.
  useEffect(() => {
    const pending = timers.current;
    return () => {
      pending.forEach((t) => window.clearTimeout(t));
      pending.clear();
    };
  }, []);

  return next;
}

/**
 * Press and hold, which is two gestures sharing an opening.
 *
 * Dylan's rule: hold and release in place asks about the thing; hold and drag
 * away moves it. One press arms both, and what happens next decides which it
 * was.
 *
 * That also settles the part of a hold that is otherwise hard on a scrolling
 * list. Movement *before* the hold arms is a scroll and cancels it; movement
 * *after* it arms is a drag. Neither has to be told from the other by distance
 * or direction, which is what a threshold would have had to do.
 *
 * Two more things a timer alone does not cover. A row already plays when
 * tapped, so the `click` that follows a hold has to be swallowed or reading
 * about a track plays it. And Android raises its own text-selection menu at
 * roughly the same delay — suppressed for touch and pen, left alone for a
 * mouse, where the context menu is the platform's business rather than ours.
 */
import { useCallback, useRef } from "react";
import type {
  PointerEvent as ReactPointerEvent,
  MouseEvent as ReactMouseEvent,
} from "react";

/** Long enough not to fire on a firm tap, short enough not to feel broken. */
const HOLD_MS = 450;

/** How far a finger may wander before it is a scroll, not a hold. */
const SLOP_PX = 10;

export function useLongPress(
  /** Held, then released without moving: tell me about this. */
  onHold: () => void,
  /** Held, then moved: pick it up. */
  onDragAway?: (x: number, y: number) => void,
) {
  const timer = useRef<number | null>(null);
  const from = useRef<{ x: number; y: number } | null>(null);
  /** Armed — the hold has fired and the press has not resolved yet. */
  const armed = useRef(false);
  /** A gesture that became a drag; its release is not "released in place". */
  const dragged = useRef(false);
  /**
   * The gesture just did something, so the `click` behind it is not a tap.
   *
   * Separate from `armed`, which `onPointerUp` has already cleared by the time
   * the click arrives — pointerup runs first, and a flag it resets cannot also
   * be the one the click reads.
   */
  const acted = useRef(false);
  const touch = useRef(false);

  const cancel = useCallback(() => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    from.current = null;
  }, []);

  /**
   * Watch for the drag on the window, not on the row.
   *
   * The movement that turns an armed hold into a drag can leave the row at
   * once — and a row is the last thing that can be relied on to still be under
   * the finger, because the table is virtualised and recycles them. Listening
   * here means the gesture survives leaving whatever started it.
   */
  const watchForDrag = useCallback(() => {
    const stop = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
    const onMove = (e: PointerEvent) => {
      if (dragged.current || !armed.current) return;
      dragged.current = true;
      acted.current = true;
      stop();
      onDragAway?.(e.clientX, e.clientY);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  }, [onDragAway]);

  const onPointerDown = useCallback(
    (e: ReactPointerEvent) => {
      if (e.button !== 0) return;
      armed.current = false;
      dragged.current = false;
      acted.current = false;
      touch.current = e.pointerType !== "mouse";
      from.current = { x: e.clientX, y: e.clientY };
      timer.current = window.setTimeout(() => {
        timer.current = null;
        armed.current = true;
        // The press is no longer a candidate scroll, so the start point goes
        // and the next move is read as a drag rather than as slop.
        from.current = null;
        // Touch and pen only.
        //
        // A row is `draggable`, and on a mouse the browser starts its own
        // HTML5 drag the moment the press moves — at which point it stops
        // sending pointer events altogether, so this one would begin and then
        // go deaf. Desktop keeps the native drag it already had; this is the
        // half that exists because touch has none.
        if (onDragAway && touch.current) watchForDrag();
      }, HOLD_MS);
    },
    [onDragAway, watchForDrag],
  );

  const onPointerMove = useCallback(
    (e: ReactPointerEvent) => {
      // Only the question of whether this press is really a scroll. Movement
      // after arming is the window listener's business.
      const start = from.current;
      if (!start) return;
      const dx = e.clientX - start.x;
      const dy = e.clientY - start.y;
      if (dx * dx + dy * dy > SLOP_PX * SLOP_PX) cancel();
    },
    [cancel],
  );

  const onPointerUp = useCallback(() => {
    cancel();
    if (armed.current && !dragged.current) {
      acted.current = true;
      onHold();
    }
    armed.current = false;
  }, [cancel, onHold]);

  const onPointerCancel = useCallback(() => {
    cancel();
    armed.current = false;
    dragged.current = false;
  }, [cancel]);

  const onContextMenu = useCallback((e: ReactMouseEvent) => {
    // Only the gesture we are replacing. A mouse keeps its context menu.
    if (touch.current) e.preventDefault();
  }, []);

  /**
   * Whether the click now arriving belongs to a gesture that already did
   * something. Reading it clears it, because a click either follows a hold or
   * it does not, and a flag left set would swallow the next genuine tap.
   */
  const swallowClick = useCallback(() => {
    if (!acted.current) return false;
    acted.current = false;
    return true;
  }, []);

  return {
    swallowClick,
    /** Give up on the press — a row is `draggable`, and starting a native drag
     *  is the same opening gesture. */
    cancel: onPointerCancel,
    handlers: {
      onPointerDown,
      onPointerMove,
      onPointerUp,
      onPointerCancel,
      onContextMenu,
    },
  };
}

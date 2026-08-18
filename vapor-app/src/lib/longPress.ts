/**
 * Press and hold.
 *
 * The narrow track row shows a title and nothing else, so artist, album, tempo
 * and key have to live somewhere a thumb can reach. Hold is that somewhere.
 *
 * Three things make this harder than a timer:
 *
 * 1. A row already plays on tap. The `click` that follows the hold would start
 *    the track as well as open the sheet, so a fired hold has to be able to
 *    swallow it — `swallowClick` reports that and clears itself.
 * 2. A scroll starts exactly like a hold. Moving past a few pixels cancels,
 *    or every flick through the library opens something.
 * 3. Android raises its own text-selection menu at roughly the same delay.
 *    That is suppressed for touch and pen, and left alone for a mouse, where
 *    the browser's context menu is the platform's business rather than ours.
 */
import { useCallback, useRef } from "react";
import type { PointerEvent as ReactPointerEvent, MouseEvent as ReactMouseEvent } from "react";

/** Long enough not to fire on a firm tap, short enough not to feel broken. */
const HOLD_MS = 450;

/** How far a finger may wander before it is a scroll, not a hold. */
const SLOP_PX = 10;

export function useLongPress(onLongPress: () => void) {
  const timer = useRef<number | null>(null);
  const from = useRef<{ x: number; y: number } | null>(null);
  const fired = useRef(false);
  const touch = useRef(false);

  const cancel = useCallback(() => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    from.current = null;
  }, []);

  const onPointerDown = useCallback(
    (e: ReactPointerEvent) => {
      // A right- or middle-click is not a hold.
      if (e.button !== 0) return;
      fired.current = false;
      touch.current = e.pointerType !== "mouse";
      from.current = { x: e.clientX, y: e.clientY };
      timer.current = window.setTimeout(() => {
        timer.current = null;
        fired.current = true;
        onLongPress();
      }, HOLD_MS);
    },
    [onLongPress],
  );

  const onPointerMove = useCallback(
    (e: ReactPointerEvent) => {
      const start = from.current;
      if (!start) return;
      const dx = e.clientX - start.x;
      const dy = e.clientY - start.y;
      if (dx * dx + dy * dy > SLOP_PX * SLOP_PX) cancel();
    },
    [cancel],
  );

  const onContextMenu = useCallback((e: ReactMouseEvent) => {
    // Only the gesture we are replacing. A mouse keeps its context menu.
    if (touch.current) e.preventDefault();
  }, []);

  /**
   * Whether the click now arriving belongs to a hold that already fired.
   *
   * Reading it clears it, because a click either follows a hold or it does
   * not, and a flag left set would swallow the next genuine tap.
   */
  const swallowClick = useCallback(() => {
    if (!fired.current) return false;
    fired.current = false;
    return true;
  }, []);

  return {
    swallowClick,
    /**
     * Give up on the press.
     *
     * Exposed because a row is `draggable`: pressing and holding to start a
     * drag is the same opening gesture, and without this the sheet opens in
     * the middle of dragging a track onto a playlist.
     */
    cancel,
    handlers: {
      onPointerDown,
      onPointerMove,
      onPointerUp: cancel,
      onPointerCancel: cancel,
      onContextMenu,
    },
  };
}

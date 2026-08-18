/**
 * The touch drag: the thing that follows the finger, and everything the
 * browser would have done for us on desktop.
 *
 * Held here rather than in each screen because a drag outlives the row it
 * started on — it ends over a tab at the other end of the window — and because
 * the rules live in `lib/drag.ts` where the desktop path can reach them too.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import * as drag from "../lib/drag";

interface DragApi {
  /** Begin carrying something. Called once the hold has armed and moved. */
  begin: (payload: drag.DragPayload, x: number, y: number) => void;
  dragging: boolean;
}

const Ctx = createContext<DragApi>({ begin: () => {}, dragging: false });

export function useDrag() {
  return useContext(Ctx);
}

export function DragLayer({
  children,
  onOpenMenu,
  onCloseMenu,
  openMenu,
  onResult,
}: {
  children: ReactNode;
  /** Hovering a tab for `DWELL_MS` asks for its list. */
  onOpenMenu: (which: "playlist" | "group") => void;
  onCloseMenu: () => void;
  /** Which list is open, so a dwell is not asked for twice. */
  openMenu: "playlist" | "group" | null;
  /** What the drop did, or why it would not. */
  onResult: (message: string, ok: boolean) => void;
}) {
  const [payload, setPayload] = useState<drag.DragPayload | null>(null);
  const [at, setAt] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const preview = useRef<HTMLDivElement>(null);

  /**
   * The tab a finger is resting on, and the timer that opens it.
   *
   * A timer rather than comparing timestamps across moves: a finger that is
   * resting produces no `pointermove` at all, so measuring the gap between
   * moves would mean the list only ever opened for someone who kept jiggling.
   */
  const dwell = useRef<{ tab: string; timer: number } | null>(null);
  const frame = useRef<number | null>(null);
  const marked = useRef<HTMLElement | null>(null);
  const openRef = useRef(openMenu);
  openRef.current = openMenu;

  /*
   * The callbacks, held still.
   *
   * They arrive as inline arrows, so their identity changes every render — and
   * every `pointermove` sets state and causes one. With them in the effect's
   * dependencies the listeners were torn down and rebuilt between moves, and
   * the teardown cancelled the dwell timer, so resting on a tab never opened
   * it. Refs keep the effect subscribed once per drag.
   */
  const openMenuRef = useRef(onOpenMenu);
  openMenuRef.current = onOpenMenu;
  const closeMenuRef = useRef(onCloseMenu);
  closeMenuRef.current = onCloseMenu;
  const resultRef = useRef(onResult);
  resultRef.current = onResult;

  const mark = useCallback((el: HTMLElement | null) => {
    if (marked.current === el) return;
    marked.current?.removeAttribute("data-over");
    if (el) el.setAttribute("data-over", "true");
    marked.current = el;
  }, []);

  /** Where the drag began, read by the effect that attaches a render later. */
  const startAt = useRef({ x: 0, y: 0 });

  const begin = useCallback((p: drag.DragPayload, x: number, y: number) => {
    startAt.current = { x, y };
    setPayload(p);
    setAt({ x, y });
  }, []);

  // The whole gesture lives on window: a finger that leaves the row it started
  // on must keep being followed, and pointer capture on a virtualised row is
  // capture on an element that may be recycled out from under it.
  useEffect(() => {
    if (!payload) return;

    const clearDwell = () => {
      if (dwell.current) clearTimeout(dwell.current.timer);
      dwell.current = null;
    };

    const stopScroll = () => {
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = null;
    };

    /**
     * Everything that depends on where the pointer is.
     *
     * Split out so it can be run once the moment the drag begins, not only on
     * the next move: the listener is attached a render *after* `begin`, so a
     * gesture that arrives at a tab and stops — which is exactly what resting
     * on one is — would otherwise never be seen at all.
     */
    const handleAt = (x: number, y: number) => {
      const target = drag.targetAt(x, y, preview.current);

      // Resting on a tab opens its list, once.
      if (target?.kind === "tab") {
        if (dwell.current?.tab !== target.tab) {
          clearDwell();
          const tab = target.tab;
          dwell.current = {
            tab,
            timer: window.setTimeout(() => {
              dwell.current = null;
              if (openRef.current !== tab) openMenuRef.current(tab);
            }, drag.DWELL_MS),
          };
        }
      } else {
        clearDwell();
      }

      mark(
        target?.kind === "item"
          ? (document.querySelector<HTMLElement>(
              `[data-drop-id="${CSS.escape(target.id)}"]`,
            ) ?? null)
          : null,
      );

      // Holding near the ends of an open list scrolls it, slowly, for as long
      // as the finger stays there.
      const list = document.querySelector<HTMLElement>("[data-tabmenu-scroll]");
      stopScroll();
      if (list) {
        const step = () => {
          if (drag.edgeScroll(list, y) !== 0) {
            frame.current = requestAnimationFrame(step);
          } else {
            frame.current = null;
          }
        };
        step();
      }
    };

    const onMove = (e: PointerEvent) => {
      e.preventDefault();
      setAt({ x: e.clientX, y: e.clientY });
      handleAt(e.clientX, e.clientY);
    };

    const onUp = (e: PointerEvent) => {
      stopScroll();
      mark(null);
      const target = drag.targetAt(e.clientX, e.clientY, preview.current);
      const held = payload;
      setPayload(null);
      clearDwell();

      if (target?.kind !== "item") {
        closeMenuRef.current();
        return;
      }
      void drag
        .dropOn(held, { menu: target.menu, id: target.id })
        .then((message) => {
          resultRef.current(message, true);
          closeMenuRef.current();
        })
        .catch((err: unknown) => {
          resultRef.current(
            err instanceof Error ? err.message : String(err),
            false,
          );
        });
    };

    const onCancel = () => {
      stopScroll();
      mark(null);
      setPayload(null);
      clearDwell();
    };

    // Where the drag started counts as the first position.
    handleAt(startAt.current.x, startAt.current.y);

    window.addEventListener("pointermove", onMove, { passive: false });
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      stopScroll();
      clearDwell();
      mark(null);
    };
  }, [payload, mark]);

  return (
    <Ctx.Provider value={{ begin, dragging: payload !== null }}>
      {children}
      {payload && (
        <div
          ref={preview}
          className="draglayer"
          style={{ left: at.x, top: at.y }}
          aria-hidden="true"
        >
          <span className="draglayer__label">{payload.label}</span>
          {payload.values.length > 1 && (
            <span className="draglayer__count">{payload.values.length}</span>
          )}
        </div>
      )}
    </Ctx.Provider>
  );
}

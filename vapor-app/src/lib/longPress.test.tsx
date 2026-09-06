/**
 * The touch drag stays alive once the hold has armed.
 *
 * Not a test of the gesture — that is the screens' business — but of the one
 * mechanism the gesture silently depended on and did not have. The drag
 * sources carry `touch-action: pan-y` so their lists can still be scrolled,
 * which hands every vertical movement to the browser's scroller; the scroller
 * then fires `pointercancel` and the drag that had just begun dies. Measured
 * in Chromium under touch emulation: a vertical drag survived three moves
 * before being cancelled, a horizontal one survived all twelve.
 *
 * jsdom has no `touch-action`, no scrolling and no compositor, so it cannot
 * reproduce that. What it can hold still is the thing that fixes it: a
 * non-passive `touchmove` listener, attached when the hold arms and not a
 * moment later. Both halves of that are load-bearing — a passive listener
 * cannot `preventDefault`, and one attached when the drag begins is already
 * too late — so both are asserted here rather than left to a comment.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { useLongPress } from "./longPress";

function Row({ onDragAway }: { onDragAway?: (x: number, y: number) => void }) {
  const hold = useLongPress(() => {}, onDragAway);
  return (
    <div data-testid="row" {...hold.handlers}>
      Row
    </div>
  );
}

/** What `document.addEventListener` was asked for, per gesture. */
let added: { type: string; passive: unknown }[];
let removed: string[];

beforeEach(() => {
  vi.useFakeTimers();
  added = [];
  removed = [];
  vi.spyOn(document, "addEventListener").mockImplementation(
    ((type: string, _fn: unknown, options: AddEventListenerOptions) => {
      added.push({ type, passive: options?.passive });
    }) as typeof document.addEventListener,
  );
  vi.spyOn(document, "removeEventListener").mockImplementation(
    ((type: string) => {
      removed.push(type);
    }) as typeof document.removeEventListener,
  );
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

/** A finger, not a mouse — the whole mechanism is touch-only. */
function press(el: HTMLElement, pointerType = "touch") {
  el.dispatchEvent(
    new PointerEvent("pointerdown", {
      bubbles: true,
      button: 0,
      pointerType,
      clientX: 10,
      clientY: 10,
    }),
  );
}

const HOLD_MS = 450;
const touchmoves = () => added.filter((a) => a.type === "touchmove");

describe("an armed hold", () => {
  it("blocks the scroll that would otherwise cancel the drag", () => {
    render(<Row onDragAway={() => {}} />);
    press(screen.getByTestId("row"));

    expect(touchmoves()).toHaveLength(0);
    vi.advanceTimersByTime(HOLD_MS);

    // Attached at arm time, before any movement. Attaching it once the drag
    // has begun — a render after the first move — is too late to stop the
    // scroll, which is the failure this exists to prevent.
    expect(touchmoves()).toHaveLength(1);
  });

  it("blocks it non-passively, or preventDefault would be ignored", () => {
    render(<Row onDragAway={() => {}} />);
    press(screen.getByTestId("row"));
    vi.advanceTimersByTime(HOLD_MS);

    // A `touchmove` listener on the document is passive by default, and a
    // passive listener cannot cancel the scroll at all.
    expect(touchmoves().map((a) => a.passive)).toEqual([false]);
  });

  it("lifts the block when the finger comes up", () => {
    render(<Row onDragAway={() => {}} />);
    const row = screen.getByTestId("row");
    press(row);
    vi.advanceTimersByTime(HOLD_MS);
    row.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));

    // Left attached, the list could never be scrolled again.
    expect(removed).toContain("touchmove");
  });

  it("lifts it when the gesture is cancelled instead", () => {
    render(<Row onDragAway={() => {}} />);
    const row = screen.getByTestId("row");
    press(row);
    vi.advanceTimersByTime(HOLD_MS);
    row.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true }));

    expect(removed).toContain("touchmove");
  });

  it("lifts it when the screen unmounts mid-gesture", () => {
    const view = render(<Row onDragAway={() => {}} />);
    press(screen.getByTestId("row"));
    vi.advanceTimersByTime(HOLD_MS);
    view.unmount();

    // Nothing would be left to lift it, and the document would stay unscrollable.
    expect(removed).toContain("touchmove");
  });
});

describe("a press that is not an armed touch drag", () => {
  it("leaves an ordinary scroll alone until the hold arms", () => {
    render(<Row onDragAway={() => {}} />);
    press(screen.getByTestId("row"));

    // The finger has not held long enough yet, so this is still a scroll and
    // must behave like one.
    vi.advanceTimersByTime(HOLD_MS - 1);
    expect(touchmoves()).toHaveLength(0);
  });

  it("blocks nothing for a mouse", () => {
    render(<Row onDragAway={() => {}} />);
    press(screen.getByTestId("row"), "mouse");
    vi.advanceTimersByTime(HOLD_MS);

    // Desktop keeps the native HTML5 drag, which is unaffected by any of this.
    expect(touchmoves()).toHaveLength(0);
  });

  it("blocks nothing where there is nothing to drag away", () => {
    render(<Row />);
    press(screen.getByTestId("row"));
    vi.advanceTimersByTime(HOLD_MS);

    expect(touchmoves()).toHaveLength(0);
  });
});

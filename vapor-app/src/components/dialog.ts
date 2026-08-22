/**
 * What `aria-modal="true"` promises, actually delivered.
 *
 * Four overlays in this app declared themselves modal — the help sheet, the
 * confirm dialog, the track sheet and the tab menu — and none of them held
 * focus. Tab walked straight out of the dialog and into the page behind it,
 * which is still rendered and still interactive. A sighted mouse user never
 * notices. Someone on a keyboard is tabbing through a library they cannot see
 * while a dialog they cannot leave sits on top, and a screen-reader user has
 * been told, by the attribute, that this cannot happen.
 *
 * There is no library for this here, and it is about thirty lines, so it is
 * thirty lines rather than a dependency.
 *
 * ## What it does not do
 *
 * It does not make the background `inert`. That is the more complete answer —
 * it takes the rest of the page out of the accessibility tree as well as out
 * of the tab order — but it needs a single wrapper around everything that is
 * *not* the overlay, and the overlays mount on `<body>` (see `overlay.ts`), so
 * there is no such element to point at yet. Trapping Tab is the part that
 * matters for someone actually using the app; `inert` would improve what a
 * screen reader is allowed to wander into.
 */
import { useEffect, type RefObject } from "react";

/**
 * Everything inside `root` that can take focus, in tab order.
 *
 * Hidden controls have to be excluded or Tab appears to stop dead on one. The
 * obvious test for that is `offsetParent === null`, and it is wrong here:
 * jsdom does no layout, so every element reports `null` and the filter removes
 * the entire dialog. The trap then had nothing to move to and blocked Tab
 * outright — which is how the component tests found it, having been written
 * expecting it to work.
 *
 * `checkVisibility` asks the engine the question directly and is what the
 * WebView actually has. Where it does not exist the attribute checks in the
 * selector are the whole answer, which is correct in jsdom because nothing
 * there is visually hidden in the first place.
 */
function focusable(root: HTMLElement): HTMLElement[] {
  const selector = [
    "a[href]",
    "button:not([disabled])",
    "input:not([disabled])",
    "select:not([disabled])",
    "textarea:not([disabled])",
    '[tabindex]:not([tabindex="-1"])',
  ].join(",");

  return Array.from(root.querySelectorAll<HTMLElement>(selector)).filter(
    (el) =>
      el.getAttribute("aria-hidden") !== "true" &&
      (typeof el.checkVisibility !== "function" || el.checkVisibility()),
  );
}

/**
 * Escape closes, Tab stays inside, and focus goes back where it came from.
 *
 * The caller still decides what gets focus *first* — Confirm deliberately lands
 * on Cancel rather than on the destructive button, and that judgement does not
 * belong in here.
 */
export function useDialog(
  container: RefObject<HTMLElement | null>,
  onClose: () => void,
): void {
  useEffect(() => {
    // Captured on open so it can be handed back on close. Without this, closing
    // a dialog drops focus to <body> and the next Tab starts from the top of
    // the document — the row you opened the sheet from is lost.
    const opener = document.activeElement as HTMLElement | null;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.key !== "Tab") return;

      const root = container.current;
      if (!root) return;

      const stops = focusable(root);
      if (stops.length === 0) {
        // Nothing to move to, so the only correct answer is to stay put.
        e.preventDefault();
        return;
      }

      const first = stops[0] as HTMLElement;
      const last = stops[stops.length - 1] as HTMLElement;
      const active = document.activeElement;

      // Also covers focus having escaped already — if it is not inside the
      // dialog at all, the next Tab brings it back rather than continuing
      // through the page.
      if (!root.contains(active)) {
        e.preventDefault();
        (e.shiftKey ? last : first).focus();
        return;
      }
      if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };

    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      // Only if focus is still somewhere the dialog left it. A close that
      // happened *because* the person clicked something else should not yank
      // them back.
      if (!document.activeElement || document.activeElement === document.body) {
        opener?.focus?.();
      }
    };
  }, [container, onClose]);
}

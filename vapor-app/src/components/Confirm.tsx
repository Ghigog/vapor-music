/**
 * "Are you sure?", drawn by the app rather than by the webview.
 *
 * ## Why `window.confirm` cannot be used here
 *
 * WKWebView does not implement the JavaScript dialogs on its own — the host
 * application has to supply a `WKUIDelegate` with
 * `runJavaScriptConfirmPanelWithMessage:`, and wry supplies none. Its Android
 * side does: `RustWebChromeClient.kt` overrides `onJsConfirm` and `onJsAlert`.
 *
 * So `window.confirm()` opened a real dialog on the phone and returned `false`
 * immediately on the desktop. Every caller was written as
 * `if (!window.confirm(…)) return;`, which turns "no dialog" into "the person
 * said no" — so on macOS, Forget a device, Delete a playlist folder and Delete
 * all data were buttons that did nothing at all, silently, with no error and
 * nothing in the console.
 *
 * The failure mode is the reason this is a component rather than a helper that
 * returns a promise: a dialog the app draws cannot be absent on one platform.
 */

import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import type { ReactNode } from "react";
import { overlayRoot } from "./overlay";

export function Confirm({
  title,
  body,
  confirmLabel = "Delete",
  destructive = true,
  onConfirm,
  onCancel,
}: {
  title: string;
  /** What is about to happen, and what will survive it. */
  body: ReactNode;
  confirmLabel?: string;
  /** Colours the confirming button. False for a decision that loses nothing. */
  destructive?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    // Focus lands on Cancel, not on the destructive button: a stray Return on
    // a dialog that just appeared should not be the thing that deletes.
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return createPortal(
    <div
      className="help__backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        className="confirm glass"
        role="alertdialog"
        aria-modal="true"
        aria-label={title}
      >
        <h2 className="confirm__title">{title}</h2>
        <div className="confirm__body">{body}</div>
        <div className="confirm__actions">
          <button
            ref={cancelRef}
            type="button"
            className="settings__button"
            onClick={onCancel}
          >
            Cancel
          </button>
          <button
            type="button"
            className={
              destructive
                ? "settings__button settings__button--danger"
                : "settings__button settings__button--primary"
            }
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>,
    overlayRoot(),
  );
}

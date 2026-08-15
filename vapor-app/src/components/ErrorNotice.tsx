/**
 * The error surface (TD-34).
 *
 * IPC failures used to render as a raw `String(e)` — which for a Tauri command
 * is the backend's own message wrapped in whatever `toString` produced, and for
 * anything else is `[object Object]`. Fine for a scaffold, wrong for a person.
 *
 * The backend already writes its errors to be read: "No password saved for this
 * server. Add it in Settings." So the job here is not to rewrite them, it is to
 * unwrap them, present them consistently, and keep the raw text reachable for
 * the cases where it is the only clue.
 */

import { useState } from "react";

/**
 * Pull a readable message out of whatever was thrown.
 *
 * Tauri rejects with the serialised error value, which for this app is a
 * string; a thrown `Error` carries `.message`; anything else is a shape nobody
 * planned for and gets stringified rather than swallowed.
 */
export function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object") {
    const record = error as Record<string, unknown>;
    for (const key of ["message", "error", "reason"]) {
      const value = record[key];
      if (typeof value === "string" && value) return value;
    }
    try {
      return JSON.stringify(error);
    } catch {
      // Circular or otherwise unserialisable: fall through to the generic.
    }
  }
  return "Something went wrong, and the reason did not survive the trip.";
}

export function ErrorNotice({
  error,
  onDismiss,
  onRetry,
}: {
  error: unknown;
  onDismiss?: (() => void) | undefined;
  onRetry?: (() => void) | undefined;
}) {
  const [showRaw, setShowRaw] = useState(false);
  const message = messageOf(error);
  const raw = typeof error === "string" ? "" : String(error);
  // Only worth offering when it says something the message does not.
  const hasRaw = raw !== "" && raw !== message;

  return (
    <div className="notice notice--error" role="alert">
      <div className="notice__body">
        <p className="notice__message">{message}</p>
        {showRaw && hasRaw && <pre className="notice__raw numeric">{raw}</pre>}
      </div>
      <div className="notice__actions">
        {hasRaw && (
          <button
            className="notice__button"
            onClick={() => setShowRaw((v) => !v)}
          >
            {showRaw ? "Less" : "Details"}
          </button>
        )}
        {onRetry && (
          <button className="notice__button" onClick={onRetry}>
            Try again
          </button>
        )}
        {onDismiss && (
          <button
            className="notice__button"
            onClick={onDismiss}
            aria-label="Dismiss"
          >
            ×
          </button>
        )}
      </div>
    </div>
  );
}

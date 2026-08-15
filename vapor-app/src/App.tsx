/**
 * App shell.
 *
 * Deliberately minimal: this proves the wiring — tokens, the generative mark,
 * and a live call into the Rust core — before any screen is built. If this
 * renders and reports a core version, the whole seam works.
 */
import { useEffect, useState } from "react";
import { VaporMark } from "./components/VaporMark";
import * as core from "./lib/core";

type Status =
  | { kind: "loading" }
  | { kind: "ready"; tracks: number }
  | { kind: "error"; message: string };

export function App() {
  const [status, setStatus] = useState<Status>({ kind: "loading" });

  useEffect(() => {
    // One real round trip to the core. An empty library is a valid answer —
    // the point is that the IPC boundary works, not that there is data.
    core
      .libraryView({})
      .then((sections) => {
        const tracks = sections.reduce((n, s) => n + s.rows.length, 0);
        setStatus({ kind: "ready", tracks });
      })
      .catch((e: unknown) => {
        setStatus({ kind: "error", message: String(e) });
      });
  }, []);

  return (
    <div className="boot">
      <VaporMark
        size={160}
        theme="light"
        state={status.kind === "loading" ? "thinking" : "idle"}
      />
      <div
        style={{
          fontFamily: "var(--font-display)",
          fontWeight: 200,
          fontSize: "var(--fs-hero)",
          letterSpacing: "var(--ls-hero)",
          lineHeight: 1,
        }}
      >
        Vapor Music
      </div>
      <div className="label">
        {status.kind === "loading" && "connecting to core"}
        {status.kind === "ready" && `core connected · ${status.tracks} tracks`}
        {status.kind === "error" && "core unreachable"}
      </div>
      {status.kind === "error" && (
        <div className="boot__error">{status.message}</div>
      )}
    </div>
  );
}

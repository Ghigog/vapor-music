/**
 * App shell.
 *
 * Deliberately minimal: this proves the wiring — tokens, the generative mark,
 * and a live call into the Rust core — before any screen is built. If this
 * renders and reports a core version, the whole seam works.
 */
import { useEffect, useState } from "react";
import { VaporMark } from "./components/VaporMark";
import { Transport } from "./components/Transport";
import { Library } from "./screens/Library";
import { Songs } from "./screens/Songs";
import * as core from "./lib/core";
import "./components/transport.css";
import "./screens/library.css";
import "./screens/songs.css";

type Status =
  | { kind: "loading" }
  | { kind: "ready"; tracks: number }
  | { kind: "error"; message: string };

export function App() {
  const [status, setStatus] = useState<Status>({ kind: "loading" });
  const [screen, setScreen] = useState<"library" | "songs">("library");

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

  // The boot screen exists only until the core answers. A person should not
  // see a splash for a round trip that takes single-digit milliseconds.
  if (status.kind === "ready") {
    return (
      <div className="shell">
        <nav className="shell__sidebar">
          {(["library", "songs"] as const).map((s) => (
            <button
              key={s}
              className={"nav__item" + (screen === s ? " nav__item--on" : "")}
              onClick={() => setScreen(s)}
            >
              {s === "library" ? "Library" : "Songs"}
            </button>
          ))}
        </nav>
        <main className="shell__content">
          {screen === "library" ? <Library /> : <Songs />}
        </main>
        {/* Lives outside the content column: the shell grid spans it across
            both, and playback outlives whichever screen started it. */}
        <Transport />
      </div>
    );
  }

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
        {status.kind === "loading" ? "connecting to core" : "core unreachable"}
      </div>
      {status.kind === "error" && (
        <div className="boot__error">{status.message}</div>
      )}
    </div>
  );
}

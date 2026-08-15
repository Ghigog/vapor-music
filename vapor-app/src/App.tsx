/**
 * App shell.
 *
 * Routing is a piece of state rather than a router: there are four screens and
 * no URLs to be back-buttoned through. When there are twelve, revisit.
 *
 * Onboarding is the exception to the layout. It takes the whole window with no
 * sidebar and no transport, because there is nothing to navigate to and nothing
 * to play — offering a disabled transport to someone who has not yet connected
 * anything is clutter posing as consistency.
 */
import { useEffect, useState } from "react";
import { VaporMark } from "./components/VaporMark";
import { Transport } from "./components/Transport";
import { Library } from "./screens/Library";
import { Songs } from "./screens/Songs";
import { Settings } from "./screens/Settings";
import { NowPlaying } from "./screens/NowPlaying";
import { Onboarding } from "./screens/Onboarding";
import * as core from "./lib/core";
import "./components/transport.css";
import "./screens/library.css";
import "./screens/songs.css";
import "./screens/settings.css";
import "./screens/nowplaying.css";
import "./screens/onboarding.css";

type Screen = "library" | "songs" | "playing" | "settings";

const SCREENS: { id: Screen; label: string }[] = [
  { id: "library", label: "Library" },
  { id: "songs", label: "Songs" },
  { id: "playing", label: "Now Playing" },
  { id: "settings", label: "Settings" },
];

type Status =
  | { kind: "loading" }
  | { kind: "onboarding" }
  | { kind: "ready" }
  | { kind: "error"; message: string };

export function App() {
  const [status, setStatus] = useState<Status>({ kind: "loading" });
  const [screen, setScreen] = useState<Screen>("library");

  useEffect(() => {
    // Settings is the round trip worth making at boot: it decides where to
    // land, and it answers whether the core is reachable at all.
    core
      .settings()
      .then((s) => {
        const connected =
          s.remote.url.trim() !== "" && s.remote.username.trim() !== "";
        setStatus({ kind: connected ? "ready" : "onboarding" });
      })
      .catch((e: unknown) => {
        setStatus({ kind: "error", message: String(e) });
      });
  }, []);

  if (status.kind === "onboarding") {
    return (
      <div className="shell shell--bare">
        <main className="shell__content">
          <Onboarding
            onConnect={() => {
              setScreen("settings");
              setStatus({ kind: "ready" });
            }}
          />
        </main>
      </div>
    );
  }

  if (status.kind === "ready") {
    return (
      <div className="shell">
        <nav className="shell__sidebar">
          {SCREENS.map((s) => (
            <button
              key={s.id}
              className={"nav__item" + (screen === s.id ? " nav__item--on" : "")}
              onClick={() => setScreen(s.id)}
            >
              {s.label}
            </button>
          ))}
        </nav>
        <main className="shell__content">
          {screen === "library" && <Library />}
          {screen === "songs" && <Songs />}
          {screen === "playing" && <NowPlaying />}
          {screen === "settings" && <Settings />}
        </main>
        {/* Outside the content column: the shell grid spans it across both, and
            playback outlives whichever screen started it. */}
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

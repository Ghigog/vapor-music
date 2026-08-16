/**
 * App shell.
 *
 * Routing is a piece of state rather than a router. There are no URLs to be
 * back-buttoned through, and the one place a real router would earn its keep —
 * drilling into a track and coming back — is handled by remembering which
 * screen asked. When there is a second level of drill-down, revisit.
 *
 * Onboarding is the exception to the layout: it takes the whole window with no
 * sidebar and no transport, because there is nothing to navigate to and nothing
 * to play, and a disabled transport on a first run is clutter posing as
 * consistency.
 */
import { useEffect, useState } from "react";
import { VaporMark } from "./components/VaporMark";
import { Transport } from "./components/Transport";
import { Library } from "./screens/Library";
import { Playlist } from "./screens/Playlist";
import { PlaylistRail } from "./components/PlaylistRail";
import { Songs } from "./screens/Songs";
import { Search } from "./screens/Search";
import { Queue } from "./screens/Queue";
import { Vibe } from "./screens/Vibe";
import { NowPlaying } from "./screens/NowPlaying";
import { LinerNotes } from "./screens/LinerNotes";
import { YourData } from "./screens/YourData";
import { Settings } from "./screens/Settings";
import { Onboarding } from "./screens/Onboarding";
import * as core from "./lib/core";
import "./components/transport.css";
import "./components/states.css";
import "./components/notice.css";
import "./screens/library.css";
import "./screens/songs.css";
import "./screens/search.css";
import "./screens/queue.css";
import "./screens/vibe.css";
import "./screens/settings.css";
import "./screens/nowplaying.css";
import "./screens/onboarding.css";
import "./screens/liner.css";
import "./screens/yourdata.css";
import "./screens/playlist.css";

type Screen =
  | "library"
  | "songs"
  | "search"
  | "playing"
  | "queue"
  | "vibe"
  | "data"
  | "settings";

/** Grouped the way a person moves through the app: find something, then hear
 *  it, then the things about the app itself. */
const NAV: { id: Screen; label: string; group: number }[] = [
  { id: "library", label: "Library", group: 0 },
  { id: "songs", label: "Songs", group: 0 },
  { id: "search", label: "Search", group: 0 },
  { id: "playing", label: "Now Playing", group: 1 },
  { id: "queue", label: "Queue", group: 1 },
  { id: "vibe", label: "Vibe DJ", group: 1 },
  { id: "data", label: "Your Data", group: 2 },
  { id: "settings", label: "Settings", group: 2 },
];

type Status =
  | { kind: "loading" }
  | { kind: "onboarding" }
  | { kind: "ready" }
  | { kind: "error"; message: string };

export function App() {
  const [status, setStatus] = useState<Status>({ kind: "loading" });
  const [screen, setScreen] = useState<Screen>("library");
  /** The track being read about, and the screen to return to. Liner Notes is a
   *  drill-down rather than a destination, so it is not in the nav. */
  const [liner, setLiner] = useState<{ href: string; from: Screen } | null>(
    null,
  );
  /** The playlist being viewed. Like Liner Notes it is a drill-down rather than
   *  a nav destination, so it lives beside `screen` rather than in it — but it
   *  is reachable from the rail on every screen, so it has no "from". */
  const [playlist, setPlaylist] = useState<string | null>(null);

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

  function openLiner(href: string) {
    setLiner({ href, from: screen });
  }

  function openPlaylist(id: string) {
    setLiner(null);
    setPlaylist(id);
  }

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
          {NAV.map((item, i) => (
            <div key={item.id}>
              {/* A rule between groups rather than headings: three labels over
                  eight items is more furniture than navigation. */}
              {i > 0 && NAV[i - 1]?.group !== item.group && (
                <div className="nav__rule" />
              )}
              <button
                className={
                  "nav__item" +
                  (screen === item.id && !liner && !playlist
                    ? " nav__item--on"
                    : "")
                }
                onClick={() => {
                  setLiner(null);
                  setPlaylist(null);
                  setScreen(item.id);
                }}
              >
                {item.label}
              </button>
            </div>
          ))}

          {/* Always on screen, because it is the drop target for tracks
              dragged out of the Songs table on another screen (TD-31). */}
          <PlaylistRail activeId={playlist} onOpen={openPlaylist} />
        </nav>

        <main className="shell__content">
          {liner ? (
            <LinerNotes
              href={liner.href}
              onBack={() => {
                setScreen(liner.from);
                setLiner(null);
              }}
            />
          ) : playlist ? (
            <Playlist
              id={playlist}
              onOpen={openLiner}
              onGone={() => setPlaylist(null)}
            />
          ) : (
            <>
              {screen === "library" && <Library />}
              {screen === "songs" && <Songs onOpen={openLiner} />}
              {screen === "search" && <Search onOpen={openLiner} />}
              {screen === "playing" && <NowPlaying />}
              {screen === "queue" && <Queue onOpen={openLiner} />}
              {screen === "vibe" && <Vibe />}
              {screen === "data" && <YourData />}
              {screen === "settings" && <Settings />}
            </>
          )}
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

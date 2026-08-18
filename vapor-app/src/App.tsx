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
import { useEffect, useRef, useState } from "react";
import { VaporMark } from "./components/VaporMark";
import { Boundary } from "./components/Boundary";
import { Transport } from "./components/Transport";
import { Library, type Opened } from "./screens/Library";
import { Playlist } from "./screens/Playlist";
import { PlaylistRail } from "./components/PlaylistRail";
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
import "./components/help.css";
import "./components/tracksheet.css";
import "./screens/library.css";
import "./screens/songs.css";
import "./screens/queue.css";
import "./screens/vibe.css";
import "./screens/settings.css";
import "./screens/nowplaying.css";
import "./screens/onboarding.css";
import "./screens/liner.css";
import "./screens/yourdata.css";
import "./screens/playlist.css";

/*
 * Destinations, not mockups.
 *
 * Songs and Search were sidebar entries because the Daylight file ships twelve
 * screen *drawings* and the rewrite built one nav item per drawing — see
 * docs/FINDINGS.md. The design's own navigation is three tabs, and its
 * Library carries a search field and a Songs tab. Both now live inside Library.
 */
type Screen =
  | "library"
  | "playing"
  | "queue"
  | "vibe"
  | "data"
  | "settings";

/**
 * Where you can go.
 *
 * Your Data is not here any more — it is a section at the bottom of Settings,
 * which is where someone asking "what does this keep, and where" is already
 * standing. Settings is not here either: it is a control in the corner of every
 * screen, because it is somewhere you visit and leave rather than a place the
 * app is.
 *
 * Grouped the way a person moves through the app: find something, then hear it.
 */
const NAV: { id: Screen; label: string; group: number }[] = [
  { id: "library", label: "Library", group: 0 },
  { id: "vibe", label: "Vibe DJ", group: 1 },
];

/**
 * What a destination is called right now.
 *
 * Vibe DJ is "Shuffle" with the DJ switched off, because that is what it does
 * then. Shared so the sidebar and the mobile tab bar cannot drift apart on it.
 */
function navLabel(item: (typeof NAV)[number], djMode: boolean): string {
  return item.id === "vibe" && !djMode ? "Shuffle" : item.label;
}

type Status =
  | { kind: "loading" }
  | { kind: "onboarding" }
  | { kind: "ready" }
  | { kind: "error"; message: string };

/** What to call the thing on screen, for an error message a person can read. */
function screenName(screen: Screen, liner: boolean, playlist: boolean): string {
  if (liner) return "Liner Notes";
  if (playlist) return "That playlist";
  switch (screen) {
    case "library":
      return "The Library";
    case "playing":
      return "Now Playing";
    case "vibe":
      return "The Vibe DJ screen";
    case "data":
      return "Your Data";
    case "settings":
      return "Settings";
    default:
      // Unreachable while `Screen` is exhaustive, and a name rather than a
      // crash if it ever stops being.
      return "This screen";
  }
}

export function App() {
  /** Data files that could not be read at startup, as sentences to show.
   *  Empty on every normal launch — see `core.startupProblems`. */
  const [damaged, setDamaged] = useState<string[]>([]);
  const [damagedSeen, setDamagedSeen] = useState(false);

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
  /** The album or artist opened inside Library. Held here, not in Library, so
   *  it lands in the history entry below and the back gesture can leave it. */
  const [opened, setOpened] = useState<Opened | null>(null);
  /**
   * Whether the DJ is conducting the queue.
   *
   * Held here rather than in Settings because it decides what the Vibe tab is
   * called, and the design relabels that tab "Shuffle" when the DJ is off.
   * Not persisted yet — it resets to conducting on launch.
   */
  /** Read from settings, not invented here. The backend is the half that
   *  decides what plays next, so it has to be the one that knows. */
  const [djMode, setDjMode] = useState(true);

  /**
   * Where the app is, in the webview's own history.
   *
   * Navigation is component state, which on Android meant the back gesture had
   * nothing to walk: it went straight past the app to the launcher, so Settings
   * was a screen you left by killing the app and starting it again.
   *
   * Two halves fix it. `MainActivity` re-enables wry's `handleBackNavigation`,
   * which Tauri turns off — back then calls `webView.goBack()` when there is
   * anywhere to go and finishes the activity when there is not. And this pushes
   * an entry per place, so there is somewhere to go. Desktop gets the same
   * thing for free: a mouse's back button, and Cmd-[.
   */
  const fromPop = useRef(false);
  const firstPlace = useRef(true);
  useEffect(() => {
    const place = { screen, liner: liner?.href ?? null, playlist, opened };
    if (firstPlace.current) {
      firstPlace.current = false;
      window.history.replaceState(place, "");
      return;
    }
    // A change that *came from* the back gesture must not push a new entry, or
    // going back would land on itself and the gesture would never leave.
    if (fromPop.current) {
      fromPop.current = false;
      return;
    }
    window.history.pushState(place, "");
    // `opened` by value rather than by reference: it is rebuilt on every
    // render that changes it, and a reference dep would push an entry for a
    // re-render that went nowhere.
  }, [screen, liner?.href, playlist, opened?.kind, opened?.name]);

  useEffect(() => {
    const onPop = (event: PopStateEvent) => {
      const place = event.state as {
        screen: Screen;
        liner: string | null;
        playlist: string | null;
        opened: Opened | null;
      } | null;
      if (!place) return;
      fromPop.current = true;
      setScreen(place.screen);
      setPlaylist(place.playlist);
      setOpened(place.opened ?? null);
      // The screen to return to is taken as the one being restored: the
      // original "from" is not in the entry, and going back from Liner Notes
      // should land where the history says, not where it was opened from.
      setLiner(place.liner ? { href: place.liner, from: place.screen } : null);
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);

  useEffect(() => {
    // Settings is the round trip worth making at boot: it decides where to
    // land, and it answers whether the core is reachable at all.
    core
      .settings()
      .then((s) => {
        const connected =
          s.remote.url.trim() !== "" && s.remote.username.trim() !== "";
        setDjMode(s.djMode ?? true);
        setStatus({ kind: connected ? "ready" : "onboarding" });
      })
      .catch((e: unknown) => {
        setStatus({ kind: "error", message: String(e) });
      });
  }, []);

  /**
   * Go to a destination, from either navigation.
   *
   * Leaving a drill-down open behind a destination change is how you land on
   * Library and get Liner Notes: the drill-downs render in front of `screen`,
   * so both have to be cleared for the tab you pressed to be the thing you see.
   */
  function go(id: Screen) {
    setLiner(null);
    setPlaylist(null);
    setOpened(null);
    setScreen(id);
  }

  function openLiner(href: string) {
    setLiner({ href, from: screen });
  }

  function openPlaylist(id: string) {
    setLiner(null);
    setPlaylist(id);
  }

  // Asked once, at startup, because that is when it is decided. A person who
  // dismisses this has been told; it is not repeated at them every render.
  useEffect(() => {
    core
      .startupProblems()
      .then(setDamaged)
      // If even this call fails the app has bigger problems, and a banner
      // about not being able to check for problems helps nobody.
      .catch(() => setDamaged([]));
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
        {damaged.length > 0 && !damagedSeen && (
          <div className="damaged" role="alert">
            <div className="damaged__body">
              <strong>Some of your data could not be read.</strong>
              <ul>
                {damaged.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            </div>
            <button
              className="damaged__dismiss"
              onClick={() => setDamagedSeen(true)}
            >
              Dismiss
            </button>
          </div>
        )}
        <nav className="shell__sidebar" aria-label="Screens and playlists">
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
                onClick={() => go(item.id)}
              >
                {navLabel(item, djMode)}
              </button>
            </div>
          ))}

          {/* Always on screen, because it is the drop target for tracks
              dragged out of the Songs table on another screen (TD-31). */}
          <PlaylistRail activeId={playlist} onOpen={openPlaylist} />
        </nav>

        <main className="shell__content">
          {/* One boundary around the screen area, named for whatever is in it.
              A screen that throws now says so and leaves the sidebar, the
              transport and every other screen reachable — where before it
              blanked the window and named nothing. */}
          <Boundary where={screenName(screen, liner !== null, playlist !== null)}>
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
              {screen === "library" && (
                <Library
                  onOpen={openLiner}
                  onOpenPlaylist={openPlaylist}
                  opened={opened}
                  onOpenedChange={setOpened}
                />
              )}
              {screen === "playing" && <NowPlaying />}
              {screen === "vibe" && (
                <Vibe
                  djMode={djMode}
                  onDjModeChange={(on) => {
                    // Optimistic, then persisted: the switch should feel
                    // immediate, and the backend is what acts on it.
                    setDjMode(on);
                    void core.setDjMode(on).catch(() => setDjMode(!on));
                  }}
                  onOpen={openLiner}
                />
              )}
              {screen === "data" && <YourData />}
              {screen === "settings" && <Settings />}
            </>
          )}
          </Boundary>
        </main>

        {/* Outside the content column: the shell grid spans it across both, and
            playback outlives whichever screen started it. */}
        <Transport onOpenNowPlaying={() => go("playing")} />

        {/*
          Settings, from anywhere.
          
          It stopped being a destination: it is somewhere you visit and leave,
          not a place the app is, and it was taking a quarter of a four-item tab
          bar to say otherwise. Outside `.shell__content` so it does not scroll
          away with whatever screen is under it, and it pushes a history entry
          like any other place — the back gesture has to leave it.
        */}
        <button
          className="shell__settings"
          aria-label="Settings"
          title="Settings"
          aria-current={screen === "settings" ? "page" : undefined}
          onClick={() => go("settings")}
        >
          <span className="icon icon--settings" aria-hidden="true" />
        </button>

        {/* The same destinations as the sidebar, for the widths that hide it.
            After the transport in the DOM because that is the order they are in
            on screen, which is the order to tab and to read them in. */}
        <nav className="shell__tabs" aria-label="Screens">
          {NAV.map((item) => (
            <button
              key={item.id}
              className={
                "shell__tab" +
                (screen === item.id && !liner && !playlist
                  ? " shell__tab--on"
                  : "")
              }
              aria-current={
                screen === item.id && !liner && !playlist ? "page" : undefined
              }
              onClick={() => go(item.id)}
            >
              {navLabel(item, djMode)}
            </button>
          ))}
        </nav>
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

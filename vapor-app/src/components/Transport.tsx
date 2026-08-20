/**
 * The transport bar (TD-03).
 *
 * The shell's grid has always reserved a `player` row for this; until now there
 * was nothing to put in it, because there was no audio path.
 *
 * ## Why it polls
 *
 * Position moves continuously and is owned by the audio thread, which publishes
 * it to an atomic. Emitting an event per block would be thousands of IPC
 * messages a second to redraw one timecode. Polling four times a second costs a
 * lock and a handful of atomic reads, and is what every native player does.
 *
 * Track changes are a different matter: those arrive as a `playback-changed`
 * event so the bar updates the instant the supervisor advances the queue,
 * rather than up to a poll interval later.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as core from "../lib/core";

/** Four times a second: fast enough that the timecode never visibly jumps a
 *  second, slow enough to be free. */
const POLL_MS = 250;

export function Transport({
  onOpenNowPlaying,
  djMode = false,
  onDjModeChange,
}: {
  /** Opens the full Now Playing screen. Omitted, the title is plain text. */
  onOpenNowPlaying?: (() => void) | undefined;
  /**
   * Whether the DJ is conducting.
   *
   * This is where it is switched, because it is a playback control and not a
   * screen setting: it changes what the player does with the queue, so it
   * belongs beside the buttons that do the same. It was a checkbox on the
   * Vibe screen, which meant the one thing that turns the app's headline
   * feature on lived somewhere you had to already be.
   */
  djMode?: boolean;
  onDjModeChange?: ((on: boolean) => void) | undefined;
} = {}) {
  const [state, setState] = useState<core.PlaybackState | null>(null);
  /** Set while a seek is being dragged, so incoming polls do not yank the
   *  handle back to where playback currently is. */
  const [scrubbing, setScrubbing] = useState<number | null>(null);
  const busy = useRef(false);

  const refresh = useCallback(async () => {
    // One request in flight at a time. A slow round trip must not queue up a
    // backlog that then all land at once.
    if (busy.current) return;
    busy.current = true;
    try {
      setState(await core.playbackState());
    } catch {
      // A failed poll is not worth surfacing — the next one is 250 ms away.
    } finally {
      busy.current = false;
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), POLL_MS);
    const unlisten = listen("playback-changed", () => void refresh());
    return () => {
      clearInterval(timer);
      void unlisten.then((f) => f());
    };
  }, [refresh]);

  if (!state) return null;

  const { status, loading, duration, error, available } = state;
  const position = scrubbing ?? state.position;
  const playing = status === "playing";
  // Nothing is loaded, so the transport has nothing to act on. Stop stays
  // available while loading so a slow download can be abandoned.
  const idle = status === "idle" && !loading;

  async function act(fn: () => Promise<unknown>) {
    try {
      await fn();
    } finally {
      await refresh();
    }
  }

  return (
    <div className="shell__player glass transport">
      <div className="transport__now">
        {/*
          The way to Now Playing.

          It was a sidebar entry, which the Daylight design never had — its Now
          Playing opens with a "⌄" dismiss chevron, the control of a sheet
          pushed up from this bar (docs/FINDINGS.md). Pressing the title of
          the thing you are listening to is how every other player opens it.

          Only when there is something to open: with nothing playing this is the
          words "Nothing playing", and a button that goes to an empty screen is
          a worse answer than no button.
        */}
        {onOpenNowPlaying && state.title ? (
          <button
            className="transport__title transport__title--open"
            title={state.title}
            onClick={onOpenNowPlaying}
            aria-label={`Now playing: ${state.title}. Open the full screen.`}
          >
            {state.title}
          </button>
        ) : (
          <div className="transport__title" title={state.title}>
            {state.title || "Nothing playing"}
          </div>
        )}
        <div className="transport__artist" title={state.artist}>
          {/* Mixing outranks the artist name here: it is the one moment the
              app is doing what it exists to do, and it lasts a few seconds. */}
          {state.mixing ? (
            <span className="transport__mixing">mixing</span>
          ) : loading ? (
            "loading…"
          ) : (
            state.artist || "—"
          )}
        </div>
      </div>

      {/* The glyphs are images, not characters.
          They were `⏮ ⏸ ▶ ⏭` — and a codepoint is not an icon. The platform
          picks the font, so on Android those render as full-colour emoji: the
          stop control came out as an orange square that matched nothing else on
          screen. These are the icons the Godot build used, drawn at one weight
          by one person, and they are masks rather than `<img>` so each one takes
          the colour of the control it sits in. */}
      <div className="transport__controls">
        {/* The mark, as a switch. A toggle rather than a checkbox: it has two
            states and the icon already says which one, so a tick beside it
            would be a second control for the same fact. `aria-pressed` is
            what carries that to a screen reader. */}
        {onDjModeChange && (
          <button
            className={
              "transport__button transport__dj" +
              (djMode ? " transport__dj--on" : "")
            }
            onClick={() => onDjModeChange(!djMode)}
            aria-pressed={djMode}
            aria-label={djMode ? "Turn Vibe DJ off" : "Turn Vibe DJ on"}
            title={djMode ? "Vibe DJ is conducting" : "Vibe DJ is off"}
          >
            <span className="icon icon--vibe" aria-hidden="true" />
          </button>
        )}
        <button
          className="transport__button"
          onClick={() => void act(core.previousTrack)}
          disabled={!available}
          aria-label="Previous track"
        >
          {/* The set has no "previous": it is the same drawing, mirrored, which
              is what the two controls are. */}
          <span className="icon icon--next icon--flip" aria-hidden="true" />
        </button>
        <button
          className="transport__button transport__button--primary"
          onClick={() =>
            void act(playing ? core.pausePlayback : core.resumePlayback)
          }
          disabled={!available || idle}
          aria-label={playing ? "Pause" : "Play"}
        >
          <span
            className={"icon " + (playing ? "icon--pause" : "icon--play")}
            aria-hidden="true"
          />
        </button>
        <button
          className="transport__button"
          onClick={() => void act(core.nextTrack)}
          disabled={!available}
          aria-label="Next track"
        >
          <span className="icon icon--next" aria-hidden="true" />
        </button>
      </div>

      <div className="transport__scrub">
        <span className="numeric transport__time">{timecode(position)}</span>
        <input
          className="transport__range"
          type="range"
          min={0}
          max={Math.max(duration, 0.01)}
          step={0.1}
          value={position}
          disabled={!available || duration <= 0}
          aria-label="Position"
          onChange={(e) => setScrubbing(Number(e.target.value))}
          // The seek is sent on release, not per pixel of drag: each one resets
          // the stretcher, and a hundred of them while dragging would be heard
          // as a stutter.
          onPointerUp={() => {
            if (scrubbing !== null) void act(() => core.seek(scrubbing));
            setScrubbing(null);
          }}
          onKeyUp={() => {
            if (scrubbing !== null) void act(() => core.seek(scrubbing));
            setScrubbing(null);
          }}
        />
        <span className="numeric transport__time">{timecode(duration)}</span>
      </div>

      <input
        className="transport__volume"
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={state.volume}
        disabled={!available}
        aria-label="Volume"
        onChange={(e) => {
          const volume = Number(e.target.value);
          setState((s) => (s ? { ...s, volume } : s));
          // Sent per change rather than on release: volume is an atomic the
          // audio thread ramps across a block, so a moving slider is cheap and
          // has to be heard while it moves.
          void core.setVolume(volume);
        }}
      />

      {(error || !available) && (
        <div className="transport__error numeric">
          {available ? error : "No audio output device"}
        </div>
      )}
    </div>
  );
}

/** m:ss. Unknown is a dash, never 0:00 — the same rule the table follows. */
function timecode(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "–:––";
  const total = Math.floor(seconds);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

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

export function Transport() {
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
        <div className="transport__title" title={state.title}>
          {state.title || "Nothing playing"}
        </div>
        <div className="transport__artist" title={state.artist}>
          {loading ? "loading…" : state.artist || "—"}
        </div>
      </div>

      <div className="transport__controls">
        <button
          className="transport__button"
          onClick={() => void act(core.previousTrack)}
          disabled={!available}
          aria-label="Previous track"
        >
          ⏮
        </button>
        <button
          className="transport__button transport__button--primary"
          onClick={() =>
            void act(playing ? core.pausePlayback : core.resumePlayback)
          }
          disabled={!available || idle}
          aria-label={playing ? "Pause" : "Play"}
        >
          {playing ? "⏸" : "▶"}
        </button>
        <button
          className="transport__button"
          onClick={() => void act(core.stopPlayback)}
          disabled={!available || idle}
          aria-label="Stop"
        >
          ⏹
        </button>
        <button
          className="transport__button"
          onClick={() => void act(core.nextTrack)}
          disabled={!available}
          aria-label="Next track"
        >
          ⏭
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

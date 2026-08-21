/**
 * Now Playing.
 *
 * The transport bar says *what* is playing; this says what it *is* — the
 * waveform, where you are in it, and what the DJ intends to do next.
 *
 * ## The mark is a readout here
 *
 * The design is explicit that the logo's states map onto real engine state, and
 * until now nothing drove them. Here `state` follows the mixer (`blending`
 * while a transition is armed or running, `playing` otherwise) and `energy`
 * follows the actual output level, published per block by the audio thread and
 * decayed so it tracks the music rather than the buffer size. When the mark
 * swirls, something is genuinely mixing.
 *
 * ## The waveform is real
 *
 * Envelope peaks come from analysis, not from a random seed — the same decoded
 * signal the tempo and key came from. A track analysed before the waveform
 * existed has none, and the bar falls back to a plain scrubber rather than
 * drawing something invented.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { VaporMark, type MarkState } from "../components/VaporMark";
import * as core from "../lib/core";
import { useThumb } from "../lib/artwork";
import { LyricsPanel } from "../components/LyricsPanel";

const POLL_MS = 250;

export function NowPlaying() {
  const [state, setState] = useState<core.PlaybackState | null>(null);
  const busy = useRef(false);

  const refresh = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    try {
      setState(await core.playbackState());
    } catch {
      // The next poll is 250 ms away; a transient failure is not worth a
      // banner on the screen a person is watching.
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

  // Before the early return: a hook cannot be called conditionally, and the
  // empty state below returns before the tile is rendered.
  const nextArt = useThumb(state?.nextHref ?? "");

  if (!state) return null;

  const { title, artist, duration, position, waveform, mixing, level } = state;
  const playing = state.status === "playing";
  const nothing = !state.href && !state.loading;

  if (nothing) {
    return (
      <div className="np np--empty">
        <VaporMark size={120} state="idle" />
        <p className="np__empty-title">Nothing playing</p>
        <p className="np__empty-body">
          Pick something from Songs, and it will appear here.
        </p>
      </div>
    );
  }

  const markState: MarkState = mixing ? "blending" : playing ? "playing" : "idle";
  const progress = duration > 0 ? position / duration : 0;

  return (
    <div className="np">
      <div className="np__art" aria-hidden="true">
        {state.cover ? (
          // The file's own sleeve. Everything else on this screen is derived;
          // this is the one thing the recording itself supplied.
          <img className="np__art-image" src={state.cover} alt="" />
        ) : (
          <>
            <div className="np__art-sheen" />
            <div className="np__art-band" />
          </>
        )}
      </div>

      <div className="np__meta">
        <div className="np__names">
          <h1 className="np__title" title={title}>
            {state.loading ? "Loading…" : title || "—"}
          </h1>
          <p className="np__artist">{artist || "—"}</p>
          <p className="np__source">
            <span className="np__dot" aria-hidden="true" />
            <span className="label">on this device</span>
          </p>
        </div>
      </div>

      <div className="np__wave-block">
        {waveform.length > 0 ? (
          <button
            type="button"
            className="np__wave"
            aria-label="Seek"
            onClick={(e) => {
              // Click-to-seek across the waveform. The bar is the scrubber;
              // a separate slider under it would be two controls for one job.
              const box = e.currentTarget.getBoundingClientRect();
              const ratio = (e.clientX - box.left) / box.width;
              void core.seek(Math.max(0, Math.min(1, ratio)) * duration);
            }}
          >
            {waveform.map((peak, i) => (
              <span
                key={i}
                className={
                  "np__bar" +
                  (i / waveform.length <= progress ? " np__bar--past" : "")
                }
                // A floor so a silent passage is still a visible bar rather
                // than a gap in the waveform.
                style={{ height: `${Math.max(peak, 0.04) * 100}%` }}
              />
            ))}
          </button>
        ) : (
          <div className="np__wave np__wave--plain">
            <div
              className="np__wave-fill"
              style={{ width: `${progress * 100}%` }}
            />
          </div>
        )}

        <div className="np__times numeric">
          <span>{timecode(position)}</span>
          <span>−{timecode(Math.max(duration - position, 0))}</span>
        </div>
      </div>

      {/*
        Icons, not codepoints — the same fault the transport had in 7c7a9ac and
        the same fix. `⏮ ⏸ ▶ ⏭` let the platform choose the font, and Android
        chooses a full-colour emoji font, so this screen's controls arrived as
        coloured glyphs beside the line-art ones on the bar below it.
      */}
      <div className="np__controls">
        <button
          className="np__button"
          onClick={() => void core.previousTrack().then(refresh)}
          aria-label="Previous track"
        >
          <span className="icon icon--next icon--flip" aria-hidden="true" />
        </button>
        <button
          className="np__button np__button--play"
          onClick={() =>
            void (playing ? core.pausePlayback() : core.resumePlayback()).then(
              refresh,
            )
          }
          aria-label={playing ? "Pause" : "Play"}
        >
          <span
            className={"icon " + (playing ? "icon--pause" : "icon--play")}
            aria-hidden="true"
          />
        </button>
        <button
          className="np__button"
          onClick={() => void core.nextTrack().then(refresh)}
          aria-label="Next track"
        >
          <span className="icon icon--next" aria-hidden="true" />
        </button>
      </div>

      {/* Up next. The design puts the mark in this card precisely because this
          is where blending is announced. */}
      <div className="np__next glass">
        {/* The record, not the app's logo.
            This drew a `VaporMark` — the same mark as the header, at 42px —
            which told you the app was running rather than what was coming. A
            sleeve is the thing anyone recognises a track by. The mark stays as
            the fallback, since a queue can hold a track whose file carries no
            artwork. */}
        <span className="np__next-art" aria-hidden="true">
          {nextArt ? (
            <img src={nextArt} alt="" />
          ) : (
            <VaporMark size={42} state={markState} energy={level} />
          )}
        </span>
        <div className="np__next-text">
          <span className="label">
            {mixing ? "blending · crossfade" : "up next"}
          </span>
          <span className="np__next-title">
            {state.nextTitle || "Nothing queued"}
          </span>
          {/* Artist and album, on one line and only where they are known —
              a dash for each would be two dashes under every title. */}
          {(state.nextArtist || state.nextAlbum) && (
            <span className="np__next-sub">
              {[state.nextArtist, state.nextAlbum].filter(Boolean).join(" · ")}
            </span>
          )}
        </div>
      </div>

      {/* Below the tile, as asked. The playhead is passed in rather than the
          panel keeping its own clock: seeks, pauses and crossfades all move
          the position, and only the engine knows where it really is. */}
      <LyricsPanel href={state.href ?? ""} position={position} />
    </div>
  );
}

/** m:ss. Unknown is a dash, never 0:00. */
function timecode(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0:00";
  const total = Math.floor(seconds);
  return `${Math.floor(total / 60)}:${(total % 60).toString().padStart(2, "0")}`;
}

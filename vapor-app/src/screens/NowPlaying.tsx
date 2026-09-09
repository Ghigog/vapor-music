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
import { artistWithGenre, UNKNOWN_GENRE } from "../lib/genre";
import { LyricsPanel } from "../components/LyricsPanel";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import { Stat, clock } from "./LinerNotes";
import type { Opened } from "./Library";

const POLL_MS = 250;

export function NowPlaying({
  djMode = false,
  onOpen,
  onOpenEntity,
}: {
  /** Whether the DJ is conducting — the gate on the "where next" picker
   *  below the transport, same as the one on the Vibe screen. */
  djMode?: boolean;
  /** Opens a track's liner notes — the title's press. */
  onOpen?: ((href: string) => void) | undefined;
  /** Opens the artist or album view in the library. */
  onOpenEntity?: ((opened: Opened) => void) | undefined;
} = {}) {
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

  /**
   * The album, and the same analysis figures Liner Notes shows.
   *
   * `PlaybackState` carries no album — it is polled four times a second and a
   * track's fuller record is not — so this is one read per track, keyed on
   * its href rather than on the poll.
   */
  const [details, setDetails] = useState<core.TrackDetails | null>(null);
  useEffect(() => {
    let cancelled = false;
    setDetails(null);
    if (!state?.href) return;
    core
      .trackDetails(state.href)
      .then((d) => {
        if (!cancelled) setDetails(d);
      })
      .catch(() => {
        if (!cancelled) setDetails(null);
      });
    return () => {
      cancelled = true;
    };
  }, [state?.href]);

  /**
   * The three ways out of the playing track, and what picking one costs.
   *
   * Only polled while the DJ is on: with it off there is no plan to steer,
   * `mixCandidates` answers empty, and asking four times a second for nothing
   * is a request this screen does not need to make.
   */
  const [candidates, setCandidates] = useState<core.MixCandidate[]>([]);
  const [blend, setBlend] = useState<core.BlendPreview | null>(null);
  const [curve, setCurve] = useState<core.Curve>("build");
  const [mixError, setMixError] = useState<string | null>(null);

  useEffect(() => {
    if (!djMode) return;
    // The curve is the backend's; read once rather than assumed, same as Vibe.
    core
      .settings()
      .then((s) => setCurve(core.asCurve(s.curve)))
      .catch(() => {});
  }, [djMode]);

  const refreshMix = useCallback(async () => {
    const [b, c] = await Promise.allSettled([core.blendPreview(), core.mixCandidates()]);
    if (b.status === "fulfilled") setBlend(b.value);
    if (c.status === "fulfilled") setCandidates(c.value);
  }, []);

  useEffect(() => {
    if (!djMode) {
      setCandidates([]);
      setBlend(null);
      return;
    }
    void refreshMix();
    const timer = setInterval(() => void refreshMix(), 1000);
    const unlisten = listen("playback-changed", () => void refreshMix());
    return () => {
      clearInterval(timer);
      void unlisten.then((f) => f());
    };
  }, [djMode, refreshMix]);

  async function pickExit(candidate: core.MixCandidate) {
    setMixError(null);
    try {
      await core.chooseNext(candidate.href, curve);
      await refreshMix();
    } catch (e: unknown) {
      setMixError(messageOf(e));
    }
  }

  if (!state) return null;

  const { title, artist, href, duration, position, waveform, mixing, level } = state;
  const playing = state.status === "playing";
  const nothing = !href && !state.loading;

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
            {/* A press, not just a heading — the same track's liner notes
                are one tap away rather than a trip through Songs to find the
                row again. Not while still loading: the title is a placeholder
                word then, not this track's. */}
            {href && !state.loading ? (
              <button
                type="button"
                className="liner__entity-link"
                onClick={() => onOpen?.(href)}
              >
                {title || "—"}
              </button>
            ) : (
              (state.loading ? "Loading…" : title || "—")
            )}
          </h1>
          {/* Genre beside the artist, not among the analysis figures: it is
              resolved per artist far more often than per track, so that is
              where a wrong one is recognisable. The artist is a press into
              their library page; the genre is not — it is corrected there,
              not typed here. */}
          <p className="np__artist">
            {artist ? (
              <button
                type="button"
                className="liner__entity-link"
                onClick={() =>
                  onOpenEntity?.({
                    kind: "artist",
                    name: artist,
                    lead: href ?? "",
                    artist: "",
                  })
                }
              >
                {artist}
              </button>
            ) : (
              "—"
            )}
            {` - ${state.genre || UNKNOWN_GENRE}`}
          </p>
          {details && details.album && (
            <p className="np__album">
              <button
                type="button"
                className="liner__entity-link"
                onClick={() =>
                  onOpenEntity?.({
                    kind: "album",
                    name: details.album,
                    lead: href ?? "",
                    artist,
                  })
                }
              >
                {details.album}
              </button>
            </p>
          )}
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

      {/* The same figures Liner Notes shows for this track, worked out on this
          device from the audio itself. Repeated here rather than left a tap
          away: this is the screen open while the track is actually playing,
          which is when the numbers mean the most. */}
      {details?.analysed && (
        <section className="liner__card glass">
          <h2 className="label">what the analysis heard</h2>
          <dl className="liner__stats">
            <Stat k="tempo" v={`${Math.round(details.bpm)} BPM`} />
            <Stat k="key" v={details.key || "—"} />
            <Stat k="loudness" v={`${details.lufs.toFixed(1)} LUFS`} />
            <Stat k="energy" v={`${Math.round(details.energy * 100)}%`} />
            <Stat k="starts" v={clock(details.cueIn)} />
            <Stat k="ends" v={clock(details.cueOut)} />
          </dl>
        </section>
      )}

      {/* Where the set is going, right where the decision matters — the same
          three exits Vibe offers, so choosing does not mean leaving this
          screen. Gated on the DJ actually conducting: with it off there is no
          plan to steer and nothing here to press. */}
      {djMode && (
        <section className="liner__card glass">
          <h2 className="label">where next</h2>
          {candidates.length === 0 ? (
            <p className="vibe__note">Nothing analysed to choose from yet.</p>
          ) : (
            <ul className="vibe__exits">
              {candidates.map((c) => (
                <li key={c.href}>
                  <button
                    className={
                      "vibe__exit vibe__exit--" +
                      c.exit +
                      (c.selected ? " vibe__exit--on" : "")
                    }
                    aria-pressed={c.selected}
                    onClick={() => void pickExit(c)}
                  >
                    <span className="vibe__exit-top">
                      <span className="vibe__exit-art" aria-hidden="true">
                        {c.cover && <img src={c.cover} alt="" />}
                      </span>
                      <span className="vibe__exit-word">{c.label}</span>
                    </span>
                    <span className="vibe__exit-title">{c.title}</span>
                    <span className="vibe__exit-artist">
                      {artistWithGenre(c.artist, c.genre)}
                    </span>
                    <span className="vibe__exit-facts numeric">
                      <span>{c.bpm > 0 ? Math.round(c.bpm) : "—"}</span>
                      <span className="vibe__dot">·</span>
                      <span>{c.key || "—"}</span>
                      <span className="vibe__dot">·</span>
                      <span>{c.transition}</span>
                    </span>
                    {c.selected && blend && (
                      <span
                        className={
                          "vibe__exit-blend numeric" +
                          (blend.matchable ? "" : " vibe__exit-warn")
                        }
                      >
                        {blend.matchable
                          ? `${blend.shiftPercent >= 0 ? "+" : ""}${blend.shiftPercent.toFixed(1)}% to beat match`
                          : "no beat match"}
                      </span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}
          <ErrorNotice error={mixError} onDismiss={() => setMixError(null)} />
        </section>
      )}

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
          {/* Artist, genre and album on one line.
              Artist and album still appear only where they are known — a dash
              for each would be two dashes under every title. Genre is stated
              either way, and that is the difference: it is here to answer
              whether the app knows what the next record is, and a blank would
              read as "yes". Keyed on there being a next track at all, so
              "Nothing queued" does not grow a genre of its own. */}
          {state.nextTitle && (
            <span className="np__next-sub">
              {[
                artistWithGenre(state.nextArtist, state.nextGenre),
                state.nextAlbum,
              ]
                .filter(Boolean)
                .join(" · ")}
            </span>
          )}
        </div>
      </div>

      {/* Below the tile, as asked. The playhead is passed in rather than the
          panel keeping its own clock: seeks, pauses and crossfades all move
          the position, and only the engine knows where it really is. */}
      <LyricsPanel href={href ?? ""} position={position} />
    </div>
  );
}

/** m:ss. Unknown is a dash, never 0:00. */
function timecode(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0:00";
  const total = Math.floor(seconds);
  return `${Math.floor(total / 60)}:${(total % 60).toString().padStart(2, "0")}`;
}

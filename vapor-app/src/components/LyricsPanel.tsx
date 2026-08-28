/**
 * Lyrics on Now Playing, following the playhead.
 *
 * The words themselves already existed — `look_up_track` fetches them from
 * LRCLIB and Liner Notes lists them with their timings. What it deliberately
 * did not do is follow along: its own comment says "following the playhead is a
 * separate job for Now Playing, which is the screen that has one". This is that
 * screen.
 *
 * ## Where the time comes from
 *
 * The playhead, passed in, rather than a clock started when the panel mounted.
 * A local timer drifts from the audio the moment anything seeks, pauses or
 * mixes — and this app crossfades between tracks, so "the position" is a real
 * question with a real answer that only the engine has.
 *
 * ## What it will not do
 *
 * Guess. Unsynced words are shown as prose with nothing highlighted, because
 * pretending to follow along when the timings are absent is worse than not
 * following: it puts the wrong line under your eye at the moment you look.
 */

import { useEffect, useRef, useState } from "react";

import * as core from "../lib/core";

/** What the panel knows about the track it is showing. */
type Load =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; looked: core.LookedUp }
  | { kind: "failed" };

/**
 * The line being sung at `position`.
 *
 * The last line whose timestamp has passed, which is what a lyric line means:
 * it stays on screen until the next one starts. `-1` before the first.
 */
export function activeLine(lines: core.LyricLine[], position: number): number {
  let at = -1;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line && line.time <= position) at = i;
    else break;
  }
  return at;
}

export function LyricsPanel({
  href,
  position,
}: {
  href: string;
  position: number;
}) {
  const [load, setLoad] = useState<Load>({ kind: "idle" });
  const activeRef = useRef<HTMLLIElement | null>(null);

  useEffect(() => {
    if (!href) {
      setLoad({ kind: "idle" });
      return;
    }
    let cancelled = false;
    setLoad({ kind: "loading" });

    /*
     * Ask what is already known first, then fetch only if there is a reason to.
     *
     * `track_lookup` never touches the network and always answers — including
     * whether the setting permits a lookup at all. `look_up_track` *refuses*
     * when it does not, and a refusal arrives here as a thrown error
     * indistinguishable from the service being unreachable. Asking first is
     * what lets "you have this switched off" and "LRCLIB did not answer" be
     * two different sentences.
     *
     * It also means a track already looked up costs no request, however often
     * this screen is opened.
     */
    core
      .trackLookup(href)
      .then((known) => {
        if (cancelled) return;
        if (!known.allowed || known.attempted) {
          setLoad({ kind: "ready", looked: known });
          return;
        }
        return core.lookUpTrack(href).then((looked) => {
          if (!cancelled) setLoad({ kind: "ready", looked });
        });
      })
      .catch(() => {
        if (!cancelled) setLoad({ kind: "failed" });
      });
    return () => {
      cancelled = true;
    };
  }, [href]);

  const lyrics = load.kind === "ready" ? load.looked.lyrics : null;
  const synced = lyrics?.synced ? lyrics.lines : null;
  const at = synced ? activeLine(synced, position) : -1;

  // Keep the sung line in view. `nearest` rather than `center` so a line near
  // the end of the song does not scroll the panel past its own content.
  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [at]);

  if (load.kind === "idle" || load.kind === "loading") return null;

  if (load.kind === "failed") {
    return (
      <section className="lyrics">
        <h2 className="label">lyrics</h2>
        <p className="lyrics__note">Could not reach the lyrics service.</p>
      </section>
    );
  }

  const { looked } = load;

  // Lookups are switched off, which is the shipped default. Say so, and say
  // where — a blank panel reads as "this track has no words".
  if (!looked.allowed) {
    return (
      <section className="lyrics">
        <h2 className="label">lyrics</h2>
        <p className="lyrics__note">
          Looking up lyrics is turned off. Settings has the switch.
        </p>
      </section>
    );
  }

  if (!lyrics) {
    return (
      <section className="lyrics">
        <h2 className="label">lyrics</h2>
        <p className="lyrics__note">
          {looked.attempted
            ? "No words for this one."
            : "Nothing looked up yet."}
        </p>
      </section>
    );
  }

  return (
    <section className="lyrics">
      <h2 className="label">lyrics</h2>
      {synced ? (
        <ol className="lyrics__lines">
          {synced.map((line, i) => (
            <li
              key={i}
              ref={i === at ? activeRef : null}
              className={
                "lyrics__line" +
                (i === at ? " lyrics__line--now" : "") +
                (i < at ? " lyrics__line--past" : "")
              }
            >
              {/* An empty line is an instrumental break, written deliberately
                  by whoever transcribed it — not a hole in the data. */}
              {line.text || <span className="lyrics__rest">♪</span>}
            </li>
          ))}
        </ol>
      ) : (
        <p className="lyrics__plain">{lyrics.plain}</p>
      )}
      <p className="lyrics__note">
        From LRCLIB.{synced ? "" : " Not timed."}
      </p>
    </section>
  );
}

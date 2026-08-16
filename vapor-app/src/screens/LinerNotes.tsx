/**
 * Liner Notes — everything known about one track.
 *
 * The design shows written notes and credits. Neither exists in a file: nothing
 * writes them, so rather than mock them up this shows what the app genuinely
 * knows — which after analysis is a great deal more than a tag would carry.
 *
 * ## Two kinds of knowledge, kept apart
 *
 * Everything above the lyrics panel was worked out on this device from the
 * audio itself, and the screen says so. The lyrics were not: they come from
 * LRCLIB, which means the artist and title were sent to a server the person
 * has no relationship with. The Godot build did that unconditionally and said
 * nothing about it; here it is off until asked, and what it returns is drawn
 * in its own panel that names where it came from.
 *
 * That separation is the point. A screen that mixed the two would make the
 * sovereignty claim on the rest of it untrue.
 */

import { useCallback, useEffect, useState } from "react";
import * as core from "../lib/core";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import { Loading } from "../components/States";

export function LinerNotes({
  href,
  onBack,
}: {
  href: string;
  onBack: () => void;
}) {
  const [track, setTrack] = useState<core.TrackDetails | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [looked, setLooked] = useState<core.LookedUp | null>(null);
  const [looking, setLooking] = useState(false);
  const [lookupError, setLookupError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setTrack(null);
    setError(null);
    core
      .trackDetails(href)
      .then((t) => {
        if (!cancelled) setTrack(t);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(messageOf(e));
      });
    return () => {
      cancelled = true;
    };
  }, [href]);

  // Read-only, and separate from the fetch: opening this screen must never be
  // the thing that sends a request.
  useEffect(() => {
    let cancelled = false;
    setLooked(null);
    setLookupError(null);
    core
      .trackLookup(href)
      .then((l) => {
        if (!cancelled) setLooked(l);
      })
      .catch(() => {
        if (!cancelled) setLooked(null);
      });
    return () => {
      cancelled = true;
    };
  }, [href]);

  const lookUp = useCallback(async () => {
    setLooking(true);
    setLookupError(null);
    try {
      setLooked(await core.lookUpTrack(href));
    } catch (e: unknown) {
      setLookupError(messageOf(e));
    } finally {
      setLooking(false);
    }
  }, [href]);

  if (error) {
    return (
      <div className="liner">
        <button className="liner__back" onClick={onBack}>
          ‹ Back
        </button>
        <ErrorNotice error={error} />
      </div>
    );
  }

  if (!track) return <Loading label="Reading…" />;

  return (
    <div className="liner">
      <button className="liner__back" onClick={onBack}>
        ‹ Back
      </button>

      <header className="liner__head">
        <div className="liner__art" aria-hidden="true">
          {track.cover && (
            <img className="liner__art-image" src={track.cover} alt="" />
          )}
        </div>
        <div className="liner__names">
          <h1 className="liner__title">{track.title}</h1>
          <p className="liner__artist">{track.artist || "—"}</p>
          <p className="liner__album">
            {[track.album || "—", track.year > 0 ? String(track.year) : null]
              .filter(Boolean)
              .join(" · ")}
          </p>
          <p className="liner__where">
            <span
              className={"liner__dot" + (track.cached ? "" : " liner__dot--remote")}
              aria-hidden="true"
            />
            <span className="label">
              {track.cached ? "on this device" : "on your server"}
            </span>
          </p>
        </div>
      </header>

      {track.waveform.length > 0 && (
        <div className="liner__wave" aria-hidden="true">
          {track.waveform.map((peak, i) => (
            <span
              key={i}
              className="liner__bar"
              style={{ height: `${Math.max(peak, 0.04) * 100}%` }}
            />
          ))}
        </div>
      )}

      {track.unplayable && (
        <section className="liner__card liner__card--warn">
          <h2 className="label">cannot be played</h2>
          <p className="liner__note">
            {track.unplayable}. Vapor read this file and could not use it —
            ffmpeg tolerates some files Symphonia will not. Re-encoding it on
            the server is the fix.
          </p>
        </section>
      )}

      {track.analysed ? (
        <>
          <section className="liner__card glass">
            <h2 className="label">what the analysis heard</h2>
            <dl className="liner__stats">
              <Stat
                k="tempo"
                v={`${Math.round(track.bpm)} BPM`}
                note={track.bpmIsManual ? "corrected by hand" : undefined}
              />
              <Stat k="key" v={track.key || "—"} />
              <Stat k="length" v={clock(track.duration)} />
              <Stat k="loudness" v={`${track.lufs.toFixed(1)} LUFS`} />
              <Stat k="energy" v={`${Math.round(track.energy * 100)}%`} />
              <Stat k="beats" v={track.beats.toLocaleString()} />
              <Stat k="starts" v={clock(track.cueIn)} />
              <Stat k="ends" v={clock(track.cueOut)} />
            </dl>
            <p className="liner__note">
              Worked out on this device from the audio itself — no lookup, no
              fingerprint sent anywhere. The cue points are what the DJ schedules
              a mix from.
            </p>
          </section>
        </>
      ) : (
        <section className="liner__card glass">
          <h2 className="label">not analysed yet</h2>
          <p className="liner__note">
            Tempo, key and cue points appear here once this track has been
            listened to. Run an analysis pass from Settings.
          </p>
        </section>
      )}

      <section className="liner__card glass">
        <h2 className="label">lyrics</h2>
        {lookupError && <ErrorNotice error={lookupError} />}

        {looked?.lyrics ? (
          <>
            {looked.lyrics.synced ? (
              /* Timed lines are shown as a list with their timings, because a
                 person reading them wants to know the words *and* that the
                 file is aligned. Following the playhead is a separate job for
                 Now Playing, which is the screen that has one. */
              <ol className="liner__lyrics">
                {looked.lyrics.lines.map((line, i) => (
                  <li key={i} className="liner__lyric">
                    <span className="liner__lyric-at numeric">
                      {clock(line.time)}
                    </span>
                    {/* An empty line is an instrumental break, not a gap in
                        the data — LRCLIB writes one deliberately. */}
                    <span className="liner__lyric-text">
                      {line.text || <span className="liner__rest">♪</span>}
                    </span>
                  </li>
                ))}
              </ol>
            ) : (
              <p className="liner__prose liner__prose--lyrics">
                {looked.lyrics.plain}
              </p>
            )}
            <p className="liner__note">
              From LRCLIB, not from this file or this device.
              {looked.lyrics.synced
                ? " Timed to the recording by whoever transcribed them, so the alignment is theirs, not ours."
                : " No timed version was available, so these are the words without the timings."}
            </p>
          </>
        ) : looked?.attempted ? (
          <p className="liner__note">
            LRCLIB has no words for this track.{" "}
            <button className="liner__link" onClick={() => void lookUp()}>
              Ask again
            </button>
            {" — a transcription may have been added since."}
          </p>
        ) : looked?.allowed ? (
          <>
            <p className="liner__note">
              Not looked up yet. This is the one thing on this screen that
              leaves the device: the artist and title go to LRCLIB and Deezer,
              and nothing else does.
            </p>
            <button
              className="liner__lookup"
              disabled={looking}
              onClick={() => void lookUp()}
            >
              {looking ? "Looking…" : "Look up lyrics and artwork"}
            </button>
          </>
        ) : (
          <p className="liner__note">
            Lyrics come from LRCLIB, which means sending this track's artist and
            title to a server that is not yours. That is switched off, and
            everything else on this screen was worked out here without it. Turn
            it on in Settings if you want the words.
          </p>
        )}
      </section>

      {track.notes && (
        <section className="liner__card glass">
          <h2 className="label">notes</h2>
          {/* The file's own comment field. Not the design's commissioned prose
              — that would have to be written by someone — but it is what a
              recording actually carries, and some releases carry a lot. */}
          <p className="liner__prose">{track.notes}</p>
        </section>
      )}

      <section className="liner__card glass">
        <h2 className="label">where it lives</h2>
        <p className="liner__path numeric">{track.hrefPath}</p>
        <p className="liner__note">
          {/* The design's "credits" become this: the honest provenance of every
              field above. */}
          {track.tagged
            ? "Read from the file's own tags where it had them, and from its path where it did not — so a badly named, untagged file is still a badly named track."
            : "Read from the file's path. Its tags are read when the track is analysed, so this fills in once it has been listened to."}
        </p>
      </section>
    </div>
  );
}

function Stat({
  k,
  v,
  note,
}: {
  k: string;
  v: string;
  note?: string | undefined;
}) {
  return (
    <div className="liner__stat">
      <dt className="label">{k}</dt>
      <dd className="numeric">
        {v}
        {note && <span className="liner__stat-note">{note}</span>}
      </dd>
    </div>
  );
}

function clock(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "—";
  const total = Math.round(seconds);
  return `${Math.floor(total / 60)}:${(total % 60).toString().padStart(2, "0")}`;
}

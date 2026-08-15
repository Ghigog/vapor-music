/**
 * Liner Notes — everything known about one track.
 *
 * The design shows written notes and credits. Neither exists: nothing reads
 * embedded tags yet, and there is no place a person could type them. Rather
 * than mock them up, this shows what the app genuinely knows — which after
 * analysis is a great deal more than a tag would carry, and all of it derived
 * here rather than fetched from anyone.
 *
 * That is the honest version of the same screen, and it is the one the
 * sovereignty claim is actually about.
 */

import { useEffect, useState } from "react";
import * as core from "../lib/core";
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
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [href]);

  if (error) {
    return (
      <div className="liner">
        <button className="liner__back" onClick={onBack}>
          ‹ Back
        </button>
        <p className="liner__error">{error}</p>
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
        <div className="liner__art" aria-hidden="true" />
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
        <h2 className="label">where it lives</h2>
        <p className="liner__path numeric">{track.hrefPath}</p>
        <p className="liner__note">
          {/* The design's "credits" become this: the honest provenance of every
              field above. */}
          Title, artist and album are read from the file's path — nothing reads
          embedded tags yet, so a badly named file is a badly named track.
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

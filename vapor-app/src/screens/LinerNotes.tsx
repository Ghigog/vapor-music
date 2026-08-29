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

/**
 * Correct one field of a track by hand.
 *
 * Everything the app knows about a track's identity is derived — from the path,
 * from the file's tags, from what a service said — and each of those is wrong
 * somewhere in every real library. Deezer files this owner's entire drum & bass
 * collection under "Dance"; a compilation track carries the album it was first
 * released on; a folder is misnamed. There has to be a last word, and it has to
 * belong to the person who can actually hear the record.
 *
 * Committed on blur or Enter, not per keystroke — each commit is a settings
 * write and a library re-read, and doing that on every letter would rebuild the
 * index eleven times for "drum and bass". Escape abandons the edit.
 */
function Correction({
  label,
  field,
  href,
  value,
  manual,
  onSaved,
}: {
  label: string;
  field: "genre" | "album" | "artist";
  href: string;
  value: string;
  /** True when the current value is one the owner typed, not one we derived. */
  manual: boolean;
  onSaved: () => void;
}) {
  const [draft, setDraft] = useState(value);
  const [error, setError] = useState<string | null>(null);

  // The row underneath can change while this is open — a lookup lands, another
  // screen corrects the same track — and the box should follow it unless it is
  // being typed in. Keying on the committed value rather than tracking focus:
  // a fresh `value` means something outside settled it.
  useEffect(() => {
    setDraft(value);
  }, [value]);

  async function commit(next: string) {
    if (next.trim() === value.trim()) return;
    try {
      await core.setTrackOverride(href, field, next);
      setError(null);
      onSaved();
    } catch (e: unknown) {
      // Put the box back to the truth rather than leaving a value on screen
      // that was not stored — a correction that looks saved and is not is
      // worse than one that visibly failed.
      setDraft(value);
      setError(messageOf(e));
    }
  }

  return (
    <label className="liner__fix">
      <span className="liner__fix-label label">
        {label}
        {manual && (
          <span className="liner__fix-badge" title="You set this by hand">
            yours
          </span>
        )}
      </span>
      <input
        className="liner__fix-input"
        type="text"
        value={draft}
        placeholder="—"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => void commit(draft)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            e.currentTarget.blur();
          } else if (e.key === "Escape") {
            e.preventDefault();
            setDraft(value);
          }
        }}
      />
      {manual && (
        <button
          type="button"
          className="liner__fix-reset"
          onClick={() => void commit("")}
        >
          Use what was found
        </button>
      )}
      {error && <ErrorNotice error={error} onDismiss={() => setError(null)} />}
    </label>
  );
}

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
  /**
   * Bumped when a correction is stored, to re-read the sheet.
   *
   * A correction changes what the *derived* fields resolve to, not just the one
   * that was typed: setting an artist can change the genre, because the genre
   * falls back to the artist's tag cloud. So the answer is re-read rather than
   * patched in place — the backend owns that resolution and this screen must
   * not keep a second copy of the rules.
   */
  const [saved, setSaved] = useState(0);

  useEffect(() => {
    let cancelled = false;
    // Not `setTrack(null)` on a re-read: blanking the sheet to a spinner every
    // time a field is committed would make the screen flash on each edit. The
    // first load still spins, because `track` starts null.
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
  }, [href, saved]);

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

  /**
   * The looked-up pictures, as data URIs.
   *
   * Fetched by the backend into a file and read back here, rather than pointed
   * at with a remote `src`: the window's CSP allows `data:` and no remote
   * host, so the page never opens a connection to Deezer of its own.
   */
  const [pictures, setPictures] = useState<{ album?: string; artist?: string }>(
    {},
  );

  useEffect(() => {
    let cancelled = false;
    setPictures({});
    const wanted = [
      ["album", looked?.albumArt] as const,
      ["artist", looked?.artistImage] as const,
    ].filter(([, url]) => !!url);
    if (wanted.length === 0) return;

    void Promise.all(
      wanted.map(async ([which, url]) => {
        const data = await core.lookedUpImage(url!).catch(() => null);
        return [which, data] as const;
      }),
    ).then((found) => {
      if (cancelled) return;
      setPictures(
        Object.fromEntries(found.filter(([, data]) => !!data)) as {
          album?: string;
          artist?: string;
        },
      );
    });
    return () => {
      cancelled = true;
    };
  }, [looked?.albumArt, looked?.artistImage]);

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
        {/* The file's own artwork first. A looked-up sleeve fills the gap
            when the file carries none, and is marked so, because a picture
            from Deezer standing in for one that came out of the recording is
            a difference worth being able to see. */}
        <div className="liner__art" aria-hidden="true">
          {(track.cover || pictures.album) && (
            <img
              className="liner__art-image"
              src={track.cover ?? pictures.album}
              alt=""
            />
          )}
          {!track.cover && pictures.album && (
            <span className="liner__art-source label">looked up</span>
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
          {/* What the app thinks this is, and the chance to say otherwise.
              Under the names rather than in a settings screen somewhere: the
              moment a person notices a genre is wrong is the moment they are
              looking at it. */}
          <div className="liner__fixes">
            <Correction
              label="Artist"
              field="artist"
              href={href}
              value={track.artist}
              manual={track.artistIsManual}
              onSaved={() => setSaved((n) => n + 1)}
            />
            <Correction
              label="Album"
              field="album"
              href={href}
              value={track.album}
              manual={track.albumIsManual}
              onSaved={() => setSaved((n) => n + 1)}
            />
            <Correction
              label="Genre"
              field="genre"
              href={href}
              value={track.genre}
              manual={track.genreIsManual}
              onSaved={() => setSaved((n) => n + 1)}
            />
          </div>

          <p className="liner__where">
            <span
              className={
                "liner__dot" + (track.cached ? "" : " liner__dot--remote")
              }
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
              fingerprint sent anywhere. The cue points are what the DJ
              schedules a mix from.
            </p>
          </section>
        </>
      ) : (
        <section className="liner__card glass">
          <h2 className="label">not analysed yet</h2>
          <p className="liner__note">
            Tempo, key and cue points appear once this has been listened to.
          </p>
        </section>
      )}

      <section className="liner__card glass">
        <h2 className="label">lyrics</h2>
        {lookupError && <ErrorNotice error={lookupError} />}

        {/* The artist portrait and the genre arrive with the words. They sit
            in this card rather than in the header for the same reason the
            words do: everything in here came from somewhere else. */}
        {(pictures.artist || looked?.genre) && (
          <div className="liner__who">
            {pictures.artist && (
              <img className="liner__who-art" src={pictures.artist} alt="" />
            )}
            <div className="liner__who-text">
              <p className="liner__who-name">{track.artist || "—"}</p>
              {looked?.genre && (
                <p className="liner__note">
                  Filed as {looked.genre} by Deezer
                  {track.genre ? `; this file says ${track.genre}.` : "."}
                </p>
              )}
            </div>
          </div>
        )}

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
              Sends this track's artist and title to LRCLIB and Deezer.
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
            Off. Lyrics are the one thing here that comes from elsewhere —
            switch it on in Settings.
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
          {/* The design's "credits" become this: where every field above came
              from. One line — it is a footnote, not the subject. */}
          {track.tagged
            ? "From the file's tags, and its path where it had none."
            : "From the file's path. Its tags are read when it is analysed."}
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

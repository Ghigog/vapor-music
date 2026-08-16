/**
 * Your Data.
 *
 * New in v2, and the design is explicit about why: it is where the sovereignty
 * claim gets proved rather than asserted. So a total is not enough — every file
 * is named, sized and pointed at, and the folder can be opened so a person can
 * see for themselves that it is plain JSON.
 *
 * The "never does" list is the design's copy, and each line is checkable
 * against this repository. That is the point of writing it down.
 */

import { useCallback, useEffect, useState } from "react";
import * as core from "../lib/core";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";

/** Offered ceilings. A large library is a legitimate reason to want a large
 *  cache — the point of the bound is that it is finite and chosen, not small. */
const CACHE_SIZES = [
  2 * 1024 ** 3,
  4 * 1024 ** 3,
  8 * 1024 ** 3,
  16 * 1024 ** 3,
  32 * 1024 ** 3,
  64 * 1024 ** 3,
  128 * 1024 ** 3,
];

/** The design's list, verbatim. Every line is a claim the code keeps. */
const NEVERS = [
  "Send a listening log anywhere — there is no endpoint to send it to.",
  "Ask for an email, a password, or a subscription.",
  "Hold your library hostage in a format only Vapor can read.",
  "Load a single analytics or advertising SDK.",
];

export function YourData() {
  const [rows, setRows] = useState<core.DataRow[]>([]);
  const [cache, setCache] = useState<core.CacheStatus | null>(null);
  const [location, setLocation] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [r, c, l] = await Promise.allSettled([
      core.dataBreakdown(),
      core.cacheStatus(),
      core.dataLocation(),
    ]);
    if (r.status === "fulfilled") setRows(r.value);
    if (c.status === "fulfilled") setCache(c.value);
    if (l.status === "fulfilled") setLocation(l.value);

    // `allSettled` and then only reading the fulfilled ones swallows every
    // failure. On this screen in particular that is the wrong thing to do: it
    // exists to let someone check what is stored, and a read that failed shows
    // as an empty list — which reads as "nothing is stored" rather than as
    // "this could not be read". Whatever loaded still shows; the rest is said.
    const failed = [r, c, l].find((x) => x.status === "rejected");
    setError(failed ? messageOf(failed.reason) : null);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const localBytes = rows
    .filter((r) => r.local)
    .reduce((sum, r) => sum + r.bytes, 0);

  return (
    <div className="data">
      <header className="data__head">
        <h1 className="data__title">Your data</h1>
        <p className="data__lede">
          Everything Vapor knows about your library, and exactly where it sits.
        </p>
      </header>

      <section className="data__card glass">
        <div className="data__summary">
          <h2 className="label">on this device</h2>
          <span className="data__total numeric">{bytes(localBytes)}</span>
        </div>

        <ul className="data__rows">
          {rows.map((row) => (
            <li key={row.label} className="data__row">
              <span className="data__row-text">
                <span className="data__row-label">{row.label}</span>
                <span className="data__row-path numeric" title={row.path}>
                  {row.path}
                </span>
              </span>
              <span className="data__row-size numeric">
                {/* The library on the server is not measured — asking a WebDAV
                    server for the size of every file is a scan, and a scan is
                    not something to run because a screen was opened. */}
                {row.local ? bytes(row.bytes) : "on your server"}
              </span>
            </li>
          ))}
        </ul>

        <div className="data__actions">
          <button
            className="data__button"
            onClick={() => {
              core.revealDataFolder().catch((e: unknown) => setError(messageOf(e)));
            }}
          >
            Open folder
          </button>
        </div>

        <p className="data__note">
          Metadata lives in plain JSON. Open it in any text editor — no export
          tool, no lock-in.
        </p>
      </section>

      {cache && (
        <section className="data__card glass">
          <div className="data__summary">
            <h2 className="label">offline cache</h2>
            <span className="data__total numeric">
              {cache.tracksCached.toLocaleString()} of{" "}
              {cache.tracksTotal.toLocaleString()} tracks
            </span>
          </div>
          <div className="data__meter">
            <div
              className="data__meter-fill"
              style={{
                width: `${Math.min((cache.bytes / Math.max(cache.maxBytes, 1)) * 100, 100)}%`,
              }}
            />
          </div>
          <p className="data__note">
            {bytes(cache.bytes)} of {bytes(cache.maxBytes)} used. Audio is
            fetched as it is needed and the oldest is dropped first — the
            analysis is kept, because it is small and expensive and the audio is
            large and cheap to fetch again.
          </p>

          <label className="data__field">
            <span className="label">ceiling</span>
            <select
              className="data__select"
              value={String(cache.maxBytes)}
              onChange={(e) => {
                void core
                  .setCacheMaxBytes(Number(e.target.value))
                  .then(refresh)
                  .catch((err: unknown) => setError(messageOf(err)));
              }}
            >
              {CACHE_SIZES.map((n) => (
                <option key={n} value={n}>
                  {bytes(n)}
                </option>
              ))}
            </select>
          </label>

          <div className="data__actions">
            {/* Distinct from "delete everything": this is the only re-fetchable
                part of the directory and the only part that gets large, so
                reclaiming space should not also cost the analysis. */}
            <button
              className="data__button"
              disabled={cache.bytes === 0}
              onClick={() => {
                void core
                  .clearAudioCache()
                  .then(async (freed) => {
                    // Read the cache back before saying anything about it.
                    // "Freed 2.1 GB" is the command's own account of what it
                    // did; whether the cache is actually empty afterwards is a
                    // different question, and it is the one being answered on
                    // screen.
                    const after = await core.cacheStatus();
                    await refresh();
                    setNote(
                      after.bytes === 0
                        ? `Freed ${bytes(freed)}. Tracks are fetched again as they are played.`
                        : `Freed ${bytes(freed)}, but ${bytes(after.bytes)} is still cached — ` +
                          `something is holding files open. Try again once playback has stopped.`,
                    );
                  })
                  .catch((err: unknown) => setError(messageOf(err)));
              }}
            >
              Clear cached audio
            </button>
          </div>
        </section>
      )}

      <section className="data__card glass">
        <h2 className="label">what Vapor never does</h2>
        <ul className="data__nevers">
          {NEVERS.map((line) => (
            <li key={line} className="data__never">
              <span className="data__never-mark sovereign" aria-hidden="true">
                ✓
              </span>
              <span>{line}</span>
            </li>
          ))}
        </ul>
      </section>

      <section className="data__card glass">
        <h2 className="label">delete</h2>
        <p className="data__note">
          Removes the index, the analysis, playlists, cached audio and the saved
          password. Your music on the server is untouched.
        </p>
        <div className="data__actions">
          <button
            className="data__button data__button--danger"
            onClick={() => {
              if (
                !window.confirm(
                  "Delete the local index, analysis, playlists, cached audio and the saved password? Your music on the server is untouched.",
                )
              ) {
                return;
              }
              // Deleting used to refresh and say nothing at all, which leaves
              // the most consequential button on the screen with no outcome
              // to read — and no way to notice a delete that only half
              // happened. What is *left* is read back and reported.
              void core
                .deleteAllData()
                .then(async () => {
                  await refresh();
                  const [remaining, stillCached] = await Promise.all([
                    core.dataBreakdown(),
                    core.cacheStatus(),
                  ]);
                  const left = remaining.filter((r) => r.local).length;
                  setNote(
                    left === 0 && stillCached.bytes === 0
                      ? "Deleted. Nothing is stored locally any more; your music on the server is untouched."
                      : `Deleted, but ${left} ${left === 1 ? "item" : "items"} and ` +
                        `${bytes(stillCached.bytes)} of cached audio are still here.`,
                  );
                })
                .catch((e: unknown) => setError(messageOf(e)));
            }}
          >
            Delete everything stored here
          </button>
        </div>
      </section>

      {location && (
        <p className="data__footnote numeric" title={location}>
          {location}
        </p>
      )}
      {note && <p className="data__ok">{note}</p>}
      {error && <ErrorNotice error={error} onDismiss={() => setError(null)} />}
    </div>
  );
}

/** Binary units, matching how the cache bound is expressed. */
function bytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(
    Math.floor(Math.log(n) / Math.log(1024)),
    units.length - 1,
  );
  const value = n / 1024 ** i;
  return `${value >= 10 || i === 0 ? Math.round(value) : value.toFixed(1)} ${units[i]}`;
}

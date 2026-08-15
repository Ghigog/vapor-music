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

  const refresh = useCallback(async () => {
    const [r, c, l] = await Promise.allSettled([
      core.dataBreakdown(),
      core.cacheStatus(),
      core.dataLocation(),
    ]);
    if (r.status === "fulfilled") setRows(r.value);
    if (c.status === "fulfilled") setCache(c.value);
    if (l.status === "fulfilled") setLocation(l.value);
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
              core
                .deleteAllData()
                .then(refresh)
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

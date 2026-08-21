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
import { Confirm } from "../components/Confirm";
import { listen } from "@tauri-apps/api/event";

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

/** A class-safe name for a row, so each slice can carry its own colour. */
function slug(label: string): string {
  return label.toLowerCase().replace(/[^a-z0-9]+/g, "-");
}

export function YourData({ embedded = false }: { embedded?: boolean } = {}) {
  const [rows, setRows] = useState<core.DataRow[]>([]);
  const [cache, setCache] = useState<core.CacheStatus | null>(null);
  const [location, setLocation] = useState("");
  const [error, setError] = useState<string | null>(null);
  /** Whether the "delete everything?" question is open. */
  const [purging, setPurging] = useState(false);
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

  /*
   * Follow the pass, which is what is changing these numbers.
   *
   * Everything on this screen was read once, when it opened. But an analysis
   * pass downloads every track it describes — about ten megabytes each — so the
   * offline cache and the track count climb the whole time it runs, and this
   * screen sat showing whatever they were when it was opened. On a job lasting
   * hours that is a stale figure presented as a current one, on the screen
   * whose whole job is saying exactly what is stored and where.
   *
   * Rate-limited because reading the breakdown walks the cache directory: a
   * track lands about once a minute today, but a warm cache or a fast
   * connection would make that far more often, and this must not turn into a
   * directory walk per track.
   */
  useEffect(() => {
    let last = 0;
    const unlisten = listen("analysis-progress", () => {
      const now = Date.now();
      if (now - last < 5000) return;
      last = now;
      void refresh();
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [refresh]);

  const localBytes = rows
    .filter((r) => r.local)
    .reduce((sum, r) => sum + r.bytes, 0);

  return (
    <div className="data">
      {/*
        Embedded, this is a section of Settings rather than a screen of its own,
        so it takes a section's heading. Two `h1`s on one page is a document
        with two titles, and a screen reader's heading list is how a lot of
        people navigate a form this long.
      */}
      <header className="data__head">
        {embedded ? (
          <h2 className="data__title data__title--section">Your data</h2>
        ) : (
          <h1 className="data__title">Your data</h1>
        )}
        <p className="data__lede">
          Everything Vapor knows about your library, and exactly where it sits.
        </p>
      </header>

      <section className="data__card glass">
        <div className="data__summary">
          <h2 className="label">on this device</h2>
          <span className="data__total numeric">{bytes(localBytes)}</span>
        </div>

        {/*
          What the total is made of, at a glance.
          
          The rows below already give every number, but a list of sizes does not
          answer "what is actually taking the space" without arithmetic — and on
          this library the answer is that the audio cache is essentially all of
          it and the metadata is a rounding error. That is worth being able to
          see, because it is the difference between "the app is hoarding" and
          "the app is holding music you asked it to hold".
          
          Only the local rows: the library on the server has no measured size,
          and giving it a share of a bar about this device would be inventing
          one.
        */}
        {localBytes > 0 && (
          <div
            className="data__split"
            role="img"
            aria-label={rows
              .filter((r) => r.local && r.bytes > 0)
              .map(
                (r) =>
                  `${r.label}: ${Math.round((r.bytes / localBytes) * 100)}%`,
              )
              .join(", ")}
          >
            {rows
              .filter((r) => r.local && r.bytes > 0)
              .map((r) => (
                <span
                  key={r.label}
                  className={`data__split-part data__split-part--${slug(r.label)}`}
                  style={{ width: `${(r.bytes / localBytes) * 100}%` }}
                  title={`${r.label} — ${bytes(r.bytes)}`}
                />
              ))}
          </div>
        )}

        <ul className="data__rows">
          {rows.map((row) => (
            <li key={row.label} className="data__row">
              <span className="data__row-text">
                <span className="data__row-label">{row.label}</span>
                <span className="data__row-path numeric" title={row.path}>
                  {row.path}
                </span>
                {/*
                  The row's own share, drawn in its own colour.

                  The stacked bar above answers "what is taking the space" only
                  if you can tell its segments apart, and on this library one
                  segment is almost the whole width — so every other category is
                  a sliver with no readable size. A bar per row gives each one a
                  full width to be small against, and keeps the colour so the
                  eye can match a row to its piece of the total.
                */}
                {row.local && localBytes > 0 && (
                  <span className="data__row-bar" aria-hidden="true">
                    <span
                      className={`data__row-bar-fill data__split-part--${slug(row.label)}`}
                      style={{
                        width: `${Math.max((row.bytes / localBytes) * 100, row.bytes > 0 ? 1 : 0)}%`,
                      }}
                    />
                  </span>
                )}
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

        {/*
          Every action on this data, together.

          Emptying the cache lived on a second card about held audio, and
          deleting everything on a third about deletion — so three cards each
          owned a verb and none of them owned the nouns, which are all here.
        */}
        <div className="data__actions">
          <button
            className="data__button"
            onClick={() => {
              core.revealDataFolder().catch((e: unknown) => setError(messageOf(e)));
            }}
          >
            Open folder
          </button>
          {/* Distinct from deleting everything: audio is the only re-fetchable
              part of the directory and the only part that gets large, so
              reclaiming space should not also cost the analysis. */}
          {/* The cap on cached audio, beside the number it caps. It had a card
              of its own — "audio held on this device" — reporting bytes the
              storage list above already reports, so only this survived it. */}
          <label className="data__field data__field--inline">
            <span className="label">keep at most</span>
            <select
              className="data__select"
              value={String(cache?.maxBytes ?? 0)}
              disabled={!cache}
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
          <button
            className="data__button"
            disabled={!cache || cache.bytes === 0}
            onClick={() => {
              setNote(null);
              void core
                .clearAudioCache()
                .then(async (freed) => {
                  await refresh();
                  // Read back what is actually gone rather than assume: the
                  // delete button on this screen was changed for the same
                  // reason — a button whose outcome nobody checks is one that
                  // can half-work in silence.
                  const after = await core.cacheStatus();
                  setNote(
                    after.bytes === 0
                      ? `Freed ${bytes(freed)}. Audio is fetched again as you play.`
                      : `Freed ${bytes(freed)}, and ${bytes(after.bytes)} is still held.`,
                  );
                })
                .catch((e: unknown) => setError(messageOf(e)));
            }}
          >
            Empty cache
          </button>
        </div>
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
            onClick={() => setPurging(true)}
          >
            Delete everything stored here
          </button>
        </div>
      </section>

      {purging && (
        <Confirm
          title="Delete everything stored here?"
          body="The index, the analysis, playlists, cached audio and the saved password all go. Your music on the server is untouched."
          confirmLabel="Delete everything"
          onConfirm={() => {
            setPurging(false);
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
          onCancel={() => setPurging(false)}
        />
      )}

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

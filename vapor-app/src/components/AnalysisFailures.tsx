/**
 * The tracks analysis could not describe.
 *
 * ## Why this screen exists
 *
 * The Settings card said "556 of 563 done" and pressing Analyse again left it
 * at 556. Nothing on screen named the seven or said what was wrong with them,
 * and there was no way to find out from inside the app.
 *
 * Two kinds of failure reach here, and the difference is the useful part:
 *
 * - **Permanent** — the file was read and could not be used. Recorded once and
 *   remembered, because the answer does not change between launches. These
 *   count as done, so they are *not* what holds the number short.
 * - **Stalled** — the pass means to try again. A track it simply had not
 *   fetched yet is ordinary and resolves itself on the next pass. A track that
 *   fails this way on every pass is the one that keeps the count short for
 *   ever, and the attempt count is what tells the two apart.
 *
 * The reason is the server's answer, not this app's summary of it. Every
 * stalled track used to read "not available locally", which is true of every
 * track in a library streamed over WebDAV and so said nothing about which one
 * of them was different — a 404, a refused password and a full disk all
 * arrived as the same six words. What is shown now names the thing to go and
 * fix.
 */

import { useEffect, useState } from "react";
import * as core from "../lib/core";
import { Modal } from "./HelpModal";
import { ErrorNotice, messageOf } from "./ErrorNotice";

/** How many passes have to hit a track before "not yet" reads as "not ever". */
const PERSISTENT = 3;

export function AnalysisFailures({ onClose }: { onClose: () => void }) {
  const [rows, setRows] = useState<core.AnalysisFailure[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    core
      .analysisFailures()
      .then((found) => {
        if (!cancelled) setRows(found);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(messageOf(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <Modal title="Could not be analysed" onClose={onClose}>
      {error && <ErrorNotice error={error} onDismiss={() => setError(null)} />}

      {rows === null && !error && <p className="failures__note">Reading…</p>}

      {rows?.length === 0 && (
        <p className="failures__note">
          Every track in your library has been described.
        </p>
      )}

      {rows && rows.length > 0 && (
        <>
          <p className="failures__note">
            Analysis reads each file to work out tempo, key and cue points.
            These {rows.length === 1 ? "one is" : `${rows.length} are`} the
            {rows.length === 1 ? " track" : " tracks"} it could not.
          </p>
          <ul className="failures__list">
            {rows.map((row) => (
              <li key={row.href} className="failures__row">
                <span className="failures__text">
                  <span className="failures__title">{row.title}</span>
                  {row.artist && (
                    <span className="failures__artist">{row.artist}</span>
                  )}
                  <span className="failures__reason numeric">{row.reason}</span>
                  <span className="failures__href numeric">{row.href}</span>
                </span>
                <span className="failures__kind">{label(row)}</span>
              </li>
            ))}
          </ul>
        </>
      )}
    </Modal>
  );
}

/**
 * What to call this failure, in the two words the row has space for.
 *
 * A stalled track on its first pass is genuinely ordinary — it had not been
 * downloaded when the pass reached it — so calling that "failed" would be
 * wrong. After a few passes the same words stop being true, and the label is
 * the only thing that can say so without making somebody press Analyse a
 * fourth time to find out.
 */
function label(row: core.AnalysisFailure): string {
  if (row.permanent) return "unreadable";
  if (row.attempts >= PERSISTENT) return `stuck · ${row.attempts} tries`;
  return row.attempts > 1 ? `retrying · ${row.attempts}` : "will retry";
}

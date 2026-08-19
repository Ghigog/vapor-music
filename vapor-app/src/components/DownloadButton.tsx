/**
 * Keep a playlist or a smart group on the device.
 *
 * Vapor plays from your server. The audio cache holds a few tracks either side
 * of where the set is and drops the rest, because a file read once is not a
 * reason to keep it. This is the exception a person asks for: these tracks are
 * fetched in full, kept outside the evicted directory, and stay until they are
 * removed.
 *
 * Progress is per track rather than one long wait. On a slow connection a
 * playlist is minutes, and a control that goes quiet for minutes has failed as
 * far as anyone watching can tell.
 */
import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as core from "../lib/core";
import { messageOf } from "./ErrorNotice";

export function DownloadButton({
  kind,
  id,
  hrefs,
  onChange,
}: {
  kind: core.Collection;
  id: string;
  /** The tracks it holds, so this can tell whether they are already kept. */
  hrefs: string[];
  /** Something changed on disk; the caller may want to re-read. */
  onChange?: (() => void) | undefined;
}) {
  const [kept, setKept] = useState<ReadonlySet<string>>(new Set());
  const [progress, setProgress] = useState<core.DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setKept(new Set(await core.downloadedTracks()));
    } catch {
      // A list that cannot be read is not worth an error on this control: the
      // button still works, it simply cannot say what is already kept.
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const unlisten = listen<core.DownloadProgress>("download-progress", (e) => {
      setProgress(e.payload);
      if (e.payload.error) setError(e.payload.error);
      if (e.payload.finished) {
        void refresh();
        onChange?.();
        // Left on screen briefly so a short download is not a flicker.
        setTimeout(() => setProgress(null), 1500);
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [refresh, onChange]);

  const has = hrefs.length > 0 && hrefs.every((h) => kept.has(h));
  const running = progress !== null && !progress.finished;

  async function act() {
    setError(null);
    try {
      if (has) {
        await core.removeDownload(kind, id);
        await refresh();
        onChange?.();
      } else {
        await core.downloadCollection(kind, id);
        // Read it back as well as listening for progress.
        //
        // The call returns as soon as the work has *started*, so this usually
        // finds nothing kept yet and the events do the rest. But a control
        // whose only way of learning is an event it might never see is one that
        // can sit there saying the wrong thing for ever — which is the same
        // fault as the analysis card's, and it is not worth repeating for the
        // sake of one less read.
        await refresh();
      }
    } catch (e: unknown) {
      setError(messageOf(e));
      setProgress(null);
    }
  }

  return (
    <div className="download">
      <button
        className={"download__button" + (has ? " download__button--has" : "")}
        onClick={() => void act()}
        disabled={running || hrefs.length === 0}
        title={
          has
            ? "Kept on this device. Remove to free the space."
            : "Keep these tracks on this device."
        }
      >
        <span
          className={"icon " + (has ? "icon--song" : "icon--library")}
          aria-hidden="true"
        />
        {running
          ? `Downloading ${progress?.done ?? 0} of ${progress?.total ?? 0}`
          : has
            ? "Downloaded"
            : "Download"}
      </button>

      {running && progress && (
        <p className="download__now numeric" aria-live="polite">
          {progress.title}
        </p>
      )}
      {error && <p className="download__error">{error}</p>}
    </div>
  );
}

/** The marker a kept track carries in a list. */
export function DownloadedMark({ kept }: { kept: boolean }) {
  if (!kept) return null;
  return (
    <span
      className="icon icon--song download__mark"
      title="Kept on this device"
      aria-label="Kept on this device"
    />
  );
}

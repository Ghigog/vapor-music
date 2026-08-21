/**
 * A tile's artwork, fetched on its own.
 *
 * One request per tile rather than a field on every row: artwork is capped at
 * 2 MB by the tag reader, and 563 rows carrying covers would move hundreds of
 * megabytes through IPC on each keystroke.
 *
 * Absent is the normal case, not a failure — a track has no cover until
 * analysis has read the file, and a freshly scanned library has none at all.
 * So there is no error state here: the gradient placeholder *is* the answer.
 *
 * Its own file because two screens draw the same tile now. The album grid had
 * it inline, and the home shelves would otherwise have needed either a second
 * copy of the fallback rule below or an import out of a screen, which is the
 * shape that ends in two screens importing each other.
 */

import { useEffect, useState } from "react";
import * as core from "../lib/core";

export function Cover({
  href,
  label,
  artist,
}: {
  href: string;
  label: string;
  /**
   * When this tile is an artist, their name — so a looked-up portrait can
   * stand in where the lead track has no embedded art (TD-53).
   *
   * The file's own artwork still wins. An artist tile falling back to a
   * picture of the *album* the lead track came from is the case this fixes:
   * it looked like the app knew who the artist was, and it did not.
   */
  artist?: string;
}) {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    // Nothing to ask about: a playlist with no tracks in it yet.
    if (!href) return;
    core
      .trackCover(href)
      .then(async (data) => {
        if (cancelled) return;
        if (data || !artist) {
          setSrc(data);
          return;
        }
        const portrait = await core.artistPortrait(artist).catch(() => null);
        if (!cancelled) setSrc(portrait);
      })
      .catch(() => {
        if (!cancelled) setSrc(null);
      });
    return () => {
      cancelled = true;
    };
  }, [href, artist]);

  return (
    <div className="card__art">
      {src ? (
        <img className="card__img" src={src} alt={`Cover of ${label}`} />
      ) : (
        <div className="card__art-sheen" aria-hidden="true" />
      )}
    </div>
  );
}

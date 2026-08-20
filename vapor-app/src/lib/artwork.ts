/**
 * Row artwork, fetched one track at a time.
 *
 * The library view carries no covers and must not: a cover runs to about 2 MB
 * and a 563-row response carrying one each would move hundreds of megabytes
 * through IPC on every keystroke — the mistake that killed the phone when the
 * queue did exactly that. So a row asks for its own, by href, when it is on
 * screen. The table is virtualised, so that is a screenful, not a library.
 *
 * ## Why the cache is bounded
 *
 * Kept, because scrolling back up should not re-fetch, and because two screens
 * showing the same track should not each hold their own copy. Bounded, because
 * these are megabyte data URIs and remembering every one a long scroll touched
 * is how a list turns into a gigabyte of webview heap. Least-recently-used, so
 * what stays is what is being looked at.
 */

import { useEffect, useState } from "react";

import * as core from "./core";

/**
 * Two stores, because the two sizes cost three orders of magnitude apart.
 *
 * A thumbnail is about 6 KB, so a whole library's worth is a few megabytes and
 * there is no reason to evict one a scroll might come back to. A full cover
 * averages 281 KB on the library this was measured against, and eight of those
 * is already 2 MB of webview heap — which is the whole reason `trackThumb`
 * exists (PERF-004).
 */
const LIMITS = { thumb: 600, full: 8 } as const;

type Size = keyof typeof LIMITS;

/** `null` is a track with no artwork — a real answer, and worth remembering. */
const caches: Record<Size, Map<string, string | null>> = {
  thumb: new Map(),
  full: new Map(),
};
/** One request per href per size, however many rows ask at once. */
const inflight: Record<Size, Map<string, Promise<string | null>>> = {
  thumb: new Map(),
  full: new Map(),
};

function remember(size: Size, href: string, cover: string | null) {
  const cache = caches[size];
  cache.delete(href);
  cache.set(href, cover);
  while (cache.size > LIMITS[size]) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
}

/** What is already known, or `undefined` for "not asked yet". */
function known(size: Size, href: string): string | null | undefined {
  const cache = caches[size];
  if (!cache.has(href)) return undefined;
  const cover = cache.get(href) ?? null;
  // Touch it, so scrolling keeps what is on screen rather than evicting it.
  remember(size, href, cover);
  return cover;
}

function fetchArt(size: Size, href: string): Promise<string | null> {
  const cached = known(size, href);
  if (cached !== undefined) return Promise.resolve(cached);

  const pending = inflight[size];
  const existing = pending.get(href);
  if (existing) return existing;

  const ask = size === "thumb" ? core.trackThumb : core.trackCover;
  const request = ask(href)
    .then((cover) => {
      remember(size, href, cover);
      return cover;
    })
    // A cover that cannot be read is drawn the same as a track that has none.
    .catch(() => null)
    .finally(() => pending.delete(href));

  pending.set(href, request);
  return request;
}

/** Tile-sized artwork — song rows, the queue's now-playing tile. */
export function thumbFor(href: string): Promise<string | null> {
  return fetchArt("thumb", href);
}

/** Full-resolution artwork, for somewhere big enough to want it. */
export function coverFor(href: string): Promise<string | null> {
  return fetchArt("full", href);
}

/** Forget everything. For tests, and for after the tags are re-read. */
export function forgetCovers() {
  for (const size of ["thumb", "full"] as const) {
    caches[size].clear();
    inflight[size].clear();
  }
}

/**
 * Artwork for one track at `size`, or `null` until there is some to draw.
 *
 * Resolves from the cache synchronously where it can, so a row scrolled back
 * into view does not flash its placeholder.
 */
function useArt(size: Size, href: string): string | null {
  const [src, setSrc] = useState<string | null>(() =>
    href ? (known(size, href) ?? null) : null,
  );

  useEffect(() => {
    // No track — a queue with nothing playing, say. Not a request.
    if (!href) {
      setSrc(null);
      return;
    }
    const cached = known(size, href);
    if (cached !== undefined) {
      setSrc(cached);
      return;
    }
    let cancelled = false;
    // The row is showing a different track now, so whatever is on it belongs
    // to the old one.
    setSrc(null);
    void fetchArt(size, href).then((cover) => {
      if (!cancelled) setSrc(cover);
    });
    return () => {
      cancelled = true;
    };
  }, [size, href]);

  return src;
}

/** Tile-sized artwork. What every list and row should use. */
export function useThumb(href: string): string | null {
  return useArt("thumb", href);
}

/** Full-resolution artwork, for somewhere big enough to want it. */
export function useCover(href: string): string | null {
  return useArt("full", href);
}

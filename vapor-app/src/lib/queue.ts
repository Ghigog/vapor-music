/**
 * What is still to come.
 *
 * The queue holds the whole set — what has played, what is playing, and what
 * follows — because reordering, removal and "play from here" all address a
 * track by its position in that list. A screen headed "Up next" wants only the
 * tail of it, and the offset back into the full list, or it would move the
 * wrong track.
 */

import type { QueueView, QueueEntry } from "./core";

export interface UpNext {
  /** Index in `view.entries` that `entries[0]` came from. */
  first: number;
  entries: QueueEntry[];
}

/**
 * Everything after the playhead.
 *
 * Not "every entry that is not the current one", which is what this was: that
 * keeps the whole history in the list, so four records into a set the screen
 * still opened on the first of them with the track playing sitting fifth — and
 * read as a list that had stopped updating. `remainingSecs` is counted from the
 * playhead on the backend, which is why the count and the time disagreed.
 *
 * Nothing playing means nothing has been played, so the whole queue is to come.
 */
export function upNextOf(view: QueueView): UpNext {
  const first = (view.current ?? -1) + 1;
  return { first, entries: view.entries.slice(first) };
}

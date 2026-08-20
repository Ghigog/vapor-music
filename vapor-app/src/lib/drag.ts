/**
 * Dragging, on a touch screen.
 *
 * HTML5 drag-and-drop does not exist on touch — `dragstart` never fires — so
 * the desktop path in `PlaylistRail` is inert on a phone and this is the other
 * half. It is deliberately only the *input*: the rules about what may be
 * dropped where live in `dropOn` below, and the desktop adapter is meant to
 * call the same function, so the two paths cannot come to different answers
 * about whether a track belongs in a dynamic group.
 *
 * What the browser gives away for free on desktop and has to be built here: the
 * thing that follows the finger, working out what is under it, opening a
 * target that needs opening, and scrolling a list while still holding
 * something over it.
 */
import * as core from "./core";

/** What is being carried. */
export interface DragPayload {
  /**
   * Tracks carry hrefs; the other three carry one name.
   *
   * The distinction is the whole of the drop rules: a playlist is a list of
   * tracks, and a dynamic group is a set of entities that resolves to tracks. So
   * every kind can go to a playlist, and a track cannot go to a group.
   */
  kind: "track" | "artist" | "album" | "genre";
  values: string[];
  /** What the thing under the finger says. */
  label: string;
}

/** Long enough to be deliberate, short enough not to feel stuck. */
export const DWELL_MS = 500;

/** How close to an edge counts as "hold here to scroll". */
const EDGE_PX = 44;

/** Pixels per frame at the very edge. Slow on purpose: this is a nudge. */
const EDGE_SPEED = 6;

export type DropTarget =
  | { kind: "tab"; tab: "playlist" | "group" }
  | { kind: "item"; menu: "playlist" | "group"; id: string }
  | null;

/**
 * What is under the pointer, as far as dropping is concerned.
 *
 * `elementFromPoint` rather than the event's target: the thing under the finger
 * during a drag is the preview following it, and asking the event would answer
 * with that every time.
 */
export function targetAt(x: number, y: number, preview: Element | null): DropTarget {
  const previous = preview instanceof HTMLElement ? preview.style.display : null;
  if (preview instanceof HTMLElement) preview.style.display = "none";
  const el = document.elementFromPoint(x, y);
  if (preview instanceof HTMLElement && previous !== null) {
    preview.style.display = previous;
  }
  if (!el) return null;

  const item = el.closest<HTMLElement>("[data-drop-id]");
  if (item) {
    const menu = item.closest<HTMLElement>("[data-menu]")?.dataset.menu;
    if (menu === "playlist" || menu === "group") {
      return { kind: "item", menu, id: item.dataset.dropId ?? "" };
    }
  }

  const tab = el.closest<HTMLElement>("[data-tab]")?.dataset.tab;
  if (tab === "playlist" || tab === "group") return { kind: "tab", tab };

  return null;
}

/** Whether this payload may be dropped on this menu at all. */
export function accepts(payload: DragPayload, menu: "playlist" | "group"): boolean {
  if (menu === "playlist") return true;
  // A dynamic group holds artists, albums and genres. A single track is not one
  // of those, and turning it into its album would put something in the set
  // that nobody chose.
  return payload.kind !== "track";
}

/** Why not, for the person holding it. */
export function refusal(payload: DragPayload): string {
  return `A dynamic group holds artists, albums and genres — not single tracks. Drop “${payload.label}” on a playlist instead.`;
}

/**
 * Scroll a list while something is being held over it.
 *
 * Returns the pixels moved, so a caller can keep asking on an animation frame
 * for as long as the finger stays put.
 */
export function edgeScroll(list: HTMLElement, y: number): number {
  const box = list.getBoundingClientRect();
  const fromTop = y - box.top;
  const fromBottom = box.bottom - y;

  if (fromTop < EDGE_PX && list.scrollTop > 0) {
    const speed = EDGE_SPEED * (1 - Math.max(fromTop, 0) / EDGE_PX);
    list.scrollTop -= speed;
    return -speed;
  }
  const room = list.scrollHeight - list.clientHeight - list.scrollTop;
  if (fromBottom < EDGE_PX && room > 0) {
    const speed = EDGE_SPEED * (1 - Math.max(fromBottom, 0) / EDGE_PX);
    list.scrollTop += speed;
    return speed;
  }
  return 0;
}

/**
 * Perform the drop.
 *
 * The one place either input path decides what a drop means, so the touch
 * adapter and the desktop one cannot disagree. Throws with a readable reason
 * rather than returning a flag: every caller shows the message.
 */
export async function dropOn(
  payload: DragPayload,
  target: { menu: "playlist" | "group"; id: string },
): Promise<string> {
  if (!accepts(payload, target.menu)) throw new Error(refusal(payload));

  if (target.menu === "group") {
    // `kind` is narrowed by `accepts` above, but the group API is explicit
    // about the three it takes.
    const kind = payload.kind as core.EntityType;
    const value = payload.values[0] ?? payload.label;
    await core.addToGroup(target.id, kind, value);
    return `Added ${payload.label} to the group.`;
  }

  const hrefs = await tracksFor(payload);
  if (hrefs.length === 0) throw new Error(`${payload.label} has no tracks to add.`);
  const added = await core.addTracksToPlaylist(target.id, hrefs);
  return added === 1 ? "Added 1 track." : `Added ${added} tracks.`;
}

/**
 * The tracks a payload stands for.
 *
 * An entity is resolved here rather than at the drop site, because "everything
 * by this artist" is a question the library answers and the drag layer should
 * not be reimplementing.
 */
async function tracksFor(payload: DragPayload): Promise<string[]> {
  if (payload.kind === "track") return payload.values;

  const name = payload.values[0] ?? payload.label;
  const view =
    payload.kind === "artist"
      ? { artist: name }
      : payload.kind === "album"
        ? { album: name }
        : { query: name };

  const sections = await core.libraryView({ ...view, groupBy: "none" });
  const rows = sections.flatMap((s) => s.rows);
  // A genre has no exact filter, so it is matched here rather than trusting a
  // text search that would also return a track whose title contains the word.
  const matching =
    payload.kind === "genre" ? rows.filter((r) => r.genre === name) : rows;
  return matching.map((r) => r.href);
}

/**
 * Dragging: the rules, and the two ways a drag arrives.
 *
 * HTML5 drag-and-drop does not exist on touch — `dragstart` never fires — so
 * the desktop path is inert on a phone and `DragLayer` is the other half. The
 * rules about what may be dropped where live in `accepts` and `dropOn` below,
 * and both input paths call them, so the two cannot come to different answers
 * about whether a track belongs in a dynamic group.
 *
 * `writeDrag`/`readDrag` are the desktop half of that: the rails used to read
 * a bare array of hrefs off the `dataTransfer` and call `addTracksToPlaylist`
 * themselves, which is how a track could be dropped on a playlist but an album
 * could not be dropped on anything — the entity kinds existed only in the
 * touch payload, and nothing on a desktop window could produce one.
 *
 * What the browser gives away for free on desktop and has to be built for
 * touch: the thing that follows the finger, working out what is under it,
 * opening a target that needs opening, and scrolling a list while still
 * holding something over it.
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

/** Every kind, in one place, so `kindOf` can look for all of them. */
const KINDS = ["track", "artist", "album", "genre"] as const;

/**
 * The native drag type carrying a payload of this kind.
 *
 * The kind is in the type string and not only inside the JSON because a
 * `dragover` handler may not read a payload: `getData` returns "" until the
 * drop, deliberately, so that a page cannot see what is merely passing over
 * it. The list of `types` is all a target gets beforehand, and it has to be
 * enough to answer "would I take this?" — which is what `accepts` asks, and
 * what decides whether a group lights up under the cursor.
 */
export function dragType(kind: DragPayload["kind"]): string {
  return `application/x-vapor-${kind}`;
}

/**
 * Put a payload on a native drag.
 *
 * `text/plain` goes on as well because Firefox refuses to begin a drag without
 * a standard type present — the same reason the Queue's reorder sets it.
 */
export function writeDrag(data: DataTransfer, payload: DragPayload) {
  data.effectAllowed = "copy";
  data.setData(dragType(payload.kind), JSON.stringify(payload));
  data.setData("text/plain", payload.values.join("\n"));
}

/**
 * What kind is being dragged, or null when the drag is not ours.
 *
 * The only question answerable during `dragover`, where the payload itself is
 * not readable — see `dragType`.
 */
export function kindOf(data: DataTransfer): DragPayload["kind"] | null {
  return KINDS.find((kind) => data.types.includes(dragType(kind))) ?? null;
}

/** Read the payload back, or null when the drag is not ours or is malformed. */
export function readDrag(data: DataTransfer): DragPayload | null {
  const kind = kindOf(data);
  if (!kind) return null;
  try {
    const parsed: unknown = JSON.parse(data.getData(dragType(kind)));
    if (typeof parsed !== "object" || parsed === null) return null;
    const { values, label } = parsed as Partial<DragPayload>;
    // Nothing to act on, and everything below is written assuming `values[0]`
    // is there.
    if (!Array.isArray(values) || values.length === 0) return null;
    return { kind, values: values as string[], label: String(label ?? "") };
  } catch {
    return null;
  }
}

/** Long enough to be deliberate, short enough not to feel stuck. */
export const DWELL_MS = 500;

/** How close to an edge counts as "hold here to scroll". */
const EDGE_PX = 44;

/** Pixels per frame at the very edge. Slow on purpose: this is a nudge. */
const EDGE_SPEED = 6;

export type DropTarget =
  | { kind: "tab"; tab: "playlist" | "group" }
  | {
      kind: "item";
      menu: "playlist" | "group";
      id: string;
      /** What to call it afterwards. Carried on the element as
       *  `data-drop-name` rather than scraped out of its text, which is a
       *  name plus a track count and would read as "Added 2 to Late Night 14". */
      name: string;
      /**
       * The row itself, for the highlight.
       *
       * Handed back rather than looked up again by id, because the same id is
       * now on two elements: the sidebar rail and the tab menu draw the same
       * playlist, and only one of them is on screen at a given width. A
       * `querySelector` for the id found whichever came first in the document
       * and marked the hidden one.
       */
      el: HTMLElement;
    }
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
      return {
        kind: "item",
        menu,
        id: item.dataset.dropId ?? "",
        name: item.dataset.dropName ?? "",
        el: item,
      };
    }
  }

  const tab = el.closest<HTMLElement>("[data-tab]")?.dataset.tab;
  if (tab === "playlist" || tab === "group") return { kind: "tab", tab };

  return null;
}

/**
 * Whether a drag of this kind may be dropped on this menu at all.
 *
 * The kind rather than the whole payload, because `dragover` is where this
 * question has to be answered and the payload is unreadable until the drop.
 */
export function accepts(
  kind: DragPayload["kind"],
  menu: "playlist" | "group",
): boolean {
  if (menu === "playlist") return true;
  // A dynamic group holds artists, albums and genres. A single track is not one
  // of those, and turning it into its album would put something in the set
  // that nobody chose.
  return kind !== "track";
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
 *
 * Both outcomes are said plainly, including the one where nothing happened.
 * Dropping six tracks onto a playlist that already has them looks identical to
 * a drop that missed, and a person who cannot tell those apart tries again.
 */
export async function dropOn(
  payload: DragPayload,
  target: { menu: "playlist" | "group"; id: string; name?: string },
): Promise<string> {
  if (!accepts(payload.kind, target.menu)) throw new Error(refusal(payload));
  const where = target.name || (target.menu === "group" ? "the group" : "the playlist");

  if (target.menu === "group") {
    // `kind` is narrowed by `accepts` above, but the group API is explicit
    // about the three it takes.
    const kind = payload.kind as core.EntityType;
    const value = payload.values[0] ?? payload.label;
    const added = await core.addToGroup(target.id, kind, value);
    changed("groups");
    return added
      ? `Added ${payload.label} to ${where}`
      : `${payload.label} is already in ${where}`;
  }

  const hrefs = await tracksFor(payload);
  if (hrefs.length === 0) throw new Error(`${payload.label} has no tracks to add.`);
  const added = await core.addTracksToPlaylist(target.id, hrefs);
  changed("playlists");
  return added === 0 ? `Already in ${where}` : `Added ${added} to ${where}`;
}

/**
 * Tell the rails what just changed.
 *
 * Fired here rather than by each caller: a drop from the tab menu on a phone
 * changes the same list the sidebar rail is drawing on a wide window, and a
 * rail that only refreshed when *it* was the drop target showed a stale count
 * for as long as the screen stayed put.
 */
function changed(what: "playlists" | "groups") {
  window.dispatchEvent(new Event(`vapor:${what}-changed`));
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

/**
 * Picking up an artist, an album or a genre.
 *
 * A dynamic group holds these rather than single tracks, and its empty state
 * has always said so — "drag an artist, album or genre onto it". Nothing could
 * produce one. `Songs` was the only drag source in the app and it carries
 * tracks, so `dropOn`'s group branch had no caller and `GroupRail` had no
 * handlers; the instruction described a gesture that did not exist.
 *
 * One hook, spread onto a tile, because both paths have to be wired the same
 * way in three places — the Library's entity grid and the Home shelves for
 * artists and albums — and the two of them wired differently is how a drop
 * ends up working with a mouse and not a finger.
 *
 * The tile stays a plain container of two buttons. `draggable` on the wrapper
 * is enough: a drag begun on a child starts on its nearest draggable ancestor,
 * so the sleeve and the play button are both places to pick the tile up from,
 * and both still click.
 */
import type { DragEvent as ReactDragEvent } from "react";

import { useDrag } from "./DragLayer";
import * as drag from "../lib/drag";
import { useLongPress } from "../lib/longPress";

/**
 * Props to spread onto a tile that stands for one entity.
 *
 * The touch half is `useLongPress` on the same terms `Songs` uses it: hold,
 * then move, and the thing comes with you. Holding still does nothing here —
 * a tile has no sheet to open — but it still has to be a hold rather than a
 * drag on any movement, or a grid could not be scrolled with a finger.
 */
export function useEntityDrag(kind: "artist" | "album" | "genre", name: string) {
  const carry = useDrag();
  const hold = useLongPress(
    () => {},
    (x, y) => carry.begin({ kind, values: [name], label: name }, x, y),
  );

  return {
    draggable: true,
    onDragStart: (e: ReactDragEvent) => {
      // Starting a native drag is the same opening gesture as a hold; without
      // this the timer is still running when the drag is already under way.
      hold.cancel();
      drag.writeDrag(e.dataTransfer, { kind, values: [name], label: name });
    },
    ...hold.handlers,
  };
}

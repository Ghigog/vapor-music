/**
 * What a track is, on a screen with no room to say it in the row.
 *
 * The narrow layout shows a title and nothing else — artist, album, tempo and
 * key came off the row because six columns at 412px clipped every title to
 * three characters. They are not gone, they are behind a press and hold.
 *
 * The liner-notes action is here too. It used to be a small control revealed on
 * hover over the artwork, which a phone has neither of; folding it in means one
 * gesture answers "tell me about this track" rather than two affordances
 * competing for the same 44px.
 */
import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";

import { useDialog } from "./dialog";
import { overlayRoot } from "./overlay";

export function TrackSheet({
  title,
  artist,
  album,
  bpm,
  musicalKey,
  onOpen,
  onClose,
}: {
  title: string;
  artist: string;
  album: string;
  bpm: number;
  musicalKey: string;
  /** Opens the liner notes. Absent when the caller has nowhere to open them. */
  onOpen?: (() => void) | undefined;
  onClose: () => void;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);

  useDialog(panel, onClose);

  useEffect(() => {
    closeRef.current?.focus();
  }, [onClose]);

  /*
   * An unread value is a dash, never a fabricated one.
   *
   * The table has shown "—" for an unanalysed tempo since the Godot stub was
   * caught inventing 120 BPM / 8A, which is indistinguishable from a real
   * result once it is on screen. Saying it twice differently would undo that.
   */
  const rows: [string, string][] = [
    ["Artist", artist || "—"],
    ["Album", album || "—"],
    ["Tempo", bpm > 0 ? `${Math.round(bpm)} BPM` : "—"],
    ["Key", musicalKey || "—"],
  ];

  return createPortal(
    <div
      className="sheet__backdrop"
      // Only when the backdrop itself is the target, so a drag that ends
      // outside does not dismiss what was being read.
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={panel}
        className="sheet glass"
        role="dialog"
        aria-modal="true"
        aria-label={`About ${title}`}
      >
        <h2 className="sheet__title">{title}</h2>

        <dl className="sheet__facts">
          {rows.map(([label, value]) => (
            <div className="sheet__fact" key={label}>
              <dt className="label">{label}</dt>
              <dd className="sheet__value">{value}</dd>
            </div>
          ))}
        </dl>

        <div className="sheet__actions">
          {onOpen && (
            <button
              className="sheet__open"
              onClick={() => {
                onOpen();
                onClose();
              }}
            >
              Liner notes
            </button>
          )}
          <button ref={closeRef} className="sheet__close" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>,
    overlayRoot(),
  );
}

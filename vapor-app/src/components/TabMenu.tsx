/**
 * A list that opens from a tab, rather than a screen the tab goes to.
 *
 * Playlists and dynamic groups are collections you pick from, not places the app
 * is — going to a screen to choose one and coming back out is two navigations
 * for a choice. This is the small window that rises from the bar instead.
 *
 * It is also the drop target the drag work needs: hovering a tab opens this,
 * and the row you release over is the one that receives what you are holding.
 */
import { useEffect, useRef } from "react";

export interface TabMenuItem {
  id: string;
  label: string;
  /** The line under the name — track count, or what a group holds. */
  detail?: string;
}

export function TabMenu({
  menu,
  title,
  items,
  empty,
  createLabel,
  onPick,
  onCreate,
  onClose,
}: {
  /** Which list this is, so a drop knows what it landed on. */
  menu: "playlist" | "group";
  title: string;
  items: TabMenuItem[];
  /** Shown instead of the list when there is nothing in it yet. */
  empty: string;
  createLabel: string;
  onPick: (id: string) => void;
  onCreate: () => void;
  onClose: () => void;
}) {
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="tabmenu__backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="tabmenu glass"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        ref={panel}
      >
        <div className="tabmenu__head">
          <h2 className="tabmenu__title label">{title}</h2>
          <button className="tabmenu__new" onClick={onCreate}>
            {createLabel}
          </button>
        </div>

        {items.length === 0 ? (
          <p className="tabmenu__empty">{empty}</p>
        ) : (
          /* The scroller the drag work holds a finger against to scroll. */
          <ul className="tabmenu__list" data-tabmenu-scroll="" data-menu={menu}>
            {items.map((item) => (
              <li key={item.id}>
                <button
                  className="tabmenu__item"
                  data-drop-id={item.id}
                  onClick={() => onPick(item.id)}
                >
                  <span className="tabmenu__name">{item.label}</span>
                  {item.detail && (
                    <span className="tabmenu__detail">{item.detail}</span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

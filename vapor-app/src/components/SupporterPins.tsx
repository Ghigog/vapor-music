/**
 * The supporter wall: one pin per donation, at the bottom of Settings.
 *
 * ## What it is for
 *
 * Dylan's framing: "it gets people to feel like they truly contributed to
 * something." So the pins accumulate on the *project*, not on the person. You
 * donate, the next build has one more pin in it, and one of them is yours. A
 * per-person badge would need the app to know who paid, which means identity,
 * which means the app verifying proof of payment before it shows you something
 * — the exact shape `docs/DECISIONS.md` §2 rules out.
 *
 * Everyone sees the same wall. Nothing here reads, stores, or asks who has
 * given anything.
 *
 * ## The art is a placeholder
 *
 * A flat enamel-pin disc drawn from the theme's own tokens, so it is coherent
 * in both themes and on any accent. It is deliberately simple and deliberately
 * not the mark — `VaporMark` is the identity and repeating it forty times is
 * not a supporter wall, it is wallpaper. Replace `Pin` with real artwork when
 * there is some; nothing outside this file knows what a pin looks like.
 */
import { KOFI_HANDLE, SUPPORTERS, kofiUrl } from "../lib/supporters";

/**
 * Beyond this many, the wall stops being a wall and starts being a texture.
 *
 * The count is still stated in words above it, so nothing is hidden — what is
 * capped is how many discs get drawn, because six hundred of them is a
 * rendering cost and a scroll, and reads as less rather than more.
 */
const MOST_DRAWN = 60;

/** One pin. Decorative — the count is in the text, so this is `aria-hidden`. */
function Pin({ index }: { index: number }) {
  /*
   * A repeating tilt, not a random one.
   *
   * Random would re-roll on every render, so the wall would twitch whenever
   * Settings re-rendered for an unrelated reason. Derived from the index it
   * is stable for the life of the build, and the five-step cycle is long
   * enough that the eye reads it as scattered rather than as a pattern.
   */
  const tilt = [-8, 5, -3, 9, -6][index % 5];
  return (
    <span
      className="pins__pin"
      style={{ transform: `rotate(${tilt}deg)` }}
      aria-hidden="true"
    >
      <svg viewBox="0 0 24 24" width="100%" height="100%" focusable="false">
        <circle cx="12" cy="12" r="11" className="pins__disc" />
        <circle cx="12" cy="12" r="7.5" className="pins__ring" />
        <circle cx="9.5" cy="8.5" r="2.6" className="pins__gleam" />
      </svg>
    </span>
  );
}

export function SupporterPins({
  count = SUPPORTERS,
  handle = KOFI_HANDLE,
}: {
  count?: number;
  handle?: string;
} = {}) {
  const url = kofiUrl(handle);

  // Nothing to show and nowhere to send anybody. Rendering an empty card with
  // a dead link would be worse than rendering nothing.
  if (!url && count <= 0) {
    return null;
  }

  const drawn = Math.min(Math.max(count, 0), MOST_DRAWN);

  return (
    <section className="settings__card glass">
      <h2 className="settings__section">Support</h2>

      {count > 0 ? (
        <p className="settings__hint">
          {/*
            "so far" rather than a bare number, because this figure is written
            by hand before a release and is therefore always at least slightly
            behind. Saying so is cheaper than being caught being wrong.
          */}
          {count === 1
            ? "One person has chipped in so far. Their pin is below."
            : `${count} people have chipped in so far. Their pins are below.`}
        </p>
      ) : (
        <p className="settings__hint">
          Nobody has chipped in yet. The first pin goes here.
        </p>
      )}

      {drawn > 0 && (
        <div className="pins" data-testid="supporter-pins">
          {Array.from({ length: drawn }, (_, i) => (
            <Pin key={i} index={i} />
          ))}
          {count > MOST_DRAWN && (
            <span className="pins__more label">
              and {count - MOST_DRAWN} more
            </span>
          )}
        </div>
      )}

      {url && (
        <p className="settings__hint">
          Vapor Music is free, and everything in it is free. Nothing here is
          bought, unlocked or withheld — a donation adds a pin to this wall and
          changes nothing else about the app, for you or for anybody.{" "}
          <a href={url} target="_blank" rel="noreferrer">
            Support it on Ko-fi
          </a>
          .
        </p>
      )}
    </section>
  );
}

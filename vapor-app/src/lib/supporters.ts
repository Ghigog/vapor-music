/**
 * How many people have supported Vapor Music, and where to do it.
 *
 * ## Why this is a constant and not a request
 *
 * The app could ask a server for this number. It would need a server: Ko-fi has
 * webhooks but no public "read my supporters" endpoint, so knowing the count at
 * runtime means standing something up to receive those webhooks and hold the
 * total. That is a service to maintain forever, and a fourth host in a privacy
 * document that currently makes a checkable claim about exactly three.
 *
 * It buys nothing. The number is read off the Ko-fi dashboard and written here
 * before a release, and the desktop updater carries it out to everybody on
 * their next launch — the delivery mechanism already exists and already runs.
 *
 * ## The failure mode is the good one
 *
 * A hand-updated count is stale between releases, and stale here can only mean
 * *low*: the number is monotonic and this file is only ever revised upward. So
 * the app can undercount the people who have chipped in, and can never claim
 * support it did not receive. Of the two ways to be wrong, that is the one to
 * pick.
 *
 * Android has no updater, so a build there shows whatever shipped with it until
 * it is replaced by hand.
 *
 * ## What this must not become
 *
 * A number that gates anything. `docs/DECISIONS.md` §2 is that a donation buys
 * nothing and unlocks nothing; the pins are a record of what happened, drawn
 * for everybody who opens Settings, and a person who has never donated sees
 * exactly the same wall as a person who has. That is what keeps it a gift
 * rather than a purchase, and it is the reason the count is global rather than
 * per-person — nothing here knows or asks who paid.
 */

/**
 * The Ko-fi page, by handle. Empty until Dylan fills it in, and the support
 * card does not render at all while it is empty — a link to a page that does
 * not exist is worse than no link.
 */
export const KOFI_HANDLE = "";

/**
 * Supporters to date, from the Ko-fi dashboard.
 *
 * **Update this before cutting a release** — `docs/RELEASE.md` carries it on
 * the checklist so it is not remembered by luck.
 */
export const SUPPORTERS = 0;

/** The Ko-fi page for [`KOFI_HANDLE`], or `null` when there is not one yet. */
export function kofiUrl(handle: string = KOFI_HANDLE): string | null {
  const trimmed = handle.trim();
  return trimmed ? `https://ko-fi.com/${trimmed}` : null;
}

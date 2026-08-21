/**
 * Which theme the app draws in, and how that reaches the DOM.
 *
 * Two designs, one identity: **Daylight** is warm paper with sky at the
 * horizon, **Lamplight** is the same room after dark — a warm umber ground
 * under one lamp rather than the cool near-black the placeholder dark mode
 * used. `public/tokens.css` holds both palettes; this file decides which one
 * is in force.
 *
 * ## Why `auto` is the absence of an attribute
 *
 * Before there was a control, dark mode was purely
 * `@media (prefers-color-scheme: dark)`. That behaviour is the default here
 * rather than a mode bolted beside it: under `auto` nothing is written to the
 * root at all, so the media query in `tokens.css` binds exactly as it always
 * did — including when the OS flips while the app is open, which no JavaScript
 * has to notice for the *colours* to follow.
 *
 * The one thing that does have to notice is the generative mark, which is a
 * canvas and cannot read a CSS custom property. {@link useTheme} exists for
 * it, and for nothing else.
 */

import { useSyncExternalStore } from "react";

/** What someone chose in Settings. */
export type Appearance = "auto" | "daylight" | "lamplight";

/** What that choice resolves to once the OS has been consulted. */
export type Theme = "daylight" | "lamplight";

/** Every value the control offers, in the order it offers them. */
export const APPEARANCES: readonly Appearance[] = [
  "daylight",
  "lamplight",
  "auto",
];

/** How each one reads on the control. */
export const APPEARANCE_LABELS: Record<Appearance, string> = {
  daylight: "Daylight",
  lamplight: "Lamplight",
  auto: "Auto",
};

/**
 * Read a stored appearance, tolerating anything.
 *
 * The backend sanitises the same field on load, so this is belt and braces
 * rather than the only guard — but `Settings.theme` is typed `string` across
 * the IPC boundary, and a `string` that reaches a `Record` lookup untested is
 * how a settings file someone edited becomes `undefined` in a template.
 */
export function asAppearance(value: string | null | undefined): Appearance {
  const word = (value ?? "").trim().toLowerCase();
  return (APPEARANCES as readonly string[]).includes(word)
    ? (word as Appearance)
    : "auto";
}

/** Whether the OS is asking for dark. False anywhere `matchMedia` is absent. */
export function prefersDark(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches
  );
}

/** The theme actually in force, given a choice and the OS behind it. */
export function resolveTheme(appearance: Appearance): Theme {
  if (appearance !== "auto") return appearance;
  return prefersDark() ? "lamplight" : "daylight";
}

/**
 * Put the choice on the document root, where `tokens.css` can see it.
 *
 * `daylight` and `lamplight` are written out; `auto` removes the attribute so
 * the media query takes over. Safe to call before React has mounted and safe
 * to call repeatedly — it is one attribute write.
 */
export function applyAppearance(appearance: Appearance): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (appearance === "auto") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", appearance);
}

/* ---- The live choice -------------------------------------------------
 *
 * A module-level store rather than React context, because the two things that
 * need it are as far apart as they could be — the Settings control that writes
 * it, and every `VaporMark` on screen, which is a canvas that has to be *told*
 * its ground rather than inheriting it. Threading a provider between those for
 * one string is more machinery than the string is worth.
 * -------------------------------------------------------------------- */

/**
 * Where the choice is mirrored so it can be applied before the backend answers.
 *
 * The settings round trip at boot retries for up to fifteen seconds against a
 * core that is still reading the library off disk — several seconds on a
 * phone. Waiting for it to know which theme to draw means someone who chose
 * Daylight on a dark-mode machine watches the app open dark and then blink.
 * The backend stays the record; this is just the head start.
 */
const REMEMBERED = "vapor.appearance";

let choice: Appearance = read();

function read(): Appearance {
  try {
    return asAppearance(localStorage.getItem(REMEMBERED));
  } catch {
    // Private-mode storage throws on access rather than returning null.
    return "auto";
  }
}

const listeners = new Set<() => void>();

function announce(): void {
  for (const listener of listeners) listener();
}

/** The choice as it stands. */
export function appearance(): Appearance {
  return choice;
}

/**
 * Adopt a choice: root attribute, mirror, and everything watching.
 *
 * Does not persist to the backend — `core.setAppearance` does that, and the
 * control calls both so the screen changes on the press rather than on the
 * round trip.
 */
export function adoptAppearance(next: Appearance): void {
  choice = next;
  applyAppearance(next);
  try {
    localStorage.setItem(REMEMBERED, next);
  } catch {
    // Not being able to remember it is not a reason to fail to apply it.
  }
  announce();
}

/** Apply whatever was remembered. Called once, before React mounts. */
export function restoreAppearance(): void {
  applyAppearance(choice);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  // The OS preference is the other half of `auto`, and it can change while the
  // app is open. The colours follow on their own — that is a media query — but
  // anything reading `useTheme` needs to be told.
  const query =
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia("(prefers-color-scheme: dark)")
      : null;
  query?.addEventListener("change", listener);
  return () => {
    listeners.delete(listener);
    query?.removeEventListener("change", listener);
  };
}

/**
 * The theme in force, for the things CSS cannot reach.
 *
 * `useSyncExternalStore` rather than `useState` plus an effect: the store is
 * outside React and read during render, which is exactly the tearing this hook
 * exists to prevent.
 */
export function useTheme(): Theme {
  return useSyncExternalStore(subscribe, snapshot, serverSnapshot);
}

/** The choice in force, for the control that shows which one is selected. */
export function useAppearance(): Appearance {
  return useSyncExternalStore(subscribe, appearance, () => "auto" as const);
}

function snapshot(): Theme {
  return resolveTheme(choice);
}

function serverSnapshot(): Theme {
  return "daylight";
}

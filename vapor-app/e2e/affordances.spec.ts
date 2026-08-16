/**
 * Can you actually press the things?
 *
 * ## Why this file exists
 *
 * Library shipped with a grid of album cards that were `<article>` elements
 * with no click handler. The home screen — the first thing anyone sees, the
 * screen showing them their own music — could not start a track. Pressing a
 * card did nothing at all, and it had never done anything.
 *
 * The suite did not catch it, and the reason is the interesting part. The
 * component test asked whether the grid rendered and whether the tabs
 * regrouped. The journey test double-clicked a row in Songs. The screen sweep
 * asserted, for Library and for seven other screens:
 *
 *     await expect(page.getByRole("main")).not.toBeEmpty();
 *
 * All of those passed against a screen that was, to the person using it,
 * broken. "It renders" is not "it works", and a suite built out of the first
 * claim will keep certifying apps that fail the second.
 *
 * ## What this file asserts instead
 *
 * Three things per screen, all about the primary thing on it:
 *
 * 1. **Nothing is a decoy.** Every track title in the content column sits
 *    inside something operable. A track you can read but cannot press is the
 *    bug in its general form, and this catches it without anyone having to
 *    think of the screen it will appear on next — including screens not
 *    written yet. The old `<article class="card">` fails this check.
 *
 * 2. **Pressing it does the thing.** Activating a track's control puts that
 *    track's name in the transport at the bottom of the window, which is where
 *    a person looks to find out whether it worked. Not "a request was made".
 *
 * 3. **The keyboard can do it too**, with real key presses.
 *
 * Check 1 is generic and should stay that way. Checks 2 and 3 are per-screen,
 * because the gesture and the expected outcome genuinely differ.
 */
import { expect, test, type Locator, type Page } from "@playwright/test";

/** The fixture library, by title. */
const TITLES = ["Windowlicker", "Xtal", "Roygbiv", "Not Yet Analysed"];

/**
 * What counts as operable.
 *
 * Anything a pointer and a keyboard can both reach. A `<div onClick>` is
 * deliberately absent: it works for a mouse and is invisible to everything
 * else, so accepting it would let the next version of this bug through with a
 * handler attached to an element nobody can focus.
 */
const OPERABLE = [
  "button:not([disabled])",
  "a[href]",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[role="button"]',
  '[role="option"]',
  '[role="link"]',
  '[role="menuitem"]',
  '[contenteditable="true"]',
].join(",");

/**
 * Composite widgets that hold the tab stop for the items inside them.
 *
 * A listbox is focused once and driven with arrow keys, so its options carry
 * no tabindex of their own — that is the ARIA pattern, not an oversight, and
 * for a table of ten thousand rows it is the only workable one. Check 3 has to
 * know the difference between "unreachable" and "reached the standard way".
 */
const COMPOSITE = '[role="listbox"], [role="grid"], [role="tree"], [role="toolbar"], [role="menu"], [role="tablist"]';

/** The flat table, now a tab inside Library rather than a screen. */
async function openSongs(page: Page) {
  await page.getByRole("button", { name: "Library", exact: true }).click();
  await page.getByRole("tab", { name: "Songs" }).click();
}

async function boot(page: Page) {
  await page.goto("/");
  await page.evaluate(() => window.__vaporReset({}));
  await expect(page.getByRole("main")).toBeVisible();
}

/** The title showing in the player at the bottom of the window. */
function transportTitle(page: Page): Locator {
  return page.locator(".transport__title");
}

/**
 * The pressable control for a track, by role rather than by class.
 *
 * `getByText(title)` returns the `<span>` holding the words, which is not what
 * anyone presses and cannot take focus — calling `.focus()` on it silently
 * does nothing, and the Enter that follows goes to the body. Ask for the
 * button that contains the title instead.
 */
function control(page: Page, title: string): Locator {
  return page.getByRole("main").getByRole("button").filter({ hasText: title }).first();
}

/**
 * Every visible track title with no operable ancestor.
 *
 * Returns descriptions rather than a boolean, so a failure names the element
 * to go and fix instead of only announcing that one exists.
 */
async function decoys(page: Page, displayOnly: string[]): Promise<string[]> {
  return page.evaluate(
    ({ titles, operable, exempt }) => {
      const main = document.querySelector("main");
      if (!main) return ["there is no <main> on this screen"];

      const found: string[] = [];
      for (const el of Array.from(main.querySelectorAll<HTMLElement>("*"))) {
        const text = (el.textContent ?? "").trim();
        if (!titles.includes(text)) continue;

        // Only the innermost element holding the title. Without this every
        // ancestor up to <main> matches and one decoy is reported eight times.
        if (Array.from(el.children).some((c) => (c.textContent ?? "").trim() === text)) {
          continue;
        }

        // Invisible text is not a decoy — nobody is trying to press it.
        const box = el.getBoundingClientRect();
        if (box.width === 0 && box.height === 0) continue;

        // Declared status readouts, which name a track without offering it.
        if (exempt.some((sel) => el.closest(sel))) continue;

        if (!el.closest(operable)) {
          const cls = el.className || "(no class)";
          found.push(
            `"${text}" in <${el.tagName.toLowerCase()} class="${cls}"> is not inside anything pressable`,
          );
        }
      }
      return found;
    },
    { titles: TITLES, operable: OPERABLE, exempt: displayOnly },
  );
}

/**
 * Whether the control for `title` can be reached by keyboard at all.
 *
 * Either it is a tab stop itself, or it is an item inside a composite widget
 * that is one. Anything else is mouse-only, however well it clicks.
 */
async function keyboardReachable(page: Page, title: string): Promise<string> {
  return page.evaluate(
    ({ title, operable, composite }) => {
      const main = document.querySelector("main");
      if (!main) return "no <main>";

      const control = Array.from(main.querySelectorAll<HTMLElement>(operable)).find(
        (el) => (el.textContent ?? "").includes(title),
      );
      if (!control) return `no operable control contains "${title}"`;

      if (control.tabIndex >= 0) return "ok";

      const owner = control.closest<HTMLElement>(composite);
      if (owner && owner.tabIndex >= 0) return "ok";

      return `the control for "${title}" is not a tab stop and is not inside a focusable ${
        owner ? owner.getAttribute("role") : "composite widget"
      }`;
    },
    { title, operable: OPERABLE, composite: COMPOSITE },
  );
}

/**
 * Each screen that lists tracks.
 *
 * `reach` leaves it open with tracks on it. `activate` presses a track with
 * the gesture a person would use on that screen. `byKeyboard` does the same
 * with keys only. `displayOnly` names regions that state what is playing
 * rather than offering it — each one needs a reason, because this list is
 * where a genuine decoy would go to hide.
 */
const SCREENS: {
  name: string;
  reach: (page: Page) => Promise<void>;
  activate: (page: Page) => Promise<void>;
  byKeyboard: (page: Page) => Promise<void>;
  plays: string;
  displayOnly?: string[];
}[] = [
  {
    name: "Songs",
    reach: async (page) => {
      await openSongs(page);
      await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();
    },
    // One click, the same as everywhere else now: the table used to select on
    // a plain click and play only on a double.
    activate: async (page) => {
      await page.getByText("Xtal", { exact: true }).click();
    },
    // The listbox pattern: focus the table once, drive it with arrows, Enter
    // plays the row under the cursor (TD-33).
    byKeyboard: async (page) => {
      await page.getByRole("listbox", { name: /tracks/i }).focus();
      await page.keyboard.press("End");
      await page.keyboard.press("Enter");
    },
    // Sorted by title ascending, so End lands on Xtal.
    plays: "Xtal",
  },
  {
    name: "Vibe (queue)",
    reach: async (page) => {
      // Filled from Songs rather than from Library, so a broken Library grid
      // fails Library's own tests and not this screen's too. A cascade of
      // failures across four screens hides which one is actually wrong.
      await openSongs(page);
      await page.getByText("Windowlicker", { exact: true }).click();
      await page.getByRole("button", { name: "Vibe DJ", exact: true }).click();
      // Scoped to a queue row: the blend panel above names the incoming track
      // too, so a bare text lookup matches twice.
      await expect(page.locator(".queue__row-title", { hasText: "Xtal" })).toBeVisible();
    },
    activate: async (page) => {
      await page.locator(".queue__row-title", { hasText: "Xtal" }).click();
    },
    byKeyboard: async (page) => {
      await control(page, "Xtal").focus();
      await page.keyboard.press("Enter");
    },
    plays: "Xtal",
    displayOnly: [
      // The header says what is playing right now: a status line, the same as
      // the transport's. Pressing the thing you are already hearing is not a
      // gesture, and there is a row for that track in the list below it.
      ".queue__now",
      // The blend panel names the outgoing and incoming tracks as a readout of
      // the mix about to happen. Both are rows in the queue underneath.
      ".vibe__blend",
    ],
  },
  {
    name: "Playlist",
    reach: async (page) => {
      await page.getByRole("button", { name: /new playlist/i }).click();
      await page.getByPlaceholder(/playlist name/i).fill("Affordances");
      await page.getByPlaceholder(/playlist name/i).press("Enter");

      await openSongs(page);
      await page.getByRole("checkbox", { name: /select windowlicker/i }).click();
      await page.getByRole("combobox").selectOption({ label: "Affordances" });

      await page.getByRole("button", { name: /affordances/i }).click();
      await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();
    },
    activate: async (page) => {
      await page.getByText("Windowlicker", { exact: true }).click();
    },
    byKeyboard: async (page) => {
      await control(page, "Windowlicker").focus();
      await page.keyboard.press("Enter");
    },
    plays: "Windowlicker",
  },
];

for (const screen of SCREENS) {
  test.describe(screen.name, () => {
    test("every track on it can be pressed", async ({ page }) => {
      await boot(page);
      await screen.reach(page);

      expect(await decoys(page, screen.displayOnly ?? [])).toEqual([]);
    });

    test("pressing a track plays it", async ({ page }) => {
      await boot(page);
      await screen.reach(page);

      await screen.activate(page);

      // The transport is outside the screen that started playback, so this
      // also proves the two agree about what is happening.
      await expect(transportTitle(page)).toHaveText(screen.plays);
    });

    test("a track can be reached by keyboard", async ({ page }) => {
      await boot(page);
      await screen.reach(page);

      expect(await keyboardReachable(page, screen.plays)).toBe("ok");
    });

    test("the keyboard alone can play a track", async ({ page }) => {
      await boot(page);
      await screen.reach(page);

      await screen.byKeyboard(page);

      await expect(transportTitle(page)).toHaveText(screen.plays);
    });
  });
}

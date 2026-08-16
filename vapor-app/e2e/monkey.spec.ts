/**
 * The monkey: a random walk through the whole app.
 *
 * Journey tests check the paths someone thought of. This checks the ones
 * nobody did — clicking the delete button while a rename is open, typing into
 * a box that is about to unmount, pressing every key on a virtualised table,
 * navigating away mid-request. Those are where React apps actually break, and
 * no amount of writing scenarios by hand covers them.
 *
 * ## Seeded, so a failure is a bug report rather than a rumour
 *
 * The sequence comes from a seeded generator, and the seed is printed on
 * failure along with the actions taken. An unreproducible monkey failure gets
 * dismissed as flake, which makes the whole exercise worthless; a reproducible
 * one is just a test with an ugly name.
 *
 * ## What counts as a failure
 *
 * Not "the app did something odd" — a monkey cannot judge that. Three things
 * that are unambiguous:
 *
 * 1. An uncaught exception or an unhandled rejection.
 * 2. The app rendering nothing at all, which is what a crashed React tree
 *    looks like from outside.
 * 3. A spinner still going long after everything settled, which is the shape
 *    of a request whose failure was swallowed.
 */
import { expect, test, type Page } from "@playwright/test";

/** Actions per run. Enough to reach deep states, short enough for CI. */
const ACTIONS = 250;

/**
 * Seeds. Fixed rather than random, so the suite is deterministic — a monkey
 * that picks its own seed reports a different failure on every run and can
 * never be said to have passed.
 *
 * Add a seed here when one finds a bug, so the case stays covered.
 */
const SEEDS = [1, 20_260_816, 99_991];

/** A small deterministic PRNG — no dependency, and reproducible from the seed. */
function rng(seed: number) {
  let state = seed >>> 0 || 1;
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    state >>>= 0;
    return state / 0xffff_ffff;
  };
}

const TYPEABLE = [
  "a",
  "test",
  "128",
  "0",
  "-1",
  "999999",
  "   ",
  "'; DROP TABLE tracks; --",
  "<script>alert(1)</script>",
  "🎵🎶",
  "ünïcödé",
  "8®",
  "a".repeat(300),
];

const KEYS = [
  "ArrowDown",
  "ArrowUp",
  "ArrowLeft",
  "ArrowRight",
  "Enter",
  "Escape",
  "Tab",
  "Space",
  "PageDown",
  "PageUp",
  "Home",
  "End",
];

for (const seed of SEEDS) {
  test(`monkey, seed ${seed}`, async ({ page }) => {
    test.setTimeout(180_000);

    const problems: string[] = [];
    page.on("pageerror", (e) => problems.push(`uncaught: ${e}`));
    page.on("console", (m) => {
      // React logs a caught render error here before the boundary takes over,
      // which is a real failure even when the page survives it.
      if (m.type() === "error" && /Uncaught|is not a function|undefined is not/.test(m.text())) {
        problems.push(`console: ${m.text()}`);
      }
    });

    await page.goto("/");
    await expect(page.getByRole("navigation")).toBeVisible();

    const random = rng(seed);
    const log: string[] = [];

    for (let i = 0; i < ACTIONS && problems.length === 0; i++) {
      const roll = random();

      try {
        if (roll < 0.45) {
          // Click something clickable, chosen at random.
          const targets = page.locator(
            'button:visible, [role=option]:visible, [role=tab]:visible, li:visible',
          );
          const count = await targets.count();
          if (count === 0) continue;
          const index = Math.floor(random() * count);
          const target = targets.nth(index);
          const label = ((await target.textContent()) ?? "").trim().slice(0, 40);
          log.push(`click "${label}"`);
          await target.click({ timeout: 2_000, force: true });
        } else if (roll < 0.65) {
          // Type into a box, including things nobody should type.
          const boxes = page.locator(
            'input:visible:not([type=range]), textarea:visible',
          );
          const count = await boxes.count();
          if (count === 0) continue;
          const box = boxes.nth(Math.floor(random() * count));
          const text = TYPEABLE[Math.floor(random() * TYPEABLE.length)]!;
          log.push(`type ${JSON.stringify(text.slice(0, 20))}`);
          await box.fill(text, { timeout: 2_000 });
        } else if (roll < 0.85) {
          const key = KEYS[Math.floor(random() * KEYS.length)]!;
          log.push(`key ${key}`);
          await page.keyboard.press(key);
        } else {
          // Double-click: opens editors and plays tracks, and is where
          // stopPropagation bugs live.
          const targets = page.locator("[role=option]:visible, h1:visible, span:visible");
          const count = await targets.count();
          if (count === 0) continue;
          await targets
            .nth(Math.floor(random() * count))
            .dblclick({ timeout: 2_000, force: true });
          log.push("dblclick");
        }
      } catch {
        // A click that misses because the element moved is the monkey being a
        // monkey, not a defect. Only the page's own errors count.
        continue;
      }
    }

    // Let anything in flight settle before judging.
    await page.waitForTimeout(1_000);

    const report = `seed ${seed}\nlast 25 actions:\n${log.slice(-25).join("\n")}`;

    expect(problems, report).toEqual([]);

    // The app is still there. A blank body is what a crashed React tree looks
    // like from outside, and it is the failure a monkey is best placed to find.
    const body = (await page.locator("body").textContent()) ?? "";
    expect(body.trim().length, `the app rendered nothing.\n${report}`).toBeGreaterThan(0);

    // Nothing is still claiming to be busy. A spinner that never stops is the
    // signature of a request whose failure was swallowed.
    const stillBusy = await page
      .getByText(/scanning…|saving…|choosing…|reading library/i)
      .count();
    expect(stillBusy, `something was still busy at the end.\n${report}`).toBe(0);
  });
}

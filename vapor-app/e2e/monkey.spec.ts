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
 *
 * ## The backend is allowed to fail
 *
 * It was not, until 2026-08-23, and that made the exercise much weaker than it
 * looked (AUD-9). Every command the monkey reached answered, so the whole error
 * half of the app — every notice, every retry, every "could not" branch — was
 * unreachable by a random walk. The first defect that ever reached an outside
 * user was an error bar that could not be dismissed, which is exactly the shape
 * of thing this is for and exactly what it could not have found.
 *
 * Failures are armed from the same seeded generator as the actions, so a run
 * that breaks stays reproducible: seed N fails the same commands at the same
 * points every time. Nothing is added to `src/test/ipc.ts` to do it — the fake
 * already has `fail`/`clearFailure`, and CLAUDE.md names that file a seam that
 * holds answers rather than decisions.
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
const SEEDS = [1, 20_260_816, 99_991, 4_242_424];

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

/**
 * Commands worth breaking, and why these.
 *
 * Every one is reached by something a person clicks, and every one has a
 * failure the UI is supposed to say something about. Read-only queries that
 * merely repaint are left out: a failed `track_thumb` is a missing picture, not
 * a state the app has to explain.
 *
 * `settings` is deliberately absent. It is fetched on boot and the app has
 * nowhere to go without it, so failing it tests the boundary rather than the
 * app.
 */
const BREAKABLE = [
  "scan_library",
  "create_playlist",
  "delete_playlist",
  "rename_playlist",
  "create_group",
  "delete_group",
  "add_to_group",
  "set_remote_config",
  "save_webdav_password",
  "identify_library",
  "add_local_folder",
  "remove_local_folder",
  "download_collection",
  "remove_download",
  "sync_with",
  "pair_with",
  "find_album_art",
  "look_up_track",
  "choose_next",
  "set_curve",
];

/** What a broken command says. Realistic shapes, not lorem. */
const BREAKAGES = [
  "the server closed the connection",
  "no route to host",
  "permission denied",
  "the disk is full",
  "",
];

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

    /**
     * Break a command for a while, then mend it.
     *
     * Armed and cleared from `random()` like everything else here, so the run
     * stays reproducible from the seed alone. A window rather than a permanent
     * break: the app has to survive the failure *and* come right again, and a
     * command that never recovers would just wedge the walk in one screen.
     */
    let broken: string | null = null;
    let mendAt = 0;

    for (let i = 0; i < ACTIONS && problems.length === 0; i++) {
      if (broken && i >= mendAt) {
        const mending = broken;
        await page.evaluate((c) => window.__vaporBackend.clearFailure(c), mending);
        log.push(`(${mending} works again)`);
        broken = null;
      }
      if (!broken && random() < 0.12) {
        const cmd = BREAKABLE[Math.floor(random() * BREAKABLE.length)]!;
        const why = BREAKAGES[Math.floor(random() * BREAKAGES.length)]!;
        await page.evaluate(
          ([c, m]) => window.__vaporBackend.fail(c!, m!),
          [cmd, why] as const,
        );
        broken = cmd;
        mendAt = i + 5 + Math.floor(random() * 20);
        log.push(`(${cmd} now fails: ${JSON.stringify(why)})`);
      }

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

      /*
       * An error notice must offer a way out.
       *
       * Checked here, straight after the action, because that is when a notice
       * has just been rendered — before the next action navigates past it. This
       * is the defect the ticket came from: the first thing to reach an outside
       * user was an error bar that could not be dismissed.
       *
       * Opportunistic, and proven so rather than assumed. Deleting the Dismiss
       * control from `ErrorNotice` and running all four seeds does NOT turn this
       * red: notices are rare in a random walk — sixteen injected failures on
       * seed 1 produced one notice on screen — and removing a button changes
       * which elements the monkey clicks, so the walk diverges and may produce
       * none at all.
       *
       * So this is a bonus, not the guard. The guard is deterministic and lives
       * in `src/components/ErrorNotice.test.tsx`, which does go red under that
       * same deletion. What is load-bearing here is the injection above, which
       * makes the app's whole error half reachable at all.
       */
      const stuck = await page.evaluate(
        () =>
          [...document.querySelectorAll(".notice--error")].filter(
            (n) => !n.querySelector('[aria-label="Dismiss"]'),
          ).length,
      );
      if (stuck > 0) {
        log.push(`!! error notice with no way out (${stuck})`);
        problems.push(`an error notice with no way to dismiss it (${stuck})`);
      }
    }

    // Mend whatever was still broken before judging. What is under test past
    // this point is whether the app recovers, and a command left failing would
    // make a legitimately-still-loading screen look like a swallowed error.
    if (broken) {
      const mending = broken;
      await page.evaluate((c) => window.__vaporBackend.clearFailure(c), mending);
    }

    // Let anything in flight settle before judging.
    await page.waitForTimeout(1_000);

    const report = `seed ${seed}\nlast 25 actions:\n${log.slice(-25).join("\n")}`;

    expect(problems, report).toEqual([]);

    /*
     * Criterion 3, which this file has described since it was written and never
     * actually checked.
     *
     * `Loading` is the app's only spinner. A second after everything settled and
     * with nothing left failing, one still on screen means a request whose
     * failure went nowhere — the screen is waiting for an answer that is never
     * coming, and says so to nobody.
     *
     * Narrow, and worth saying so: `Loading` has exactly one call site today
     * (`LinerNotes`), so this covers one screen rather than the app. It widens
     * on its own as the component gets used, which is the argument for keying
     * on the component rather than on a list of screens.
     */
    const stillLoading = page.locator(".state__title", { hasText: /…$/ });
    expect(
      await stillLoading.count(),
      `a spinner outlived the run, which is what a swallowed failure looks ` +
        `like.\n${report}`,
    ).toBe(0);

    /*
     * And the same thing again for real, on whatever is left standing.
     *
     * The per-step check above says a Dismiss control exists; this says pressing
     * it works. Only reached when a notice outlives the whole walk, which is
     * uncommon — it is the cheap half of the pair, not the load-bearing one.
     */
    const dismiss = page.getByRole("button", { name: "Dismiss" });
    for (let i = await dismiss.count(); i > 0; i--) {
      await dismiss.first().click({ timeout: 2_000 });
    }
    expect(
      await dismiss.count(),
      `an error notice would not dismiss.\n${report}`,
    ).toBe(0);

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

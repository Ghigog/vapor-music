/**
 * Booting the app against the IPC fake.
 *
 * Shared rather than copied. Every spec here needs the same two things — the
 * backend in a chosen state, and a way to tell the app has finished starting —
 * and each file had grown its own slightly different copy of both.
 */
import { expect, type Page } from "@playwright/test";

/**
 * Start with the backend in a chosen state.
 *
 * The options are planted before the first navigation rather than applied
 * after it. `window.__vaporReset` writes them to `sessionStorage` and reloads,
 * so a spec that called it after `goto` loaded the app twice — once against
 * defaults, thrown away, then again for real — and the wait below raced that
 * second load. Under four parallel workers it lost, and "Going back" failed on
 * a `<main>` that was still navigating. One navigation cannot race itself.
 *
 * `addInitScript` runs ahead of the page's own scripts on each document load,
 * writing the same key the fake reads at construction, so the backend is built
 * with these options the first time instead of being rebuilt with them.
 */
export async function boot(page: Page, options: Record<string, unknown> = {}) {
  await page.addInitScript((o) => {
    sessionStorage.setItem("vapor:fake-options", JSON.stringify(o));
  }, options);
  await page.goto("/");
  // Wait for the content column rather than the sidebar: onboarding takes the
  // whole window with no sidebar at all, so waiting for the nav hangs on
  // exactly the first-run case some of these tests exist to cover.
  await expect(page.getByRole("main")).toBeVisible();
}

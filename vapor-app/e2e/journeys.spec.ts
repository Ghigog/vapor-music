/**
 * Whole journeys through the real UI.
 *
 * The component tests render one screen at a time. These drive the app as a
 * person does — across screens, in order, with the state each step leaves
 * behind — which is where the seams are. Every defect that reached Dylan was
 * found this way, by hand, and none of them were reproducible from a single
 * screen in isolation.
 */
import { expect, test, type Page } from "@playwright/test";

/** Start with the backend in a chosen state. Options survive the reload. */
async function boot(page: Page, options: Record<string, unknown> = {}) {
  await page.goto("/");
  await page.evaluate((o) => window.__vaporReset(o), options);
  // Wait for the content column rather than the sidebar: onboarding takes the
  // whole window with no sidebar at all, so waiting for the nav hangs on
  // exactly the first-run case these tests exist to cover.
  await expect(page.getByRole("main")).toBeVisible();
}

test.describe("A first run", () => {
  test("connect, scan, analyse, play", async ({ page }) => {
    await boot(page, { connected: false });

    // Onboarding hands off to Settings rather than asking for details itself.
    await page.getByRole("button", { name: /choose where music lives/i }).click();

    await page.getByLabel("Server address", { exact: true }).fill("https://app.koofr.net");
    await page.getByLabel("Username", { exact: true }).fill("someone@example.com");
    await page.getByLabel("Password", { exact: true }).fill("an-app-password");
    await page.getByLabel("Folder", { exact: true }).fill("/dav/Koofr/Music");

    // Scan applies the form first, so this is one press rather than two.
    await page.getByRole("button", { name: /scan library/i }).click();
    await expect(page.getByText(/found 4 tracks/i)).toBeVisible();

    await page.getByRole("button", { name: /^analyse$/i }).click();

    await page.getByRole("button", { name: /^songs$/i }).click();
    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();

    await page.getByText("Windowlicker", { exact: true }).dblclick();

    // The transport is outside the content column and outlives the screen that
    // started playback, which is the point of it being there.
    await expect(page.getByRole("button", { name: /^pause$/i })).toBeVisible();
  });

  test("a wrong server address is refused with an explanation", async ({ page }) => {
    await boot(page, { connected: false });

    await page.getByRole("button", { name: /choose where music lives/i }).click();
    // A Koofr app password in the address field — the mistake that happens.
    await page.getByLabel("Server address", { exact: true }).fill("4wg9ie7xi8v7nbi6");
    await page.getByLabel("Username", { exact: true }).fill("someone@example.com");
    await page.getByRole("button", { name: /^save$/i }).click();

    await expect(page.getByRole("alert")).toContainText(/not a server address/i);
  });
});

test.describe("Building a playlist", () => {
  test("make one, add to it from the table, play it", async ({ page }) => {
    await boot(page);

    await page.getByRole("button", { name: /new playlist/i }).click();
    await page.getByPlaceholder(/playlist name/i).fill("Late Night");
    await page.getByPlaceholder(/playlist name/i).press("Enter");

    // Creating opens it, and it starts empty with an explanation.
    await expect(page.getByText(/nothing in here yet/i)).toBeVisible();

    await page.getByRole("button", { name: /^songs$/i }).click();
    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();

    // Select two rows and add them through the selection bar.
    await page.getByText("Windowlicker", { exact: true }).click({ modifiers: ["Meta"] });
    await page.getByText("Xtal", { exact: true }).click({ modifiers: ["Meta"] });
    await page.getByRole("combobox").selectOption({ label: "Late Night" });

    // The rail's count is the visible proof it landed.
    await expect(page.getByRole("button", { name: /late night/i })).toContainText("2");

    await page.getByRole("button", { name: /late night/i }).click();
    // Scoped to the screen: the transport has a Play button too.
    await page.getByRole("main").getByRole("button", { name: /^play$/i }).click();
    await expect(page.getByRole("button", { name: /^pause$/i })).toBeVisible();
  });

  test("a playlist can be renamed and deleted", async ({ page }) => {
    await boot(page);

    await page.getByRole("button", { name: /new playlist/i }).click();
    await page.getByPlaceholder(/playlist name/i).fill("Temporary");
    await page.getByPlaceholder(/playlist name/i).press("Enter");

    await page.getByRole("heading", { name: "Temporary" }).dblclick();
    await page.getByRole("textbox").fill("Renamed");
    await page.getByRole("textbox").press("Enter");
    await expect(page.getByRole("heading", { name: "Renamed" })).toBeVisible();

    await page.getByRole("button", { name: /delete the playlist/i }).click();
    await expect(page.getByRole("button", { name: /renamed/i })).toHaveCount(0);
  });
});

test.describe("Correcting a tempo", () => {
  /**
   * The gesture has to survive the first click's side effects.
   *
   * Selecting a row used to push the whole table down, because the selection
   * bar was inserted above it in the flow. The row moved out from under the
   * pointer between the two clicks, so the second one landed on the bar and no
   * `dblclick` ever reached the cell — double-clicking a BPM cell opened the
   * "Add to…" dropdown instead of the editor, and the tempo correction was
   * unreachable by its own gesture.
   *
   * Invisible to the component tests: jsdom has no layout, so nothing moves.
   */
  test("a hand-typed BPM is accepted and marked as a correction", async ({ page }) => {
    await boot(page);

    await page.getByRole("button", { name: /^songs$/i }).click();
    const row = page.getByRole("option").filter({ hasText: "Not Yet Analysed" });
    await expect(row).toBeVisible();

    await row.getByTitle(/correct the tempo/i).dblclick();
    await row.getByRole("textbox").fill("128");
    await row.getByRole("textbox").press("Enter");

    await expect(row.getByText("128")).toBeVisible();
  });

  /** The selecting click must leave the rows where they were. */
  test("selecting a row does not move the table", async ({ page }) => {
    await boot(page);
    await page.getByRole("button", { name: "Songs", exact: true }).click();

    const row = page.getByRole("option").filter({ hasText: "Roygbiv" });
    await expect(row).toBeVisible();
    const before = await row.boundingBox();

    await row.click();
    await expect(page.getByText(/1 selected/i)).toBeVisible();

    const after = await row.boundingBox();
    expect(after?.y).toBeCloseTo(before?.y ?? 0, 0);
  });
});

test.describe("Every screen opens", () => {
  const screens = [
    "Library",
    "Songs",
    "Search",
    "Now Playing",
    "Queue",
    "Vibe DJ",
    "Your Data",
    "Settings",
  ];

  for (const name of screens) {
    test(`${name} renders without an error`, async ({ page }) => {
      const failures: string[] = [];
      page.on("pageerror", (e) => failures.push(String(e)));

      await boot(page);
      await page.getByRole("button", { name, exact: true }).click();

      // Something rendered, and nothing threw on the way.
      await expect(page.getByRole("main")).not.toBeEmpty();
      expect(failures).toEqual([]);
    });
  }
});

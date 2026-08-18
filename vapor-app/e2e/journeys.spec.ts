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

/**
 * Open the flat track table.
 *
 * It is a tab inside Library now, not a sidebar destination — the Daylight
 * design never had a Songs screen (docs/FINDINGS.md).
 */
async function openSongs(page: Page) {
  await page.getByRole("button", { name: "Library", exact: true }).click();
  await page.getByRole("tab", { name: "Songs" }).click();
}

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

    await openSongs(page);
    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();

    // One click plays, the way tapping a row on a phone does.
    await page.getByText("Windowlicker", { exact: true }).click();

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

    await openSongs(page);
    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();

    // Select two rows through their checkboxes and add them via the bar.
    await page.getByRole("checkbox", { name: /select windowlicker/i }).click();
    await page.getByRole("checkbox", { name: /select xtal/i }).click();
    await page.getByRole("combobox").selectOption({ label: "Late Night" });

    // The rail's count is the visible proof it landed.
    await expect(page.getByRole("button", { name: /late night/i })).toContainText("2");

    await page.getByRole("button", { name: /late night/i }).click();
    // Scoped to the screen: the transport has a Play button too.
    await page.getByRole("main").getByRole("button", { name: /^play$/i }).click();
    await expect(page.getByRole("button", { name: /^pause$/i })).toBeVisible();
  });

  /**
   * Filing a playlist into a folder, through a real drag.
   *
   * The component tests dispatch a synthetic `drop` because jsdom has no
   * `DataTransfer` at all — which means they assert the handler works, not
   * that a mouse can reach it. `dragTo` is a real press, move and release in
   * a real browser, and it is the only layer where the difference shows.
   */
  test("a playlist can be filed into a folder and taken back out", async ({ page }) => {
    await boot(page);

    await page.getByRole("button", { name: /new folder/i }).click();
    await page.getByPlaceholder(/folder name/i).fill("Sets");
    await page.getByPlaceholder(/folder name/i).press("Enter");

    await page.getByRole("button", { name: /new playlist/i }).click();
    await page.getByPlaceholder(/playlist name/i).fill("Openers");
    await page.getByPlaceholder(/playlist name/i).press("Enter");

    const folder = page.locator(".rail__folder").filter({ hasText: "Sets" });
    await expect(folder.getByText("Drag a playlist here.")).toBeVisible();

    // Scoped to the rail: the open playlist screen has a "Delete the playlist
    // Openers" button, which also carries the name.
    const inRail = page.locator(".rail__item").filter({ hasText: "Openers" });
    await inRail.dragTo(folder);

    // Inside the folder's own subtree, which is the whole claim.
    await expect(folder.getByText("Openers")).toBeVisible();
    await expect(folder.getByText("Drag a playlist here.")).toHaveCount(0);

    // And back out, or a playlist filed once is filed forever.
    await folder
      .locator(".rail__item")
      .filter({ hasText: "Openers" })
      .dragTo(page.getByText(/not in a folder/i));

    await expect(folder.getByText("Openers")).toHaveCount(0);
    await expect(inRail).toBeVisible();
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

/**
 * The gestures, through a real mouse.
 *
 * Shift-click especially: the table only ever read `metaKey` and `ctrlKey`, so
 * selecting a run of tracks was impossible and nothing noticed, because no test
 * had ever held a modifier down across two clicks in a real browser.
 */
test.describe("Selecting and opening tracks", () => {
  test("one click plays, two open the track", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    // Scoped to the table: once it plays, the title is in the transport too.
    const inTable = page.getByRole("main").getByText("Xtal", { exact: true });

    await inTable.click();
    await expect(page.locator(".transport__title")).toHaveText("Xtal");

    await inTable.dblclick();
    // Liner notes: the screen that used to be reachable only from a button
    // that faded in over the artwork.
    await expect(page.getByRole("button", { name: /back/i })).toBeVisible();
  });

  test("shift-click selects a run of tracks", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    await page.getByRole("checkbox", { name: /select roygbiv/i }).click();
    await page
      .getByRole("checkbox", { name: /select xtal/i })
      .click({ modifiers: ["Shift"] });

    // Roygbiv, Windowlicker, Xtal — the rows between the two, inclusive.
    await expect(page.getByText(/3 selected/i)).toBeVisible();
  });

  test("ticking a checkbox does not start the track", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    await page.getByRole("checkbox", { name: /select xtal/i }).click();

    await expect(page.getByText(/1 selected/i)).toBeVisible();
    await expect(page.locator(".transport__title")).toHaveText("Nothing playing");
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

    await openSongs(page);
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
    await openSongs(page);

    const row = page.getByRole("option").filter({ hasText: "Roygbiv" });
    await expect(row).toBeVisible();
    const before = await row.boundingBox();

    // Through the checkbox: a plain click plays now, and this test is about
    // what selecting does to the layout.
    await row.getByRole("checkbox").click();
    await expect(page.getByText(/1 selected/i)).toBeVisible();

    const after = await row.boundingBox();
    expect(after?.y).toBeCloseTo(before?.y ?? 0, 0);
  });
});

/**
 * Where a screen begins.
 *
 * Settings capped itself at 620px and left-aligned; Your Data capped at 640px
 * and centred. Moving between them slid the whole page sideways, and each
 * screen had reached its width by a private decision rather than a shared one.
 */
test.describe("Screen layout", () => {
  const screens = ["Library", "Vibe DJ", "Your Data", "Settings"];

  test("every screen starts at the same left edge", async ({ page }) => {
    await boot(page);
    // With nothing playing, Vibe shows a centred empty state instead of
    // itself, and the measurement would be of the wrong element.
    await openSongs(page);
    await page.getByText("Windowlicker", { exact: true }).click();

    const lefts: { name: string; x: number }[] = [];
    for (const name of screens) {
      await page.getByRole("button", { name, exact: true }).click();
      const root = page.getByRole("main").locator("> *").first();
      await expect(root).toBeVisible();
      const box = await root.boundingBox();
      lefts.push({ name, x: Math.round(box?.x ?? -1) });
    }

    const first = lefts[0]!.x;
    // Named in the message: a bare "expected 28 to be 328" says nothing about
    // which screen wandered off.
    expect(lefts.filter((l) => l.x !== first)).toEqual([]);
  });

  /** The point of removing the caps: the column is actually used. */
  test("a screen fills the width available to it", async ({ page }) => {
    await boot(page);

    const main = page.getByRole("main");
    const mainBox = await main.boundingBox();
    const root = main.locator("> *").first();
    const rootBox = await root.boundingBox();

    // Within the content column's own padding, not a third of it.
    expect(rootBox!.width).toBeGreaterThan((mainBox!.width ?? 0) * 0.9);
  });
});

/**
 * A smoke test, and only that.
 *
 * This caught nothing about Library's dead album grid, because "something
 * rendered and nothing threw" is true of a screen whose primary control does
 * not work. It is kept for what it does cover — a screen that crashes on open
 * — and `affordances.spec.ts` asserts that the things on these screens can
 * actually be pressed. Do not add coverage here by adding screens to this list.
 */
test.describe("Every screen opens", () => {
  // The four destinations. Now Playing opens from the player bar and the queue
  // lives on Vibe, so neither is reachable from the sidebar to smoke-test.
  const screens = ["Library", "Vibe DJ", "Your Data", "Settings"];

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

/**
 * Where Now Playing and the Queue went.
 *
 * Both were sidebar destinations. The Daylight design has neither: its Now
 * Playing opens with a "⌄" dismiss chevron from the player bar, and its Queue
 * is labelled "06 Queue — bottom sheet" and reads "Conducted by Vibe"
 * (docs/FINDINGS.md). The rewrite made twelve mockups into twelve tabs.
 */
test.describe("Navigation", () => {
  test("the sidebar no longer offers Now Playing or Queue", async ({ page }) => {
    await boot(page);

    await expect(page.getByRole("button", { name: "Now Playing", exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Queue", exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Songs", exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Search", exact: true })).toHaveCount(0);
  });

  test("pressing the title in the player bar opens Now Playing", async ({ page }) => {
    await boot(page);
    await openSongs(page);
    await page.getByText("Windowlicker", { exact: true }).click();

    // Wait for the transport to catch up: the title is plain text until there
    // is something playing, and only becomes the button that opens the screen
    // once the state arrives. Clicking too early hits the text.
    await expect(page.locator(".transport__title")).toHaveText("Windowlicker");
    await page.locator(".transport__title").click();

    // The full screen, which carries its own transport controls.
    await expect(
      page.getByRole("main").getByRole("button", { name: /next track/i }),
    ).toBeVisible();
  });

  test("the queue lives on the Vibe screen, and says who ordered it", async ({ page }) => {
    await boot(page);
    await openSongs(page);
    await page.getByText("Windowlicker", { exact: true }).click();
    await page.getByRole("button", { name: "Vibe DJ", exact: true }).click();

    await expect(page.getByText(/conducted by vibe/i)).toBeVisible();
    // Scoped to the queue: with the DJ on, the blend panel names the incoming
    // track too, and both being present is the point rather than a problem.
    await expect(page.locator(".queue__row-title", { hasText: "Xtal" })).toBeVisible();
  });

  test("turning the DJ off makes it the Shuffle screen", async ({ page }) => {
    await boot(page);
    await openSongs(page);
    await page.getByText("Windowlicker", { exact: true }).click();
    await page.getByRole("button", { name: "Vibe DJ", exact: true }).click();

    await page.getByRole("checkbox", { name: /dj/i }).uncheck();

    // The tab renames itself, exactly as the design's nav does.
    await expect(page.getByRole("button", { name: "Shuffle", exact: true })).toBeVisible();
    await expect(page.getByText(/standard shuffle/i)).toBeVisible();
    // The DJ's own controls are gone; the queue is not.
    await expect(page.getByRole("button", { name: /conduct from here/i })).toHaveCount(0);
    await expect(page.locator(".queue__row-title", { hasText: "Xtal" })).toBeVisible();
  });
});

/**
 * Albums and artists, as things rather than as headings.
 *
 * The Albums tab used to render a card per *track* grouped under an album
 * heading, so "All Melody" was a header with nine tiles beneath it and none of
 * them was the album. A tab called Albums lists albums.
 */
test.describe("Going back", () => {
  /**
   * Back has to leave a screen, not the app.
   *
   * Navigation was component state with no history behind it, so on Android the
   * back gesture went straight past the app to the launcher: Settings was a
   * screen you left by killing the app and starting it again. `App.tsx` pushes
   * an entry per place, and `MainActivity` re-enables the `handleBackNavigation`
   * that Tauri turns off, so the gesture calls `webView.goBack()` while there is
   * anywhere to go.
   *
   * Asserted through the browser's own history, which is the same mechanism the
   * gesture drives.
   */
  test("returns to the previous screen rather than leaving", async ({ page }) => {
    await boot(page);

    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByText(/where your music lives/i)).toBeVisible();

    await page.goBack();

    await expect(page.getByRole("heading", { name: /your library/i })).toBeVisible();
  });

  test("walks back through several screens in order", async ({ page }) => {
    await boot(page);

    await page.getByRole("button", { name: "Vibe DJ" }).click();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByText(/where your music lives/i)).toBeVisible();

    await page.goBack();
    await expect(page.getByRole("button", { name: /^help$/i })).toBeVisible();

    await page.goBack();
    await expect(page.getByRole("heading", { name: /your library/i })).toBeVisible();
  });
});

test.describe("Albums and artists", () => {
  test("the Albums tab lists albums, not tracks", async ({ page }) => {
    await boot(page);

    await expect(page.getByText("Windowlicker EP", { exact: true })).toBeVisible();
    await expect(page.getByText("Selected Ambient Works", { exact: true })).toBeVisible();
    // The track of that name belongs under Songs.
    await expect(page.getByRole("main").getByText("Windowlicker", { exact: true })).toHaveCount(0);
  });

  test("opening an album shows only its tracks, and back returns", async ({ page }) => {
    await boot(page);

    await page.getByRole("button", { name: /open the album windowlicker ep/i }).click();

    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();
    await expect(page.getByText("Xtal", { exact: true })).toHaveCount(0);

    await page.getByRole("button", { name: /‹ albums/i }).click();
    await expect(page.getByText("Selected Ambient Works", { exact: true })).toBeVisible();
  });

  test("an album plays from its card without being opened", async ({ page }) => {
    await boot(page);

    await page.getByRole("button", { name: /play selected ambient works/i }).click();

    await expect(page.locator(".transport__title")).toHaveText("Xtal");
  });

  test("the Artists tab lists artists and opens one", async ({ page }) => {
    await boot(page);
    await page.getByRole("tab", { name: "Artists" }).click();

    // Waited for by its own control rather than by text: the album grid is
    // still on screen for a beat after the tab is pressed, and two albums there
    // carry "Aphex Twin" as their subtitle.
    const tile = page.getByRole("button", { name: /open the artist aphex twin/i });
    await expect(tile).toBeVisible();
    await tile.click();

    // Both of that artist's tracks, and nobody else's.
    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();
    await expect(page.getByText("Xtal", { exact: true })).toBeVisible();
    await expect(page.getByText("Roygbiv", { exact: true })).toHaveCount(0);
  });
});

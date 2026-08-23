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
import { boot } from "./harness";

/*
 * What is deliberately *not* here.
 *
 * These tests drive the same React app against the same fake IPC as the
 * component suite, in a real browser. That is worth paying for when the thing
 * under test needs a real browser — geometry, scroll position, touch input, the
 * history stack — and is pure cost when it does not: the component suite runs
 * the whole of itself in about seven seconds, against minutes here.
 *
 * So six tests were removed rather than rewritten, each already covered:
 *
 *   - "one click plays, two open the track"
 *     covered by Songs.test.tsx — 'plays the track that was clicked once' and 'opens the track on a double click'
 *   - "shift-click selects a run of tracks"
 *     covered by Songs.test.tsx — 'extends the selection with shift, across a range of rows'
 *   - "ticking a checkbox does not start the track"
 *     covered by Songs.test.tsx — 'selects with the checkbox without playing anything'
 *   - "a playlist can be filed into a folder and taken back out"
 *     covered by Playlist.test.tsx — 'files a playlist into a folder when it is dragged onto one' and 'takes a playlist back out again'
 *   - "a playlist can be renamed and deleted"
 *     covered by Playlist.test.tsx — 'renames on a double-click' and 'deletes, and tells the shell there is nothing left to show'
 *   - "a hand-typed BPM is accepted and marked as a correction"
 *     covered by Songs.test.tsx — 'accepts a hand-typed BPM'
 *
 * Add a journey here when it crosses screens or needs the browser. Add it to
 * the component suite when it does not.
 */

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


test.describe("A first run", () => {
  test("connect, scan, analyse, play", async ({ page }) => {
    await boot(page, { connected: false });

    // Onboarding offers two ways in. This journey is the server one, which
    // still hands off to Settings rather than asking for details itself; the
    // folder path finishes on the onboarding screen and never comes here.
    await page.getByRole("button", { name: /connect a server instead/i }).click();

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

  /*
   * Whether an address is an address is the backend's ruling, not this file's.
   * The fake used to restate the rule with its own regex and the two came
   * apart twice, so it stopped (eb5fa70) and the refusal is arranged here.
   *
   * What the journey still owns is everything around that ruling: onboarding
   * hands off to Settings, the address the owner typed is sent, and the reason
   * it comes back with reaches the screen instead of being swallowed. Arranging
   * the refusal tests that path; computing it here would only test a regex.
   */
  test("a wrong server address is refused with an explanation", async ({ page }) => {
    await boot(page, { connected: false });
    await page.evaluate(() =>
      window.__vaporBackend.fail(
        "set_remote_config",
        '"https://4wg9ie7xi8v7nbi6" is not a server address',
      ),
    );

    await page.getByRole("button", { name: /connect a server instead/i }).click();
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


});

/**
 * The gestures, through a real mouse.
 *
 * Shift-click especially: the table only ever read `metaKey` and `ctrlKey`, so
 * selecting a run of tracks was impossible and nothing noticed, because no test
 * had ever held a modifier down across two clicks in a real browser.
 */
test.describe("Selecting and opening tracks", () => {


});

test.describe("Correcting a tempo", () => {

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
/**
 * Go to a screen, whichever way it is reached.
 *
 * Settings stopped being a nav destination — it is the bubble in the corner —
 * and Your Data stopped being a screen at all: it is a section at the bottom
 * of Settings. Tests that walked the nav for all four had to learn both.
 */
async function goTo(page: Page, name: string) {
  if (name === "Settings") {
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    return;
  }
  await page.getByRole("button", { name, exact: true }).click();
}

test.describe("Screen layout", () => {
  const screens = ["Library", "Vibe DJ", "Settings"];

  test("every screen starts at the same left edge", async ({ page }) => {
    await boot(page);
    // With nothing playing, Vibe shows a centred empty state instead of
    // itself, and the measurement would be of the wrong element.
    await openSongs(page);
    await page.getByText("Windowlicker", { exact: true }).click();

    /*
     * Vibe is exempt, deliberately.
     *
     * It is three cards to compare, four curve buttons and a queue, and across
     * a wide window the cards drift far enough apart that comparing them is a
     * head-turn and the buttons become metre-wide letterboxes. It is held to a
     * 640px column and centred, so it does not share a left edge with anything
     * — which is the point rather than a regression.
     */
    const flush = screens.filter((s) => s !== "Vibe DJ");

    const lefts: { name: string; x: number }[] = [];
    for (const name of flush) {
      await goTo(page, name);
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
  // Now Playing opens from the player bar and the queue lives on Vibe, so
  // neither is reachable from the nav to smoke-test. Your Data is no longer a
  // screen — it is the last section of Settings, and is covered below.
  const screens = ["Library", "Vibe DJ", "Settings"];

  for (const name of screens) {
    test(`${name} renders without an error`, async ({ page }) => {
      const failures: string[] = [];
      page.on("pageerror", (e) => failures.push(String(e)));

      await boot(page);
      await goTo(page, name);

      // Something rendered, and nothing threw on the way.
      await expect(page.getByRole("main")).not.toBeEmpty();
      expect(failures).toEqual([]);
    });
  }

  /** Your Data moved rather than went: it is the bottom of Settings now. */
  test("Your Data renders inside Settings", async ({ page }) => {
    const failures: string[] = [];
    page.on("pageerror", (e) => failures.push(String(e)));

    await boot(page);
    await goTo(page, "Settings");

    await expect(
      page.getByRole("heading", { name: /your data/i }),
    ).toBeVisible();
    // The "what Vapor never does" prose is gone, so this checks the part that
    // carries the claim instead: the breakdown of what is actually on disk,
    // and the control that empties it.
    //
    // Exact, because Settings now also has a "Music on this device" heading for
    // local folders and the loose pattern matched both.
    await expect(
      page.getByRole("heading", { name: "on this device", exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: /empty cache/i }),
    ).toBeVisible();
    expect(failures).toEqual([]);
  });
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

    // Switched from the transport, where the switch now lives — it is a
    // playback control, so it sits with the playback controls.
    await page.getByRole("button", { name: /turn vibe dj off/i }).click();

    // The tab renames itself, exactly as the design's nav does.
    await expect(page.getByRole("button", { name: "Shuffle", exact: true })).toBeVisible();
    await expect(page.getByText(/standard shuffle/i)).toBeVisible();
    // The DJ's controls are covered rather than removed, and the cover says
    // where the switch is. The queue below stays live either way.
    await expect(
      page.getByText(/please enable vibe dj in your player/i),
    ).toBeVisible();
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

    // `exact`, because the transport's DJ switch is labelled "Turn Vibe DJ
    // on" and Playwright matches names by substring — without it this reaches
    // for two buttons and takes neither.
    await page.getByRole("button", { name: "Vibe DJ", exact: true }).click();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByText(/where your music lives/i)).toBeVisible();

    await page.goBack();
    await expect(page.getByRole("button", { name: /how the dj chooses/i })).toBeVisible();

    await page.goBack();
    await expect(page.getByRole("heading", { name: /your library/i })).toBeVisible();
  });

  /**
   * The drill-down is a place too.
   *
   * Opening an album was Library's own state, which the history stack could not
   * see: back skipped straight past it and left the app, exactly as Settings
   * used to. App owns it now, so an album is an entry like any other.
   */
  test("leaves an opened album before it leaves the app", async ({ page }) => {
    await boot(page);

    await page.getByRole("tab", { name: "Albums" }).click();
    await page.getByRole("button", { name: /open the album windowlicker ep/i }).click();
    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();

    await page.goBack();

    // Back at the grid, with the other albums showing again.
    await expect(page.getByText("Selected Ambient Works", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: /‹ albums/i })).toHaveCount(0);
  });
});

/**
 * The front door.
 *
 * The library screen opens on shelves of what someone actually listens to,
 * because almost nobody arrives at their own music looking for a particular
 * record — they arrive wanting something on, and the thing they reach for is a
 * playlist they already have. What the shelves are ranked on is the backend's,
 * and what they draw is `Home.test.tsx`'s. What is only true in a real browser
 * is the wiring underneath: a tile on a shelf opens a drill-down App owns, and
 * the way back out is the history stack.
 */
test.describe("The home shelves", () => {
  test("opens on the shelves, and a playlist tile opens the playlist", async ({
    page,
  }) => {
    await boot(page, {
      playlists: [
        {
          id: "p1",
          name: "Night Drive",
          customCoverPath: "",
          tracks: ["/dav/Koofr/Music/xtal.m4a"],
          folderId: "",
        },
      ],
    });

    await expect(page.getByRole("heading", { name: "Playlists" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Smart groups" })).toBeVisible();

    await page.getByRole("button", { name: /open the playlist night drive/i }).click();
    await expect(page.getByRole("heading", { name: "Night Drive" })).toBeVisible();

    // Out again the way the back gesture goes, which is the half that only
    // exists once App owns the drill-down.
    await page.goBack();
    await expect(page.getByRole("heading", { name: "Smart groups" })).toBeVisible();
  });

  /**
   * A shelf is a row that scrolls sideways, and the page it sits on must not.
   *
   * An overflow container that reports its content width to its parent is how
   * one row of twelve albums drags the whole window with it, which is worse on
   * a phone than the thing it was trying to show.
   */
  test("scrolls sideways without taking the page with it", async ({ page }) => {
    // Eight albums, because four fit: a shelf with nothing hanging off the end
    // proves nothing about what happens when something does. The counts are
    // the shape `library_entities` answers in — see `makeEntity` in the fake.
    await boot(page, {
      albums: Array.from({ length: 8 }, (_, i) => ({
        name: `Album ${i + 1}`,
        subtitle: "Aphex Twin",
        tracks: 1,
        lead: "/dav/Koofr/Music/xtal.m4a",
        plays: 0,
        lastPlayed: 0,
      })),
    });

    const row = page.locator(".shelf__row").last();
    await expect(row).toBeVisible();
    // Wider than its box, or there is nothing to scroll and this proves
    // nothing about what happens when there is.
    const scrollable = await row.evaluate(
      (el) => el.scrollWidth > el.clientWidth + 1,
    );
    expect(scrollable).toBe(true);

    await row.evaluate((el) => el.scrollBy(400, 0));
    await expect
      .poll(() => row.evaluate((el) => el.scrollLeft))
      .toBeGreaterThan(0);

    const page_scrolls = await page.evaluate(
      () => document.documentElement.scrollWidth > window.innerWidth + 1,
    );
    expect(page_scrolls).toBe(false);
  });
});

test.describe("Albums and artists", () => {
  test("the Albums tab lists albums, not tracks", async ({ page }) => {
    await boot(page);
    // Asked for, because the screen opens on the home shelves — which draw
    // albums too, so a test that skipped this would pass without ever
    // reaching the tab it is named after.
    await page.getByRole("tab", { name: "Albums" }).click();

    await expect(page.getByText("Windowlicker EP", { exact: true })).toBeVisible();
    await expect(page.getByText("Selected Ambient Works", { exact: true })).toBeVisible();
    // The track of that name belongs under Songs.
    await expect(page.getByRole("main").getByText("Windowlicker", { exact: true })).toHaveCount(0);
  });

  /*
   * Opening an album and leaving it again.
   *
   * This used to also assert that *only* that album's tracks were listed. It
   * could only ever assert that against the fake backend these tests run on,
   * because narrowing rows to an album is `vapor_library::index` and this suite
   * never reaches Rust. The fake no longer narrows — see `src/test/ipc.ts` —
   * and the claim went with it rather than being propped up.
   *
   * What survives is what this suite can honestly witness: the drill-down
   * opens, and the way back out works in a real browser with real history.
   * That the right rows arrive is covered in `Library.test.tsx`, which asserts
   * the album reached the request, and in the backend's own tests.
   */
  test("opening an album drills in, and back returns", async ({ page }) => {
    await boot(page);
    await page.getByRole("tab", { name: "Albums" }).click();

    await page.getByRole("button", { name: /open the album windowlicker ep/i }).click();

    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: /‹ albums/i })).toBeVisible();

    await page.getByRole("button", { name: /‹ albums/i }).click();
    await expect(page.getByText("Selected Ambient Works", { exact: true })).toBeVisible();
  });

  test("an album plays from its card without being opened", async ({ page }) => {
    await boot(page);
    await page.getByRole("tab", { name: "Albums" }).click();

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

    // That it opens, and that tracks are listed. Which tracks is the backend's
    // narrowing, and this suite does not run the backend — see the note on the
    // album test above.
    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: /‹ artists/i })).toBeVisible();
  });
});

/**
 * A control must sit inside the row that holds it.
 *
 * `.setrow` was `width: 100%` with 16px of padding either side, and
 * `box-sizing` is not reset globally — only `.shell` sets it. So every settings
 * row was 32px wider than its own card, and because `.setgroup__rows` clips its
 * overflow the control at the right-hand end was sliced through rather than
 * wrapping or scrolling. The Analyse button was cut in half.
 *
 * Only measurable in a real browser, which is why it belongs here and not in
 * the component suite: jsdom gives every element a zero rect, so an overflow of
 * exactly the padding is invisible to it. The page does not scroll sideways
 * either — the clip hides that too — so the existing layout tests cannot see
 * it. What catches it is comparing the two edges.
 */
test.describe("Settings rows contain their controls", () => {
  test("no control is clipped by the row it sits in", async ({ page }) => {
    await boot(page);
    await page.getByRole("button", { name: "Settings", exact: true }).click();

    const rows = page.locator(".setrow").filter({ has: page.locator(".setrow__control") });
    const count = await rows.count();
    expect(count, "settings should have rows with controls").toBeGreaterThan(0);

    for (let i = 0; i < count; i += 1) {
      const row = rows.nth(i);
      const [rowBox, controlBox, label] = await Promise.all([
        row.boundingBox(),
        row.locator(".setrow__control").boundingBox(),
        row.locator(".setrow__title").innerText(),
      ]);
      if (!rowBox || !controlBox) continue;
      expect(
        Math.round(controlBox.x + controlBox.width),
        `the control in "${label}" runs past the right edge of its row`,
      ).toBeLessThanOrEqual(Math.round(rowBox.x + rowBox.width));
    }
  });
});

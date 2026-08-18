/**
 * The narrow layout.
 *
 * Below 768px the sidebar is hidden and the six-column track table cannot fit,
 * so the app draws itself differently. None of that was covered: the only
 * Playwright project was a 1280px desktop window, and every one of these
 * defects reached Dylan on a real phone instead.
 *
 * These run under the `narrow` project in playwright.config.ts, which uses a
 * phone preset — the touch input and `hover: none` matter as much as the width.
 */
import { expect, test, type Page } from "@playwright/test";

async function boot(page: Page, options: Record<string, unknown> = {}) {
  await page.goto("/");
  await page.evaluate((o) => window.__vaporReset(o), options);
  await expect(page.getByRole("main")).toBeVisible();
}

test.describe("Navigation exists at all", () => {
  /**
   * The defect underneath "I couldn't back out of Settings".
   *
   * `.shell__sidebar` is `display: none` below the breakpoint and nothing took
   * its place, so a phone had no navigation whatsoever. Settings was reachable
   * through onboarding and then had no exit — which is why the back-gesture fix
   * in 7c7a9ac looked like it had not worked.
   */
  test("the tab bar is present and the sidebar is not", async ({ page }) => {
    await boot(page);

    await expect(page.getByRole("navigation", { name: "Screens" })).toBeVisible();
    await expect(
      page.getByRole("navigation", { name: /screens and playlists/i }),
    ).toBeHidden();
  });

  test("every destination is reachable, and leaving Settings needs no gesture", async ({
    page,
  }) => {
    await boot(page);

    const tabs = page.getByRole("navigation", { name: "Screens" });
    await tabs.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByText(/where your music lives/i)).toBeVisible();

    // The half that was missing: a way out that is not the back gesture.
    await tabs.getByRole("button", { name: "Library", exact: true }).click();
    await expect(page.getByRole("heading", { name: /your library/i })).toBeVisible();
  });

  test("the tab bar marks where you are", async ({ page }) => {
    await boot(page);

    const tabs = page.getByRole("navigation", { name: "Screens" });
    await tabs.getByRole("button", { name: "Your Data" }).click();

    await expect(tabs.getByRole("button", { name: "Your Data" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(
      tabs.getByRole("button", { name: "Library", exact: true }),
    ).not.toHaveAttribute("aria-current", "page");
  });
});

test.describe("The track table at 412px", () => {
  async function openSongs(page: Page) {
    await page
      .getByRole("navigation", { name: "Screens" })
      .getByRole("button", { name: "Library", exact: true })
      .click();
    await page.getByRole("tab", { name: "Songs" }).click();
  }

  /**
   * The headings used to overprint into "AALRBTUIMS T": two `nowrap` controls
   * in one flex cell, overflowing it and drawing over the column beside them.
   */
  test("sorts by title and artist, and drops the columns that do not fit", async ({
    page,
  }) => {
    await boot(page);
    await openSongs(page);

    // Not anchored at the end: the sorted control carries an arrow and ", sorted
    // ascending" in its accessible name, and title is the default sort.
    await expect(page.getByRole("button", { name: /^title/i })).toBeVisible();
    await expect(page.getByRole("button", { name: /^artist$/i })).toBeVisible();

    // By cell rather than by role: a hidden control is absent from the
    // accessibility tree, so asserting a missing button is hidden would pass
    // just as well against a table that failed to render at all.
    for (const cell of [4, 5, 6]) {
      await expect(page.locator(`.songs__cell[data-cell="${cell}"]`)).toBeHidden();
    }
  });

  /**
   * Titles were clipped to "Car…" and "Filo…" — the name columns were left
   * about 80px between them once six columns had taken their share.
   */
  test("shows a whole track title instead of three characters of one", async ({
    page,
  }) => {
    await boot(page);
    await openSongs(page);

    const title = page.locator(".songrow__title").first();
    await expect(title).toBeVisible();

    // Ellipsis is invisible to `toHaveText`, so ask the element whether its
    // text actually fits the box it was given.
    const clipped = await title.evaluate(
      (el) => el.scrollWidth > el.clientWidth + 1,
    );
    expect(clipped).toBe(false);
  });

  test("shows the title and nothing else", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    await expect(page.locator(".songrow__title").first()).toBeVisible();
    // Artist, tempo, key and the sleeve are all off the row; the sleeve never
    // loaded a picture in the first place.
    for (const gone of ["__artist", "__meta", "__art", "__info"]) {
      await expect(page.locator(`.songrow${gone}`).first()).toBeHidden();
    }
  });
});

test.describe("Press and hold a track", () => {
  async function openSongs(page: Page) {
    await page
      .getByRole("navigation", { name: "Screens" })
      .getByRole("button", { name: "Library", exact: true })
      .click();
    await page.getByRole("tab", { name: "Songs" }).click();
  }

  /** The gesture: down, wait past the threshold, up. */
  async function hold(page: Page, title: string) {
    await page
      .getByRole("option")
      .filter({ hasText: title })
      .click({ delay: 700 });
  }

  /**
   * The narrow row is a title alone, so everything else it used to say has to
   * be somewhere a thumb can reach.
   */
  test("opens the facts the row no longer carries", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    await hold(page, "Windowlicker");

    const sheet = page.getByRole("dialog", { name: /about windowlicker/i });
    await expect(sheet).toBeVisible();
    await expect(sheet).toContainText("Aphex Twin");
    await expect(sheet).toContainText("Windowlicker EP");
    await expect(sheet).toContainText("124 BPM");
    await expect(sheet).toContainText("8A");
  });

  /**
   * A row plays when tapped. The click that arrives at the end of a hold must
   * not also start the track, or reading about something plays it.
   */
  test("does not also start the track", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    await hold(page, "Windowlicker");
    await expect(page.getByRole("dialog")).toBeVisible();

    await expect(page.locator(".transport__title")).toHaveText(/nothing playing/i);
  });

  test("carries the liner notes, which the row no longer offers", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    await hold(page, "Windowlicker");
    await page.getByRole("button", { name: /liner notes/i }).click();

    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(page.getByRole("button", { name: /back/i })).toBeVisible();
  });

  test("closes without a keyboard", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    await hold(page, "Windowlicker");
    await page.getByRole("button", { name: /^close$/i }).click();

    await expect(page.getByRole("dialog")).toHaveCount(0);
  });
});

test.describe("Playlists as a tab", () => {
  /**
   * The playlist rail lives in the sidebar, and a phone has no sidebar, so
   * playlists were the one thing on this screen with no way to reach them.
   * They are a Library tab now, which is where the Daylight design puts them.
   */
  test("is reachable and opens a playlist", async ({ page }) => {
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

    await page.getByRole("tab", { name: "Playlists" }).click();

    const playlist = page.getByRole("button", { name: /night drive/i });
    await expect(playlist).toBeVisible();
    await playlist.click();

    await expect(page.getByRole("heading", { name: /night drive/i })).toBeVisible();
  });

  /** Creating one has to work here too: it is the only route on a phone. */
  test("can make a playlist without a sidebar", async ({ page }) => {
    await boot(page);

    await page.getByRole("tab", { name: "Playlists" }).click();
    await page.getByRole("button", { name: /new playlist/i }).click();
    await page.getByPlaceholder(/playlist name/i).fill("Openers");
    await page.getByPlaceholder(/playlist name/i).press("Enter");

    await expect(page.getByRole("button", { name: /openers/i })).toBeVisible();
  });
});

test.describe("The Vibe screen at 412px", () => {
  /**
   * With nothing playing the screen has no exits and no curves to draw, so a
   * test that merely tolerated their absence would pass against any layout at
   * all. Start a track first.
   */
  /**
   * Five analysed tracks, because three exits need three candidates behind
   * whatever is playing and the default fixture only carries three analysed
   * rows in total — two of which the other cards take.
   */
  const LIBRARY = [
    ["/1.mp3", "Alpha", 128, "8A", "house"],
    ["/2.mp3", "Bravo", 126, "8A", "house"],
    ["/3.mp3", "Charlie", 124, "9A", "house"],
    ["/4.mp3", "Delta", 90, "2B", "ambient"],
    ["/5.mp3", "Echo", 174, "11B", "jungle"],
  ] as const;

  async function vibeWithASetRunning(page: Page) {
    await boot(page, {
      rows: LIBRARY.map(([href, title, bpm, key, genre]) => ({
        href,
        title,
        artist: "Nobody",
        album: "A Record",
        artistSource: "tag",
        albumSource: "tag",
        genre,
        bpm,
        key,
        year: 2020,
        manualPos: 0,
      })),
    });
    await page.getByRole("tab", { name: "Songs" }).click();
    await page.getByRole("option").filter({ hasText: "Alpha" }).click();
    await expect(page.locator(".transport__title")).toHaveText("Alpha");

    await page
      .getByRole("navigation", { name: "Screens" })
      .getByRole("button", { name: /vibe dj|shuffle/i })
      .click();
  }

  /**
   * Three exits are a set of three. `auto-fit` with a 120px minimum needs
   * 380px to lay them across and a phone has less, so they came out two above
   * one — which reads as the DJ having found two options, not three.
   */
  test("puts the three exits on one row", async ({ page }) => {
    await vibeWithASetRunning(page);

    const exits = page.locator(".vibe__exits > li");
    await expect(exits).toHaveCount(3);

    const tops = await exits.evaluateAll((els) =>
      els.map((e) => Math.round(e.getBoundingClientRect().top)),
    );
    expect(new Set(tops).size, `exits wrapped onto ${new Set(tops).size} rows`).toBe(1);
  });

  test("puts the four curves on one row, without their labels", async ({ page }) => {
    await vibeWithASetRunning(page);

    const curves = page.locator(".vibe__curve");
    await expect(curves).toHaveCount(4);

    const tops = await curves.evaluateAll((els) =>
      els.map((e) => Math.round(e.getBoundingClientRect().top)),
    );
    expect(new Set(tops).size, `curves wrapped onto ${new Set(tops).size} rows`).toBe(1);

    // The name is still there to be read, just not drawn.
    await expect(curves.first()).toHaveAccessibleName(/build|chill|wave|hold/i);
  });

  /** Deprecated: the curve buttons are how the vibe is changed. */
  test("has no vibe limit slider", async ({ page }) => {
    await vibeWithASetRunning(page);

    await expect(page.getByLabel(/vibe limit/i)).toHaveCount(0);
  });
});

test.describe("The layout fits the screen", () => {
  /**
   * A page wider than its viewport is the symptom every one of the above shares.
   * Checked on the screens most likely to overflow: a table, a workbench and a
   * long form.
   */
  for (const [name, open] of [
    ["Library", async (page: Page) => {}],
    [
      "Songs",
      async (page: Page) => {
        await page.getByRole("tab", { name: "Songs" }).click();
      },
    ],
    [
      "Settings",
      async (page: Page) => {
        await page
          .getByRole("navigation", { name: "Screens" })
          .getByRole("button", { name: "Settings", exact: true })
          .click();
      },
    ],
    [
      "Vibe DJ",
      async (page: Page) => {
        await page
          .getByRole("navigation", { name: "Screens" })
          .getByRole("button", { name: /vibe dj|shuffle/i })
          .click();
      },
    ],
  ] as const) {
    test(`${name} does not scroll sideways`, async ({ page }) => {
      await boot(page);
      await open(page);

      const overflow = await page.evaluate(() => {
        const el = document.documentElement;
        return el.scrollWidth - el.clientWidth;
      });
      expect(overflow).toBeLessThanOrEqual(0);
    });
  }
});

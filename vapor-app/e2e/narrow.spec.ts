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
import { boot } from "./harness";


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
    // Settings is the corner bubble now, not a tab.
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByText(/where your music lives/i)).toBeVisible();

    // The half that was missing: a way out that is not the back gesture.
    await tabs.getByRole("button", { name: "Library", exact: true }).click();
    await expect(page.getByRole("heading", { name: /your library/i })).toBeVisible();
  });

  /** It has to be there on every screen, which is the whole reason it moved. */
  test("the settings bubble is reachable from each destination", async ({ page }) => {
    await boot(page);
    const tabs = page.getByRole("navigation", { name: "Screens" });
    const bubble = page.getByRole("button", { name: "Settings", exact: true });

    for (const name of ["Library", "Vibe DJ"]) {
      await tabs.getByRole("button", { name: new RegExp(`^${name}`, "i") }).click();
      await expect(bubble).toBeVisible();
    }
  });

  /** Your Data moved into Settings rather than going away. */
  test("Your Data is the bottom of Settings", async ({ page }) => {
    await boot(page);

    await page.getByRole("button", { name: "Settings", exact: true }).click();

    await expect(
      page.getByRole("heading", { name: /^your data$/i }),
    ).toBeVisible();
  });

  test("the tab bar marks where you are", async ({ page }) => {
    await boot(page);

    const tabs = page.getByRole("navigation", { name: "Screens" });
    await tabs.getByRole("button", { name: /^vibe dj|^shuffle/i }).click();

    await expect(
      tabs.getByRole("button", { name: /^vibe dj|^shuffle/i }),
    ).toHaveAttribute("aria-current", "page");
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

test.describe("Sorting does not move the headings", () => {
  async function openSongs(page: Page) {
    await page
      .getByRole("navigation", { name: "Screens" })
      .getByRole("button", { name: "Library", exact: true })
      .click();
    await page.getByRole("tab", { name: "Songs" }).click();
  }

  /**
   * The arrow used to be rendered only on the active heading, which made that
   * heading wider than the others — so pressing one shoved its neighbours
   * sideways and the row re-laid itself out under the finger that had just
   * chosen. The slot is always there now.
   */
  test("pressing a heading leaves the others where they were", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    // By class, not by role: the accessible name gains ", sorted ascending"
    // when a heading becomes the sort, so a name-based locator stops matching
    // the very element whose position is the thing being measured.
    const headings = page.locator(".songs__col");
    const lefts = () =>
      headings.evaluateAll((els) =>
        els.map((e) => Math.round(e.getBoundingClientRect().left)),
      );

    const before = await lefts();
    expect(before.length).toBeGreaterThan(1);

    // Title holds the arrow at first; move it to Artist.
    await headings.nth(1).click();
    await expect(headings.nth(1)).toHaveAccessibleName(/sorted/i);

    expect(await lefts(), "the headings moved when one was pressed").toEqual(before);
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

test.describe("Playlists and groups open from the bar", () => {
  function tab(page: Page, name: string) {
    return page
      .getByRole("navigation", { name: "Screens" })
      .getByRole("button", { name, exact: true });
  }

  /**
   * The rail lives in the sidebar and a phone has no sidebar, so playlists were
   * unreachable. They are a tab now — and it opens a list rather than going to
   * a screen, because a playlist is something you pick, not a place the app is.
   */
  test("the playlists tab opens a list, and picking one opens it", async ({ page }) => {
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

    await tab(page, "Playlists").click();

    const menu = page.getByRole("dialog", { name: "Playlists" });
    await expect(menu).toBeVisible();
    await menu.getByRole("button", { name: /night drive/i }).click();

    await expect(menu).toHaveCount(0);
    await expect(page.getByRole("heading", { name: /night drive/i })).toBeVisible();
  });

  test("a new playlist is made and opened from the list", async ({ page }) => {
    await boot(page);

    await tab(page, "Playlists").click();
    await page.getByRole("button", { name: /new playlist/i }).click();

    await expect(
      page.getByRole("heading", { name: /new playlist/i }),
    ).toBeVisible();
  });

  /**
   * A dynamic group holds artists, albums and genres, and resolves to tracks when
   * it is read — so it keeps up with the library rather than being maintained.
   */
  test("the groups tab opens a list, and a new group opens empty", async ({ page }) => {
    await boot(page);

    await tab(page, "Groups").click();
    const menu = page.getByRole("dialog", { name: "Dynamic groups" });
    await expect(menu).toBeVisible();
    await expect(menu).toContainText(/no dynamic groups yet/i);

    await menu.getByRole("button", { name: /new group/i }).click();

    await expect(page.getByRole("button", { name: /^new group$/i })).toBeVisible();
    await expect(page.getByText(/drag an artist, album or genre onto it/i)).toBeVisible();
  });

  test("an existing group lists the tracks it resolves to", async ({ page }) => {
    await boot(page, {
      groups: [
        {
          id: "g1",
          name: "Braindance",
          entities: [{ entityType: "artist", value: "Aphex Twin" }],
        },
      ],
    });

    /*
     * Which tracks a group resolves to is `vapor_library::group` matching
     * entities against the index, and it is tested there. The fake used to
     * restate that union and stopped (eb5fa70); it now hands back the whole
     * library unless a test asks for a narrower answer.
     *
     * So the answer is arranged, and what is asserted is the screen's half:
     * it draws the tracks it was given and does not pad the list with the rest
     * of the library. Roygbiv is the check on the second half.
     */
    await page.evaluate(() => {
      const backend = window.__vaporBackend;
      backend.answers("group_tracks", backend.rowsNamed("Windowlicker", "Xtal"));
    });

    await tab(page, "Groups").click();
    await page
      .getByRole("dialog", { name: "Dynamic groups" })
      .getByRole("button", { name: /braindance/i })
      .click();

    await expect(page.getByText("Windowlicker", { exact: true })).toBeVisible();
    await expect(page.getByText("Xtal", { exact: true })).toBeVisible();
    await expect(page.getByText("Roygbiv", { exact: true })).toHaveCount(0);
  });

  /** Back has to leave a group, the way it leaves every other drill-down. */
  test("back leaves a group", async ({ page }) => {
    await boot(page, {
      groups: [{ id: "g1", name: "Braindance", entities: [] }],
    });

    await tab(page, "Groups").click();
    await page
      .getByRole("dialog", { name: "Dynamic groups" })
      .getByRole("button", { name: /braindance/i })
      .click();
    await expect(page.getByRole("button", { name: /^braindance$/i })).toBeVisible();

    await page.goBack();
    await expect(page.getByRole("heading", { name: /your library/i })).toBeVisible();
  });
});

test.describe("Dragging with a finger", () => {
  /**
   * HTML5 drag-and-drop never fires on touch, so none of this exists for free:
   * the thing that follows the finger, working out what is under it, opening a
   * target by resting on it, and the drop itself are all ours.
   *
   * Driven with synthetic pointer events of type `touch`, not `page.mouse`.
   * Two reasons, and both are the point: `dragTo` speaks the HTML5 protocol
   * that does not work here, and a *mouse* press on a `draggable` row starts
   * the browser's own drag — which then stops delivering pointer events
   * entirely. The touch path is the one being tested and the one that ships.
   */
  async function send(
    page: Page,
    type: "pointerdown" | "pointermove" | "pointerup",
    x: number,
    y: number,
  ) {
    await page.evaluate(
      ([t, cx, cy]) => {
        const target =
          document.elementFromPoint(cx as number, cy as number) ?? window;
        target.dispatchEvent(
          new PointerEvent(t as string, {
            pointerId: 1,
            pointerType: "touch",
            isPrimary: true,
            clientX: cx as number,
            clientY: cy as number,
            bubbles: true,
            cancelable: true,
          }),
        );
      },
      [type, x, y] as const,
    );
  }

  async function centreOf(page: Page, selector: string) {
    const box = await page.locator(selector).first().boundingBox();
    if (!box) throw new Error(`no box for ${selector}`);
    return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  }

  async function openSongs(page: Page) {
    await page
      .getByRole("navigation", { name: "Screens" })
      .getByRole("button", { name: "Library", exact: true })
      .click();
    await page.getByRole("tab", { name: "Songs" }).click();
    await expect(page.locator(".songrow__title").first()).toBeVisible();
  }

  /** Hold a row past the threshold, then lift it by moving. */
  async function lift(page: Page) {
    const row = await centreOf(page, ".songrow");
    await send(page, "pointerdown", row.x, row.y);
    await page.waitForTimeout(600);
    await send(page, "pointermove", row.x + 24, row.y);
    await expect(page.locator(".draglayer")).toBeVisible();
  }

  test("a held row is picked up, and resting on a tab opens its list", async ({
    page,
  }) => {
    await boot(page, {
      playlists: [
        {
          id: "p1",
          name: "Night Drive",
          customCoverPath: "",
          tracks: [],
          folderId: "",
        },
      ],
    });
    await openSongs(page);
    await lift(page);

    // Rest on the tab: no further movement, which is what resting is.
    const tab = await centreOf(page, '[data-tab="playlist"]');
    await send(page, "pointermove", tab.x, tab.y);
    await expect(page.getByRole("dialog", { name: "Playlists" })).toBeVisible();

    // Scoped to the menu. The sidebar rail carries the same `data-drop-id`
    // now that it is a drop target too, and on a narrow window it is the one
    // that is hidden — an unscoped selector finds it first and has no box.
    const item = await centreOf(page, '.tabmenu [data-drop-id="p1"]');
    await send(page, "pointermove", item.x, item.y);
    await send(page, "pointerup", item.x, item.y);

    // Named, not counted. A drop used to answer "Added 1 track", which does
    // not say *where* — and on a phone the list closes behind it, so the name
    // is the only confirmation you get that it landed on the one you meant.
    await expect(page.getByText(/added 1 to night drive/i)).toBeVisible();
  });

  /**
   * The one move the rules turn down. A group holds artists, albums and
   * genres, and a drag that silently did nothing would read as a broken
   * gesture rather than an answer.
   */
  test("a track dropped on a dynamic group is refused, with a reason", async ({
    page,
  }) => {
    await boot(page, {
      groups: [{ id: "g1", name: "Braindance", entities: [] }],
    });
    await openSongs(page);
    await lift(page);

    const tab = await centreOf(page, '[data-tab="group"]');
    await send(page, "pointermove", tab.x, tab.y);
    await expect(page.getByRole("dialog", { name: "Dynamic groups" })).toBeVisible();

    const item = await centreOf(page, '.tabmenu [data-drop-id="g1"]');
    await send(page, "pointermove", item.x, item.y);
    await send(page, "pointerup", item.x, item.y);

    await expect(page.getByText(/not single tracks/i)).toBeVisible();
  });

  /** Moving before the hold arms is a scroll, and must not pick anything up. */
  test("a flick through the list does not start a drag", async ({ page }) => {
    await boot(page);
    await openSongs(page);

    const row = await centreOf(page, ".songrow");
    await send(page, "pointerdown", row.x, row.y);
    await send(page, "pointermove", row.x, row.y - 120);
    await send(page, "pointerup", row.x, row.y - 120);

    await expect(page.locator(".draglayer")).toHaveCount(0);
  });
});

test.describe("Duplicate tracks", () => {
  const TWICE = ["/a.mp3", "/a (1).mp3"].map((href, i) => ({
    href,
    // The filenames differ; the recording does not. That gap is the bug —
    // the row title comes from the path, so the copy reads as its own track.
    title: "Bocca di rosa",
    artist: "Fabrizio De Andre",
    album: "Volume 1",
    artistSource: "tag" as const,
    albumSource: "tag" as const,
    genre: "canzone",
    bpm: 121 + i * 0,
    key: "6A",
    year: 1967,
    manualPos: 0,
  }));

  test("are counted, and hidden only when asked", async ({ page }) => {
    await boot(page, { rows: TWICE });

    await page.getByRole("button", { name: "Settings", exact: true }).click();
    // The row's subtitle is the count and its state, not a sentence about it.
    await expect(page.getByText(/1 extra copy — still showing/i)).toBeVisible();

    // The table's row count is not asserted here any more. Whether a copy is
    // hidden is decided by `vapor_library`, and this suite runs against the
    // fake in `src/test/ipc.ts`, which no longer re-derives that rule — so a
    // count here would have been a fact about the fake. What the setting row
    // says, and that the switch drives it, is this screen's own.
    await page.getByLabel(/hide duplicates/i).check();
    await expect(page.getByText(/1 hidden/i)).toBeVisible();

    await page.getByLabel(/hide duplicates/i).uncheck();
    await expect(page.getByText(/1 extra copy — still showing/i)).toBeVisible();
  });
});

test.describe("Keeping a playlist on the device", () => {
  /**
   * Vapor plays from your server, and the audio cache holds a few tracks
   * either side of the set and drops the rest. This is the exception someone
   * asks for: fetched in full, kept outside the evicted directory, and gone
   * only when they say so.
   */
  test("downloads a playlist, then gives the space back", async ({ page }) => {
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

    await page
      .getByRole("navigation", { name: "Screens" })
      .getByRole("button", { name: "Playlists", exact: true })
      .click();
    await page
      .getByRole("dialog", { name: "Playlists" })
      .getByRole("button", { name: /night drive/i })
      .click();

    const button = page.getByRole("button", { name: /^download$/i });
    await expect(button).toBeVisible();
    await button.click();

    // It reports itself kept, which is the whole signal that it worked.
    await expect(page.getByRole("button", { name: /downloaded/i })).toBeVisible();

    await page.getByRole("button", { name: /downloaded/i }).click();
    await expect(page.getByRole("button", { name: /^download$/i })).toBeVisible();
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
        await page.getByRole("button", { name: "Settings", exact: true }).click();
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

  /**
   * The tab bar stays one row.
   *
   * Four pills fitted a phone and five do not. Playlists stopped being one of
   * them for exactly this — it wrapped the bar onto two rows and pushed the
   * grid down the screen — and Home made it five again, so the bar scrolls
   * sideways instead. Asserted by height, because "wrapped" is not a thing the
   * DOM will tell you directly: two rows of 36px pills is over 70.
   */
  test("the library's five tabs stay on one row", async ({ page }) => {
    await boot(page);

    const bar = page.locator(".library__tabs");
    await expect(bar.getByRole("tab")).toHaveCount(5);
    const height = await bar.evaluate((el) => el.getBoundingClientRect().height);
    expect(height).toBeLessThan(60);
  });
});

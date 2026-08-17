/**
 * Songs — the flat track table.
 *
 * The table carries four behaviours that are easy to break and invisible when
 * broken: virtualisation, keyboard navigation, a selection distinct from the
 * cursor, and a BPM cell that can be corrected by hand. Each is tested for what
 * a person would notice, not for how it is implemented.
 *
 * Unknown values are the recurring theme. A track with no analysis has a BPM of
 * 0 and no key, and the rule the whole app follows is that those render as "—"
 * and never as "0" — the Godot stub fabricating 120 BPM is the failure that
 * rule exists to prevent.
 */
import { describe, expect, it } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Songs } from "./Songs";
import { makeRow } from "../test/ipc";
import { emitEvent, useBackend } from "../test/setup";

/** The rows currently in the DOM. Virtualised, so this is what is *visible*. */
function rows() {
  return screen.getAllByRole("option");
}

describe("Songs — showing the library", () => {
  it("lists what the backend returned", async () => {
    useBackend();
    render(<Songs />);

    expect(await screen.findByText("Windowlicker")).toBeInTheDocument();
    expect(screen.getByText("Roygbiv")).toBeInTheDocument();
    expect(screen.getAllByText("Aphex Twin").length).toBeGreaterThan(0);
  });

  it("renders an unknown tempo and key as a dash, never as zero", async () => {
    useBackend();
    render(<Songs />);

    const row = (await screen.findByText("Not Yet Analysed")).closest(
      "[role=option]",
    ) as HTMLElement;

    expect(within(row).queryByText("0")).not.toBeInTheDocument();
    expect(within(row).getAllByText("—").length).toBeGreaterThan(0);
  });

  it("says so when the library is empty rather than showing a blank table", async () => {
    useBackend({ rows: [] });
    render(<Songs />);

    // Waiting for the *empty state* rather than for the absence of rows:
    // there are no rows while the screen is still loading, so asserting their
    // absence passes before the screen has said anything at all.
    expect(
      await screen.findByText(/nothing here|no tracks|empty/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/reading library/i)).not.toBeInTheDocument();
  });

  it("filters as you type", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    await user.type(screen.getByRole("searchbox"), "roygbiv");

    await waitFor(() => {
      expect(screen.queryByText("Windowlicker")).not.toBeInTheDocument();
    });
    expect(screen.getByText("Roygbiv")).toBeInTheDocument();
  });

  it("reports a failure to load rather than showing an empty table", async () => {
    const backend = useBackend();
    backend.fail("library_view", "the index is corrupt");
    render(<Songs />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/corrupt/i);
  });
});

describe("Songs — playing", () => {
  it("double-clicking a row plays from it, keeping the rest of the order", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await user.dblClick(await screen.findByText("Roygbiv"));

    await waitFor(() => {
      expect(backend.called("play_tracks")).toBe(true);
    });
    const args = backend.lastArgs("play_tracks");
    expect(args?.start).toBe("/dav/Koofr/Music/roygbiv.m4a");
    // The whole visible list is queued, not just the one row.
    expect((args?.hrefs as string[]).length).toBeGreaterThan(1);
  });
});

describe("Songs — selection and the keyboard", () => {
  it("moves a cursor with the arrow keys without changing the selection", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    const list = screen.getByRole("listbox");
    list.focus();

    await user.keyboard("{ArrowDown}{ArrowDown}");

    // The cursor moved; nothing became selected. That distinction is the point:
    // an arrow key must not silently change what a bulk action applies to.
    expect(
      rows().filter((r) => r.getAttribute("aria-selected") === "true"),
    ).toHaveLength(0);
  });

  it("Space selects the row under the cursor", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    screen.getByRole("listbox").focus();
    await user.keyboard("{ArrowDown}[Space]");

    await waitFor(() => {
      expect(
        rows().filter((r) => r.getAttribute("aria-selected") === "true").length,
      ).toBeGreaterThan(0);
    });
  });

  it("Enter plays the row under the cursor", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    screen.getByRole("listbox").focus();
    await user.keyboard("{Enter}");

    await waitFor(() => {
      expect(backend.called("play_tracks")).toBe(true);
    });
  });

  it("modifier-clicking builds a selection", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    await user.keyboard("{Meta>}");
    await user.click(screen.getByText("Windowlicker"));
    await user.click(screen.getByText("Xtal"));
    await user.keyboard("{/Meta}");

    await waitFor(() => {
      expect(
        rows().filter((r) => r.getAttribute("aria-selected") === "true"),
      ).toHaveLength(2);
    });
  });

  it("adds a selection to a playlist in the order shown, not insertion order", async () => {
    const backend = useBackend({
      playlists: [
        { id: "p1", name: "Later", customCoverPath: "", tracks: [], folderId: "" },
      ],
    });
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    // Select the second row first, then the first.
    await user.keyboard("{Meta>}");
    await user.click(screen.getByText("Xtal"));
    await user.click(screen.getByText("Windowlicker"));
    await user.keyboard("{/Meta}");

    await user.selectOptions(await screen.findByRole("combobox"), "p1");

    await waitFor(() => {
      expect(backend.called("add_tracks_to_playlist")).toBe(true);
    });
    const hrefs = backend.lastArgs("add_tracks_to_playlist")?.hrefs as string[];
    // Screen order: Windowlicker sorts before Xtal.
    expect(hrefs[0]).toBe("/dav/Koofr/Music/windowlicker.m4a");
    expect(hrefs[1]).toBe("/dav/Koofr/Music/xtal.m4a");
  });
});

describe("Songs — correcting a tempo", () => {
  it("accepts a hand-typed BPM", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    const row = (await screen.findByText("Not Yet Analysed")).closest(
      "[role=option]",
    ) as HTMLElement;
    // By what the cell tells the person to do, not by position: an unanalysed
    // track shows a dash in the artist cell too, so "the first dash" is the
    // wrong one.
    await user.dblClick(within(row).getByTitle(/correct the tempo/i));

    const box = within(row).getByRole("textbox");
    await user.type(box, "128");
    await user.keyboard("{Enter}");

    await waitFor(() => {
      expect(backend.called("set_bpm_override")).toBe(true);
    });
    expect(backend.lastArgs("set_bpm_override")?.bpm).toBe(128);
  });

  it("refuses an implausible tempo rather than clamping it", async () => {
    const backend = useBackend();
    backend.fail("set_bpm_override", "A tempo of 4000 is not plausible.");
    const user = userEvent.setup();
    render(<Songs />);

    const row = (await screen.findByText("Not Yet Analysed")).closest(
      "[role=option]",
    ) as HTMLElement;
    await user.dblClick(within(row).getByTitle(/correct the tempo/i));
    await user.type(within(row).getByRole("textbox"), "4000");
    await user.keyboard("{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent(/not plausible/i);
  });

  /**
   * A correction changes where the beats are, not just the number in the cell,
   * so the backend re-tracks the grid against it — which needs the audio and
   * takes seconds. During that window the track still mixes, on an approximate
   * grid, and the cell has to say so. A cell that looked finished would be
   * claiming an alignment that is not there yet.
   */
  it("says so while the beat grid is being rebuilt", async () => {
    useBackend();
    render(<Songs />);

    const row = (await screen.findByText("Not Yet Analysed")).closest(
      "[role=option]",
    ) as HTMLElement;
    const href = "/dav/Koofr/Music/unanalysed.mp3";

    act(() => {
      emitEvent("bpm-retrack", { href, bpm: 128, beats: null, error: null });
    });
    expect(
      within(row).getByTitle(/finding the beats at this tempo/i),
    ).toBeInTheDocument();

    // Finished: back to the ordinary cell, still correctable.
    act(() => {
      emitEvent("bpm-retrack", { href, bpm: 128, beats: 642, error: null });
    });
    expect(
      within(row).queryByTitle(/finding the beats at this tempo/i),
    ).not.toBeInTheDocument();
  });

  /**
   * A failed re-track clears the marker too. The correction itself is saved
   * either way, so leaving the cell pulsing forever would report a job that is
   * not running.
   */
  it("stops saying so when the rebuild fails", async () => {
    useBackend();
    render(<Songs />);

    const row = (await screen.findByText("Not Yet Analysed")).closest(
      "[role=option]",
    ) as HTMLElement;
    const href = "/dav/Koofr/Music/unanalysed.mp3";

    act(() => {
      emitEvent("bpm-retrack", { href, bpm: 128, beats: null, error: null });
    });
    act(() => {
      emitEvent("bpm-retrack", {
        href,
        bpm: 128,
        beats: null,
        error: "not available locally",
      });
    });

    expect(
      within(row).queryByTitle(/finding the beats at this tempo/i),
    ).not.toBeInTheDocument();
  });

  it("rejects text that is not a number without calling the backend", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    const row = (await screen.findByText("Not Yet Analysed")).closest(
      "[role=option]",
    ) as HTMLElement;
    await user.dblClick(within(row).getByTitle(/correct the tempo/i));
    const box = within(row).getByRole("textbox");
    await user.type(box, "abc");
    await user.keyboard("{Enter}");

    await waitFor(() => {
      expect(backend.called("set_bpm_override")).toBe(false);
    });
  });
});

describe("Songs — sorting", () => {
  it("sorts by a column, and reverses on a second press", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    const bpm = screen.getByRole("button", { name: /^bpm/i });

    await user.click(bpm);
    await waitFor(() => {
      expect((backend.lastArgs("library_view")?.view as { sortKey?: string })?.sortKey).toBe("bpm");
    });

    await user.click(bpm);
    await waitFor(() => {
      const view = backend.lastArgs("library_view")?.view as { ascending?: boolean };
      expect(view?.ascending).toBe(false);
    });
  });

  /**
   * TD-46. The sort controls were `role="columnheader"` inside a
   * `role="row"`, above a `listbox` of `option`s — a row must live in a
   * table, grid or treegrid, and there is none, so a screen reader was told
   * about a table header with no table.
   */
  it("has no table roles above a listbox that is not a table", async () => {
    useBackend();
    render(<Songs />);

    await screen.findByText("Windowlicker");

    expect(screen.getByRole("listbox")).toBeInTheDocument();
    expect(screen.queryAllByRole("row")).toHaveLength(0);
    expect(screen.queryAllByRole("columnheader")).toHaveLength(0);
  });

  /**
   * Which control is sorting, and which way, without seeing the arrow. The
   * arrow is `aria-hidden` — read aloud it is "up arrow", which does not say
   * what it is sorting.
   */
  it("says in each control's own name which one is sorting and which way", async () => {
    const user = userEvent.setup();
    useBackend();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    await user.click(screen.getByRole("button", { name: /^bpm/i }));

    const bpm = await screen.findByRole("button", { name: /bpm, sorted ascending/i });
    expect(bpm).toHaveAttribute("aria-pressed", "true");
    // And the ones that are not sorting say so by not being pressed.
    expect(screen.getByRole("button", { name: /^artist$/i })).toHaveAttribute(
      "aria-pressed",
      "false",
    );

    await user.click(bpm);
    expect(
      await screen.findByRole("button", { name: /bpm, sorted descending/i }),
    ).toBeInTheDocument();
  });

  it("can sort by artist from the header", async () => {
    // Artist used to be `display: none` in the header, so the table could not
    // be sorted by it from its own header at all (TD-32).
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    await user.click(screen.getByRole("button", { name: /^artist$/i }));

    await waitFor(() => {
      expect(
        (backend.lastArgs("library_view")?.view as { sortKey?: string })?.sortKey,
      ).toBe("artist");
    });
  });
});

describe("Songs — a large library", () => {
  it("does not put ten thousand rows in the DOM", async () => {
    const many = Array.from({ length: 10_000 }, (_, i) =>
      makeRow({ href: `/t/${i}.m4a`, title: `Track ${i}` }),
    );
    useBackend({ rows: many });
    render(<Songs />);

    await screen.findByText("Track 0");
    // Virtualised: only what fits, plus overscan. The exact number depends on
    // the measured viewport, so the assertion is about the order of magnitude.
    expect(screen.getAllByRole("option").length).toBeLessThan(200);
  });
});

/**
 * The gestures.
 *
 * The table shipped with a plain click that only *selected*, on a screen whose
 * purpose is playing something, and with the liner notes reachable only from a
 * small button that fades in over the artwork on hover. Selecting a run of
 * tracks was impossible: the row read `metaKey` and `ctrlKey` and nothing else,
 * so shift-click did nothing at all.
 *
 * The model now: one click plays, two open the track, and the checkbox selects.
 */
describe("Songs — gestures", () => {
  it("plays the track that was clicked once", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await user.click(await screen.findByText("Xtal"));

    await waitFor(() => expect(backend.state.status).toBe("playing"));
    expect(backend.state.current).toBe("/dav/Koofr/Music/xtal.m4a");
  });

  it("queues the whole visible table behind it, in the order shown", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await user.click(await screen.findByText("Xtal"));

    await waitFor(() => expect(backend.state.queue.length).toBe(4));
  });

  it("does not select the row it plays", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await user.click(await screen.findByText("Xtal"));

    // A selection bar appearing after a plain click is the old behaviour.
    expect(screen.queryByText(/1 selected/i)).not.toBeInTheDocument();
  });

  it("opens the track on a double click", async () => {
    useBackend();
    const user = userEvent.setup();
    const opened: string[] = [];
    render(<Songs onOpen={(href) => opened.push(href)} />);

    await user.dblClick(await screen.findByText("Xtal"));

    expect(opened).toEqual(["/dav/Koofr/Music/xtal.m4a"]);
  });

  /** The second click must not re-issue the queue and restart the track. */
  it("does not restart the track while opening it", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs onOpen={() => {}} />);

    await user.dblClick(await screen.findByText("Xtal"));

    const plays = backend.calls.filter((c) => c.cmd === "play_tracks").length;
    expect(plays).toBe(1);
  });

  it("selects with the checkbox without playing anything", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Xtal");
    await user.click(screen.getByRole("checkbox", { name: /select xtal/i }));

    expect(await screen.findByText(/1 selected/i)).toBeInTheDocument();
    expect(backend.called("play_tracks")).toBe(false);
  });

  it("extends the selection with shift, across a range of rows", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    // Sorted by title: Not Yet Analysed, Roygbiv, Windowlicker, Xtal.
    await screen.findByText("Roygbiv");
    await user.click(screen.getByRole("checkbox", { name: /select roygbiv/i }));
    await user.keyboard("{Shift>}");
    await user.click(screen.getByRole("checkbox", { name: /select xtal/i }));
    await user.keyboard("{/Shift}");

    expect(await screen.findByText(/3 selected/i)).toBeInTheDocument();
  });

  it("extends the selection with shift-click on the row itself", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Roygbiv");
    await user.click(screen.getByRole("checkbox", { name: /select roygbiv/i }));
    await user.keyboard("{Shift>}");
    await user.click(screen.getByText("Xtal"));
    await user.keyboard("{/Shift}");

    expect(await screen.findByText(/3 selected/i)).toBeInTheDocument();
  });

  it("still toggles one row at a time with the platform modifier", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Xtal");
    await user.keyboard("{Meta>}");
    await user.click(screen.getByText("Xtal"));
    await user.click(screen.getByText("Roygbiv"));
    await user.keyboard("{/Meta}");

    expect(await screen.findByText(/2 selected/i)).toBeInTheDocument();
  });
});

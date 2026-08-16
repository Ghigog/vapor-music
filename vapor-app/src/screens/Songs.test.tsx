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
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Songs } from "./Songs";
import { makeRow } from "../test/ipc";
import { useBackend } from "../test/setup";

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
    const bpm = screen.getByRole("columnheader", { name: /bpm/i });

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

  it("can sort by artist from the header", async () => {
    // Artist used to be `display: none` in the header, so the table could not
    // be sorted by it from its own header at all (TD-32).
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Songs />);

    await screen.findByText("Windowlicker");
    await user.click(screen.getByRole("columnheader", { name: /^artist$/i }));

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

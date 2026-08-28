/**
 * The shell.
 *
 * Most of what App does is route between screens, and each screen is tested on
 * its own. What is only testable here is the thing that spans them: the notice
 * shown when the backend could not read one of its data files at startup.
 *
 * That notice is the only difference between "you have no playlists" and "your
 * playlists could not be read". The app deliberately starts anyway — refusing
 * to open because one of fourteen files is damaged is a worse answer — so
 * without it a person is shown an empty library with no indication that their
 * data is sitting on disk, intact and merely unreadable.
 */
import { describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import * as drag from "./lib/drag";
import { useBackend } from "./test/setup";

describe("App — startup damage", () => {
  it("says nothing on a normal launch", async () => {
    useBackend();
    render(<App />);

    // Wait for the shell to settle so this is not asserting on a frame before
    // the check has run. By name because the shell renders two navigations —
    // the sidebar and the narrow-width tab bar — and CSS, which decides which
    // of them is showing, is not applied here.
    await screen.findByRole("navigation", { name: /screens and playlists/i });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("names every file it could not read, and where the bytes went", async () => {
    useBackend({
      damaged: [
        "Your playlists file could not be read (expected value at line 1). It has been kept at /data/playlists.corrupt.json and the app started with an empty one, so nothing has been overwritten.",
        "Your tags file could not be read (EOF while parsing). It has been kept at /data/tags.corrupt.json and the app started with an empty one, so nothing has been overwritten.",
      ],
    });
    render(<App />);

    const notice = await screen.findByRole("alert");
    expect(notice).toHaveTextContent(/could not be read/i);
    // The path is the actionable half of the sentence and must survive to the
    // screen intact.
    expect(notice).toHaveTextContent("/data/playlists.corrupt.json");
    expect(notice).toHaveTextContent("/data/tags.corrupt.json");
  });

  it("can be dismissed, and stays dismissed", async () => {
    useBackend({ damaged: ["Your tags file could not be read (bad json)."] });
    const user = userEvent.setup();
    render(<App />);

    await screen.findByRole("alert");
    await user.click(screen.getByRole("button", { name: /dismiss/i }));

    await waitFor(() => {
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
  });

  /// A backend too broken to answer must not take the whole shell down with
  /// it: the notice is a courtesy, and the app is still usable without it.
  it("still renders when the check itself fails", async () => {
    const backend = useBackend();
    backend.fail("startup_problems", "the state lock is poisoned");
    render(<App />);

    expect(
      await screen.findByRole("navigation", { name: /screens and playlists/i }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

/**
 * Where "back" lands.
 *
 * The library tab was local state inside `Library`, and opening a track's liner
 * notes unmounts `Library` entirely — the shell renders one or the other, not
 * both. So coming back remounted it at its default tab: you left from Songs and
 * returned to Albums, every time.
 *
 * It is a place, so it belongs in the history entry beside `opened` and
 * `playlist`. This is in the component suite rather than the browser one on
 * purpose: it is state routing, not layout, and it costs seconds here against
 * minutes there.
 */
describe("App — the library tab is a place", () => {
  it("returns to the tab that was open, not the default one", async () => {
    useBackend();
    render(<App />);
    const user = userEvent.setup();

    await screen.findByRole("navigation", { name: /screens and playlists/i });

    // Albums is where the library starts, so moving to Songs is the change
    // that has to survive the round trip.
    await user.click(await screen.findByRole("tab", { name: /^songs$/i }));
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: /^songs$/i })).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );

    // Into a track, then back out the way the shell's own control does it.
    const row = await screen.findByRole("option", { name: /Roygbiv/i });
    await user.dblClick(row);
    await user.click(await screen.findByRole("button", { name: /back/i }));

    await waitFor(() =>
      expect(screen.getByRole("tab", { name: /^songs$/i })).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
  });
});

/**
 * The tab bar as a drop target (POL-7).
 *
 * At the width that shows it there is no sidebar rail, so the tab is the only
 * way to a playlist — and a tab is not a target: the list you have to land on
 * only exists once it is open. A finger resting on one has opened it since the
 * touch drag was built (`DragLayer` times a dwell over `[data-tab]`); a drag
 * the browser itself is running had no equivalent, so on a narrow window an
 * album carried to Playlists had nothing under it.
 */
describe("App — dragging at the playlists tab", () => {
  /** A native drag of one album, as a tile in the library produces one. */
  function dragOver(element: HTMLElement) {
    const data = new DataTransfer();
    drag.writeDrag(data, {
      kind: "album",
      values: ["Selected Ambient Works"],
      label: "Selected Ambient Works",
    });
    element.dispatchEvent(
      new DragEvent("dragover", {
        bubbles: true,
        cancelable: true,
        dataTransfer: data,
      }),
    );
  }

  it("opens the list when a drag rests on the tab", async () => {
    useBackend({
      playlists: [
        {
          id: "p1",
          name: "Late Night",
          customCoverPath: "",
          tracks: [],
          folderId: "",
        },
      ],
    });
    render(<App />);

    await screen.findByRole("navigation", { name: /screens and playlists/i });
    const tab = document.querySelector<HTMLElement>('[data-tab="playlist"]');
    expect(tab).not.toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();

    vi.useFakeTimers();
    try {
      // Resting, not passing through: `dragover` repeats while the cursor is
      // over the target, and the list opens only once the dwell is served.
      act(() => {
        dragOver(tab!);
      });
      expect(screen.queryByRole("dialog")).toBeNull();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(drag.DWELL_MS + 50);
      });
    } finally {
      vi.useRealTimers();
    }

    const menu = await screen.findByRole("dialog", { name: /playlists/i });
    // And it is a target now: the row carries what a drop needs to act on.
    expect(
      menu.querySelector('[data-drop-name="Late Night"]'),
    ).not.toBeNull();
  });

  it("leaves the list shut for a drag that only passes over the tab", async () => {
    useBackend();
    render(<App />);

    await screen.findByRole("navigation", { name: /screens and playlists/i });
    const tab = document.querySelector<HTMLElement>('[data-tab="playlist"]');

    vi.useFakeTimers();
    try {
      act(() => {
        dragOver(tab!);
        tab!.dispatchEvent(new DragEvent("dragleave", { bubbles: true }));
      });
      await act(async () => {
        await vi.advanceTimersByTimeAsync(drag.DWELL_MS + 50);
      });
    } finally {
      vi.useRealTimers();
    }

    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

/**
 * The top of the library: two shelves, the genre row, and the table under them.
 *
 * What the screen does with what it is handed, not how any of it was ranked —
 * that is `home_shelves_for` and `order_entities` in the backend, which have
 * their own tests. The questions here are the ones only a screen can get
 * wrong: does each part draw, does a tile go where it says it goes, and does
 * pressing play queue that thing rather than whatever else is on screen.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Library } from "./Library";
import { useBackend } from "../test/setup";
import { makeEntity, makeRow } from "../test/ipc";
import type * as core from "../lib/core";

/** A playlist with two of the default library's tracks in it. */
function aPlaylist(): core.Playlist[] {
  return [
    {
      id: "p1",
      name: "Late night",
      customCoverPath: "",
      tracks: ["/dav/Koofr/Music/xtal.m4a", "/dav/Koofr/Music/roygbiv.m4a"],
      folderId: "",
    },
  ];
}

function aGroup(): core.DynamicGroup[] {
  return [
    {
      id: "g1",
      name: "Ambient",
      entities: [{ entityType: "artist", value: "Aphex Twin" }],
    },
  ];
}

describe("Library — one screen, top to bottom", () => {
  /**
   * The screen opens on what someone actually wants.
   *
   * Almost nobody arrives at their own library looking for a particular
   * record; they arrive wanting something on. The album grid answered the
   * other question, and it was the first thing anyone saw.
   */
  it("opens on the shelves, the genres and the table", async () => {
    useBackend();
    render(<Library />);

    expect(await screen.findByRole("heading", { name: /^artists$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^albums$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^genres$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^songs$/i })).toBeInTheDocument();
  });

  /**
   * There is one library view now.
   *
   * Home, Albums, Artists, Genres and Songs were five ways of laying the same
   * library out, four of which were visited once. A tab bar left on the page
   * would be a set of doors onto a room you are already standing in.
   */
  it("has no tab bar", async () => {
    useBackend();
    render(<Library />);

    await screen.findByRole("heading", { name: /^albums$/i });
    expect(screen.queryAllByRole("tab")).toHaveLength(0);
  });

  /**
   * Playlists and smart groups are in the sidebar, on every screen, where they
   * are also the drop target for a dragged track. A shelf of them here was a
   * second, worse copy of a list that never leaves the window.
   */
  it("does not shelve playlists or smart groups", async () => {
    useBackend({ playlists: aPlaylist(), groups: aGroup() });
    render(<Library />);

    await screen.findByRole("heading", { name: /^albums$/i });
    expect(
      screen.queryByRole("heading", { name: /^playlists$/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: /^smart groups$/i }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Late night")).not.toBeInTheDocument();
  });

  /** Both shelves carry their tiles. */
  it("draws what is on each shelf", async () => {
    useBackend();
    render(<Library />);

    // By the tile rather than by the text, throughout. "Aphex Twin" is also
    // the subtitle under two of the albums on the shelf below, and
    // "Windowlicker EP" is the album column of three rows in the table under
    // that — the whole library is on one screen, so a bare `getByText` cannot
    // say which half of it a test means.
    expect(
      await screen.findByRole("button", { name: /open the artist aphex twin/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /open the album windowlicker ep/i }),
    ).toBeInTheDocument();
  });

  /**
   * The genres, in the order they were handed over.
   *
   * That order is a ranking — plays for a track, skips against it, added up
   * over what is filed under each genre. A row that re-sorted them here would
   * put Acid Jazz first for ever in a library whose owner plays house.
   */
  it("draws the genres in the order the backend ranked them", async () => {
    useBackend();
    render(<Library />);

    const pills = await screen.findAllByRole("button", {
      name: /open the genre /i,
    });
    expect(pills.map((p) => p.textContent)).toEqual(["Electronic3", "Ambient1"]);
  });

  /** A genre pill goes where a genre tile went: into the tracks under it. */
  it("opens a genre into its tracks", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /open the genre electronic/i }),
    );

    expect(
      await screen.findByRole("heading", { name: "Electronic" }),
    ).toBeInTheDocument();
  });

  /** An album tile goes where the grid's album tile went: inside the record. */
  it("opens an album into its track list", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /open the album windowlicker ep/i }),
    );

    expect(
      await screen.findByRole("heading", { name: "Windowlicker EP" }),
    ).toBeInTheDocument();
  });

  /**
   * Back goes where the crumb says.
   *
   * The crumb used to name a tab, which is a place that no longer exists.
   * There is one view to return to and it says so.
   */
  it("says the press returns to the library, because it does", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /open the album windowlicker ep/i }),
    );
    const back = await screen.findByRole("button", { name: /‹ library/i });
    await user.click(back);

    expect(
      await screen.findByRole("heading", { name: /^albums$/i }),
    ).toBeInTheDocument();
  });

  /**
   * An artist is not a collection, and saying it is would be a lie in the
   * store: there is no gesture that means "put on Aphex Twin", only playing
   * their records. So nothing is credited, and the shelf ranks them on the
   * plays their tracks earned.
   */
  it("credits nothing when an artist is played", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /play windowlicker ep/i }),
    );

    await waitFor(() => expect(backend.state.status).toBe("playing"));
    expect(backend.lastArgs("play_tracks")?.collection ?? null).toBeNull();
  });

  /**
   * A shelf holds the first dozen of something ranked by plays, so filtering
   * one would answer "no such album" for an album that is in the library and
   * merely thirteenth. Searching is its own result.
   */
  it("searches rather than filtering the shelves", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await screen.findByRole("heading", { name: /^albums$/i });
    await user.type(screen.getByRole("searchbox"), "xtal");

    expect(await screen.findByText("Xtal")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { name: /^albums$/i }),
      ).not.toBeInTheDocument(),
    );
  });

  /** Clearing the field puts the shelves back rather than leaving the table. */
  it("comes back to the shelves when the search is cleared", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    const box = await screen.findByRole("searchbox");
    await user.type(box, "xtal");
    await screen.findByText("Xtal");
    await user.clear(box);

    expect(
      await screen.findByRole("heading", { name: /^albums$/i }),
    ).toBeInTheDocument();
  });

  /** An empty library is one sentence, not two blank rows. */
  it("says the library is empty rather than drawing blank shelves", async () => {
    useBackend({ rows: [], albums: [], artists: [], genres: [] });
    render(<Library />);

    expect(await screen.findByText(/no music yet/i)).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: /^albums$/i }),
    ).not.toBeInTheDocument();
  });

  it("reports a failure to read the shelves", async () => {
    const backend = useBackend();
    backend.fail("home_shelves", "the index would not open");
    render(<Library />);

    expect(await screen.findByText(/could not read the library/i)).toBeInTheDocument();
  });

  /**
   * The line under the title counts the library, whatever is on screen.
   *
   * It used to be reported upwards by whichever view was showing, so it read
   * "0 tracks" whenever the page opened straight into an album — which the
   * back gesture reaches every time somebody leaves liner notes.
   */
  it("counts the whole library in the line under the title", async () => {
    useBackend({
      rows: [
        makeRow({ href: "/a.m4a", title: "A" }),
        makeRow({ href: "/b.m4a", title: "B" }),
        makeRow({ href: "/c.m4a", title: "C" }),
      ],
      albums: [makeEntity({ name: "An Album", lead: "/a.m4a", tracks: 3 })],
    });
    render(<Library />);

    expect(await screen.findByText(/^3 tracks · on this device/i)).toBeInTheDocument();
  });

  /**
   * The shelves are ranked on play counts, so they go stale as a side effect
   * of the app being used — not only when the library changes. A scan is the
   * event both halves already listen for.
   */
  it("re-reads the shelves when the library changes underneath it", async () => {
    const backend = useBackend();
    render(<Library />);

    await screen.findByRole("heading", { name: /^albums$/i });
    const before = backend.timesCalled("home_shelves");
    window.dispatchEvent(new Event("vapor:library-changed"));

    await waitFor(() =>
      expect(backend.timesCalled("home_shelves")).toBeGreaterThan(before),
    );
  });
});

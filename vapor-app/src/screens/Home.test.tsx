/**
 * The home shelves.
 *
 * What the screen does with four ranked lists, not how they were ranked —
 * that is `home_shelves_for` in the backend, which has its own tests for the
 * four keys it sorts on. The questions here are the ones only a screen can get
 * wrong: does each shelf draw, does a tile go where it says it goes, and does
 * pressing play queue that thing rather than whatever else is on screen.
 */
import { describe, expect, it, vi } from "vitest";
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

describe("Library — the home shelves", () => {
  /**
   * The screen opens on what someone actually wants.
   *
   * Almost nobody arrives at their own library looking for a particular
   * record; they arrive wanting something on. The album grid answered the
   * other question, and it was the first thing anyone saw.
   */
  it("opens on the shelves, not on the album grid", async () => {
    useBackend({ playlists: aPlaylist(), groups: aGroup() });
    render(<Library />);

    expect(await screen.findByRole("heading", { name: /^playlists$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^smart groups$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^artists$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^albums$/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /^home$/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  /** Four shelves, and every one of them carries its tiles. */
  it("draws what is on each shelf", async () => {
    useBackend({ playlists: aPlaylist(), groups: aGroup() });
    render(<Library />);

    expect(await screen.findByText("Late night")).toBeInTheDocument();
    expect(screen.getByText("Ambient")).toBeInTheDocument();
    // By the tile rather than by the text: "Aphex Twin" is also the subtitle
    // under two of the albums on the shelf below.
    expect(
      screen.getByRole("button", { name: /open the artist aphex twin/i }),
    ).toBeInTheDocument();
    expect(screen.getByText("Windowlicker EP")).toBeInTheDocument();
    // The playlist's size, said under its title.
    expect(screen.getByText("2 tracks")).toBeInTheDocument();
  });

  /**
   * The shelves are the front door, and a door has to open.
   *
   * A playlist and a group are drill-downs the app owns rather than views
   * inside this screen, so all this one can do is ask — and asking with the
   * id, not the name, is the half that has to be right.
   */
  it("opens the playlist a tile names", async () => {
    useBackend({ playlists: aPlaylist() });
    const onOpenPlaylist = vi.fn();
    const user = userEvent.setup();
    render(<Library onOpenPlaylist={onOpenPlaylist} />);

    await user.click(
      await screen.findByRole("button", { name: /open the playlist late night/i }),
    );

    expect(onOpenPlaylist).toHaveBeenCalledWith("p1");
  });

  it("opens the smart group a tile names", async () => {
    useBackend({ groups: aGroup() });
    const onOpenGroup = vi.fn();
    const user = userEvent.setup();
    render(<Library onOpenGroup={onOpenGroup} />);

    await user.click(
      await screen.findByRole("button", { name: /open the smart group ambient/i }),
    );

    expect(onOpenGroup).toHaveBeenCalledWith("g1");
  });

  /** An album tile goes where the grid's album tile goes: inside the record. */
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
   * The crumb read "Albums" whatever you had opened, which was true when the
   * grid was the only way in. From a shelf it would be pointing at a tab the
   * press does not visit.
   */
  it("says the press returns home, because it does", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /open the album windowlicker ep/i }),
    );
    const back = await screen.findByRole("button", { name: /‹ home/i });
    await user.click(back);

    expect(
      await screen.findByRole("heading", { name: /^playlists$/i }),
    ).toBeInTheDocument();
  });

  /**
   * Pressing play on a playlist plays the playlist.
   *
   * Not what else is on screen: the shelves show four rows of other people's
   * records, and queueing the visible set — which is what the album grid does
   * — would mean pressing one playlist and getting everything.
   */
  it("queues the playlist it was told to play, and credits it", async () => {
    const backend = useBackend({ playlists: aPlaylist() });
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /play late night/i }),
    );

    await waitFor(() => expect(backend.state.status).toBe("playing"));
    expect(backend.state.queue).toEqual([
      "/dav/Koofr/Music/xtal.m4a",
      "/dav/Koofr/Music/roygbiv.m4a",
    ]);
    // The DJ conducts inside it, and the listen is credited to the playlist
    // by id — which is what puts it back at the front of the shelf.
    expect(backend.lastArgs("play_tracks")?.scope).toBe("Late night");
    expect(backend.lastArgs("play_tracks")?.collection).toEqual({
      kind: "playlist",
      id: "p1",
    });
  });

  it("credits a smart group when one is played", async () => {
    const backend = useBackend({ groups: aGroup() });
    const user = userEvent.setup();
    render(<Library />);

    await user.click(await screen.findByRole("button", { name: /play ambient/i }));

    await waitFor(() => expect(backend.state.status).toBe("playing"));
    expect(backend.lastArgs("play_tracks")?.collection).toEqual({
      kind: "group",
      id: "g1",
    });
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
    useBackend({ playlists: aPlaylist() });
    const user = userEvent.setup();
    render(<Library />);

    await screen.findByText("Late night");
    await user.type(screen.getByRole("searchbox"), "xtal");

    expect(await screen.findByText("Xtal")).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: /^smart groups$/i }),
    ).not.toBeInTheDocument();
  });

  /** Clearing the field puts the shelves back rather than leaving the table. */
  it("comes back to the shelves when the search is cleared", async () => {
    useBackend({ playlists: aPlaylist() });
    const user = userEvent.setup();
    render(<Library />);

    await screen.findByText("Late night");
    const box = screen.getByRole("searchbox");
    await user.type(box, "xtal");
    await screen.findByText("Xtal");
    await user.clear(box);

    expect(await screen.findByText("Late night")).toBeInTheDocument();
  });

  /**
   * Two shelves can be empty in a library that is full — nobody has made a
   * playlist yet — and four empty rows under four headings reads as broken.
   */
  it("says what a playlist shelf is for when there are none", async () => {
    useBackend();
    render(<Library />);

    expect(await screen.findByText(/playlists you make will show up here/i))
      .toBeInTheDocument();
    expect(screen.getByText(/a smart group is a set of artists and albums/i))
      .toBeInTheDocument();
  });

  /** An empty library is one sentence, not four empty shelves. */
  it("says the library is empty rather than drawing four blank rows", async () => {
    useBackend({ rows: [], albums: [], artists: [] });
    render(<Library />);

    expect(await screen.findByText(/no music yet/i)).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: /^smart groups$/i }),
    ).not.toBeInTheDocument();
  });

  it("reports a failure to read the shelves", async () => {
    const backend = useBackend();
    backend.fail("home_shelves", "the index would not open");
    render(<Library />);

    expect(await screen.findByText(/could not read the library/i)).toBeInTheDocument();
  });

  /**
   * The line under the title counts the library, not the shelves.
   *
   * Twelve albums is not five hundred tracks, and home never fetches rows —
   * so the number rides along with the shelves rather than being counted on
   * this side out of something that does not contain it.
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
    const backend = useBackend({ playlists: aPlaylist() });
    render(<Library />);

    await screen.findByText("Late night");
    const before = backend.timesCalled("home_shelves");
    window.dispatchEvent(new Event("vapor:library-changed"));

    await waitFor(() =>
      expect(backend.timesCalled("home_shelves")).toBeGreaterThan(before),
    );
  });
});

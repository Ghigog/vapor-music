/**
 * A playlist, and the sidebar rail that lists them.
 *
 * Built and shipped without a single test, which is the habit this suite
 * exists to break. The interesting cases are the ones that are easy to get
 * wrong and quiet when wrong: a playlist holding an href whose file has left
 * the library, an optimistic reorder that the backend then refuses, and a drop
 * that adds nothing because everything dropped was already there.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Playlist } from "./Playlist";
import { PlaylistRail, TRACK_DRAG_TYPE } from "../components/PlaylistRail";
import type * as core from "../lib/core";
import { useBackend } from "../test/setup";

function playlist(over: Partial<core.Playlist> = {}): core.Playlist {
  return {
    id: "p1",
    name: "Late Night",
    customCoverPath: "",
    tracks: [
      "/dav/Koofr/Music/windowlicker.m4a",
      "/dav/Koofr/Music/xtal.m4a",
      "/dav/Koofr/Music/roygbiv.m4a",
    ],
    folderId: "",
    ...over,
  };
}

/** A drag carrying `hrefs`, as the Songs table produces one. */
function dropOn(element: HTMLElement, hrefs: string[]) {
  const data = new DataTransfer();
  data.setData(TRACK_DRAG_TYPE, JSON.stringify(hrefs));
  element.dispatchEvent(
    new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: data }),
  );
}

describe("Playlist — showing one", () => {
  it("lists its tracks in playlist order, not sorted", async () => {
    useBackend({
      playlists: [
        playlist({
          tracks: [
            "/dav/Koofr/Music/roygbiv.m4a",
            "/dav/Koofr/Music/windowlicker.m4a",
          ],
        }),
      ],
    });
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    const items = await screen.findAllByRole("listitem");
    expect(within(items[0]!).getByText("Roygbiv")).toBeInTheDocument();
    expect(within(items[1]!).getByText("Windowlicker")).toBeInTheDocument();
  });

  /**
   * A playlist stores hrefs, and a file can leave the library after being
   * added. Those are skipped rather than drawn as blank rows nobody can play —
   * and the count says so, so 12 tracks showing 11 rows is explained.
   */
  it("skips tracks whose files have left the library, and says how many", async () => {
    useBackend({
      playlists: [
        playlist({
          tracks: [
            "/dav/Koofr/Music/windowlicker.m4a",
            "/gone/missing.m4a",
            "/dav/Koofr/Music/xtal.m4a",
          ],
        }),
      ],
    });
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    await screen.findByText("Windowlicker");
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByText(/not in the library/i)).toBeInTheDocument();
  });

  it("offers nothing to play when it is empty, and says what to do", async () => {
    useBackend({ playlists: [playlist({ tracks: [] })] });
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    expect(await screen.findByText(/nothing in here yet/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^play$/i })).toBeDisabled();
  });

  it("says so when the playlist does not exist", async () => {
    useBackend({ playlists: [] });
    render(<Playlist id="gone" onOpen={() => {}} onGone={() => {}} />);

    expect(await screen.findByText(/that playlist is gone/i)).toBeInTheDocument();
  });
});

describe("Playlist — acting on one", () => {
  it("plays the whole playlist in order", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    await user.click(await screen.findByRole("button", { name: /^play$/i }));

    await waitFor(() => expect(backend.called("play_tracks")).toBe(true));
    expect(backend.lastArgs("play_tracks")?.hrefs).toEqual(playlist().tracks);
  });

  it("plays from a chosen track without losing the rest", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    await user.click(await screen.findByText("Xtal"));

    await waitFor(() => expect(backend.called("play_tracks")).toBe(true));
    expect(backend.lastArgs("play_tracks")?.start).toBe(
      "/dav/Koofr/Music/xtal.m4a",
    );
    expect((backend.lastArgs("play_tracks")?.hrefs as string[]).length).toBe(3);
  });

  it("renames on a double-click", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    await user.dblClick(await screen.findByRole("heading", { name: "Late Night" }));
    const box = screen.getByRole("textbox");
    await user.clear(box);
    await user.type(box, "Very Late Night{Enter}");

    await waitFor(() => {
      expect(backend.state.playlists[0]?.name).toBe("Very Late Night");
    });
  });

  it("abandons a rename on Escape", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    await user.dblClick(await screen.findByRole("heading", { name: "Late Night" }));
    await user.clear(screen.getByRole("textbox"));
    await user.type(screen.getByRole("textbox"), "Discarded{Escape}");

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Late Night" })).toBeInTheDocument();
    });
    expect(backend.called("rename_playlist")).toBe(false);
  });

  it("removes a track", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    const first = (await screen.findAllByRole("listitem"))[0]!;
    await user.click(within(first).getByRole("button", { name: /^remove /i }));

    await waitFor(() => {
      expect(backend.state.playlists[0]?.tracks).toHaveLength(2);
    });
    expect(backend.state.playlists[0]?.tracks[0]).toBe("/dav/Koofr/Music/xtal.m4a");
  });

  it("reorders with the buttons, so the keyboard is not left out", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    const second = (await screen.findAllByRole("listitem"))[1]!;
    await user.click(within(second).getByRole("button", { name: /move .+ up/i }));

    await waitFor(() => {
      expect(backend.state.playlists[0]?.tracks[0]).toBe(
        "/dav/Koofr/Music/xtal.m4a",
      );
    });
  });

  it("cannot move the first track up or the last one down", async () => {
    useBackend({ playlists: [playlist()] });
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    const items = await screen.findAllByRole("listitem");
    expect(
      within(items[0]!).getByRole("button", { name: /move .+ up/i }),
    ).toBeDisabled();
    expect(
      within(items[items.length - 1]!).getByRole("button", { name: /move .+ down/i }),
    ).toBeDisabled();
  });

  it("deletes, and tells the shell there is nothing left to show", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    let gone = false;
    render(
      <Playlist id="p1" onOpen={() => {}} onGone={() => { gone = true; }} />,
    );

    await user.click(await screen.findByRole("button", { name: /delete the playlist/i }));

    await waitFor(() => expect(gone).toBe(true));
    expect(backend.state.playlists).toHaveLength(0);
  });

  it("surfaces a refused rename rather than pretending it worked", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    backend.fail("rename_playlist", "A playlist needs a name.");
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} onGone={() => {}} />);

    await user.dblClick(await screen.findByRole("heading", { name: "Late Night" }));
    await user.clear(screen.getByRole("textbox"));
    await user.type(screen.getByRole("textbox"), "x{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent(/needs a name/i);
  });
});

describe("The playlist rail", () => {
  it("lists playlists with their lengths", async () => {
    useBackend({ playlists: [playlist(), playlist({ id: "p2", name: "Warm Up", tracks: [] })] });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    expect(await screen.findByText("Late Night")).toBeInTheDocument();
    expect(screen.getByText("Warm Up")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
  });

  it("says what to do when there are none", async () => {
    useBackend({ playlists: [] });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    expect(await screen.findByText(/none yet/i)).toBeInTheDocument();
  });

  it("creates one and opens it", async () => {
    const backend = useBackend({ playlists: [] });
    const user = userEvent.setup();
    let opened: string | null = null;
    render(<PlaylistRail activeId={null} onOpen={(id) => { opened = id; }} />);

    await user.click(await screen.findByRole("button", { name: /new playlist/i }));
    await user.type(screen.getByRole("textbox"), "Fresh{Enter}");

    await waitFor(() => expect(backend.state.playlists).toHaveLength(1));
    expect(backend.state.playlists[0]?.name).toBe("Fresh");
    expect(opened).not.toBeNull();
  });

  it("does not create an unnamed playlist", async () => {
    const backend = useBackend({ playlists: [] });
    const user = userEvent.setup();
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    await user.click(await screen.findByRole("button", { name: /new playlist/i }));
    await user.type(screen.getByRole("textbox"), "   {Enter}");

    await waitFor(() => {
      expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    });
    expect(backend.called("create_playlist")).toBe(false);
  });

  it("accepts tracks dropped onto a playlist", async () => {
    const backend = useBackend({
      playlists: [playlist({ id: "p2", name: "Warm Up", tracks: [] })],
    });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /warm up/i });
    dropOn(target, [
      "/dav/Koofr/Music/windowlicker.m4a",
      "/dav/Koofr/Music/xtal.m4a",
    ]);

    await waitFor(() => {
      expect(backend.state.playlists[0]?.tracks).toHaveLength(2);
    });
    expect(await screen.findByText(/added 2 to warm up/i)).toBeInTheDocument();
  });

  /**
   * Dropping six tracks that are all already there looks identical to a drop
   * that missed, unless the screen says which happened.
   */
  it("says when a drop added nothing because they were already there", async () => {
    useBackend({ playlists: [playlist()] });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /late night/i });
    dropOn(target, playlist().tracks);

    expect(await screen.findByText(/already in late night/i)).toBeInTheDocument();
  });

  it("ignores a drag that is not ours", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /late night/i });
    const data = new DataTransfer();
    data.setData("text/plain", "some text from somewhere else");
    target.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: data }),
    );

    await new Promise((r) => setTimeout(r, 50));
    expect(backend.called("add_tracks_to_playlist")).toBe(false);
  });

  it("survives a drop carrying malformed data", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /late night/i });
    const data = new DataTransfer();
    data.setData(TRACK_DRAG_TYPE, "{not json at all");
    target.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: data }),
    );

    await new Promise((r) => setTimeout(r, 50));
    expect(backend.called("add_tracks_to_playlist")).toBe(false);
    // Still on screen rather than having thrown during the drop handler.
    expect(screen.getByText("Late Night")).toBeInTheDocument();
  });
});

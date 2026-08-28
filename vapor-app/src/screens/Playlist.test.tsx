/**
 * A playlist, and the sidebar rail that lists them.
 *
 * Built and shipped without a single test, which is the habit this suite
 * exists to break. The interesting cases are the ones that are easy to get
 * wrong and quiet when wrong: a playlist holding an href whose file has left
 * the library, an optimistic reorder that the backend then refuses, and a drop
 * that adds nothing because everything dropped was already there.
 */
import { describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Playlist } from "./Playlist";
import { PlaylistRail, PLAYLIST_DRAG_TYPE } from "../components/PlaylistRail";
import * as drag from "../lib/drag";
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
  drag.writeDrag(data, {
    kind: "track",
    values: hrefs,
    label: `${hrefs.length} tracks`,
  });
  element.dispatchEvent(
    new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: data }),
  );
}

/**
 * A pointer event jsdom will carry the coordinates of.
 *
 * The drag listens on `window`, because a finger that leaves the row it
 * started on has to keep being followed — so these are dispatched there rather
 * than through `fireEvent` on an element.
 */
function finger(type: string, x: number, y: number): Event {
  const event = new Event(type, { bubbles: true });
  Object.assign(event, { clientX: x, clientY: y, button: 0 });
  return event;
}

/** Put an element's box along the bottom of a 400x600 screen. */
function atBottom(element: HTMLElement) {
  vi.spyOn(element, "getBoundingClientRect").mockReturnValue({
    left: 0,
    right: 400,
    top: 500,
    bottom: 600,
    x: 0,
    y: 500,
    width: 400,
    height: 100,
    toJSON: () => ({}),
  } as DOMRect);
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
    render(<Playlist id="p1" onOpen={() => {}} />);

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
    render(<Playlist id="p1" onOpen={() => {}} />);

    await screen.findByText("Windowlicker");
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByText(/not in the library/i)).toBeInTheDocument();
  });

  /**
   * The instruction has to be performable on the device it is read on.
   *
   * It said "drag tracks from Songs onto this playlist in the sidebar". On a
   * phone there is no sidebar — it is a bar along the bottom at that width —
   * and it is not only tracks that can be dropped (POL-7).
   */
  it("says what to do when it is empty, naming the bar and not a sidebar", async () => {
    useBackend({ playlists: [playlist({ tracks: [] })] });
    render(<Playlist id="p1" onOpen={() => {}} />);

    expect(await screen.findByText(/nothing in here yet/i)).toBeInTheDocument();
    expect(screen.getByText(/tracks, albums or artists/i)).toBeInTheDocument();
    expect(screen.getByText(/bar at the bottom/i)).toBeInTheDocument();
    expect(screen.queryByText(/sidebar/i)).toBeNull();
  });

  it("says so when the playlist does not exist", async () => {
    useBackend({ playlists: [] });
    render(<Playlist id="gone" onOpen={() => {}} />);

    expect(await screen.findByText(/that playlist is gone/i)).toBeInTheDocument();
  });
});

describe("Playlist — acting on one", () => {
  /**
   * POL-6: a playlist had Play and Delete and a group had neither, which is a
   * good part of why the two read as separate features. Pressing a row was
   * already how a playlist gets played, so the button said nothing the list
   * did not — and nothing on this screen deletes any more.
   */
  it("has no Play or Delete button — a row is what plays it", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} />);

    await screen.findByText("Windowlicker");
    expect(screen.queryByRole("button", { name: /^play$/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /delete/i })).toBeNull();

    await user.click(screen.getByText("Windowlicker"));
    await waitFor(() => expect(backend.called("play_tracks")).toBe(true));
    expect(backend.lastArgs("play_tracks")?.hrefs).toEqual(playlist().tracks);
  });

  /** POL-6: the one control both screens are meant to have. */
  it("offers to keep the playlist on the device", async () => {
    useBackend({ playlists: [playlist()] });
    render(<Playlist id="p1" onOpen={() => {}} />);

    expect(
      await screen.findByRole("button", { name: /download/i }),
    ).toBeInTheDocument();
  });

  it("plays from a chosen track without losing the rest", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} />);

    await user.click(await screen.findByText("Xtal"));

    await waitFor(() => expect(backend.called("play_tracks")).toBe(true));
    expect(backend.lastArgs("play_tracks")?.start).toBe(
      "/dav/Koofr/Music/xtal.m4a",
    );
    expect((backend.lastArgs("play_tracks")?.hrefs as string[]).length).toBe(3);
    // And conducted within the playlist rather than out of it: the DJ plans
    // past the end of the queue, so without a scope a playlist ran out after
    // its own tracks and the set wandered into the library.
    expect(backend.lastArgs("play_tracks")?.scope).toBe(playlist().name);
  });

  /**
   * POL-6 records this as "playlists cannot be renamed at all".
   *
   * They could — on a double-click of the heading, which is a gesture a phone
   * does not have, so on the device this app is for it was true. One press of
   * the title now, the same as a group's.
   */
  it("renames from the title, with one press", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "Late Night" }));
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
    render(<Playlist id="p1" onOpen={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "Late Night" }));
    await user.clear(screen.getByRole("textbox"));
    await user.type(screen.getByRole("textbox"), "Discarded{Escape}");

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Late Night" })).toBeInTheDocument();
    });
    expect(backend.called("rename_playlist")).toBe(false);
  });

  /**
   * Removing is a gesture now (POL-6).
   *
   * The row's ✕ was revealed on hover, and a phone has no hover — so on the
   * device this app is for the control was revealed by nothing at all. A row
   * is dragged to a panel that rises at the bottom instead.
   */
  it("removes a track dragged onto the Remove panel", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    render(<Playlist id="p1" onOpen={() => {}} />);

    const first = (await screen.findAllByRole("listitem"))[0]!;
    // Nothing to fall onto until something is being carried.
    expect(screen.queryByText("Remove")).toBeNull();
    expect(within(first).queryByRole("button", { name: /^remove /i })).toBeNull();

    fireEvent.dragStart(first, { dataTransfer: new DataTransfer() });

    const bin = await screen.findByText("Remove");
    fireEvent.dragOver(bin, { dataTransfer: new DataTransfer() });
    fireEvent.drop(bin, { dataTransfer: new DataTransfer() });

    await waitFor(() => {
      expect(backend.state.playlists[0]?.tracks).toHaveLength(2);
    });
    expect(backend.state.playlists[0]?.tracks[0]).toBe("/dav/Koofr/Music/xtal.m4a");
  });

  /**
   * And the half a phone actually has.
   *
   * HTML5 drag does not exist on touch — `dragstart` never fires — so the test
   * above proves nothing about the device this is for. Hold, then move, and
   * the row comes with you; release it over the panel and it is gone. Moving
   * before the hold arms is a scroll, which is the case after this one.
   *
   * `getBoundingClientRect` answers zero for everything in jsdom, which would
   * put the panel at the origin and make every release land on it. Its box is
   * stubbed so a test can miss the panel as well as hit it.
   */
  it("removes a track held and dragged onto the panel with a finger", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    render(<Playlist id="p1" onOpen={() => {}} />);

    const first = (await screen.findAllByRole("listitem"))[0]!;

    vi.useFakeTimers();
    try {
      fireEvent.pointerDown(first, { button: 0, clientX: 20, clientY: 60 });
      act(() => {
        vi.advanceTimersByTime(600);
      });
      act(() => {
        window.dispatchEvent(finger("pointermove", 20, 74));
      });

      atBottom(screen.getByText("Remove"));

      act(() => {
        window.dispatchEvent(finger("pointermove", 200, 550));
      });
      act(() => {
        window.dispatchEvent(finger("pointerup", 200, 550));
      });
    } finally {
      vi.useRealTimers();
    }

    await waitFor(() => {
      expect(backend.state.playlists[0]?.tracks).toHaveLength(2);
    });
    expect(backend.state.playlists[0]?.tracks[0]).toBe("/dav/Koofr/Music/xtal.m4a");
    // And it did not play on the way out. A row plays when it is tapped, and
    // a gesture that ends in a click on the row it started from would play the
    // track it was taking away.
    expect(backend.called("play_tracks")).toBe(false);
  });

  /**
   * A press is not a pick-up until it moves.
   *
   * `useLongPress` marks any press that outlives the hold delay as having done
   * something, which on a mouse is an ordinary slow click — so reading that
   * flag to decide whether to play would stop a deliberate press working at
   * all. Only an actual pick-up swallows the click.
   */
  it("still plays a row on a press that outlasts the hold", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    render(<Playlist id="p1" onOpen={() => {}} />);

    const first = (await screen.findAllByRole("listitem"))[0]!;
    const open = within(first).getByRole("button", { name: /^Windowlicker/ });

    vi.useFakeTimers();
    try {
      fireEvent.pointerDown(open, { button: 0, clientX: 20, clientY: 60 });
      act(() => {
        vi.advanceTimersByTime(900);
      });
      fireEvent.pointerUp(open, { button: 0, clientX: 20, clientY: 60 });
      fireEvent.click(open);
    } finally {
      vi.useRealTimers();
    }

    await waitFor(() => expect(backend.called("play_tracks")).toBe(true));
    expect(backend.called("remove_playlist_track")).toBe(false);
  });

  /** Let go anywhere else and the track stays where it was. */
  it("keeps the track when the finger comes up short of the panel", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    render(<Playlist id="p1" onOpen={() => {}} />);

    const first = (await screen.findAllByRole("listitem"))[0]!;

    vi.useFakeTimers();
    try {
      fireEvent.pointerDown(first, { button: 0, clientX: 20, clientY: 60 });
      act(() => {
        vi.advanceTimersByTime(600);
      });
      act(() => {
        window.dispatchEvent(finger("pointermove", 20, 74));
      });

      atBottom(screen.getByText("Remove"));

      act(() => {
        window.dispatchEvent(finger("pointerup", 200, 300));
      });
    } finally {
      vi.useRealTimers();
    }

    await new Promise((r) => setTimeout(r, 50));
    expect(backend.called("remove_playlist_track")).toBe(false);
    expect(screen.queryByText("Remove")).toBeNull();
  });

  it("reorders with the buttons, so the keyboard is not left out", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} />);

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
    render(<Playlist id="p1" onOpen={() => {}} />);

    const items = await screen.findAllByRole("listitem");
    expect(
      within(items[0]!).getByRole("button", { name: /move .+ up/i }),
    ).toBeDisabled();
    expect(
      within(items[items.length - 1]!).getByRole("button", { name: /move .+ down/i }),
    ).toBeDisabled();
  });

  it("surfaces a refused rename rather than pretending it worked", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    backend.fail("rename_playlist", "A playlist needs a name.");
    const user = userEvent.setup();
    render(<Playlist id="p1" onOpen={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "Late Night" }));
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

    // The empty state is an instruction now, not a statement of absence — the
    // "+" beside it is the thing to press.
    expect(await screen.findByText(/create a playlist/i)).toBeInTheDocument();
  });

  /**
   * A failed read is not an empty library.
   *
   * The rail used to `.catch(() => setPlaylists([]))`, so a call that never
   * reached the backend rendered as "None yet" — someone who had just made a
   * playlist was told they had none, and the playlist was on disk the whole
   * time. Under `tauri dev` the app restarts on every backend edit, and a call
   * in flight across a restart fails, which is exactly when somebody is
   * looking.
   */
  it("does not report a failed read as having none", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    backend.fail("playlists", "the backend went away");
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    await waitFor(() =>
      expect(screen.getByText(/could not read your playlists/i)).toBeInTheDocument(),
    );
    // And it does not sit there inviting a playlist to be made as though the
    // read had succeeded and found none.
    expect(screen.queryByText(/create a playlist/i)).toBeNull();
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
    data.setData(drag.dragType("track"), "{not json at all");
    target.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: data }),
    );

    await new Promise((r) => setTimeout(r, 50));
    expect(backend.called("add_tracks_to_playlist")).toBe(false);
    // Still on screen rather than having thrown during the drop handler.
    expect(screen.getByText("Late Night")).toBeInTheDocument();
  });
});

/**
 * Playlist folders.
 *
 * `playlist_folder_service.gd` was ported to `vapor-library` at migration time
 * — `FolderStore`, and `folder_id` on a playlist, both tested — and the shell
 * exposed neither, so `folderId` reached the frontend as a field nothing could
 * set. The handover names this exact failure: a parameter carried across
 * without its behaviour.
 */
describe("Playlist rail — folders", () => {
  function folder(over: Partial<core.Folder> = {}): core.Folder {
    return { id: "f1", name: "Sets", parentId: "", ...over };
  }

  /** A drag carrying a playlist id, as the rail's own rows produce one. */
  function dragPlaylistOnto(element: HTMLElement, id: string) {
    const data = new DataTransfer();
    data.setData(PLAYLIST_DRAG_TYPE, id);
    element.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: data }),
    );
  }

  it("draws a folder with the playlists filed in it", async () => {
    useBackend({
      folders: [folder()],
      playlists: [
        playlist({ id: "p1", name: "Late Night", folderId: "f1" }),
        playlist({ id: "p2", name: "Loose", folderId: "" }),
      ],
    });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const group = (await screen.findByRole("button", { name: /^Sets/ })).closest(
      "section",
    );
    expect(group).not.toBeNull();
    expect(within(group!).getByText("Late Night")).toBeInTheDocument();
    // The one outside it is not drawn inside it.
    expect(within(group!).queryByText("Loose")).not.toBeInTheDocument();
    expect(screen.getByText("Loose")).toBeInTheDocument();
  });

  it("collapses and reopens a folder", async () => {
    const user = userEvent.setup();
    useBackend({
      folders: [folder()],
      playlists: [playlist({ id: "p1", name: "Late Night", folderId: "f1" })],
    });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const head = await screen.findByRole("button", { name: /^Sets/ });
    await user.click(head);
    expect(screen.queryByText("Late Night")).not.toBeInTheDocument();

    await user.click(head);
    expect(await screen.findByText("Late Night")).toBeInTheDocument();
  });

  it("creates a folder", async () => {
    const backend = useBackend({ playlists: [] });
    const user = userEvent.setup();
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    await user.click(await screen.findByRole("button", { name: /new folder/i }));
    await user.type(screen.getByRole("textbox"), "Sets{Enter}");

    await waitFor(() => expect(backend.state.folders).toHaveLength(1));
    expect(backend.state.folders[0]?.name).toBe("Sets");
    // A folder is not a playlist, however similar the two boxes look.
    expect(backend.called("create_playlist")).toBe(false);
  });

  it("files a playlist into a folder when it is dragged onto one", async () => {
    const backend = useBackend({
      folders: [folder()],
      playlists: [playlist({ id: "p1", name: "Late Night", folderId: "" })],
    });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const group = (await screen.findByRole("button", { name: /^Sets/ })).closest(
      "section",
    );
    dragPlaylistOnto(group!, "p1");

    await waitFor(() =>
      expect(backend.state.playlists[0]?.folderId).toBe("f1"),
    );
    expect(await screen.findByText(/moved late night to sets/i)).toBeInTheDocument();
  });

  /** Without a target for "no folder", a playlist filed once is filed forever. */
  it("takes a playlist back out again", async () => {
    const backend = useBackend({
      folders: [folder()],
      playlists: [playlist({ id: "p1", name: "Late Night", folderId: "f1" })],
    });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    dragPlaylistOnto(await screen.findByText(/not in a folder/i), "p1");

    await waitFor(() => expect(backend.state.playlists[0]?.folderId).toBe(""));
  });

  /**
   * Deleting a container must not delete what it contains, and the prompt has
   * to say so — otherwise a person declines a safe action fearing an unsafe
   * one.
   */
  it("deletes a folder and keeps the playlists that were in it", async () => {
    const backend = useBackend({
      folders: [folder()],
      playlists: [playlist({ id: "p1", name: "Late Night", folderId: "f1" })],
    });
    const user = userEvent.setup();
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    await user.click(
      await screen.findByRole("button", { name: /delete folder sets/i }),
    );

    // The question says what survives, so a safe action is not declined out of
    // fear of an unsafe one.
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent(/move back to the top level/i);
    await user.click(within(dialog).getByRole("button", { name: /^delete$/i }));

    await waitFor(() => expect(backend.state.folders).toHaveLength(0));
    expect(backend.state.playlists).toHaveLength(1);
    expect(backend.state.playlists[0]?.folderId).toBe("");
    // And it is still on screen, at the top level now.
    expect(await screen.findByText("Late Night")).toBeInTheDocument();
  });

  it("does not delete a folder when the question is declined", async () => {
    const backend = useBackend({ folders: [folder()], playlists: [] });
    const user = userEvent.setup();
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    await user.click(
      await screen.findByRole("button", { name: /delete folder sets/i }),
    );

    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: /cancel/i }));

    await waitFor(() =>
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument(),
    );
    expect(backend.called("delete_folder")).toBe(false);
  });
});

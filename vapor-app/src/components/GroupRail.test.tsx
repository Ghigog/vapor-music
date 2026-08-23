/**
 * Dragging an artist, an album or a genre onto a rail.
 *
 * The group rail's own empty state has always read "drag an artist, album or
 * genre onto it", and until now no part of the app implemented that gesture:
 * the rail had no drop handlers, and the only drag source was the Songs table,
 * which carries tracks. `lib/drag` had the rule for a group the whole time —
 * `accepts` refuses a bare track — and nothing could reach it.
 *
 * So these are the cases that were quiet when wrong: a group that takes what
 * it is made of, a group that turns away what it is not, and an album dropped
 * on a *playlist*, which is not a passthrough but a lookup — a playlist wants
 * the tracks on that album, not the word.
 */
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { GroupRail } from "./GroupRail";
import { PlaylistRail } from "./PlaylistRail";
import * as drag from "../lib/drag";
import type * as core from "../lib/core";
import { useBackend } from "../test/setup";

function group(over: Partial<core.DynamicGroup> = {}): core.DynamicGroup {
  return { id: "g1", name: "Ambient", entities: [], ...over };
}

/** A drag of one entity, as an album tile in the library produces one. */
function dropEntity(
  element: HTMLElement,
  kind: "artist" | "album" | "genre",
  name: string,
) {
  const data = new DataTransfer();
  drag.writeDrag(data, { kind, values: [name], label: name });
  element.dispatchEvent(
    new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: data }),
  );
}

/** Whether the target would take this drag — what decides the highlight. */
function wouldTake(element: HTMLElement, kind: drag.DragPayload["kind"]): boolean {
  const data = new DataTransfer();
  drag.writeDrag(data, { kind, values: ["whatever"], label: "whatever" });
  const event = new DragEvent("dragover", {
    bubbles: true,
    cancelable: true,
    dataTransfer: data,
  });
  element.dispatchEvent(event);
  // A target claims a drop by cancelling the event. One that does not is
  // saying "not mine", and the cursor shows a no-entry sign on its own.
  return event.defaultPrevented;
}

describe("Group rail — dropping onto a group", () => {
  it("adds an album dropped on it", async () => {
    const backend = useBackend({ groups: [group()] });
    render(<GroupRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /ambient/i });
    dropEntity(target, "album", "Selected Ambient Works");

    expect(
      await screen.findByText(/added selected ambient works to ambient/i),
    ).toBeInTheDocument();
    expect(backend.called("add_to_group")).toBe(true);
  });

  it("adds an artist dropped on it", async () => {
    useBackend({ groups: [group()] });
    render(<GroupRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /ambient/i });
    dropEntity(target, "artist", "Aphex Twin");

    expect(
      await screen.findByText(/added aphex twin to ambient/i),
    ).toBeInTheDocument();
  });

  /*
   * The same reason the playlist rail says it: a drop that changed nothing and
   * a drop that missed look identical, and a person who cannot tell them apart
   * drags it again.
   */
  it("says when the group already holds what was dropped", async () => {
    useBackend({
      groups: [
        group({ entities: [{ entityType: "artist", value: "Aphex Twin" }] }),
      ],
    });
    render(<GroupRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /ambient/i });
    dropEntity(target, "artist", "Aphex Twin");

    expect(
      await screen.findByText(/aphex twin is already in ambient/i),
    ).toBeInTheDocument();
  });

  /*
   * Turned away in `dragover`, not at the drop. A group that lights up under a
   * track and then explains why it cannot have it is worse than one that never
   * lights up: the highlight is a promise.
   */
  it("does not offer to take a track", async () => {
    useBackend({ groups: [group()] });
    render(<GroupRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /ambient/i });
    expect(wouldTake(target, "track")).toBe(false);
    expect(wouldTake(target, "album")).toBe(true);
  });

  /*
   * The touch path has no cursor to refuse with — it puts the thing down and
   * finds out — so the sentence still has to exist.
   */
  it("explains itself when a track is dropped on it anyway", async () => {
    const backend = useBackend({ groups: [group()] });
    render(<GroupRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /ambient/i });
    const data = new DataTransfer();
    drag.writeDrag(data, {
      kind: "track",
      values: ["/dav/Koofr/Music/xtal.m4a"],
      label: "Xtal",
    });
    target.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: data }),
    );

    expect(
      await screen.findByText(/holds artists, albums and genres/i),
    ).toBeInTheDocument();
    expect(backend.called("add_to_group")).toBe(false);
  });

  it("ignores a drag that is not ours", async () => {
    const backend = useBackend({ groups: [group()] });
    render(<GroupRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /ambient/i });
    const data = new DataTransfer();
    data.setData("text/plain", "some text from somewhere else");
    target.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: data }),
    );

    await new Promise((r) => setTimeout(r, 50));
    expect(backend.called("add_to_group")).toBe(false);
  });
});

/**
 * The other half of the same drag.
 *
 * A playlist takes every kind, because everything resolves to tracks. An album
 * dropped on one has to become the tracks on that album — dropping the word
 * "Selected Ambient Works" into a list of hrefs would add nothing playable.
 */
describe("Playlist rail — dropping an entity", () => {
  function playlist(over: Partial<core.Playlist> = {}): core.Playlist {
    return {
      id: "p1",
      name: "Late Night",
      customCoverPath: "",
      tracks: [],
      folderId: "",
      ...over,
    };
  }

  /*
   * What is asserted is the *question asked of the library*, not a count. The
   * fake does no filtering on purpose — that is `vapor_library::index`, which
   * tests it, and a fake that filtered a second way would be a second opinion
   * about the same thing. What belongs to this side is asking for the right
   * scope, and that the answer becomes hrefs rather than the word "album".
   */
  it("asks the library for that album's tracks, and adds those", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /late night/i });
    dropEntity(target, "album", "Selected Ambient Works");

    expect(await screen.findByText(/added \d+ to late night/i)).toBeInTheDocument();
    expect(backend.lastArgs("library_view")?.view).toMatchObject({
      album: "Selected Ambient Works",
      groupBy: "none",
    });
    // The hrefs, not the album's name.
    expect(backend.lastArgs("add_tracks_to_playlist")?.hrefs).toEqual(
      expect.arrayContaining(["/dav/Koofr/Music/xtal.m4a"]),
    );
  });

  it("scopes an artist drop to that artist", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /late night/i });
    dropEntity(target, "artist", "Aphex Twin");

    expect(await screen.findByText(/added \d+ to late night/i)).toBeInTheDocument();
    expect(backend.lastArgs("library_view")?.view).toMatchObject({
      artist: "Aphex Twin",
      groupBy: "none",
    });
  });

  /*
   * A genre is the one kind the index has no filter for, so `tracksFor` runs a
   * text query and then keeps only the rows whose genre actually matches —
   * without which a search for "Ambient" would also collect a track merely
   * called "Ambient". That last step is this side's, so it is tested here: the
   * default library has no genres set, and none of it may come through.
   */
  it("keeps only rows whose genre matches, not everything the search returned", async () => {
    const backend = useBackend({ playlists: [playlist()] });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /late night/i });
    dropEntity(target, "genre", "Ambient");

    expect(
      await screen.findByText(/ambient has no tracks to add/i),
    ).toBeInTheDocument();
    expect(backend.called("add_tracks_to_playlist")).toBe(false);
  });

  it("offers to take a track, unlike a group", async () => {
    useBackend({ playlists: [playlist()] });
    render(<PlaylistRail activeId={null} onOpen={() => {}} />);

    const target = await screen.findByRole("button", { name: /late night/i });
    expect(wouldTake(target, "track")).toBe(true);
    expect(wouldTake(target, "album")).toBe(true);
  });
});

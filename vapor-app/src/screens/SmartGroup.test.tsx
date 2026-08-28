/**
 * One dynamic group.
 *
 * Written for POL-6, which is the observation that a playlist and a group are
 * one feature presented as two — different title sizes, a download button on
 * one of them, Play and Delete on one of them, and a rename gesture on one of
 * them that a phone cannot perform. Play went from both and Delete is on both;
 * the rest matched up. The cases here are the ones that go quiet
 * when that drifts apart again: they assert what this screen has *in common*
 * with `Playlist`, so a change to one of them fails beside a change to the
 * other.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SmartGroup } from "./SmartGroup";
import type * as core from "../lib/core";
import { useBackend } from "../test/setup";

function group(over: Partial<core.DynamicGroup> = {}): core.DynamicGroup {
  return {
    id: "g1",
    name: "Ambient",
    entities: [{ entityType: "artist", value: "Aphex Twin" }],
    ...over,
  };
}

describe("SmartGroup — matching the playlist screen", () => {
  /**
   * A heading, and one you can press.
   *
   * The title was a bare button; the playlist's was an `h1` at a different
   * size. Both are an `h1` you press to rename now, so a screen reader gets a
   * heading on both and a finger gets the same gesture on both.
   */
  it("puts the name in a heading you press to rename", async () => {
    const backend = useBackend({ groups: [group()] });
    const user = userEvent.setup();
    render(<SmartGroup id="g1" onGone={() => {}} />);

    expect(
      await screen.findByRole("heading", { name: "Ambient" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Ambient" }));
    const box = screen.getByRole("textbox", { name: /group name/i });
    await user.clear(box);
    await user.type(box, "Deep Ambient{Enter}");

    await waitFor(() => {
      expect(backend.lastArgs("rename_group")?.name).toBe("Deep Ambient");
    });
    expect(
      await screen.findByRole("heading", { name: "Deep Ambient" }),
    ).toBeInTheDocument();
  });

  /**
   * The download button was drawn only once a group resolved to something,
   * so an empty group had one fewer control than an empty playlist — which is
   * exactly the difference POL-6 is about. It is drawn either way now and
   * disables itself when there is nothing to keep.
   */
  it("offers to keep the group on the device even before it resolves to anything", async () => {
    // An empty library as well as an empty group, so the group resolves to no
    // tracks at all — which is the state the button used not to be drawn in.
    useBackend({ groups: [group({ entities: [] })], rows: [] });
    render(<SmartGroup id="g1" onGone={() => {}} />);

    const keep = await screen.findByRole("button", { name: /download/i });
    expect(keep).toBeInTheDocument();
    // Present, and honest about having nothing to keep — which is a different
    // thing from being absent, and the difference POL-6 is about.
    expect(keep).toBeDisabled();
  });

  it("has no Play button, as the playlist screen has none", async () => {
    useBackend({ groups: [group()] });
    render(<SmartGroup id="g1" onGone={() => {}} />);

    await screen.findByRole("heading", { name: "Ambient" });
    expect(screen.queryByRole("button", { name: /^play$/i })).toBeNull();
  });

  /**
   * Delete, which this screen never had.
   *
   * POL-6 matched the two screens by taking Delete off the playlist, which
   * made them match at nothing: `delete_group` had never had a caller and
   * `delete_playlist` no longer did, so a group made by mistake could not be
   * unmade from anywhere in the app. Both screens have it now.
   */
  it("deletes the group, once the confirmation is answered", async () => {
    const backend = useBackend({ groups: [group()] });
    const user = userEvent.setup();
    let gone = false;
    render(<SmartGroup id="g1" onGone={() => { gone = true; }} />);

    await screen.findByRole("heading", { name: "Ambient" });
    await user.click(screen.getByRole("button", { name: /^delete$/i }));

    expect(await screen.findByRole("alertdialog")).toBeInTheDocument();
    expect(backend.called("delete_group")).toBe(false);

    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", {
        name: /^delete$/i,
      }),
    );

    await waitFor(() => expect(backend.called("delete_group")).toBe(true));
    expect(backend.lastArgs("delete_group")?.id).toBe("g1");
    await waitFor(() => expect(gone).toBe(true));
  });

  it("deletes nothing when the confirmation is declined", async () => {
    const backend = useBackend({ groups: [group()] });
    const user = userEvent.setup();
    render(<SmartGroup id="g1" onGone={() => {}} />);

    await screen.findByRole("heading", { name: "Ambient" });
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await user.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", {
        name: /cancel/i,
      }),
    );

    expect(backend.called("delete_group")).toBe(false);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  /** The same instruction the playlist's empty state gives, for the same
   *  reason: there is no sidebar at the width this is read on (POL-7). */
  it("says what to do when it is empty, naming the bar and not a sidebar", async () => {
    useBackend({ groups: [group({ entities: [] })] });
    render(<SmartGroup id="g1" onGone={() => {}} />);

    expect(
      await screen.findByText(/artists, albums or genres/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/bar at the bottom/i)).toBeInTheDocument();
    expect(screen.queryByText(/sidebar/i)).toBeNull();
  });
});

/**
 * The lyrics panel, and the one piece of arithmetic in it.
 *
 * Which line is "now" is the whole feature: get it wrong by one and the words
 * under the reader's eye are the words they have already heard.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { LyricsPanel, activeLine } from "./LyricsPanel";
import { useBackend } from "../test/setup";
import type * as core from "../lib/core";

const LINES: core.LyricLine[] = [
  { time: 0, text: "first" },
  { time: 10, text: "second" },
  { time: 20, text: "" },
  { time: 30, text: "fourth" },
];

describe("activeLine", () => {
  it("is nothing before the first line", () => {
    expect(activeLine(LINES, -1)).toBe(-1);
  });

  /** A line lands *on* its timestamp, not after it. */
  it("takes a line the instant its time arrives", () => {
    expect(activeLine(LINES, 0)).toBe(0);
    expect(activeLine(LINES, 10)).toBe(1);
  });

  /** And holds until the next one, which is what a lyric line means. */
  it("holds a line until the next begins", () => {
    expect(activeLine(LINES, 9.99)).toBe(0);
    expect(activeLine(LINES, 19.5)).toBe(1);
  });

  it("stays on the last line to the end of the track", () => {
    expect(activeLine(LINES, 3000)).toBe(3);
  });

  it("has no line to hold when there are none", () => {
    expect(activeLine([], 12)).toBe(-1);
  });
});

describe("LyricsPanel", () => {
  const A_TRACK = "/dav/Koofr/Music/windowlicker.m4a";

  it("says where the switch is when lookups are off", async () => {
    useBackend();
    render(<LyricsPanel href={A_TRACK} position={0} />);

    // Not a blank panel: that reads as "this track has no words".
    await waitFor(() =>
      expect(screen.getByText(/turned off/i)).toBeInTheDocument(),
    );
    expect(screen.getByText(/settings/i)).toBeInTheDocument();
  });

  it("shows the words, and marks the one being sung", async () => {
    useBackend({
      metadataLookup: true,
      lyrics: {
        [A_TRACK]: { synced: true, lines: LINES, plain: "" },
      },
    });
    const { rerender } = render(<LyricsPanel href={A_TRACK} position={0} />);

    await waitFor(() => expect(screen.getByText("first")).toBeInTheDocument());
    expect(screen.getByText("first").className).toContain("--now");

    rerender(<LyricsPanel href={A_TRACK} position={12} />);
    await waitFor(() =>
      expect(screen.getByText("second").className).toContain("--now"),
    );
    // And the one before it is behind us, not still lit.
    expect(screen.getByText("first").className).toContain("--past");
  });

  /**
   * Unsynced words must not pretend to follow along. Highlighting a guessed
   * line puts the wrong words under the eye at the moment someone looks.
   */
  it("does not follow along without timings", async () => {
    useBackend({
      metadataLookup: true,
      lyrics: {
        [A_TRACK]: { synced: false, lines: [], plain: "just the words" },
      },
    });
    render(<LyricsPanel href={A_TRACK} position={45} />);

    await waitFor(() =>
      expect(screen.getByText("just the words")).toBeInTheDocument(),
    );
    expect(document.querySelector(".lyrics__line--now")).toBeNull();
    expect(screen.getByText(/not timed/i)).toBeInTheDocument();
  });
});

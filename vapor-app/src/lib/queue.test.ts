import { describe, expect, it } from "vitest";

import { upNextOf } from "./queue";
import type { QueueEntry, QueueView } from "./core";

function entry(href: string, current: boolean): QueueEntry {
  return { href, title: href, artist: "", bpm: 0, key: "", current };
}

function view(hrefs: string[], current: number | null): QueueView {
  return {
    entries: hrefs.map((h, i) => entry(h, i === current)),
    repeat: "off",
    shuffled: false,
    current,
    remainingSecs: 0,
  };
}

describe("upNextOf", () => {
  it("drops everything already played", () => {
    // Four records in, the fifth playing. Up next is the sixth onwards.
    const v = view(["a", "b", "c", "d", "e", "f", "g"], 4);
    const { first, entries } = upNextOf(v);

    expect(entries.map((e) => e.href)).toEqual(["f", "g"]);
    expect(first).toBe(5);
  });

  it("keeps the offset back into the queue", () => {
    // The offset is what reordering and removal are addressed by, so the first
    // track shown must still know its own position in the whole set.
    const v = view(["a", "b", "c"], 0);
    const { first, entries } = upNextOf(v);

    expect(first).toBe(1);
    expect(v.entries[first]?.href).toBe(entries[0]?.href);
  });

  it("treats nothing playing as a whole set still to come", () => {
    const { first, entries } = upNextOf(view(["a", "b"], null));

    expect(first).toBe(0);
    expect(entries.map((e) => e.href)).toEqual(["a", "b"]);
  });

  it("is empty at the end of the set", () => {
    expect(upNextOf(view(["a", "b"], 1)).entries).toEqual([]);
  });
});

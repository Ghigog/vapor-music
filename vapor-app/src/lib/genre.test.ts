import { describe, expect, it } from "vitest";
import { artistWithGenre, UNKNOWN_GENRE } from "./genre";

describe("artistWithGenre", () => {
  it("names both when both are known", () => {
    expect(artistWithGenre("Excision", "Dubstep")).toBe("Excision - Dubstep");
  });

  /*
   * The case the whole change exists for.
   *
   * A missing genre is a finding, not a gap to tidy away: it is the difference
   * between the app having the genre wrong and never having had one, and those
   * have different fixes. A label that quietly printed the artist alone would
   * look identical to one whose genre happened to be known.
   */
  it("says so out loud when the genre is missing", () => {
    expect(artistWithGenre("Willie Wright", "")).toBe(
      `Willie Wright - ${UNKNOWN_GENRE}`,
    );
  });

  /*
   * The two halves keep the conventions they already had.
   *
   * An unknown artist has rendered as a dash everywhere in this app since the
   * tables were written; an unknown genre is a sentence. Making them match
   * would mean changing one of them, and neither change belongs to this.
   */
  it("keeps the table's dash for an unknown artist", () => {
    expect(artistWithGenre("", "Folk")).toBe("— - Folk");
    expect(artistWithGenre("", "")).toBe(`— - ${UNKNOWN_GENRE}`);
  });
});

/**
 * How a track's genre reads on a screen.
 *
 * Added 2026-09-08, after a folk record turned up in the middle of a dubstep
 * set and there was no way to tell, from any screen, whether the app had the
 * genre wrong or simply did not have one. Those are different faults with
 * different fixes — a correction, or a lookup that never ran — and the screens
 * could not distinguish them because they never showed a genre at all.
 */

/**
 * The word for a track whose genre the app does not have.
 *
 * Said rather than left blank, and this is the point of the whole change: an
 * absent genre is a *finding*, not a gap to tidy away. A row that quietly
 * printed the artist alone would look identical to one whose genre happened to
 * be missing, which is the state that needs to be visible.
 */
export const UNKNOWN_GENRE = "unknown genre";

/**
 * `Artist - Genre`, with either half allowed to be missing.
 *
 * The shell flattens its placeholders — "Other", "N/A" and friends come across
 * as empty, because that is what `genre_distance` scores them as — so an empty
 * `genre` here means the planner had no genre either, and the label can say so
 * without qualification.
 *
 * An unknown *artist* still renders as the dash the tables use, so the two
 * halves keep their existing separate conventions rather than inventing a
 * third.
 */
export function artistWithGenre(artist: string, genre: string): string {
  return `${artist || "—"} - ${genre || UNKNOWN_GENRE}`;
}

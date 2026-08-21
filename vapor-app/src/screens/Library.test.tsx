/**
 * The Library's album cover, and the toggle it carries.
 *
 * A file's embedded artwork can simply be wrong — the owner's copy of one album
 * carries an unrelated picture, and reading the file more carefully cannot fix
 * that. The only thing that knows the picture is wrong is the person looking at
 * it, so the cover exists to let them say so and have the app go and ask.
 *
 * Tested for what a person does: open an album, see a cover, tap it, get a
 * different cover, change their mind and tap again.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Library } from "./Library";
import { useBackend } from "../test/setup";
import { makeEntity, makeRow } from "../test/ipc";
import type * as core from "../lib/core";

const FOUND =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mNkAAIAAAoAAv/lxKUAAAAASUVORK5CYII=";

/** Two tracks in one album folder, as a real album is. */
function album() {
  return [
    makeRow({
      href: "/dav/Music/Tame%20Impala/Currents/01.mp3",
      title: "Let It Happen",
      artist: "Tame Impala",
      album: "Currents",
    }),
    makeRow({
      href: "/dav/Music/Tame%20Impala/Currents/02.mp3",
      title: "Nangs",
      artist: "Tame Impala",
      album: "Currents",
    }),
  ];
}

/**
 * The album entity a scan of `album()` produces.
 *
 * Stated, not derived. The grouping rule — an album is its title plus the
 * folder its tracks sit in — belongs to `vapor_library` and is tested there;
 * restating it in the fake gave two implementations that could disagree.
 */
function currents(): core.LibraryEntity[] {
  return [
    makeEntity({
      name: "Currents",
      subtitle: "Tame Impala",
      tracks: 2,
      lead: "/dav/Music/Tame%20Impala/Currents/01.mp3",
    }),
  ];
}

async function openTheAlbum(user: ReturnType<typeof userEvent.setup>) {
  // Via the Albums tab, because the screen opens on the home shelves now and
  // those draw albums too — with the same accessible name on the same tile.
  // Without this the artwork tests below would be exercising `Home` while
  // saying they are about the grid.
  await user.click(await screen.findByRole("tab", { name: /^albums$/i }));
  // By the accessible name, as a person reaches it: the tile's cover is the
  // button and the title beside it is a label, not a control.
  await user.click(
    await screen.findByRole("button", { name: /open the album currents/i }),
  );
}

describe("Library — album artwork", () => {
  /** The cover is the control: tapping it swaps where the picture comes from. */
  function cover() {
    return screen.findByRole("button", { name: /cover of currents/i });
  }

  it("says where the picture is from and where tapping sends the query", async () => {
    useBackend({ rows: album(), albums: currents(), covers: true });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    // The control has to say where the query goes. This is the one part of the
    // app that talks to a stranger.
    expect(await cover()).toHaveAccessibleName(/search deezer/i);
    expect(screen.getByText(/^from file$/i)).toBeInTheDocument();
  });

  it("replaces the cover with what the search found", async () => {
    useBackend({
      rows: album(), albums: currents(),
      covers: true,
      albumArtSearch: { Currents: FOUND },
    });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    await user.click(await cover());

    await waitFor(() => {
      expect(screen.getByAltText(/cover of currents/i)).toHaveAttribute("src", FOUND);
    });
    // And it says the cover is no longer the file's.
    expect(screen.getByText(/^deezer$/i)).toBeInTheDocument();
  });

  /// The undo matters as much as the action: album search is fuzzy, and a wrong
  /// match with no way back is worse than the wrong artwork it replaced. The
  /// same control undoes it, which is what makes it a toggle rather than a
  /// button with a second button hiding underneath it.
  it("can be sent back to the file's own artwork by tapping again", async () => {
    useBackend({
      rows: album(), albums: currents(),
      covers: true,
      albumArtSearch: { Currents: FOUND },
    });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    await user.click(await cover());
    await waitFor(() => {
      expect(screen.getByAltText(/cover of currents/i)).toHaveAttribute("src", FOUND);
    });

    await user.click(await cover());

    await waitFor(() => {
      expect(screen.getByAltText(/cover of currents/i)).not.toHaveAttribute("src", FOUND);
    });
    expect(screen.getByText(/^from file$/i)).toBeInTheDocument();
  });

  /// Nothing matching is an ordinary outcome — tags and services disagree about
  /// spelling all the time — and it has to be said on screen rather than
  /// leaving the control stuck on "Searching…".
  it("says so when the search finds nothing", async () => {
    useBackend({ rows: album(), albums: currents(), covers: true, albumArtSearch: {} });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    await user.click(await cover());

    expect(await screen.findByRole("alert")).toHaveTextContent(/nothing came back/i);
    // Usable again rather than left disabled.
    expect(await cover()).toBeEnabled();
  });

  /// An album whose files carry no picture at all is the fallback case, and the
  /// cover is exactly where it can be fixed — including when there is no
  /// picture there to tap yet.
  it("still offers the search when the files have no artwork", async () => {
    useBackend({
      rows: album(), albums: currents(),
      covers: false,
      albumArtSearch: { Currents: FOUND },
    });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    await user.click(await cover());

    await waitFor(() => {
      expect(screen.getByAltText(/cover of currents/i)).toHaveAttribute("src", FOUND);
    });
  });

  /// The artist view has no album to search for, so it must not offer to.
  it("does not offer artwork search on an artist", async () => {
    useBackend({
      rows: album(),
      albums: currents(),
      artists: [
        makeEntity({
          name: "Tame Impala",
          subtitle: "1 album",
          tracks: 2,
          lead: "/dav/Music/Tame%20Impala/Currents/01.mp3",
        }),
      ],
      covers: true,
    });
    const user = userEvent.setup();
    render(<Library />);

    await user.click(await screen.findByRole("tab", { name: /^artists$/i }));
    await user.click(
      await screen.findByRole("button", { name: /open the artist tame impala/i }),
    );

    await screen.findByRole("button", { name: /‹ artists/i });
    expect(
      screen.queryByRole("button", { name: /cover of/i }),
    ).not.toBeInTheDocument();
  });
});

describe("Library — album identity", () => {
  /// Two albums that share a title are two albums. Grouping by title alone
  /// merged them into one tile with one cover — latent in the owner's library
  /// today, and certain to bite eventually.
  /*
   * Two records can share a title, and the grid has to draw both.
   *
   * That they arrive as two entities rather than one is the backend's rule —
   * an album is its title plus its folder — and `vapor_library` tests it. What
   * is this grid's to get wrong is what it does when they do arrive: the cards
   * were keyed on name alone, so React treated the second as a duplicate of the
   * first and warned about it. Hence two entities in, two cards expected out.
   */
  it("keeps two albums with the same name apart", async () => {
    useBackend({
      rows: [
        makeRow({
          href: "/dav/Music/Queen/Greatest%20Hits/01.mp3",
          title: "Bohemian Rhapsody",
          artist: "Queen",
          album: "Greatest Hits",
        }),
        makeRow({
          href: "/dav/Music/Abba/Greatest%20Hits/01.mp3",
          title: "Dancing Queen",
          artist: "Abba",
          album: "Greatest Hits",
        }),
      ],
      albums: [
        makeEntity({
          name: "Greatest Hits",
          subtitle: "Queen",
          lead: "/dav/Music/Queen/Greatest%20Hits/01.mp3",
        }),
        makeEntity({
          name: "Greatest Hits",
          subtitle: "Abba",
          lead: "/dav/Music/Abba/Greatest%20Hits/01.mp3",
        }),
      ],
      covers: true,
    });
    const user = userEvent.setup();
    render(<Library />);
    await user.click(await screen.findByRole("tab", { name: /^albums$/i }));

    await waitFor(() => {
      expect(screen.getAllByText("Greatest Hits")).toHaveLength(2);
    });
    // And each names its own artist rather than one claiming both.
    expect(screen.getByText("Queen")).toBeInTheDocument();
    expect(screen.getByText("Abba")).toBeInTheDocument();
  });
});

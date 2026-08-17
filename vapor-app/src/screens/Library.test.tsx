/**
 * The Library's album artwork panel.
 *
 * A file's embedded artwork can simply be wrong — the owner's copy of one album
 * carries an unrelated picture, and reading the file more carefully cannot fix
 * that. The only thing that knows the picture is wrong is the person looking at
 * it, so the panel exists to let them say so and have the app go and ask.
 *
 * Tested for what a person does: open an album, see a cover, press the button,
 * get a different cover, change their mind.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Library } from "./Library";
import { useBackend } from "../test/setup";
import { makeRow } from "../test/ipc";

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

async function openTheAlbum(user: ReturnType<typeof userEvent.setup>) {
  // By the accessible name, as a person reaches it: the tile's cover is the
  // button and the title beside it is a label, not a control.
  await user.click(
    await screen.findByRole("button", { name: /open the album currents/i }),
  );
}

describe("Library — album artwork", () => {
  it("offers to find artwork, and says what pressing it does", async () => {
    useBackend({ rows: album(), covers: true });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    expect(
      await screen.findByRole("button", { name: /find artwork/i }),
    ).toBeInTheDocument();
    // The label has to say where the query goes. This is the one part of the
    // app that talks to a stranger.
    expect(screen.getByText(/sends the artist and album name to deezer/i)).toBeInTheDocument();
  });

  it("replaces the cover with what the search found", async () => {
    useBackend({
      rows: album(),
      covers: true,
      albumArtSearch: { Currents: FOUND },
    });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    await user.click(await screen.findByRole("button", { name: /find artwork/i }));

    await waitFor(() => {
      expect(screen.getByAltText(/cover of currents/i)).toHaveAttribute("src", FOUND);
    });
    // And it says the cover is no longer the file's.
    expect(screen.getByText(/using artwork found on deezer/i)).toBeInTheDocument();
  });

  /// The undo matters as much as the action: album search is fuzzy, and a wrong
  /// match with no way back is worse than the wrong artwork it replaced.
  it("can be sent back to the file's own artwork", async () => {
    useBackend({
      rows: album(),
      covers: true,
      albumArtSearch: { Currents: FOUND },
    });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    await user.click(await screen.findByRole("button", { name: /find artwork/i }));
    await waitFor(() => {
      expect(screen.getByAltText(/cover of currents/i)).toHaveAttribute("src", FOUND);
    });

    await user.click(screen.getByRole("button", { name: /file’s own artwork/i }));

    await waitFor(() => {
      expect(screen.getByAltText(/cover of currents/i)).not.toHaveAttribute("src", FOUND);
    });
    expect(screen.getByText(/using the artwork inside the files/i)).toBeInTheDocument();
  });

  /// Nothing matching is an ordinary outcome — tags and services disagree about
  /// spelling all the time — and it has to be said on screen rather than
  /// leaving the button stuck on "Searching…".
  it("says so when the search finds nothing", async () => {
    useBackend({ rows: album(), covers: true, albumArtSearch: {} });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    await user.click(await screen.findByRole("button", { name: /find artwork/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/nothing came back/i);
    // The button is usable again rather than left disabled.
    expect(screen.getByRole("button", { name: /find artwork/i })).toBeEnabled();
  });

  /// An album whose files carry no picture at all is the fallback case, and the
  /// panel is exactly where it can be fixed.
  it("still offers the search when the files have no artwork", async () => {
    useBackend({
      rows: album(),
      covers: false,
      albumArtSearch: { Currents: FOUND },
    });
    const user = userEvent.setup();
    render(<Library />);
    await openTheAlbum(user);

    await user.click(await screen.findByRole("button", { name: /find artwork/i }));

    await waitFor(() => {
      expect(screen.getByAltText(/cover of currents/i)).toHaveAttribute("src", FOUND);
    });
  });

  /// The artist view has no album to search for, so it must not offer to.
  it("does not offer artwork search on an artist", async () => {
    useBackend({ rows: album(), covers: true });
    const user = userEvent.setup();
    render(<Library />);

    await user.click(await screen.findByRole("tab", { name: /^artists$/i }));
    await user.click(
      await screen.findByRole("button", { name: /open the artist tame impala/i }),
    );

    await screen.findByRole("button", { name: /‹ artists/i });
    expect(
      screen.queryByRole("button", { name: /find artwork/i }),
    ).not.toBeInTheDocument();
  });
});

describe("Library — album identity", () => {
  /// Two albums that share a title are two albums. Grouping by title alone
  /// merged them into one tile with one cover — latent in the owner's library
  /// today, and certain to bite eventually.
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
      covers: true,
    });
    render(<Library />);

    await waitFor(() => {
      expect(screen.getAllByText("Greatest Hits")).toHaveLength(2);
    });
    // And each names its own artist rather than one claiming both.
    expect(screen.getByText("Queen")).toBeInTheDocument();
    expect(screen.getByText("Abba")).toBeInTheDocument();
  });
});

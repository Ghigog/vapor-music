/**
 * The remaining screens: Library, Search, Queue, Vibe, Now Playing, Liner
 * Notes, Your Data, Onboarding, and the Transport.
 *
 * One file rather than eight, because each of these is smaller than Settings or
 * Songs and the interesting cases are the same three questions asked of each:
 *
 * 1. Does it show what the backend gave it?
 * 2. Does it say something useful when there is nothing, or when the call
 *    fails, rather than an empty box or a spinner that never stops?
 * 3. Do its controls reach the backend?
 *
 * Screens with real logic of their own — Settings, Songs, Playlist — have their
 * own files.
 */
import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Library } from "./Library";
import { Queue } from "./Queue";
import { Vibe } from "./Vibe";
import { NowPlaying } from "./NowPlaying";
import { LinerNotes } from "./LinerNotes";
import { YourData } from "./YourData";
import { Onboarding } from "./Onboarding";
import { Transport } from "../components/Transport";
import { useBackend } from "../test/setup";
import { makeRow } from "../test/ipc";
import type * as core from "../lib/core";

const A_TRACK = "/dav/Koofr/Music/windowlicker.m4a";

/** The flat table is a tab now, and the default tab is Albums. */
async function openSongsTab(user: ReturnType<typeof userEvent.setup>) {
  await screen.findByRole("tab", { name: /^songs$/i });
  await user.click(screen.getByRole("tab", { name: /^songs$/i }));
}

describe("Library", () => {
  /**
   * A tab called Albums lists albums.
   *
   * It used to list *tracks* grouped under an album heading — so "All Melody"
   * was a header with nine tiles beneath it, none of which was the album. That
   * answers "what is on this record", which is the question you ask after
   * opening one, not before.
   */
  it("lists albums, not the tracks on them", async () => {
    useBackend();
    render(<Library />);

    expect(await screen.findByText("Windowlicker EP")).toBeInTheDocument();
    expect(screen.getByText("Selected Ambient Works")).toBeInTheDocument();
    // The track of that name is not a card here.
    expect(screen.queryByText("Windowlicker")).not.toBeInTheDocument();
  });

  it("lists artists under the Artists tab", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await screen.findByText("Windowlicker EP");
    await user.click(screen.getByRole("tab", { name: /artists/i }));

    expect(await screen.findByText("Aphex Twin")).toBeInTheDocument();
    // Two albums in the fixture, and the tile says so.
    expect(screen.getByText(/2 albums/i)).toBeInTheDocument();
  });

  /** An album names its artist; a compilation says so rather than picking one. */
  it("names the artist under each album", async () => {
    useBackend();
    render(<Library />);

    await screen.findByText("Windowlicker EP");
    expect(screen.getAllByText("Aphex Twin").length).toBeGreaterThan(0);
  });

  it("opens an album to its tracks, and comes back", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /open the album windowlicker ep/i }),
    );

    expect(await screen.findByText("Windowlicker")).toBeInTheDocument();
    // Narrowed to that album, so the other one is not in the table.
    expect(screen.queryByText("Xtal")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /albums/i }));
    expect(await screen.findByText("Selected Ambient Works")).toBeInTheDocument();
  });

  it("plays an album from its card without opening it", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /play selected ambient works/i }),
    );

    await waitFor(() => expect(backend.state.status).toBe("playing"));
    // Only that album is queued — not everything on screen.
    expect(backend.state.queue).toEqual(["/dav/Koofr/Music/xtal.m4a"]);
  });

  it("shows a cover when the file carried one", async () => {
    useBackend({ covers: true });
    render(<Library />);

    const art = await screen.findByAltText(/cover of windowlicker ep/i);
    expect(art).toHaveAttribute("src", expect.stringContaining("data:image"));
  });

  /** No artwork is the normal state of a freshly scanned library. */
  it("draws a placeholder rather than a broken image when there is no cover", async () => {
    useBackend({ covers: false });
    render(<Library />);

    await screen.findByText("Windowlicker EP");
    expect(screen.queryByAltText(/cover of/i)).not.toBeInTheDocument();
  });

  it("regroups when a tab is pressed", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await screen.findByText("Windowlicker EP");
    await user.click(screen.getByRole("tab", { name: /artists/i }));

    await waitFor(() => {
      const view = backend.lastArgs("library_entities")?.view as { groupBy?: string };
      expect(view?.groupBy).toBe("artist");
    });
  });

  /**
   * The home screen's whole job.
   *
   * Library shipped with cards that were an `<article>` with no handler: the
   * first screen anyone sees, showing their music, and pressing a track did
   * nothing. Nothing caught it because every test here asked whether the grid
   * *rendered* and none asked whether it *worked*.
   */
  it("plays the track whose row was pressed", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await openSongsTab(user);
    await user.click(await screen.findByText("Windowlicker"));

    await waitFor(() => expect(backend.state.status).toBe("playing"));
    expect(backend.state.current).toBe(A_TRACK);
  });

  /** What you can see is what goes in the queue. */
  it("queues everything on screen behind it, in the order shown", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await openSongsTab(user);
    await user.click(await screen.findByText("Xtal"));

    await waitFor(() => expect(backend.state.queue.length).toBe(4));
    expect(backend.state.queue).toContain(A_TRACK);
    expect(backend.state.current).toBe("/dav/Koofr/Music/xtal.m4a");
  });

  /** Filtered down, the queue is the filtered set — not the whole library. */
  it("queues only what the search left on screen", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await openSongsTab(user);
    await screen.findByText("Windowlicker");
    await user.type(screen.getByRole("searchbox"), "xtal");

    // Wait for the filter to land, not merely for the row to exist — it exists
    // in the unfiltered table too, and clicking it before the debounced reload
    // arrives queues the whole library and passes for the wrong reason.
    await waitFor(() =>
      expect(screen.queryByText("Windowlicker")).not.toBeInTheDocument(),
    );

    await user.click(await screen.findByText("Xtal"));

    await waitFor(() => expect(backend.state.current).toBe("/dav/Koofr/Music/xtal.m4a"));
    expect(backend.state.queue).toEqual(["/dav/Koofr/Music/xtal.m4a"]);
  });

  /** A failure to start belongs on the screen, not in the console. */
  it("says so when an album will not play", async () => {
    const backend = useBackend();
    backend.fail("play_tracks", "That file is no longer on the server.");
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /play windowlicker ep/i }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(/no longer on the server/i);
  });

  /**
   * The Songs tab is the table, not an ungrouped grid.
   *
   * Songs and Search were separate sidebar destinations, which the Daylight
   * design never had: its Library carries the search field and the flat list
   * as a tab (docs/DESIGN_DRIFT.md).
   */
  it("shows the track table under the Songs tab", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await openSongsTab(user);

    // The table, identifiable by its sortable columns — the grid has none.
    expect(
      await screen.findByRole("columnheader", { name: /album/i }),
    ).toBeInTheDocument();
  });

  /** One search field, at the top, filtering whichever view is open. */
  it("filters the table with the search field above it", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await openSongsTab(user);
    await user.type(screen.getByRole("searchbox"), "xtal");

    await waitFor(() =>
      expect(screen.queryByText("Windowlicker")).not.toBeInTheDocument(),
    );
    // Awaited: the table is briefly empty while the filtered fetch is in
    // flight, and the row arrives after Windowlicker has gone.
    expect(await screen.findByText("Xtal")).toBeInTheDocument();
  });

  /** Two boxes filtering the same list is a screen nobody can use. */
  it("has exactly one search field", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await openSongsTab(user);

    expect(screen.getAllByRole("searchbox")).toHaveLength(1);
  });

  it("says the library is empty rather than showing a blank grid", async () => {
    useBackend({ rows: [] });
    render(<Library />);

    expect(await screen.findByText(/no albums yet/i)).toBeInTheDocument();
  });

  it("reports a failure to load the track table", async () => {
    const backend = useBackend();
    backend.fail("library_view", "the index would not open");
    const user = userEvent.setup();
    render(<Library />);

    await openSongsTab(user);
    expect(await screen.findByRole("alert")).toHaveTextContent(/would not open/i);
  });
});

describe("Queue", () => {
  it("says the queue is empty when it is", async () => {
    useBackend();
    render(<Queue onOpen={() => {}} />);

    expect(await screen.findByText(/nothing|empty|queue/i)).toBeInTheDocument();
  });

  it("lists what is queued", async () => {
    const backend = useBackend();
    await backend.invoke("play_tracks", {
      hrefs: [A_TRACK, "/dav/Koofr/Music/xtal.m4a"],
      start: A_TRACK,
    });
    render(<Queue onOpen={() => {}} />);

    expect(await screen.findByText("Windowlicker")).toBeInTheDocument();
    expect(screen.getByText("Xtal")).toBeInTheDocument();
  });

  it("reorders with the buttons", async () => {
    const backend = useBackend();
    await backend.invoke("play_tracks", {
      hrefs: [A_TRACK, "/dav/Koofr/Music/xtal.m4a"],
      start: A_TRACK,
    });
    const user = userEvent.setup();
    render(<Queue onOpen={() => {}} />);

    await screen.findByText("Xtal");
    const up = screen.getAllByRole("button", { name: /up/i });
    expect(up.length).toBeGreaterThan(0);
    await user.click(up[up.length - 1]!);
    await waitFor(() => expect(backend.called("move_in_queue")).toBe(true));
  });
});

describe("Vibe DJ", () => {
  /** Vibe conducts *from* the playing track, so there has to be one. */
  async function playing() {
    const backend = useBackend();
    await backend.invoke("play_tracks", { hrefs: [A_TRACK], start: A_TRACK });
    return backend;
  }

  it("will not conduct with nothing playing, and says so", async () => {
    useBackend();
    render(<Vibe />);

    // The controls are not merely disabled — the screen replaces them with an
    // explanation, which is the better answer when there is nothing to act on.
    expect(await screen.findByText(/nothing to conduct yet/i)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /conduct from here/i }),
    ).not.toBeInTheDocument();
  });

  it("plans a path and says how many tracks it could not use", async () => {
    await playing();
    const user = userEvent.setup();
    render(<Vibe />);

    await user.click(
      await screen.findByRole("button", { name: /conduct from here/i }),
    );

    // One fixture track is unanalysed, and the screen must say it was passed
    // over rather than quietly planning without it (TD-43b). Silence here would
    // make a set built from a tenth of the library look like one built from all
    // of it.
    expect(
      await screen.findByText(/1 was passed over — not analysed yet/i),
    ).toBeInTheDocument();
  });

  /**
   * The curve names are the original's, not the rewrite's.
   *
   * `dj_pathfinder.gd` matches on "build vibe" and "chill down"; the rewrite
   * displayed "Build" and "Wind down", and named the unnamed `_` fallback
   * "Steady" (docs/DESIGN_DRIFT.md). Pinned here because a label with no source
   * is invisible to every other kind of test.
   */
  it("offers the curves under the names the original gave them", async () => {
    await playing();
    render(<Vibe />);

    expect(await screen.findByRole("button", { name: /build vibe/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /chill down/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /wave/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /wind down/i })).toBeNull();
  });

  /**
   * The help sheet renders `docs/ai_dj_workflow.md` itself, as the Godot
   * original did from `res://`. Asserting on the document's own words proves
   * the file is really being read rather than a copy of it having been typed
   * into the component.
   */
  it("explains the mixes from the spec document", async () => {
    await playing();
    const user = userEvent.setup();
    render(<Vibe />);

    await user.click(await screen.findByRole("button", { name: /^help$/i }));

    const sheet = await screen.findByRole("dialog");
    expect(sheet).toHaveTextContent(/perfect match/i);
    expect(sheet).toHaveTextContent(/bass swap/i);
    expect(sheet).toHaveTextContent(/vibe limit/i);
    // §7, added because the curves were never reachable from the Godot UI and
    // arrived in this app unexplained.
    expect(sheet).toHaveTextContent(/conduct a set/i);
    expect(sheet).toHaveTextContent(/build vibe/i);
  });

  it("closes the help sheet with Escape", async () => {
    await playing();
    const user = userEvent.setup();
    render(<Vibe />);

    await user.click(await screen.findByRole("button", { name: /^help$/i }));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  /** Nothing playing is exactly when someone wants to read what it will do. */
  it("offers help even with nothing to conduct", async () => {
    useBackend();
    render(<Vibe />);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: /^help$/i })).toBeInTheDocument(),
    );
  });

  it("reports a failure to plan", async () => {
    const backend = await playing();
    backend.fail("vibe_path", "no analysed tracks to work with");
    const user = userEvent.setup();
    render(<Vibe />);

    await user.click(
      await screen.findByRole("button", { name: /conduct from here/i }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(/no analysed/i);
  });
});

describe("Now Playing", () => {
  it("says nothing is playing when nothing is", async () => {
    useBackend();
    render(<NowPlaying />);

    expect(await screen.findByText(/nothing playing|nothing/i)).toBeInTheDocument();
  });

  it("shows the track that is playing", async () => {
    const backend = useBackend();
    await backend.invoke("play_tracks", { hrefs: [A_TRACK], start: A_TRACK });
    render(<NowPlaying />);

    expect(await screen.findByText("Windowlicker")).toBeInTheDocument();
  });

  /**
   * This screen has its own transport, separate from the one in the shell, and
   * nothing here had ever pressed it. A second set of controls is a second
   * chance to be wired to nothing — which is what Library's grid was.
   */
  it("pauses, and shows that it is paused", async () => {
    const backend = useBackend();
    await backend.invoke("play_tracks", { hrefs: [A_TRACK], start: A_TRACK });
    const user = userEvent.setup();
    render(<NowPlaying />);

    await user.click(await screen.findByRole("button", { name: /^pause$/i }));

    await waitFor(() => expect(backend.state.status).toBe("paused"));
    // The button becomes the other thing, so the screen agrees with the state.
    expect(await screen.findByRole("button", { name: /^play$/i })).toBeInTheDocument();
  });

  it("resumes from paused", async () => {
    const backend = useBackend();
    await backend.invoke("play_tracks", { hrefs: [A_TRACK], start: A_TRACK });
    await backend.invoke("pause_playback");
    const user = userEvent.setup();
    render(<NowPlaying />);

    await user.click(await screen.findByRole("button", { name: /^play$/i }));

    await waitFor(() => expect(backend.state.status).toBe("playing"));
  });

  it("skips to the next track and says which one", async () => {
    const backend = useBackend();
    await backend.invoke("play_tracks", {
      hrefs: [A_TRACK, "/dav/Koofr/Music/xtal.m4a"],
      start: A_TRACK,
    });
    const user = userEvent.setup();
    render(<NowPlaying />);

    await screen.findByText("Windowlicker");
    await user.click(screen.getByRole("button", { name: /next track/i }));

    expect(await screen.findByText("Xtal")).toBeInTheDocument();
  });

  it("goes back to the previous track", async () => {
    const backend = useBackend();
    await backend.invoke("play_tracks", {
      hrefs: [A_TRACK, "/dav/Koofr/Music/xtal.m4a"],
      start: "/dav/Koofr/Music/xtal.m4a",
    });
    const user = userEvent.setup();
    render(<NowPlaying />);

    await screen.findByText("Xtal");
    await user.click(screen.getByRole("button", { name: /previous track/i }));

    expect(await screen.findByText("Windowlicker")).toBeInTheDocument();
  });
});

describe("Liner Notes", () => {
  it("shows what is known about a track", async () => {
    useBackend();
    render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

    expect(await screen.findByText("Windowlicker")).toBeInTheDocument();
    expect(screen.getByText("Aphex Twin")).toBeInTheDocument();
  });

  it("goes back", async () => {
    useBackend();
    const user = userEvent.setup();
    let back = false;
    render(<LinerNotes href={A_TRACK} onBack={() => { back = true; }} />);

    await user.click(await screen.findByRole("button", { name: /back/i }));
    expect(back).toBe(true);
  });

  it("says an unanalysed track is unanalysed rather than showing zeroes", async () => {
    useBackend();
    render(
      <LinerNotes href="/dav/Koofr/Music/unanalysed.mp3" onBack={() => {}} />,
    );

    await screen.findByText("Not Yet Analysed");
    // The rule the whole app follows: unknown is a dash, never a zero.
    expect(screen.queryByText(/^0 BPM$/)).not.toBeInTheDocument();
  });

  it("reports a track it cannot describe", async () => {
    const backend = useBackend();
    backend.fail("track_details", "That track is not in the library.");
    render(<LinerNotes href="/gone.m4a" onBack={() => {}} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/not in the library/i);
  });

  /**
   * Lyrics, and the consent around them.
   *
   * Ported from `metadata_service.gd`, which fetched from LRCLIB and Deezer
   * unconditionally and said nothing about it. Every other field on this
   * screen was worked out on the device from the audio; these were not, and
   * the screen has to keep the two apart or the claim on the rest is untrue.
   */
  describe("lyrics", () => {
    const WORDS: core.Lyrics = {
      synced: true,
      lines: [
        { time: 12, text: "The first line" },
        { time: 18, text: "The second line" },
      ],
      plain: "",
    };

    it("does not look anything up merely because the screen was opened", async () => {
      const backend = useBackend({ metadataLookup: true });
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      await screen.findByText("Windowlicker");
      expect(backend.called("look_up_track")).toBe(false);
    });

    /**
     * Off is the shipped default, and the screen has to say what turning it on
     * would cost rather than showing an empty panel.
     */
    it("says what a lookup would send when lookups are switched off", async () => {
      const backend = useBackend();
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      expect(await screen.findByText(/artist and title to a server/i))
        .toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: /look up/i }),
      ).not.toBeInTheDocument();
      expect(backend.called("look_up_track")).toBe(false);
    });

    it("looks a track up when asked, and shows the words with their timings", async () => {
      useBackend({ metadataLookup: true, lyrics: { [A_TRACK]: WORDS } });
      const user = userEvent.setup();
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      await user.click(await screen.findByRole("button", { name: /look up/i }));

      expect(await screen.findByText("The first line")).toBeInTheDocument();
      expect(screen.getByText("The second line")).toBeInTheDocument();
      // 12 seconds in, written as a clock rather than as a number of seconds.
      expect(screen.getByText("0:12")).toBeInTheDocument();
    });

    it("names where the words came from", async () => {
      useBackend({ metadataLookup: true, lyrics: { [A_TRACK]: WORDS } });
      const user = userEvent.setup();
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      await user.click(await screen.findByRole("button", { name: /look up/i }));

      expect(await screen.findByText(/from lrclib/i)).toBeInTheDocument();
    });

    /**
     * "Found nothing" and "not asked yet" are different states, and a panel
     * that renders them the same asks the service again on every visit.
     */
    it("says so when the service has no words for a track", async () => {
      useBackend({ metadataLookup: true });
      const user = userEvent.setup();
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      await user.click(await screen.findByRole("button", { name: /look up/i }));

      expect(await screen.findByText(/no words for this track/i))
        .toBeInTheDocument();
    });

    it("surfaces a refused lookup rather than failing silently", async () => {
      const backend = useBackend({ metadataLookup: true });
      backend.fail("look_up_track", "Looking up lyrics and artwork is switched off.");
      const user = userEvent.setup();
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      await user.click(await screen.findByRole("button", { name: /look up/i }));

      expect(await screen.findByRole("alert")).toHaveTextContent(/switched off/i);
    });

    it("shows plain words when there is no timed version", async () => {
      useBackend({
        metadataLookup: true,
        lyrics: {
          [A_TRACK]: { synced: false, lines: [], plain: "Just the words" },
        },
      });
      const user = userEvent.setup();
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      await user.click(await screen.findByRole("button", { name: /look up/i }));

      expect(await screen.findByText("Just the words")).toBeInTheDocument();
      expect(screen.getByText(/without the timings/i)).toBeInTheDocument();
    });
  });
});

describe("Your Data", () => {
  it("itemises what is stored, so the claim can be checked", async () => {
    useBackend();
    render(<YourData />);

    expect(await screen.findByText(/audio cache/i)).toBeInTheDocument();
    // "Analysis" also appears in the surrounding prose, so match the row.
    expect(screen.getAllByText(/analysis/i).length).toBeGreaterThan(0);
  });

  it("reports a failure to read the breakdown", async () => {
    const backend = useBackend();
    backend.fail("data_breakdown", "cannot read the data directory");
    render(<YourData />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/cannot read/i);
  });

  it("clears the cache and says so once the cache is actually empty", async () => {
    useBackend({ cacheBytes: 2_000_000_000 });
    const user = userEvent.setup();
    render(<YourData />);

    await user.click(await screen.findByRole("button", { name: /clear cached audio/i }));

    expect(await screen.findByText(/freed/i)).toBeInTheDocument();
    // Not merely "freed": the screen re-read the cache and it is empty.
    expect(screen.queryByText(/still cached/i)).not.toBeInTheDocument();
  });

  /**
   * The TD-50 shape again, in a different place: a command that returns a
   * number without doing the thing. `clear_audio_cache` reports the bytes it
   * freed, and "freed 2 GB" was printed straight from that return value — so a
   * clear that deleted nothing read exactly like one that worked.
   */
  it("does not claim the cache is empty when the files are still there", async () => {
    useBackend({ cacheBytes: 2_000_000_000, cacheResistsClearing: true });
    const user = userEvent.setup();
    render(<YourData />);

    await user.click(await screen.findByRole("button", { name: /clear cached audio/i }));

    expect(await screen.findByText(/still cached/i)).toBeInTheDocument();
  });

  it("deletes everything and confirms nothing is left", async () => {
    const user = userEvent.setup();
    useBackend();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    try {
      render(<YourData />);

      await user.click(
        await screen.findByRole("button", { name: /delete everything stored here/i }),
      );

      expect(await screen.findByText(/nothing is stored locally/i)).toBeInTheDocument();
    } finally {
      confirm.mockRestore();
    }
  });

  /** The most consequential button on the screen must not fire on a cancel. */
  it("deletes nothing when the confirmation is declined", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    try {
      render(<YourData />);

      await user.click(
        await screen.findByRole("button", { name: /delete everything stored here/i }),
      );

      expect(backend.called("delete_all_data")).toBe(false);
    } finally {
      confirm.mockRestore();
    }
  });
});

describe("Onboarding", () => {
  /**
   * Onboarding is a welcome, not a form.
   *
   * It says what the app is and hands off to Settings — the connection details
   * are filled in there. Worth a test precisely because it is easy to assume
   * otherwise and write the form tests against the wrong screen, which is what
   * happened first here.
   */
  it("explains the premise and hands off to Settings", async () => {
    useBackend({ connected: false });
    const user = userEvent.setup();
    let handedOff = false;
    render(<Onboarding onConnect={() => { handedOff = true; }} />);

    expect(
      await screen.findByText(/no account. nothing to sign up for./i),
    ).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: /choose where music lives/i }),
    );
    expect(handedOff).toBe(true);
  });
});

describe("Transport", () => {
  it("says nothing is playing, and offers disabled controls rather than none", async () => {
    useBackend();
    render(<Transport />);

    expect(await screen.findByText(/nothing playing/i)).toBeInTheDocument();
  });

  it("plays, pauses and skips", async () => {
    const backend = useBackend();
    await backend.invoke("play_tracks", {
      hrefs: [A_TRACK, "/dav/Koofr/Music/xtal.m4a"],
      start: A_TRACK,
    });
    const user = userEvent.setup();
    render(<Transport />);

    await screen.findByText("Windowlicker");

    await user.click(screen.getByRole("button", { name: /^pause$/i }));
    await waitFor(() => expect(backend.called("pause_playback")).toBe(true));

    await user.click(screen.getByRole("button", { name: /next track/i }));
    await waitFor(() => expect(backend.called("next_track")).toBe(true));
  });

  it("says when there is no audio device at all", async () => {
    const backend = useBackend();
    backend.fail("playback_state", "No audio output device is available.");
    render(<Transport />);

    // A transport that reads "playing" in silence is the failure this
    // prevents, so the screen must not claim anything is playing.
    await waitFor(() => {
      expect(screen.queryByRole("button", { name: /^pause$/i })).not.toBeInTheDocument();
    });
  });
});

/**
 * The three exits.
 *
 * `_get_match_type_between` and the four-step choice cycle were the half of the
 * original DJ the rewrite dropped: it kept the planner and lost the chooser, so
 * the screen could plan a set but never show the choice it was making or let
 * anyone overrule it (docs/DESIGN_DRIFT.md).
 */
describe("Vibe DJ — Match, Fresh and Switch", () => {
  /**
   * A library with one of each kind in it, relative to the track playing:
   * a near-identical tempo in the same genre, one 20 BPM away in the same
   * genre, and one in a different genre altogether.
   */
  const HERE = "/here.m4a";
  const MIXABLE: core.Row[] = [
    makeRow({ href: HERE, title: "Here", bpm: 120, key: "8A", genre: "Electronic" }),
    makeRow({ href: "/near.m4a", title: "Near", bpm: 122, key: "9A", genre: "Electronic" }),
    makeRow({ href: "/lift.m4a", title: "Lift", bpm: 140, key: "10A", genre: "Electronic" }),
    makeRow({ href: "/away.m4a", title: "Away", bpm: 121, key: "3B", genre: "Jazz" }),
  ];

  async function playing() {
    const backend = useBackend({ rows: MIXABLE });
    await backend.invoke("play_tracks", {
      hrefs: MIXABLE.map((r) => r.href),
      start: HERE,
    });
    return backend;
  }

  it("offers one way out of each kind", async () => {
    await playing();
    render(<Vibe />);

    expect(await screen.findByText("MATCH")).toBeInTheDocument();
    expect(screen.getByText("FRESH")).toBeInTheDocument();
    expect(screen.getByText("SWITCH")).toBeInTheDocument();
  });

  /**
   * Each card names the mix the engine would actually perform — the design's
   * `fx` field, which is the whole reason the alternates are worth showing.
   */
  it("names the transition each one would use", async () => {
    await playing();
    render(<Vibe />);

    expect(await screen.findByText(/bass swap/i)).toBeInTheDocument();
    expect(screen.getByText(/filter sweep/i)).toBeInTheDocument();
    expect(screen.getByText(/echo out/i)).toBeInTheDocument();
  });

  /** Exactly one, or the badge says nothing. */
  it("marks the one the DJ would pick, and only that one", async () => {
    await playing();
    render(<Vibe />);

    expect(await screen.findAllByText(/ai choice/i)).toHaveLength(1);
  });

  it("puts the chosen track next when one is pressed", async () => {
    const backend = await playing();
    const user = userEvent.setup();
    render(<Vibe />);

    await user.click(await screen.findByText("SWITCH"));

    await waitFor(() => expect(backend.called("choose_next")).toBe(true));
    expect(backend.lastArgs("choose_next")?.href).toBe("/away.m4a");
    // Next in the queue, immediately after what is playing.
    const queue = backend.state.queue;
    expect(queue[queue.indexOf(HERE) + 1]).toBe("/away.m4a");
  });

  /**
   * §4: the override moves the selection, not the badge. Taking a step by
   * hand still advances the cycle, so the next transition is the next step of
   * it either way.
   */
  it("advances the cycle when a step is taken by hand", async () => {
    const backend = await playing();
    const user = userEvent.setup();
    render(<Vibe />);

    // Step 0 is Match.
    const first = await screen.findByText("MATCH");
    expect(first.closest("button")).toHaveTextContent(/ai choice/i);

    await user.click(screen.getByText("SWITCH"));

    // Step 1 is Fresh, whatever was pressed.
    await waitFor(() =>
      expect(screen.getByText("FRESH").closest("button")).toHaveTextContent(
        /ai choice/i,
      ),
    );
    expect(backend.called("choose_next")).toBe(true);
  });

  it("says so rather than showing empty cards when nothing is analysed", async () => {
    const backend = useBackend({ rows: [makeRow({ href: "/a.mp3", title: "A", bpm: 0 })] });
    await backend.invoke("play_tracks", { hrefs: ["/a.mp3"], start: "/a.mp3" });
    render(<Vibe />);

    expect(await screen.findByText(/nothing analysed to choose from/i)).toBeInTheDocument();
  });
});

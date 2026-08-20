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

  /**
   * Playing a record conducts within it.
   *
   * Queueing only the album was never enough on its own: the DJ plans past the
   * end of the queue, and with no scope it planned out of the whole library —
   * so the second track was from somewhere else and the record you pressed
   * play on lasted one song.
   */
  it("conducts within the album that was played", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await user.click(
      await screen.findByRole("button", { name: /play selected ambient works/i }),
    );

    await waitFor(() => expect(backend.state.status).toBe("playing"));
    expect(backend.lastArgs("play_tracks")?.scope).toBe("Selected Ambient Works");
  });

  /** An unfiltered list is the library, and says so by naming no scope. */
  it("names no scope when the whole library was played from", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await openSongsTab(user);
    await user.click(await screen.findByText("Windowlicker"));

    await waitFor(() => expect(backend.state.status).toBe("playing"));
    expect(backend.lastArgs("play_tracks")?.scope ?? null).toBeNull();
  });

  /**
   * TD-53. A looked-up portrait was fetched, cached and shown on exactly one
   * screen. An artist tile falling back to the artwork of whichever album its
   * lead track came from looked like the app knew who the artist was.
   */
  it("shows a looked-up portrait on an artist with no artwork of its own", async () => {
    const backend = useBackend({
      covers: false,
      metadataLookup: true,
      lyrics: {
        [A_TRACK]: { synced: false, lines: [], plain: "words" },
      },
    });
    await backend.invoke("look_up_track", { href: A_TRACK, force: false });
    const user = userEvent.setup();
    render(<Library />);

    await screen.findByText("Windowlicker EP");
    await user.click(screen.getByRole("tab", { name: /artists/i }));

    expect(await screen.findByAltText(/cover of aphex twin/i)).toBeInTheDocument();
  });

  /** The file's own artwork still wins where it has any. */
  it("does not replace an artist's own artwork with a looked-up one", async () => {
    const backend = useBackend({ covers: true, metadataLookup: true });
    const user = userEvent.setup();
    render(<Library />);

    await screen.findByText("Windowlicker EP");
    await user.click(screen.getByRole("tab", { name: /artists/i }));

    await screen.findByAltText(/cover of aphex twin/i);
    expect(backend.called("artist_portrait")).toBe(false);
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
   * as a tab (docs/FINDINGS.md).
   */
  it("shows the track table under the Songs tab", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Library />);

    await openSongsTab(user);

    // The table, identifiable by its sortable columns — the grid has none.
    expect(
      await screen.findByRole("button", { name: /^album/i }),
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
    expect(screen.queryByRole("button", { name: /build vibe/i })).not.toBeInTheDocument();
  });

  /**
   * Pressing a curve *is* conducting.
   *
   * There was a "Conduct from here" button, and it was the only thing that ever
   * ran the planner — so a set was something you had to know to ask for, and
   * the DJ otherwise appended one track at a time with no idea where it was
   * going. The curve is now saved to the backend, which plans along it on its
   * own whether or not this screen is open.
   */
  it("re-plans the set when a curve is chosen, with no button to press", async () => {
    const backend = await playing();
    const user = userEvent.setup();
    render(<Vibe />);

    expect(screen.queryByRole("button", { name: /conduct from here/i })).toBeNull();

    await user.click(await screen.findByRole("button", { name: /chill down/i }));

    await waitFor(() => expect(backend.called("set_curve")).toBe(true));
    expect(backend.lastArgs("set_curve")?.curve).toBe("chill");
    expect(backend.state.settings.curve).toBe("chill");
  });

  /** The curve shown is the backend's, not a default this component invented. */
  it("shows the curve the backend is actually planning along", async () => {
    const backend = await playing();
    await backend.invoke("set_curve", { curve: "wave" });
    render(<Vibe />);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: /wave/i })).toHaveAttribute(
        "aria-pressed",
        "true",
      ),
    );
  });

  /**
   * The curve names are the original's, not the rewrite's.
   *
   * `dj_pathfinder.gd` matches on "build vibe" and "chill down"; the rewrite
   * displayed "Build" and "Wind down", and named the unnamed `_` fallback
   * "Steady" (docs/FINDINGS.md). Pinned here because a label with no source
   * is invisible to every other kind of test.
   */
  it("offers the curves under the names the original gave them", async () => {
    await playing();
    render(<Vibe />);

    expect(await screen.findByRole("button", { name: /build vibe/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /chill down/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /wave/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /hold steady/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /wind down/i })).toBeNull();

    // The arithmetic that used to sit under each label is gone — "Energy +0.4,
    // tempo +15 BPM across the set" is true and is not what anyone is choosing
    // between. The shape and the colour say it instead.
    expect(screen.queryByText(/tempo \+15 BPM/i)).toBeNull();
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

    await user.click(await screen.findByRole("button", { name: /how the dj chooses/i }));

    const sheet = await screen.findByRole("dialog");
    expect(sheet).toHaveTextContent(/bass swap/i);
    expect(sheet).toHaveTextContent(/vibe limit/i);
    // §7, added because the curves were never reachable from the Godot UI and
    // arrived in this app unexplained.
    expect(sheet).toHaveTextContent(/conduct a set/i);
    expect(sheet).toHaveTextContent(/build vibe/i);
    // The document describes the exits the app actually has. The help sheet
    // renders it verbatim, so a stale document is a screen explaining a feature
    // that is not there — which is what it was, describing a "Perfect Match"
    // class and a four-step cycle that had both been deleted.
    expect(sheet).toHaveTextContent(/\bStay\b/);
    expect(sheet).toHaveTextContent(/\bFollow\b/);
    expect(sheet).toHaveTextContent(/\bSwitch\b/);
    expect(sheet.textContent).not.toMatch(/Perfect Match/i);
  });

  /**
   * A hard-wrapped source line is not a paragraph break.
   *
   * The renderer emitted one `<p>` per line of the markdown, and the document
   * is wrapped at about eighty columns — so sentences were cut in half with a
   * paragraph gap through the middle, and a bullet with a continuation line
   * became a bullet plus a stray fragment beneath the list ("in." on its own).
   */
  it("renders a wrapped sentence as one paragraph", async () => {
    await playing();
    const user = userEvent.setup();
    render(<Vibe />);

    await user.click(
      await screen.findByRole("button", { name: /how the dj chooses/i }),
    );
    const sheet = await screen.findByRole("dialog");

    // This sentence crosses a wrap in `docs/ai_dj_workflow.md`. Split, no
    // single element holds it.
    const whole =
      "There are three ways out of the track that is playing, and the Vibe screen shows one record for each. Press one to send the set that way.";
    const paragraphs = Array.from(sheet.querySelectorAll("p"));
    expect(paragraphs.some((p) => p.textContent === whole)).toBe(true);

    // And a wrapped bullet stays inside its own item.
    const items = Array.from(sheet.querySelectorAll("li"));
    expect(
      items.some((li) => /^Follow — carry on\..*on its own\.$/.test(li.textContent ?? "")),
    ).toBe(true);
  });

  it("closes the help sheet with Escape", async () => {
    await playing();
    const user = userEvent.setup();
    render(<Vibe />);

    await user.click(await screen.findByRole("button", { name: /how the dj chooses/i }));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  /** Nothing playing is exactly when someone wants to read what it will do. */
  it("offers help even with nothing to conduct", async () => {
    useBackend();
    render(<Vibe />);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: /how the dj chooses/i })).toBeInTheDocument(),
    );
  });

  /**
   * The label named the hardware.
   *
   * "Conducting on this device" said where the sound comes out, which is not
   * a question anyone was asking and read the same whatever you pressed play
   * in. What matters is the set being conducted — and it has to be the real
   * one, because the planner is confined to it.
   */
  it("names the set it is conducting over", async () => {
    const backend = useBackend();
    await backend.invoke("play_tracks", {
      hrefs: [A_TRACK],
      start: A_TRACK,
      scope: "Windowlicker EP",
    });
    render(<Vibe />);

    expect(
      await screen.findByText(/conducting on windowlicker ep/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/this device/i)).toBeNull();
  });

  /** Playing from an unfiltered list is the library, and says so. */
  it("says the library when nothing narrower was played from", async () => {
    await playing();
    render(<Vibe />);

    expect(
      await screen.findByText(/conducting on your library/i),
    ).toBeInTheDocument();
  });

  /**
   * The switch is not on this screen any more, so this screen has to say
   * where it went. It used to render nothing at all with the DJ off, which
   * left the feature's own screen with no sign the feature existed.
   */
  it("points at the player when the DJ is switched off", async () => {
    await playing();
    render(<Vibe djMode={false} />);

    expect(
      await screen.findByText(/please enable vibe dj in your player/i),
    ).toBeInTheDocument();
    // Dimmed and covered, not removed: the curves are still there.
    expect(screen.getByRole("button", { name: /chill down/i })).toBeInTheDocument();
  });

  it("carries no DJ checkbox of its own", async () => {
    await playing();
    render(<Vibe />);

    await screen.findByRole("button", { name: /chill down/i });
    expect(screen.queryByRole("checkbox", { name: /dj/i })).toBeNull();
  });

  it("reports a failure to plan", async () => {
    const backend = await playing();
    backend.fail("set_curve", "no analysed tracks to work with");
    const user = userEvent.setup();
    render(<Vibe />);

    await user.click(await screen.findByRole("button", { name: /chill down/i }));

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
    /**
     * Asserts the behaviour, not the sentence: off is named as a state, the
     * control is absent, and nothing is sent. An earlier version matched the
     * exact wording and broke when the screen was cut down — which is rule 1
     * in `docs/TESTING.md`, written after the same mistake.
     */
    it("says lookups are off, and offers no way to trip one by accident", async () => {
      const backend = useBackend();
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      const lyrics = (await screen.findByText(/^lyrics$/i)).closest("section");
      expect(lyrics).toHaveTextContent(/off/i);
      expect(lyrics).toHaveTextContent(/settings/i);
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

    /**
     * A looked-up sleeve fills the gap when the file carries none, and says
     * so — a picture from Deezer standing in for one that came out of the
     * recording is a difference worth being able to see.
     */
    it("marks a sleeve that came from the lookup rather than the file", async () => {
      useBackend({
        covers: false,
        metadataLookup: true,
        lyrics: { [A_TRACK]: WORDS },
      });
      const user = userEvent.setup();
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      await user.click(await screen.findByRole("button", { name: /look up/i }));

      expect(await screen.findByText(/looked up/i)).toBeInTheDocument();
    });

    /** The file's own artwork wins; the lookup only fills a gap. */
    it("keeps the file's own cover when it has one", async () => {
      useBackend({
        covers: true,
        metadataLookup: true,
        lyrics: { [A_TRACK]: WORDS },
      });
      const user = userEvent.setup();
      render(<LinerNotes href={A_TRACK} onBack={() => {}} />);

      await user.click(await screen.findByRole("button", { name: /look up/i }));
      await screen.findByText("The first line");

      expect(screen.queryByText(/looked up/i)).not.toBeInTheDocument();
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

  /**
   * The DJ switch lives here now.
   *
   * It was a checkbox on the Vibe screen, which meant the control that turns
   * the app's headline feature on was inside the screen that only makes sense
   * once it is on. It acts on playback, so it sits with the playback controls.
   */
  it("toggles the DJ on and off", async () => {
    useBackend();
    const onChange = vi.fn();
    const user = userEvent.setup();
    const { rerender } = render(
      <Transport djMode={false} onDjModeChange={onChange} />,
    );

    const button = await screen.findByRole("button", { name: /turn vibe dj on/i });
    // A toggle, not a checkbox — pressed state, not a tick.
    expect(button).toHaveAttribute("aria-pressed", "false");

    await user.click(button);
    expect(onChange).toHaveBeenCalledWith(true);

    rerender(<Transport djMode={true} onDjModeChange={onChange} />);
    const on = await screen.findByRole("button", { name: /turn vibe dj off/i });
    expect(on).toHaveAttribute("aria-pressed", "true");

    await user.click(on);
    expect(onChange).toHaveBeenLastCalledWith(false);
  });

  /** Nothing to switch means no switch, rather than one that does nothing. */
  it("omits the DJ switch when there is nothing to switch", async () => {
    useBackend();
    render(<Transport />);

    await screen.findByText(/nothing playing/i);
    expect(screen.queryByRole("button", { name: /vibe dj/i })).toBeNull();
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
 * `_get_match_type_between` and the choice cycle were the half of the original
 * DJ the rewrite dropped: it kept the planner and lost the chooser, so the
 * screen could plan a set but never show the choice it was making or let anyone
 * overrule it (docs/FINDINGS.md).
 *
 * The cycle itself is gone. It rotated Match → Fresh → Match → Switch and named
 * whichever step it was on the "AI choice", which meant the badge and the queue
 * could disagree about what was coming next — and that the set and the exit
 * were two features arguing. Follow *is* the set now: the planner queues ten
 * ahead and the middle card shows the head of that queue, so the default is
 * always "carry on" and the other two are departures from it.
 */
describe("Vibe DJ — Stay, Follow and Switch", () => {
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

    expect(await screen.findByText("STAY")).toBeInTheDocument();
    expect(screen.getByText("FOLLOW")).toBeInTheDocument();
    expect(screen.getByText("SWITCH")).toBeInTheDocument();
  });

  /**
   * The middle card is the set, not a fourth opinion about it.
   *
   * This is the whole shape of the feature: Follow shows whatever the planner
   * has queued next, so the screen and the queue cannot disagree. Falsifiable —
   * change the queue and the card has to follow it.
   */
  it("shows the set's own next track as Follow", async () => {
    const backend = await playing();
    render(<Vibe />);

    const follow = (await screen.findByText("FOLLOW")).closest("button");
    const queued = backend.state.queue[backend.state.queue.indexOf(HERE) + 1];
    const row = MIXABLE.find((r) => r.href === queued);
    expect(follow).toHaveTextContent(row!.title);
    // And it is the one marked as queued, because it is the one queued.
    expect(follow).toHaveAttribute("aria-pressed", "true");
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

  /**
   * Exactly one card is marked, and it is the one actually queued.
   *
   * There used to be two marks — an "AI choice" badge and a selection ring —
   * and they could point at different tracks, which is a screen telling you two
   * things about one question. There is one now.
   */
  it("marks the queued track, and only that one", async () => {
    await playing();
    render(<Vibe />);

    await screen.findByText("FOLLOW");
    const pressed = screen
      .getAllByRole("button")
      .filter((b) => b.getAttribute("aria-pressed") === "true" && /STAY|FOLLOW|SWITCH/.test(b.textContent ?? ""));
    expect(pressed).toHaveLength(1);
    expect(screen.queryByText(/ai choice/i)).toBeNull();
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
   * Taking an exit folds it into the set.
   *
   * This is the consolidation, stated as a test: Follow is whatever is queued,
   * so overruling the DJ does not leave the screen showing one track marked and
   * a different one queued. The old behaviour left an "AI choice" badge behind
   * on whatever step the cycle was on, which is exactly that disagreement.
   */
  it("makes the exit that was taken the set's own next track", async () => {
    const backend = await playing();
    const user = userEvent.setup();
    render(<Vibe />);

    const chosen = (await screen.findByText("SWITCH")).closest("button");
    const title = chosen?.querySelector(".vibe__exit-title")?.textContent ?? "";
    expect(title).not.toBe("");

    await user.click(screen.getByText("SWITCH"));

    // It is now the middle card — the set carries on through it.
    await waitFor(() => {
      const follow = screen.getByText("FOLLOW").closest("button");
      expect(follow).toHaveTextContent(title);
      expect(follow).toHaveAttribute("aria-pressed", "true");
    });
    expect(backend.called("choose_next")).toBe(true);
  });

  it("says so rather than showing empty cards when nothing is analysed", async () => {
    const backend = useBackend({ rows: [makeRow({ href: "/a.mp3", title: "A", bpm: 0 })] });
    await backend.invoke("play_tracks", { hrefs: ["/a.mp3"], start: "/a.mp3" });
    render(<Vibe />);

    expect(await screen.findByText(/nothing analysed to choose from/i)).toBeInTheDocument();
  });
});

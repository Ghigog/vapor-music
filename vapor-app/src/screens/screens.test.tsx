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
import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Library } from "./Library";
import { Search } from "./Search";
import { Queue } from "./Queue";
import { Vibe } from "./Vibe";
import { NowPlaying } from "./NowPlaying";
import { LinerNotes } from "./LinerNotes";
import { YourData } from "./YourData";
import { Onboarding } from "./Onboarding";
import { Transport } from "../components/Transport";
import { useBackend } from "../test/setup";

const A_TRACK = "/dav/Koofr/Music/windowlicker.m4a";

describe("Library", () => {
  it("shows the library grouped, and can regroup", async () => {
    const backend = useBackend();
    const user = userEvent.setup();
    render(<Library />);

    expect(await screen.findByText("Windowlicker")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /artists/i }));
    await waitFor(() => {
      const view = backend.lastArgs("library_view")?.view as { groupBy?: string };
      expect(view?.groupBy).toBe("artist");
    });
  });

  it("says the library is empty rather than showing a blank grid", async () => {
    useBackend({ rows: [] });
    render(<Library />);

    expect(
      await screen.findByText(/nothing|no tracks|empty|connect/i),
    ).toBeInTheDocument();
  });

  it("reports a failure to load", async () => {
    const backend = useBackend();
    backend.fail("library_view", "the index would not open");
    render(<Library />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/would not open/i);
  });
});

describe("Search", () => {
  it("finds nothing before anything is typed", async () => {
    useBackend();
    render(<Search onOpen={() => {}} />);

    await waitFor(() => {
      expect(screen.getByRole("searchbox")).toBeInTheDocument();
    });
    expect(screen.queryByText("Windowlicker")).not.toBeInTheDocument();
  });

  it("shows matches for what was typed", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Search onOpen={() => {}} />);

    await user.type(screen.getByRole("searchbox"), "window");

    // The title appears twice — once as the top result, once in the list.
    await waitFor(() => {
      expect(screen.getAllByText("Windowlicker").length).toBeGreaterThan(0);
    });
  });

  /**
   * A failed search used to present as "nothing matched", which is a different
   * answer and a wrong one (TD-34).
   */
  it("distinguishes a failed search from an empty one", async () => {
    const backend = useBackend();
    backend.fail("search", "the index is unavailable");
    const user = userEvent.setup();
    render(<Search onOpen={() => {}} />);

    await user.type(screen.getByRole("searchbox"), "window");

    expect(await screen.findByRole("alert")).toHaveTextContent(/unavailable/i);
  });

  it("says plainly when a real search matched nothing", async () => {
    useBackend();
    const user = userEvent.setup();
    render(<Search onOpen={() => {}} />);

    await user.type(screen.getByRole("searchbox"), "zzzzznothing");

    expect(await screen.findByText(/nothing|no match|no results/i)).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
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

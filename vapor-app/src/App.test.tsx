/**
 * The shell.
 *
 * Most of what App does is route between screens, and each screen is tested on
 * its own. What is only testable here is the thing that spans them: the notice
 * shown when the backend could not read one of its data files at startup.
 *
 * That notice is the only difference between "you have no playlists" and "your
 * playlists could not be read". The app deliberately starts anyway — refusing
 * to open because one of fourteen files is damaged is a worse answer — so
 * without it a person is shown an empty library with no indication that their
 * data is sitting on disk, intact and merely unreadable.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { useBackend } from "./test/setup";

describe("App — startup damage", () => {
  it("says nothing on a normal launch", async () => {
    useBackend();
    render(<App />);

    // Wait for the shell to settle so this is not asserting on a frame before
    // the check has run. By name because the shell renders two navigations —
    // the sidebar and the narrow-width tab bar — and CSS, which decides which
    // of them is showing, is not applied here.
    await screen.findByRole("navigation", { name: /screens and playlists/i });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("names every file it could not read, and where the bytes went", async () => {
    useBackend({
      damaged: [
        "Your playlists file could not be read (expected value at line 1). It has been kept at /data/playlists.corrupt.json and the app started with an empty one, so nothing has been overwritten.",
        "Your tags file could not be read (EOF while parsing). It has been kept at /data/tags.corrupt.json and the app started with an empty one, so nothing has been overwritten.",
      ],
    });
    render(<App />);

    const notice = await screen.findByRole("alert");
    expect(notice).toHaveTextContent(/could not be read/i);
    // The path is the actionable half of the sentence and must survive to the
    // screen intact.
    expect(notice).toHaveTextContent("/data/playlists.corrupt.json");
    expect(notice).toHaveTextContent("/data/tags.corrupt.json");
  });

  it("can be dismissed, and stays dismissed", async () => {
    useBackend({ damaged: ["Your tags file could not be read (bad json)."] });
    const user = userEvent.setup();
    render(<App />);

    await screen.findByRole("alert");
    await user.click(screen.getByRole("button", { name: /dismiss/i }));

    await waitFor(() => {
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
  });

  /// A backend too broken to answer must not take the whole shell down with
  /// it: the notice is a courtesy, and the app is still usable without it.
  it("still renders when the check itself fails", async () => {
    const backend = useBackend();
    backend.fail("startup_problems", "the state lock is poisoned");
    render(<App />);

    expect(
      await screen.findByRole("navigation", { name: /screens and playlists/i }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

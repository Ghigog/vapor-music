/**
 * Settings — the screen where five defects reached a person in one sitting.
 *
 * Every one of them has a test here, named for the symptom rather than the
 * mechanism, so a regression reads as the thing that went wrong rather than as
 * an internal detail:
 *
 * * an error bar permanently on screen
 * * "Scan library" appearing to do nothing, its answer below the fold
 * * a rename logging the account out
 * * "no password saved" while a password sat in the box
 * * the box claiming "unchanged" when nothing was stored
 *
 * Queried by role and text throughout. A test that clicks
 * `.settings__button--primary` still passes after the button is relabelled to
 * something meaningless, which is the opposite of what a test is for.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Settings } from "./Settings";
import { useBackend } from "../test/setup";

/** The remote card, so a query cannot accidentally match another card. */
function remoteCard() {
  return screen.getByRole("heading", { name: /where your music lives/i })
    .closest("section") as HTMLElement;
}

describe("Settings — connecting a server", () => {
  it("shows nothing alarming on a clean screen", async () => {
    useBackend({ connected: true });
    render(<Settings />);

    // The defect: ErrorNotice renders whatever it is given, so an unguarded one
    // is a permanent red bar reporting a failure that never happened.
    await waitFor(() => {
      expect(screen.getByDisplayValue("https://app.koofr.net")).toBeInTheDocument();
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("saves what was typed", async () => {
    const backend = useBackend({ connected: false });
    const user = userEvent.setup();
    render(<Settings />);

    await user.type(
      screen.getByLabelText("Server address"),
      "https://app.koofr.net",
    );
    await user.type(screen.getByLabelText("Username"), "someone@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2");
    await user.clear(screen.getByLabelText("Folder"));
    await user.type(screen.getByLabelText("Folder"), "/dav/Koofr/Music");

    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(backend.state.settings.remote.url).toBe("https://app.koofr.net");
    });
    expect(backend.state.settings.remote.username).toBe("someone@example.com");
    expect(backend.state.settings.remote.folder).toBe("/dav/Koofr/Music");
    expect(backend.state.hasPassword).toBe(true);
  });

  /**
   * The reported symptom: "No password saved for this account" while the
   * password was plainly visible in the box above it.
   *
   * Scanning reads the credential from the keychain, so a password typed but
   * not saved is one the scan cannot see. Pressing Scan after filling the form
   * means "use what I just typed".
   */
  it("scanning applies what is in the form first", async () => {
    const backend = useBackend({ connected: false });
    const user = userEvent.setup();
    render(<Settings />);

    await user.type(
      screen.getByLabelText("Server address"),
      "https://app.koofr.net",
    );
    await user.type(screen.getByLabelText("Username"), "someone@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2");
    await user.clear(screen.getByLabelText("Folder"));
    await user.type(screen.getByLabelText("Folder"), "/dav/Koofr/Music");

    // Straight to Scan, with nothing saved.
    await user.click(screen.getByRole("button", { name: /scan library/i }));

    await waitFor(() => {
      expect(backend.called("scan_library")).toBe(true);
    });
    expect(backend.state.hasPassword).toBe(true);
    expect(await screen.findByText(/found 4 tracks/i)).toBeInTheDocument();
  });

  /**
   * The answer belongs beside the button that asked the question.
   *
   * It used to render after every card on the page, so clicking Scan in the
   * first card put the result four cards below the fold and the button looked
   * like it had done nothing.
   */
  it("puts the scan result inside the card the button is in", async () => {
    useBackend({ connected: true });
    const user = userEvent.setup();
    render(<Settings />);

    await user.click(await screen.findByRole("button", { name: /scan library/i }));

    const found = await screen.findByText(/found 4 tracks/i);
    expect(remoteCard()).toContainElement(found);
  });

  /**
   * TD-49. A folder that is a name rather than a path is the real mistake:
   * Koofr mounts under /dav/Koofr, so "Music" alone matches nothing.
   *
   * This used to report "No tracks found", because the backend swallowed the
   * 404 on the base directory the same way it skips an unreadable
   * subdirectory, and returned an empty success. An empty library says exactly
   * the same thing, so there was nothing to tell a person their path was
   * wrong — which is the state Dylan was actually in.
   */
  it("says a missing folder is missing, not that the library is empty", async () => {
    useBackend({ connected: true });
    const user = userEvent.setup();
    render(<Settings />);

    const folder = await screen.findByLabelText("Folder");
    await user.clear(folder);
    await user.type(folder, "Music");
    await user.click(screen.getByRole("button", { name: /scan library/i }));

    const alert = await screen.findByRole("alert");
    // The path that was tried, so the mistake is legible in the message.
    expect(alert.textContent).toMatch(/Music/);
    // And the field to go and change.
    expect(alert.textContent).toMatch(/folder/i);
    // Emphatically not the old answer.
    expect(alert.textContent).not.toMatch(/no tracks found/i);
  });

  it("says how many folders it could not read, rather than walking past them", async () => {
    // A scan that skipped half a library still reports "found 4 tracks", and
    // that is indistinguishable from a library with 4 tracks in it.
    useBackend({ connected: true, unreadableFolders: 2 });
    const user = userEvent.setup();
    render(<Settings />);

    await user.click(await screen.findByRole("button", { name: /scan library/i }));

    const note = await screen.findByText(/found 4 tracks/i);
    expect(note.textContent).toMatch(/2 folders could not be read/i);
  });

  it("stays quiet about unreadable folders when there were none", async () => {
    useBackend({ connected: true });
    const user = userEvent.setup();
    render(<Settings />);

    await user.click(await screen.findByRole("button", { name: /scan library/i }));

    const note = await screen.findByText(/found 4 tracks/i);
    expect(note.textContent).not.toMatch(/could not be read/i);
  });
});

describe("Settings — the password, and knowing whether one is stored", () => {
  /**
   * The screen could not ask whether a password existed, so it always showed
   * "unchanged" — asserting a credential whether or not one was there.
   */
  it("says no password is saved when none is", async () => {
    useBackend({ connected: true, withPassword: false });
    render(<Settings />);

    expect(await screen.findByText(/no password saved yet/i)).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toHaveAttribute(
      "placeholder",
      "required",
    );
  });

  it("says the password is kept when one is stored", async () => {
    useBackend({ connected: true, withPassword: true });
    render(<Settings />);

    await waitFor(() => {
      expect(screen.getByLabelText("Password")).toHaveAttribute(
        "placeholder",
        "unchanged",
      );
    });
    expect(screen.getByText(/keychain/i)).toBeInTheDocument();
    expect(screen.queryByText(/no password saved yet/i)).not.toBeInTheDocument();
  });

  it("an empty password box keeps the stored one rather than clearing it", async () => {
    const backend = useBackend({ connected: true, withPassword: true });
    const user = userEvent.setup();
    render(<Settings />);

    // Change something else entirely and save.
    const folder = await screen.findByLabelText("Folder");
    await user.clear(folder);
    await user.type(folder, "/dav/Koofr/Other");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(backend.state.settings.remote.folder).toBe("/dav/Koofr/Other");
    });
    expect(backend.state.hasPassword).toBe(true);
    expect(backend.called("save_webdav_password")).toBe(false);
  });

  /**
   * The defect that logged the account out: correcting a username used to
   * delete the keychain entry, while an empty password box means "unchanged".
   * Together that destroyed the credential.
   */
  it("correcting the username does not lose the password", async () => {
    const backend = useBackend({ connected: true, withPassword: true });
    const user = userEvent.setup();
    render(<Settings />);

    const username = await screen.findByLabelText("Username");
    await user.clear(username);
    await user.type(username, "corrected@example.com");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(backend.state.settings.remote.username).toBe("corrected@example.com");
    });
    expect(backend.state.hasPassword).toBe(true);
  });
});

describe("Settings — unhappy paths", () => {
  it("surfaces a refused server address instead of storing it", async () => {
    const backend = useBackend({ connected: false });
    const user = userEvent.setup();
    render(<Settings />);

    // A Koofr app password pasted into the address field — the way it happens.
    await user.type(screen.getByLabelText("Server address"), "4wg9ie7xi8v7nbi6");
    await user.type(screen.getByLabelText("Username"), "someone@example.com");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/not a server address/i);
    expect(backend.state.settings.remote.url).toBe("");
  });

  it("reports a scan that fails rather than sitting on a spinner", async () => {
    const backend = useBackend({ connected: true });
    backend.fail("scan_library", "Could not reach the server: timed out");
    const user = userEvent.setup();
    render(<Settings />);

    await user.click(await screen.findByRole("button", { name: /scan library/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/timed out/i);
    // The button must come back, or the screen is stuck.
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /scan library/i })).toBeEnabled();
    });
  });

  it("reports a failure to read settings at all", async () => {
    const backend = useBackend();
    backend.fail("settings", "the core is unreachable");
    render(<Settings />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/unreachable/i);
  });

  it("dismissing an error removes it", async () => {
    const backend = useBackend({ connected: true });
    backend.fail("scan_library", "nope");
    const user = userEvent.setup();
    render(<Settings />);

    await user.click(await screen.findByRole("button", { name: /scan library/i }));
    const alert = await screen.findByRole("alert");

    await user.click(within(alert).getByRole("button", { name: /dismiss|×|close/i }));
    await waitFor(() => {
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
  });

  it("refuses to save or scan before there is anything to connect to", async () => {
    useBackend({ connected: false });
    render(<Settings />);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /^save$/i })).toBeDisabled();
    });
    expect(screen.getByRole("button", { name: /scan library/i })).toBeDisabled();
  });
});

describe("Settings — the field hints", () => {
  it("explains every field, revealed on demand", async () => {
    useBackend({ connected: true });
    const user = userEvent.setup();
    render(<Settings />);

    const toggles = await screen.findAllByRole("button", {
      name: /^what goes in /i,
    });
    expect(toggles).toHaveLength(4);

    // Collapsed by default: four open hints bury a four-field form.
    expect(screen.queryByText(/email address you sign in with/i)).not.toBeInTheDocument();

    const username = screen.getByRole("button", { name: /what goes in username/i });
    await user.click(username);

    expect(await screen.findByText(/email address you sign in with/i)).toBeInTheDocument();
    expect(username).toHaveAttribute("aria-expanded", "true");
  });

  it("the hints are reachable from the keyboard", async () => {
    useBackend({ connected: true });
    const user = userEvent.setup();
    render(<Settings />);

    const toggle = await screen.findByRole("button", {
      name: /what goes in password/i,
    });
    toggle.focus();
    await user.keyboard("{Enter}");

    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });
});

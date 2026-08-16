/**
 * Syncing with another device.
 *
 * The interesting cases here are the refusals and the states that are not
 * failures — a paired device that is switched off, a network with nothing on
 * it — because those are what a person actually sees most of the time, and a
 * panel that renders them as blanks reads as broken.
 *
 * The parts that decide anything are tested in Rust, where they live:
 * `vapor_library::sync` for pairing and reconciliation, `peers.rs` for what an
 * unpaired device is allowed to ask for.
 */
import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SyncPanel } from "./SyncPanel";
import type * as core from "../lib/core";
import { useBackend } from "../test/setup";

function peer(over: Partial<core.SyncPeer> = {}): core.SyncPeer {
  return {
    id: "device-b",
    name: "Dylan's Phone",
    kind: "phone",
    address: "192.168.1.55:7677",
    lastSeen: 1,
    ...over,
  };
}

function trusted(over: Partial<core.TrustedDevice> = {}): core.TrustedDevice {
  return {
    id: "device-b",
    name: "Dylan's Phone",
    kind: "phone",
    pairedAt: 1,
    ...over,
  };
}

describe("Sync panel", () => {
  it("names this device, so the other end can be told what to look for", async () => {
    useBackend({ syncEnabled: true });
    render(<SyncPanel />);

    expect(await screen.findByText(/this device is/i)).toBeInTheDocument();
  });

  /** A lone machine is the normal case, and it needs an explanation. */
  it("explains an empty network rather than showing a blank", async () => {
    useBackend({ syncEnabled: true, peers: [] });
    render(<SyncPanel />);

    expect(
      await screen.findByText(/nothing else running vapor here/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/none yet/i)).toBeInTheDocument();
  });

  it("lists a device it can see, and offers both ends of the exchange", async () => {
    useBackend({ syncEnabled: true, peers: [peer()] });
    render(<SyncPanel />);

    expect(await screen.findByText("Dylan's Phone")).toBeInTheDocument();
    // Two, because this device cannot know which end of the pairing it is.
    expect(screen.getByRole("button", { name: /show a code/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /enter a code/i })).toBeInTheDocument();
  });

  it("shows a code for the other device to type", async () => {
    useBackend({ syncEnabled: true, peers: [peer()] });
    const user = userEvent.setup();
    render(<SyncPanel />);

    await user.click(await screen.findByRole("button", { name: /show a code/i }));

    expect(await screen.findByText("482913")).toBeInTheDocument();
    expect(screen.getByText(/show this on the other device/i)).toBeInTheDocument();
  });

  it("pairs when the right code is typed", async () => {
    const backend = useBackend({ syncEnabled: true, peers: [peer()] });
    const user = userEvent.setup();
    render(<SyncPanel />);

    await user.click(await screen.findByRole("button", { name: /enter a code/i }));
    await user.type(screen.getByLabelText(/code shown on/i), "482913{Enter}");

    await waitFor(() => expect(backend.called("pair_with")).toBe(true));
    expect(await screen.findByText(/paired with dylan's phone/i)).toBeInTheDocument();
  });

  /** A wrong code has to say so, and say how many tries are left. */
  it("reports a refused code instead of failing quietly", async () => {
    useBackend({ syncEnabled: true, peers: [peer()] });
    const user = userEvent.setup();
    render(<SyncPanel />);

    await user.click(await screen.findByRole("button", { name: /enter a code/i }));
    await user.type(screen.getByLabelText(/code shown on/i), "000000{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent(/wrong code/i);
  });

  /**
   * A paired device that is switched off is the ordinary case, not an error —
   * but "Sync now" against something unreachable is a button that can only
   * fail.
   */
  it("says a paired device is away, and does not offer to sync with it", async () => {
    useBackend({ syncEnabled: true, peers: [], trustedPeers: [trusted()] });
    render(<SyncPanel />);

    expect(await screen.findByText(/not on this network/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /sync now/i })).toBeDisabled();
  });

  it("syncs with a paired device that is here, and reports what moved", async () => {
    const backend = useBackend({ syncEnabled: true, peers: [peer()], trustedPeers: [trusted()] });
    const user = userEvent.setup();
    render(<SyncPanel />);

    await user.click(await screen.findByRole("button", { name: /sync now/i }));

    await waitFor(() => expect(backend.called("sync_with")).toBe(true));
    expect(await screen.findByText(/2 of 2 moved from dylan's phone/i))
      .toBeInTheDocument();
  });

  /** The filters are the point of having them: they reach the backend. */
  it("passes what to move through to the sync", async () => {
    const backend = useBackend({ syncEnabled: true, peers: [peer()], trustedPeers: [trusted()] });
    const user = userEvent.setup();
    render(<SyncPanel />);

    await user.click(await screen.findByRole("checkbox", { name: /track files/i }));
    await user.click(screen.getByRole("button", { name: /sync now/i }));

    await waitFor(() => expect(backend.called("sync_with")).toBe(true));
    expect(backend.lastArgs("sync_with")?.what).toEqual({
      tracks: false,
      playlists: true,
    });
  });

  /** Unpairing is not undoable, and the prompt says what it costs. */
  it("asks before forgetting a device", async () => {
    const confirm = vi.spyOn(window, "confirm").mockImplementation(() => true);
    const backend = useBackend({ syncEnabled: true, peers: [peer()], trustedPeers: [trusted()] });
    const user = userEvent.setup();
    render(<SyncPanel />);

    await user.click(await screen.findByRole("button", { name: /forget/i }));

    await waitFor(() => expect(backend.called("forget_peer")).toBe(true));
    expect(confirm.mock.calls[0]?.[0]).toMatch(/paired again/i);
    confirm.mockRestore();
  });

  it("does not forget a device when the prompt is declined", async () => {
    const confirm = vi.spyOn(window, "confirm").mockImplementation(() => false);
    const backend = useBackend({ syncEnabled: true, peers: [peer()], trustedPeers: [trusted()] });
    const user = userEvent.setup();
    render(<SyncPanel />);

    await user.click(await screen.findByRole("button", { name: /forget/i }));

    await new Promise((r) => setTimeout(r, 50));
    expect(backend.called("forget_peer")).toBe(false);
    confirm.mockRestore();
  });

  /**
   * SYNC-006 — the other way to sync, and the one that works when the two
   * devices are never switched on at the same time.
   */
  describe("through the server", () => {
    it("writes the first document when there is none", async () => {
      const backend = useBackend({ syncEnabled: true });
      const user = userEvent.setup();
      render(<SyncPanel />);

      await user.click(
        await screen.findByRole("button", { name: /sync through the server/i }),
      );

      await waitFor(() =>
        expect(backend.called("sync_shared_document")).toBe(true),
      );
      expect(await screen.findByText(/written for the first time/i))
        .toBeInTheDocument();
    });

    it("takes a playlist the server has and this device does not", async () => {
      const backend = useBackend({ syncEnabled: true,
        playlists: [],
        sharedDocument: [
          {
            id: "p9",
            name: "From the laptop",
            customCoverPath: "",
            tracks: ["/dav/Koofr/Music/xtal.m4a"],
            folderId: "",
          },
        ],
      });
      const user = userEvent.setup();
      render(<SyncPanel />);

      await user.click(
        await screen.findByRole("button", { name: /sync through the server/i }),
      );

      await waitFor(() => expect(backend.state.playlists).toHaveLength(1));
      expect(await screen.findByText(/1 playlist arrived/i)).toBeInTheDocument();
    });

    /**
     * Nothing-to-do is a real outcome and has to read as one. Reporting only
     * the changes makes it indistinguishable from a failure that did not
     * throw.
     */
    it("says so when both ends already agree", async () => {
      useBackend({ syncEnabled: true, playlists: [], sharedDocument: [] });
      const user = userEvent.setup();
      render(<SyncPanel />);

      await user.click(
        await screen.findByRole("button", { name: /sync through the server/i }),
      );

      expect(await screen.findByText(/already in step/i)).toBeInTheDocument();
    });

    it("reports having nowhere to keep it when no server is set up", async () => {
      useBackend({ syncEnabled: true, connected: false });
      const user = userEvent.setup();
      render(<SyncPanel />);

      await user.click(
        await screen.findByRole("button", { name: /sync through the server/i }),
      );

      expect(await screen.findByRole("alert")).toHaveTextContent(
        /nowhere to keep it/i,
      );
    });
  });

  /**
   * Off is the shipped default, and the same decision as lookups: a beacon
   * every five seconds announces this machine to whatever network it is
   * joined to.
   */
  describe("the switch", () => {
    it("announces nothing and lists nothing until it is turned on", async () => {
      useBackend({ peers: [peer()] });
      render(<SyncPanel />);

      expect(await screen.findByRole("checkbox", { name: /find my other devices/i }))
        .not.toBeChecked();
      // Not merely hidden — the backend does not report a peer either.
      expect(screen.queryByText("Dylan's Phone")).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /show a code/i }))
        .not.toBeInTheDocument();
    });

    it("starts finding devices when it is turned on", async () => {
      const backend = useBackend({ peers: [peer()] });
      const user = userEvent.setup();
      render(<SyncPanel />);

      await user.click(
        await screen.findByRole("checkbox", { name: /find my other devices/i }),
      );

      await waitFor(() => expect(backend.called("set_sync_enabled")).toBe(true));
      expect(await screen.findByText("Dylan's Phone")).toBeInTheDocument();
    });

    /** A device that can no longer be discovered should not still be trusted. */
    it("forgets paired devices when it is turned off", async () => {
      const backend = useBackend({
        syncEnabled: true,
        peers: [peer()],
        trustedPeers: [trusted()],
      });
      const user = userEvent.setup();
      render(<SyncPanel />);

      await user.click(
        await screen.findByRole("checkbox", { name: /find my other devices/i }),
      );

      await waitFor(() => expect(backend.called("set_sync_enabled")).toBe(true));
      expect(await screen.findByText(/paired devices have been forgotten/i))
        .toBeInTheDocument();
    });
  });
});

/**
 * The fake, tested against the real command list.
 *
 * A fake that has drifted from the thing it fakes turns every test written
 * against it into a test of the drift. `command_bindings.rs` already proves
 * every backend command has a binding in `core.ts`; this proves the fake
 * answers every command `core.ts` can send.
 *
 * The list is read out of `core.ts` rather than typed here, so a new command
 * fails this the moment it is added rather than the first time a screen calls
 * it in a test and gets a confusing render failure.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { FakeBackend } from "./ipc";

/** Every command string `core.ts` passes to `invoke`. */
function commandsInCore(): string[] {
  const source = readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), "../lib/core.ts"),
    "utf8",
  );
  const found = new Set<string>();
  for (const m of source.matchAll(/invoke<[^>]*>\(\s*"([a-z_]+)"/g)) {
    if (m[1]) found.add(m[1]);
  }
  return [...found].sort();
}

describe("the IPC fake", () => {
  it("answers every command the frontend can send", async () => {
    const commands = commandsInCore();
    expect(commands.length).toBeGreaterThan(30);

    const missing: string[] = [];
    for (const cmd of commands) {
      // A fresh backend per command: some commands change state in ways that
      // would make a later one throw for an unrelated reason.
      const backend = new FakeBackend({ connected: true });
      try {
        // Arguments the commands that need one will accept. A command that
        // rejects *these* is answering, which is all this checks.
        await backend.invoke(cmd, {
          id: "p1",
          href: "/dav/Koofr/Music/windowlicker.m4a",
          hrefs: [],
          username: "someone@example.com",
          password: "x",
          name: "x",
          url: "https://example.com",
          folder: "/dav/Koofr/Music",
          index: 0,
          from: 0,
          to: 0,
          bpm: 120,
          bytes: 1,
          mode: "off",
          shuffled: false,
          start: null,
          curve: "wave",
          query: "",
          view: {},
        });
      } catch (e) {
        // Only "no case for" means the fake is missing the command; a domain
        // rejection is a real answer.
        if (String(e).includes("has no case for")) missing.push(cmd);
      }
    }

    expect(missing).toEqual([]);
  });

  it("holds real state rather than returning canned values", async () => {
    const backend = new FakeBackend({ connected: true });

    const made = (await backend.invoke("create_playlist", { name: "Test" })) as {
      id: string;
    };
    await backend.invoke("add_tracks_to_playlist", {
      id: made.id,
      hrefs: ["/dav/Koofr/Music/xtal.m4a"],
    });

    const playlists = (await backend.invoke("playlists")) as { tracks: string[] }[];
    expect(playlists[0]?.tracks).toEqual(["/dav/Koofr/Music/xtal.m4a"]);
  });

  it("refuses to answer a command it does not know", async () => {
    const backend = new FakeBackend();
    // Loud rather than undefined: a silent undefined becomes a render failure
    // three layers away from the cause.
    await expect(backend.invoke("no_such_command")).rejects.toThrow(
      /has no case for/,
    );
  });

  it("can be made to fail any command, for the unhappy paths", async () => {
    const backend = new FakeBackend();
    backend.fail("scan_library", "the server is on fire");
    await expect(backend.invoke("scan_library")).rejects.toThrow(/on fire/);

    backend.clearFailure("scan_library");
    await expect(backend.invoke("scan_library")).resolves.toBeDefined();
  });
});

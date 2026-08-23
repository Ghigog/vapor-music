/**
 * Onboarding — the first thing a person sees.
 *
 * Settings made the app usable; this makes the first thirty seconds of it make
 * sense. The copy is the design's, not invented: the promises in particular are
 * the product's actual argument, and paraphrasing them would weaken a claim the
 * rest of the code goes to some trouble to keep true.
 *
 * ## The second path, removed and then restored
 *
 * This file used to say the design's second path — "Use files on this phone" —
 * was "deliberately absent", because Vapor is cloud-first and a local folder
 * would be "a second source of truth, a second scanner and a second sync story,
 * in service of a promise the product does not make".
 *
 * Reversed on 2026-08-22, and the reasoning kept rather than deleted, because it
 * was a real decision and deserves a real answer.
 *
 * It could not stand against the rest of the product. `README.md` calls the app
 * local-first and its first design principle is that every feature works with no
 * internet — while as shipped nobody could play a note without standing up a
 * WebDAV server. The two documents could not both be right.
 *
 * Its three objections, answered:
 *
 * * *A second source of truth.* Sources are additive and a track belongs to
 *   exactly one of them; its href carries which. See `local.rs`.
 * * *A second scanner.* A directory walk against the same extension filter the
 *   server scan uses. Tens of lines, not a subsystem.
 * * *A second sync story.* A shorter one: a local library does no WebDAV sync,
 *   and LAN sync between a person's own devices is untouched.
 */

import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { VaporMark } from "../components/VaporMark";
import * as core from "../lib/core";

/** From the design. Each of these is a claim the code has to keep. */
const PROMISES = [
  "Nothing leaves this device unless you move it yourself.",
  "Tempo, key and energy are worked out here — not in a datacentre.",
  "Your metadata is plain JSON you can read without us.",
];

export function Onboarding({
  onConnect,
  onLibraryReady,
}: {
  onConnect: () => void;
  /** A folder was added and scanned. There is a library now. */
  onLibraryReady: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState("");

  async function chooseFolder() {
    setProblem("");
    const picked = await open({ directory: true, multiple: false });
    // Cancelling the picker is not an error and must not read as one.
    if (typeof picked !== "string") return;

    setBusy(true);
    try {
      await core.addLocalFolder(picked);
      // Scanned before the app claims to be ready. The old flow flipped to
      // "ready" on the button press, so a mistyped server left somebody in a
      // working app with an empty library and no way back to this screen.
      await core.scanLibrary();
      onLibraryReady();
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="onboard">
      <div className="onboard__mark">
        {/* Idle, breathing. The mark only becomes a readout once there is
            something to read — see Now Playing. */}
        <VaporMark size={132} state="idle" />
      </div>

      <h1 className="onboard__title">
        Bring your
        <br />
        library home.
      </h1>

      <p className="onboard__body">
        Vapor plays the files you already own, from a drive you already control
        — and works out the mixing right here on this device.
      </p>

      <ul className="onboard__promises">
        {PROMISES.map((promise) => (
          <li key={promise} className="onboard__promise">
            <span className="onboard__tick sovereign" aria-hidden="true">
              ✓
            </span>
            <span>{promise}</span>
          </li>
        ))}
      </ul>

      <div className="onboard__actions">
        {/*
          Music already on this device comes first, because it is the path that
          needs nothing: no server, no address, no password. The design had it
          second and the code had it not at all.
        */}
        <button
          className="onboard__button"
          onClick={chooseFolder}
          disabled={busy}
        >
          {busy ? "Reading your music…" : "Choose a folder on this device"}
        </button>
        <button
          className="onboard__button onboard__button--quiet"
          onClick={onConnect}
          disabled={busy}
        >
          Connect a server instead
        </button>
      </div>

      {problem && (
        <p className="onboard__problem" role="alert">
          {problem}
        </p>
      )}

      <p className="onboard__footnote">
        No account. Nothing to sign up for. You can add the other later.
      </p>
    </div>
  );
}

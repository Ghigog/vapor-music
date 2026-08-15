/**
 * Onboarding — the first thing a person sees.
 *
 * Settings made the app usable; this makes the first thirty seconds of it make
 * sense. The copy is the design's, not invented: the promises in particular are
 * the product's actual argument, and paraphrasing them would weaken a claim the
 * rest of the code goes to some trouble to keep true.
 *
 * It is one screen with two paths because the design has two, and only one of
 * them exists yet — see the note on the second button.
 */

import { VaporMark } from "../components/VaporMark";

/** From the design. Each of these is a claim the code has to keep. */
const PROMISES = [
  "Nothing leaves this device unless you move it yourself.",
  "Tempo, key and energy are worked out here — not in a datacentre.",
  "Your metadata is plain JSON you can read without us.",
];

export function Onboarding({ onConnect }: { onConnect: () => void }) {
  return (
    <div className="onboard">
      <div className="onboard__mark">
        {/* Idle, breathing. The mark only becomes a readout once there is
            something to read — see Now Playing. */}
        <VaporMark size={132} theme="light" state="idle" />
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
        <button className="onboard__button" onClick={onConnect}>
          Choose where music lives
        </button>
        {/*
          The design's second path — "Use files on this phone" — is not built.
          A local-folder library needs a file picker, a watcher and a scanner
          that walks a filesystem rather than WebDAV, none of which exist. It is
          shown disabled rather than hidden so the choice the product intends to
          offer is visible, and honest about not being ready.
        */}
        <button className="onboard__button onboard__button--quiet" disabled>
          Use files on this device
          <span className="onboard__soon">not yet</span>
        </button>
      </div>

      <p className="onboard__footnote">No account. Nothing to sign up for.</p>
    </div>
  );
}

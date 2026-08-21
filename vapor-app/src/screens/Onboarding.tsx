/**
 * Onboarding — the first thing a person sees.
 *
 * Settings made the app usable; this makes the first thirty seconds of it make
 * sense. The copy is the design's, not invented: the promises in particular are
 * the product's actual argument, and paraphrasing them would weaken a claim the
 * rest of the code goes to some trouble to keep true.
 *
 * The design offers a second path — "Use files on this phone" — and it is
 * deliberately absent. Vapor is cloud-first: the library lives on the user's own
 * server and local storage is only a bounded cache. A local-folder library would
 * be a second source of truth, a second scanner and a second sync story, in
 * service of a promise the product does not make.
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
        <button className="onboard__button" onClick={onConnect}>
          Choose where music lives
        </button>
      </div>

      <p className="onboard__footnote">No account. Nothing to sign up for.</p>
    </div>
  );
}

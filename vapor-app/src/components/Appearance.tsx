/**
 * The appearance control.
 *
 * Three choices, not a switch. A two-state toggle can say "dark: on/off" but
 * it cannot say "whichever the machine is", and that third answer is the one
 * the app shipped with — the placeholder dark mode was nothing but
 * `prefers-color-scheme`. Turning that into "off" for everybody who never
 * opened Settings would be a change of behaviour disguised as a new feature,
 * so `auto` stays, and stays the default.
 *
 * The press applies the theme immediately and persists it afterwards. If the
 * write fails the choice is put back: a screen that says Lamplight and reopens
 * in Daylight tomorrow is worse than one that admits it could not save.
 */

import { useState } from "react";
import * as core from "../lib/core";
import { ErrorNotice, messageOf } from "./ErrorNotice";
import {
  APPEARANCES,
  APPEARANCE_LABELS,
  adoptAppearance,
  useAppearance,
  type Appearance,
} from "../lib/theme";

export function AppearanceControl() {
  const chosen = useAppearance();
  const [error, setError] = useState<string | null>(null);

  async function choose(next: Appearance) {
    if (next === chosen) return;
    const previous = chosen;
    setError(null);
    // Applied before the round trip, not after: this is the one setting whose
    // effect is the screen you are looking at, so a delay reads as a dead
    // button rather than as saving.
    adoptAppearance(next);
    try {
      await core.setAppearance(next);
    } catch (e: unknown) {
      adoptAppearance(previous);
      setError(messageOf(e));
    }
  }

  return (
    <div className="appearance">
      <div
        className="appearance__segments"
        role="radiogroup"
        aria-label="Appearance"
      >
        {APPEARANCES.map((option) => (
          <button
            key={option}
            type="button"
            role="radio"
            aria-checked={option === chosen}
            className={
              option === chosen
                ? "appearance__segment appearance__segment--on"
                : "appearance__segment"
            }
            onClick={() => void choose(option)}
          >
            {option !== "auto" && (
              <span
                className={`appearance__swatch appearance__swatch--${option}`}
                aria-hidden="true"
              />
            )}
            {APPEARANCE_LABELS[option]}
          </button>
        ))}
      </div>
      {error && <ErrorNotice error={error} onDismiss={() => setError(null)} />}
    </div>
  );
}

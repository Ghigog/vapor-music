/**
 * React wrapper for the <vapor-mark> custom element.
 *
 * The element is registered globally by public/vapor-mark.js. This exists only
 * to give it a typed surface and to keep the string-attribute contract in one
 * place — the mark's *shape* is still being iterated on, but these attributes
 * are stable, so screens should code against this rather than the element.
 *
 * The states are not decoration. They map onto real engine state: `thinking`
 * while the pathfinder chooses, `blending` during a transition, and `energy`
 * from the playing deck's level. Wired that way the logo is a readout.
 */
import { useEffect, useRef } from "react";

export type MarkState = "idle" | "playing" | "blending" | "thinking";

export interface VaporMarkProps {
  size?: number;
  theme?: "light" | "dark";
  state?: MarkState;
  /** 0–1 amplitude drive, used while playing. */
  energy?: number;
  speed?: number;
  /** Render a single frame and stop — for print and export. */
  still?: boolean;
}

export function VaporMark({
  size = 160,
  theme = "light",
  state = "idle",
  energy,
  speed,
  still = false,
}: VaporMarkProps) {
  const ref = useRef<HTMLElement>(null);

  // Attributes are set imperatively rather than via JSX props: the element
  // observes attributes, and React would otherwise serialise unknown props
  // inconsistently across versions.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.setAttribute("size", String(size));
    el.setAttribute("theme", theme);
    el.setAttribute("state", state);
    if (energy !== undefined) el.setAttribute("energy", String(energy));
    if (speed !== undefined) el.setAttribute("speed", String(speed));
    if (still) el.setAttribute("static", "");
    else el.removeAttribute("static");
  }, [size, theme, state, energy, speed, still]);

  return (
    <vapor-mark
      ref={ref}
      style={{ width: size, height: size, display: "block" }}
    />
  );
}

// React 19 moved the JSX namespace out of the global scope and into the react
// module, so a `declare global` block no longer registers a custom element.
// The prop types are referenced unqualified because inside this augmentation
// they already resolve to react's own declarations.
declare module "react" {
  namespace JSX {
    interface IntrinsicElements {
      "vapor-mark": DetailedHTMLProps<HTMLAttributes<HTMLElement>, HTMLElement>;
    }
  }
}

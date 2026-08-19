/**
 * React wrapper for the <vapor-ribbon> custom element.
 *
 * The element is registered globally by public/vapor-ribbon.js. This exists
 * only to give it a typed surface and to keep the string-attribute contract in
 * one place — the mark's *shape* is still being iterated on, but these
 * attributes are stable, so screens should code against this rather than the
 * element.
 *
 * WHY THIS IS A RIBBON AND NOT A TUBE
 *
 * The old mark (public/vapor-mark.js) modelled a tube: circular cross-section,
 * normal taken from the gradient of a height field. Two artefacts fall out of
 * that model and neither is fixable inside it — the highlight is a pill running
 * down the spine, so the eye reads a sausage; and the normal differences a
 * height field built on a piecewise-linear centreline, so every node kink
 * survives into the specular term as a faint transverse line. Together they are
 * the "worm made of stacked cylinders" read.
 *
 * The ribbon is a flat strip with a twist along its length. Its screen width is
 * the strip turning — broad on the face, a filament edge-on — and its normal
 * comes from the twist angle directly rather than from any height field, so the
 * node chain cannot print into the shading.
 *
 * The states are not decoration. They map onto real engine state: `thinking`
 * while the pathfinder chooses, `blending` during a transition, and `energy`
 * from the playing deck's level. Wired that way the logo is a readout.
 */
import { useEffect, useRef } from "react";
import markDef from "../lib/mark.json";

export type MarkState = "idle" | "playing" | "blending" | "thinking";

/**
 * The chosen geometry, in one place.
 *
 * Read from src/lib/mark.json rather than written here, because the icon
 * exporter reads the same file. Icons are rendered by the same code path as the
 * running logo, so the only way they can disagree is if the numbers disagree —
 * and there is only one copy of the numbers.
 *
 * These are the values the mark study renders at ("Ribbon Mark Lab"), not the
 * element's own fallbacks: the element ships slightly different defaults
 * (turns 1.5, curl 0.85) and would otherwise quietly draw a different logo than
 * the one that was signed off.
 */
export const MARK = {
  ...markDef.geometry,
  ...markDef.material,
} as const;

/**
 * The frame the fixed logo is pinned to — see mark.json.
 *
 * Exported so a screen that wants the *logo* rather than the *readout* can ask
 * for it by name and get exactly what the app icon shows.
 */
export const LOGO_POSE: number = markDef.pose;

export interface VaporMarkProps {
  size?: number;
  theme?: "light" | "dark";
  state?: MarkState;
  /** 0–1 amplitude drive, used while playing. */
  energy?: number;
  speed?: number;
  /** Render a single frame and stop — for print and export. */
  still?: boolean;
  /**
   * Freeze on the frame the idle animation reaches N seconds in.
   *
   * This is how a fixed logo gets locked: the renderer reproduces a given pose
   * exactly and forever, so a marketing asset and the running app can be the
   * same drawing. `pose` implies `still`. No pose is chosen yet — the contact
   * sheet in the mark study is the menu.
   */
  pose?: number;
  /** Geometry and material overrides. Defaults come from {@link MARK}. */
  turns?: number;
  ribbon?: number;
  curl?: number;
  dispersion?: number;
  hue?: number;
}

export function VaporMark({
  size = 160,
  theme = "light",
  state = "idle",
  energy,
  speed,
  still = false,
  pose,
  turns = MARK.turns,
  ribbon = MARK.ribbon,
  curl = MARK.curl,
  dispersion = MARK.dispersion,
  hue = MARK.hue,
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
    el.setAttribute("turns", String(turns));
    el.setAttribute("ribbon", String(ribbon));
    el.setAttribute("curl", String(curl));
    el.setAttribute("dispersion", String(dispersion));
    el.setAttribute("hue", String(hue));
    if (energy !== undefined) el.setAttribute("energy", String(energy));
    if (speed !== undefined) el.setAttribute("speed", String(speed));
    if (pose !== undefined) el.setAttribute("pose", String(pose));
    else el.removeAttribute("pose");
    if (still) el.setAttribute("static", "");
    else el.removeAttribute("static");
  }, [
    size,
    theme,
    state,
    energy,
    speed,
    still,
    pose,
    turns,
    ribbon,
    curl,
    dispersion,
    hue,
  ]);

  return (
    <vapor-ribbon
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
      "vapor-ribbon": DetailedHTMLProps<HTMLAttributes<HTMLElement>, HTMLElement>;
    }
  }
}

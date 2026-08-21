/**
 * The theme layer.
 *
 * What is actually worth testing here is the *absence* of an attribute: `auto`
 * has to leave the root alone so the `prefers-color-scheme` query in
 * `tokens.css` keeps binding, which is the behaviour that shipped before there
 * was a control. A version of this that wrote `data-theme="light"` for auto
 * would pass any test that only looked at the colours on a light machine, and
 * would silently stop following the OS on every machine.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { setPrefersDark } from "../test/setup";
import {
  adoptAppearance,
  applyAppearance,
  asAppearance,
  appearance,
  resolveTheme,
} from "./theme";

afterEach(() => {
  vi.unstubAllGlobals();
  // The store is module state, so it survives into the next test unless put
  // back — and `auto` is what a fresh install has.
  adoptAppearance("auto");
});

describe("reading a stored choice", () => {
  it("takes the three words it knows", () => {
    expect(asAppearance("daylight")).toBe("daylight");
    expect(asAppearance("lamplight")).toBe("lamplight");
    expect(asAppearance("auto")).toBe("auto");
  });

  it("tolerates casing and whitespace, which is what a hand-edited file has", () => {
    expect(asAppearance("  Lamplight ")).toBe("lamplight");
    expect(asAppearance("AUTO")).toBe("auto");
  });

  /** `default_dark` is the Godot theme-resource name this field used to hold. */
  it("falls back to following the OS for anything else", () => {
    expect(asAppearance("default_dark")).toBe("auto");
    expect(asAppearance("")).toBe("auto");
    expect(asAppearance(null)).toBe("auto");
    expect(asAppearance(undefined)).toBe("auto");
  });
});

describe("resolving a choice against the machine", () => {
  it("ignores the OS once someone has chosen", () => {
    setPrefersDark(true);
    expect(resolveTheme("daylight")).toBe("daylight");
    setPrefersDark(false);
    expect(resolveTheme("lamplight")).toBe("lamplight");
  });

  it("asks the OS under auto", () => {
    setPrefersDark(true);
    expect(resolveTheme("auto")).toBe("lamplight");
    setPrefersDark(false);
    expect(resolveTheme("auto")).toBe("daylight");
  });
});

describe("applying a choice to the root", () => {
  it("names the theme when one was chosen", () => {
    applyAppearance("lamplight");
    expect(document.documentElement.getAttribute("data-theme")).toBe("lamplight");
    applyAppearance("daylight");
    expect(document.documentElement.getAttribute("data-theme")).toBe("daylight");
  });

  it("writes nothing under auto, so the media query still binds", () => {
    applyAppearance("lamplight");
    applyAppearance("auto");
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });
});

describe("remembering the choice", () => {
  it("mirrors it locally, so the next launch does not wait for the backend", () => {
    adoptAppearance("lamplight");
    expect(localStorage.getItem("vapor.appearance")).toBe("lamplight");
    expect(appearance()).toBe("lamplight");
  });

  /** Private-mode storage throws on access rather than returning null. */
  it("still applies the theme when storage refuses", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    });
    expect(() => adoptAppearance("lamplight")).not.toThrow();
    expect(document.documentElement.getAttribute("data-theme")).toBe("lamplight");
  });
});

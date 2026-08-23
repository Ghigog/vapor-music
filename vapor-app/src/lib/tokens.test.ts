/**
 * The token layer, checked as data.
 *
 * Two rules that a person cannot hold in their head while editing CSS, and
 * that broke the first dark mode between them:
 *
 * 1. **Nothing outside `tokens.css` names a colour.** The placeholder dark
 *    theme swapped a dozen tokens while some sixty rules still said
 *    `rgba(255,255,255,0.6)` outright, so on a dark machine the text went dark
 *    and the surfaces stayed light. Every one of those rules typechecked,
 *    rendered, and passed its screen's tests.
 * 2. **The two dark blocks agree.** A custom property cannot be aliased to a
 *    whole block and `@import` cannot take a media query, so the Lamplight
 *    values are written twice — once for `[data-theme]`, once for
 *    `prefers-color-scheme`. Two copies drift; this is the cheaper half of the
 *    trade that keeps them.
 */
import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

const root = join(import.meta.dirname, "..", "..");
const tokens = readFileSync(join(root, "public", "tokens.css"), "utf8");

/** Every `--name: value;` declaration inside one block. */
function declarations(block: string): Map<string, string> {
  const found = new Map<string, string>();
  for (const match of block.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) {
    const [, name, value] = match;
    if (name && value) found.set(name, value.replace(/\s+/g, " ").trim());
  }
  return found;
}

/** The body of the rule whose selector line contains `needle`. */
function block(needle: string): string {
  const at = tokens.indexOf(needle);
  expect(at, `no rule mentioning ${needle}`).toBeGreaterThan(-1);
  const open = tokens.indexOf("{", at);
  let depth = 0;
  for (let i = open; i < tokens.length; i += 1) {
    if (tokens[i] === "{") depth += 1;
    else if (tokens[i] === "}") {
      depth -= 1;
      if (depth === 0) return tokens.slice(open + 1, i);
    }
  }
  throw new Error(`unterminated rule at ${needle}`);
}

describe("the two ways of asking for Lamplight", () => {
  const chosen = declarations(block(':root[data-theme="dark"]'));
  const followed = declarations(
    block(':root:not([data-theme="light"]):not([data-theme="daylight"])'),
  );

  it("define the same tokens", () => {
    expect([...followed.keys()].sort()).toEqual([...chosen.keys()].sort());
  });

  it("give them the same values", () => {
    for (const [name, value] of chosen) {
      expect(followed.get(name), `${name} differs between the two blocks`).toBe(
        value,
      );
    }
  });

  it("override every token they need to and no token they do not", () => {
    const light = declarations(block(":root {"));
    for (const name of chosen.keys()) {
      expect(light.has(name), `${name} is dark-only, so Daylight has no value`)
        .toBe(true);
    }
  });
});

describe("colours live in tokens.css", () => {
  /** Every stylesheet the app ships except the token file itself. */
  function stylesheets(dir: string): string[] {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return stylesheets(path);
      return entry.isFile() && entry.name.endsWith(".css") ? [path] : [];
    });
  }

  it("and nowhere else", () => {
    // Hex, rgb()/rgba(), hsl()/hsla() and the CSS colour keywords that actually
    // turn up in hand-written CSS. `transparent` and `currentColor` are not
    // colours in this sense — they are references to something else.
    const literal =
      /#[0-9a-f]{3,8}\b|\brgba?\(\s*\d|\bhsla?\(\s*\d|(?<![-\w])(?:white|black|red|green|blue|grey|gray)(?![-\w])/i;
    const offenders: string[] = [];

    for (const sheet of stylesheets(join(root, "src"))) {
      const source = readFileSync(sheet, "utf8")
        // Comments explain past colours by name; they do not draw anything.
        .replace(/\/\*[\s\S]*?\*\//g, "");
      source.split("\n").forEach((line, i) => {
        if (literal.test(line)) {
          offenders.push(`${relative(root, sheet)}:${i + 1}  ${line.trim()}`);
        }
      });
    }

    expect(offenders, "put these in public/tokens.css").toEqual([]);
  });
});

/**
 * Contrast, computed rather than trusted.
 *
 * `docs/DESIGN_LANGUAGE.md` commits the app to WCAG 2.1 AA. Nothing checked
 * it, and the primary button spent its life at 4.02:1 — white on `#007aff`,
 * which is Apple's blue and looks entirely correct until it is measured. The
 * comment beside it recorded 5.6:1, a figure that belonged to a label colour
 * the theme no longer used.
 *
 * Only the pairs that are *decided* are asserted: a label on a fill, body ink
 * on the page, and — since AUD-6 — Daylight's quiet scale, `--ink-3` and
 * `--ink-4`, which were 2.24:1 and 2.76:1 and are now guarded on both the flat
 * page and the palest stop of its gradient. What is still undecided stays out:
 * `--ink-soft`, `--accent`-as-text and `--sov-quiet` are large-text-only by
 * measurement, and Lamplight's `--ink-4` is 4.39:1. A test that asserts a
 * failing value is how the failure becomes permanent — see
 * `docs/workspace/tickets.md`.
 */
describe("contrast", () => {
  const channel = (c: number) =>
    c / 255 <= 0.03928 ? c / 255 / 12.92 : ((c / 255 + 0.055) / 1.055) ** 2.4;

  function luminance(hex: string): number {
    const h = hex.trim().replace("#", "");
    const full = h.length === 3 ? [...h].map((c) => c + c).join("") : h;
    const [r, g, b] = [0, 2, 4].map((i) =>
      channel(parseInt(full.slice(i, i + 2), 16)),
    ) as [number, number, number];
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  }

  function ratio(a: string, b: string): number {
    const [x, y] = [luminance(a), luminance(b)];
    const [hi, lo] = x > y ? [x, y] : [y, x];
    return (hi + 0.05) / (lo + 0.05);
  }

  it("computes a known pair correctly", () => {
    // Guards the maths itself: black on white is exactly 21:1, and a formula
    // that is subtly wrong would otherwise make every assertion below
    // meaningless in whichever direction it errs.
    expect(ratio("#000000", "#ffffff")).toBeCloseTo(21, 5);
    expect(ratio("#ffffff", "#ffffff")).toBeCloseTo(1, 5);
  });

  const themes: Array<[string, string]> = [
    ["Daylight", ":root {"],
    ["Lamplight", ':root[data-theme="dark"]'],
  ];

  for (const [name, selector] of themes) {
    it(`${name}: the label on the primary fill passes AA`, () => {
      const t = declarations(block(selector));
      const label = t.get("--on-accent");
      const fill = t.get("--accent-fill");

      expect(label, `${name} has no --on-accent`).toBeTruthy();
      expect(fill, `${name} has no --accent-fill`).toBeTruthy();

      const measured = ratio(label as string, fill as string);
      expect(
        measured,
        `${label} on ${fill} is ${measured.toFixed(2)}:1, under the 4.5:1 AA ` +
          `floor for the button label this pair exists to carry`,
      ).toBeGreaterThanOrEqual(4.5);
    });

    it(`${name}: body ink passes AA on the page`, () => {
      const t = declarations(block(selector));
      const page = t.get("--page");
      expect(page, `${name} has no --page`).toBeTruthy();

      for (const token of ["--ink", "--ink-2"]) {
        const value = t.get(token);
        expect(value, `${name} has no ${token}`).toBeTruthy();
        const measured = ratio(value as string, page as string);
        expect(
          measured,
          `${token} ${value} on --page ${page} is ${measured.toFixed(2)}:1`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    });
  }

  it("Daylight: the quiet ink scale passes AA everywhere the page goes", () => {
    const t = declarations(block(":root {"));
    const page = t.get("--page");
    expect(page, "Daylight has no --page").toBeTruthy();

    // `--page` is the flat fallback; what is actually behind these labels most
    // of the time is `--page-gradient`, and its palest stop is paler than the
    // fallback. That stop is the real worst case, so it is read back out of
    // the gradient rather than written down here — restyling the horizon
    // cannot then quietly lower the bar this test is holding.
    const stops = (t.get("--page-gradient") ?? "").match(/#[0-9a-f]{3,8}\b/gi);
    expect(stops?.length, "Daylight --page-gradient names no colours")
      .toBeGreaterThan(0);
    const palest = (stops as string[]).reduce((a, b) =>
      luminance(b) > luminance(a) ? b : a,
    );

    for (const token of ["--ink-3", "--ink-4"]) {
      const value = t.get(token);
      expect(value, `Daylight has no ${token}`).toBeTruthy();
      for (const [what, bg] of [
        ["--page", page as string],
        ["the palest gradient stop", palest],
      ] as Array<[string, string]>) {
        const measured = ratio(value as string, bg);
        expect(
          measured,
          `${token} ${value} on ${what} ${bg} is ${measured.toFixed(2)}:1. ` +
            `--ink-4 is what \`.label\` is set in at 11px, which is the most ` +
            `repeated text style in the app`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("Daylight: --ink-3-rgb is the same grey as --ink-3", () => {
    // The triple exists only so eighteen rules can take `--ink-3` at partial
    // alpha. Nothing makes the two track each other, and AUD-6 moved one of
    // them, so the drift is worth a line rather than a comment.
    const t = declarations(block(":root {"));
    const hex = (t.get("--ink-3") ?? "").replace("#", "");
    const triple = (t.get("--ink-3-rgb") ?? "")
      .split(",")
      .map((n) => Number(n.trim()));

    expect(triple, "--ink-3-rgb is not three numbers").toHaveLength(3);
    expect(
      triple,
      `--ink-3-rgb no longer matches --ink-3 #${hex}`,
    ).toEqual([0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16)));
  });
});

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

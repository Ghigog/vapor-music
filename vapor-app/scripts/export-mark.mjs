/**
 * Render the Vapor mark to PNG masters.
 *
 * WHY A RENDERER AND NOT A DRAWN FILE
 *
 * The mark is a shaded glass ribbon — per-pixel Fresnel, Beer absorption and
 * chromatically split refraction. There is no vector form of it, so the icons
 * cannot be an .svg that someone hand-edits. What there IS instead is
 * determinism: <vapor-ribbon> at a given `pose` is the same drawing every time,
 * on every machine. So the master is not a file someone drew and must not lose;
 * it is a number in src/lib/mark.json plus this script, and it regenerates.
 *
 * That is also why the app and the icons cannot drift. Both read mark.json, and
 * both are drawn by public/vapor-ribbon.js. Change the pose, run this, and the
 * logo on the boot screen and the logo in the dock move together.
 *
 * Output feeds `npx tauri icon`, which fans one master out to every desktop,
 * Android and iOS size Tauri needs.
 *
 * Usage
 *   node scripts/export-mark.mjs                     # the standard set
 *   node scripts/export-mark.mjs --pose 0 --contact-sheet
 */
import { chromium } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const MARK = JSON.parse(await readFile(resolve(ROOT, "src/lib/mark.json"), "utf8"));
const RIBBON = await readFile(resolve(ROOT, "public/vapor-ribbon.js"), "utf8");

const argv = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 && argv[i + 1] && !argv[i + 1].startsWith("--") ? argv[i + 1] : fallback;
};
const pose = Number(flag("pose", MARK.pose));
const sheet = argv.includes("--contact-sheet");

/**
 * Everything the mark needs to be reproduced, as element attributes.
 *
 * `field` is the important one and it is why this is not just a screenshot of
 * the app: the element caps its shading field at 300px to hold a frame budget,
 * which is right on screen and wrong for a 1024px master. Forcing field = size
 * renders 1:1 instead of upscaling a 300px buffer.
 */
const attrs = (size, theme, poseAt) => ({
  size: String(size),
  field: String(size),
  theme,
  state: "idle",
  pose: String(poseAt),
  ...Object.fromEntries(
    Object.entries({ ...MARK.geometry, ...MARK.material }).map(([k, v]) => [k, String(v)]),
  ),
});

const browser = await chromium.launch();
// deviceScaleFactor 1 so the canvas bitmap is exactly `size` px and nothing is
// resampled on the way out.
const page = await browser.newPage({ viewport: { width: 64, height: 64 }, deviceScaleFactor: 1 });
await page.addScriptTag({ content: RIBBON });

/**
 * Plate colours.
 *
 * These are the two environments the mark was designed against — the study's
 * page and its "On dark" panel — so the glass refracts something it expects
 * rather than whatever the user's dock happens to be.
 */
/**
 * Plate fills.
 *
 * White is the default because a macOS icon sits in a row of mostly light,
 * shaded tiles; a full-bleed dark slab reads as a hole in that row.
 */
const PLATES = {
  white: ["#ffffff", "#f4f6fb"],
  dark: ["#12131f", "#171a2c"],
  light: ["#f7f8fc", "#eceff9"],
};

/**
 * The macOS icon grid, MEASURED rather than remembered.
 *
 * Taken off /System/Applications/Music.app and Notes.app on macOS 26.5: both
 * put a body of exactly 80.47% of the canvas dead centre, with equal padding on
 * all four sides, and bake a faint downward shadow into that padding (peak
 * alpha 27 below the body, 8 above).
 *
 * The corner is NOT a circular arc. Fitting the measured profile — 51px inset
 * at the body's top edge decaying to 8px at 10% depth — gives a superellipse
 * |x|^n + |y|^n = 1 with n ≈ 2.2 and a corner extent of 24.76% of the body.
 * A circular `roundRect` at the same radius sits ~3px wide through the middle
 * of the curve, which is small in numbers and is precisely the "not quite an
 * Apple icon" tell.
 */
const GRID = {
  body: 0.8047,
  corner: 0.2476,
  exponent: 2.2,
  shadowAlpha: 0.11,
};

/** Draw one mark and return its canvas as raw PNG bytes. */
async function render(size, theme, poseAt, plate = "none", shape = "squircle") {
  const dataUrl = await page.evaluate(async (a) => {
    // Built detached with `pose` set LAST: the element only draws once it is
    // frozen, so setting pose last collapses what would be one full-resolution
    // redraw per attribute into a single draw. Appending frozen also means no
    // rAF loop is ever started.
    const el = document.createElement("vapor-ribbon");
    const { pose: p, plateStops: _ps, shape: _sh, grid: _g, inset: _i, ...rest } = a;
    for (const [k, v] of Object.entries(rest)) el.setAttribute(k, v);
    el.setAttribute("pose", p);
    document.body.append(el);
    await new Promise((r) => requestAnimationFrame(r));
    const canvas = el.shadowRoot.querySelector("canvas");
    if (!a.plateStops) {
      const bare = canvas.toDataURL("image/png");
      el.remove();
      return bare;
    }

    const S = canvas.width;
    const out = document.createElement("canvas");
    out.width = out.height = S;
    const g = out.getContext("2d");
    const G = a.grid;

    /**
     * Trace a superellipse-cornered rectangle.
     *
     * Each corner runs x = R − R·cos(θ)^(2/n), y = R − R·sin(θ)^(2/n), which
     * satisfies ((R−x)/R)^n + ((R−y)/R)^n = 1 — the continuous corner Apple
     * uses. n = 2 would give back a plain circular round-rect.
     */
    const squircle = (x0, y0, side, R, n) => {
      const e = 2 / n, N = 48;
      const pt = (cx, cy, sx, sy, t) => {
        const th = (t * Math.PI) / 2;
        return [cx + sx * (R - R * Math.pow(Math.cos(th), e)),
                cy + sy * (R - R * Math.pow(Math.sin(th), e))];
      };
      g.beginPath();
      const corners = [
        [x0, y0, 1, 1],                     // top-left
        [x0 + side, y0, -1, 1],             // top-right
        [x0 + side, y0 + side, -1, -1],     // bottom-right
        [x0, y0 + side, 1, -1],             // bottom-left
      ];
      corners.forEach(([cx, cy, sx, sy], ci) => {
        for (let i = 0; i <= N; i++) {
          // Alternate sweep direction so consecutive corners join head-to-tail.
          const t = ci % 2 === 0 ? i / N : 1 - i / N;
          const [px, py] = pt(cx, cy, sx, sy, t);
          if (ci === 0 && i === 0) g.moveTo(px, py); else g.lineTo(px, py);
        }
      });
      g.closePath();
    };

    // Full-bleed for platforms that apply their own mask (iOS, Android);
    // the measured macOS grid otherwise.
    const bleed = a.shape === "square";
    const side = bleed ? S : S * G.body;
    const org = (S - side) / 2;
    const R = side * G.corner;

    if (!bleed) {
      // The faint contact shadow real system icons bake into their padding.
      g.save();
      g.filter = `blur(${S * 0.012}px)`;
      g.globalAlpha = G.shadowAlpha;
      squircle(org, org + S * 0.008, side, R, G.exponent);
      g.fillStyle = "#2a2f45";
      g.fill();
      g.restore();
    }

    const grad = g.createLinearGradient(org, org, org + side * 0.4, org + side);
    grad.addColorStop(0, a.plateStops[0]);
    grad.addColorStop(1, a.plateStops[1]);
    if (bleed) { g.beginPath(); g.rect(0, 0, S, S); } else squircle(org, org, side, R, G.exponent);
    g.fillStyle = grad;
    g.fill();

    // Clip the mark to the plate so nothing spills past the corner.
    g.save();
    if (bleed) { g.beginPath(); g.rect(0, 0, S, S); } else squircle(org, org, side, R, G.exponent);
    g.clip();
    // Drawn at full size and scaled into the safe area, so the inset costs no
    // resolution.
    const inner = side * (1 - a.inset * 2);
    g.drawImage(canvas, org + (side - inner) / 2, org + (side - inner) / 2, inner, inner);
    g.restore();

    const url = out.toDataURL("image/png");
    el.remove();
    return url;
  }, {
    ...attrs(size, theme, poseAt),
    plateStops: plate === "none" ? null : PLATES[plate],
    shape,
    grid: GRID,
    inset: MARK.icon.inset,
  });
  return Buffer.from(dataUrl.split(",")[1], "base64");
}

async function emit(relPath, size, theme, plate = "none", shape = "squircle", poseAt = pose) {
  const out = resolve(ROOT, relPath);
  await mkdir(dirname(out), { recursive: true });
  const png = await render(size, theme, poseAt, plate, shape);
  await writeFile(out, png);
  const tag = plate === "none" ? "transparent" : `${plate}/${shape}`;
  console.log(`  ${relPath.padEnd(32)} ${String(size).padStart(4)}px ${theme.padEnd(5)} ${tag.padEnd(14)} (${(png.length / 1024).toFixed(1)} kB)`);
}

if (sheet) {
  // A pose is a permanent brand decision, so make it cheap to look at.
  console.log(`contact sheet — poses 0..13`);
  for (let p = 0; p <= 13; p++) await emit(`brand/sheet/pose-${p}.png`, 256, "light", "none", "squircle", p);
} else if (argv.includes("--options")) {
  console.log(`plate options — pose ${pose}`);
  for (const [name, plate, theme] of [
    ["a-white", "white", "light"],
    ["b-white-dark-mark", "white", "dark"],
    ["c-dark", "dark", "dark"],
  ]) await emit(`brand/options/${name}.png`, 512, theme, plate, "squircle");
} else {
  const plate = MARK.icon.plate;
  const theme = MARK.icon.markTheme;
  console.log(`mark masters — pose ${pose}, ${plate} plate`);

  // macOS: the measured grid — inset body, superellipse corner, baked shadow.
  // This is the master the .icns is cut from.
  await emit("brand/icon-macos-1024.png", 1024, theme, plate, "squircle");

  // Everything else masks the icon itself, so it wants full bleed. Handing a
  // pre-rounded image to iOS gets the corners rounded twice.
  await emit("brand/icon-square-1024.png", 1024, theme, plate, "square");

  // The bare mark, for in-app and marketing use where a plate would be wrong.
  await emit("brand/mark-1024.png", 1024, "light", "none");
  await emit("brand/mark-1024-dark.png", 1024, "dark", "none");

  await emit("public/favicon.png", 64, theme, plate, "square");
  await emit("public/apple-touch-icon.png", 180, theme, plate, "square");

  console.log(`\nnext: npx tauri icon brand/icon-square-1024.png --ios-color "#ffffff"`);
  console.log(`then: node scripts/make-icns.mjs   (macOS grid .icns)`);
}

await browser.close();

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
const PLATES = {
  dark: ["#12131f", "#171a2c"],
  light: ["#f7f8fc", "#eceff9"],
};

/** Draw one mark and return its canvas as raw PNG bytes. */
async function render(size, theme, poseAt, plate = "none") {
  const dataUrl = await page.evaluate(async (a) => {
    // Built detached with `pose` set LAST: the element only draws once it is
    // frozen, so setting pose last collapses what would be one full-resolution
    // redraw per attribute into a single draw. Appending frozen also means no
    // rAF loop is ever started.
    const el = document.createElement("vapor-ribbon");
    const { pose: p, plateStops: _ps, radius: _r, inset: _i, ...rest } = a;
    for (const [k, v] of Object.entries(rest)) el.setAttribute(k, v);
    el.setAttribute("pose", p);
    document.body.append(el);
    await new Promise((r) => requestAnimationFrame(r));
    const canvas = el.shadowRoot.querySelector("canvas");
    let url;
    if (a.plateStops) {
      // Composite onto the plate rather than letting the element draw over a
      // page background: the PNG has to carry the plate, since nothing behind
      // an app icon is under our control.
      const out = document.createElement("canvas");
      out.width = out.height = canvas.width;
      const g = out.getContext("2d");
      const S = out.width, r = S * a.radius;
      const grad = g.createLinearGradient(0, 0, S * 0.35, S);
      grad.addColorStop(0, a.plateStops[0]);
      grad.addColorStop(1, a.plateStops[1]);
      g.beginPath();
      g.roundRect(0, 0, S, S, r);
      g.fillStyle = grad;
      g.fill();
      // The mark is drawn at full size and scaled down into the safe area, so
      // the inset never costs resolution.
      const inner = S * (1 - a.inset * 2);
      g.drawImage(canvas, (S - inner) / 2, (S - inner) / 2, inner, inner);
      url = out.toDataURL("image/png");
    } else {
      url = canvas.toDataURL("image/png");
    }
    el.remove();
    return url;
  }, {
    ...attrs(size, theme, poseAt),
    plateStops: plate === "none" ? null : PLATES[plate],
    radius: MARK.icon.radius,
    inset: MARK.icon.inset,
  });
  return Buffer.from(dataUrl.split(",")[1], "base64");
}

async function emit(relPath, size, theme, plate = "none", poseAt = pose) {
  const out = resolve(ROOT, relPath);
  await mkdir(dirname(out), { recursive: true });
  const png = await render(size, theme, poseAt, plate);
  await writeFile(out, png);
  const tag = plate === "none" ? "transparent" : `${plate} plate`;
  console.log(`  ${relPath.padEnd(34)} ${String(size).padStart(4)}px ${theme.padEnd(5)} ${tag}  (${(png.length / 1024).toFixed(1)} kB)`);
}

if (sheet) {
  // A pose is a permanent brand decision, so make it cheap to look at.
  console.log(`contact sheet — poses 0..13`);
  for (let p = 0; p <= 13; p++) await emit(`brand/sheet/pose-${p}.png`, 256, "light", "none", p);
} else if (argv.includes("--options")) {
  // The three plate treatments, same pose, for picking between.
  console.log(`plate options — pose ${pose}`);
  await emit("brand/options/a-dark-plate.png", 512, "dark", "dark");
  await emit("brand/options/b-light-plate.png", 512, "light", "light");
  await emit("brand/options/c-transparent.png", 512, "light", "none");
} else {
  const plate = MARK.icon.plate;
  const theme = plate === "light" ? "light" : plate === "dark" ? "dark" : "light";
  console.log(`mark masters — pose ${pose}, turns ${MARK.geometry.turns}, ${plate} plate`);

  // The icon master. Opaque and full-bleed when plated, which is what iOS
  // requires and what every other platform tolerates.
  await emit("brand/icon-1024.png", 1024, theme, plate);

  // The bare mark, for in-app and marketing use where a plate would be wrong.
  await emit("brand/mark-1024.png", 1024, "light", "none");
  await emit("brand/mark-1024-dark.png", 1024, "dark", "none");

  // Web. The favicon is plated for the same reason the app icon is — a browser
  // tab strip is someone else's background.
  await emit("public/favicon.png", 64, theme, plate);
  await emit("public/apple-touch-icon.png", 180, theme, plate);

  console.log(`\nnext: npx tauri icon brand/icon-1024.png`);
}

await browser.close();

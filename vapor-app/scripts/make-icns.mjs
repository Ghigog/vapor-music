/**
 * Build src-tauri/icons/icon.icns from the macOS-grid master.
 *
 * WHY THIS EXISTS SEPARATELY FROM `tauri icon`
 *
 * `tauri icon` takes one master and fans it out to every platform, but macOS
 * and everyone else want different pictures:
 *
 *   · iOS and Android apply their own mask, so they need a FULL-BLEED square.
 *     Handing them a pre-rounded image rounds the corners twice and leaves a
 *     pale rind inside the system's mask.
 *   · macOS applies no mask at all. The app supplies the shape, and by
 *     convention that shape is a superellipse occupying 80.47% of the canvas,
 *     centred, with a faint shadow baked into the remaining padding. Those
 *     numbers are measured off Music.app and Notes.app on macOS 26.5 — see
 *     GRID in export-mark.mjs.
 *
 * So `tauri icon` runs against the square master, and this then replaces the
 * one file it got wrong.
 *
 * Usage: node scripts/make-icns.mjs
 */
import { execFile } from "node:child_process";
import { mkdtemp, rm, access } from "node:fs/promises";
import { promisify } from "node:util";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const run = promisify(execFile);
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const MASTER = resolve(ROOT, "brand/icon-macos-1024.png");
const OUT = resolve(ROOT, "src-tauri/icons/icon.icns");

try {
  await access(MASTER);
} catch {
  console.error(`missing ${MASTER}\nrun: npm run mark`);
  process.exit(1);
}

// The ten representations iconutil expects. Downscaled from the 1024 master
// rather than re-rendered per size: the shading field would be coarser at small
// sizes than a filtered downscale of the big one.
const REPS = [
  [16, "icon_16x16.png"], [32, "icon_16x16@2x.png"],
  [32, "icon_32x32.png"], [64, "icon_32x32@2x.png"],
  [128, "icon_128x128.png"], [256, "icon_128x128@2x.png"],
  [256, "icon_256x256.png"], [512, "icon_256x256@2x.png"],
  [512, "icon_512x512.png"], [1024, "icon_512x512@2x.png"],
];

const work = await mkdtemp(join(tmpdir(), "vapor-icns-"));
const iconset = join(work, "icon.iconset");
await run("mkdir", ["-p", iconset]);

for (const [px, name] of REPS) {
  await run("sips", ["-z", String(px), String(px), MASTER, "--out", join(iconset, name)]);
}
await run("iconutil", ["-c", "icns", iconset, "-o", OUT]);
await rm(work, { recursive: true, force: true });

const { stdout } = await run("sips", ["-g", "pixelWidth", OUT]);
console.log(`wrote ${OUT.replace(ROOT + "/", "")}  (${stdout.trim().split("\n").pop().trim()})`);

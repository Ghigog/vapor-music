/**
 * Build the macOS 26 layered app icon (.icon) and install it into a built .app.
 *
 * WHY THIS AND NOT JUST AN .icns
 *
 * Tahoe composes app icons at runtime. Given a layered asset it draws the tile,
 * the mask, the material, the specular highlight and the shadow itself, and
 * derives the Dark / Clear / Tinted appearances from the same layers. Given a
 * flat .icns it can only show the bitmap — which is why a hand-drawn dark tile
 * sits out of a Dock that has gone tinted: the system had nothing to recolour.
 *
 * So the artwork we hand over is the MARK ALONE on transparency. No plate, no
 * corner, no shadow. Every one of those is Apple's to draw, and supplying our
 * own gets it drawn on top of theirs.
 *
 * The format is a directory: icon.json plus Assets/. Compiled with actool into
 * an Assets.car (what Tahoe actually reads) and an AppIcon.icns (the fallback
 * for older systems and for Finder's legacy paths). Both ship.
 *
 * Notes from probing actool, since none of this is documented:
 *   · colours are "<colorspace>:<r>,<g>,<b>,<a>" — a bare #hex is rejected
 *   · the bundle must be named AppIcon.icon to match --app-icon AppIcon, or
 *     actool emits Assets.car and silently no .icns
 *   · `specular` and `translucency` do not change the fallback .icns but DO
 *     change Assets.car, so they are runtime compositor settings and worth
 *     setting even though the static preview looks identical
 *   · shadow kinds are "neutral" and "none"; "chromatic" is rejected
 *   · `is-glass` on a layer and `blur-material` on a group are not accepted here
 *
 * Usage: node scripts/make-icon.mjs [path/to/App.app]
 */
import { execFile } from "node:child_process";
import { mkdir, rm, copyFile, writeFile, readFile, access } from "node:fs/promises";
import { promisify } from "node:util";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const run = promisify(execFile);
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const MARK = JSON.parse(await readFile(resolve(ROOT, "src/lib/mark.json"), "utf8"));
const LAYER = resolve(ROOT, "brand/icon-layer-1024.png");
const DOC = resolve(ROOT, "brand/AppIcon.icon");
const APP = process.argv[2]
  ? resolve(process.argv[2])
  : resolve(ROOT, "src-tauri/target/release/bundle/macos/Vapor Music.app");

try { await access(LAYER); } catch {
  console.error(`missing ${LAYER}\nrun: npm run mark`);
  process.exit(1);
}

/** actool wants an explicit colour space; a bare hex string is rejected. */
const srgb = (r, g, b, a = 1) =>
  `srgb:${[r, g, b, a].map((v) => v.toFixed(5)).join(",")}`;

/**
 * APPEARANCES — why this is not "make it green".
 *
 * Tinted is not a palette swap. You author a GREYSCALE base and the system
 * colourises it with whatever tint the user has chosen, so the only thing the
 * author controls is contrast. A white tile hands the system near-white to
 * colourise and comes back looking untinted — which is why the icon sat out of
 * a tinted Dock while Discord, a flat mid-luminance icns, tinted correctly.
 *
 * So the tinted fill is a NEUTRAL GREY. Any colour cast here would skew every
 * tint the user might pick; grey renders all of them faithfully.
 *
 * The artwork has to swap too. One image cannot serve both: the default sits on
 * a white tile and wants a darker mark, Dark and Tinted sit on a dark base and
 * want a light one. `image-name-specializations` carries the second image —
 * verified embedded, 7 of 9 image digests in the compiled catalog change when
 * it is present.
 */
const APPEARANCES = MARK.icon.appearances ?? {};
const TINTED_APPEARANCES = ["light-tint", "dark-tint"];

const fillSpecs = [];
if (APPEARANCES.dark) fillSpecs.push({ appearance: "dark-color", value: { solid: APPEARANCES.dark } });
if (APPEARANCES.tinted) {
  for (const a of TINTED_APPEARANCES) fillSpecs.push({ appearance: a, value: { solid: APPEARANCES.tinted } });
}

const LIGHT_LAYER = resolve(ROOT, "brand/icon-layer-light-1024.png");
let hasLight = true;
try { await access(LIGHT_LAYER); } catch { hasLight = false; }
const imageSpecs = hasLight
  ? ["dark-color", ...TINTED_APPEARANCES].map((a) => ({ appearance: a, value: "mark-light.png" }))
  : [];

// The document. One group holding the mark, on a white tile.
const doc = {
  fill: { solid: srgb(1, 1, 1) },
  ...(fillSpecs.length ? { "fill-specializations": fillSpecs } : {}),
  groups: [
    {
      layers: [
        {
          "image-name": "mark.png",
          ...(imageSpecs.length ? { "image-name-specializations": imageSpecs } : {}),
        },
      ],
      // Let the system light the mark and drop its shadow. Neutral rather than
      // tinted: the ribbon already carries the colour.
      specular: true,
      shadow: { kind: "neutral", opacity: MARK.icon.shadowOpacity ?? 0.5 },
    },
  ],
  "supported-platforms": { circles: ["watchOS"], squares: ["macOS", "iOS"] },
};

await rm(DOC, { recursive: true, force: true });
await mkdir(join(DOC, "Assets"), { recursive: true });
await copyFile(LAYER, join(DOC, "Assets", "mark.png"));
if (hasLight) await copyFile(LIGHT_LAYER, join(DOC, "Assets", "mark-light.png"));
await writeFile(join(DOC, "icon.json"), JSON.stringify(doc, null, 2) + "\n");
console.log(`wrote ${DOC.replace(ROOT + "/", "")}`);

const out = resolve(ROOT, "brand/.compiled");
await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });
const partial = join(out, "partial.plist");

const { stdout } = await run("xcrun", [
  "actool", DOC, "--compile", out, "--platform", "macosx",
  "--minimum-deployment-target", "26.0", "--app-icon", "AppIcon",
  "--include-all-app-icons", "--output-partial-info-plist", partial,
]);
if (!stdout.includes("Assets.car")) {
  console.error("actool produced no Assets.car:\n" + stdout);
  process.exit(1);
}
console.log("compiled Assets.car + AppIcon.icns");

try { await access(APP); } catch {
  console.log(`\nno .app at ${APP}\nbuild first (npm run app:build), then re-run.`);
  process.exit(0);
}

// Install into the bundle. Tauri has already put its own icon.icns there and
// pointed CFBundleIconFile at it; both keys are repointed at AppIcon so the
// layered asset wins on Tahoe and the compiled icns covers everything older.
const res = join(APP, "Contents", "Resources");
await copyFile(join(out, "Assets.car"), join(res, "Assets.car"));
await copyFile(join(out, "AppIcon.icns"), join(res, "AppIcon.icns"));
const plist = join(APP, "Contents", "Info.plist");
for (const [k, v] of [["CFBundleIconName", "AppIcon"], ["CFBundleIconFile", "AppIcon"]]) {
  await run("/usr/libexec/PlistBuddy", ["-c", `Delete :${k}`, plist]).catch(() => {});
  await run("/usr/libexec/PlistBuddy", ["-c", `Add :${k} string ${v}`, plist]);
}
// The bundle's signature covers Resources and Info.plist, so it must be redone.
await run("codesign", ["--force", "--deep", "--sign", "-", APP]).catch((e) => {
  console.warn("  codesign failed (ad-hoc):", e.message.split("\n")[0]);
});
console.log(`installed into ${APP.replace(ROOT + "/", "")}`);

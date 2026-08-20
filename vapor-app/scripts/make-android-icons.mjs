/**
 * Rewrite the Android adaptive-icon layers.
 *
 * WHY `tauri icon` GETS THIS WRONG
 *
 * It fans one master out to every platform, so ic_launcher_foreground ends up
 * being the master — plate and all — which is 100% opaque. An adaptive icon
 * expects the opposite: the FOREGROUND is the logo alone on transparency and
 * the BACKGROUND supplies the fill. Handing Android an opaque foreground means
 * the launcher masks a white tile over a white background and the mark sits
 * small in the middle of it, and the parallax the launcher applies to the
 * foreground has a plate to slide around.
 *
 * The canvas is 108dp with only the middle 66dp guaranteed visible — every
 * launcher masks the rest to its own shape. So the mark is drawn at ~62% and
 * everything outside it stays transparent.
 *
 * Also writes a monochrome layer. That is Android 13+ themed icons, which is
 * the same idea as the macOS tinted appearance: the system recolours a single
 * -colour silhouette to the user's theme. Without it, a themed home screen
 * falls back to showing the icon untinted — the exact complaint we just fixed
 * on the desktop side.
 *
 * Run after `tauri icon`, which is what put the wrong files there.
 *
 * Usage: node scripts/make-android-icons.mjs
 */
import { chromium } from "@playwright/test";
import { readFile, writeFile, access } from "node:fs/promises";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const MARK = JSON.parse(await readFile(resolve(ROOT, "src/lib/mark.json"), "utf8"));
const SRC = resolve(ROOT, "brand/icon-layer-1024.png");
const RES = resolve(ROOT, "src-tauri/gen/android/app/src/main/res");

try { await access(SRC); } catch {
  console.error(`missing ${SRC}\nrun: npm run mark`);
  process.exit(1);
}

/** 108dp canvas per density bucket. */
const DENSITIES = [
  ["mdpi", 108], ["hdpi", 162], ["xhdpi", 216], ["xxhdpi", 324], ["xxxhdpi", 432],
];

// Fraction of the 108dp canvas the mark may occupy. The guaranteed-visible
// circle is 66/108; staying just inside it keeps the mark whole under every
// launcher mask.
const SAFE = MARK.icon.androidSafe ?? 0.60;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 64, height: 64 }, deviceScaleFactor: 1 });
const srcData = (await readFile(SRC)).toString("base64");

/**
 * Draw the mark fitted to the safe zone on transparency; `mono` flattens it to
 * one colour.
 *
 * The source layer is ALREADY inset for Icon Composer, so scaling it as a whole
 * would inset twice and leave the mark at ~36% of the canvas against a 61% safe
 * zone. Instead its opaque bounding box is measured and that box is fitted to
 * the safe zone, which makes the result independent of whatever padding the
 * source happens to carry.
 */
async function layer(px, mono) {
  const url = await page.evaluate(async ([d, px, mono, safe]) => {
    const img = new Image();
    await new Promise((ok) => { img.onload = ok; img.src = "data:image/png;base64," + d; });
    const m = document.createElement("canvas");
    m.width = m.height = img.width;
    const mg = m.getContext("2d");
    mg.drawImage(img, 0, 0);
    const N = img.width, data = mg.getImageData(0, 0, N, N).data;
    let x0 = N, x1 = -1, y0 = N, y1 = -1;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
      if (data[(y * N + x) * 4 + 3] > 8) {
        if (x < x0) x0 = x; if (x > x1) x1 = x;
        if (y < y0) y0 = y; if (y > y1) y1 = y;
      }
    }
    const bw = x1 - x0 + 1, bh = y1 - y0 + 1;
    const c = document.createElement("canvas");
    c.width = c.height = px;
    const g = c.getContext("2d");
    // Fit the LONGER side of the mark to the safe zone so it stays inside the
    // guaranteed-visible circle whichever way round it is.
    const k = (px * safe) / Math.max(bw, bh);
    const dw = bw * k, dh = bh * k;
    g.drawImage(img, x0, y0, bw, bh, (px - dw) / 2, (px - dh) / 2, dw, dh);
    if (mono) {
      // Themed icons want a single-colour silhouette; the system recolours it.
      // Alpha is kept as the shape and every pixel forced to one ink.
      g.globalCompositeOperation = "source-in";
      g.fillStyle = "#000000";
      g.fillRect(0, 0, px, px);
    }
    return c.toDataURL("image/png");
  }, [srcData, px, mono, SAFE]);
  return Buffer.from(url.split(",")[1], "base64");
}

for (const [bucket, px] of DENSITIES) {
  const dir = join(RES, `mipmap-${bucket}`);
  await writeFile(join(dir, "ic_launcher_foreground.png"), await layer(px, false));
  await writeFile(join(dir, "ic_launcher_monochrome.png"), await layer(px, true));
  console.log(`  mipmap-${bucket.padEnd(8)} foreground + monochrome @ ${px}px`);
}
await browser.close();

// Declare the monochrome layer, which tauri's generated XML does not have.
const xml = `<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/ic_launcher_background"/>
    <foreground android:drawable="@mipmap/ic_launcher_foreground"/>
    <monochrome android:drawable="@mipmap/ic_launcher_monochrome"/>
</adaptive-icon>
`;
for (const name of ["ic_launcher.xml", "ic_launcher_round.xml"]) {
  const p = join(RES, "mipmap-anydpi-v26", name);
  try { await access(p); await writeFile(p, xml); console.log(`  wrote ${name}`); } catch {}
}
console.log(`\nadaptive foreground is now transparent outside the mark; background stays @color/ic_launcher_background`);

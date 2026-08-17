/**
 * Check that a built `dist/` can actually load itself.
 *
 * Vite names bundles by content hash, so `index.html` points at
 * `assets/index-<hash>.js`. If two builds overlap — a rebuild landing while a
 * package step copies the directory, or two processes building at once — the
 * HTML can end up naming a chunk that no longer exists. The app then starts,
 * serves the page, loads no JavaScript, and shows an empty window: no React, so
 * not even the error boundary that would otherwise say what went wrong.
 *
 * That failure is invisible to every other check. `tsc` passes, `vite build`
 * exits 0, the component tests pass, and the e2e suite runs against its own
 * separate build. Only the packaged app is broken, and only until the next
 * clean build — which makes it exactly the kind of thing that gets described as
 * "it just didn't open" and never reproduced.
 *
 * ## What this can and cannot catch
 *
 * It runs immediately after `vite build`, so it verifies the bundle that build
 * just produced. `tauri build` regenerates `dist` through this same script
 * before packaging, which means a `dist` that was *already* torn is simply
 * replaced rather than caught — that path is safe for a different reason.
 *
 * What it catches is a build that produced an inconsistent or empty `dist` of
 * its own accord, and a hand-run `npm run build` whose output is broken before
 * anyone packages it. What it cannot catch is another process rewriting `dist`
 * in the window between this check and the point Tauri embeds the files; that
 * window is small and the answer to it is not to run two builds at once.
 *
 * Run after any build that will be shipped or bundled.
 */

import { readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "..", "dist");
const index = join(dist, "index.html");

if (!existsSync(index)) {
  console.error(`verify-dist: no ${index} — run \`npm run build\` first.`);
  process.exit(1);
}

const html = readFileSync(index, "utf8");

// Every local src/href in the document. Remote and inline sources are not this
// script's business — the CSP is what governs those.
const refs = [...html.matchAll(/(?:src|href)="([^"]+)"/g)]
  .map((m) => m[1])
  .filter((r) => !/^(https?:|data:|#|mailto:)/.test(r));

const missing = refs.filter((r) => !existsSync(join(dist, r.replace(/^\//, ""))));

if (missing.length > 0) {
  console.error("verify-dist: index.html points at files that are not there:");
  for (const m of missing) console.error(`  ${m}`);
  console.error(
    "\nThe bundle is torn. This usually means two builds overlapped.\n" +
      "Delete dist/ and build again before packaging.",
  );
  process.exit(1);
}

// A build that produced no script at all would pass the check above by having
// nothing to check, which is the failure this whole script exists to catch.
const scripts = refs.filter((r) => r.endsWith(".js"));
if (scripts.length === 0) {
  console.error("verify-dist: index.html loads no JavaScript at all.");
  process.exit(1);
}

console.log(`verify-dist: ${refs.length} local references resolve, ${scripts.length} script(s).`);

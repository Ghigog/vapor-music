/**
 * What build is this?
 *
 * Baked in at compile time rather than asked for over IPC. A version is a
 * static string; fetching it through `@tauri-apps/api/app` would make the
 * About screen await the backend for something the bundler already knows, and
 * would need a mock in every test environment that does not have a backend —
 * jsdom and the browser e2e both.
 *
 * The reason it exists at all: there is no telemetry and no crash reporting,
 * so the only channel is a person saying what happened. "It crashed" is not
 * actionable. "It crashed, 2.0.0, a3f9c21" names the tree.
 *
 * Both vite configs use this so the dev server, the production bundle and the
 * e2e build all stamp the same way.
 */
import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");

export function buildStamp() {
  const version = JSON.parse(
    readFileSync(path.join(root, "package.json"), "utf8"),
  ).version;

  let commit = "unknown";
  try {
    commit = execSync("git rev-parse --short HEAD", {
      cwd: root,
      // Errors are the expected case in a source tarball, not a failure worth
      // printing: the version still identifies the release.
      stdio: ["ignore", "pipe", "ignore"],
    })
      .toString()
      .trim();
  } catch {
    // Left as "unknown".
  }

  return {
    __APP_VERSION__: JSON.stringify(version),
    __APP_COMMIT__: JSON.stringify(commit),
  };
}

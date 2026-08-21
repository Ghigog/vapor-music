/**
 * Fail if the checked-in TypeScript is not what the Rust currently produces.
 *
 * The types under `src/lib/generated/` are derived by `ts-rs` from the structs
 * in `vapor-core`. They are committed so the frontend can be built and typed
 * without a Rust toolchain, which means they can also go stale — someone adds
 * a field, does not run the Rust tests, and the frontend types silently
 * describe a shape the backend no longer sends. That is the exact failure this
 * whole arrangement exists to prevent, so it is checked rather than trusted.
 *
 * Regenerating is `cargo test` in `vapor-core` and in `vapor-app/src-tauri`;
 * the export runs as part of the normal suite in both.
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..", "..");
const committed = join(root, "vapor-app", "src", "lib", "generated");
const scratch = mkdtempSync(join(tmpdir(), "vapor-types-"));

try {
  // Both crates: `vapor-core` holds the shared library types and
  // `src-tauri` the per-command response shapes. They export into one
  // directory, so both have to run or half the set looks deleted.
  for (const crate of ["vapor-core", join("vapor-app", "src-tauri")]) {
    execFileSync("cargo", ["test", "--quiet", "export_bindings"], {
      cwd: join(root, crate),
      env: { ...process.env, TS_RS_EXPORT_DIR: scratch },
      stdio: "pipe",
    });
  }

  const read = (dir) =>
    Object.fromEntries(
      readdirSync(dir)
        .filter((f) => f.endsWith(".ts"))
        .map((f) => [f, readFileSync(join(dir, f), "utf8")]),
    );

  const fresh = read(scratch);
  const old = read(committed);
  const names = [...new Set([...Object.keys(fresh), ...Object.keys(old)])].sort();
  const drifted = names.filter((n) => fresh[n] !== old[n]);

  if (drifted.length > 0) {
    console.error(
      "Generated types are stale:\n" +
        drifted.map((n) => `  ${n}`).join("\n") +
        "\n\nRegenerate with:  (cd vapor-core && cargo test) && \\\n                  (cd vapor-app/src-tauri && cargo test)\n",
    );
    process.exit(1);
  }
  console.log(`generated types match the Rust (${names.length} files)`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}

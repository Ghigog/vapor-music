/**
 * Fail if the APK's Java half and its native half disagree about names.
 *
 * Written 2026-08-24, the night v2.0.0-rc.3 installed and crashed on launch.
 *
 * That APK was the first release build this project ever produced, and so also
 * the first time R8 had ever run against it — every earlier Android build was
 * `--debug`, where minification does not happen. Nothing in CI noticed, because
 * nothing in CI had ever looked inside an APK: `app.yml` runs `cargo check` and
 * `cargo clippy` for the Android target, which prove the Rust compiles and say
 * nothing about whether the thing that comes out of Gradle runs.
 *
 * The failure mode this exists for is specific and silent. The Rust half
 * reaches the Java half by *string*:
 *
 *   - `#[no_mangle] extern "system" fn Java_com_dylangrowcoot_vapormusic_MainActivity_setupNdkContext`
 *     is linked to `external fun setupNdkContext()` by the JVM matching that
 *     name at runtime. Nothing checks it at build time.
 *   - wry and tauri call `env.find_class("app/tauri/plugin/PluginManager")` and
 *     friends, again by string.
 *
 * R8 is free to rename or drop anything it cannot see referenced, and a keep
 * rule that misses one of these produces a build that compiles, packages,
 * signs, uploads and then dies on the first launch. The build is green the
 * whole way.
 *
 * So: pull the names out of both halves and check they still line up. This is
 * not a substitute for launching the app — it cannot be — but it turns the
 * exact failure that shipped into a red build in about a second.
 *
 * Usage:  node scripts/verify-apk.mjs <path-to.apk>
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";

const apk = process.argv[2];
if (!apk || !existsSync(apk)) {
  console.error("usage: node scripts/verify-apk.mjs <path-to.apk>");
  process.exit(2);
}

/** Every entry in the zip, so we can find the dex and the .so without unpacking. */
const entries = execFileSync("unzip", ["-Z1", apk], { encoding: "utf8" })
  .split("\n")
  .map((l) => l.trim())
  .filter(Boolean);

const dexes = entries.filter((e) => /^classes\d*\.dex$/.test(e));
const sos = entries.filter((e) => e.endsWith(".so") && e.includes("vapor"));

if (dexes.length === 0) throw new Error("no classes.dex in the APK");
if (sos.length === 0) throw new Error("no libvapor_*.so in the APK");

/**
 * Printable ASCII runs out of a binary.
 *
 * Both formats keep their identifiers as plain NUL- or length-delimited ASCII,
 * so this is enough to read the names out of either without a parser for
 * either. A real DEX parser would be more precise and is not worth the code:
 * a false *negative* here costs nothing (some other check or a device finds
 * it), and this method produces no false positives, because a name that is
 * present is genuinely present.
 */
/**
 * One character, not the four a `strings(1)` habit would suggest.
 *
 * A DEX string entry is a ULEB128 length, the bytes, then a NUL — so a short
 * method name sits between two unprintable bytes and is a run of its own
 * length. `Rust.ipc` is three characters, and a four-character floor reported
 * it missing from an APK that contained it. The first version of this script
 * failed exactly that way, on the APK it was written to diagnose.
 */
const MIN_RUN = 1;

const stringsOf = (entry) => {
  const buf = execFileSync("unzip", ["-p", apk, entry], {
    maxBuffer: 512 * 1024 * 1024,
  });
  const out = [];
  let cur = "";
  for (const byte of buf) {
    if (byte >= 0x20 && byte < 0x7f) {
      cur += String.fromCharCode(byte);
    } else {
      if (cur.length >= MIN_RUN) out.push(cur);
      cur = "";
    }
  }
  if (cur.length >= MIN_RUN) out.push(cur);
  return out;
};

const dexText = dexes.flatMap(stringsOf).join("\n");
const soText = sos.flatMap(stringsOf).join("\n");

const problems = [];

// ---------------------------------------------------------------------------
// 1. Every JNI entry point the native library exports must still exist in Java.
//
// The symbol encodes the class and method the JVM will look for. `_1` is JNI's
// escape for a literal underscore; the package separator is a plain `_`, which
// is why this only checks our own package — one whose name has no underscores
// in it, so the decoding is unambiguous.
// ---------------------------------------------------------------------------
const PKG = "com_dylangrowcoot_vapormusic";
const jni = [
  ...new Set(soText.match(new RegExp(`Java_${PKG}_[A-Za-z0-9_]+`, "g")) ?? []),
];

if (jni.length === 0) {
  problems.push(
    "no Java_com_dylangrowcoot_vapormusic_* symbols in the native library at " +
      "all — either the .so is not the one this app loads, or the JNI layer " +
      "has gone. Both are worse than a rename.",
  );
}

for (const sym of jni) {
  const rest = sym.slice(`Java_${PKG}_`.length);
  const cut = rest.indexOf("_");
  if (cut < 0) continue; // no method part; not a shape we emit
  const cls = rest.slice(0, cut);
  const method = rest.slice(cut + 1).replaceAll("_1", "_");

  if (!dexText.includes(`Lcom/dylangrowcoot/vapormusic/${cls};`)) {
    problems.push(
      `${sym}\n    the native library exports this, but the DEX has no ` +
        `class com.dylangrowcoot.vapormusic.${cls}.\n    R8 renamed or removed ` +
        `it; the JVM will throw UnsatisfiedLinkError at startup.`,
    );
  } else if (!dexText.includes(method)) {
    problems.push(
      `${sym}\n    class ${cls} survived but the method name "${method}" is ` +
        `not in the DEX.\n    R8 renamed the method; the link fails at ` +
        `startup, not at build time.`,
    );
  }
}

// ---------------------------------------------------------------------------
// 2. Every class the native library resolves by string must still be there.
//
// wry and tauri do this from Rust — `find_class("app/tauri/plugin/PluginManager")`
// — and a missing one is a ClassNotFoundException on the first frame.
// ---------------------------------------------------------------------------
const resolved = [
  ...new Set(
    soText.match(/\b(?:app\/tauri|com\/dylangrowcoot)[A-Za-z0-9/$_]*/g) ?? [],
  ),
].filter((c) => /\/[A-Z]/.test(c)); // a class, not just a package prefix

for (const cls of resolved) {
  if (!dexText.includes(`L${cls};`)) {
    problems.push(
      `${cls}\n    the native library looks this class up by name, and it is ` +
        `not in the DEX.\n    R8 renamed or removed it; find_class returns ` +
        `null and the app dies on launch.`,
    );
  }
}

// ---------------------------------------------------------------------------
// 3. R8 must not have run at all.
//
// Minification is off for the release build (gen/android/build.gradle.kts),
// because the first APK it ever touched crashed on launch. This check exists
// because turning it off is easy to do and easy to *think* you have done: the
// first attempt set `isMinifyEnabled = false` at a point in Gradle's lifecycle
// where the Android template's own release block overwrote it afterwards. The
// build stayed green, the APK came out the same size as the broken one, and
// v2.0.0-rc.4 shipped with R8 still on.
//
// The tell is a class whose last package segment is a single lowercase letter.
// R8 renames what it cannot see referenced, starting at `a`, and nothing in
// either of these packages is legitimately named that.
//
// If minification is ever turned back on deliberately — with a device to prove
// it — this check has to be removed in the same change. That is the point of
// it: the decision becomes visible instead of silent.
// ---------------------------------------------------------------------------
const renamed = [
  ...new Set(
    dexText.match(/L(?:app\/tauri|com\/dylangrowcoot)[A-Za-z0-9/$_]*\/[a-z];/g) ??
      [],
  ),
];

if (renamed.length > 0) {
  problems.push(
    `R8 ran on this APK, and it is supposed to be off.\n    ` +
      `${renamed.length} obfuscated class name(s), e.g. ${renamed.slice(0, 3).join(", ")}\n    ` +
      `gen/android/build.gradle.kts sets isMinifyEnabled = false from ` +
      `afterEvaluate.\n    If that stopped taking effect, the release build ` +
      `type is being reconfigured after it.`,
  );
}

// ---------------------------------------------------------------------------

console.log(`apk        ${apk}`);
console.log(`dex        ${dexes.join(", ")}`);
console.log(`native     ${sos.join(", ")}`);
console.log(`jni exports checked      ${jni.length}`);
console.log(`resolved classes checked ${resolved.length}`);
console.log(`obfuscated class names     ${renamed.length} (expected 0)`);

if (problems.length > 0) {
  console.error(
    `\nThe Java and native halves of this APK disagree about ${problems.length} ` +
      `name(s):\n\n` +
      problems.map((p) => `  ${p}`).join("\n\n") +
      `\n\nThis APK would install and then crash on launch. If minification is ` +
      `on,\nthe cause is a missing keep rule in app/proguard-rules.pro — see ` +
      `docs/ANDROID.md.\n`,
  );
  process.exit(1);
}

console.log("\nthe Java and native halves agree on every name");

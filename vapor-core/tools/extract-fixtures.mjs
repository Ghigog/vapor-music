#!/usr/bin/env node
//
// Extract Essentia ground truth from the running Godot app's metadata cache.
//
// The Godot build has already analysed the whole library with Essentia. That
// output is the only large, real reference set available for validating the
// Rust replacement — synthetic signals prove correctness on the easy cases and
// say nothing about how either implementation behaves on actual music.
//
// The generated file is NOT committed: it is multi-megabyte and contains the
// user's own track paths. Regenerate it locally.
//
// Usage:
//   node vapor-core/tools/extract-fixtures.mjs [metadata_cache.json] [audio_cache_dir]

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import os from "node:os";

const DEFAULT_USERDATA = path.join(
  os.homedir(),
  "Library/Application Support/Godot/app_userdata/vapor music",
);

const cachePath = process.argv[2] ?? path.join(DEFAULT_USERDATA, "metadata_cache.json");
const audioDir = process.argv[3] ?? path.join(DEFAULT_USERDATA, "audio_cache");
const outPath = path.join(
  path.dirname(new URL(import.meta.url).pathname),
  "../fixtures/essentia_ground_truth.json",
);

if (!fs.existsSync(cachePath)) {
  console.error(`error: metadata cache not found at ${cachePath}`);
  console.error("Run the Godot app and let it analyse the library first.");
  process.exit(1);
}

const cache = JSON.parse(fs.readFileSync(cachePath, "utf8"));

const out = [];
let noAnalysis = 0;
let noAudio = 0;

for (const [href, entry] of Object.entries(cache)) {
  // Skip anything Essentia never produced a result for — including the tracks
  // the stub used to fabricate 120 BPM / 8A for.
  if (!(entry.bpm > 0) || !entry.musical_key) {
    noAnalysis++;
    continue;
  }

  // AudioManager derives the cache filename as md5(href) + original extension
  // (see audio_manager.gd `is_track_cached`).
  const ext = (href.split(".").pop() || "mp3").toLowerCase();
  const file = crypto.createHash("md5").update(href, "utf8").digest("hex") + "." + ext;

  if (!fs.existsSync(path.join(audioDir, file))) {
    noAudio++;
    continue;
  }

  out.push({
    href,
    file,
    ext,
    bpm: entry.bpm,
    camelot: entry.musical_key,
    duration: entry.duration,
    lufs: entry.lufs,
    cue_in: entry.cue_in,
    cue_out: entry.cue_out,
    beat_grid: entry.beat_grid,
    downbeats: entry.downbeats ?? [],
  });
}

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(out, null, 1));

const byExt = {};
for (const o of out) byExt[o.ext] = (byExt[o.ext] ?? 0) + 1;

console.log(`wrote ${out.length} fixtures -> ${path.relative(process.cwd(), outPath)}`);
console.log(`  by container: ${Object.entries(byExt).map(([k, v]) => `${k}=${v}`).join(" ")}`);
if (noAnalysis) console.log(`  skipped (no Essentia result): ${noAnalysis}`);
if (noAudio) console.log(`  skipped (audio not cached): ${noAudio}`);

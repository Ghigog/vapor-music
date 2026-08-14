//! Validation harness: compare `vapor-dsp` against the Essentia results the
//! Godot app already produced for the real library.
//!
//! This is the spike's actual deliverable. It reports a number, not an
//! impression: what fraction of 563 real tracks this implementation agrees with
//! Essentia on, for tempo and for key.
//!
//! Usage:
//!   cargo run --release --bin validate -- <audio_cache_dir> [limit]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    href: String,
    file: String,
    ext: String,
    bpm: f64,
    camelot: String,
    #[serde(default)]
    beat_grid: Vec<f32>,
}

/// Ratios that count as a *metrical* rather than an outright wrong tempo.
///
/// The original list held only octave relations (1/2, 2, 1/3, 3). Rendering a
/// real transition surfaced a track detected at 83.4 BPM that Essentia calls
/// 110.9 — a 3:4 relation, which was being counted as a plain failure and hid
/// how large the metrical-error class really is (MIG-014).
const METRICAL_RATIOS: [f64; 8] = [
    0.5,
    2.0,
    1.0 / 3.0,
    3.0,
    2.0 / 3.0,
    3.0 / 2.0,
    3.0 / 4.0,
    4.0 / 3.0,
];

/// Standard beat-tracking tolerance.
const BEAT_TOLERANCE: f32 = 0.07;

/// Tempo agreement tolerance. Essentia's own BPM is itself an estimate, and the
/// app only uses BPM to choose transition types and pitch-adjust by 1-2%, so
/// sub-percent equality is not the bar. 2% is roughly 2.4 BPM at 120.
const BPM_TOLERANCE: f64 = 0.02;

fn main() {
    let mut args = std::env::args().skip(1);
    let cache_dir = PathBuf::from(
        args.next()
            .unwrap_or_else(|| usage("missing <audio_cache_dir>")),
    );
    let limit: usize = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/essentia_ground_truth.json");
    let raw = std::fs::read_to_string(&fixture_path).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {e}", fixture_path.display());
        eprintln!("regenerate it with: node vapor-core/tools/extract-fixtures.mjs");
        std::process::exit(1);
    });
    let fixtures: Vec<Fixture> = serde_json::from_str(&raw).expect("fixture json");

    let n = fixtures.len().min(limit);
    println!("Validating {n} tracks against Essentia ground truth\n");

    let mut bpm_ok = 0usize;
    let mut bpm_metrical = 0usize;
    let mut key_exact = 0usize;
    let mut key_adjacent = 0usize;
    let mut beat_f: Vec<f32> = Vec::new();
    let mut beat_good = 0usize;
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut by_ext: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    let start = Instant::now();

    for f in fixtures.iter().take(n) {
        let path = cache_dir.join(&f.file);
        let entry = by_ext.entry(f.ext.clone()).or_insert((0, 0));
        entry.1 += 1;

        let a = match vapor_dsp::analyze_file(&path) {
            Ok(a) => a,
            Err(e) => {
                failed.push((f.href.clone(), e.to_string()));
                continue;
            }
        };
        entry.0 += 1;

        // Tempo, with metrical errors counted separately: a half/double/three-
        // quarter result still yields a periodic grid, but it is not agreement.
        let ratio = a.bpm as f64 / f.bpm;
        if (ratio - 1.0).abs() <= BPM_TOLERANCE {
            bpm_ok += 1;
        } else if METRICAL_RATIOS
            .iter()
            .any(|m| (ratio / m - 1.0).abs() <= BPM_TOLERANCE)
        {
            bpm_metrical += 1;
        }

        // Beat grid. This is the measure that actually matters for mixing: the
        // engine phase-locks to a beat position, not to a BPM.
        if !f.beat_grid.is_empty() && !a.beats.is_empty() {
            let score = vapor_dsp::beats::f_measure(&a.beats, &f.beat_grid, BEAT_TOLERANCE);
            beat_f.push(score);
            if score >= 0.8 {
                beat_good += 1;
            }
        }

        if a.camelot == f.camelot {
            key_exact += 1;
        } else if vapor_dsp::key::camelot_distance(&a.camelot, &f.camelot) == Some(1) {
            key_adjacent += 1;
        }
    }

    let elapsed = start.elapsed();
    let decoded: usize = by_ext.values().map(|(ok, _)| ok).sum();
    let pct = |x: usize| 100.0 * x as f64 / decoded.max(1) as f64;

    println!("=== Decode ===");
    for (ext, (ok, total)) in &by_ext {
        println!("  .{ext:<5} {ok:>4}/{total:<4} decoded");
    }
    if !failed.is_empty() {
        println!("  {} failures:", failed.len());
        for (href, err) in failed.iter().take(10) {
            println!("    {err}  <-  {href}");
        }
    }

    println!(
        "\n=== Tempo (vs Essentia, +/-{:.0}%) ===",
        BPM_TOLERANCE * 100.0
    );
    println!(
        "  exact agreement   {bpm_ok:>4}/{decoded}  ({:.1}%)",
        pct(bpm_ok)
    );
    println!(
        "  metrical error    {bpm_metrical:>4}/{decoded}  ({:.1}%)   (1/2, 2, 1/3, 3, 2/3, 3/2, 3/4, 4/3)",
        pct(bpm_metrical)
    );
    println!(
        "  periodic (either) {:>4}/{decoded}  ({:.1}%)",
        bpm_ok + bpm_metrical,
        pct(bpm_ok + bpm_metrical)
    );

    println!(
        "\n=== Beat grid (F-measure vs Essentia, +/-{:.0} ms) ===",
        BEAT_TOLERANCE * 1000.0
    );
    if beat_f.is_empty() {
        println!("  no comparable grids");
    } else {
        let mean: f32 = beat_f.iter().sum::<f32>() / beat_f.len() as f32;
        let mut sorted = beat_f.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        println!("  tracks compared   {:>4}", beat_f.len());
        println!("  mean F-measure    {mean:>7.3}");
        println!("  median F-measure  {median:>7.3}");
        println!(
            "  F >= 0.8          {beat_good:>4}/{}  ({:.1}%)",
            beat_f.len(),
            100.0 * beat_good as f64 / beat_f.len() as f64
        );
    }

    println!("\n=== Key (vs Essentia) ===");
    println!(
        "  exact Camelot     {key_exact:>4}/{decoded}  ({:.1}%)",
        pct(key_exact)
    );
    println!(
        "  adjacent on wheel {key_adjacent:>4}/{decoded}  ({:.1}%)",
        pct(key_adjacent)
    );
    println!(
        "  compatible (either) {:>2}/{decoded}  ({:.1}%)",
        key_exact + key_adjacent,
        pct(key_exact + key_adjacent)
    );

    println!(
        "\n{:.1}s total, {:.2}s/track",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / n.max(1) as f64
    );
}

fn usage(msg: &str) -> ! {
    eprintln!("error: {msg}");
    eprintln!("usage: validate <audio_cache_dir> [limit]");
    std::process::exit(1);
}

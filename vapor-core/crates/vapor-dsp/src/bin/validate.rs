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
    #[serde(default)]
    downbeats: Vec<f32>,
    #[serde(default)]
    cue_in: f32,
    #[serde(default)]
    cue_out: f32,
    #[serde(default)]
    lufs: f32,
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

/// Find a fixture's audio in the cache, under either naming scheme.
///
/// The fixtures were written when the Godot build named cache files by an MD5
/// of the href; the Rust shell names them `{fnv1a(href):016x}.{ext}`. Without
/// the second lookup this harness finds nothing at all on a current machine and
/// reports a clean sweep of zero, which is the least useful way for a
/// validation tool to fail.
fn locate(cache_dir: &Path, f: &Fixture) -> Option<PathBuf> {
    let by_fixture = cache_dir.join(&f.file);
    if by_fixture.exists() {
        return Some(by_fixture);
    }
    let ext = f
        .href
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("audio");
    let by_href = cache_dir.join(format!("{:016x}.{ext}", fnv1a(&f.href)));
    by_href.exists().then_some(by_href)
}

/// A metrical ratio as musicians would say it.
fn name_ratio(m: f64) -> String {
    for (num, den) in [
        (1, 2),
        (2, 1),
        (1, 3),
        (3, 1),
        (2, 3),
        (3, 2),
        (3, 4),
        (4, 3),
    ] {
        if (m - num as f64 / den as f64).abs() < 1e-6 {
            return format!("{num}:{den}");
        }
    }
    format!("{m:.3}")
}

/// The readable tail of an href, for a report a person has to scan.
fn short(href: &str) -> String {
    let tail = href.rsplit('/').next().unwrap_or(href);
    let decoded = tail.replace("%20", " ");
    decoded.chars().take(58).collect()
}

/// FNV-1a, matching the shell's `cache::hash`.
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

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
    let mut down_f: Vec<f32> = Vec::new();
    let mut down_found = 0usize;
    let mut down_eligible = 0usize;
    let mut bars: BTreeMap<usize, usize> = BTreeMap::new();
    let mut contrast_vs_f: Vec<(f32, f32)> = Vec::new();
    let mut best_shift: BTreeMap<usize, usize> = BTreeMap::new();
    let mut cue_in_err: Vec<f32> = Vec::new();
    let mut cue_out_err: Vec<f32> = Vec::new();
    let mut lufs_err: Vec<f32> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut metrical: Vec<(String, f64, f64, f64)> = Vec::new();
    let mut outright: Vec<(String, f64, f64)> = Vec::new();
    let mut by_ext: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    let start = Instant::now();

    for f in fixtures.iter().take(n) {
        let Some(path) = locate(&cache_dir, f) else {
            continue;
        };
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
        } else if let Some(m) = METRICAL_RATIOS
            .iter()
            .find(|m| (ratio / *m - 1.0).abs() <= BPM_TOLERANCE)
        {
            bpm_metrical += 1;
            // Named individually, because "13.6% metrical" is a number you
            // cannot act on and "four of them are 3:4" is one you can.
            metrical.push((f.href.clone(), f.bpm, a.bpm as f64, *m));
        } else {
            outright.push((f.href.clone(), f.bpm, a.bpm as f64));
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

        // Downbeats, scored only where the beat grids already agree.
        //
        // Not a convenience: where the two disagree about the beat they are not
        // describing the same pulse, so a downbeat comparison measures nothing
        // — Essentia's bar of four of *its* beats and ours of four of ours are
        // different lengths. Restricting to agreed grids is what makes the
        // number mean "did we find beat one" rather than "did we find the same
        // tempo", which is already reported above.
        //
        // Called directly rather than read off `Analysis`, because `metre` is
        // measured here and deliberately does not reach the app — see its
        // module docs for the numbers this prints and why.
        if !f.beat_grid.is_empty()
            && !a.beats.is_empty()
            && vapor_dsp::beats::f_measure(&a.beats, &f.beat_grid, BEAT_TOLERANCE) >= 0.8
            && f.downbeats.len() > 4
        {
            down_eligible += 1;
            // Decoded a second time: this tool measures, it does not need to
            // be quick, and threading the samples out of `analyze_file` to save
            // a decode would change the thing being validated.
            let metre = vapor_dsp::decode::decode_to_mono(&path)
                .ok()
                .and_then(|audio| {
                    let spec = vapor_dsp::spectrum::for_tempo(&audio.samples, audio.sample_rate);
                    vapor_dsp::metre::detect(&spec, &a.beats)
                });
            if let Some(m) = metre {
                down_found += 1;
                let score = vapor_dsp::beats::f_measure(&m.downbeats, &f.downbeats, BEAT_TOLERANCE);
                down_f.push(score);
                *bars.entry(m.beats_per_bar).or_default() += 1;
                contrast_vs_f.push((m.contrast, score));

                // Which phase *would* have been right. Spread evenly across the
                // bar means the detector is guessing; a consistent non-zero
                // offset would instead be a convention mismatch and a one-line
                // fix, which is worth telling apart before concluding.
                let mut best = (0usize, -1.0f32);
                for shift in 0..m.beats_per_bar.max(1) {
                    let shifted: Vec<f32> = a
                        .beats
                        .iter()
                        .skip(shift)
                        .step_by(m.beats_per_bar.max(1))
                        .copied()
                        .collect();
                    let s = vapor_dsp::beats::f_measure(&shifted, &f.downbeats, BEAT_TOLERANCE);
                    if s > best.1 {
                        best = (shift, s);
                    }
                }
                *best_shift.entry(best.0).or_default() += 1;
            }
        }

        // Cue points and loudness (MIG-005). Ground truth comes from the same
        // Essentia-era run, and the C++ that produced it was already portable
        // code — so these should agree closely, not merely correlate.
        cue_in_err.push((a.cue_in - f.cue_in).abs());
        cue_out_err.push((a.cue_out - f.cue_out).abs());
        if f.lufs < 0.0 && f.lufs > -70.0 {
            lufs_err.push((a.lufs - f.lufs).abs());
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

    if !metrical.is_empty() {
        // Grouped by relation: which *kind* of metrical error dominates is the
        // thing that decides whether bar-level detection could fix it.
        let mut by_ratio: BTreeMap<String, usize> = BTreeMap::new();
        for (_, _, _, m) in &metrical {
            *by_ratio.entry(name_ratio(*m)).or_default() += 1;
        }
        let summary: Vec<String> = by_ratio.iter().map(|(k, v)| format!("{v}x {k}")).collect();
        println!("  by relation:      {}", summary.join(", "));
        for (href, truth, ours, m) in &metrical {
            println!(
                "    {:>7.1} -> {:>7.1}  ({})  {}",
                truth,
                ours,
                name_ratio(*m),
                short(href)
            );
        }
    }
    if !outright.is_empty() {
        println!("  outright wrong    {:>4}:", outright.len());
        for (href, truth, ours) in &outright {
            println!(
                "    {truth:>7.1} -> {ours:>7.1}  (ratio {:.3})  {}",
                ours / truth,
                short(href)
            );
        }
    }

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

    println!("\n=== Downbeats (only where the beat grids agree, F >= 0.8) ===");
    if down_eligible == 0 {
        println!("  no tracks with an agreed beat grid and reference downbeats");
    } else {
        println!(
            "  eligible tracks   {down_eligible:>4}   (of {} compared)",
            beat_f.len()
        );
        println!(
            "  metre found       {down_found:>4}/{down_eligible}  ({:.1}%)                the rest report no bar rather than guessing",
            100.0 * down_found as f64 / down_eligible as f64
        );
        for (b, n) in &bars {
            println!("    {b} beats to a bar: {n}");
        }
        if !down_f.is_empty() {
            let mean: f32 = down_f.iter().sum::<f32>() / down_f.len() as f32;
            let mut sorted = down_f.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let good = down_f.iter().filter(|&&s| s >= 0.8).count();
            println!("  mean F-measure    {mean:>7.3}");
            println!("  median F-measure  {:>7.3}", sorted[sorted.len() / 2]);
            println!(
                "  F >= 0.8          {good:>4}/{}  ({:.1}%)",
                down_f.len(),
                100.0 * good as f64 / down_f.len() as f64
            );
        }
    }

    if !best_shift.is_empty() {
        println!("  which phase would have been right:");
        for (shift, n) in &best_shift {
            println!("    beat {shift} of the bar: {n}");
        }
    }
    if !contrast_vs_f.is_empty() {
        let mut v = contrast_vs_f.clone();
        v.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("  contrast vs correctness, strongest first:");
        for (c, f) in v.iter() {
            println!("    contrast {c:>6.3}   F {f:>6.3}");
        }
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

    println!("\n=== Cue points and loudness (vs Essentia) ===");
    let summarise = |label: &str, v: &[f32], unit: &str, tol: f32| {
        if v.is_empty() {
            println!("  {label}: no data");
            return;
        }
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean: f32 = s.iter().sum::<f32>() / s.len() as f32;
        let within = s.iter().filter(|&&e| e <= tol).count();
        println!(
            "  {label:<10} mean |err| {mean:>7.3}{unit}  median {:>7.3}{unit}  within {tol}{unit}: {}/{} ({:.1}%)",
            s[s.len() / 2],
            within,
            s.len(),
            100.0 * within as f64 / s.len() as f64
        );
    };
    summarise("cue_in", &cue_in_err, "s", 0.5);
    summarise("cue_out", &cue_out_err, "s", 0.5);
    summarise("lufs", &lufs_err, "LU", 1.0);

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

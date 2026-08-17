//! Why the tempo estimator picked what it picked.
//!
//! `validate` says *that* a track came out at a metrically related tempo.
//! This says *why*, which is a different question with two possible answers
//! and only one of them is a bug:
//!
//! * the comb filter genuinely preferred the wrong period — the evidence is
//!   ambiguous, and no amount of re-weighting fixes it honestly; or
//! * the comb filter preferred the right one and [`octave_prior`] overruled it
//!   — a rule beating the evidence, which is a bug and is fixable.
//!
//! Usage:
//!   cargo run --release --bin metre-probe -- <audio_cache_dir> [limit]

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    href: String,
    file: String,
    bpm: f64,
}

const BPM_TOLERANCE: f64 = 0.02;

/// The relations `validate` counts as metrical rather than outright wrong.
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

fn main() {
    let mut args = std::env::args().skip(1);
    let cache_dir = PathBuf::from(args.next().expect("usage: metre-probe <audio_cache_dir>"));
    let limit: usize = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let raw = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/essentia_ground_truth.json"),
    )
    .expect("fixtures");
    let fixtures: Vec<Fixture> = serde_json::from_str(&raw).expect("fixture json");

    println!(
        "{:>8} {:>8}  {:>6}  {:>7} {:>7}  {:>7} {:>7}   track",
        "truth", "ours", "verdict", "comb@us", "comb@gt", "wtd@us", "wtd@gt"
    );

    let (mut prior_flipped, mut evidence_wrong, mut agreed) = (0, 0, 0);

    for f in fixtures.iter().take(limit) {
        let Some(path) = locate(&cache_dir, f) else {
            continue;
        };
        let Ok(audio) = vapor_dsp::decode::decode_to_mono(&path) else {
            continue;
        };
        let spec = vapor_dsp::spectrum::for_tempo(&audio.samples, audio.sample_rate);
        let Some(result) = vapor_dsp::tempo::estimate_windowed(
            &spec,
            vapor_dsp::TEMPO_SKIP_SECS as f32,
            vapor_dsp::TEMPO_WINDOW_SECS as f32,
        ) else {
            continue;
        };

        let ours = result.bpm as f64;
        let ratio = ours / f.bpm;
        if (ratio - 1.0).abs() <= BPM_TOLERANCE {
            agreed += 1;
            continue;
        }

        // Score both answers on the same grid the estimator ranks.
        let all = vapor_dsp::tempo::candidates(&result.odf, result.odf_rate);
        let nearest = |target: f64| {
            all.iter()
                .min_by(|a, b| {
                    (a.bpm as f64 - target)
                        .abs()
                        .total_cmp(&(b.bpm as f64 - target).abs())
                })
                .copied()
                .expect("a non-empty grid")
        };
        let us = nearest(ours);
        let gt = nearest(f.bpm);

        // The distinction the whole tool exists for.
        let verdict = if gt.comb > us.comb {
            prior_flipped += 1;
            "PRIOR"
        } else {
            evidence_wrong += 1;
            "comb"
        };

        println!(
            "{:>8.1} {:>8.1}  {:>6}  {:>7.4} {:>7.4}  {:>7.4} {:>7.4}   {}",
            f.bpm,
            ours,
            verdict,
            us.comb,
            gt.comb,
            us.weighted(),
            gt.weighted(),
            short(&f.href)
        );
    }

    println!("\n{agreed} agreed with Essentia");
    println!(
        "{prior_flipped} where the comb preferred Essentia's answer and the prior overrode it"
    );
    println!("{evidence_wrong} where the comb itself preferred ours");

    // The margins above are small. Small compared to *what* is the question
    // that decides whether any rule could separate them, so measure it: on the
    // tracks that come out right, how far ahead of the nearest metrically
    // related alternative is the winner? If the right answers win by the same
    // slim margins the wrong ones lose by, the margin carries no information
    // and a threshold tuned to flip six tracks is fitting noise.
    println!("\n=== Margin between the true tempo and its nearest metrical rival ===");
    let mut right_margins: Vec<f64> = Vec::new();
    let mut wrong_margins: Vec<f64> = Vec::new();

    for f in fixtures.iter().take(limit) {
        let Some(path) = locate(&cache_dir, f) else {
            continue;
        };
        let Ok(audio) = vapor_dsp::decode::decode_to_mono(&path) else {
            continue;
        };
        let spec = vapor_dsp::spectrum::for_tempo(&audio.samples, audio.sample_rate);
        let Some(result) = vapor_dsp::tempo::estimate_windowed(
            &spec,
            vapor_dsp::TEMPO_SKIP_SECS as f32,
            vapor_dsp::TEMPO_WINDOW_SECS as f32,
        ) else {
            continue;
        };
        let all = vapor_dsp::tempo::candidates(&result.odf, result.odf_rate);
        let nearest = |target: f64| {
            all.iter()
                .min_by(|a, b| {
                    (a.bpm as f64 - target)
                        .abs()
                        .total_cmp(&(b.bpm as f64 - target).abs())
                })
                .copied()
                .expect("a non-empty grid")
        };

        let truth = nearest(f.bpm);
        // The rival has to be the *actual competing peak*, not the arithmetic
        // ideal. The comb score is spiky in BPM and a real 2:3 relative sits a
        // few grid steps off exactly two-thirds — for one track, 116.5 against
        // an ideal 114.9, which scored 0.0046 and 0.0021. Taking the ideal
        // understated the rival by half and would have turned this whole
        // measurement into an artefact of where the grid points fell.
        let rival = all
            .iter()
            .filter(|c| {
                let r = c.bpm as f64 / f.bpm;
                METRICAL_RATIOS
                    .iter()
                    .any(|m| (r / m - 1.0).abs() <= BPM_TOLERANCE)
            })
            .max_by(|a, b| a.comb.total_cmp(&b.comb))
            .copied();
        let Some(rival) = rival else { continue };
        if truth.comb <= 0.0 {
            continue;
        }

        let margin = (truth.comb - rival.comb) as f64 / truth.comb as f64;
        let correct = ((result.bpm as f64) / f.bpm - 1.0).abs() <= BPM_TOLERANCE;
        if correct {
            right_margins.push(margin);
        } else {
            wrong_margins.push(margin);
            println!(
                "    wrong: margin {:>7.3}  truth {:>6.1} (comb {:.4})  rival {:>6.1} (comb {:.4})  {}",
                margin,
                f.bpm,
                truth.comb,
                rival.bpm,
                rival.comb,
                short(&f.href)
            );
        }
    }

    summarise("tracks we get right", &mut right_margins);
    summarise("tracks we get wrong", &mut wrong_margins);
    println!(
        "\nWhere these two ranges overlap, no re-weighting of the prior separates them:\n\
         the true tempo does not reliably win by a bigger margin on the tracks it wins."
    );

    // The one hypothesis the broadband comb cannot already be testing.
    //
    // `onset_detection_function` runs over 30 Hz - 5 kHz and whitens, so the
    // kick drum is one voice among many in it. In 4/4 dance music the kick is
    // the bar-level signal: it lands on the downbeat and repeats per bar. A
    // *bass-band* onset function is therefore evidence the current estimator
    // has never seen, rather than a re-weighting of evidence it already has —
    // which is what the two reverted attempts were.
    //
    // If it separates the failures, bar-level detection is worth building on
    // it. If it does not, it is not, and no amount of tuning changes that.
    println!("\n=== Does a bass-band onset function separate them? (30-120 Hz) ===");
    let mut bass_right: Vec<f64> = Vec::new();
    let mut bass_wrong: Vec<f64> = Vec::new();

    for f in fixtures.iter().take(limit) {
        let Some(path) = locate(&cache_dir, f) else {
            continue;
        };
        let Ok(audio) = vapor_dsp::decode::decode_to_mono(&path) else {
            continue;
        };
        let spec = vapor_dsp::spectrum::for_tempo(&audio.samples, audio.sample_rate);
        let Some(result) = vapor_dsp::tempo::estimate_windowed(
            &spec,
            vapor_dsp::TEMPO_SKIP_SECS as f32,
            vapor_dsp::TEMPO_WINDOW_SECS as f32,
        ) else {
            continue;
        };

        let bass = bass_odf(&spec);
        let rate = spec.frame_rate();
        let truth_score = comb(&bass, rate, f.bpm as f32);
        let rival_bpm = best_metrical_rival(&bass, rate, f.bpm);
        let rival_score = comb(&bass, rate, rival_bpm);
        if truth_score <= 0.0 {
            continue;
        }
        let margin = (truth_score - rival_score) as f64 / truth_score as f64;

        if ((result.bpm as f64) / f.bpm - 1.0).abs() <= BPM_TOLERANCE {
            bass_right.push(margin);
        } else {
            bass_wrong.push(margin);
            println!(
                "    wrong: bass margin {:>7.3}  truth {:>6.1}  rival {:>6.1}   {}",
                margin,
                f.bpm,
                rival_bpm,
                short(&f.href)
            );
        }
    }
    summarise("bass, tracks right", &mut bass_right);
    summarise("bass, tracks wrong", &mut bass_wrong);
}

/// Spectral flux restricted to the kick's band, whitened the same way.
fn bass_odf(spec: &vapor_dsp::spectrum::Spectrogram) -> Vec<f32> {
    let bins = spec.frames[0].len();
    let bin_of = |hz: f32| ((hz / spec.bin_freq(1)).round() as usize).clamp(1, bins - 1);
    let (lo, hi) = (bin_of(30.0), bin_of(120.0).max(bin_of(30.0) + 1));

    let mut odf = Vec::with_capacity(spec.frames.len());
    odf.push(0.0);
    for i in 1..spec.frames.len() {
        let (prev, cur) = (&spec.frames[i - 1], &spec.frames[i]);
        let mut flux = 0.0f32;
        for k in lo..hi {
            let d = (1.0 + cur[k]).ln() - (1.0 + prev[k]).ln();
            if d > 0.0 {
                flux += d;
            }
        }
        odf.push(flux);
    }

    // Same local-mean subtraction the real ODF uses, so the two are comparable.
    const RADIUS: usize = 12;
    let orig = odf.clone();
    for i in 0..odf.len() {
        let lo = i.saturating_sub(RADIUS);
        let hi = (i + RADIUS + 1).min(orig.len());
        let mean: f32 = orig[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        odf[i] = (orig[i] - mean).max(0.0);
    }
    let max = odf.iter().cloned().fold(0.0f32, f32::max);
    if max > 0.0 {
        for v in odf.iter_mut() {
            *v /= max;
        }
    }
    odf
}

/// The comb score `tempo` uses, over an arbitrary onset function.
fn comb(odf: &[f32], rate: f32, bpm: f32) -> f32 {
    let period = 60.0 / bpm * rate;
    let (mut score, mut total) = (0.0f32, 0.0f32);
    for m in 1..=4 {
        let lag = (period * m as f32).round() as usize;
        if lag >= odf.len() {
            break;
        }
        let n = odf.len() - lag;
        let acc: f32 = (0..n).map(|i| odf[i] * odf[i + lag]).sum();
        let w = 1.0 / m as f32;
        score += w * (acc / n as f32);
        total += w;
    }
    if total > 0.0 {
        score / total
    } else {
        0.0
    }
}

fn best_metrical_rival(odf: &[f32], rate: f32, truth: f64) -> f32 {
    METRICAL_RATIOS
        .iter()
        .map(|m| (truth * m) as f32)
        .filter(|b| (60.0..=200.0).contains(b))
        .max_by(|a, b| comb(odf, rate, *a).total_cmp(&comb(odf, rate, *b)))
        .unwrap_or(truth as f32)
}

fn summarise(label: &str, v: &mut [f64]) {
    if v.is_empty() {
        println!("  {label}: no data");
        return;
    }
    v.sort_by(f64::total_cmp);
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    println!(
        "  {label:<22} n={:<3} min {:>7.3}  median {:>7.3}  mean {:>7.3}  max {:>7.3}",
        v.len(),
        v[0],
        v[v.len() / 2],
        mean,
        v[v.len() - 1]
    );
}

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

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn short(href: &str) -> String {
    let tail = href.rsplit('/').next().unwrap_or(href);
    tail.replace("%20", " ").chars().take(46).collect()
}

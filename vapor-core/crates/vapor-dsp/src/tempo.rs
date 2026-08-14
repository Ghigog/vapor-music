//! Tempo estimation — the replacement for Essentia's `RhythmExtractor2013`.
//!
//! This is the single highest-risk component of the whole migration: everything
//! else in the DSP stack has an off-the-shelf pure-Rust answer, and beat
//! tracking does not. It is validated against 563 real Essentia results rather
//! than against taste, because "sounds about right" is exactly how a beat
//! tracker ships broken.
//!
//! Pipeline: spectral flux onset detection function -> adaptive-mean whitening
//! -> autocorrelation over the plausible tempo range -> harmonic-aware peak
//! picking.
//!
//! Octave errors (reporting half or double the true tempo) are the dominant
//! failure mode in every beat tracker, so they are handled explicitly in
//! `pick_tempo` rather than left to the raw autocorrelation peak.

use crate::spectrum::Spectrogram;

/// Tempo search range. The real library spans 69.5–184.6 BPM; this is widened
/// to the conventional range so the estimator is not tuned to one collection.
const MIN_BPM: f32 = 60.0;
const MAX_BPM: f32 = 200.0;

/// Preferred perceptual tempo centre, used only to break octave ambiguity.
/// Humans tap around 120 BPM, which is what makes 2x/0.5x errors resolvable.
const TEMPO_CENTRE: f32 = 120.0;
const TEMPO_SIGMA: f32 = 0.55;

pub struct TempoResult {
    pub bpm: f32,
    /// Onset detection function, retained for beat-grid construction.
    pub odf: Vec<f32>,
    pub odf_rate: f32,
}

pub fn estimate(spec: &Spectrogram) -> Option<TempoResult> {
    if spec.frames.len() < 32 {
        return None;
    }

    let odf = onset_detection_function(spec);
    let odf_rate = spec.frame_rate();
    let bpm = pick_tempo(&odf, odf_rate)?;

    Some(TempoResult {
        bpm,
        odf,
        odf_rate,
    })
}

/// Spectral flux: summed positive frame-to-frame change in log magnitude.
///
/// Log rather than linear magnitude because percussive transients in loud,
/// heavily compressed masters (most of this library) otherwise swamp everything
/// else and the ODF degenerates to a handful of spikes.
fn onset_detection_function(spec: &Spectrogram) -> Vec<f32> {
    let n = spec.frames.len();
    let bins = spec.frames[0].len();

    // Ignore the very lowest bins: rumble and DC contribute no onset
    // information but do contribute drift.
    let lo = ((30.0 / spec.bin_freq(1)).ceil() as usize).max(1);
    let hi = bins.min(((5000.0 / spec.bin_freq(1)).ceil() as usize).max(lo + 1));

    let mut odf = Vec::with_capacity(n);
    odf.push(0.0);

    for i in 1..n {
        let prev = &spec.frames[i - 1];
        let cur = &spec.frames[i];
        let mut flux = 0.0f32;
        for k in lo..hi {
            let d = (1.0 + cur[k]).ln() - (1.0 + prev[k]).ln();
            if d > 0.0 {
                flux += d;
            }
        }
        odf.push(flux);
    }

    whiten(&mut odf);
    odf
}

/// Subtract a local moving mean and half-wave rectify.
///
/// Without this, a track with a loud section and a quiet section produces an
/// ODF dominated by the loud part, and the autocorrelation locks onto the
/// structure of the arrangement rather than the beat.
fn whiten(odf: &mut [f32]) {
    const RADIUS: usize = 12;
    let n = odf.len();
    let orig = odf.to_vec();

    for i in 0..n {
        let lo = i.saturating_sub(RADIUS);
        let hi = (i + RADIUS + 1).min(n);
        let mean: f32 = orig[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        odf[i] = (orig[i] - mean).max(0.0);
    }

    let max = odf.iter().cloned().fold(0.0f32, f32::max);
    if max > 0.0 {
        for v in odf.iter_mut() {
            *v /= max;
        }
    }
}

/// Unbiased autocorrelation of the ODF at a given lag.
fn autocorr_at(odf: &[f32], lag: usize) -> f32 {
    if lag == 0 || lag >= odf.len() {
        return 0.0;
    }
    let n = odf.len() - lag;
    let mut acc = 0.0f32;
    for i in 0..n {
        acc += odf[i] * odf[i + lag];
    }
    // Normalise by overlap length, otherwise long lags are systematically
    // penalised and the estimator biases toward fast tempi.
    acc / n as f32
}

/// Score a candidate tempo by summing autocorrelation at its beat period and
/// the first few multiples — a comb filter. A true tempo reinforces at 1x, 2x,
/// 3x and 4x the beat period; a spurious peak generally does not.
fn comb_score(odf: &[f32], odf_rate: f32, bpm: f32) -> f32 {
    let period = 60.0 / bpm * odf_rate;
    let mut score = 0.0;
    let mut weight_total = 0.0;
    for m in 1..=4 {
        let lag = (period * m as f32).round() as usize;
        if lag >= odf.len() {
            break;
        }
        // Later multiples are weaker evidence; weight them down.
        let w = 1.0 / m as f32;
        score += w * autocorr_at(odf, lag);
        weight_total += w;
    }
    if weight_total > 0.0 {
        score / weight_total
    } else {
        0.0
    }
}

/// Log-normal preference for tempi near the perceptual centre.
///
/// Applied only to separate octave-related candidates that the comb filter
/// scores almost identically — it must never be strong enough to override
/// genuine evidence, or every drum-and-bass track collapses to half tempo.
fn octave_prior(bpm: f32) -> f32 {
    let z = (bpm / TEMPO_CENTRE).ln() / TEMPO_SIGMA;
    (-0.5 * z * z).exp()
}

fn pick_tempo(odf: &[f32], odf_rate: f32) -> Option<f32> {
    // Search on a log-spaced grid: tempo perception is multiplicative, and a
    // linear grid wastes resolution at the top of the range.
    let steps = 400;
    let ln_lo = MIN_BPM.ln();
    let ln_hi = MAX_BPM.ln();

    let mut best: Option<(f32, f32)> = None;
    let mut scored: Vec<(f32, f32)> = Vec::with_capacity(steps);

    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let bpm = (ln_lo + t * (ln_hi - ln_lo)).exp();
        let raw = comb_score(odf, odf_rate, bpm);
        scored.push((bpm, raw));
        let weighted = raw * octave_prior(bpm);
        if best.is_none_or(|(_, b)| weighted > b) {
            best = Some((bpm, weighted));
        }
    }

    let (coarse_bpm, _) = best?;

    // Refine locally on a fine linear grid around the winner. The log grid is
    // ~0.3% apart near 120 BPM, which is already close, but the fixtures carry
    // fractional BPM (86.87…) and the tolerance test is proportional.
    let mut refined = coarse_bpm;
    let mut refined_score = comb_score(odf, odf_rate, coarse_bpm);
    let span = coarse_bpm * 0.03;
    for i in 0..=60 {
        let bpm = coarse_bpm - span + (2.0 * span) * i as f32 / 60.0;
        if !(MIN_BPM..=MAX_BPM).contains(&bpm) {
            continue;
        }
        let s = comb_score(odf, odf_rate, bpm);
        if s > refined_score {
            refined_score = s;
            refined = bpm;
        }
    }

    Some(refined)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic click track is the one case where the answer is known
    /// exactly, independent of any reference implementation.
    fn click_track(bpm: f32, secs: f32, rate: u32) -> Vec<f32> {
        let n = (secs * rate as f32) as usize;
        let mut s = vec![0.0f32; n];
        let period = (60.0 / bpm * rate as f32) as usize;
        let mut i = 0;
        while i < n {
            // Short decaying burst of noise-free tone as a transient.
            for k in 0..(rate as usize / 100).min(n - i) {
                let t = k as f32 / rate as f32;
                s[i + k] = (1.0 - t * 100.0).max(0.0)
                    * (2.0 * std::f32::consts::PI * 1000.0 * t).sin();
            }
            i += period;
        }
        s
    }

    #[test]
    fn detects_synthetic_click_tempo() {
        for &target in &[90.0f32, 120.0, 140.0] {
            let s = click_track(target, 30.0, 44100);
            let spec = crate::spectrum::for_tempo(&s, 44100);
            let r = estimate(&spec).expect("tempo");
            let err = (r.bpm - target).abs() / target;
            assert!(
                err < 0.02,
                "click track {target} BPM -> estimated {:.2} ({:.1}% off)",
                r.bpm,
                err * 100.0
            );
        }
    }

    #[test]
    fn octave_prior_peaks_at_centre() {
        assert!(octave_prior(120.0) > octave_prior(60.0));
        assert!(octave_prior(120.0) > octave_prior(240.0));
    }
}

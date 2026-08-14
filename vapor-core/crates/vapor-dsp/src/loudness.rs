//! Cue points and loudness — the portable half of `audio_dsp.cpp` (MIG-005).
//!
//! `_analyze_samples_impl` in the C++ extension was already pure, portable code
//! with no Essentia calls: it computed cue in/out and integrated loudness from
//! a plain sample buffer. Only the *loaders* around it required ffmpeg. Porting
//! it is therefore a straight translation rather than a reimplementation, and
//! the results can be diffed against the Essentia-era values already stored for
//! every track.
//!
//! These are not cosmetic. `get_transition_trigger_time` in `audio_manager.gd`
//! schedules transitions from `cue_in` and the outro, so a wrong cue point
//! misplaces every mix.

/// Windows of 10 ms are used for silence detection, matching the C++.
const CUE_WINDOW_SECS: f64 = 0.01;
/// RMS below this counts as silence — about −40 dB.
const SILENCE_THRESHOLD: f64 = 0.01;

/// First and last non-silent moments, in seconds.
///
/// `cue_out` is the end of the last non-silent window, so it is the point after
/// which nothing audible remains — not the file duration.
pub fn cue_points(samples: &[f32], sample_rate: f32) -> (f32, f32) {
    let duration = samples.len() as f64 / sample_rate as f64;
    if samples.is_empty() || sample_rate <= 0.0 {
        return (0.0, 0.0);
    }

    let window = ((CUE_WINDOW_SECS * sample_rate as f64).round() as usize).max(1);
    let windows = samples.len() / window;
    if windows == 0 {
        return (0.0, duration as f32);
    }

    let rms_at = |i: usize| -> f64 {
        let start = i * window;
        let sum: f64 = samples[start..start + window]
            .iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum();
        (sum / window as f64).sqrt()
    };

    let mut cue_in = 0.0f64;
    for i in 0..windows {
        if rms_at(i) > SILENCE_THRESHOLD {
            cue_in = (i * window) as f64 / sample_rate as f64;
            break;
        }
    }

    let mut cue_out = duration;
    for i in (0..windows).rev() {
        if rms_at(i) > SILENCE_THRESHOLD {
            cue_out = ((i + 1) * window) as f64 / sample_rate as f64;
            break;
        }
    }

    (cue_in as f32, cue_out as f32)
}

/// Integrated loudness in LUFS, per EBU R128 / ITU-R BS.1770.
///
/// Mono: the signal has already been downmixed, so the channel weighting
/// reduces to unity and the constant is the standard −0.691.
///
/// Returns −70.0 for silence, which is the standard's absolute gate.
pub fn integrated_lufs(samples: &[f32], sample_rate: f32) -> f32 {
    if samples.is_empty() || sample_rate <= 0.0 {
        return -70.0;
    }

    let filtered = k_weight(samples, sample_rate as f64);

    // 400 ms blocks with 75% overlap, i.e. a 100 ms step.
    let n400 = (0.4 * sample_rate as f64).round() as usize;
    let n100 = (0.1 * sample_rate as f64).round() as usize;

    let mut block_energies: Vec<f64> = Vec::new();
    if filtered.len() < n400 {
        // Shorter than one block: treat the whole signal as a single block
        // rather than reporting silence.
        let sum: f64 = filtered.iter().map(|v| v * v).sum();
        block_energies.push(sum / filtered.len() as f64);
    } else {
        let mut start = 0usize;
        while start + n400 <= filtered.len() {
            let sum: f64 = filtered[start..start + n400].iter().map(|v| v * v).sum();
            block_energies.push(sum / n400 as f64);
            start += n100.max(1);
        }
    }

    // Absolute gate at −70 LUFS.
    let abs_gate = 10f64.powf((-70.0 + 0.691) / 10.0);
    let gated: Vec<f64> = block_energies
        .into_iter()
        .filter(|&e| e >= abs_gate)
        .collect();
    if gated.is_empty() {
        return -70.0;
    }

    let mean: f64 = gated.iter().sum::<f64>() / gated.len() as f64;

    // Relative gate at 10 dB below the ungated mean. This is what stops a
    // track's quiet passages from dragging the figure down.
    let rel_gate = 0.1 * mean;
    let second: Vec<f64> = gated.into_iter().filter(|&e| e >= rel_gate).collect();

    let integrated = if second.is_empty() {
        mean
    } else {
        second.iter().sum::<f64>() / second.len() as f64
    };

    if integrated > 1e-12 {
        (-0.691 + 10.0 * integrated.log10()) as f32
    } else {
        -70.0
    }
}

/// ITU-R BS.1770 K-weighting: a high-shelf followed by a high-pass.
///
/// Coefficients are the standard's, computed for the actual sample rate rather
/// than hardcoded for 48 kHz — the library is mostly 44.1 kHz.
fn k_weight(samples: &[f32], rate: f64) -> Vec<f64> {
    // Stage 1: high-frequency shelving filter.
    let f0 = 1681.974450955533;
    let g = 3.999843853973347;
    let q = 0.7071752369554196;

    let k = (std::f64::consts::PI * f0 / rate).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;

    let pb = [
        (vh + vb * k / q + k * k) / a0,
        2.0 * (k * k - vh) / a0,
        (vh - vb * k / q + k * k) / a0,
    ];
    let pa = [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0];

    // Stage 2: high-pass filter.
    let f0 = 38.13547087602444;
    let q = 0.5003270373238773;
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let a0_hp = 1.0 + k / q + k * k;
    let ra = [2.0 * (k * k - 1.0) / a0_hp, (1.0 - k / q + k * k) / a0_hp];

    let mut out = Vec::with_capacity(samples.len());
    let (mut x1, mut x2, mut y1, mut y2) = (0.0f64, 0.0, 0.0, 0.0);
    let (mut r1, mut r2, mut s1, mut s2) = (0.0f64, 0.0, 0.0, 0.0);

    for &sample in samples {
        let x = sample as f64;

        let stage1 = pb[0] * x + pb[1] * x1 + pb[2] * x2 - pa[0] * y1 - pa[1] * y2;
        x2 = x1;
        x1 = x;
        y2 = y1;
        y1 = stage1;

        // The high-pass numerator is [1, -2, 1].
        let stage2 = stage1 - 2.0 * r1 + r2 - ra[0] * s1 - ra[1] * s2;
        r2 = r1;
        r1 = stage1;
        s2 = s1;
        s1 = stage2;

        out.push(stage2);
    }
    out
}

/// Peak envelope for waveform display, as `bins` values in `[0, 1]`.
pub fn waveform_peaks(samples: &[f32], bins: usize) -> Vec<f32> {
    if samples.is_empty() || bins == 0 {
        return vec![0.0; bins];
    }
    let per = (samples.len() as f64 / bins as f64).max(1.0);
    (0..bins)
        .map(|i| {
            let start = (i as f64 * per) as usize;
            let end = (((i + 1) as f64 * per) as usize).min(samples.len());
            if start >= end {
                return 0.0;
            }
            samples[start..end]
                .iter()
                .fold(0.0f32, |m, &s| m.max(s.abs()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 44100.0;

    fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
        let n = (secs * RATE) as usize;
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / RATE).sin())
            .collect()
    }

    #[test]
    fn cue_points_find_the_audible_span() {
        let mut s = vec![0.0f32; (RATE * 2.0) as usize];
        s.extend(sine(440.0, 0.5, 3.0));
        s.extend(vec![0.0f32; (RATE * 2.0) as usize]);

        let (cue_in, cue_out) = cue_points(&s, RATE);
        assert!(
            (cue_in - 2.0).abs() < 0.05,
            "cue_in {cue_in:.3} should be ~2.0"
        );
        assert!(
            (cue_out - 5.0).abs() < 0.05,
            "cue_out {cue_out:.3} should be ~5.0"
        );
    }

    #[test]
    fn silence_reports_a_zero_span() {
        let s = vec![0.0f32; (RATE * 3.0) as usize];
        let (cue_in, cue_out) = cue_points(&s, RATE);
        assert_eq!(cue_in, 0.0);
        // Nothing audible: cue_out falls back to the duration.
        assert!((cue_out - 3.0).abs() < 0.05);
    }

    /// Anchor against the standard: a −20 dBFS 1 kHz tone reads about
    /// −20 LUFS, since K-weighting is near unity gain at 1 kHz.
    #[test]
    fn lufs_matches_the_reference_tone() {
        let amp = 10f32.powf(-20.0 / 20.0) * std::f32::consts::SQRT_2;
        let s = sine(1000.0, amp, 5.0);
        let lufs = integrated_lufs(&s, RATE);
        assert!(
            (lufs - -20.0).abs() < 1.0,
            "expected about -20 LUFS, got {lufs:.2}"
        );
    }

    #[test]
    fn louder_material_reads_louder() {
        let quiet = integrated_lufs(&sine(1000.0, 0.05, 5.0), RATE);
        let loud = integrated_lufs(&sine(1000.0, 0.5, 5.0), RATE);
        assert!(loud > quiet + 15.0, "quiet {quiet:.1}, loud {loud:.1}");
    }

    #[test]
    fn silence_is_gated_to_the_floor() {
        let s = vec![0.0f32; (RATE * 3.0) as usize];
        assert_eq!(integrated_lufs(&s, RATE), -70.0);
    }

    /// K-weighting should attenuate deep bass relative to 1 kHz: that is the
    /// whole point of the high-pass stage.
    #[test]
    fn k_weighting_attenuates_sub_bass() {
        let mid = integrated_lufs(&sine(1000.0, 0.5, 5.0), RATE);
        let sub = integrated_lufs(&sine(30.0, 0.5, 5.0), RATE);
        assert!(
            sub < mid - 5.0,
            "30 Hz ({sub:.1}) should read well below 1 kHz ({mid:.1})"
        );
    }

    #[test]
    fn waveform_peaks_track_the_envelope() {
        let mut s = sine(440.0, 0.2, 1.0);
        s.extend(sine(440.0, 0.9, 1.0));
        let peaks = waveform_peaks(&s, 10);
        assert_eq!(peaks.len(), 10);
        assert!(peaks[0] < 0.3, "first half should be quiet: {}", peaks[0]);
        assert!(peaks[9] > 0.7, "second half should be loud: {}", peaks[9]);
    }
}

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

/// Window used for the energy envelope, in seconds. From `audio_dsp.cpp`,
/// which uses a flat 44100 samples — one second at its assumed rate.
const ENERGY_WINDOW_SECS: f32 = 1.0;

/// Perceived energy, 0–1.
///
/// **Ported from `audio_dsp.cpp`**, which computes it as the mean of a
/// one-second RMS envelope divided by that envelope's peak.
///
/// It is a dynamics ratio rather than a loudness: a track that sits near its
/// own peak the whole way through reads as relentless, while one with quiet
/// verses and a loud chorus reads as varied — at identical LUFS.
///
/// That is a defensible measure of something, but it is not intensity, and it
/// drove the set ordering on the claim that it was. Measured on 2026-08-17 it
/// put ballads above drum & bass with the two ranges fully overlapping, and
/// `vapor_library::intensity_from_lufs` replaced it everywhere intensity was
/// actually wanted. What still reads it is the mix's mid cut, via
/// [`has_vocal_presence`].
///
/// Returns 0.5 for a signal too short or too quiet to measure, matching the
/// original's fallback: an unknown energy sits in the middle rather than at an
/// end, where it would drag a curve toward it.
pub fn energy_level(samples: &[f32], sample_rate: f32) -> f32 {
    let window = (sample_rate * ENERGY_WINDOW_SECS) as usize;
    if window == 0 {
        return 0.5;
    }
    let windows = samples.len() / window;
    if windows == 0 {
        return 0.5;
    }

    let mut peak = 0.0f32;
    let mut total = 0.0f32;
    for i in 0..windows {
        let slice = &samples[i * window..(i + 1) * window];
        let sum_squares: f32 = slice.iter().map(|s| s * s).sum();
        let rms = (sum_squares / window as f32).sqrt();
        peak = peak.max(rms);
        total += rms;
    }

    // The original's guard: below this the ratio is noise divided by noise.
    if peak <= 0.001 {
        return 0.5;
    }
    ((total / windows as f32) / peak).clamp(0.0, 1.0)
}

#[cfg(test)]
mod energy_tests {
    use super::*;

    const RATE: f32 = 44_100.0;

    fn tone(secs: f32, amplitude: impl Fn(usize) -> f32) -> Vec<f32> {
        let n = (RATE * secs) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE;
                (t * 220.0 * std::f32::consts::TAU).sin() * amplitude(i)
            })
            .collect()
    }

    /// The distinction the measure exists to make, and the one loudness cannot:
    /// a track that holds its level against one that swings.
    #[test]
    fn a_relentless_track_out_energises_a_dynamic_one() {
        let steady = tone(20.0, |_| 0.5);
        // Same peak, but half the time it is nearly silent.
        let swinging = tone(20.0, |i| {
            if (i / (RATE as usize * 2)).is_multiple_of(2) {
                0.5
            } else {
                0.02
            }
        });

        let a = energy_level(&steady, RATE);
        let b = energy_level(&swinging, RATE);
        assert!(a > 0.9, "a steady track should sit near its own peak: {a}");
        assert!(b < 0.7, "a swinging track should not: {b}");
    }

    /// Loudness cannot tell those two apart, which is why this is not loudness.
    #[test]
    fn energy_is_independent_of_absolute_level() {
        let loud = tone(20.0, |_| 0.8);
        let quiet = tone(20.0, |_| 0.05);
        let (a, b) = (energy_level(&loud, RATE), energy_level(&quiet, RATE));
        assert!(
            (a - b).abs() < 0.05,
            "halving the volume changed the energy: {a} vs {b}"
        );
    }

    /// Unknown sits in the middle rather than at an end, where it would drag a
    /// curve toward it.
    #[test]
    fn too_short_or_too_quiet_reports_the_middle() {
        assert_eq!(energy_level(&[], RATE), 0.5);
        assert_eq!(energy_level(&vec![0.0; 44_100 * 5], RATE), 0.5);
        assert_eq!(
            energy_level(&vec![0.1; 100], RATE),
            0.5,
            "shorter than a window"
        );
    }

    #[test]
    fn energy_is_bounded() {
        for amp in [0.0f32, 0.001, 0.5, 1.0] {
            let e = energy_level(&tone(5.0, |_| amp), RATE);
            assert!((0.0..=1.0).contains(&e), "amp {amp} gave {e}");
        }
    }
}

/// Hop for transient detection, in seconds. From `audio_dsp.cpp`, which uses a
/// flat 2205 samples and labels it 50 ms.
const TRANSIENT_HOP_SECS: f32 = 0.05;
/// A hop counts as a transient above this share of the track's loudest hop.
const TRANSIENT_THRESHOLD: f32 = 0.7;

/// Times of the track's strongest hits, in seconds.
///
/// **Ported from `audio_dsp.cpp`.** RMS in 50 ms hops; a hop is a transient when
/// it is above 70% of the loudest hop *and* louder than both its neighbours.
///
/// A deliberately blunt measure — it finds the handful of biggest moments in a
/// track, not every drum hit. That is what it is for: the Godot build uses these
/// to place structural markers, where a dense onset list would be noise.
pub fn transients(samples: &[f32], sample_rate: f32) -> Vec<f32> {
    let hop = (sample_rate * TRANSIENT_HOP_SECS) as usize;
    if hop == 0 || samples.len() < hop * 3 {
        return Vec::new();
    }

    let hops = samples.len() / hop;
    let mut energies = Vec::with_capacity(hops);
    let mut peak = 0.0f32;
    for i in 0..hops {
        let slice = &samples[i * hop..(i + 1) * hop];
        let rms = (slice.iter().map(|s| s * s).sum::<f32>() / hop as f32).sqrt();
        peak = peak.max(rms);
        energies.push(rms);
    }

    let threshold = peak * TRANSIENT_THRESHOLD;
    (1..energies.len().saturating_sub(1))
        .filter(|&i| {
            energies[i] > threshold
                && energies[i] > energies[i - 1]
                && energies[i] > energies[i + 1]
        })
        .map(|i| (i * hop) as f32 / sample_rate)
        .collect()
}

#[cfg(test)]
mod transient_tests {
    use super::*;

    const RATE: f32 = 44_100.0;

    /// Bursts at known times must come back at those times.
    #[test]
    fn finds_hits_where_they_were_put() {
        let secs = 10.0;
        let n = (RATE * secs) as usize;
        let mut samples = vec![0.02f32; n];
        let hits = [1.0f32, 3.5, 7.25];
        for &at in &hits {
            let start = (at * RATE) as usize;
            for k in 0..(RATE * 0.03) as usize {
                if start + k < n {
                    samples[start + k] = 0.9;
                }
            }
        }

        let found = transients(&samples, RATE);
        assert!(
            !found.is_empty(),
            "found no transients in an obvious signal"
        );
        for &at in &hits {
            assert!(
                found
                    .iter()
                    .any(|t| (t - at).abs() <= TRANSIENT_HOP_SECS * 2.0),
                "missed the hit at {at}s; found {found:?}"
            );
        }
    }

    /// A steady signal has no *relative* peaks, so it must report none rather
    /// than every hop that happens to clear an absolute threshold.
    #[test]
    fn a_steady_signal_has_no_transients() {
        let steady = vec![0.5f32; (RATE * 5.0) as usize];
        assert!(transients(&steady, RATE).is_empty());
    }

    #[test]
    fn silence_and_short_input_are_handled() {
        assert!(transients(&[], RATE).is_empty());
        assert!(transients(&vec![0.0; 100], RATE).is_empty());
        assert!(transients(&vec![0.0; (RATE * 3.0) as usize], RATE).is_empty());
    }

    /// Times must be in order and inside the track, since callers place markers
    /// with them.
    #[test]
    fn times_are_ordered_and_within_the_track() {
        let secs = 20.0;
        let n = (RATE * secs) as usize;
        let samples: Vec<f32> = (0..n)
            .map(|i| if (i / 4410) % 7 == 0 { 0.9 } else { 0.05 })
            .collect();

        let found = transients(&samples, RATE);
        assert!(
            found.windows(2).all(|w| w[1] > w[0]),
            "not ordered: {found:?}"
        );
        assert!(found.iter().all(|&t| t >= 0.0 && t < secs));
    }
}

/// The threshold above which a track counts as having vocals.
///
/// `audio_analyzer.gd:516` is the whole definition:
///
/// ```gdscript
/// results["vocal_presence"] = results.get("energy_level", 0.5) > 0.35
/// ```
///
/// So `vocal_presence` is **not a vocal detector** and never was — it is
/// [`energy_level`] over a threshold, wearing a misleading name. Recorded here
/// because the name invites exactly the wrong port: adding a real
/// vocal-detection stage would be inventing a feature the app has never had,
/// and would change which pairs get the mid-cut (TD-21) for reasons no
/// measurement supports. If a real detector is ever wanted it should be a new
/// decision with its own evidence, not smuggled in under this name.
pub const VOCAL_PRESENCE_ENERGY: f32 = 0.35;

/// Whether a track reads as carrying vocals, in the only sense the Godot build
/// ever meant. See [`VOCAL_PRESENCE_ENERGY`].
pub fn has_vocal_presence(energy: f32) -> bool {
    energy > VOCAL_PRESENCE_ENERGY
}

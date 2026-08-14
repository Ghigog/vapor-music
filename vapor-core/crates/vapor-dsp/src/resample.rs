//! Sample rate conversion (MIG-016).
//!
//! The mixer runs at one rate. A library does not: 44.1 kHz and 48 kHz sit side
//! by side, and until now `render_mix` simply refused any pair that disagreed —
//! honest, but it means a real library cannot be mixed.
//!
//! Conversion happens once, at load, rather than per block. That keeps the
//! audio path simple (the stretcher and decks only ever see one rate) and puts
//! the cost where latency does not matter.
//!
//! ## Why windowed sinc rather than linear interpolation
//!
//! Linear interpolation is a poor low-pass filter: it leaves substantial
//! imaging above the passband and rolls off audibly below Nyquist. On a DJ
//! mixer that lands on exactly the material transitions expose — sustained
//! tones and cymbals held against each other. Band-limited interpolation costs
//! more arithmetic but this runs once per track, offline.
//!
//! The kernel is precomputed across a fixed set of sub-sample phases rather
//! than evaluated per output sample; that is what keeps a five-minute track to
//! well under a second.

/// Half-width of the interpolation kernel, in input samples.
///
/// 16 taps either side is a common quality/cost balance: stopband rejection is
/// good enough that imaging sits below the noise floor of any lossy source.
const HALF_TAPS: usize = 16;

/// Sub-sample positions the kernel is precomputed for. Quantising the phase to
/// 512 steps puts the residual error far below the 16-bit floor.
const PHASES: usize = 512;

/// Convert `input` from `from_rate` to `to_rate`.
///
/// Returns the input unchanged when the rates already match, so the common case
/// costs nothing and cannot colour the signal.
pub fn resample(input: &[[f32; 2]], from_rate: u32, to_rate: u32) -> Vec<[f32; 2]> {
    if from_rate == to_rate || input.is_empty() || from_rate == 0 || to_rate == 0 {
        return input.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    if out_len == 0 {
        return Vec::new();
    }

    // When downsampling, the kernel cutoff must follow the *output* Nyquist or
    // everything above it folds back as aliasing. Upsampling keeps the input
    // bandwidth, so the cutoff stays at 1.0.
    let cutoff = if ratio > 1.0 { 1.0 / ratio } else { 1.0 } as f32;
    let kernel = build_kernel(cutoff);

    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let base = pos.floor() as isize;
        let frac = (pos - base as f64) as f32;

        // Phase index into the precomputed kernel table.
        let phase = ((frac * PHASES as f32) as usize).min(PHASES - 1);
        let taps = &kernel[phase];

        let mut acc = [0.0f32, 0.0];
        for (t, &w) in taps.iter().enumerate() {
            let idx = base + t as isize - HALF_TAPS as isize + 1;
            if idx < 0 || idx as usize >= input.len() {
                continue;
            }
            let s = input[idx as usize];
            acc[0] += s[0] * w;
            acc[1] += s[1] * w;
        }
        out.push(acc);
    }
    out
}

/// Mono convenience for the analysis path, which works on a single channel.
pub fn resample_mono(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }
    let stereo: Vec<[f32; 2]> = input.iter().map(|&s| [s, s]).collect();
    resample(&stereo, from_rate, to_rate)
        .into_iter()
        .map(|s| s[0])
        .collect()
}

/// Precompute the windowed-sinc kernel for every sub-sample phase.
///
/// Each row is normalised to unit sum so a constant input passes through at
/// exactly its own level — without that, the conversion applies a small
/// phase-dependent gain ripple that reads as distortion on sustained tones.
fn build_kernel(cutoff: f32) -> Vec<Vec<f32>> {
    let taps = HALF_TAPS * 2;
    let mut table = Vec::with_capacity(PHASES);

    for p in 0..PHASES {
        let frac = p as f32 / PHASES as f32;
        let mut row = Vec::with_capacity(taps);
        let mut sum = 0.0f32;

        for t in 0..taps {
            // Distance from this tap to the interpolation point.
            let x = t as f32 - HALF_TAPS as f32 + 1.0 - frac;
            let w = sinc(x * cutoff) * cutoff * blackman(x);
            row.push(w);
            sum += w;
        }

        if sum.abs() > 1e-9 {
            for w in row.iter_mut() {
                *w /= sum;
            }
        }
        table.push(row);
    }
    table
}

fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else {
        let pix = std::f32::consts::PI * x;
        pix.sin() / pix
    }
}

/// Blackman window over the kernel's support, which suppresses the sidelobes a
/// bare truncated sinc would leave.
fn blackman(x: f32) -> f32 {
    let n = HALF_TAPS as f32;
    if x.abs() > n {
        return 0.0;
    }
    let t = (x + n) / (2.0 * n);
    0.42 - 0.5 * (2.0 * std::f32::consts::PI * t).cos()
        + 0.08 * (4.0 * std::f32::consts::PI * t).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, secs: f32) -> Vec<[f32; 2]> {
        let n = (secs * rate as f32) as usize;
        (0..n)
            .map(|i| {
                let v = (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 0.5;
                [v, v]
            })
            .collect()
    }

    fn rms(s: &[[f32; 2]]) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        (s.iter().map(|f| (f[0] * f[0]) as f64).sum::<f64>() / s.len() as f64).sqrt() as f32
    }

    #[test]
    fn matching_rates_are_a_passthrough() {
        let s = sine(440.0, 44100, 0.5);
        let out = resample(&s, 44100, 44100);
        assert_eq!(out, s);
    }

    #[test]
    fn output_length_follows_the_rate_ratio() {
        let s = sine(440.0, 48000, 1.0);
        let out = resample(&s, 48000, 44100);
        let expected = (s.len() as f64 * 44100.0 / 48000.0) as usize;
        assert!(
            (out.len() as isize - expected as isize).abs() <= 2,
            "got {} samples, expected ~{expected}",
            out.len()
        );
    }

    /// Level must survive conversion. A kernel that is not sum-normalised
    /// introduces a phase-dependent gain ripple, which is audible on sustained
    /// tones as distortion rather than as a level change.
    #[test]
    fn level_is_preserved_across_conversion() {
        for (from, to) in [(48000u32, 44100u32), (44100, 48000), (22050, 44100)] {
            let s = sine(440.0, from, 1.0);
            let out = resample(&s, from, to);
            let (a, b) = (rms(&s), rms(&out));
            assert!(
                (a - b).abs() / a < 0.02,
                "{from}->{to}: RMS {a:.4} became {b:.4}"
            );
        }
    }

    /// A converted sine must still be a sine at the same frequency. Comparing
    /// against an independently generated reference at the target rate catches
    /// both pitch errors and gross distortion.
    #[test]
    fn a_tone_keeps_its_frequency_and_shape() {
        let from = 48000u32;
        let to = 44100u32;
        let freq = 1000.0;

        let out = resample(&sine(freq, from, 1.0), from, to);
        let reference = sine(freq, to, 1.0);

        // Skip the kernel's edge transient at both ends.
        let skip = 64;
        let n = out.len().min(reference.len()) - skip;
        let mut err = 0.0f64;
        for i in skip..n {
            let d = (out[i][0] - reference[i][0]) as f64;
            err += d * d;
        }
        let rmse = (err / (n - skip) as f64).sqrt();
        assert!(rmse < 0.02, "RMSE {rmse:.4} against a reference tone");
    }

    /// Downsampling must band-limit. Content above the new Nyquist has to be
    /// filtered out, not folded back — aliasing is the failure that makes a
    /// naive resampler unusable.
    #[test]
    fn downsampling_rejects_content_above_the_new_nyquist() {
        let from = 48000u32;
        let to = 16000u32; // Nyquist 8 kHz

        // 15 kHz is above the target Nyquist and would alias to 1 kHz.
        let out = resample(&sine(15000.0, from, 1.0), from, to);
        let level = rms(&out);
        assert!(
            level < 0.02,
            "aliased content survived at RMS {level:.4}; it should be rejected"
        );
    }

    #[test]
    fn handles_degenerate_input() {
        assert!(resample(&[], 44100, 48000).is_empty());
        assert!(!resample(&sine(440.0, 44100, 0.1), 0, 48000).is_empty());
    }

    #[test]
    fn mono_helper_matches_the_stereo_path() {
        let mono: Vec<f32> = (0..4410)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin() * 0.5)
            .collect();
        let out = resample_mono(&mono, 44100, 48000);
        let expected = (mono.len() as f64 * 48000.0 / 44100.0) as usize;
        assert!((out.len() as isize - expected as isize).abs() <= 2);
    }
}

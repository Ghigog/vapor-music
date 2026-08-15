//! Shared STFT front end for tempo and key analysis.
//!
//! Tempo and key want *opposite* things from the transform, so this is
//! parameterised rather than computed once and shared:
//!
//! * Tempo needs fine **time** resolution — a short window and small hop, so
//!   percussive transients stay sharp in the onset detection function.
//! * Key needs fine **frequency** resolution. At a 2048-point window and
//!   44.1 kHz, bins are 21.5 Hz apart while a semitone at C4 (261 Hz) is only
//!   15.6 Hz — one bin spans more than a semitone, so spectral leakage lands in
//!   the neighbouring pitch class and the chroma is simply wrong. (Measured:
//!   a synthesised C major triad resolved as F minor.)
//!
//! Hence [`TEMPO_WINDOW`]/[`TEMPO_HOP`] and [`KEY_WINDOW`]/[`KEY_HOP`]. The key
//! spectrogram is not as expensive as it looks: the hop scales with the window,
//! so the frame count falls as the window grows.

use std::sync::Arc;

use rustfft::{num_complex::Complex32, Fft, FftPlanner};

/// 2048 @ 44.1 kHz = 46 ms window, 11.6 ms hop -> 86 Hz onset function.
pub const TEMPO_WINDOW: usize = 2048;
pub const TEMPO_HOP: usize = 512;

/// 8192 @ 44.1 kHz = 5.4 Hz per bin, ~3 bins per semitone at C4.
pub const KEY_WINDOW: usize = 8192;
pub const KEY_HOP: usize = 4096;

pub struct Spectrogram {
    /// `frames[i]` is the magnitude spectrum of frame `i`, length `window / 2`.
    pub frames: Vec<Vec<f32>>,
    pub sample_rate: u32,
    pub window: usize,
    pub hop: usize,
}

impl Spectrogram {
    /// Frames per second — the sample rate of the onset detection function.
    pub fn frame_rate(&self) -> f32 {
        self.sample_rate as f32 / self.hop as f32
    }

    /// Centre frequency of magnitude bin `k`.
    pub fn bin_freq(&self, k: usize) -> f32 {
        k as f32 * self.sample_rate as f32 / self.window as f32
    }

    /// Width of one bin in Hz.
    pub fn bin_width(&self) -> f32 {
        self.sample_rate as f32 / self.window as f32
    }
}

pub fn compute(samples: &[f32], sample_rate: u32, window: usize, hop: usize) -> Spectrogram {
    let mut planner = FftPlanner::<f32>::new();
    let fft: Arc<dyn Fft<f32>> = planner.plan_fft_forward(window);

    let win = hann(window);
    let half = window / 2;

    let frame_count = if samples.len() < window {
        0
    } else {
        (samples.len() - window) / hop + 1
    };

    let mut frames = Vec::with_capacity(frame_count);
    let mut buf = vec![Complex32::new(0.0, 0.0); window];
    let mut scratch = vec![Complex32::new(0.0, 0.0); fft.get_inplace_scratch_len()];

    for f in 0..frame_count {
        let start = f * hop;
        for i in 0..window {
            buf[i] = Complex32::new(samples[start + i] * win[i], 0.0);
        }
        fft.process_with_scratch(&mut buf, &mut scratch);

        let mut mags = Vec::with_capacity(half);
        for c in buf.iter().take(half) {
            mags.push(c.norm());
        }
        frames.push(mags);
    }

    Spectrogram {
        frames,
        sample_rate,
        window,
        hop,
    }
}

pub fn for_tempo(samples: &[f32], sample_rate: u32) -> Spectrogram {
    compute(samples, sample_rate, TEMPO_WINDOW, TEMPO_HOP)
}

pub fn for_key(samples: &[f32], sample_rate: u32) -> Spectrogram {
    compute(samples, sample_rate, KEY_WINDOW, KEY_HOP)
}

/// Where the "brightness" split sits, in Hz.
///
/// Roughly the top of the fundamental range for most instruments and voices, so
/// energy above it is mostly harmonics, cymbals and consonants — the parts that
/// make a track feel bright and busy rather than merely loud.
const BRIGHTNESS_SPLIT_HZ: f32 = 1_500.0;

/// Fraction of the spectrum's energy above [`BRIGHTNESS_SPLIT_HZ`], 0–1.
///
/// A perceptual ingredient rather than a measurement: at equal loudness, a
/// bright mix reads as more energetic than a dark one, which is why loudness
/// alone orders a set badly. Returns 0 for silence, where the ratio is
/// undefined and any other answer would be invented.
pub fn brightness(spec: &Spectrogram) -> f32 {
    if spec.frames.is_empty() {
        return 0.0;
    }

    let mut high = 0.0f64;
    let mut total = 0.0f64;
    for frame in &spec.frames {
        for (bin, magnitude) in frame.iter().enumerate() {
            // Power rather than magnitude: energy ratios are what perception
            // tracks, and squaring stops a wide quiet band outweighing a narrow
            // loud one.
            let power = (*magnitude as f64) * (*magnitude as f64);
            total += power;
            if spec.bin_freq(bin) >= BRIGHTNESS_SPLIT_HZ {
                high += power;
            }
        }
    }

    if total <= f64::EPSILON {
        return 0.0;
    }
    (high / total) as f32
}

fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = std::f32::consts::PI * 2.0 * i as f32 / n as f32;
            0.5 - 0.5 * x.cos()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Brightness has to separate a bright signal from a dark one at the same
    /// loudness, which is the whole reason it exists alongside LUFS.
    #[test]
    fn brightness_separates_a_bright_tone_from_a_dark_one() {
        let rate = 44_100;
        let tone = |hz: f32| -> Vec<f32> {
            (0..rate as usize)
                .map(|i| (i as f32 / rate as f32 * hz * std::f32::consts::TAU).sin() * 0.5)
                .collect()
        };

        let dark = brightness(&for_key(&tone(200.0), rate));
        let bright = brightness(&for_key(&tone(6_000.0), rate));

        assert!(dark < 0.25, "a 200 Hz tone read as bright: {dark}");
        assert!(bright > 0.75, "a 6 kHz tone read as dark: {bright}");
    }

    /// Silence has no ratio. Returning anything but zero would be inventing one.
    #[test]
    fn brightness_of_silence_is_zero() {
        let silence = vec![0.0f32; 44_100];
        assert_eq!(brightness(&for_key(&silence, 44_100)), 0.0);
    }

    #[test]
    fn brightness_is_bounded() {
        let rate = 44_100;
        let noise: Vec<f32> = (0..rate as usize)
            .map(|i| ((i * 2654435761usize) % 2000) as f32 / 1000.0 - 1.0)
            .collect();
        let b = brightness(&for_key(&noise, rate));
        assert!((0.0..=1.0).contains(&b), "out of range: {b}");
    }

    /// The property that broke the first implementation: at least ~2 bins per
    /// semitone across the range chroma actually reads.
    #[test]
    fn key_window_resolves_a_semitone_in_the_chroma_range() {
        let spec = compute(&vec![0.0; KEY_WINDOW * 2], 44100, KEY_WINDOW, KEY_HOP);
        let lowest = 130.0f32; // C3, the bottom of the chroma range
        let semitone_hz = lowest * (2f32.powf(1.0 / 12.0) - 1.0);
        let bins_per_semitone = semitone_hz / spec.bin_width();
        assert!(
            bins_per_semitone >= 1.4,
            "only {bins_per_semitone:.2} bins per semitone at {lowest} Hz"
        );
    }

    /// And the converse for tempo: the hop must resolve onsets well inside the
    /// gap between beats at the fastest tempo searched.
    #[test]
    fn tempo_hop_resolves_fast_onsets() {
        let spec = compute(&vec![0.0; TEMPO_WINDOW * 2], 44100, TEMPO_WINDOW, TEMPO_HOP);
        let frames_per_beat_at_200bpm = spec.frame_rate() * 60.0 / 200.0;
        assert!(
            frames_per_beat_at_200bpm >= 8.0,
            "only {frames_per_beat_at_200bpm:.1} frames per beat at 200 BPM"
        );
    }
}

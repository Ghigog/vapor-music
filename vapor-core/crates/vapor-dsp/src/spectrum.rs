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

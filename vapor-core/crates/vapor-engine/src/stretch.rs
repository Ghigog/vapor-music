//! Time-stretching — the replacement for Rubber Band.
//!
//! WSOLA (Waveform Similarity Overlap-Add): read overlapping grains, search a
//! small window for the offset that best correlates with what was already
//! written, and cross-fade. Tempo changes, pitch does not.
//!
//! **Why WSOLA and not a phase vocoder.** Beat-matching needs ±1–2% (the
//! Godot build's `_speed_scale_*` ramps), occasionally up to ±6% for a Tempo
//! Morph. In that range WSOLA is transparent, costs a fraction of an FFT-based
//! stretcher, and has no pre-echo. A phase vocoder earns its cost at
//! ratios far outside what harmonic mixing ever asks for.
//!
//! This is deliberately a placeholder for a real evaluation — see MIG-012.
//! It exists to prove beat-matching works without a GPL dependency, not to
//! settle the choice.

/// Grain size in samples at 44.1 kHz. ~46 ms: long enough to preserve bass
/// periodicity, short enough that the search window stays cheap.
const GRAIN: usize = 2048;
/// Overlap between consecutive output grains.
const OVERLAP: usize = GRAIN / 2;
/// How far to search for the best-correlating offset, in samples.
const SEARCH: usize = 256;

pub struct Stretcher {
    /// Fractional read position in the source, in frames.
    read_pos: f64,
    /// Tail of the previous grain, awaiting cross-fade with the next.
    tail: Vec<[f32; 2]>,
    window: Vec<f32>,
    initialised: bool,
}

impl Default for Stretcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Stretcher {
    pub fn new() -> Self {
        Stretcher {
            read_pos: 0.0,
            tail: vec![[0.0; 2]; OVERLAP],
            window: crossfade_ramp(OVERLAP),
            initialised: false,
        }
    }

    pub fn reset(&mut self, position_frames: f64) {
        self.read_pos = position_frames;
        self.tail.iter_mut().for_each(|s| *s = [0.0; 2]);
        self.initialised = false;
    }

    /// Current source read position in frames — the deck's true playback
    /// position, which drifts from output time exactly by the stretch ratio.
    pub fn source_position(&self) -> f64 {
        self.read_pos
    }

    /// Produce `out.len()` output frames from `src`, consuming source at
    /// `ratio` frames of source per frame of output.
    ///
    /// `ratio > 1.0` plays faster (consumes more source), `< 1.0` slower.
    /// At exactly 1.0 this is a pass-through copy — no grain machinery, so the
    /// common case costs nothing and cannot colour the signal.
    pub fn process(&mut self, src: &[[f32; 2]], ratio: f64, out: &mut [[f32; 2]]) -> usize {
        if (ratio - 1.0).abs() < 1e-6 {
            return self.passthrough(src, out);
        }

        let mut written = 0;
        while written < out.len() {
            let remaining = out.len() - written;
            let n = remaining.min(OVERLAP);

            if !self.grain(src, ratio, n, &mut out[written..written + n]) {
                return written;
            }
            written += n;
        }
        written
    }

    fn passthrough(&mut self, src: &[[f32; 2]], out: &mut [[f32; 2]]) -> usize {
        let start = self.read_pos as usize;
        let avail = src.len().saturating_sub(start);
        let n = out.len().min(avail);
        out[..n].copy_from_slice(&src[start..start + n]);
        self.read_pos += n as f64;
        n
    }

    /// Emit one grain of `n` frames.
    fn grain(&mut self, src: &[[f32; 2]], ratio: f64, n: usize, out: &mut [[f32; 2]]) -> bool {
        if self.read_pos < 0.0 {
            return false;
        }
        let ideal = self.read_pos as usize;

        // Room for the grain body plus the tail that gets stashed for the next
        // cross-fade. The search window is clamped rather than required, so a
        // grain at the very start of the file is still emitted — requiring it
        // meant the first grain always failed and the stretcher produced
        // nothing at all.
        let need = n + OVERLAP;
        if ideal + need >= src.len() {
            return false;
        }

        let start = if self.initialised {
            self.best_offset(src, ideal, need)
        } else {
            self.initialised = true;
            ideal
        };

        // Cross-fade the stored tail into the head of this grain.
        for i in 0..n.min(OVERLAP) {
            let w = self.window[i];
            for ch in 0..2 {
                out[i][ch] = self.tail[i][ch] * (1.0 - w) + src[start + i][ch] * w;
            }
        }

        // Any frames past the overlap region come straight from the source.
        if n > OVERLAP {
            out[OVERLAP..n].copy_from_slice(&src[start + OVERLAP..start + n]);
        }

        // Stash the next tail.
        let tail_start = start + n;
        for i in 0..OVERLAP {
            self.tail[i] = src.get(tail_start + i).copied().unwrap_or([0.0; 2]);
        }

        self.read_pos += n as f64 * ratio;
        true
    }

    /// Search ±SEARCH around the ideal read position for the offset whose
    /// leading frames best correlate with the stored tail. This is the entire
    /// point of WSOLA: splicing at a waveform-similar point avoids the phase
    /// discontinuity that a naive overlap-add would produce as a click.
    fn best_offset(&self, src: &[[f32; 2]], ideal: usize, need: usize) -> usize {
        let lo = ideal.saturating_sub(SEARCH);
        let hi = (ideal + SEARCH).min(src.len().saturating_sub(need + 1));
        if hi <= lo {
            return ideal;
        }

        let mut best = ideal;
        let mut best_score = f32::NEG_INFINITY;

        // Coarse stride: sample accuracy is unnecessary and the cost is linear.
        let mut cand = lo;
        while cand <= hi {
            let mut score = 0.0f32;
            // Correlate over a subset of the overlap — enough to find the
            // alignment, cheap enough to run every grain.
            for i in (0..OVERLAP).step_by(4) {
                let a = self.tail[i];
                let b = src[cand + i];
                score += a[0] * b[0] + a[1] * b[1];
            }
            if score > best_score {
                best_score = score;
                best = cand;
            }
            cand += 8;
        }
        best
    }
}

/// Rising equal-power cross-fade ramp: 0 at the start of the overlap, 1 at the
/// end, with `w^2 + (1-w)^2`-style complementary energy via sin²/cos².
///
/// **Not** a full Hann window. A Hann returns to zero at the end of the
/// overlap, which leaves the last output sample of every grain equal to the
/// stored tail rather than the new source — a discontinuity at each grain
/// boundary, i.e. a click every 23 ms.
fn crossfade_ramp(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = std::f32::consts::FRAC_PI_2 * i as f32 / (n - 1) as f32;
            x.sin().powi(2)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, secs: f32, rate: f32) -> Vec<[f32; 2]> {
        let n = (secs * rate) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / rate;
                let v = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn unity_ratio_is_bit_exact_passthrough() {
        let src = sine(440.0, 1.0, 44100.0);
        let mut s = Stretcher::new();
        let mut out = vec![[0.0f32; 2]; 4096];
        let n = s.process(&src, 1.0, &mut out);
        assert_eq!(n, 4096);
        assert_eq!(&out[..], &src[..4096]);
    }

    /// The property that matters for beat-matching: output duration scales by
    /// the ratio, so a deck asked to run 2% fast consumes 2% more source.
    #[test]
    fn source_consumption_tracks_the_ratio() {
        for &ratio in &[0.94f64, 0.98, 1.02, 1.06] {
            let src = sine(220.0, 10.0, 44100.0);
            let mut s = Stretcher::new();
            let mut out = vec![[0.0f32; 2]; 44100];
            let n = s.process(&src, ratio, &mut out);
            assert_eq!(n, out.len(), "ratio {ratio} produced a short buffer");

            let expected = out.len() as f64 * ratio;
            let actual = s.source_position();
            let err = (actual - expected).abs() / expected;
            assert!(
                err < 0.02,
                "ratio {ratio}: consumed {actual:.0} source frames, expected ~{expected:.0}"
            );
        }
    }

    /// Stretching must not introduce clicks. A discontinuity shows up as a
    /// sample-to-sample jump far larger than the signal's own slew rate.
    #[test]
    fn stretched_output_has_no_discontinuities() {
        let rate = 44100.0;
        let src = sine(220.0, 10.0, rate);
        let mut s = Stretcher::new();
        let mut out = vec![[0.0f32; 2]; 44100 * 4];
        let n = s.process(&src, 1.02, &mut out);

        // A 220 Hz sine at amplitude 0.5 slews at most ~0.016 per sample.
        // Allow generous headroom; a real click is an order of magnitude worse.
        let max_step = 0.15f32;
        for i in 1..n {
            let d = (out[i][0] - out[i - 1][0]).abs();
            assert!(d < max_step, "discontinuity {d:.3} at frame {i}");
        }
    }

    #[test]
    fn stops_cleanly_at_end_of_source() {
        let src = sine(440.0, 0.2, 44100.0);
        let mut s = Stretcher::new();
        let mut out = vec![[0.0f32; 2]; 44100];
        let n = s.process(&src, 1.03, &mut out);
        assert!(n < out.len(), "should run out of source");
        assert!(n > 0, "should produce something before running out");
    }
}

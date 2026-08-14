//! Master peak limiter.
//!
//! ## Why this exists, and why the RMS guard was not enough
//!
//! The three-band guard in [`crate::clipping`] is a faithful port of the Godot
//! implementation, and it prevents *sustained* level buildup — two basslines
//! occupying the low end at once. It is RMS-based, and measurement showed that
//! is the wrong tool for the clipping actually observed:
//!
//! ```text
//!   real Bass Swap transition window
//!     RMS   0.257     guard threshold 0.630  -> guard correctly does nothing
//!     peak  1.000     crest factor 3.9x      -> clipping is peak-domain
//! ```
//!
//! Modern masters run a crest factor near 4x, so two decks can sum well past
//! full scale on transients while their combined RMS stays comfortably under
//! any sane threshold. No RMS guard catches that. Neither would the Godot
//! build's, which is worth saying plainly: porting it faithfully did not fix
//! the defect that motivated the port.
//!
//! ## Design
//!
//! Block-look-ahead peak limiting. The mixer renders a whole block before
//! anything leaves, so the block's peak is known in advance and gain can be
//! reduced *before* the transient rather than after it — no overshoot, and none
//! of the latency a sample-delay line would add.
//!
//! Gain reduction attacks instantly and releases over a time constant, so a
//! single transient does not duck the following audio audibly.

/// Highest sample value allowed out. Slightly under full scale so downstream
/// conversion to 16-bit cannot round up into a clip.
pub const CEILING: f32 = 0.99;

/// Release time constant. Long enough not to pump on successive transients,
/// short enough that the mix recovers within a beat.
const RELEASE_SECS: f32 = 0.25;

#[derive(Clone, Copy, Debug)]
pub struct Limiter {
    /// Current gain reduction, 1.0 = no reduction.
    gain: f32,
    release_coef: f32,
}

impl Limiter {
    pub fn new(sample_rate: f32, block_size: usize) -> Self {
        // One release step is applied per block, so the coefficient is derived
        // from the block rate rather than the sample rate.
        let block_rate = sample_rate / block_size.max(1) as f32;
        let release_coef = (-1.0 / (RELEASE_SECS * block_rate)).exp();
        Limiter {
            gain: 1.0,
            release_coef,
        }
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
    }

    /// Current gain reduction in dB (0.0 when inactive), for metering.
    pub fn reduction_db(&self) -> f32 {
        if self.gain >= 1.0 {
            0.0
        } else {
            20.0 * self.gain.log10()
        }
    }

    /// Limit one block in place.
    pub fn process(&mut self, block: &mut [[f32; 2]]) {
        if block.is_empty() {
            return;
        }

        // Look ahead over the whole block: the peak is known before any sample
        // is written, so the required gain can be applied from the first sample
        // and nothing overshoots.
        let peak = block
            .iter()
            .flat_map(|s| [s[0].abs(), s[1].abs()])
            .fold(0.0f32, f32::max);

        let needed = if peak > CEILING { CEILING / peak } else { 1.0 };

        // Attack is instantaneous — a limiter that eases into reduction lets
        // the transient it is supposed to catch straight through.
        if needed < self.gain {
            self.gain = needed;
        } else {
            // Release toward unity, but never above what this block allows.
            //
            // `min(needed)`, not `max(needed)`: when a block needs no limiting
            // `needed` is 1.0, and taking the maximum would snap the gain
            // straight back to unity and cancel the release entirely — which is
            // exactly the pumping this time constant exists to avoid.
            self.gain = (self.gain / self.release_coef).min(1.0).min(needed);
        }

        if self.gain < 1.0 {
            for s in block.iter_mut() {
                s[0] *= self.gain;
                s[1] *= self.gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 44100.0;
    const BLOCK: usize = 512;

    fn block_of(v: f32) -> Vec<[f32; 2]> {
        vec![[v, v]; BLOCK]
    }

    #[test]
    fn quiet_audio_passes_through_untouched() {
        let mut l = Limiter::new(RATE, BLOCK);
        let mut b = block_of(0.5);
        l.process(&mut b);
        assert!(b.iter().all(|s| (s[0] - 0.5).abs() < 1e-6));
        assert_eq!(l.reduction_db(), 0.0);
    }

    #[test]
    fn overs_are_brought_under_the_ceiling() {
        let mut l = Limiter::new(RATE, BLOCK);
        let mut b = block_of(1.8);
        l.process(&mut b);
        let peak = b
            .iter()
            .flat_map(|s| [s[0].abs(), s[1].abs()])
            .fold(0.0f32, f32::max);
        assert!(
            peak <= CEILING + 1e-4,
            "peak {peak:.4} exceeded the ceiling"
        );
    }

    /// The property a naive limiter gets wrong: the very first sample of a
    /// transient must already be reduced, not the ones after it.
    #[test]
    fn the_first_sample_of_a_transient_is_already_limited() {
        let mut l = Limiter::new(RATE, BLOCK);
        let mut b = vec![[0.1f32, 0.1]; BLOCK];
        b[0] = [2.0, 2.0]; // spike on the first sample
        l.process(&mut b);
        assert!(
            b[0][0].abs() <= CEILING + 1e-4,
            "first sample overshot at {:.3}",
            b[0][0]
        );
    }

    #[test]
    fn gain_recovers_after_a_transient() {
        let mut l = Limiter::new(RATE, BLOCK);
        let mut loud = block_of(2.0);
        l.process(&mut loud);
        let reduced = l.reduction_db();
        assert!(reduced < -3.0, "expected real reduction, got {reduced}");

        // Feed quiet blocks; the gain must climb back toward unity.
        for _ in 0..200 {
            let mut q = block_of(0.2);
            l.process(&mut q);
        }
        assert!(
            l.reduction_db() > -0.5,
            "limiter did not release, still at {} dB",
            l.reduction_db()
        );
    }

    /// Release must not be so fast that each block jumps back to unity — that
    /// is what makes a limiter audibly pump.
    #[test]
    fn release_is_gradual_not_instant() {
        let mut l = Limiter::new(RATE, BLOCK);
        let mut loud = block_of(2.0);
        l.process(&mut loud);
        let after_hit = l.reduction_db();

        let mut q = block_of(0.2);
        l.process(&mut q);
        let after_one = l.reduction_db();

        assert!(
            after_one > after_hit && after_one < 0.0,
            "expected partial recovery, went {after_hit:.2} -> {after_one:.2} dB"
        );
    }
}

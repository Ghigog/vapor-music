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
    /// The gain actually applied to the final sample of the previous block.
    ///
    /// Not the same as `gain`, which is where the ramp *ended*: a final sample
    /// over the ceiling is scaled by `CEILING / mag` instead, and it is the
    /// applied value the waveform is made of.
    last_applied: f32,
    /// How far the applied gain jumped across the last block boundary.
    ///
    /// The one discontinuity this design can still produce. Within a block the
    /// gain is continuous by construction — the ramp is linear and the ceiling
    /// term meets it exactly at the crossover — so a step can only appear
    /// between the last sample of one block and the first of the next, and only
    /// if the state carried between them is wrong. That is not hypothetical:
    /// dropping `self.gain = target` at the end of `process` does exactly this,
    /// and it has been dropped before.
    boundary_jump: f32,
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
            last_applied: 1.0,
            boundary_jump: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
        self.last_applied = 1.0;
        self.boundary_jump = 0.0;
    }

    /// The step in applied gain across the most recent block boundary.
    ///
    /// Zero in normal operation. Anything meaningfully above it is a
    /// discontinuity in the output — a click, or a buzz at the block rate.
    pub fn boundary_jump(&self) -> f32 {
        self.boundary_jump
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

        // Where the gain has to be by the end of this block. Attack is still
        // immediate in the sense that matters — the ceiling is enforced per
        // sample below, so a transient cannot slip through while the ramp is
        // still on its way down.
        let target = if needed < self.gain {
            needed
        } else {
            // Release toward unity, but never above what this block allows.
            //
            // `min(needed)`, not `max(needed)`: when a block needs no limiting
            // `needed` is 1.0, and taking the maximum would snap the gain
            // straight back to unity and cancel the release entirely — which is
            // exactly the pumping this time constant exists to avoid.
            (self.gain / self.release_coef).min(1.0).min(needed)
        };

        // Nothing over the ceiling and no reduction left to release: the
        // common case, and it should cost a comparison rather than a pass.
        if self.gain >= 1.0 && peak <= CEILING {
            // The block is left alone, so unity is what reached the waveform.
            self.boundary_jump = (1.0 - self.last_applied).abs();
            self.last_applied = 1.0;
            self.gain = target;
            return;
        }

        /*
         * Ramped across the block, not applied flat to it (LIM-001).
         *
         * This used to multiply the whole block by one number. The number
         * changes between blocks, so every change landed as a step
         * discontinuity in the waveform at a block boundary — one is a click,
         * and a run of them at the block rate is a buzz. Heard as "pops and
         * scratches" during mixes, which is when the limiter has most to do:
         * the stretcher outputs above full scale at beat-matching ratios
         * (measured +0.57 dB at 1.06), so a transition is exactly when this
         * engages. `write_out` has always ramped the master volume across a
         * block for this reason; this was the one gain stage that did not.
         *
         * The per-sample term is what keeps the ramp honest. Ramping *into* a
         * reduction would let the first samples of a transient through, which
         * is the property `the_first_sample_of_a_transient_is_already_limited`
         * exists to hold — so where a sample would clear the ceiling it is
         * scaled to sit exactly on it. That term follows the signal's own
         * envelope, so where it takes over it does so continuously, and it
         * cannot introduce a step of its own.
         */
        let start = self.gain;
        let step = (target - start) / block.len() as f32;
        let mut applied = self.last_applied;
        for (i, s) in block.iter_mut().enumerate() {
            let ramp = start + step * i as f32;
            let mag = s[0].abs().max(s[1].abs());
            let gain = if mag * ramp > CEILING {
                CEILING / mag
            } else {
                ramp
            };
            if i == 0 {
                // Measured against the previous block's final sample, which is
                // the only pair of adjacent samples whose gains come from
                // different invocations.
                self.boundary_jump = (gain - self.last_applied).abs();
            }
            applied = gain;
            s[0] *= gain;
            s[1] *= gain;
        }
        self.last_applied = applied;

        // The ramp ended here, so the next block starts from it. Without this
        // the limiter holds no state at all between blocks and the release
        // never happens — which the release tests catch, but which is worth
        // saying out loud next to the thing that depends on it.
        self.gain = target;
    }
}

#[cfg(test)]
mod tests {

    /// The metric has to see the bug it exists for.
    ///
    /// `boundary_jump` replaced a counter that compared gain *reduction*
    /// between blocks and called any change a step. That was true when the
    /// limiter applied one gain to a whole block; once the gain ramps, a change
    /// between blocks is interpolated and no longer a discontinuity — so the
    /// old counter reported seventeen "steps" during a perfectly clean mix and
    /// would have gone on doing it for ever.
    ///
    /// This measures the boundary itself, which is the only place a step can
    /// still appear. Driven with the material that produces reduction: loud
    /// blocks, so the limiter is actually working across the join.
    #[test]
    fn a_clean_run_of_loud_blocks_reports_no_discontinuity() {
        let mut limiter = Limiter::new(48_000.0, 512);
        let mut worst = 0.0f32;

        for block_index in 0..40 {
            // Loud enough to need reduction, and varying, so the gain is moving
            // at every boundary rather than sitting still.
            let amplitude = 1.2 + 0.6 * ((block_index % 5) as f32 / 5.0);
            let mut block = vec![[0.0f32; 2]; 512];
            for (i, s) in block.iter_mut().enumerate() {
                let phase = (i as f32 / 512.0) * std::f32::consts::TAU * 4.0;
                let v = amplitude * phase.sin();
                *s = [v, v];
            }
            limiter.process(&mut block);
            if block_index > 0 {
                worst = worst.max(limiter.boundary_jump());
            }
        }

        assert!(
            worst < 1e-3,
            "the gain jumped {worst} across a block boundary; the ramp is \
             supposed to make the join continuous"
        );
    }

    /// And it has to fail when the state between blocks is dropped.
    ///
    /// Reproducing the real defect rather than asserting against a constant:
    /// `reset()` between blocks is what "the limiter holds no state" looks
    /// like, which is precisely what omitting `self.gain = target` produced.
    #[test]
    fn losing_the_gain_between_blocks_shows_up_as_a_jump() {
        let mut limiter = Limiter::new(48_000.0, 512);
        let loud = || {
            let mut block = vec![[0.0f32; 2]; 512];
            for (i, s) in block.iter_mut().enumerate() {
                let phase = (i as f32 / 512.0) * std::f32::consts::TAU * 4.0;
                let v = 3.0 * phase.sin();
                *s = [v, v];
            }
            block
        };

        let mut block = loud();
        limiter.process(&mut block);
        let settled = limiter.boundary_jump();

        // Throw the carried gain away, exactly as the bug did.
        let carried = limiter.last_applied;
        limiter.gain = 1.0;
        let mut next = loud();
        limiter.process(&mut next);

        assert!(
            limiter.boundary_jump() > settled,
            "a limiter that forgot its gain ({carried}) must report a bigger \
             jump than one that did not"
        );
        assert!(
            limiter.boundary_jump() > 0.05,
            "and a jump that size is a click, not float noise: got {}",
            limiter.boundary_jump()
        );
    }
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

    /// The defect this file was rewritten for (LIM-001).
    ///
    /// The gain used to be one number per block, multiplied across the whole
    /// block, so every change between blocks landed as a step discontinuity in
    /// the waveform at the block edge. A single one is a click; at the block
    /// rate it is a buzz. In the field: nineteen steps in 250 ms during a mix,
    /// reported as "pops and scratches".
    ///
    /// Measured as the largest change in *applied gain* between two adjacent
    /// samples. A ramp spreads a block's worth of change over its 512 samples;
    /// a flat per-block gain puts all of it at one edge.
    #[test]
    fn the_gain_ramps_across_the_block_rather_than_stepping_at_its_edge() {
        let mut l = Limiter::new(RATE, BLOCK);
        // Engage it hard, then release over quiet blocks — the release is where
        // the gain moves every block and the steps were audible.
        l.process(&mut block_of(2.0));

        const BODY: f32 = 0.3;
        let mut applied = Vec::new();
        for _ in 0..8 {
            let mut q = block_of(BODY);
            l.process(&mut q);
            // Well under the ceiling, so what is left is the gain itself.
            applied.extend(q.iter().map(|s| s[0] / BODY));
        }

        let worst = applied
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);

        // Release moves a few percent of gain per block; spread over 512
        // samples that is well under 1e-3 each. Flat-per-block would show the
        // whole per-block jump — about 0.02 — at a single sample.
        assert!(
            worst < 1e-3,
            "applied gain jumped {worst:.5} between adjacent samples, which is \
             a step in the waveform rather than a ramp",
        );
        // And it really did move, or the test proves nothing.
        let travelled = (applied[applied.len() - 1] - applied[0]).abs();
        assert!(travelled > 0.05, "gain barely moved ({travelled:.3}); the test \
             would pass on a limiter that does nothing at all");
    }

    /// Release must not be so fast that each block jumps back to unity — that
    /// is what makes a limiter audibly pump."""
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

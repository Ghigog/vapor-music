//! Signalsmith Stretch — the chosen time-stretcher (MIG-012).
//!
//! ## Why this one
//!
//! The field of things that are actually used for this is small: Rubber Band,
//! Signalsmith Stretch, élastique, and a phase vocoder you wrote yourself.
//!
//! * **Rubber Band** is the reference implementation and sounds it. It is
//!   GPL-2.0-or-later or a paid commercial licence. GPL is compatible with this
//!   app's AGPL, so that is not the objection — the objection is that it is a
//!   substantial C++ library with its own build system, and the migration's
//!   entire reason for existing was to stop maintaining a C++ dependency tail
//!   that only built on one machine.
//! * **élastique** is proprietary and priced per title.
//! * **Signalsmith Stretch** is MIT, is a handful of header files, is written
//!   by a DSP engineer for exactly this application, and holds up at the large
//!   ratios where a naive phase vocoder smears. It has a maintained Rust
//!   wrapper.
//!
//! So: Signalsmith. It is the best free answer and the licence costs nothing.
//!
//! ## What it costs, stated plainly
//!
//! It is C++ behind a `cc` build, which means **it does not compile to wasm**.
//! The portable claim in the architecture — one DSP implementation, native and
//! browser — survives because [`crate::stretch`]'s WSOLA is still here and is
//! what the wasm build uses.
//!
//! ## Where the latency actually enters
//!
//! The preset reports 2646 frames of input latency and 2646 of output latency
//! at 44.1 kHz. Measured with `examples/impulse.rs`, which drives the wrapper
//! directly with a click and reports where it comes out, the mapping is:
//!
//! ```text
//! output_frame = (input_frame + input_latency) / ratio + output_latency
//! ```
//!
//! Both latencies count, but **in different domains**: output latency is
//! already in output frames, input latency is in input frames and only becomes
//! output frames after dividing by the ratio. That is the whole of it, and it
//! is why three pre-roll attempts each moved the error by precisely zero
//! milliseconds — pre-roll shifts input and output together, so the difference
//! between them is invariant. Measured, at a 128 BPM grid:
//!
//! | Strategy | Error at ratio 1.02 / 1.05 / 0.95 |
//! |---|---|
//! | no compensation | 119.6 / 117.7 / 123.6 ms |
//! | `seek` pre-roll | 119.6 / 117.7 / 123.6 ms — *bit-identical to none* |
//! | push input, pull no output | 119.7 / 117.4 / 123.6 ms |
//! | feed `L·ratio`, discard `L` | 119.0 / 117.3 / 123.4 ms |
//! | **discard `Lₒ + Lᵢ/ratio` of output** | **0.2 / 0.5 / 0.4 ms** |
//!
//! `seek` being bit-identical to doing nothing is the tell: the library's own
//! pre-roll API cannot fix this either, because pre-roll is not what is wrong.
//! The correction is one-sided — throw away the first `Lₒ + Lᵢ/ratio` frames of
//! output and read nothing from before the start position. No history window is
//! needed, and the mix does not begin with a hole.
//!
//! ## What the fix costs
//!
//! The discard is computed from the ratio in force when the stream is primed.
//! A ratio change afterwards re-expresses the input-side latency in different
//! output frames, and the alignment shifts by `Lᵢ·(1/r₁ − 1/r₂)`. Across the
//! engine's ±6% stretch range that is at most about 3.4 ms, an order of
//! magnitude under the 28 ms this has to beat. Re-priming mid-mix would cost a
//! discontinuity to buy that back, so it is not done.
//!
//! Everything else about it checks out: allocation-free on the audio thread
//! across 200 blocks, finite output at every ratio, a true pass-through at
//! unity, and a read position that tracks the ratio. See
//! `tests/signalsmith_realtime.rs`.

use crate::source::SourceView;
use crate::stretch::{to_f32, Rendered};

/// The largest block this will ever be asked for, in frames.
///
/// Buffers are sized to it once, because `process` runs on the audio thread
/// and must not allocate. Both the render loop and the priming discard are
/// served in pieces of at most this, so no buffer is ever sized from the
/// latency and no request can be too large to serve. The previous version sized
/// a buffer from the latency and bailed out when it did not fit, which is a
/// silent no-op wearing the costume of a correction.
const MAX_BLOCK: usize = 4096;

/// Samples spent crossfading out of the stretcher. See [`Signalsmith::hand_over`].
const FADE: usize = 256;

/// Signalsmith's own recommendation for music, at 44.1 kHz.
const SAMPLE_RATE: u32 = 44_100;

pub struct Signalsmith {
    inner: signalsmith_stretch::Stretch,
    /// Source frame that the *next output frame* corresponds to.
    ///
    /// This is what [`Self::source_position`] reports and what the beat grid is
    /// read against. It is deliberately **not** the source cursor: the whole
    /// bug this file documents was one field trying to be both.
    read_pos: f64,
    /// Source frame the stretcher has been fed up to.
    ///
    /// Runs ahead of `read_pos` by the latency, which is exactly what latency
    /// means. Kept fractional so a long mix does not drift by accumulating a
    /// truncation every block.
    feed_pos: f64,
    /// Interleaved input handed to the stretcher.
    input: Vec<f32>,
    /// Interleaved output taken back from it.
    output: Vec<f32>,
    /// Whether the stretcher has been primed since the last reset.
    primed: bool,
}

impl Signalsmith {
    pub fn new() -> Self {
        let inner = signalsmith_stretch::Stretch::preset_default(2, SAMPLE_RATE);
        // One block's worth, plus a frame for the fractional carry. Nothing
        // here is sized from the latency, so there is no size to get wrong.
        let frames = MAX_BLOCK + 2;
        Signalsmith {
            inner,
            read_pos: 0.0,
            feed_pos: 0.0,
            input: vec![0.0; frames * 2],
            output: vec![0.0; frames * 2],
            primed: false,
        }
    }

    pub fn reset(&mut self, position_frames: f64) {
        self.inner.reset();
        self.read_pos = position_frames;
        self.feed_pos = position_frames;
        self.primed = false;
    }

    pub fn source_position(&self) -> f64 {
        self.read_pos
    }

    /// Produce `out.len()` frames, consuming source at `ratio` frames of
    /// source per frame of output.
    ///
    /// Same contract as [`crate::stretch::Stretcher::process`], including the
    /// pass-through at unity — a mix that is not stretching should not be
    /// paying for an FFT or be coloured by one.
    pub fn process(&mut self, src: &SourceView<'_>, ratio: f64, out: &mut [[f32; 2]]) -> Rendered {
        if (ratio - 1.0).abs() < 1e-6 {
            // Primed means the vocoder is mid-stream and its output is not
            // sample-aligned with the source, so handing straight over to a raw
            // copy is a step in the waveform. See `hand_over`.
            if self.primed {
                return self.hand_over(src, out);
            }
            return self.passthrough(src, out);
        }

        if !self.primed {
            if !self.prime(src, ratio) {
                // The source could not supply the pre-roll yet — a cold seek
                // against a slow disk. Stay unprimed and try again next block
                // rather than emitting audio that is known to be 120 ms out.
                return Rendered {
                    frames: 0,
                    ended: src.is_complete(),
                };
            }
            self.primed = true;
        }

        let mut written = 0;
        while written < out.len() {
            let want = (out.len() - written).min(self.max_out(ratio));
            if !self.block(src, want, ratio, Some(&mut out[written..written + want])) {
                return Rendered {
                    frames: written,
                    ended: src.is_complete(),
                };
            }
            self.read_pos += want as f64 * ratio;
            written += want;
        }

        Rendered {
            frames: written,
            ended: false,
        }
    }

    /// Discard the stretcher's latency before the first real block.
    ///
    /// `Lₒ + Lᵢ/ratio` frames of output, thrown away, with input fed forward
    /// from `read_pos`. See the module documentation for why it is those two
    /// terms and why pre-roll is not one of them.
    ///
    /// `read_pos` does not move: the discarded frames are the stretcher filling
    /// up, not part of the track being played. `feed_pos` does, by the input
    /// those frames consumed, and the gap between the two is the latency made
    /// explicit.
    fn prime(&mut self, src: &SourceView<'_>, ratio: f64) -> bool {
        // A partially-primed stretcher from a failed attempt would otherwise
        // still be holding that audio when the retry re-feeds it.
        self.inner.reset();
        self.feed_pos = self.read_pos;

        let discard = (self.inner.output_latency() as f64
            + self.inner.input_latency() as f64 / ratio)
            .round() as usize;

        let mut left = discard;
        while left > 0 {
            let want = left.min(self.max_out(ratio));
            if !self.block(src, want, ratio, None) {
                return false;
            }
            left -= want;
        }
        true
    }

    /// Output frames servable in one call at `ratio` without overrunning the
    /// input buffer. At ratios above 1 the input side is the binding
    /// constraint, so a large ratio produces a shorter block rather than a
    /// failure.
    fn max_out(&self, ratio: f64) -> usize {
        let by_input = (MAX_BLOCK as f64 / ratio.max(f64::MIN_POSITIVE)).floor();
        (by_input as usize).clamp(1, MAX_BLOCK)
    }

    /// Produce `want` frames, writing them to `dest` or discarding them.
    ///
    /// False when the source cannot supply the input — which the caller has to
    /// distinguish from the end of the track, since a slow disk read looks
    /// identical from here.
    fn block(
        &mut self,
        src: &SourceView<'_>,
        want: usize,
        ratio: f64,
        dest: Option<&mut [[f32; 2]]>,
    ) -> bool {
        debug_assert!(want <= MAX_BLOCK);
        let start = self.feed_pos;
        let first = start.floor().max(0.0) as u64;
        let mut end = start + want as f64 * ratio;
        // Input frames this block consumes. Taken as a difference of floors so
        // the fractional remainder stays in `feed_pos` instead of being
        // truncated away once per block.
        let mut need = (end.floor().max(0.0) as u64).saturating_sub(first) as usize;
        if need == 0 {
            // Only reachable when `want · ratio < 1`. Feed one frame and put
            // `feed_pos` where that leaves it, or the carry desynchronises.
            need = 1;
            end = first as f64 + 1.0;
        }

        if !self.fill(src, first, need) {
            return false;
        }
        self.inner
            .process(&self.input[..need * 2], &mut self.output[..want * 2]);

        if let Some(dest) = dest {
            for (i, frame) in dest.iter_mut().enumerate() {
                *frame = [self.output[i * 2], self.output[i * 2 + 1]];
            }
        }
        self.feed_pos = end;
        true
    }

    /// Read `needed` frames from `first` into the interleaved input buffer.
    fn fill(&mut self, src: &SourceView<'_>, first: u64, needed: usize) -> bool {
        debug_assert!(needed * 2 <= self.input.len());
        for i in 0..needed {
            let Some(frame) = src.get(first + i as u64) else {
                return false;
            };
            self.input[i * 2] = to_f32(frame[0]);
            self.input[i * 2 + 1] = to_f32(frame[1]);
        }
        true
    }

    /// Leave the stretcher for the pass-through, over a short crossfade.
    ///
    /// ## The click this removes
    ///
    /// `TEMPO_GLIDE_SECS` walks the playing deck's ratio back to 1.0 over six
    /// seconds after every beat-matched mix. The instant it arrives, `process`
    /// used to switch to copying raw source samples — and a phase vocoder's
    /// output is a reconstruction, not a copy, so it does not line up with the
    /// source sample for sample. Measured with a 220 Hz tone
    /// (`tests/glide_splice.rs`): a step of 0.304 against the 0.0157 the tone
    /// itself moves between neighbouring samples, **19x**, at full volume, six
    /// seconds after every mix.
    ///
    /// ## Why a crossfade rather than staying stretched
    ///
    /// The alternative is to keep running the vocoder at unity for the rest of
    /// the track. That has no seam, but it pays an FFT per block for ever and
    /// keeps a reconstruction where a copy would do — the pass-through exists
    /// because a deck that is not stretching should be bit-exact.
    ///
    /// [`FADE`] samples is under six milliseconds: long enough that a step
    /// becomes a ramp, short enough that nothing is audibly faded.
    fn hand_over(&mut self, src: &SourceView<'_>, out: &mut [[f32; 2]]) -> Rendered {
        let n = out.len().min(FADE);

        // What the vocoder would have said next, at unity.
        let mut stretched = [[0.0f32; 2]; FADE];
        if !self.block(src, n, 1.0, Some(&mut stretched[..n])) {
            // The source cannot serve the fade yet. Stay as we are and try
            // again next block rather than splicing without one.
            return Rendered {
                frames: 0,
                ended: src.is_complete(),
            };
        }

        // And what the source itself holds there.
        let base = self.read_pos.floor().max(0.0) as u64;
        for i in 0..n {
            let Some(raw) = src.get(base + i as u64) else {
                return Rendered {
                    frames: 0,
                    ended: src.is_complete(),
                };
            };
            let w = i as f32 / n as f32;
            out[i] = [
                stretched[i][0] * (1.0 - w) + to_f32(raw[0]) * w,
                stretched[i][1] * (1.0 - w) + to_f32(raw[1]) * w,
            ];
        }

        self.read_pos += n as f64;
        self.feed_pos = self.read_pos;
        self.inner.reset();
        self.primed = false;

        // Whatever is left of the block is already the plain copy.
        if n < out.len() {
            let rest = self.passthrough(src, &mut out[n..]);
            return Rendered {
                frames: n + rest.frames,
                ended: rest.ended,
            };
        }
        Rendered {
            frames: n,
            ended: false,
        }
    }

    /// Unity ratio: copy, and cost nothing.
    fn passthrough(&mut self, src: &SourceView<'_>, out: &mut [[f32; 2]]) -> Rendered {
        // The stretcher is being bypassed, so its buffered state is about to go
        // stale. Drop it now and re-prime on the way back into stretching,
        // rather than resuming a mix from audio that is 120 ms old.
        if self.primed {
            self.inner.reset();
            self.primed = false;
        }

        let base = self.read_pos.floor().max(0.0) as u64;
        for (i, frame) in out.iter_mut().enumerate() {
            let Some(sample) = src.get(base + i as u64) else {
                self.read_pos += i as f64;
                self.feed_pos = self.read_pos;
                return Rendered {
                    frames: i,
                    ended: src.is_complete(),
                };
            };
            *frame = [to_f32(sample[0]), to_f32(sample[1])];
        }
        self.read_pos += out.len() as f64;
        self.feed_pos = self.read_pos;
        Rendered {
            frames: out.len(),
            ended: false,
        }
    }
}

impl Default for Signalsmith {
    fn default() -> Self {
        Self::new()
    }
}

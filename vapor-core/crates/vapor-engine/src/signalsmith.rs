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
//! ## Why it is not the default yet
//!
//! **It does not hold the beat grid as integrated here.** Measured
//! 2026-08-16 by `beat_alignment::rendered_transition_keeps_all_onsets_on_the_outgoing_grid`:
//!
//! | Stretcher | Worst onset error, 128 BPM grid |
//! |---|---|
//! | WSOLA (`crate::stretch`) | 28.29 ms — passes |
//! | Signalsmith, as integrated | **118.4 ms** — fails |
//!
//! The preset reports 2646 frames of latency each way at 44.1 kHz: 60 ms in,
//! 60 ms out, 120 ms together, which is within a rounding error of the
//! measurement. That is almost certainly the mechanism.
//!
//! What is *not* yet explained is that the number did not move at all under
//! three different corrections — priming with upcoming audio, priming with
//! `seek` pre-roll, and running the latency out through `process` and
//! discarding it. An unchanged figure across three attempts means the offset
//! is entering somewhere other than where it is being corrected, and guessing
//! again is not the way to find out. The next step is to measure the impulse
//! response of this wrapper directly — feed a click, find it in the output —
//! rather than to reason about it.
//!
//! So the library choice is settled and the integration is not. WSOLA remains
//! the default; this is selectable, and everything else about it checks out:
//! allocation-free on the audio thread across 200 blocks, finite output at
//! every ratio, a true pass-through at unity, and a read position that tracks
//! the ratio. See `tests/signalsmith_realtime.rs`.

use crate::source::SourceView;
use crate::stretch::{to_f32, Rendered};

/// The largest block this will ever be asked for, in frames.
///
/// Buffers are sized to it once, because `process` runs on the audio thread
/// and must not allocate. A request larger than this is served in pieces
/// rather than growing anything.
const MAX_BLOCK: usize = 4096;

/// Signalsmith's own recommendation for music, at 44.1 kHz.
const SAMPLE_RATE: u32 = 44_100;

pub struct Signalsmith {
    inner: signalsmith_stretch::Stretch,
    /// Fractional read position in the source, in frames.
    read_pos: f64,
    /// Interleaved input handed to the stretcher.
    input: Vec<f32>,
    /// Interleaved output taken back from it.
    output: Vec<f32>,
    /// Whether the stretcher has been primed since the last reset.
    ///
    /// Signalsmith has latency: the first `output_latency()` frames it
    /// produces are the tail of silence it started with. Priming with real
    /// audio at the seek position is what stops a mix beginning with a hole.
    primed: bool,
}

impl Signalsmith {
    pub fn new() -> Self {
        Signalsmith {
            inner: signalsmith_stretch::Stretch::preset_default(2, SAMPLE_RATE),
            read_pos: 0.0,
            input: vec![0.0; MAX_BLOCK * 4],
            output: vec![0.0; MAX_BLOCK * 2],
            primed: false,
        }
    }

    pub fn reset(&mut self, position_frames: f64) {
        self.inner.reset();
        self.read_pos = position_frames;
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
            return self.passthrough(src, out);
        }

        let mut written = 0;
        while written < out.len() {
            let want = (out.len() - written).min(MAX_BLOCK);
            // How much source this block of output consumes. Rounded, with the
            // remainder carried in `read_pos`, so a long mix does not drift by
            // accumulating a truncation every block.
            let start = self.read_pos;
            let end = start + want as f64 * ratio;
            let first = start.floor().max(0.0) as u64;
            let needed = ((end.floor().max(0.0) as u64).saturating_sub(first)).max(1) as usize;

            if !self.fill(src, first, needed) {
                return Rendered {
                    frames: written,
                    ended: src.is_complete(),
                };
            }

            if !self.primed {
                self.prime(src, first, ratio);
                self.primed = true;
                // `prime` used the input buffer for the pre-roll, so the block
                // has to be read again on top of it.
                if !self.fill(src, first, needed) {
                    return Rendered {
                        frames: written,
                        ended: src.is_complete(),
                    };
                }
            }

            self.inner
                .process(&self.input[..needed * 2], &mut self.output[..want * 2]);

            for (i, frame) in out[written..written + want].iter_mut().enumerate() {
                *frame = [self.output[i * 2], self.output[i * 2 + 1]];
            }

            self.read_pos = end;
            written += want;
        }

        Rendered {
            frames: written,
            ended: false,
        }
    }

    /// Run the stretcher's latency out before the first real block.
    ///
    /// This is the whole of beat alignment for a stretcher that has latency,
    /// and it is worth stating exactly. Signalsmith's default preset reports
    /// 2646 frames in and 2646 out at 44.1 kHz — 60 ms each way, 120 ms
    /// together. Ignore it and every onset lands 120 ms late: measured at
    /// **118.4 ms off a 128 BPM grid**, against 28.29 ms for the WSOLA this
    /// replaces. For beat-matching that is not a subtle degradation, it is a
    /// different beat.
    ///
    /// Output frame `n` corresponds to input frame `(n − L) · ratio`. So to
    /// make output frame 0 correspond to `read_pos`, feed input starting
    /// `L · ratio` frames *earlier* and throw away the first `L` frames of
    /// output. `read_pos` itself does not move: the discarded audio is the
    /// pre-roll, not part of the track being played.
    ///
    /// The pre-roll comes out of the deck's history window, which exists
    /// because the WSOLA search also reads backwards (see `crate::source`).
    /// Where there is no history — the track starts here — the stretcher is
    /// primed with what silence it has, and the first 120 ms of a track nobody
    /// is mixing into is not where alignment matters.
    fn prime(&mut self, src: &SourceView<'_>, at: u64, ratio: f64) {
        let latency = self.inner.output_latency();
        if latency == 0 {
            return;
        }
        let consumed = ((latency as f64) * ratio).ceil() as usize;
        if consumed * 2 > self.input.len() || latency * 2 > self.output.len() {
            return;
        }

        let from = at.saturating_sub(consumed as u64);
        let available = (at - from) as usize;
        if available == 0 {
            return;
        }

        // Short history means the track begins near here; feed what there is.
        if !self.fill(src, from, available) {
            return;
        }
        self.inner.process(
            &self.input[..available * 2],
            &mut self.output[..latency * 2],
        );
    }

    /// Read `needed` frames from `first` into the interleaved input buffer.
    ///
    /// False when the source cannot supply them — which the caller has to
    /// distinguish from the end of the track, since a slow disk read looks
    /// identical from here.
    fn fill(&mut self, src: &SourceView<'_>, first: u64, needed: usize) -> bool {
        if needed * 2 > self.input.len() {
            return false;
        }
        for i in 0..needed {
            let Some(frame) = src.get(first + i as u64) else {
                return false;
            };
            self.input[i * 2] = to_f32(frame[0]);
            self.input[i * 2 + 1] = to_f32(frame[1]);
        }
        true
    }

    /// Unity ratio: copy, and cost nothing.
    fn passthrough(&mut self, src: &SourceView<'_>, out: &mut [[f32; 2]]) -> Rendered {
        let base = self.read_pos.floor().max(0.0) as u64;
        for (i, frame) in out.iter_mut().enumerate() {
            let Some(sample) = src.get(base + i as u64) else {
                self.read_pos += i as f64;
                return Rendered {
                    frames: i,
                    ended: src.is_complete(),
                };
            };
            *frame = [to_f32(sample[0]), to_f32(sample[1])];
        }
        self.read_pos += out.len() as f64;
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

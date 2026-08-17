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
//!
//! ## Reading through a window
//!
//! The source is a [`SourceView`] rather than a slice, because a deck may hold
//! only a few seconds of the track (TD-09). Nothing about the algorithm
//! changes: the same indices are read, they are just absolute positions in the
//! track rather than offsets into a buffer that happens to hold all of it.

/// Grain size in samples at 44.1 kHz. ~46 ms: long enough to preserve bass
/// periodicity, short enough that the search window stays cheap.
const GRAIN: usize = 2048;
/// Overlap between consecutive output grains.
const OVERLAP: usize = GRAIN / 2;
/// How far to search for the best-correlating offset, in samples.
///
/// This is the number that decides how a streaming source has to be shaped:
/// the search reads *backwards* from the read position, so a deck's audio
/// cannot be a queue it pops from. See [`crate::source`], whose `HISTORY` must
/// stay comfortably above this.
const SEARCH: usize = 256;

use crate::source::SourceView;

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
    ///
    /// A short return has two quite different meanings and the caller is told
    /// which: see [`Rendered`].
    pub fn process(&mut self, src: &SourceView<'_>, ratio: f64, out: &mut [[f32; 2]]) -> Rendered {
        if (ratio - 1.0).abs() < 1e-6 {
            return self.passthrough(src, out);
        }

        let mut written = 0;
        while written < out.len() {
            let remaining = out.len() - written;
            let n = remaining.min(OVERLAP);

            if !self.grain(src, ratio, n, &mut out[written..written + n]) {
                return Rendered {
                    frames: written,
                    // Out of frames with the decoder finished means the track
                    // is over; out of frames with more still coming means the
                    // decoder is behind, and the deck must wait rather than
                    // treat it as the end of the song.
                    ended: src.is_complete(),
                };
            }
            written += n;
        }
        Rendered {
            frames: written,
            ended: false,
        }
    }

    fn passthrough(&mut self, src: &SourceView<'_>, out: &mut [[f32; 2]]) -> Rendered {
        let start = self.read_pos as u64;
        let mut n = 0;
        while n < out.len() {
            let Some(f) = src.get(start + n as u64) else {
                break;
            };
            out[n] = [to_f32(f[0]), to_f32(f[1])];
            n += 1;
        }
        self.read_pos += n as f64;
        Rendered {
            frames: n,
            ended: src.is_complete() && start + n as u64 >= src.end(),
        }
    }

    /// Emit one grain of `n` frames. False when the source cannot supply it.
    fn grain(&mut self, src: &SourceView<'_>, ratio: f64, n: usize, out: &mut [[f32; 2]]) -> bool {
        if self.read_pos < 0.0 {
            return false;
        }
        let ideal = self.read_pos as u64;
        // Behind the window: the search history has been retired out from under
        // the read position. Only reachable if a deck renders long after
        // publishing where it was, and silence beats reading the wrong audio.
        if ideal < src.start() {
            return false;
        }

        // Room for the grain body plus the tail that gets stashed for the next
        // cross-fade. The search window is clamped rather than required, so a
        // grain at the very start of the file is still emitted — requiring it
        // meant the first grain always failed and the stretcher produced
        // nothing at all.
        let need = (n + OVERLAP) as u64;
        if ideal + need >= src.end() {
            return false;
        }

        let start = if self.initialised {
            self.best_offset(src, ideal, need)
        } else {
            self.initialised = true;
            ideal
        };

        // Every index below is inside `[src.start(), src.end())` by
        // construction — `best_offset` clamps to exactly that — so a missing
        // frame is unreachable rather than merely unlikely. It still reads as
        // silence instead of panicking, because this runs on the audio thread.
        let at = |i: u64| src.get(i).unwrap_or([0; 2]);

        // Cross-fade the stored tail into the head of this grain.
        for (i, o) in out.iter_mut().take(OVERLAP).enumerate() {
            let w = self.window[i];
            let f = at(start + i as u64);
            for ch in 0..2 {
                o[ch] = self.tail[i][ch] * (1.0 - w) + to_f32(f[ch]) * w;
            }
        }

        // Any frames past the overlap region come straight from the source.
        for (i, o) in out.iter_mut().enumerate().skip(OVERLAP) {
            let f = at(start + i as u64);
            *o = [to_f32(f[0]), to_f32(f[1])];
        }

        // Stash the next tail.
        let tail_start = start + n as u64;
        for i in 0..OVERLAP {
            self.tail[i] = src
                .get(tail_start + i as u64)
                .map(|f| [to_f32(f[0]), to_f32(f[1])])
                .unwrap_or([0.0; 2]);
        }

        self.read_pos += n as f64 * ratio;
        true
    }

    /// Search ±SEARCH around the ideal read position for the offset whose
    /// leading frames best correlate with the stored tail. This is the entire
    /// point of WSOLA: splicing at a waveform-similar point avoids the phase
    /// discontinuity that a naive overlap-add would produce as a click.
    ///
    /// The range is clamped to what the source can actually supply at both
    /// ends. Streaming makes the lower bound real: audio far enough behind the
    /// playhead has been retired, and correlating against a slot the decoder
    /// has since overwritten would splice in a different part of the song.
    fn best_offset(&self, src: &SourceView<'_>, ideal: u64, need: u64) -> u64 {
        let lo = ideal.saturating_sub(SEARCH as u64).max(src.start());
        let hi = (ideal + SEARCH as u64).min(src.end().saturating_sub(need + 1));
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
                let b = src.get(cand + i as u64).unwrap_or([0; 2]);
                score += a[0] * to_f32(b[0]) + a[1] * to_f32(b[1]);
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

/// The result of one call to [`Stretcher::process`].
///
/// `frames` short of what was asked for is not on its own an error, and the two
/// reasons for it are not the same thing at all:
///
/// * **The track ended.** `ended` is set; the deck stops and the player moves
///   on to whatever is next.
/// * **The decoder is behind.** `ended` is clear; the deck holds its position
///   and tries again on the next block, having produced silence for this one.
///
/// Before streaming these were indistinguishable, because a `Vec` that ran out
/// had only one possible meaning. Conflating them now would make a player skip
/// to the next song because a disk read was slow.
pub struct Rendered {
    pub frames: usize,
    pub ended: bool,
}

/// One stored sample as a float.
///
/// Divided by 32768 rather than 32767, so the conversion is an exact power of
/// two and the round trip through `from_f32` is lossless for every value it can
/// represent.
#[inline]
pub(crate) fn to_f32(sample: i16) -> f32 {
    sample as f32 / 32_768.0
}

/// One float as a stored sample.
///
/// Clamped before scaling: a value above full scale would otherwise wrap to the
/// opposite sign, turning a hot master into a burst of noise.
#[inline]
pub fn from_f32(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32_768.0).clamp(-32_768.0, 32_767.0) as i16
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
    use crate::source::Window;

    fn sine(freq: f32, secs: f32, rate: f32) -> Vec<[i16; 2]> {
        let n = (secs * rate) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / rate;
                let v = from_f32((2.0 * std::f32::consts::PI * freq * t).sin() * 0.5);
                [v, v]
            })
            .collect()
    }

    /// The conversion has to be exact in the direction that matters: whatever
    /// was stored comes back unchanged, so a passthrough is still a passthrough.
    #[test]
    fn the_sample_conversion_round_trips() {
        for raw in [i16::MIN, -32_767, -1, 0, 1, 16_384, i16::MAX] {
            assert_eq!(from_f32(to_f32(raw)), raw, "{raw} did not survive");
        }
    }

    /// A float above full scale must clamp rather than wrap, or a hot master
    /// becomes a burst of noise.
    #[test]
    fn conversion_clamps_instead_of_wrapping() {
        assert_eq!(from_f32(2.0), i16::MAX);
        assert_eq!(from_f32(-2.0), i16::MIN);
        assert!(from_f32(1.0) > 0, "full scale flipped sign");
    }

    #[test]
    fn unity_ratio_is_bit_exact_passthrough() {
        let src = sine(440.0, 1.0, 44100.0);
        let mut s = Stretcher::new();
        let mut out = vec![[0.0f32; 2]; 4096];
        let r = s.process(&SourceView::Memory(&src), 1.0, &mut out);
        assert_eq!(r.frames, 4096);
        let expected: Vec<[f32; 2]> = src[..4096]
            .iter()
            .map(|f| [to_f32(f[0]), to_f32(f[1])])
            .collect();
        assert_eq!(&out[..], &expected[..]);
    }

    /// The property that matters for beat-matching: output duration scales by
    /// the ratio, so a deck asked to run 2% fast consumes 2% more source.
    #[test]
    fn source_consumption_tracks_the_ratio() {
        for &ratio in &[0.94f64, 0.98, 1.02, 1.06] {
            let src = sine(220.0, 10.0, 44100.0);
            let mut s = Stretcher::new();
            let mut out = vec![[0.0f32; 2]; 44100];
            let r = s.process(&SourceView::Memory(&src), ratio, &mut out);
            assert_eq!(r.frames, out.len(), "ratio {ratio} produced a short buffer");

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
        let r = s.process(&SourceView::Memory(&src), 1.02, &mut out);

        // A 220 Hz sine at amplitude 0.5 slews at most ~0.016 per sample.
        // Allow generous headroom; a real click is an order of magnitude worse.
        let max_step = 0.15f32;
        for i in 1..r.frames {
            let d = (out[i][0] - out[i - 1][0]).abs();
            assert!(d < max_step, "discontinuity {d:.3} at frame {i}");
        }
    }

    #[test]
    fn stops_cleanly_at_end_of_source() {
        let src = sine(440.0, 0.2, 44100.0);
        let mut s = Stretcher::new();
        let mut out = vec![[0.0f32; 2]; 44100];
        let r = s.process(&SourceView::Memory(&src), 1.03, &mut out);
        assert!(r.frames < out.len(), "should run out of source");
        assert!(r.frames > 0, "should produce something before running out");
        assert!(
            r.ended,
            "running off the end of a whole track is the end of it"
        );
    }

    /// A window is not a queue: the search reads backwards, and the audio it
    /// reaches for must be the audio that was actually there.
    ///
    /// Stretching the same signal through a window and through the whole buffer
    /// has to give the same samples, or streaming changes how the mix sounds.
    #[test]
    fn stretching_through_a_window_matches_the_whole_buffer() {
        const BLOCK: usize = 512;
        let src = sine(220.0, 10.0, 44_100.0);
        let ratio = 1.02;

        // Both sides are rendered a block at a time, because the stretcher's
        // grains follow the block length — a fair comparison has to drive them
        // identically, and a player always does.
        let mut whole = Stretcher::new();
        let mut whole_out = Vec::new();
        let mut block = [[0.0f32; 2]; BLOCK];
        loop {
            let r = whole.process(&SourceView::Memory(&src), ratio, &mut block);
            whole_out.extend_from_slice(&block[..r.frames]);
            if r.frames < BLOCK {
                break;
            }
        }

        // The same audio, seen through a window a decoder keeps just ahead of
        // the playhead — deliberately small, so it wraps many times.
        let window = Window::with_capacity(1 << 14);
        let mut streamed = Stretcher::new();
        let mut streamed_out = Vec::new();
        let mut written = 0usize;

        while streamed_out.len() < whole_out.len() {
            // Keep the producer ahead, exactly as the decoder thread does.
            window.publish_read(streamed.source_position() as u64);
            let room = window.writable().min(src.len() - written);
            if room > 0 {
                written += window.write_frames(&src[written..written + room]);
            }
            if written == src.len() {
                window.set_complete();
            }

            let r = streamed.process(
                &SourceView::Stream(window.view(), window.is_complete()),
                ratio,
                &mut block,
            );
            assert!(
                r.frames > 0 || r.ended,
                "the window starved with the whole track available to fill it"
            );
            streamed_out.extend_from_slice(&block[..r.frames]);
            if r.ended {
                break;
            }
        }

        assert_eq!(
            streamed_out.len(),
            whole_out.len(),
            "streaming produced a different number of frames"
        );
        if let Some(i) = (0..whole_out.len()).find(|&i| streamed_out[i] != whole_out[i]) {
            panic!(
                "stretching through a window diverged at frame {i} of {}: \
                 {:?} through the window, {:?} through the whole buffer",
                whole_out.len(),
                streamed_out[i],
                whole_out[i]
            );
        }
    }

    /// Starving must be distinguishable from finishing. A player that confused
    /// them would skip to the next song because a disk read was slow.
    #[test]
    fn an_empty_window_starves_rather_than_ends() {
        let window = Window::with_capacity(1 << 14);
        let src = sine(220.0, 1.0, 44_100.0);
        window.write_frames(&src[..2048]);

        let mut s = Stretcher::new();
        let mut out = vec![[0.0f32; 2]; 8192];
        let r = s.process(
            &SourceView::Stream(window.view(), window.is_complete()),
            1.02,
            &mut out,
        );

        assert!(r.frames < out.len(), "more was produced than was written");
        assert!(
            !r.ended,
            "a decoder that has not caught up was reported as the end of the track"
        );

        // And once it truly is the end, it says so.
        window.set_complete();
        let r = s.process(
            &SourceView::Stream(window.view(), window.is_complete()),
            1.02,
            &mut out,
        );
        assert!(r.ended, "a completed window did not report the end");
    }

    /// The read position must not move when nothing was produced, or a stall
    /// would silently skip audio instead of merely delaying it.
    #[test]
    fn starving_does_not_advance_the_read_position() {
        let window = Window::with_capacity(1 << 14);
        let mut s = Stretcher::new();
        let mut out = vec![[0.0f32; 2]; 1024];

        let before = s.source_position();
        let r = s.process(&SourceView::Stream(window.view(), false), 1.02, &mut out);
        assert_eq!(r.frames, 0);
        assert_eq!(
            s.source_position(),
            before,
            "the playhead moved through audio that was never rendered"
        );
    }
}

// ---------------------------------------------------------------------------
// Choosing one
// ---------------------------------------------------------------------------

/// Which time-stretcher a deck uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// Signalsmith Stretch. The answer to MIG-012, and the default everywhere
    /// it compiles. Measured at 0.18 ms worst onset deviation across a 128 BPM
    /// transition by `beat_alignment::signalsmith_also_keeps_all_onsets_on_the_outgoing_grid`.
    Signalsmith,
    /// The WSOLA in this module. What wasm uses, since Signalsmith is C++ and
    /// does not compile there — and what the other half of an A/B is. Measured
    /// at 5.84 ms on the same transition: inside the 15 ms that reads as tight,
    /// and 32× looser than the stretcher that replaced it.
    Wsola,
}

/// Signalsmith natively; WSOLA on wasm, where the C++ will not build.
///
/// This is not a preference, it is the only choice each target has. A `cfg` on
/// a `#[default]` attribute would not express that as clearly as writing it
/// out.
impl Default for Quality {
    fn default() -> Quality {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Quality::Signalsmith
        }
        #[cfg(target_arch = "wasm32")]
        {
            Quality::Wsola
        }
    }
}

impl Quality {
    pub fn parse(s: &str) -> Quality {
        match s {
            "wsola" => Quality::Wsola,
            _ => Quality::Signalsmith,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Quality::Signalsmith => "signalsmith",
            Quality::Wsola => "wsola",
        }
    }
}

/// A deck's stretcher, whichever it is.
///
/// An enum rather than a boxed trait: this is called once per audio block, the
/// set of implementations is closed, and a virtual call on the audio thread is
/// the sort of thing that gets looked at later and cannot be explained.
pub enum Any {
    Wsola(Stretcher),
    #[cfg(not(target_arch = "wasm32"))]
    Signalsmith(crate::signalsmith::Signalsmith),
}

impl Any {
    pub fn new(quality: Quality) -> Any {
        match quality {
            #[cfg(not(target_arch = "wasm32"))]
            Quality::Signalsmith => Any::Signalsmith(crate::signalsmith::Signalsmith::new()),
            // wasm has one option, and asking for the other one there is a
            // configuration mistake rather than a reason to fail.
            #[cfg(target_arch = "wasm32")]
            Quality::Signalsmith => Any::Wsola(Stretcher::new()),
            Quality::Wsola => Any::Wsola(Stretcher::new()),
        }
    }

    pub fn quality(&self) -> Quality {
        match self {
            Any::Wsola(_) => Quality::Wsola,
            #[cfg(not(target_arch = "wasm32"))]
            Any::Signalsmith(_) => Quality::Signalsmith,
        }
    }

    pub fn reset(&mut self, position_frames: f64) {
        match self {
            Any::Wsola(s) => s.reset(position_frames),
            #[cfg(not(target_arch = "wasm32"))]
            Any::Signalsmith(s) => s.reset(position_frames),
        }
    }

    pub fn source_position(&self) -> f64 {
        match self {
            Any::Wsola(s) => s.source_position(),
            #[cfg(not(target_arch = "wasm32"))]
            Any::Signalsmith(s) => s.source_position(),
        }
    }

    pub fn process(
        &mut self,
        src: &crate::source::SourceView<'_>,
        ratio: f64,
        out: &mut [[f32; 2]],
    ) -> Rendered {
        match self {
            Any::Wsola(s) => s.process(src, ratio, out),
            #[cfg(not(target_arch = "wasm32"))]
            Any::Signalsmith(s) => s.process(src, ratio, out),
        }
    }
}

impl Default for Any {
    fn default() -> Self {
        Any::new(Quality::default())
    }
}

//! Sample rate conversion (MIG-016).
//!
//! The mixer runs at one rate. A library does not: 44.1 kHz and 48 kHz sit side
//! by side, and until now `render_mix` simply refused any pair that disagreed —
//! honest, but it means a real library cannot be mixed.
//!
//! Conversion happens off the audio thread — either once at load
//! ([`resample`]) or chunk by chunk as a track streams
//! ([`StreamResampler`]). Either way the stretcher and the decks only ever see
//! one rate, and the cost lands where latency does not matter.
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
///
/// This is [`StreamResampler`] handed the whole track at once. Writing it as a
/// second loop over the same kernel is the duplication that stays correct right
/// up until one copy is fixed — the same reason `decode.rs` has one conversion
/// table behind a `Sink` rather than one per output shape. The consequence
/// worth having is that every test below exercises the streaming path too.
pub fn resample(input: &[[f32; 2]], from_rate: u32, to_rate: u32) -> Vec<[f32; 2]> {
    if from_rate == to_rate || input.is_empty() || from_rate == 0 || to_rate == 0 {
        return input.to_vec();
    }

    let mut r = StreamResampler::new(from_rate, to_rate);
    let mut out = Vec::with_capacity(r.output_len_for(input.len() as u64) as usize);
    r.push(input, &mut out);
    r.flush(&mut out);
    out
}

/// Rate conversion for audio that arrives a chunk at a time (TD-09).
///
/// Streaming decode hands over a few thousand frames at a time, and converting
/// each chunk with [`resample`] would be wrong rather than merely inefficient:
/// the kernel reaches [`HALF_TAPS`] frames either side of each output sample,
/// and a chunk boundary treats the frames past its edge as silence. That is a
/// 32-sample notch every chunk — a buzz at the chunk rate, not a subtle loss.
///
/// So the input frames the kernel still needs are carried across calls, and an
/// output frame is emitted only once every tap that feeds it has arrived. The
/// output is therefore identical however the input happens to be divided up,
/// which is the property `chunking_does_not_change_the_output` pins.
pub struct StreamResampler {
    /// Input frames consumed per output frame.
    ratio: f64,
    /// `None` when the rates match and this is a passthrough.
    kernel: Option<Vec<Vec<f32>>>,
    /// Input frames still needed by the kernel. `held[0]` is absolute input
    /// frame `held_start`.
    held: Vec<[f32; 2]>,
    held_start: u64,
    /// Absolute input frames accepted so far.
    input_len: u64,
    /// Absolute index of the next output frame to emit.
    next_out: u64,
}

impl StreamResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> StreamResampler {
        let passthrough = from_rate == to_rate || from_rate == 0 || to_rate == 0;
        let ratio = if passthrough {
            1.0
        } else {
            from_rate as f64 / to_rate as f64
        };

        StreamResampler {
            ratio,
            kernel: if passthrough {
                None
            } else {
                // When downsampling, the kernel cutoff must follow the *output*
                // Nyquist or everything above it folds back as aliasing.
                // Upsampling keeps the input bandwidth, so the cutoff stays 1.0.
                let cutoff = if ratio > 1.0 { 1.0 / ratio } else { 1.0 } as f32;
                Some(build_kernel(cutoff))
            },
            held: Vec::new(),
            held_start: 0,
            input_len: 0,
            next_out: 0,
        }
    }

    /// Output frames a whole stream of `input_frames` converts to.
    pub fn output_len_for(&self, input_frames: u64) -> u64 {
        (input_frames as f64 / self.ratio).floor() as u64
    }

    /// Output frames emitted so far — the stream's position in its own
    /// timeline, which is what the deck counts in.
    pub fn output_position(&self) -> u64 {
        self.next_out
    }

    /// Accept input frames and emit every output frame they complete.
    ///
    /// Appends to `out` rather than returning a `Vec`: the caller owns a buffer
    /// it reuses for the life of the track, so a streaming decode allocates
    /// nothing per chunk.
    pub fn push(&mut self, input: &[[f32; 2]], out: &mut Vec<[f32; 2]>) {
        if self.kernel.is_none() {
            out.extend_from_slice(input);
            self.input_len += input.len() as u64;
            self.next_out += input.len() as u64;
            return;
        }

        self.held.extend_from_slice(input);
        self.input_len += input.len() as u64;

        // An output frame is ready only when the last tap that feeds it has
        // arrived; the rest wait for the next chunk.
        while self.taps_available(self.next_out) {
            let frame = self.interpolate(self.next_out);
            out.push(frame);
            self.next_out += 1;
        }
        self.retire();
    }

    /// No more input is coming: emit the frames whose kernels run off the end.
    ///
    /// The tail is exactly what [`resample`] produces at the end of a buffer —
    /// taps past the last frame contribute nothing — so a streamed track ends
    /// on the same sample as a loaded one.
    pub fn flush(&mut self, out: &mut Vec<[f32; 2]>) {
        if self.kernel.is_none() {
            return;
        }
        let last = self.output_len_for(self.input_len);
        while self.next_out < last {
            let frame = self.interpolate(self.next_out);
            out.push(frame);
            self.next_out += 1;
        }
        self.retire();
    }

    /// Restart at an arbitrary input frame, after the decoder has seeked.
    ///
    /// Positions stay absolute, so the output timeline still matches the beat
    /// grid that was measured against the whole file. The first
    /// [`HALF_TAPS`] frames after a seek are interpolated against the silence
    /// in front of them, the same as the first frames of a file — inaudible at
    /// 16 taps, and the alternative is decoding a run-up nobody hears.
    pub fn seek_to_input(&mut self, input_frame: u64) {
        self.held.clear();
        self.held_start = input_frame;
        self.input_len = input_frame;
        // The first output frame at or after the seek point, so output
        // positions remain a function of the source timeline alone.
        self.next_out = (input_frame as f64 / self.ratio).ceil() as u64;
    }

    /// Whether every tap feeding output frame `i` has been received.
    fn taps_available(&self, i: u64) -> bool {
        let base = (i as f64 * self.ratio).floor() as i64;
        // Taps run from `base - HALF_TAPS + 1` to `base + HALF_TAPS`. Only the
        // forward reach can be missing; anything before `held_start` is either
        // the start of the file or the far side of a seek, and reads as zero.
        (base + HALF_TAPS as i64) < self.input_len as i64
    }

    /// One output frame, from whichever taps are in hand.
    fn interpolate(&self, i: u64) -> [f32; 2] {
        let pos = i as f64 * self.ratio;
        let base = pos.floor() as i64;
        let frac = (pos - base as f64) as f32;

        // Phase index into the precomputed kernel table.
        let phase = ((frac * PHASES as f32) as usize).min(PHASES - 1);
        let taps = &self.kernel.as_ref().expect("passthrough handled above")[phase];

        let mut acc = [0.0f32, 0.0];
        for (t, &w) in taps.iter().enumerate() {
            let idx = base + t as i64 - HALF_TAPS as i64 + 1;
            // Outside the input — before its start or past its end — the sample
            // is zero, which is what dropping the tap amounts to.
            if idx < self.held_start as i64 || idx >= self.input_len as i64 {
                continue;
            }
            let s = self.held[(idx - self.held_start as i64) as usize];
            acc[0] += s[0] * w;
            acc[1] += s[1] * w;
        }
        acc
    }

    /// Drop input frames no future output frame can reach.
    fn retire(&mut self) {
        let base = (self.next_out as f64 * self.ratio).floor() as i64;
        // Never past what has actually arrived, or `held_start` would claim
        // frames the buffer does not hold.
        let oldest = (base - HALF_TAPS as i64 + 1)
            .clamp(self.held_start as i64, self.input_len as i64) as u64;
        let drop = (oldest - self.held_start) as usize;
        if drop > 0 {
            self.held.drain(..drop.min(self.held.len()));
            self.held_start = oldest;
        }
    }
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

    /// The property the streaming path exists for: how the input happens to be
    /// divided up cannot change a single output sample.
    ///
    /// Chunk sizes are deliberately irregular and mostly not multiples of
    /// anything, because a bug in the carried-over history would hide behind
    /// tidy powers of two.
    #[test]
    fn chunking_does_not_change_the_output() {
        let input = sine(1000.0, 48000, 1.0);
        let whole = resample(&input, 48000, 44100);

        let mut r = StreamResampler::new(48000, 44100);
        let mut chunked = Vec::new();
        let mut at = 0usize;
        for size in [1usize, 7, 1023, 4096, 333, 17, 65536].iter().cycle() {
            if at >= input.len() {
                break;
            }
            let end = (at + size).min(input.len());
            r.push(&input[at..end], &mut chunked);
            at = end;
        }
        r.flush(&mut chunked);

        assert_eq!(
            whole.len(),
            chunked.len(),
            "streaming produced {} frames, whole-buffer {}",
            chunked.len(),
            whole.len()
        );
        // Bit-exact, not approximate: it is the same arithmetic in the same
        // order over the same taps, so anything else means state was lost at a
        // boundary.
        assert_eq!(whole, chunked, "a chunk boundary changed the output");
    }

    /// A chunk boundary that dropped the kernel's history would notch 32
    /// samples of every chunk. On a sustained tone that is plainly visible as a
    /// level collapse, so measure it rather than trusting the equality above to
    /// have covered the audible consequence.
    #[test]
    fn a_chunk_boundary_leaves_no_hole_in_a_sustained_tone() {
        let input = sine(440.0, 48000, 0.5);
        let mut r = StreamResampler::new(48000, 44100);
        let mut out = Vec::new();
        for chunk in input.chunks(512) {
            r.push(chunk, &mut out);
        }
        r.flush(&mut out);

        // Skip the leading edge transient, then check no short span collapses.
        let skip = 64;
        for w in out[skip..out.len() - skip].chunks(64) {
            let level = rms(w);
            assert!(
                level > 0.2,
                "a {:.4} RMS span appeared mid-tone — a chunk boundary was not bridged",
                level
            );
        }
    }

    /// The buffer must not grow with the track, or streaming has simply moved
    /// the memory it was supposed to remove.
    #[test]
    fn held_input_stays_bounded_however_long_the_stream() {
        let mut r = StreamResampler::new(48000, 44100);
        let mut out = Vec::new();
        let chunk = sine(440.0, 48000, 0.1);

        for _ in 0..100 {
            out.clear();
            r.push(&chunk, &mut out);
            assert!(
                r.held.len() <= chunk.len() + HALF_TAPS * 2,
                "held {} input frames after a {}-frame chunk",
                r.held.len(),
                chunk.len()
            );
        }
    }

    /// Seeking restarts the conversion without disturbing the output timeline:
    /// a frame's position still follows from the source position alone, which
    /// is what keeps playback aligned with a beat grid measured over the file.
    #[test]
    fn seeking_resumes_on_the_same_output_timeline() {
        let mut r = StreamResampler::new(48000, 44100);
        let input_frame = 48_000u64 * 30;
        r.seek_to_input(input_frame);

        let expected = (input_frame as f64 * 44100.0 / 48000.0).ceil() as u64;
        assert_eq!(r.output_position(), expected);

        let mut out = Vec::new();
        r.push(&sine(440.0, 48000, 0.2), &mut out);
        assert!(!out.is_empty(), "nothing was produced after a seek");
        assert_eq!(
            r.output_position(),
            expected + out.len() as u64,
            "the output position and the frames produced disagree"
        );
    }

    /// Matching rates must stay a true passthrough in the streaming path too —
    /// most of a real library is already at the device rate.
    #[test]
    fn streaming_at_a_matching_rate_is_a_passthrough() {
        let input = sine(440.0, 48000, 0.2);
        let mut r = StreamResampler::new(48000, 48000);
        let mut out = Vec::new();
        for chunk in input.chunks(500) {
            r.push(chunk, &mut out);
        }
        r.flush(&mut out);
        assert_eq!(out, input);
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

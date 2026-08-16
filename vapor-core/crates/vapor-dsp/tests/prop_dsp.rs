//! Property and fuzz tests for the DSP.
//!
//! Two different jobs here.
//!
//! **Properties** hold for every input: rate conversion produces a length that
//! follows the ratio, chunking never changes the output, analysis never panics
//! on a signal whatever shape it has.
//!
//! **Fuzzing the decoder** is the one that matters most for safety. Symphonia
//! is fed whatever bytes are in a file, and the app reaches files it did not
//! write — a truncated download, a renamed `.txt`, a corrupt cache entry. None
//! of those may take the process down. There is already one known-malformed
//! file in the real library (TD-12), so this is a case that happens rather than
//! a hypothetical.

use proptest::prelude::*;
use vapor_dsp::{decode, resample};

/// A WAV whose header is valid and whose contents are whatever was generated.
fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    v.extend_from_slice(&(channels * 2).to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Arbitrary bytes must be *rejected*, never fatal.
    ///
    /// The app opens files it did not write: a truncated download, something
    /// renamed to `.m4a`, a cache entry corrupted on disk. An `Err` is a track
    /// that shows as unplayable with a reason (TD-12); a panic is the whole app
    /// going down because one file is bad.
    #[test]
    fn arbitrary_bytes_never_panic_the_decoder(
        bytes in prop::collection::vec(any::<u8>(), 0..4_096),
    ) {
        let _ = decode::decode_bytes_to_mono(bytes.clone(), None);
        let _ = decode::decode_bytes_to_stereo(bytes, Some("m4a"));
    }

    /// A file that starts out looking like a WAV and then stops is the shape a
    /// half-finished download has, and the one most likely to be mishandled.
    #[test]
    fn a_truncated_file_is_rejected_rather_than_fatal(
        samples in prop::collection::vec(any::<i16>(), 1..2_000),
        cut in 0usize..44,
    ) {
        let full = wav(44_100, 2, &samples);
        let truncated = full[..full.len().min(cut)].to_vec();
        let _ = decode::decode_bytes_to_mono(truncated, Some("wav"));
    }

    /// A header that claims one thing and a body that is another — a real
    /// corruption mode, and the one that reaches array indexing soonest.
    #[test]
    fn a_lying_header_is_rejected_rather_than_fatal(
        rate in 0u32..200_000,
        channels in 0u16..64,
        samples in prop::collection::vec(any::<i16>(), 0..500),
    ) {
        let _ = decode::decode_bytes_to_mono(wav(rate, channels, &samples), Some("wav"));
    }

    /// Rate conversion output length follows the ratio, for every pair of rates
    /// the app can meet — not only the 44.1/48 pair the examples use.
    #[test]
    fn conversion_length_follows_the_ratio(
        from in 8_000u32..192_000,
        to in 8_000u32..192_000,
        frames in 100usize..5_000,
    ) {
        let input: Vec<[f32; 2]> = (0..frames)
            .map(|i| {
                let t = i as f32 / from as f32;
                let v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                [v, v]
            })
            .collect();

        let out = resample::resample(&input, from, to);
        let expected = (frames as f64 * to as f64 / from as f64).floor() as usize;
        // Within one frame: the last output frame is emitted only when every
        // tap feeding it exists.
        prop_assert!(
            out.len().abs_diff(expected) <= 1,
            "{}->{} on {} frames gave {}, expected ~{}",
            from, to, frames, out.len(), expected
        );
    }

    /// However the input is divided up, the converted output is identical.
    ///
    /// This is the property the streaming decoder depends on: a track played
    /// through the window must sound like the same track loaded whole.
    #[test]
    fn chunking_never_changes_the_conversion(
        chunk in 1usize..3_000,
        frames in 500usize..6_000,
    ) {
        let input: Vec<[f32; 2]> = (0..frames)
            .map(|i| {
                let t = i as f32 / 44_100.0;
                let v = (t * 330.0 * std::f32::consts::TAU).sin() * 0.4;
                [v, v * 0.5]
            })
            .collect();

        let whole = resample::resample(&input, 44_100, 48_000);

        let mut streaming = resample::StreamResampler::new(44_100, 48_000);
        let mut chunked = Vec::new();
        for slice in input.chunks(chunk) {
            streaming.push(slice, &mut chunked);
        }
        streaming.flush(&mut chunked);

        prop_assert_eq!(whole, chunked, "a chunk size of {} changed the output", chunk);
    }

    /// Analysis never panics, whatever the signal.
    ///
    /// Silence, a single sample, a square wave, pure noise: the library has all
    /// of these somewhere, and an unanalysable track must report that rather
    /// than end the pass over the whole library.
    #[test]
    fn analysis_never_panics(
        samples in prop::collection::vec(-1.0f32..1.0, 0..40_000),
        rate in prop::sample::select(vec![8_000u32, 22_050, 44_100, 48_000]),
    ) {
        let audio = decode::DecodedAudio { samples, sample_rate: rate, channels: 1 };
        let _ = vapor_dsp::analyze_decoded(&audio);
    }

    /// Segment detection stays inside the track, whatever the audio.
    ///
    /// The boundaries index into the signal downstream, so one past the end is
    /// a panic waiting for the right track.
    #[test]
    fn segment_boundaries_stay_inside_the_track(
        secs in 0.01f32..40.0,
        rate in prop::sample::select(vec![22_050u32, 44_100, 48_000]),
    ) {
        let n = (secs * rate as f32) as usize;
        let samples: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / rate as f32;
                (t * 220.0 * std::f32::consts::TAU).sin() * 0.5
            })
            .collect();

        let segments = vapor_dsp::segments::detect(&samples, rate as f32);
        let duration = n as f32 / rate as f32;

        prop_assert!(segments.intro_end >= 0.0);
        prop_assert!(segments.intro_end <= duration + 0.001, "intro past the end");
        prop_assert!(segments.outro_start >= 0.0);
        prop_assert!(segments.outro_start <= duration + 0.001, "outro past the end");
        prop_assert!(segments.intro_len() >= 0.0);
        prop_assert!(segments.outro_len(duration) >= 0.0);
    }

    /// Quantising to the deck's storage format never wraps.
    ///
    /// A sample above full scale that wraps turns a hot master into a burst of
    /// noise — the loudest possible signal becoming the worst possible one.
    #[test]
    fn quantising_saturates_rather_than_wrapping(
        l in -10.0f32..10.0,
        r in -10.0f32..10.0,
    ) {
        let [ql, qr] = vapor_dsp::frame_to_i16([l, r]);
        prop_assert_eq!(ql > 0, l > 0.0, "left flipped sign at {}", l);
        prop_assert_eq!(qr > 0, r > 0.0, "right flipped sign at {}", r);
    }
}

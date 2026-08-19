//! Audio decoding via Symphonia — to mono for analysis, to stereo for playback.
//!
//! This replaces three things at once in the Godot build:
//!
//! * Essentia's `MonoLoader` / `EasyLoader`, and therefore the entire
//!   ffmpeg / taglib / chromaprint / libsamplerate dependency tail.
//! * The `popen("ffprobe ...")` channel-count probe and `system("ffmpeg ...")`
//!   downmix in `src/audio_dsp.cpp`, which hardcode Homebrew paths and cannot
//!   work on Android at any price.
//! * `_load_standard_audio_stream()` in `audio_manager.gd`, which handles only
//!   mp3/wav/ogg and so returns null for every `.m4a` — 58% of the real library.
//!
//! Downmixing happens inline over the decoded frames, so the
//! arbitrary-channel-count case (BUG-001, Dolby Atmos `.m4a`) needs no temp
//! files and no external binary.
//!
//! ## Two outputs, one conversion table
//!
//! Analysis wants mono — every DSP stage here operates on a single channel, and
//! decoding to stereo would double its memory for nothing. Playback wants
//! stereo, because a player that collapsed the stereo image would be a
//! regression a person can hear immediately.
//!
//! Both go through the same [`Sink`] trait, so the per-format sample
//! conversions — i24 scaling, the unsigned offsets — exist once. Two copies of
//! that match statement is exactly the kind of duplication that stays correct
//! until one of them is fixed.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug)]
pub struct DecodedAudio {
    /// Mono samples in [-1.0, 1.0].
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: usize,
}

impl DecodedAudio {
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / self.sample_rate as f64
    }
}

/// Stereo frames, ready for a deck.
#[derive(Debug)]
pub struct DecodedStereo {
    /// Interleaved-by-frame stereo samples in [-1.0, 1.0]. This is exactly the
    /// shape `vapor_engine::Deck::load` takes, so playback needs no repacking.
    pub frames: Vec<[f32; 2]>,
    pub sample_rate: u32,
    /// Channels the source actually carried, before downmixing.
    pub channels: usize,
}

impl DecodedStereo {
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.frames.len() as f64 / self.sample_rate as f64
    }
}

#[derive(Debug)]
pub enum DecodeError {
    Io(String),
    Unsupported(String),
    Decode(String),
    Empty,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Io(m) => write!(f, "io: {m}"),
            DecodeError::Unsupported(m) => write!(f, "unsupported: {m}"),
            DecodeError::Decode(m) => write!(f, "decode: {m}"),
            DecodeError::Empty => write!(f, "decoded to zero samples"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decode any supported file to mono f32 at its native sample rate.
///
/// Native only in practice — the browser has no filesystem. Web callers read
/// the cached bytes out of OPFS and use [`decode_bytes_to_mono`].
pub fn decode_to_mono(path: &Path) -> Result<DecodedAudio, DecodeError> {
    let mut samples: Vec<f32> = Vec::new();
    let (sample_rate, channels) = decode_stream(source_from_path(path)?, &mut Mono(&mut samples))?;
    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
    })
}

/// Decode from an in-memory buffer.
///
/// This is the entry point the wasm build uses: bytes come from OPFS or a fetch
/// response, never from a path. `ext_hint` is optional — Symphonia probes the
/// actual container regardless, which matters because the cache names files
/// after the remote href's extension and that is not guaranteed to match.
pub fn decode_bytes_to_mono(
    bytes: Vec<u8>,
    ext_hint: Option<&str>,
) -> Result<DecodedAudio, DecodeError> {
    let mut samples: Vec<f32> = Vec::new();
    let (sample_rate, channels) =
        decode_stream(source_from_bytes(bytes, ext_hint), &mut Mono(&mut samples))?;
    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
    })
}

/// Decode any supported file to stereo f32 at its native sample rate.
///
/// The playback counterpart of [`decode_to_mono`]. The offline mixing tools
/// duplicate the mono signal into both channels, which is adequate for
/// measuring beat alignment and unacceptable for listening.
pub fn decode_to_stereo(path: &Path) -> Result<DecodedStereo, DecodeError> {
    let mut frames: Vec<[f32; 2]> = Vec::new();
    let (sample_rate, channels) = decode_stream(source_from_path(path)?, &mut Stereo(&mut frames))?;
    Ok(DecodedStereo {
        frames,
        sample_rate,
        channels,
    })
}

/// Decode stereo from an in-memory buffer — the wasm playback entry point.
pub fn decode_bytes_to_stereo(
    bytes: Vec<u8>,
    ext_hint: Option<&str>,
) -> Result<DecodedStereo, DecodeError> {
    let mut frames: Vec<[f32; 2]> = Vec::new();
    let (sample_rate, channels) =
        decode_stream(source_from_bytes(bytes, ext_hint), &mut Stereo(&mut frames))?;
    Ok(DecodedStereo {
        frames,
        sample_rate,
        channels,
    })
}

/// A media source plus the format hint that goes with it.
pub struct Source {
    pub(crate) mss: MediaSourceStream,
    pub(crate) hint: Hint,
}

impl Source {
    /// Build a source over anything symphonia can read.
    ///
    /// Public so the shell can hand in something that is not a file — a track
    /// still arriving over the network, say. `ext_hint` is the file extension
    /// where one is known; symphonia probes regardless, but the hint saves it
    /// guessing and matters for containers that are easy to confuse.
    pub fn new(source: Box<dyn symphonia::core::io::MediaSource>, ext_hint: Option<&str>) -> Self {
        Source {
            mss: MediaSourceStream::new(source, Default::default()),
            hint: hint_from_extension(ext_hint),
        }
    }
}

pub fn source_from_path(path: &Path) -> Result<Source, DecodeError> {
    let file = File::open(path).map_err(|e| DecodeError::Io(e.to_string()))?;
    Ok(Source {
        mss: MediaSourceStream::new(Box::new(file), Default::default()),
        hint: hint_from_extension(path.extension().and_then(|e| e.to_str())),
    })
}

fn source_from_bytes(bytes: Vec<u8>, ext_hint: Option<&str>) -> Source {
    let cursor = std::io::Cursor::new(bytes);
    Source {
        mss: MediaSourceStream::new(Box::new(cursor), Default::default()),
        hint: hint_from_extension(ext_hint),
    }
}

fn hint_from_extension(ext: Option<&str>) -> Hint {
    let mut hint = Hint::new();
    if let Some(e) = ext {
        hint.with_extension(e);
    }
    hint
}

/// A probed file: the reader, its decoder, and what the container claims.
///
/// Shared by the whole-file decode below and by [`crate::stream`], so both
/// reach the audio through exactly the same probe options. That matters more
/// than saving a few lines: `enable_gapless` shifts where sample zero is, and
/// a playback path that disagreed with the analysis path about that would
/// place every beat in the grid slightly wrong.
pub(crate) struct Opened {
    pub(crate) format: Box<dyn symphonia::core::formats::FormatReader>,
    pub(crate) decoder: Box<dyn symphonia::core::codecs::Decoder>,
    pub(crate) track_id: u32,
    pub(crate) sample_rate: u32,
    /// Frames the container declares, when it declares any.
    pub(crate) n_frames: Option<u64>,
}

pub(crate) fn open(source: Source) -> Result<Opened, DecodeError> {
    let Source { mss, hint } = source;
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| DecodeError::Unsupported(e.to_string()))?;

    let format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| DecodeError::Unsupported("no decodable audio track".into()))?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(0);
    let n_frames = track.codec_params.n_frames;

    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| DecodeError::Unsupported(e.to_string()))?;

    Ok(Opened {
        format,
        decoder,
        track_id,
        sample_rate,
        n_frames,
    })
}

/// Decode everything in `source` into `sink`, returning the sample rate and the
/// source's channel count.
///
/// **A panic inside the codec becomes a `DecodeError`.** Symphonia's AAC
/// decoder subtracts with overflow on some malformed ADTS headers — found by
/// `arbitrary_bytes_never_panic_the_decoder`, which is a property test over
/// random bytes and had not happened to generate that shape before. It is a
/// third-party crate, so the panic cannot be fixed here; what can be fixed is
/// that a single bad file in someone's library takes down the thread decoding
/// it. Contained, it is an unplayable track with a reason attached, which is
/// machinery the app already has (TD-12).
///
/// `catch_unwind` is the right tool and a blunt one. It needs `AssertUnwindSafe`
/// because `sink` is borrowed mutably across it: a panic part-way through may
/// leave partially-decoded samples in it. That is acceptable here precisely
/// because the result is discarded on the error path — the caller gets `Err`
/// and never sees the sink.
fn decode_stream<S: Sink>(source: Source, sink: &mut S) -> Result<(u32, usize), DecodeError> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decode_stream_inner(source, sink)
    }));
    match outcome {
        Ok(result) => result,
        Err(_) => Err(DecodeError::Decode(
            "the decoder failed on this file — it is malformed in a way the codec cannot handle"
                .to_string(),
        )),
    }
}

fn decode_stream_inner<S: Sink>(source: Source, sink: &mut S) -> Result<(u32, usize), DecodeError> {
    let Opened {
        mut format,
        mut decoder,
        track_id,
        mut sample_rate,
        ..
    } = open(source)?;

    let mut channels = 0usize;
    let mut frames = 0usize;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // Symphonia signals clean end-of-stream as an io error of kind
            // UnexpectedEof; anything else is a real failure.
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(DecodeError::Decode(e.to_string())),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = *buf.spec();
                sample_rate = spec.rate;
                channels = spec.channels.count();
                frames += buf.frames();
                append(&buf, sink);
            }
            // A corrupt packet mid-file should not lose the whole track; the
            // analysis is statistical and tolerates a dropped frame.
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(DecodeError::Decode(e.to_string())),
        }
    }

    if frames == 0 {
        return Err(DecodeError::Empty);
    }

    Ok((sample_rate, channels.max(1)))
}

/// Where decoded frames go.
///
/// A trait rather than one decode loop per output shape. Both implementations
/// are monomorphised, so the abstraction costs nothing at run time — which
/// matters, because the mono path runs over a whole library.
pub(crate) trait Sink {
    fn reserve(&mut self, frames: usize);
    /// Accept one frame. `get(c)` reads channel `c`, already converted to f32.
    fn frame<F: Fn(usize) -> f32>(&mut self, channels: usize, get: F);
}

/// Mono for analysis: every channel averaged.
///
/// Averaging *all* channels, not just the front pair, is what lets an
/// arbitrary-channel-count file be analysed with no external downmix step —
/// the thing `audio_dsp.cpp` shelled out to ffmpeg for.
struct Mono<'a>(&'a mut Vec<f32>);

impl Sink for Mono<'_> {
    fn reserve(&mut self, frames: usize) {
        self.0.reserve(frames);
    }

    fn frame<F: Fn(usize) -> f32>(&mut self, channels: usize, get: F) {
        let mut acc = 0.0f32;
        for c in 0..channels {
            acc += get(c);
        }
        self.0.push(acc / channels as f32);
    }
}

/// Stereo for playback: mono is doubled, everything else takes the front pair.
///
/// Taking channels 0 and 1 from a multichannel source rather than averaging
/// them all is deliberate — averaging would fold the surrounds into the front
/// image and make a 5.1 music mix sound like a bad mono fold-down. The one
/// multichannel family in the real library is Dolby Atmos, which Symphonia
/// cannot decode at all (TD-05), so this path is presently theoretical.
pub(crate) struct Stereo<'a>(pub(crate) &'a mut Vec<[f32; 2]>);

impl Sink for Stereo<'_> {
    fn reserve(&mut self, frames: usize) {
        self.0.reserve(frames);
    }

    fn frame<F: Fn(usize) -> f32>(&mut self, channels: usize, get: F) {
        self.0.push(if channels == 1 {
            let s = get(0);
            [s, s]
        } else {
            [get(0), get(1)]
        });
    }
}

/// Feed one decoded buffer to a sink, converting whatever sample format the
/// codec produced.
pub(crate) fn append<S: Sink>(buf: &AudioBufferRef<'_>, out: &mut S) {
    macro_rules! feed {
        ($b:expr, $conv:expr) => {{
            let b = $b;
            let chans = b.spec().channels.count();
            let frames = b.frames();
            out.reserve(frames);
            for i in 0..frames {
                out.frame(chans, |c| $conv(b.chan(c)[i]));
            }
        }};
    }

    match buf {
        AudioBufferRef::F32(b) => feed!(b.as_ref(), |s: f32| s),
        AudioBufferRef::F64(b) => feed!(b.as_ref(), |s: f64| s as f32),
        AudioBufferRef::S32(b) => feed!(b.as_ref(), |s: i32| s as f32 / i32::MAX as f32),
        AudioBufferRef::S24(b) => feed!(b.as_ref(), |s: symphonia::core::sample::i24| {
            s.inner() as f32 / 8_388_607.0
        }),
        AudioBufferRef::S16(b) => feed!(b.as_ref(), |s: i16| s as f32 / i16::MAX as f32),
        AudioBufferRef::S8(b) => feed!(b.as_ref(), |s: i8| s as f32 / i8::MAX as f32),
        AudioBufferRef::U32(b) => feed!(b.as_ref(), |s: u32| {
            (s as f32 - 2_147_483_648.0) / 2_147_483_648.0
        }),
        AudioBufferRef::U24(b) => feed!(b.as_ref(), |s: symphonia::core::sample::u24| {
            (s.inner() as f32 - 8_388_608.0) / 8_388_608.0
        }),
        AudioBufferRef::U16(b) => feed!(b.as_ref(), |s: u16| (s as f32 - 32_768.0) / 32_768.0),
        AudioBufferRef::U8(b) => feed!(b.as_ref(), |s: u8| (s as f32 - 128.0) / 128.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16-bit PCM WAV with per-channel sample values, so a test can assert
    /// which channel a sample came from. Synthetic on purpose: the analysis
    /// fixtures need a personal library (TD-43) and these do not.
    fn wav(channels: u16, rate: u32, frames: &[[f32; 2]]) -> Vec<u8> {
        let data_len = (frames.len() * channels as usize * 2) as u32;
        let mut v = Vec::with_capacity(44 + data_len as usize);
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_len).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&channels.to_le_bytes());
        v.extend_from_slice(&rate.to_le_bytes());
        v.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        v.extend_from_slice(&(channels * 2).to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_len.to_le_bytes());
        for f in frames {
            for sample in f.iter().take(channels as usize) {
                let q = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                v.extend_from_slice(&q.to_le_bytes());
            }
        }
        v
    }

    /// Left and right deliberately differ, so a collapsed image is detectable.
    fn asymmetric(n: usize) -> Vec<[f32; 2]> {
        (0..n)
            .map(|i| {
                let t = i as f32 / 44_100.0;
                let l = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                let r = (t * 660.0 * std::f32::consts::TAU).sin() * 0.25;
                [l, r]
            })
            .collect()
    }

    /// The reason `decode_to_stereo` exists: the offline tools duplicate mono
    /// into both channels, and a player that did the same would silently throw
    /// away half the recording.
    #[test]
    fn stereo_decoding_keeps_the_channels_apart() {
        let frames = asymmetric(4_410);
        let decoded = decode_bytes_to_stereo(wav(2, 44_100, &frames), Some("wav")).expect("decode");

        assert_eq!(decoded.sample_rate, 44_100);
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.frames.len(), frames.len());

        let divergence = decoded
            .frames
            .iter()
            .map(|f| (f[0] - f[1]).abs())
            .fold(0.0f32, f32::max);
        assert!(
            divergence > 0.1,
            "left and right came back identical — the image was collapsed"
        );
    }

    /// Sample values must survive the round trip, not merely the shape.
    #[test]
    fn stereo_samples_match_what_was_encoded() {
        let frames = asymmetric(1_000);
        let decoded = decode_bytes_to_stereo(wav(2, 44_100, &frames), Some("wav")).expect("decode");

        for (i, (got, want)) in decoded.frames.iter().zip(&frames).enumerate() {
            for ch in 0..2 {
                // 16-bit quantisation is the only permitted difference.
                assert!(
                    (got[ch] - want[ch]).abs() < 1e-4,
                    "frame {i} channel {ch}: {} vs {}",
                    got[ch],
                    want[ch]
                );
            }
        }
    }

    /// Mono is the average of every channel. Analysis depends on this and its
    /// accuracy figures were measured with it, so a change here would silently
    /// move the validated numbers.
    #[test]
    fn mono_decoding_averages_the_channels() {
        let frames = asymmetric(1_000);
        let bytes = wav(2, 44_100, &frames);
        let mono = decode_bytes_to_mono(bytes, Some("wav")).expect("decode");

        assert_eq!(mono.samples.len(), frames.len());
        for (i, (got, want)) in mono.samples.iter().zip(&frames).enumerate() {
            let expected = (want[0] + want[1]) / 2.0;
            assert!(
                (got - expected).abs() < 1e-4,
                "sample {i}: {got} vs {expected}"
            );
        }
    }

    /// A mono source must reach both speakers, not just the left one.
    #[test]
    fn a_mono_source_is_doubled_into_both_channels() {
        let frames: Vec<[f32; 2]> = (0..500)
            .map(|i| {
                let v = (i as f32 / 44_100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                [v, v]
            })
            .collect();
        let decoded = decode_bytes_to_stereo(wav(1, 44_100, &frames), Some("wav")).expect("decode");

        assert_eq!(decoded.channels, 1);
        assert!(
            decoded.frames.iter().all(|f| f[0] == f[1]),
            "a mono source produced differing channels"
        );
        assert!(
            decoded.frames.iter().any(|f| f[0].abs() > 0.1),
            "a mono source decoded to silence"
        );
    }

    /// Both sinks read the same stream; disagreeing about its length would
    /// mean a track whose analysis and playback are offset from each other.
    #[test]
    fn both_sinks_agree_on_length_and_rate() {
        let frames = asymmetric(2_000);
        let bytes = wav(2, 48_000, &frames);

        let mono = decode_bytes_to_mono(bytes.clone(), Some("wav")).expect("mono");
        let stereo = decode_bytes_to_stereo(bytes, Some("wav")).expect("stereo");

        assert_eq!(mono.samples.len(), stereo.frames.len());
        assert_eq!(mono.sample_rate, stereo.sample_rate);
        assert_eq!(mono.sample_rate, 48_000);
    }
}

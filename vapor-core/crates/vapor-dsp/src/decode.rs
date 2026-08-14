//! Audio decoding to mono f32, via Symphonia.
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
//! Downmixing to mono happens inline over the decoded frames, so the
//! arbitrary-channel-count case (BUG-001, Dolby Atmos `.m4a`) needs no temp
//! files and no external binary.

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
    let file = File::open(path).map_err(|e| DecodeError::Io(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let hint = hint_from_extension(path.extension().and_then(|e| e.to_str()));
    decode_stream(mss, hint)
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
    let source = std::io::Cursor::new(bytes);
    let mss = MediaSourceStream::new(Box::new(source), Default::default());
    decode_stream(mss, hint_from_extension(ext_hint))
}

fn hint_from_extension(ext: Option<&str>) -> Hint {
    let mut hint = Hint::new();
    if let Some(e) = ext {
        hint.with_extension(e);
    }
    hint
}

fn decode_stream(mss: MediaSourceStream, hint: Hint) -> Result<DecodedAudio, DecodeError> {
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

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| DecodeError::Unsupported("no decodable audio track".into()))?;
    let track_id = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| DecodeError::Unsupported(e.to_string()))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut channels = 0usize;

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
                append_mono(&buf, &mut samples);
            }
            // A corrupt packet mid-file should not lose the whole track; the
            // analysis is statistical and tolerates a dropped frame.
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(DecodeError::Decode(e.to_string())),
        }
    }

    if samples.is_empty() {
        return Err(DecodeError::Empty);
    }

    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels: channels.max(1),
    })
}

/// Average all channels of one decoded buffer into the mono output.
fn append_mono(buf: &AudioBufferRef<'_>, out: &mut Vec<f32>) {
    macro_rules! mix {
        ($b:expr, $conv:expr) => {{
            let b = $b;
            let chans = b.spec().channels.count();
            let frames = b.frames();
            out.reserve(frames);
            if chans == 1 {
                let ch = b.chan(0);
                out.extend(ch[..frames].iter().map(|&s| $conv(s)));
            } else {
                let inv = 1.0 / chans as f32;
                for i in 0..frames {
                    let mut acc = 0.0f32;
                    for c in 0..chans {
                        acc += $conv(b.chan(c)[i]);
                    }
                    out.push(acc * inv);
                }
            }
        }};
    }

    match buf {
        AudioBufferRef::F32(b) => mix!(b.as_ref(), |s: f32| s),
        AudioBufferRef::F64(b) => mix!(b.as_ref(), |s: f64| s as f32),
        AudioBufferRef::S32(b) => mix!(b.as_ref(), |s: i32| s as f32 / i32::MAX as f32),
        AudioBufferRef::S24(b) => mix!(b.as_ref(), |s: symphonia::core::sample::i24| {
            s.inner() as f32 / 8_388_607.0
        }),
        AudioBufferRef::S16(b) => mix!(b.as_ref(), |s: i16| s as f32 / i16::MAX as f32),
        AudioBufferRef::S8(b) => mix!(b.as_ref(), |s: i8| s as f32 / i8::MAX as f32),
        AudioBufferRef::U32(b) => mix!(b.as_ref(), |s: u32| {
            (s as f32 - 2_147_483_648.0) / 2_147_483_648.0
        }),
        AudioBufferRef::U24(b) => mix!(b.as_ref(), |s: symphonia::core::sample::u24| {
            (s.inner() as f32 - 8_388_608.0) / 8_388_608.0
        }),
        AudioBufferRef::U16(b) => mix!(b.as_ref(), |s: u16| (s as f32 - 32_768.0) / 32_768.0),
        AudioBufferRef::U8(b) => mix!(b.as_ref(), |s: u8| (s as f32 - 128.0) / 128.0),
    }
}

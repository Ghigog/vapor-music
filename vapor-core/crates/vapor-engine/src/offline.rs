//! Offline helpers shared by the `render` and `play` binaries.
//!
//! Decoding, analysing and scheduling a transition between two real files is
//! the same work whether the result is written to disk or sent to a device, so
//! it lives here rather than being duplicated in two `main`s.
//!
//! This is developer tooling, not the player's runtime path — a real player
//! streams rather than decoding whole tracks into memory.

use std::path::Path;

use crate::mixer::BeatGrid;
use crate::{Mixer, TransitionType};

const BLOCK: usize = 512;
/// How much of the outgoing track to play before the mix starts.
pub const LEAD_IN_SECS: f32 = 12.0;
/// How long to keep rendering after the mix completes.
pub const TAIL_SECS: f32 = 6.0;

#[derive(Debug)]
pub enum OfflineError {
    Decode(String),
    Analysis(String),
    RateMismatch(u32, u32),
    /// Carries the two BPMs so the caller can see *why* — a bare
    /// "TempoTooFar" hides whether the tracks really are incompatible or the
    /// analysis simply got one of them wrong.
    NotMatchable {
        err: crate::MatchError,
        bpm_a: f32,
        bpm_b: f32,
    },
}

impl std::fmt::Display for OfflineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OfflineError::Decode(m) => write!(f, "cannot decode: {m}"),
            OfflineError::Analysis(m) => write!(f, "cannot analyse: {m}"),
            OfflineError::RateMismatch(a, b) => write!(
                f,
                "sample rate mismatch ({a} vs {b}); resampling is not implemented yet"
            ),
            OfflineError::NotMatchable { err, bpm_a, bpm_b } => write!(
                f,
                "cannot beat-match {bpm_a:.1} -> {bpm_b:.1} BPM ({:+.1}%): {err:?}\n\
                 If those BPMs look wrong, pass --bpm-a / --bpm-b to override \
                 the detector (see MIG-001/MIG-002).",
                (bpm_b / bpm_a - 1.0) * 100.0
            ),
        }
    }
}

pub fn parse_kind(s: Option<&str>) -> Option<TransitionType> {
    match s.unwrap_or("crossfade") {
        "crossfade" => Some(TransitionType::StandardCrossfade),
        "bassswap" => Some(TransitionType::BassSwap),
        "filtersweep" => Some(TransitionType::FilterSweep),
        _ => None,
    }
}

pub struct Track {
    pub frames: Vec<[f32; 2]>,
    pub sample_rate: u32,
    pub grid: BeatGrid,
}

/// Decode and analyse one track, producing the beat grid the mixer needs.
///
/// `bpm_override` bypasses the detector. That exists because the mixer's
/// correctness and the analyser's accuracy are separate concerns: a refused
/// transition should be diagnosable as "these tracks really are incompatible"
/// rather than "the BPM estimate was wrong", and mixing quality can be judged
/// before MIG-001/MIG-002 land.
pub fn load_track(path: &Path, bpm_override: Option<f32>) -> Result<Track, OfflineError> {
    let mono =
        vapor_dsp::decode::decode_to_mono(path).map_err(|e| OfflineError::Decode(e.to_string()))?;
    let analysis =
        vapor_dsp::analyze_decoded(&mono).map_err(|e| OfflineError::Analysis(e.to_string()))?;

    // `decode_to_mono` gives the analysis signal; duplicating to stereo keeps
    // this tooling simple. A real player keeps the stereo decode.
    let frames: Vec<[f32; 2]> = mono.samples.iter().map(|&s| [s, s]).collect();

    let bpm = bpm_override.unwrap_or(analysis.bpm);

    // Prefer the tracked beat grid (MIG-002). It follows real tempo drift and
    // real downbeat phase, neither of which a BPM alone can express.
    //
    // The synthesised fallback — beats every 60/bpm from t=0 — is only reached
    // when tracking failed, or when a BPM override makes the tracked grid
    // inconsistent with the requested tempo. It is a poor substitute: it
    // assumes the first beat sits exactly at zero and the tempo never wavers,
    // and both are false for real music.
    let beats = if !analysis.beats.is_empty() && bpm_override.is_none() {
        analysis.beats.clone()
    } else {
        let period = 60.0 / bpm;
        let count = (mono.duration_secs() as f32 / period) as usize;
        (0..count).map(|i| i as f32 * period).collect()
    };

    Ok(Track {
        frames,
        sample_rate: mono.sample_rate,
        grid: BeatGrid { bpm, beats },
    })
}

pub struct MixResult {
    pub samples: Vec<[f32; 2]>,
    pub sample_rate: u32,
    pub start_at: f32,
    pub ratio: f64,
    pub peak: f32,
    pub bpm_a: f32,
    pub bpm_b: f32,
}

/// Decode both tracks, schedule a beat-matched transition, and render the whole
/// thing — lead-in, mix and tail — into one buffer.
pub fn render_mix(
    a_path: &Path,
    b_path: &Path,
    kind: TransitionType,
    duration: f32,
    bpm_a: Option<f32>,
    bpm_b: Option<f32>,
) -> Result<MixResult, OfflineError> {
    let a = load_track(a_path, bpm_a)?;
    let b = load_track(b_path, bpm_b)?;

    if a.sample_rate != b.sample_rate {
        return Err(OfflineError::RateMismatch(a.sample_rate, b.sample_rate));
    }
    let rate = a.sample_rate;
    let ratio = Mixer::tempo_ratio(&a.grid, &b.grid).map_err(|err| OfflineError::NotMatchable {
        err,
        bpm_a: a.grid.bpm,
        bpm_b: b.grid.bpm,
    })?;

    let (a_bpm, b_bpm) = (a.grid.bpm, b.grid.bpm);
    let mut mixer = Mixer::new(rate as f32, BLOCK);
    let a_duration = a.frames.len() as f32 / rate as f32;
    mixer.deck_a.load(a.frames);
    mixer.deck_b.load(b.frames);

    // Start the mix a sensible way into the outgoing track, without running
    // past its end.
    let start_at = (a_duration - duration - 2.0)
        .min(LEAD_IN_SECS + duration)
        .max(1.0);
    let deck_a_from = (start_at - LEAD_IN_SECS).max(0.0);

    mixer.deck_a.seek_seconds(deck_a_from as f64);
    mixer.deck_a.play();

    let mut out: Vec<[f32; 2]> = Vec::new();
    let mut block = [[0.0f32; 2]; BLOCK];

    let lead_blocks = ((start_at - deck_a_from) * rate as f32 / BLOCK as f32) as usize;
    for _ in 0..lead_blocks {
        mixer.render(&mut block);
        out.extend_from_slice(&block);
    }

    let cue_in = b.grid.beats.first().copied().unwrap_or(0.0).max(1.0);
    mixer
        .start_transition(kind, duration, &a.grid, &b.grid, start_at, cue_in)
        .map_err(|err| OfflineError::NotMatchable {
            err,
            bpm_a: a.grid.bpm,
            bpm_b: b.grid.bpm,
        })?;

    let mix_blocks = ((duration + TAIL_SECS) * rate as f32 / BLOCK as f32) as usize;
    for _ in 0..mix_blocks {
        mixer.render(&mut block);
        out.extend_from_slice(&block);
    }

    let peak = out
        .iter()
        .flat_map(|s| [s[0].abs(), s[1].abs()])
        .fold(0.0f32, f32::max);

    Ok(MixResult {
        samples: out,
        sample_rate: rate,
        start_at,
        ratio,
        peak,
        bpm_a: a_bpm,
        bpm_b: b_bpm,
    })
}

/// Minimal 16-bit PCM WAV writer — avoids a dependency for something this small.
pub fn write_wav(path: &Path, samples: &[[f32; 2]], rate: u32) -> std::io::Result<()> {
    let data_len = (samples.len() * 2 * 2) as u32;
    let mut buf = Vec::with_capacity(44 + data_len as usize);

    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&2u16.to_le_bytes()); // channels
    buf.extend_from_slice(&rate.to_le_bytes());
    buf.extend_from_slice(&(rate * 4).to_le_bytes()); // byte rate
    buf.extend_from_slice(&4u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());

    for s in samples {
        for v in s {
            let q = (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            buf.extend_from_slice(&q.to_le_bytes());
        }
    }

    std::fs::write(path, buf)
}

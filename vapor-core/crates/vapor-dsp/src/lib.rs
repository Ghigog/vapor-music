//! `vapor-dsp` — platform-independent audio analysis for Vapor Music.
//!
//! This crate exists to answer one question before the migration commits to
//! anything: can the analysis that currently only works on macOS (Essentia via
//! a GDExtension, linking the whole ffmpeg stack and shelling out to Homebrew
//! binaries) be reproduced in portable Rust that also compiles to wasm?
//!
//! It deliberately has no audio I/O, no engine and no platform code — the whole
//! crate is `cargo test`-able on any CI runner, which is the property the
//! current 224-test GUT suite lacks.

pub mod decode;
pub mod key;
pub mod spectrum;
pub mod tempo;

use std::path::Path;

#[derive(Debug, Clone)]
pub struct Analysis {
    pub bpm: f32,
    pub camelot: String,
    pub key_name: String,
    pub key_confidence: f32,
    pub duration_secs: f64,
    pub sample_rate: u32,
    pub channels: usize,
}

#[derive(Debug)]
pub enum AnalysisError {
    Decode(decode::DecodeError),
    /// Decoded, but too short or too quiet to analyse.
    Insufficient,
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalysisError::Decode(e) => write!(f, "{e}"),
            AnalysisError::Insufficient => write!(f, "insufficient audio to analyse"),
        }
    }
}

/// Analyse a file end to end.
///
/// Analysis runs on a downmixed mono signal. Long tracks are analysed from a
/// bounded interior window: tempo and key are global properties, and reading
/// every sample of a 10-minute track buys nothing but import time.
pub fn analyze_file(path: &Path) -> Result<Analysis, AnalysisError> {
    let audio = decode::decode_to_mono(path).map_err(AnalysisError::Decode)?;
    analyze_decoded(&audio)
}

/// Analyse from an in-memory buffer — the entry point for the wasm build,
/// where audio comes from OPFS or a fetch rather than a path.
pub fn analyze_bytes(bytes: Vec<u8>, ext_hint: Option<&str>) -> Result<Analysis, AnalysisError> {
    let audio = decode::decode_bytes_to_mono(bytes, ext_hint).map_err(AnalysisError::Decode)?;
    analyze_decoded(&audio)
}

/// Longest span actually fed to the FFT. Beyond this the estimate stops
/// improving and import time keeps growing.
const ANALYSIS_WINDOW_SECS: f64 = 120.0;
/// Skipped at the start, to step over fade-ins, silence and spoken intros.
const ANALYSIS_SKIP_SECS: f64 = 15.0;

pub fn analyze_decoded(audio: &decode::DecodedAudio) -> Result<Analysis, AnalysisError> {
    let rate = audio.sample_rate;
    if rate == 0 || audio.samples.len() < rate as usize {
        return Err(AnalysisError::Insufficient);
    }

    let total = audio.samples.len();
    let skip = ((ANALYSIS_SKIP_SECS * rate as f64) as usize).min(total / 8);
    let want = (ANALYSIS_WINDOW_SECS * rate as f64) as usize;
    let end = (skip + want).min(total);
    let slice = &audio.samples[skip..end];

    // Two transforms, not one: tempo needs time resolution and key needs
    // frequency resolution. See the `spectrum` module docs.
    let tempo_spec = spectrum::for_tempo(slice, rate);
    let tempo = tempo::estimate(&tempo_spec).ok_or(AnalysisError::Insufficient)?;

    let key_spec = spectrum::for_key(slice, rate);
    let key = key::estimate(&key_spec).ok_or(AnalysisError::Insufficient)?;

    Ok(Analysis {
        bpm: tempo.bpm,
        camelot: key.camelot,
        key_name: key.name,
        key_confidence: key.confidence,
        duration_secs: audio.duration_secs(),
        sample_rate: rate,
        channels: audio.channels,
    })
}

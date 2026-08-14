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

pub mod beats;
pub mod decode;
pub mod key;
pub mod loudness;
pub mod spectrum;
pub mod tempo;

use std::path::Path;

#[derive(Debug, Clone)]
pub struct Analysis {
    pub bpm: f32,
    /// Beat times in seconds, covering the whole track, so they index the
    /// original file directly and can be handed to `vapor_engine::BeatGrid`.
    /// Empty when the signal was too short or too sparse to track.
    pub beats: Vec<f32>,
    pub camelot: String,
    pub key_name: String,
    pub key_confidence: f32,
    /// First and last audible moments, in seconds. `get_transition_trigger_time`
    /// in the Godot build schedules mixes from these, so they are load-bearing
    /// rather than informational.
    pub cue_in: f32,
    pub cue_out: f32,
    /// Integrated loudness, EBU R128 / ITU-R BS.1770.
    pub lufs: f32,
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
/// Runs on a downmixed mono signal. Beat tracking covers the whole track;
/// tempo and key are picked from a bounded interior window, since both are
/// global properties and the window avoids fade-ins and run-outs.
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

/// Span of the onset function used to pick the tempo. Beats still come from
/// the whole track; only the tempo estimate is windowed.
const TEMPO_WINDOW_SECS: f64 = 120.0;
const TEMPO_SKIP_SECS: f64 = 15.0;

/// Longest span fed to the *key* transform. Key is a global property, the
/// 8192-point FFT it needs is the expensive one, and the estimate stops
/// improving well before a whole track has been read.
const KEY_WINDOW_SECS: f64 = 120.0;
/// Skipped at the start of the key window, to step over fade-ins, silence and
/// spoken intros.
const KEY_SKIP_SECS: f64 = 15.0;

pub fn analyze_decoded(audio: &decode::DecodedAudio) -> Result<Analysis, AnalysisError> {
    let rate = audio.sample_rate;
    if rate == 0 || audio.samples.len() < rate as usize {
        return Err(AnalysisError::Insufficient);
    }

    // Beats are tracked over the WHOLE track.
    //
    // Windowing them was a real defect, caught by measuring beat F-measure
    // against Essentia: a 120 s window over a 289 s track caps recall at ~0.41
    // however good the tracking is. The product consequence is worse than the
    // metric — a windowed grid has no beats near the outro, which is precisely
    // where transitions get scheduled.
    //
    // Tempo is picked from a representative middle span of the same onset
    // function; see `tempo::estimate_windowed` for why the two differ.
    let tempo_spec = spectrum::for_tempo(&audio.samples, rate);
    let tempo = tempo::estimate_windowed(
        &tempo_spec,
        TEMPO_SKIP_SECS as f32,
        TEMPO_WINDOW_SECS as f32,
    )
    .ok_or(AnalysisError::Insufficient)?;
    let beats = beats::track(&tempo.odf, tempo.odf_rate, tempo.bpm)
        .map(|g| g.beats)
        .unwrap_or_default();

    // Key keeps its window. Note this is a *different* transform anyway —
    // tempo needs time resolution, key needs frequency resolution — so nothing
    // is shared and nothing is wasted. See the `spectrum` module docs.
    let total = audio.samples.len();
    let skip = ((KEY_SKIP_SECS * rate as f64) as usize).min(total / 8);
    let want = (KEY_WINDOW_SECS * rate as f64) as usize;
    let end = (skip + want).min(total);
    let key_spec = spectrum::for_key(&audio.samples[skip..end], rate);
    let key = key::estimate(&key_spec).ok_or(AnalysisError::Insufficient)?;

    let (cue_in, cue_out) = loudness::cue_points(&audio.samples, rate as f32);
    let lufs = loudness::integrated_lufs(&audio.samples, rate as f32);

    Ok(Analysis {
        bpm: tempo.bpm,
        beats,
        cue_in,
        cue_out,
        lufs,
        camelot: key.camelot,
        key_name: key.name,
        key_confidence: key.confidence,
        duration_secs: audio.duration_secs(),
        sample_rate: rate,
        channels: audio.channels,
    })
}

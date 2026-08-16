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
pub mod resample;
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
    /// Camelot key of the track's opening and closing spans (TD-13).
    ///
    /// The pathfinder prefers these to the whole-track key, because a mix
    /// happens at the *seam*: what matters is the key the outgoing track ends
    /// in and the one the incoming track starts in, which on a track that
    /// modulates are not the whole-track answer. Empty when the track is too
    /// short for a segment to mean anything separate from the whole.
    pub intro_key: String,
    pub outro_key: String,
    /// Perceived energy, 0–1.
    ///
    /// A dynamics ratio, not a loudness — see [`loudness::energy_level`], which
    /// is the `audio_dsp.cpp` measure ported unchanged.
    pub energy: f32,
    /// Envelope peaks across the whole track, for drawing a waveform.
    ///
    /// A fixed number of bins rather than a fixed resolution, so a five-minute
    /// track and a ninety-second one cost the same to store and draw the same
    /// way. [`WAVEFORM_BINS`] is comfortably more than the design's 52 bars,
    /// which lets a wider screen subsample rather than interpolate.
    pub waveform: Vec<f32>,
}

/// Envelope bins stored per track. See [`Analysis::waveform`].
pub const WAVEFORM_BINS: usize = 64;

/// Span read for the intro and outro keys, in seconds.
///
/// Long enough to establish a tonality and short enough to still be "the
/// opening" of a five-minute track. Both are measured from the audible content
/// rather than the file, so a long silent lead-in does not become the intro.
const SEGMENT_SECS: f64 = 45.0;

/// Below this a track has no meaningful segments — the intro, the outro and the
/// whole are all the same music, and reporting three keys would imply a
/// distinction that is not there.
const MIN_SEGMENTED_SECS: f64 = SEGMENT_SECS * 3.0;

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

/// Decode a file into deck-ready stereo frames at `target_rate`.
///
/// The two steps a player needs before audio can reach a device, in the order
/// that keeps the audio thread simple: decode to stereo, then convert the rate
/// **once, here**, rather than per block. The output stream runs at one fixed
/// rate for the life of the app — reconfiguring a device between tracks is
/// audible and, on some hosts, not possible at all — while a real library holds
/// 44.1 and 48 kHz side by side.
///
/// Lives here rather than in the shell because the browser build needs exactly
/// the same two steps before handing frames to an AudioWorklet (MIG-013).
pub fn decode_for_playback(
    path: &Path,
    target_rate: u32,
) -> Result<Vec<[f32; 2]>, decode::DecodeError> {
    let decoded = decode::decode_to_stereo(path)?;
    Ok(resample::resample(
        &decoded.frames,
        decoded.sample_rate,
        target_rate,
    ))
}

/// Decode bytes into deck-ready stereo frames — the wasm counterpart of
/// [`decode_for_playback`].
pub fn decode_bytes_for_playback(
    bytes: Vec<u8>,
    ext_hint: Option<&str>,
    target_rate: u32,
) -> Result<Vec<[f32; 2]>, decode::DecodeError> {
    let decoded = decode::decode_bytes_to_stereo(bytes, ext_hint)?;
    Ok(resample::resample(
        &decoded.frames,
        decoded.sample_rate,
        target_rate,
    ))
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

    // Segment keys are read from the audible span, not the file: a track with
    // ten seconds of silence at the front would otherwise have an "intro key"
    // estimated from nothing.
    let duration = audio.duration_secs();
    let (intro_key, outro_key) = if duration >= MIN_SEGMENTED_SECS {
        let sample_at = |secs: f64| ((secs * rate as f64) as usize).min(total);
        let intro_from = sample_at(cue_in as f64);
        let intro_to = sample_at(cue_in as f64 + SEGMENT_SECS);
        let outro_to = sample_at(cue_out as f64);
        let outro_from = sample_at((cue_out as f64 - SEGMENT_SECS).max(cue_in as f64));

        let segment = |from: usize, to: usize| -> String {
            if to <= from {
                return String::new();
            }
            key::estimate(&spectrum::for_key(&audio.samples[from..to], rate))
                .map(|k| k.camelot)
                .unwrap_or_default()
        };
        (segment(intro_from, intro_to), segment(outro_from, outro_to))
    } else {
        (String::new(), String::new())
    };

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
        intro_key,
        outro_key,
        energy: loudness::energy_level(&audio.samples, rate as f32),
        // Computed from the same decoded signal every other stage used, so it
        // costs a pass over memory rather than another decode.
        waveform: loudness::waveform_peaks(&audio.samples, WAVEFORM_BINS),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A track shorter than three segments has no separate intro or outro, and
    /// claiming otherwise would imply a distinction that is not there.
    #[test]
    fn a_short_track_reports_no_segment_keys() {
        let rate = 44_100u32;
        let secs = 30.0;
        let samples: Vec<f32> = (0..(rate as f64 * secs) as usize)
            .map(|i| {
                let t = i as f32 / rate as f32;
                // A C major triad, so key estimation has something to find.
                ((t * 261.63 * std::f32::consts::TAU).sin()
                    + (t * 329.63 * std::f32::consts::TAU).sin()
                    + (t * 392.00 * std::f32::consts::TAU).sin())
                    * 0.2
            })
            .collect();

        let audio = decode::DecodedAudio {
            samples,
            sample_rate: rate,
            channels: 1,
        };
        let result = analyze_decoded(&audio).expect("analyse");
        assert!(result.intro_key.is_empty());
        assert!(result.outro_key.is_empty());
        assert!(
            !result.camelot.is_empty(),
            "the whole-track key still works"
        );
    }

    /// A long track gets segment keys, and on one that never modulates they
    /// agree with the whole — which is the case that proves the windows are
    /// pointed at real audio rather than at silence.
    #[test]
    fn a_long_unmodulating_track_agrees_with_itself() {
        let rate = 44_100u32;
        let secs = 180.0;
        let samples: Vec<f32> = (0..(rate as f64 * secs) as usize)
            .map(|i| {
                let t = i as f32 / rate as f32;
                ((t * 261.63 * std::f32::consts::TAU).sin()
                    + (t * 329.63 * std::f32::consts::TAU).sin()
                    + (t * 392.00 * std::f32::consts::TAU).sin())
                    * 0.2
            })
            .collect();

        let audio = decode::DecodedAudio {
            samples,
            sample_rate: rate,
            channels: 1,
        };
        let result = analyze_decoded(&audio).expect("analyse");

        assert!(
            !result.intro_key.is_empty(),
            "no intro key on a 3-minute track"
        );
        assert!(
            !result.outro_key.is_empty(),
            "no outro key on a 3-minute track"
        );
        assert_eq!(
            result.intro_key, result.outro_key,
            "a track that never modulates reported two different segment keys"
        );
    }
}

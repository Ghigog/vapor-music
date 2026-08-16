//! Where a track's intro ends and its outro begins (TD-21, MIG-009).
//!
//! Ported from the segment-detection block in `src/audio_dsp.cpp`, which is
//! plain portable DSP — no Essentia — and was simply never carried across.
//!
//! ## What it is for
//!
//! `get_transition_duration` picks how long a mix runs from how much room the
//! two tracks give it: the shorter of the outgoing track's outro and the
//! incoming track's intro, snapped down to a musical phrase. Without these
//! boundaries that whole path is unreachable, and every mix falls back to the
//! per-type default — which is why the durations shipped so far are 3 to 6
//! seconds regardless of the music.
//!
//! ## How a boundary is found
//!
//! Three signals have to agree, which is what stops a quiet passage mid-track
//! reading as an outro:
//!
//! * **Level.** The window is above 15% of the track's peak RMS.
//! * **Brightness.** Its mean spectral centroid is above 800 Hz — a filtered
//!   or pad-only intro sits below that even when it is loud.
//! * **Rhythm.** At least three transients across this window and the next two,
//!   so something is actually playing a beat.
//!
//! The intro ends at the first second where all three hold; the outro starts
//! three seconds after the last one. Both are then clamped into sane bounds,
//! because a track whose whole body is quiet would otherwise report an intro
//! covering half of it.

use crate::spectrum;

/// Analysis window, in seconds. The C++ aggregates everything to one-second
/// buckets and the thresholds below are tuned against that.
const WINDOW_SECS: f32 = 1.0;

/// FFT size and hop, from the original.
const FFT_SIZE: usize = 1024;
const HOP: usize = 512;

/// Level a window must exceed, as a fraction of the track's peak RMS.
const LEVEL_FRACTION: f32 = 0.15;

/// Mean spectral centroid a window must exceed, in Hz.
const BRIGHTNESS_HZ: f32 = 800.0;

/// Transients required across a window and the two after it.
const TRANSIENT_DENSITY: usize = 3;

/// Onset strength threshold, as a fraction of the loudest onset — with a floor,
/// because on a quiet track 5% of very little is noise.
const ONSET_FRACTION: f32 = 0.05;
const ONSET_FLOOR: f32 = 10.0;

/// Seconds after the last rhythmic window that the outro is taken to start.
const OUTRO_LAG_SECS: f32 = 3.0;

/// Longest an intro may be, in seconds, and as a fraction of the track.
const INTRO_MAX_SECS: f32 = 60.0;
const INTRO_MAX_FRACTION: f32 = 0.25;

/// Earliest an outro may start, as a fraction of the track, and how far before
/// the end it must begin.
const OUTRO_MIN_FRACTION: f32 = 0.70;
const OUTRO_TAIL_SECS: f32 = 5.0;

/// Defaults when the track is too short to analyse, from the original.
const DEFAULT_INTRO_END: f32 = 30.0;
const DEFAULT_OUTRO_BEFORE_END: f32 = 30.0;

/// The spans a transition can use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segments {
    /// The intro runs `[0, intro_end)`.
    pub intro_end: f32,
    /// The outro runs `[outro_start, duration)`.
    pub outro_start: f32,
}

impl Segments {
    pub fn intro_len(&self) -> f32 {
        self.intro_end.max(0.0)
    }

    pub fn outro_len(&self, duration: f32) -> f32 {
        (duration - self.outro_start).max(0.0)
    }
}

/// Find the intro and outro boundaries of a mono signal.
pub fn detect(samples: &[f32], sample_rate: f32) -> Segments {
    let duration = samples.len() as f32 / sample_rate;
    let mut intro_end = DEFAULT_INTRO_END;
    let mut outro_start = duration - DEFAULT_OUTRO_BEFORE_END;

    let window = (WINDOW_SECS * sample_rate) as usize;
    let windows = samples.len().checked_div(window).unwrap_or(0);

    if windows >= 3 && samples.len() >= FFT_SIZE {
        let rms = rms_envelope(samples, window, windows);
        let peak = rms.iter().copied().fold(0.0f32, f32::max);

        let (centroid, transients) = per_window_features(samples, sample_rate, window, windows);

        let rhythmic = |i: usize| -> bool {
            let density: usize = transients[i..(i + 3).min(transients.len())].iter().sum();
            rms[i] > peak * LEVEL_FRACTION
                && centroid[i] > BRIGHTNESS_HZ
                && density >= TRANSIENT_DENSITY
        };

        // The intro ends where the track first sounds like it has started.
        for i in 0..windows.saturating_sub(2) {
            if rhythmic(i) {
                intro_end = i as f32 * WINDOW_SECS;
                break;
            }
        }

        // The outro starts a few seconds after the track last sounds that way.
        for i in (0..windows.saturating_sub(2)).rev() {
            if rhythmic(i) {
                outro_start = i as f32 * WINDOW_SECS + OUTRO_LAG_SECS;
                break;
            }
        }
    }

    // Clamped so a track that never reads as rhythmic still reports plausible
    // spans rather than an intro covering half of it.
    let intro_high = INTRO_MAX_SECS.min(duration * INTRO_MAX_FRACTION).max(0.0);
    let outro_low = duration * OUTRO_MIN_FRACTION;
    let outro_high = outro_low.max(duration - OUTRO_TAIL_SECS);

    Segments {
        intro_end: intro_end.clamp(0.0, intro_high),
        outro_start: outro_start.clamp(outro_low, outro_high),
    }
}

/// RMS per one-second window.
fn rms_envelope(samples: &[f32], window: usize, windows: usize) -> Vec<f32> {
    (0..windows)
        .map(|i| {
            let slice = &samples[i * window..(i + 1) * window];
            (slice.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / window as f64).sqrt()
                as f32
        })
        .collect()
}

/// Mean spectral centroid and transient count for each one-second window.
///
/// Transients come from the first-order difference of high-frequency content,
/// which is a different detector from [`crate::loudness::transients`] and
/// deliberately so: that one finds a track's *strongest hits* for display,
/// this one asks whether a second of audio contains rhythmic activity at all.
fn per_window_features(
    samples: &[f32],
    sample_rate: f32,
    window: usize,
    windows: usize,
) -> (Vec<f32>, Vec<usize>) {
    let spec = spectrum::compute(samples, sample_rate as u32, FFT_SIZE, HOP);
    let frames = spec.frames.len();

    let mut frame_centroid = vec![0.0f32; frames];
    let mut frame_hfc = vec![0.0f32; frames];

    for (f, frame) in spec.frames.iter().enumerate() {
        let mut numerator = 0.0f64;
        let mut denominator = 0.0f64;
        let mut hfc = 0.0f64;
        for (k, &magnitude) in frame.iter().enumerate() {
            let magnitude = magnitude as f64;
            numerator += spec.bin_freq(k) as f64 * magnitude;
            denominator += magnitude;
            // Weighted by bin index rather than frequency, as in the original:
            // it is an onset detector, not a measurement, and the weighting
            // only has to favour the top of the spectrum.
            hfc += k as f64 * magnitude;
        }
        frame_centroid[f] = if denominator > 1e-6 {
            (numerator / denominator) as f32
        } else {
            0.0
        };
        frame_hfc[f] = hfc as f32;
    }

    // Onset strength: how much high-frequency content just arrived.
    let mut onset = vec![0.0f32; frames];
    for f in 1..frames {
        onset[f] = (frame_hfc[f] - frame_hfc[f - 1]).max(0.0);
    }
    let loudest = onset.iter().copied().fold(0.0f32, f32::max);
    let threshold = (loudest * ONSET_FRACTION).max(ONSET_FLOOR);

    let mut centroid_sum = vec![0.0f32; windows];
    let mut centroid_count = vec![0usize; windows];
    let mut transients = vec![0usize; windows];

    for f in 0..frames {
        let index = f * HOP / window;
        if index >= windows {
            continue;
        }
        centroid_sum[index] += frame_centroid[f];
        centroid_count[index] += 1;

        // A local peak above the threshold, which is what distinguishes an
        // onset from a swell.
        if f > 0
            && f + 1 < frames
            && onset[f] > threshold
            && onset[f] > onset[f - 1]
            && onset[f] > onset[f + 1]
        {
            transients[index] += 1;
        }
    }

    let centroid = (0..windows)
        .map(|i| {
            if centroid_count[i] > 0 {
                centroid_sum[i] / centroid_count[i] as f32
            } else {
                0.0
            }
        })
        .collect();

    (centroid, transients)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 44_100.0;

    /// A track shaped like a track: a quiet pad, a rhythmic body, a quiet tail.
    fn shaped(intro: f32, body: f32, outro: f32) -> Vec<f32> {
        let mut out = Vec::new();

        // Intro: a low sine, no percussion — loud enough to be audible but dark
        // and rhythmless, which is exactly the case the brightness and density
        // conditions exist to reject.
        for i in 0..(intro * RATE) as usize {
            let t = i as f32 / RATE;
            out.push((t * 110.0 * std::f32::consts::TAU).sin() * 0.4);
        }

        // Body: a bass line, a sustained bright component standing in for the
        // hats and cymbals any drum kit puts across the spectrum, and a click
        // every half second for the rhythm. The bright component has to be
        // *sustained* — a percussive transient occupies 2% of a window, so
        // clicks alone leave the mean centroid down near the bass note and the
        // section reads as an intro.
        for i in 0..(body * RATE) as usize {
            let t = i as f32 / RATE;
            let mut v = (t * 220.0 * std::f32::consts::TAU).sin() * 0.3
                + (t * 2500.0 * std::f32::consts::TAU).sin() * 0.25;
            let into_beat = t % 0.5;
            if into_beat < 0.01 {
                let env = 1.0 - into_beat / 0.01;
                v += env * (t * 6000.0 * std::f32::consts::TAU).sin() * 0.7;
            }
            out.push(v);
        }

        for i in 0..(outro * RATE) as usize {
            let t = i as f32 / RATE;
            out.push((t * 110.0 * std::f32::consts::TAU).sin() * 0.4);
        }

        out
    }

    /// The boundaries must land on the rhythmic section, not on the pads either
    /// side of it. This is the whole job.
    #[test]
    fn boundaries_bracket_the_rhythmic_section() {
        let audio = shaped(20.0, 60.0, 20.0);
        let duration = audio.len() as f32 / RATE;
        let s = detect(&audio, RATE);

        assert!(
            s.intro_end >= 15.0 && s.intro_end <= 25.0,
            "the intro ended at {:.1}s, expected around 20s",
            s.intro_end
        );
        assert!(
            s.outro_start >= 75.0 && s.outro_start <= 90.0,
            "the outro started at {:.1}s, expected around 80s (duration {duration:.1}s)",
            s.outro_start
        );
    }

    /// A quiet, dark intro must not be mistaken for the body just because it is
    /// audible — that is what the brightness condition is for.
    #[test]
    fn a_loud_but_dark_intro_is_still_an_intro() {
        let audio = shaped(15.0, 60.0, 15.0);
        let s = detect(&audio, RATE);
        assert!(
            s.intro_end > 8.0,
            "a dark rhythmless intro was treated as the body at {:.1}s",
            s.intro_end
        );
    }

    /// Boundaries are clamped, so even a track that never reads as rhythmic
    /// reports spans a transition can use rather than nonsense.
    #[test]
    fn boundaries_stay_within_their_bounds() {
        // Pure silence: nothing anywhere passes the level test.
        let quiet = vec![0.0f32; (RATE * 120.0) as usize];
        let duration = quiet.len() as f32 / RATE;
        let s = detect(&quiet, RATE);

        assert!(s.intro_end >= 0.0 && s.intro_end <= duration * INTRO_MAX_FRACTION + 0.01);
        assert!(s.outro_start >= duration * OUTRO_MIN_FRACTION - 0.01);
        assert!(s.outro_start <= duration);
        assert!(
            s.outro_start > s.intro_end,
            "the outro started before the intro ended"
        );
    }

    /// A track too short to hold three windows must not panic or index past the
    /// end — a ninety-second interlude and a two-second sting go through the
    /// same path.
    #[test]
    fn a_very_short_track_is_handled() {
        for secs in [0.05f32, 0.5, 1.0, 2.5] {
            let audio = vec![0.1f32; (RATE * secs) as usize];
            let s = detect(&audio, RATE);
            assert!(s.intro_end.is_finite() && s.outro_start.is_finite());
            assert!(s.intro_end >= 0.0);
        }
        assert!(detect(&[], RATE).intro_end >= 0.0);
    }

    /// The lengths a transition actually asks for.
    #[test]
    fn segment_lengths_are_reported_from_the_boundaries() {
        let s = Segments {
            intro_end: 12.0,
            outro_start: 200.0,
        };
        assert_eq!(s.intro_len(), 12.0);
        assert_eq!(s.outro_len(220.0), 20.0);
        // A duration before the outro start cannot produce a negative span.
        assert_eq!(s.outro_len(100.0), 0.0);
    }
}

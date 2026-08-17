//! Bar-level metre — how many beats to a bar, and which beat is the first.
//!
//! [`crate::beats`] says *where* the beats are. It does not say which of them
//! is beat one, and nothing downstream has ever known: `phrase_duration` picks
//! a transition length of 4, 8 or 16 bars from `240 / bpm`, which is a whole
//! number of bars long and can still begin in the middle of one. A mix that is
//! eight bars long but starts on beat three lands the incoming track's downbeat
//! on the outgoing track's backbeat, which is the thing a DJ would notice first
//! and the beat grid cannot see.
//!
//! ## This does not work on real music, and nothing uses it
//!
//! **Read this before wiring it into anything.** It is correct on synthetic
//! audio and no better than guessing on the library, so `Analysis` deliberately
//! does not carry a downbeat field. It is kept because the measurement is worth
//! more than the code, and because the next attempt should start from the
//! numbers rather than from the same idea.
//!
//! The premise: [`crate::tempo`]'s onset function spans 30 Hz – 5 kHz and
//! whitens, so the kick is one voice among many in it. A **bass-band** onset
//! function is therefore evidence the beat tracker has never used, and the kick
//! lands on the downbeat. That is true of a great deal of music and **false of
//! four-on-the-floor**, where the kick lands on *every* beat and so
//! distinguishes no phase at all — which is most of this library.
//!
//! Measured by `bin/validate.rs` against Essentia's own downbeats, on the 26
//! tracks whose beat grids already agree (where they do not, the two are not
//! describing the same pulse and a downbeat comparison measures nothing):
//!
//! | | Result |
//! |---|---|
//! | bar length found | 4/4 on all 24, correct |
//! | downbeat F-measure | mean 0.194, median 0.006 |
//! | F ≥ 0.8 | 3 of 24 (12.5%) |
//! | chance, for a 4-phase choice | 25% |
//!
//! Below chance. Two features were tried: the bass onset strength above, and
//! spectral novelty between consecutive beats — the arrangement changing at the
//! bar line, which does not depend on where the kick falls. Novelty scored
//! mean 0.213 against 0.194, still half of chance, and it broke the one case
//! the detector got right for the right reason: a track with no percussion,
//! which it then claimed a metre for. So the bass-only version is what remains,
//! and neither is worth shipping.
//!
//! [`Metre::contrast`] was meant to be the escape hatch — report a bar only
//! when confident. It is not one: the two highest-contrast tracks in the set
//! (1.739 and 1.732) score F = 0.011 and 0.021. Confidence is uncorrelated with
//! correctness, so there is no threshold at which this becomes honest.
//!
//! ## What would be worth trying next
//!
//! Not another onset feature. The downbeat in four-on-the-floor is carried by
//! harmony and arrangement — the bassline changing note, the 4-, 8- and 16-bar
//! phrase structure — rather than by any single beat being louder. That is a
//! self-similarity problem over beat-synchronous features, not a peak-picking
//! one. The fixtures carry Essentia `downbeats` for all 563 tracks, so the
//! harness exists; only 44 of them have local audio, which is the other half of
//! why this could not be pushed further.
//!
//! ## It does not touch the tempo either
//!
//! The bass band does look promising there — on the six tracks where tempo
//! estimation picks a metrically related answer it prefers the true tempo on
//! four and is neutral on two, while the broadband comb prefers the wrong one
//! on all six. But on the 38 tracks that already come out right its margin is
//! +0.018 median against a spread of −0.357 to +0.342, so a blend tuned on 44
//! tracks with six errors and no held-out set would be fitting noise. Two
//! earlier attempts at exactly that were made and reverted; see
//! `docs/FINDINGS.md`.

use crate::spectrum::Spectrogram;

/// Bar lengths considered, in beats.
///
/// Four first and three second, and the order matters — see [`PREFER_FOUR`].
/// Five and seven exist in music and not in this library; adding them costs
/// accuracy on 4/4 to serve a case that would still be wrong for other reasons.
const CANDIDATE_BARS: [usize; 2] = [4, 3];

/// How much better a 3-beat bar must score than a 4-beat one to be believed.
///
/// A waltz and a 4/4 track with a weak third beat produce a similar pattern,
/// and 4/4 is overwhelmingly more common here. Requiring 3 to win outright by a
/// margin means the failure mode is "a waltz read as 4/4", which costs one
/// misplaced downbeat every other bar, rather than "half the library read as a
/// waltz", which costs everything.
const PREFER_FOUR: f32 = 1.15;

/// Bass band, in Hz. The kick's fundamental and first harmonic, below where the
/// snare and most of the bassline live.
const BASS_LO_HZ: f32 = 30.0;
const BASS_HI_HZ: f32 = 120.0;

/// Below this contrast the metre is a guess and is reported as absent.
///
/// The score is the mean bass onset strength on the chosen downbeats divided by
/// the mean across all beats, so 1.0 means the downbeats are indistinguishable
/// from any other beat. Ambient music with no percussion lands here, and it
/// should: there is no downbeat to find and inventing one puts every transition
/// on an arbitrary boundary while claiming it is musical.
const MIN_CONTRAST: f32 = 1.08;

/// Where the bars are.
#[derive(Clone, Debug, PartialEq)]
pub struct Metre {
    /// Beats to a bar — 4 or 3.
    pub beats_per_bar: usize,
    /// Index of the first downbeat in the beat list.
    pub phase: usize,
    /// Downbeat times in seconds.
    pub downbeats: Vec<f32>,
    /// Mean bass onset strength on the downbeats over the mean on all beats.
    ///
    /// Carried rather than thresholded away, so a caller can be more demanding
    /// than [`MIN_CONTRAST`] without re-running anything.
    pub contrast: f32,
}

/// Find the bar given a beat grid.
///
/// `None` when there is no bar to find: too few beats to see a pattern, or a
/// track whose downbeats carry no more weight than any other beat.
pub fn detect(spec: &Spectrogram, beats: &[f32]) -> Option<Metre> {
    // Four bars of 4 is the least that can distinguish a pattern from a
    // coincidence. Below it, any phase fits.
    if beats.len() < 16 || spec.frames.is_empty() {
        return None;
    }

    let strength = bass_onset_per_beat(spec, beats);
    let mean: f32 = strength.iter().sum::<f32>() / strength.len() as f32;
    if mean <= 0.0 {
        return None;
    }

    let mut best: Option<(usize, usize, f32)> = None;
    for &bar in &CANDIDATE_BARS {
        if beats.len() < bar * 4 {
            continue;
        }
        let (phase, contrast) = best_phase(&strength, bar, mean);
        // The comparison is against whatever four scored, so a three only wins
        // by clearing it outright.
        let adjusted = if bar == 3 {
            contrast / PREFER_FOUR
        } else {
            contrast
        };
        if best.is_none_or(|(_, _, b)| adjusted > b) {
            best = Some((bar, phase, adjusted));
        }
    }

    let (beats_per_bar, phase, _) = best?;
    // Report the contrast that was actually measured, not the one biased for
    // the comparison — the bias exists to choose between bar lengths, and
    // passing it on would understate a waltz's confidence to a caller.
    let (_, contrast) = best_phase(&strength, beats_per_bar, mean);
    if contrast < MIN_CONTRAST {
        return None;
    }

    let downbeats = beats
        .iter()
        .skip(phase)
        .step_by(beats_per_bar)
        .copied()
        .collect();

    Some(Metre {
        beats_per_bar,
        phase,
        downbeats,
        contrast,
    })
}

/// The phase whose beats carry the most bass onset, and by how much.
fn best_phase(strength: &[f32], bar: usize, mean: f32) -> (usize, f32) {
    let mut best = (0usize, 0.0f32);
    for phase in 0..bar {
        let hits: Vec<f32> = strength.iter().skip(phase).step_by(bar).copied().collect();
        if hits.is_empty() {
            continue;
        }
        let avg = hits.iter().sum::<f32>() / hits.len() as f32;
        let contrast = avg / mean;
        if contrast > best.1 {
            best = (phase, contrast);
        }
    }
    best
}

/// Bass-band onset strength at each beat.
///
/// Measured over a short window *starting* at the beat rather than centred on
/// it: a kick is an attack, its energy is after the onset and not before, and a
/// centred window pulls in the tail of the previous beat.
fn bass_onset_per_beat(spec: &Spectrogram, beats: &[f32]) -> Vec<f32> {
    let flux = bass_flux(spec);
    let rate = spec.frame_rate();

    // A sixth of the gap to the next beat, so the window scales with tempo and
    // never reaches the following beat at any of them.
    let default_gap = if beats.len() > 1 {
        beats[1] - beats[0]
    } else {
        0.5
    };

    beats
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            let gap = beats.get(i + 1).map_or(default_gap, |next| next - t);
            let from = (t * rate).round().max(0.0) as usize;
            let to = ((t + gap / 6.0) * rate).round().max(0.0) as usize;
            let to = to.clamp(from + 1, flux.len());
            if from >= flux.len() {
                return 0.0;
            }
            flux[from..to].iter().cloned().fold(0.0f32, f32::max)
        })
        .collect()
}

/// Positive frame-to-frame change in log magnitude, bass band only.
fn bass_flux(spec: &Spectrogram) -> Vec<f32> {
    let bins = spec.frames[0].len();
    let bin_of = |hz: f32| ((hz / spec.bin_freq(1)).round() as usize).clamp(1, bins.max(2) - 1);
    let lo = bin_of(BASS_LO_HZ);
    let hi = bin_of(BASS_HI_HZ).max(lo + 1);

    let mut flux = Vec::with_capacity(spec.frames.len());
    flux.push(0.0);
    for i in 1..spec.frames.len() {
        let (prev, cur) = (&spec.frames[i - 1], &spec.frames[i]);
        let mut sum = 0.0f32;
        for k in lo..hi.min(bins) {
            let d = (1.0 + cur[k]).ln() - (1.0 + prev[k]).ln();
            if d > 0.0 {
                sum += d;
            }
        }
        flux.push(sum);
    }
    flux
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Audio with a kick on a known beat of the bar and a hat on the rest.
    ///
    /// Synthetic because it is the only case where the answer is known by
    /// construction rather than by another estimator's opinion — the kick is at
    /// 60 Hz and the hats are at 4 kHz, so a detector reading the bass band has
    /// exactly one right answer and no reference to agree with.
    fn pattern(bpm: f32, bars: usize, beats_per_bar: usize, downbeat: usize) -> Vec<f32> {
        let rate = 44_100.0f32;
        let period = 60.0 / bpm;
        let total = ((bars * beats_per_bar) as f32 * period * rate) as usize + 4410;
        let mut s = vec![0.0f32; total];

        for i in 0..bars * beats_per_bar {
            let is_downbeat = i % beats_per_bar == downbeat;
            let start = (i as f32 * period * rate) as usize;
            let (freq, amp, decay) = if is_downbeat {
                (60.0, 1.0, 12.0)
            } else {
                (4000.0, 0.28, 90.0)
            };
            let len = (rate * 0.12) as usize;
            for k in 0..len.min(total - start) {
                let t = k as f32 / rate;
                s[start + k] += amp * (-decay * t).exp() * (std::f32::consts::TAU * freq * t).sin();
            }
        }
        s
    }

    fn beats_of(bpm: f32, count: usize) -> Vec<f32> {
        let period = 60.0 / bpm;
        (0..count).map(|i| i as f32 * period).collect()
    }

    #[test]
    fn finds_four_four_and_the_downbeat() {
        for downbeat in 0..4 {
            let audio = pattern(128.0, 12, 4, downbeat);
            let spec = crate::spectrum::for_tempo(&audio, 44_100);
            let beats = beats_of(128.0, 48);

            let m = detect(&spec, &beats).expect("a metre");
            assert_eq!(m.beats_per_bar, 4, "downbeat {downbeat}");
            assert_eq!(m.phase, downbeat, "downbeat {downbeat}");
            assert!(m.contrast > 1.2, "contrast {} too weak", m.contrast);
        }
    }

    /// The downbeats returned are the beats themselves, not a synthesised grid
    /// — a bar every four beats of a grid that already follows tempo drift.
    #[test]
    fn downbeats_are_drawn_from_the_beat_grid() {
        let audio = pattern(128.0, 12, 4, 2);
        let spec = crate::spectrum::for_tempo(&audio, 44_100);
        let beats = beats_of(128.0, 48);

        let m = detect(&spec, &beats).expect("a metre");
        assert_eq!(m.downbeats.len(), (48usize - 2).div_ceil(4));
        assert_eq!(m.downbeats[0], beats[2]);
        assert_eq!(m.downbeats[1], beats[6]);
        assert!(m.downbeats.iter().all(|d| beats.contains(d)));
    }

    #[test]
    fn finds_a_three_beat_bar() {
        let audio = pattern(120.0, 16, 3, 0);
        let spec = crate::spectrum::for_tempo(&audio, 44_100);
        let beats = beats_of(120.0, 48);

        let m = detect(&spec, &beats).expect("a metre");
        assert_eq!(m.beats_per_bar, 3);
        assert_eq!(m.phase, 0);
    }

    /// No percussion means no downbeat, and saying so is the point.
    ///
    /// Inventing one would put every transition on an arbitrary boundary while
    /// claiming it is a musical one, which is worse than admitting there is
    /// nothing to align to.
    #[test]
    fn a_track_with_no_kick_reports_no_metre() {
        let rate = 44_100.0f32;
        let n = (rate * 20.0) as usize;
        // A sustained chord: real audio, no transients, nothing periodic in the
        // bass band to mistake for a bar.
        let audio: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / rate;
                0.2 * ((std::f32::consts::TAU * 220.0 * t).sin()
                    + (std::f32::consts::TAU * 277.0 * t).sin())
            })
            .collect();
        let spec = crate::spectrum::for_tempo(&audio, 44_100);
        let beats = beats_of(120.0, 40);

        assert_eq!(detect(&spec, &beats), None);
    }

    #[test]
    fn too_few_beats_to_see_a_bar_is_not_a_guess() {
        let audio = pattern(128.0, 2, 4, 0);
        let spec = crate::spectrum::for_tempo(&audio, 44_100);
        assert_eq!(detect(&spec, &beats_of(128.0, 8)), None);
    }
}

//! Beat tracking — producing actual beat positions, not just a tempo.
//!
//! A BPM alone is not enough to mix with. `vapor-engine` needs to know *where*
//! the beats are: `Mixer::aligned_incoming_position` phase-locks the incoming
//! deck to a specific beat, and until this module existed the engine's offline
//! tooling had to fake a grid by assuming the first beat sits at t=0 and the
//! tempo never wanders. Both assumptions are false for real music.
//!
//! ## Method
//!
//! Dynamic programming beat tracking, after Ellis (2007), *Beat Tracking by
//! Dynamic Programming*. Given the onset detection function and a target beat
//! period, find the beat sequence maximising
//!
//! ```text
//!   sum(onset strength at each beat)  +  tightness * sum(period consistency)
//! ```
//!
//! The DP formulation matters: greedy peak-picking on the ODF locks onto the
//! loudest transients, which in dance music are often offbeat snares or
//! syncopated stabs. The consistency term is what makes the tracker prefer a
//! steady pulse over a louder but irregular one, and it lets the grid follow
//! genuine tempo drift instead of either snapping to it or ignoring it.

/// Weight of period consistency against onset strength.
///
/// Higher values force a more rigid grid. Chosen by measuring F-measure against
/// 563 Essentia beat grids rather than by ear — see `tools/` and the validator.
const TIGHTNESS: f32 = 100.0;

pub struct BeatGrid {
    /// Beat times in seconds, relative to the start of the analysed signal.
    pub beats: Vec<f32>,
}

/// Track beats in `odf` (sampled at `odf_rate` Hz) around a target `bpm`.
///
/// Returns `None` when the signal is too short to contain a useful number of
/// beats.
pub fn track(odf: &[f32], odf_rate: f32, bpm: f32) -> Option<BeatGrid> {
    if bpm <= 0.0 || odf.len() < 16 {
        return None;
    }

    let period = 60.0 / bpm * odf_rate;
    if !period.is_finite() || period < 2.0 || period as usize >= odf.len() {
        return None;
    }

    // Local score: cumulative best score of a beat sequence ending at t.
    let n = odf.len();
    let mut score = vec![f32::NEG_INFINITY; n];
    // Backpointer to the previous beat.
    let mut prev = vec![usize::MAX; n];

    // Candidate inter-beat intervals to consider, from half to double the
    // target period. Wider than any real tempo drift, so the penalty rather
    // than the bound is what constrains the result.
    let lo_gap = (period * 0.5).round().max(1.0) as usize;
    let hi_gap = (period * 2.0).round() as usize;

    // Precompute the consistency penalty per gap: -(ln(gap/period))^2, which is
    // zero at the target period and falls off symmetrically in log-space, so a
    // half-period and a double-period error are penalised equally.
    let penalty: Vec<f32> = (0..=hi_gap)
        .map(|g| {
            if g == 0 {
                f32::NEG_INFINITY
            } else {
                let r = (g as f32 / period).ln();
                -(r * r)
            }
        })
        .collect();

    for t in 0..n {
        // A beat can start the sequence if it is within the first period —
        // otherwise every grid would be forced to begin at the very start of
        // the signal, whatever the audio does.
        let mut best = if t < period.ceil() as usize {
            odf[t]
        } else {
            f32::NEG_INFINITY
        };
        let mut best_prev = usize::MAX;

        if t >= lo_gap {
            let first = t.saturating_sub(hi_gap);
            let last = t - lo_gap;
            for (tau, &s) in score.iter().enumerate().take(last + 1).skip(first) {
                if s == f32::NEG_INFINITY {
                    continue;
                }
                let cand = s + TIGHTNESS * penalty[t - tau];
                if cand > best {
                    best = cand;
                    best_prev = tau;
                }
            }
            if best_prev != usize::MAX {
                best += odf[t];
            }
        }

        score[t] = best;
        prev[t] = best_prev;
    }

    // Start the backtrace from the best-scoring beat in the final period, not
    // from the global maximum: the cumulative score grows with sequence length,
    // so the global max is almost always near the end anyway, but restricting
    // it avoids truncating the grid when the tail is quiet.
    let tail_start = n.saturating_sub(period.ceil() as usize);
    let mut end = tail_start;
    let mut best_end = f32::NEG_INFINITY;
    for (t, &s) in score.iter().enumerate().skip(tail_start) {
        if s > best_end {
            best_end = s;
            end = t;
        }
    }
    if best_end == f32::NEG_INFINITY {
        return None;
    }

    let mut frames = Vec::new();
    let mut cur = end;
    while cur != usize::MAX {
        frames.push(cur);
        cur = prev[cur];
    }
    frames.reverse();

    if frames.len() < 4 {
        return None;
    }

    Some(BeatGrid {
        beats: frames.iter().map(|&f| f as f32 / odf_rate).collect(),
    })
}

/// F-measure of `detected` against `reference`, with a tolerance window.
///
/// The standard beat-tracking metric: a detected beat counts as correct if some
/// reference beat lies within `tolerance` seconds, each reference beat matching
/// at most once. 70 ms is the conventional window.
pub fn f_measure(detected: &[f32], reference: &[f32], tolerance: f32) -> f32 {
    if detected.is_empty() || reference.is_empty() {
        return 0.0;
    }

    let mut used = vec![false; reference.len()];
    let mut hits = 0usize;

    for &d in detected {
        // Reference grids are sorted, so a linear scan with early exit is
        // enough; this runs over whole libraries, not per audio frame.
        let mut best: Option<(usize, f32)> = None;
        for (i, &r) in reference.iter().enumerate() {
            if used[i] {
                continue;
            }
            let err = (d - r).abs();
            if err <= tolerance && best.is_none_or(|(_, b)| err < b) {
                best = Some((i, err));
            }
            if r > d + tolerance {
                break;
            }
        }
        if let Some((i, _)) = best {
            used[i] = true;
            hits += 1;
        }
    }

    let precision = hits as f32 / detected.len() as f32;
    let recall = hits as f32 / reference.len() as f32;
    if precision + recall <= 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic ODF with impulses at a known period, plus a decoy offbeat
    /// that is *louder* than the real beats. Greedy peak-picking follows the
    /// decoys; the DP tracker should not.
    fn odf_with_offbeat_decoys(period: usize, n: usize, offset: usize) -> Vec<f32> {
        let mut odf = vec![0.0f32; n];
        let mut t = offset;
        while t < n {
            odf[t] = 1.0;
            let off = t + period / 2;
            if off < n {
                odf[off] = 1.4; // louder than the beat itself
            }
            t += period;
        }
        odf
    }

    #[test]
    fn recovers_a_known_pulse() {
        let rate = 86.13f32; // odf frames per second at 44.1 kHz / hop 512
        let bpm = 120.0;
        let period = (60.0 / bpm * rate).round() as usize;
        let n = period * 40;

        let mut odf = vec![0.0f32; n];
        let mut t = 5;
        while t < n {
            odf[t] = 1.0;
            t += period;
        }

        let grid = track(&odf, rate, bpm).expect("grid");
        assert!(grid.beats.len() > 30, "got {} beats", grid.beats.len());

        // Every detected beat should sit on the true pulse.
        let reference: Vec<f32> = (0..)
            .map(|i| (5 + i * period) as f32 / rate)
            .take_while(|&s| s < n as f32 / rate)
            .collect();
        let f = f_measure(&grid.beats, &reference, 0.07);
        assert!(f > 0.95, "F-measure {f:.3} on a clean pulse");
    }

    /// Re-tracking at a corrected tempo lands on the right half of the beats.
    ///
    /// The case a hand correction is usually for: a track whose real pulse is
    /// 128 but which has audible eighths, so detection reported 256 and built
    /// its grid on every eighth. Correcting to 128 means keeping every other
    /// beat — and *which* other one is not something arithmetic can decide.
    /// Take the wrong set and every beat lands exactly off, which is the worst
    /// possible answer rather than a near miss.
    ///
    /// The tracker decides it from onset strength, so this asserts both halves
    /// of the claim: the grid matches the strong pulse, and does not match the
    /// weak one it could just as easily have chosen.
    #[test]
    fn re_tracking_at_half_tempo_picks_the_strong_pulse() {
        let rate = 86.13f32;
        let real_bpm = 128.0;
        let period = (60.0 / real_bpm * rate).round() as usize;
        let n = period * 60;

        // Strong beats at 128, weaker eighths between them — enough to make a
        // tempo estimator say 256, not enough to be the beat.
        let mut odf = vec![0.0f32; n];
        let offset = 9;
        let mut t = offset;
        while t < n {
            odf[t] = 1.0;
            let eighth = t + period / 2;
            if eighth < n {
                odf[eighth] = 0.55;
            }
            t += period;
        }

        let grid = track(&odf, rate, real_bpm).expect("grid");

        let strong: Vec<f32> = (0..)
            .map(|i| (offset + i * period) as f32 / rate)
            .take_while(|&s| s < n as f32 / rate)
            .collect();
        let weak: Vec<f32> = (0..)
            .map(|i| (offset + period / 2 + i * period) as f32 / rate)
            .take_while(|&s| s < n as f32 / rate)
            .collect();

        let on = f_measure(&grid.beats, &strong, 0.07);
        let off = f_measure(&grid.beats, &weak, 0.07);
        assert!(on > 0.95, "F-measure {on:.3} against the real pulse");
        assert!(
            off < 0.1,
            "grid landed on the offbeats instead ({off:.3} against them)"
        );
    }

    /// The reason for dynamic programming rather than peak-picking.
    #[test]
    fn prefers_a_steady_pulse_over_louder_offbeats() {
        let rate = 86.13f32;
        let bpm = 120.0;
        let period = (60.0 / bpm * rate).round() as usize;
        let n = period * 40;
        let odf = odf_with_offbeat_decoys(period, n, 7);

        let grid = track(&odf, rate, bpm).expect("grid");

        // The onbeat and offbeat sequences are both period-consistent, so the
        // tracker may lock to either — but it must pick ONE and stay on it,
        // rather than alternating and producing a half-period grid.
        let gaps: Vec<f32> = grid
            .beats
            .windows(2)
            .map(|w| (w[1] - w[0]) * rate)
            .collect();
        let mean: f32 = gaps.iter().sum::<f32>() / gaps.len() as f32;
        assert!(
            (mean - period as f32).abs() < 2.0,
            "mean gap {mean:.1} frames, expected ~{period}"
        );
    }

    #[test]
    fn f_measure_is_one_for_identical_grids() {
        let g: Vec<f32> = (0..50).map(|i| i as f32 * 0.5).collect();
        assert!((f_measure(&g, &g, 0.07) - 1.0).abs() < 1e-6);
    }

    /// A grid offset by half a beat is the classic "on the offbeat" failure and
    /// must score near zero, or the metric cannot detect it.
    #[test]
    fn f_measure_rejects_an_offbeat_grid() {
        let reference: Vec<f32> = (0..50).map(|i| i as f32 * 0.5).collect();
        let offbeat: Vec<f32> = (0..50).map(|i| i as f32 * 0.5 + 0.25).collect();
        let f = f_measure(&offbeat, &reference, 0.07);
        assert!(f < 0.05, "offbeat grid scored {f:.3}");
    }

    #[test]
    fn f_measure_penalises_a_double_time_grid() {
        let reference: Vec<f32> = (0..50).map(|i| i as f32 * 0.5).collect();
        let double: Vec<f32> = (0..100).map(|i| i as f32 * 0.25).collect();
        let f = f_measure(&double, &reference, 0.07);
        // Half the detected beats are spurious, so precision caps at ~0.5.
        assert!(f < 0.7, "double-time grid scored {f:.3}");
    }

    #[test]
    fn refuses_signals_that_are_too_short() {
        assert!(track(&[0.0; 8], 86.13, 120.0).is_none());
        assert!(track(&[0.0; 1000], 86.13, 0.0).is_none());
    }
}

//! Beat-sync drift correction — the PLL (TD-21, MIG-009).
//!
//! Ported from `calculate_pll_adjustment` and `_apply_pll_sync` in
//! `audio_manager.gd`, plus `AudioDSP::get_cross_correlation_offset` for the
//! waveform term.
//!
//! ## What it is for here, which is not what it was for there
//!
//! In the Godot build the PLL was the mechanism that achieved sync. It polled
//! `get_playback_position()` from `_process` at frame rate, so the two decks'
//! notion of "now" came from the frame scheduler and wandered accordingly.
//!
//! Here both decks advance from the same audio clock — the count of frames
//! actually rendered — so once phase-locked they cannot drift apart by
//! construction. What is left for the loop to correct is **the music**: a track
//! whose real tempo wanders from the single number the analysis assigned it, a
//! beat grid that is slightly wrong, a drummer who is human. That is a smaller
//! job than the original had, and the gain and clamp below are the original's
//! regardless.
//!
//! ## Where it runs
//!
//! Not on the audio thread. The phase detector needs both beat grids, which are
//! `Vec<f32>`, and the correlation needs a few thousand samples of both decks —
//! neither may cross into the render path. So this is a pure function the shell
//! calls on a control thread, and only the resulting scalar reaches the mixer.
//! The same split as `schedule_prepared`, for the same reason.

use crate::mixer::BeatGrid;

/// Proportional gain, from `calculate_pll_adjustment`.
pub const KP: f32 = 0.15;

/// Largest correction the loop may apply, ±0.5%.
///
/// The original's clamp. It matters more than it looks: the correction
/// multiplies the stretch ratio, and anything a listener can hear as a tempo
/// change is worse than the drift it is fixing.
pub const MAX_ADJUSTMENT: f64 = 0.005;

/// How far the waveform correlation searches, in seconds.
pub const MAX_LAG_SECS: f32 = 0.1;

/// Rate the correlation runs at.
///
/// The original downsamples to 4 kHz before correlating. That is not a quality
/// compromise — it is what makes a ±0.1 s search affordable, and phase
/// information below 2 kHz is all the alignment needs.
pub const CORRELATION_RATE: f32 = 4_000.0;

/// How long a Tempo Morph runs before the loop engages, capped.
///
/// From `_pll_delay = min(3.0, duration * 0.5)`. During a morph both decks are
/// deliberately bending toward a shared tempo, so a loop chasing phase in the
/// first half would be fighting the transition rather than helping it.
pub const MORPH_DELAY_CAP_SECS: f32 = 3.0;

/// The correction to apply, and the error that produced it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Correction {
    /// Multiplier for the incoming deck's stretch ratio: `ratio * (1 + adj)`.
    pub adjustment: f64,
    /// The measured phase error, for display. Positive means the incoming
    /// track's beat is late.
    pub phase_error_secs: f32,
}

impl Correction {
    pub const NONE: Correction = Correction {
        adjustment: 0.0,
        phase_error_secs: 0.0,
    };
}

/// A locked pair of beats, and the loop that keeps them together.
#[derive(Debug, Clone, Copy)]
pub struct Pll {
    /// The beat in each grid that the transition aligned. Everything after is
    /// counted from this pair, so the loop tracks *the same* beat rather than
    /// whichever is nearest and risking a whole-beat jump.
    out_index: usize,
    in_index: usize,
    /// Seconds into the transition before the loop engages.
    delay_secs: f32,
    elapsed: f32,
}

impl Pll {
    /// Arm the loop on the pair of beats a transition aligned.
    ///
    /// `start_time_out` and `cue_in` are the same two the mixer used to compute
    /// the incoming position, so the loop anchors on exactly the beats that were
    /// matched rather than re-deriving a pair that might differ by one.
    pub fn arm(
        outgoing: &BeatGrid,
        incoming: &BeatGrid,
        start_time_out: f32,
        cue_in: f32,
        delay_secs: f32,
    ) -> Option<Pll> {
        let out_beat = outgoing.beat_at_or_after(start_time_out)?;
        let in_beat = incoming.beat_at_or_after(cue_in)?;
        Some(Pll {
            out_index: closest_beat_index(out_beat, &outgoing.beats)?,
            in_index: closest_beat_index(in_beat, &incoming.beats)?,
            delay_secs,
            elapsed: 0.0,
        })
    }

    /// Advance the loop's own clock. Below the delay it corrects nothing.
    pub fn advance(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    pub fn is_engaged(&self) -> bool {
        self.elapsed >= self.delay_secs
    }

    /// Measure the phase error and return the correction for it.
    ///
    /// `pos_out` and `pos_in` are the two decks' positions in their own
    /// timelines. `ratio` is the incoming deck's stretch, and `cc_offset` the
    /// waveform correlation from [`cross_correlation_offset`] — pass 0.0 to run
    /// on the beat grids alone.
    pub fn correction(
        &self,
        outgoing: &BeatGrid,
        incoming: &BeatGrid,
        pos_out: f32,
        pos_in: f32,
        ratio: f64,
        cc_offset: f32,
    ) -> Correction {
        if !self.is_engaged() {
            return Correction::NONE;
        }

        let Some(idx_out) = closest_beat_index(pos_out, &outgoing.beats) else {
            return Correction::NONE;
        };

        // The incoming beat that *should* be sounding alongside this one,
        // counted from the pair that was locked. Taking the nearest incoming
        // beat instead would let the loop settle a whole beat out and call it
        // aligned.
        let expected = self.in_index as isize + (idx_out as isize - self.out_index as isize);
        if expected < 0 || expected as usize >= incoming.beats.len() {
            return Correction::NONE;
        }

        let dt_out = outgoing.beats[idx_out] - pos_out;
        // Divided by the ratio, which the original did not do.
        //
        // `dt_in` is how far the next beat is in the *incoming track's own*
        // timeline; the incoming deck consumes `ratio` source seconds per
        // output second, so what a listener waits is `dt_in / ratio`. The
        // original compared the two directly, which biases the error by
        // `dt_out * (ratio - 1)` — a sawtooth at the beat rate reaching 30 ms
        // at full stretch, which a loop with a 0.15 gain then chases. See the
        // measurement in `docs/MIGRATION.md`: the faithful version made beat
        // alignment worse, this one makes it better.
        let dt_in = (incoming.beats[expected as usize] - pos_in) / ratio as f32;

        let phase_error = dt_in - dt_out - cc_offset;
        let adjustment = ((phase_error * KP) as f64).clamp(-MAX_ADJUSTMENT, MAX_ADJUSTMENT);

        Correction {
            adjustment,
            phase_error_secs: phase_error,
        }
    }
}

/// Index of the beat closest to `pos`.
///
/// Binary search, as in `_find_closest_beat_index` — a linear scan is fine for
/// one call and not fine at the rate a loop runs.
pub fn closest_beat_index(pos: f32, beats: &[f32]) -> Option<usize> {
    if beats.is_empty() {
        return None;
    }

    let (mut low, mut high) = (0usize, beats.len() - 1);
    while low < high {
        let mid = (low + high) / 2;
        if beats[mid] < pos {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    if low > 0 && (beats[low - 1] - pos).abs() < (beats[low] - pos).abs() {
        return Some(low - 1);
    }
    Some(low)
}

/// How far `y` lags `x`, in seconds, by normalised cross-correlation.
///
/// The waveform half of the phase detector. Beat grids say where the beats are
/// *believed* to be; this says where the audio actually lines up, which catches
/// a grid that is systematically early or late.
///
/// `x` must span the search window plus [`MAX_LAG_SECS`] either side, and `y`
/// just the window — the caller extracts both, because only it knows where the
/// samples live. Both are mono at `rate`.
///
/// Returns 0.0 when there is not enough signal to judge, which the loop treats
/// as "no waveform opinion" rather than as "perfectly aligned".
pub fn cross_correlation_offset(x: &[f32], y: &[f32], rate: f32) -> f32 {
    if x.is_empty() || y.is_empty() || rate <= 0.0 {
        return 0.0;
    }

    let x = downsample(x, rate, CORRELATION_RATE);
    let y = downsample(y, rate, CORRELATION_RATE);

    let n = y.len();
    let lags = (MAX_LAG_SECS * CORRELATION_RATE) as usize;
    if n == 0 || x.len() < n + 2 * lags {
        return 0.0;
    }

    let den_y: f64 = y.iter().map(|&s| (s as f64) * (s as f64)).sum();
    if den_y < 1e-6 {
        return 0.0;
    }

    // The energy of every window of `x` this will correlate against, computed
    // by sliding rather than summed afresh per lag — the original does the same,
    // and without it the cost is quadratic in the search width.
    let mut den_x = vec![0.0f64; 2 * lags + 1];
    let mut running: f64 = x[..n].iter().map(|&s| (s as f64) * (s as f64)).sum();
    den_x[0] = running;
    for lag in 1..=(2 * lags) {
        let leaving = x[lag - 1] as f64;
        let entering = x[lag + n - 1] as f64;
        running -= leaving * leaving;
        running += entering * entering;
        den_x[lag] = running;
    }

    let mut best_lag = 0isize;
    let mut best_corr = -2.0f64;
    for lag in 0..=(2 * lags) {
        if den_x[lag] < 1e-6 {
            continue;
        }
        let num: f64 = (0..n).map(|i| x[lag + i] as f64 * y[i] as f64).sum();
        let corr = num / (den_x[lag] * den_y).sqrt();
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag as isize - lags as isize;
        }
    }

    best_lag as f32 / CORRELATION_RATE
}

/// Box-average down to `to` Hz, as `get_cross_correlation_offset` does.
///
/// A box average rather than the proper windowed sinc in `vapor_dsp::resample`:
/// the correlation cares about gross alignment, and the aliasing a box filter
/// leaves is common to both signals, so it largely cancels in the ratio.
fn downsample(input: &[f32], from: f32, to: f32) -> Vec<f32> {
    if from <= to {
        return input.to_vec();
    }
    let step = from / to;
    let out_len = (input.len() as f32 / step) as usize;
    (0..out_len)
        .map(|i| {
            let start = (i as f32 * step) as usize;
            let end = (((i as f32 + 1.0) * step) as usize)
                .max(start + 1)
                .min(input.len());
            let slice = &input[start..end];
            slice.iter().sum::<f32>() / slice.len() as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(bpm: f32, start: f32, count: usize) -> BeatGrid {
        let period = 60.0 / bpm;
        BeatGrid {
            bpm,
            beats: (0..count).map(|i| start + i as f32 * period).collect(),
        }
    }

    #[test]
    fn closest_beat_is_the_nearest_one_either_side() {
        let beats = [0.0f32, 1.0, 2.0, 3.0];
        assert_eq!(closest_beat_index(-5.0, &beats), Some(0));
        assert_eq!(closest_beat_index(0.4, &beats), Some(0));
        assert_eq!(closest_beat_index(0.6, &beats), Some(1));
        assert_eq!(closest_beat_index(2.9, &beats), Some(3));
        assert_eq!(closest_beat_index(99.0, &beats), Some(3));
        assert_eq!(closest_beat_index(0.0, &[]), None);
    }

    /// The incoming position that is genuinely simultaneous with `pos_out`.
    ///
    /// The two anchor beats sound together by construction — that is what the
    /// transition aligned — so everything else follows from the distance back
    /// to them, scaled into the incoming track's own timeline.
    fn aligned_in(out: &BeatGrid, inc: &BeatGrid, pll: &Pll, pos_out: f32, ratio: f32) -> f32 {
        let lead = out.beats[pll.out_index] - pos_out;
        inc.beats[pll.in_index] - lead * ratio
    }

    /// Two decks already in phase need no correction. A loop that corrects a
    /// locked pair is a loop that makes things worse.
    #[test]
    fn a_locked_pair_is_left_alone() {
        let out = grid(128.0, 0.0, 400);
        let inc = grid(128.0, 0.0, 400);
        let pll = Pll::arm(&out, &inc, 30.0, 10.0, 0.0).expect("arm");

        for step in 0..16 {
            let pos_out = 30.0 + step as f32 * 0.05;
            let pos_in = aligned_in(&out, &inc, &pll, pos_out, 1.0);
            let c = pll.correction(&out, &inc, pos_out, pos_in, 1.0, 0.0);
            assert!(
                c.adjustment.abs() < 1e-9,
                "a locked pair was corrected by {} at step {step}",
                c.adjustment
            );
        }
    }

    /// The sign has to be right, and it is the thing most easily got backwards:
    /// an incoming track whose beat is *late* must be sped up.
    #[test]
    fn a_late_incoming_beat_is_sped_up() {
        let out = grid(128.0, 0.0, 400);
        let inc = grid(128.0, 0.0, 400);
        let pll = Pll::arm(&out, &inc, 30.0, 10.0, 0.0).expect("arm");

        // 20 ms behind where it should be, so its next beat is that much
        // further away.
        let pos_out = 30.1;
        let pos_in = aligned_in(&out, &inc, &pll, pos_out, 1.0) - 0.02;

        let c = pll.correction(&out, &inc, pos_out, pos_in, 1.0, 0.0);
        assert!(
            c.adjustment > 0.0,
            "a late incoming beat was slowed further ({})",
            c.adjustment
        );
        assert!(
            (c.phase_error_secs - 0.02).abs() < 1e-4,
            "a 20 ms lag measured as {:.1} ms",
            c.phase_error_secs * 1000.0
        );
    }

    #[test]
    fn an_early_incoming_beat_is_slowed_down() {
        let out = grid(128.0, 0.0, 400);
        let inc = grid(128.0, 0.0, 400);
        let pll = Pll::arm(&out, &inc, 30.0, 10.0, 0.0).expect("arm");

        let pos_out = 30.1;
        let pos_in = aligned_in(&out, &inc, &pll, pos_out, 1.0) + 0.02;

        let c = pll.correction(&out, &inc, pos_out, pos_in, 1.0, 0.0);
        assert!(c.adjustment < 0.0, "an early incoming beat was sped up");
    }

    /// The clamp is what keeps a correction inaudible. Without it a bad grid
    /// produces a tempo jump rather than a nudge.
    #[test]
    fn the_correction_is_clamped_to_half_a_percent() {
        let out = grid(128.0, 0.0, 400);
        let inc = grid(128.0, 0.0, 400);
        let pll = Pll::arm(&out, &inc, 30.0, 10.0, 0.0).expect("arm");

        // Half a beat out, which is as wrong as it gets.
        let c = pll.correction(&out, &inc, 30.0, 10.234, 1.0, 0.0);
        assert!(
            c.adjustment.abs() <= MAX_ADJUSTMENT + 1e-12,
            "correction {} exceeded the clamp",
            c.adjustment
        );
    }

    /// The bias the original had, stated as a test: with the incoming deck
    /// stretched, a pair that is genuinely aligned must still read as aligned.
    ///
    /// Comparing the two beat distances directly — as `calculate_pll_adjustment`
    /// does — reports an error of `dt_out * (ratio - 1)` here, which is a
    /// sawtooth at the beat rate rather than a real phase error.
    #[test]
    fn a_stretched_deck_in_phase_reads_as_in_phase() {
        let out = grid(128.0, 0.0, 400);
        let inc = grid(124.0, 0.0, 400);
        let ratio = 128.0 / 124.0;
        let pll = Pll::arm(&out, &inc, 30.0, 10.0, 0.0).expect("arm");

        let idx_out = closest_beat_index(30.0, &out.beats).expect("beat");
        let idx_in = pll.in_index + (idx_out - pll.out_index);

        // Step through a whole beat. At every point the incoming deck sits the
        // correctly *scaled* distance from its beat, so the error must stay at
        // zero rather than sweeping.
        for step in 0..20 {
            let lead_out = 0.4 * step as f32 / 20.0;
            let pos_out = out.beats[idx_out] - lead_out;
            let pos_in = inc.beats[idx_in] - lead_out * ratio as f32;

            let c = pll.correction(&out, &inc, pos_out, pos_in, ratio, 0.0);
            assert!(
                c.phase_error_secs.abs() < 1e-4,
                "step {step}: a correctly aligned stretched deck read {:.1} ms out",
                c.phase_error_secs * 1000.0
            );
        }
    }

    /// A Tempo Morph must not be corrected while it is still morphing.
    #[test]
    fn the_loop_waits_out_its_delay() {
        let out = grid(128.0, 0.0, 400);
        let inc = grid(128.0, 0.0, 400);
        let mut pll = Pll::arm(&out, &inc, 30.0, 10.0, 3.0).expect("arm");

        assert!(!pll.is_engaged());
        assert_eq!(
            pll.correction(&out, &inc, 30.1, 10.08, 1.0, 0.0),
            Correction::NONE
        );

        pll.advance(3.5);
        assert!(pll.is_engaged());
        assert!(pll.correction(&out, &inc, 30.1, 10.08, 1.0, 0.0).adjustment > 0.0);
    }

    /// A grid that runs out must stop the loop rather than wrap to its start.
    #[test]
    fn running_past_the_end_of_a_grid_corrects_nothing() {
        let out = grid(128.0, 0.0, 400);
        let inc = grid(128.0, 0.0, 20);
        let pll = Pll::arm(&out, &inc, 30.0, 5.0, 0.0).expect("arm");

        assert_eq!(
            pll.correction(&out, &inc, 100.0, 10.0, 1.0, 0.0),
            Correction::NONE
        );
    }

    fn chirp(n: usize, rate: f32) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / rate;
                (t * 300.0 * std::f32::consts::TAU).sin() * (1.0 - (t * 4.0).fract()).max(0.0)
            })
            .collect()
    }

    /// The correlation must find a known offset, with the right sign.
    ///
    /// Both slices come from one signal, so the answer is not a matter of
    /// judgement: `x` spans the search window and `y` is taken `shift` seconds
    /// further along it, so the reported lag must be `shift`.
    #[test]
    fn cross_correlation_finds_a_known_shift() {
        let rate = 44_100.0f32;
        let signal = chirp(2 * rate as usize, rate);

        let window = (0.5 * rate) as usize;
        let pad = (MAX_LAG_SECS * rate) as usize;
        let base = rate as usize;

        for shift_ms in [-60.0f32, -30.0, 0.0, 30.0, 60.0] {
            let shift = (shift_ms / 1000.0 * rate) as isize;

            // x[lag + pad + i] is signal[base + lag + i]; y[i] is
            // signal[base + shift + i]. They match at lag == shift.
            let x = &signal[base - pad..base + window + pad];
            let start = (base as isize + shift) as usize;
            let y = &signal[start..start + window];

            let found = cross_correlation_offset(x, y, rate);
            assert!(
                (found - shift_ms / 1000.0).abs() < 0.005,
                "a known {shift_ms} ms shift was measured as {:.1} ms",
                found * 1000.0
            );
        }
    }

    /// Silence has no alignment to report, and inventing one would feed the
    /// loop noise during a quiet passage.
    #[test]
    fn silence_produces_no_opinion() {
        let quiet = vec![0.0f32; 44_100];
        assert_eq!(
            cross_correlation_offset(&quiet, &quiet[..4410], 44_100.0),
            0.0
        );
        assert_eq!(cross_correlation_offset(&[], &[], 44_100.0), 0.0);
    }
}

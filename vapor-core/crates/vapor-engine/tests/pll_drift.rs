//! Does the drift correction actually correct drift? (TD-21, MIG-009)
//!
//! The PLL's own unit tests prove the phase detector reports the right number
//! for a constructed error. They cannot say whether the loop *helps*, because
//! that depends on rendering real audio through the stretcher and measuring
//! where the beats land — the lesson from the `tempo_ratio` inversion, which
//! passed a unit test that encoded the same misunderstanding.
//!
//! So this renders a transition into a track whose **real tempo wanders from
//! the one its grid claims**, which is the only thing the loop is for here.
//! Both decks already run off the same audio clock, so there is no clock drift
//! left to chase; what is left is a drummer who is human, or an analysis that
//! assigned one number to a performance that did not hold it.
//!
//! Three arrangements are measured against each other:
//!
//! | | |
//! |---|---|
//! | **off** | no correction — the baseline the mixer shipped with |
//! | **unscaled** | the phase detector exactly as `calculate_pll_adjustment` writes it |
//! | **scaled** | the same, with `dt_in` converted into output time |
//!
//! The third exists because the original compares a distance measured in the
//! incoming track's timeline against one in the outgoing track's, and those
//! tick at different rates whenever the decks are stretched.

use vapor_engine::mixer::BeatGrid;
use vapor_engine::pll::{closest_beat_index, Pll};
use vapor_engine::{Mixer, TransitionType};

const RATE: f32 = 44_100.0;
const BLOCK: usize = 512;

/// A click track that really runs at `actual_bpm`, paired with the beat grid an
/// analysis that decided `claimed_bpm` would have produced for it.
///
/// The gap between the two is the whole subject. Tempo agreement is ~81%
/// (`docs/MIGRATION.md`), and the disagreements are not all wild — a track
/// called 124.0 that is really 124.6 passes every check the mixer makes and
/// then slips a beat's width every twenty seconds. That is precisely the error
/// a drift correction is for, and precisely the size the ±0.5% clamp was
/// chosen to cover.
fn click_track(actual_bpm: f32, claimed_bpm: f32, secs: f32) -> (Vec<[i16; 2]>, BeatGrid) {
    let n = (secs * RATE) as usize;
    let mut samples = vec![[0i16; 2]; n];

    // Stepping in fractional seconds and rounding at use. Integer stepping
    // accumulates exactly the error that undid MIG-002b's first attempt, and
    // the mistake first appeared in a test fixture where it looked harmless.
    let period = 60.0 / actual_bpm as f64;
    let mut t = 0.0f64;
    while t < secs as f64 {
        let start = (t * RATE as f64) as usize;
        for k in 0..(RATE * 0.005) as usize {
            if start + k >= n {
                break;
            }
            let tt = k as f32 / RATE;
            let env = (1.0 - tt / 0.005).max(0.0);
            let v = env * (2.0 * std::f32::consts::PI * 2000.0 * tt).sin() * 0.8;
            let q = vapor_engine::stretch::from_f32(v);
            samples[start + k] = [q, q];
        }
        t += period;
    }

    // What the analysis believes: one tempo, evenly spaced, slightly wrong.
    let claimed_period = 60.0 / claimed_bpm;
    let claimed = BeatGrid {
        bpm: claimed_bpm,
        beats: (0..(secs / claimed_period) as usize)
            .map(|i| i as f32 * claimed_period)
            .collect(),
    };

    (samples, claimed)
}

fn onsets(buf: &[[f32; 2]], threshold: f32) -> Vec<f32> {
    let mut out = Vec::new();
    let mut last = -1.0f32;
    for (i, frame) in buf.iter().enumerate() {
        let level = frame[0].abs().max(frame[1].abs());
        let t = i as f32 / RATE;
        if level > threshold && (last < 0.0 || t - last > 0.1) {
            out.push(t);
            last = t;
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Off,
    /// `calculate_pll_adjustment` as written: the two beat distances compared
    /// directly, in their own timelines.
    Unscaled,
    /// The same, with the incoming distance converted into output time.
    Scaled,
    /// Scaled, plus the waveform correlation term.
    Correlated,
}

/// Window the correlation compares, from `_apply_pll_sync`'s 0.5 s.
const CC_WINDOW: f32 = 0.5;

/// Mono samples spanning `[from, from + len)` seconds, zero-padded at the edges.
///
/// Stands in for what the shell reads out of a deck's window: the correlation
/// needs actual audio, and audio is the one thing beat grids do not contain.
fn span(samples: &[[i16; 2]], from: f32, len: f32) -> Vec<f32> {
    let start = (from * RATE) as isize;
    let n = (len * RATE) as usize;
    (0..n)
        .map(|i| {
            let idx = start + i as isize;
            if idx < 0 || idx as usize >= samples.len() {
                return 0.0;
            }
            let f = samples[idx as usize];
            (f[0] as f32 + f[1] as f32) / (2.0 * 32_768.0)
        })
        .collect()
}

/// Render a transition and report the worst distance from any onset to the
/// outgoing deck's grid, in milliseconds.
fn worst_deviation_ms(mode: Mode) -> f32 {
    let out_bpm = 128.0f32;
    let in_bpm = 124.0f32;
    let duration = 12.0f32;
    let start_at = 20.0f32;
    let cue_in = 8.0f32;

    // The outgoing track is what its grid says. The incoming one is 0.3%
    // faster than the analysis decided — well inside what the mixer accepts,
    // and enough to slip a beat's width across a long mix.
    let (out_samples, out_grid) = click_track(out_bpm, out_bpm, 90.0);
    let (in_samples, in_grid) = click_track(in_bpm * 1.003, in_bpm, 90.0);
    // Kept for the correlation, which needs the audio itself.
    let (out_audio, in_audio) = (out_samples.clone(), in_samples.clone());

    let mut mixer = Mixer::new(RATE, BLOCK);
    mixer.deck_a.load(out_samples);
    mixer.deck_b.load(in_samples);
    mixer.deck_a.seek_seconds(start_at as f64);
    mixer.deck_a.play();
    mixer
        .start_transition(
            TransitionType::StandardCrossfade,
            duration,
            &out_grid,
            &in_grid,
            start_at,
            cue_in,
        )
        .expect("schedulable");

    let ratio = Mixer::tempo_ratio(&out_grid, &in_grid).expect("ratio");
    let mut pll = Pll::arm(&out_grid, &in_grid, start_at, cue_in, 0.0).expect("arm");

    let total = ((duration - 0.25) * RATE) as usize;
    let mut rendered = Vec::with_capacity(total);
    let mut block = [[0.0f32; 2]; BLOCK];
    let dt = BLOCK as f32 / RATE;
    let mut cc = 0.0f32;

    while rendered.len() < total {
        if mode != Mode::Off {
            // Exactly what the shell does: read both positions, correct on a
            // control thread, hand the mixer a scalar.
            let pos_out = mixer.outgoing().position_seconds() as f32;
            let pos_in = mixer.incoming().position_seconds() as f32;
            pll.advance(dt);

            let adjustment = match mode {
                Mode::Scaled => {
                    pll.correction(&out_grid, &in_grid, pos_out, pos_in, ratio, 0.0)
                        .adjustment
                }
                Mode::Correlated => {
                    // Refreshed on the original's 0.1 s timer rather than every
                    // block: it is by far the most expensive part, and the
                    // alignment it measures does not move that fast.
                    if rendered.len() % ((RATE * 0.1) as usize) < BLOCK {
                        let max_lag = vapor_engine::pll::MAX_LAG_SECS;
                        let x = span(
                            &out_audio,
                            pos_out - CC_WINDOW / 2.0 - max_lag,
                            CC_WINDOW + 2.0 * max_lag,
                        );
                        let y = span(&in_audio, pos_in - CC_WINDOW / 2.0, CC_WINDOW);
                        cc = vapor_engine::pll::cross_correlation_offset(&x, &y, RATE);
                    }
                    pll.correction(&out_grid, &in_grid, pos_out, pos_in, ratio, cc)
                        .adjustment
                }
                Mode::Unscaled => unscaled_adjustment(&pll, &out_grid, &in_grid, pos_out, pos_in),
                Mode::Off => 0.0,
            };
            mixer.set_pll_adjustment(adjustment);
        }

        mixer.render(&mut block);
        rendered.extend_from_slice(&block);
    }
    rendered.truncate(total);

    // Every onset in the output should sit on the outgoing grid. The incoming
    // deck's clicks are the ones that can slip.
    let period = 60.0 / out_bpm;
    let first_beat_source = (start_at / period).ceil() * period;
    let anchor = first_beat_source - start_at;

    let mut worst = 0.0f32;
    for o in onsets(&rendered, 0.05) {
        let phase = ((o - anchor) / period).rem_euclid(1.0);
        worst = worst.max(phase.min(1.0 - phase) * period);
    }
    worst * 1000.0
}

/// `calculate_pll_adjustment` with no ratio conversion, reproduced here so the
/// two can be measured side by side rather than argued about.
fn unscaled_adjustment(
    pll: &Pll,
    outgoing: &BeatGrid,
    incoming: &BeatGrid,
    pos_out: f32,
    pos_in: f32,
) -> f64 {
    // The scaled version with ratio 1.0 *is* the unscaled formula, so this
    // needs no second copy of the index arithmetic.
    let _ = closest_beat_index(pos_out, &outgoing.beats);
    pll.correction(outgoing, incoming, pos_out, pos_in, 1.0, 0.0)
        .adjustment
}

/// The measurement. Printed as well as asserted, because the numbers are the
/// point and a pass/fail alone would not tell the next person what they were.
#[test]
fn drift_correction_improves_beat_alignment() {
    let off = worst_deviation_ms(Mode::Off);
    let unscaled = worst_deviation_ms(Mode::Unscaled);
    let scaled = worst_deviation_ms(Mode::Scaled);
    let correlated = worst_deviation_ms(Mode::Correlated);

    println!(
        "worst beat deviation, 12 s mix into a track 0.3% off its declared tempo:\n  \
         correction off        {off:.2} ms\n  \
         grids, unscaled       {unscaled:.2} ms   (as calculate_pll_adjustment writes it)\n  \
         grids, scaled         {scaled:.2} ms\n  \
         grids + correlation   {correlated:.2} ms"
    );

    // The grid terms cannot help here and must not hurt. Both decks advance
    // from the same audio clock, so the distance from a position to a beat in a
    // *static* grid is exactly what the arithmetic says it is — there is no
    // error for the detector to find. The unscaled form does worse than nothing
    // because its timeline mismatch invents one.
    assert!(
        scaled <= off + 0.01,
        "the grid correction made alignment worse: {scaled:.2} ms with it, {off:.2} ms without"
    );
    assert!(
        scaled <= unscaled + 0.01,
        "converting the incoming distance into output time made alignment worse: \
         {scaled:.2} ms scaled, {unscaled:.2} ms unscaled"
    );

    // The correlation is the term that can see the problem, because it looks at
    // the audio rather than at what the analysis claimed about it.
    assert!(
        correlated < off * 0.75,
        "the correlation failed to pull back a 0.3% tempo error: {correlated:.2} ms \
         against {off:.2} ms uncorrected"
    );
}

/// Whatever the loop does to alignment, it must not be audible as a tempo
/// change. A correction that wanders is heard as wow and flutter — the same
/// property the post-transition glide is tested for.
#[test]
fn the_correction_stays_within_its_clamp() {
    let out_bpm = 128.0f32;
    let (out_samples, out_grid) = click_track(out_bpm, out_bpm, 60.0);
    let (in_samples, in_grid) = click_track(124.0 * 1.003, 124.0, 60.0);

    let mut mixer = Mixer::new(RATE, BLOCK);
    mixer.deck_a.load(out_samples);
    mixer.deck_b.load(in_samples);
    mixer.deck_a.seek_seconds(20.0);
    mixer.deck_a.play();
    mixer
        .start_transition(
            TransitionType::StandardCrossfade,
            10.0,
            &out_grid,
            &in_grid,
            20.0,
            8.0,
        )
        .expect("schedulable");

    let ratio = Mixer::tempo_ratio(&out_grid, &in_grid).expect("ratio");
    let mut pll = Pll::arm(&out_grid, &in_grid, 20.0, 8.0, 0.0).expect("arm");
    let mut block = [[0.0f32; 2]; BLOCK];
    let dt = BLOCK as f32 / RATE;

    let base = ratio;
    for _ in 0..((10.0 * RATE) as usize / BLOCK) {
        let pos_out = mixer.outgoing().position_seconds() as f32;
        let pos_in = mixer.incoming().position_seconds() as f32;
        pll.advance(dt);
        let c = pll.correction(&out_grid, &in_grid, pos_out, pos_in, ratio, 0.0);
        mixer.set_pll_adjustment(c.adjustment);
        mixer.render(&mut block);

        let applied = mixer.incoming().ratio;
        assert!(
            (applied / base - 1.0).abs() <= vapor_engine::pll::MAX_ADJUSTMENT + 1e-9,
            "the deck ran at {applied:.6} against a base of {base:.6}, which is \
             outside the clamp and would be heard as a tempo change"
        );
    }
}

/// The correction must not compound. Feeding the same adjustment every block
/// has to leave the deck at one steady rate, not accelerate it.
#[test]
fn a_steady_correction_does_not_run_away() {
    let (out_samples, out_grid) = click_track(128.0, 128.0, 40.0);
    let (in_samples, in_grid) = click_track(126.0, 126.0, 40.0);

    let mut mixer = Mixer::new(RATE, BLOCK);
    mixer.deck_a.load(out_samples);
    mixer.deck_b.load(in_samples);
    mixer.deck_a.seek_seconds(10.0);
    mixer.deck_a.play();
    mixer
        .start_transition(
            TransitionType::StandardCrossfade,
            8.0,
            &out_grid,
            &in_grid,
            10.0,
            4.0,
        )
        .expect("schedulable");

    let base = Mixer::tempo_ratio(&out_grid, &in_grid).expect("ratio");
    let mut block = [[0.0f32; 2]; BLOCK];

    mixer.set_pll_adjustment(0.005);
    for _ in 0..200 {
        mixer.render(&mut block);
        let applied = mixer.incoming().ratio;
        assert!(
            (applied - base * 1.005).abs() < 1e-9,
            "a constant correction drifted the ratio to {applied:.9}, expected \
             {:.9} — the correction is compounding",
            base * 1.005
        );
    }
}

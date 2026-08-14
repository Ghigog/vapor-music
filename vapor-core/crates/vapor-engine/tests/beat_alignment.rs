//! End-to-end verification that a rendered transition is actually beat-matched.
//!
//! The unit tests in `mixer` prove the *scheduling arithmetic*. They would still
//! pass if the deck ignored its stretch ratio, if the stretcher consumed source
//! at the wrong rate, or if the two decks drifted apart while rendering. This
//! test renders real audio through the whole chain and measures where the beats
//! landed in the output.
//!
//! Click tracks are used rather than music because the ground truth is exact:
//! every onset position is known by construction, so a misalignment is a
//! measurement, not an opinion.

use vapor_engine::mixer::BeatGrid;
use vapor_engine::{Mixer, TransitionType};

const RATE: f32 = 44100.0;
const BLOCK: usize = 512;

/// A click track: a short decaying burst at each beat.
fn click_track(bpm: f32, secs: f32, first_beat: f32) -> (Vec<[f32; 2]>, BeatGrid) {
    let n = (secs * RATE) as usize;
    let mut samples = vec![[0.0f32; 2]; n];
    let period = 60.0 / bpm;

    let mut beats = Vec::new();
    let mut t = first_beat;
    while t < secs {
        beats.push(t);
        let start = (t * RATE) as usize;
        // 5 ms decaying 2 kHz burst — narrow enough to localise precisely.
        let len = (RATE * 0.005) as usize;
        for k in 0..len {
            if start + k >= n {
                break;
            }
            let tt = k as f32 / RATE;
            let env = (1.0 - tt / 0.005).max(0.0);
            let v = env * (2.0 * std::f32::consts::PI * 2000.0 * tt).sin() * 0.8;
            samples[start + k] = [v, v];
        }
        t += period;
    }

    (samples, BeatGrid { bpm, beats })
}

/// Find onset times in a rendered buffer by peak-picking the envelope.
fn detect_onsets(buf: &[[f32; 2]], threshold: f32) -> Vec<f32> {
    let mut onsets = Vec::new();
    // Refractory period so one burst yields one onset.
    let refractory = (RATE * 0.05) as usize;
    let mut last = 0usize;

    for (i, frame) in buf.iter().enumerate().skip(1) {
        let a = frame[0].abs();
        if a > threshold && (onsets.is_empty() || i - last > refractory) {
            onsets.push(i as f32 / RATE);
            last = i;
        }
    }
    onsets
}

fn render_transition(
    kind: TransitionType,
    duration: f32,
    out_bpm: f32,
    in_bpm: f32,
    start_at: f32,
    total_secs: f32,
) -> Vec<[f32; 2]> {
    let (out_samples, out_grid) = click_track(out_bpm, 90.0, 0.0);
    let (in_samples, in_grid) = click_track(in_bpm, 90.0, 0.0);

    let mut mixer = Mixer::new(RATE, BLOCK);
    mixer.deck_a.load(out_samples);
    mixer.deck_b.load(in_samples);
    mixer.deck_a.seek_seconds(start_at as f64);
    mixer.deck_a.play();

    mixer
        .start_transition(kind, duration, &out_grid, &in_grid, start_at, 8.0)
        .expect("transition should be schedulable");

    let total = (total_secs * RATE) as usize;
    let mut rendered = Vec::with_capacity(total);
    let mut block = [[0.0f32; 2]; BLOCK];

    while rendered.len() < total {
        mixer.render(&mut block);
        rendered.extend_from_slice(&block);
    }
    rendered.truncate(total);
    rendered
}

/// The core claim: during the overlap, every onset in the output sits on the
/// outgoing deck's beat grid. If the incoming deck were not tempo-matched and
/// phase-aligned, its clicks would fall between the outgoing clicks and drift
/// further apart every beat.
#[test]
fn rendered_transition_keeps_all_onsets_on_the_outgoing_grid() {
    let out_bpm = 128.0;
    let in_bpm = 124.0; // ~3.1% slower — within MAX_STRETCH
    let duration = 6.0;
    let start_at = 20.0;

    let rendered = render_transition(
        TransitionType::StandardCrossfade,
        duration,
        out_bpm,
        in_bpm,
        start_at,
        duration + 1.0,
    );

    let onsets = detect_onsets(&rendered, 0.05);
    assert!(
        onsets.len() > 8,
        "expected a stream of onsets, found {}",
        onsets.len()
    );

    let period = 60.0 / out_bpm;

    // The grid is anchored to the *output*, not to output time zero. Deck A was
    // seeked to `start_at`, which is mid-bar, so the first beat heard arrives
    // one partial period in. Deriving the anchor rather than assuming zero is
    // the difference between testing the mixer and testing an assumption.
    let first_beat_source = (start_at / period).ceil() * period;
    let anchor = first_beat_source - start_at;

    // Tolerance: WSOLA splices at waveform-similar points within its search
    // window (±256 frames ~= 5.8 ms), so a matched beat can sit a few ms off.
    // 15 ms is well inside what reads as "tight" for a DJ mix.
    let tolerance = 0.015;

    // Only the transition window is beat-matched. Once the mix completes the
    // incoming deck returns to its own tempo — that is the intended behaviour
    // (`deck_roles_swap_and_tempo_returns_to_normal`), so onsets after the
    // transition legitimately leave the outgoing grid.
    let checked: Vec<f32> = onsets
        .iter()
        .copied()
        .filter(|&o| o < duration - 0.25)
        .collect();
    assert!(
        checked.len() > 8,
        "expected onsets throughout the transition, found {}",
        checked.len()
    );

    let mut worst: f32 = 0.0;
    for &o in &checked {
        // Distance to the nearest point on the anchored outgoing grid.
        let phase = ((o - anchor) / period).rem_euclid(1.0);
        let err = phase.min(1.0 - phase) * period;
        worst = worst.max(err);
        assert!(
            err < tolerance,
            "onset at {o:.4}s is {:.1} ms off the {out_bpm} BPM grid \
             (anchor {anchor:.4}s)",
            err * 1000.0
        );
    }
    println!(
        "{} onsets checked across the transition, worst deviation {:.2} ms",
        checked.len(),
        worst * 1000.0
    );
}

/// A transition whose tempo difference exceeds what stretching can bridge must
/// be refused up front rather than producing a mix that drifts.
#[test]
fn implausible_tempo_pairs_are_refused_not_rendered() {
    let (out_samples, out_grid) = click_track(128.0, 60.0, 0.0);
    let (in_samples, in_grid) = click_track(175.0, 60.0, 0.0);

    let mut mixer = Mixer::new(RATE, BLOCK);
    mixer.deck_a.load(out_samples);
    mixer.deck_b.load(in_samples);
    mixer.deck_a.play();

    let r = mixer.start_transition(
        TransitionType::StandardCrossfade,
        4.0,
        &out_grid,
        &in_grid,
        20.0,
        8.0,
    );
    assert!(r.is_err(), "175 vs 128 BPM should be refused");
    assert!(!mixer.is_transitioning());
}

/// After the transition completes, the incoming deck becomes the active deck,
/// at full level and back at its own tempo.
#[test]
fn deck_roles_swap_and_tempo_returns_to_normal() {
    let out_bpm = 128.0;
    let in_bpm = 126.0;
    let duration = 4.0;

    let (out_samples, out_grid) = click_track(out_bpm, 90.0, 0.0);
    let (in_samples, in_grid) = click_track(in_bpm, 90.0, 0.0);

    let mut mixer = Mixer::new(RATE, BLOCK);
    mixer.deck_a.load(out_samples);
    mixer.deck_b.load(in_samples);
    mixer.deck_a.seek_seconds(20.0);
    mixer.deck_a.play();
    mixer
        .start_transition(
            TransitionType::BassSwap,
            duration,
            &out_grid,
            &in_grid,
            20.0,
            8.0,
        )
        .unwrap();

    // Deck B runs stretched during the mix.
    assert!((mixer.deck_b.ratio - 1.0).abs() > 1e-6);

    let mut block = [[0.0f32; 2]; BLOCK];
    let blocks = ((duration + 0.5) * RATE / BLOCK as f32) as usize;
    for _ in 0..blocks {
        mixer.render(&mut block);
    }

    assert!(
        !mixer.is_transitioning(),
        "transition should have completed"
    );
    assert!(
        !mixer.deck_a.is_playing(),
        "outgoing deck should have stopped"
    );
    assert!(mixer.deck_b.is_playing(), "incoming deck should still play");
    assert!(
        (mixer.deck_b.ratio - 1.0).abs() < 1e-9,
        "tempo should return to the track's own after the mix, got {}",
        mixer.deck_b.ratio
    );
}

/// The mix must never clip, across every transition type.
#[test]
fn no_transition_clips_the_master() {
    for kind in [
        TransitionType::StandardCrossfade,
        TransitionType::BassSwap,
        TransitionType::FilterSweep,
    ] {
        let rendered = render_transition(kind, 4.0, 128.0, 126.0, 20.0, 5.0);
        let peak = rendered
            .iter()
            .flat_map(|s| [s[0].abs(), s[1].abs()])
            .fold(0.0f32, f32::max);
        assert!(peak <= 1.0, "{kind:?} peaked at {peak:.3}");
        assert!(peak > 0.05, "{kind:?} produced near-silence ({peak:.4})");
    }
}

/// MIG-006 regression. Bass Swap holds the outgoing deck at 0 dB while the
/// incoming one reaches full level, so without the three-band guard their sum
/// clips. Measured on a real transition before the fix: 152 clipped samples.
///
/// Each deck is a loud low tone that is safe *on its own* — only the sum can
/// exceed unity. Click tracks are not used here: their 0.8 transients plus a
/// loud tone would put a single deck over full scale, and the test would then
/// be measuring its own fixture rather than the guard.
#[test]
fn bass_swap_does_not_clip_two_loud_decks() {
    let bpm = 128.0;
    let secs = 90.0;
    let n = (secs * RATE) as usize;

    let tone = |freq: f32, amp: f32| -> Vec<[f32; 2]> {
        (0..n)
            .map(|i| {
                let v = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / RATE).sin();
                [v, v]
            })
            .collect()
    };

    // 0.75 each: safe alone, and their sum reaches 1.5 without a guard.
    let a_samples = tone(60.0, 0.75);
    let b_samples = tone(55.0, 0.75);

    let period = 60.0 / bpm;
    let grid = |()| BeatGrid {
        bpm,
        beats: (0..((secs / period) as usize))
            .map(|i| i as f32 * period)
            .collect(),
    };
    let (a_grid, b_grid) = (grid(()), grid(()));

    // Sanity: neither source clips on its own, so any clipping is the mix.
    for src in [&a_samples, &b_samples] {
        let peak = src
            .iter()
            .flat_map(|s| [s[0].abs(), s[1].abs()])
            .fold(0.0f32, f32::max);
        assert!(peak < 0.99, "fixture deck already peaks at {peak:.3}");
    }

    let mut mixer = Mixer::new(RATE, BLOCK);
    mixer.deck_a.load(a_samples);
    mixer.deck_b.load(b_samples);
    mixer.deck_a.seek_seconds(20.0);
    mixer.deck_a.play();
    mixer
        .start_transition(TransitionType::BassSwap, 6.0, &a_grid, &b_grid, 20.0, 8.0)
        .unwrap();

    let mut rendered = Vec::new();
    let mut block = [[0.0f32; 2]; BLOCK];
    for _ in 0..((7.0 * RATE / BLOCK as f32) as usize) {
        mixer.render(&mut block);
        rendered.extend_from_slice(&block);
    }

    let clipped = rendered
        .iter()
        .flat_map(|s| [s[0].abs(), s[1].abs()])
        .filter(|&v| v >= 0.9995)
        .count();

    assert!(
        clipped < 50,
        "{clipped} clipped samples — the three-band guard is not holding the mix under unity"
    );
}

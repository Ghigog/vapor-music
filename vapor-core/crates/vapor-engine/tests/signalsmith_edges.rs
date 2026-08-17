//! Signalsmith at the edges of how the mixer actually drives it.
//!
//! `signalsmith_realtime.rs` covers the steady state: no allocation, finite
//! output, a read position that tracks the ratio, and a true pass-through at
//! unity. These cover the transitions between those states, which is where the
//! wrapper's own bookkeeping lives and where its bugs have been.
//!
//! Separate binary from the realtime tests on purpose: those install a counting
//! global allocator, and a test that deliberately reconfigures the stretcher
//! would make its numbers mean something else.

use vapor_engine::signalsmith::Signalsmith;
use vapor_engine::TrackSource;

const RATE: usize = 44_100;

/// A tone with a slow envelope, so a discontinuity is visible as a jump rather
/// than hidden inside a waveform that was moving anyway.
fn tone(frames: usize) -> TrackSource {
    let samples = (0..frames)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let v = (0.4 * (std::f32::consts::TAU * 220.0 * t).sin() * i16::MAX as f32) as i16;
            [v, v]
        })
        .collect();
    TrackSource::Memory(samples)
}

fn finite(out: &[[f32; 2]]) -> bool {
    out.iter().flat_map(|f| [f[0], f[1]]).all(|s| s.is_finite())
}

fn peak(out: &[[f32; 2]]) -> f32 {
    out.iter()
        .flat_map(|f| [f[0].abs(), f[1].abs()])
        .fold(0.0f32, f32::max)
}

/// The mixer glides the ratio back to 1.0 after a transition, so it passes
/// through unity while playing. The wrapper takes a different path at unity —
/// a straight copy, bypassing the stretcher — and the stretcher's buffered
/// state goes stale while that is happening.
///
/// What must not happen is going back the other way and resuming from audio
/// that is 120 ms old.
#[test]
fn crossing_unity_in_both_directions_stays_finite_and_continuous() {
    let source = tone(RATE * 8);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    stretch.reset(0.0);

    let mut out = vec![[0.0f32; 2]; 512];
    // Down through unity and back up, as a glide does.
    let ratios = [1.04, 1.02, 1.0, 1.0, 0.98, 1.0, 1.03];

    let mut last_peak = None;
    for ratio in ratios {
        for _ in 0..4 {
            out.fill([0.0; 2]);
            stretch.process(&view, ratio, &mut out);
            assert!(finite(&out), "ratio {ratio} produced a non-finite sample");

            let p = peak(&out);
            assert!(p > 0.01, "ratio {ratio} produced near-silence ({p:.4})");
            // The source is a steady tone, so the block peak should stay in the
            // same neighbourhood throughout. A large jump is the stretcher
            // resuming from stale audio or restarting from silence.
            if let Some(previous) = last_peak {
                let jump: f32 = p / previous;
                assert!(
                    (0.25..4.0).contains(&jump),
                    "level jumped by {jump:.2}x crossing into ratio {ratio}"
                );
            }
            last_peak = Some(p);
        }
    }
}

/// The correction is computed from the ratio in force when the stream is
/// primed. A ratio change afterwards re-expresses the input-side latency in
/// different output frames, which is a known and bounded cost — but it must
/// stay bounded rather than compounding block after block.
#[test]
fn the_read_position_stays_honest_across_ratio_changes() {
    let source = tone(RATE * 20);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    stretch.reset(0.0);

    let mut out = vec![[0.0f32; 2]; 512];
    let mut expected = 0.0f64;

    for (block, ratio) in [1.05f64, 1.05, 1.02, 0.97, 1.01, 1.04]
        .into_iter()
        .enumerate()
    {
        for _ in 0..8 {
            stretch.process(&view, ratio, &mut out);
            expected += 512.0 * ratio;
        }
        let actual = stretch.source_position();
        assert!(
            (actual - expected).abs() < 2.0,
            "after block group {block} the position drifted to {actual} from {expected}"
        );
    }
}

/// A seek is a reset, and the stretcher must come back playing from the new
/// position rather than from wherever it was.
#[test]
fn a_reset_moves_the_read_position_and_keeps_producing() {
    let source = tone(RATE * 20);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    stretch.reset(0.0);

    let mut out = vec![[0.0f32; 2]; 512];
    for _ in 0..10 {
        stretch.process(&view, 1.03, &mut out);
    }
    assert!(stretch.source_position() > 0.0);

    let target = (RATE * 10) as f64;
    stretch.reset(target);
    assert_eq!(stretch.source_position(), target);

    out.fill([0.0; 2]);
    let rendered = stretch.process(&view, 1.03, &mut out);
    assert!(rendered.frames > 0, "nothing was produced after a reset");
    assert!(finite(&out));
    assert!(peak(&out) > 0.01, "the deck went silent after a reset");
}

/// Repeated resets are what scrubbing is, and each one re-primes. None of them
/// may leave the stretcher in a state that produces nothing.
#[test]
fn scrubbing_re_primes_every_time() {
    let source = tone(RATE * 30);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    let mut out = vec![[0.0f32; 2]; 256];

    for step in 0..12 {
        stretch.reset((step * RATE) as f64);
        out.fill([0.0; 2]);
        let rendered = stretch.process(&view, 1.02, &mut out);
        assert!(
            rendered.frames > 0,
            "seek to {step}s produced nothing at all"
        );
        assert!(finite(&out), "seek to {step}s produced a non-finite sample");
    }
}

/// Near the end there is not enough source left to prime against. That has to
/// come back as "no frames" rather than as a panic or as garbage — the deck
/// reads a short return as the end of the track and stops.
#[test]
fn running_off_the_end_is_a_short_read_not_a_crash() {
    let source = tone(RATE / 2);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    stretch.reset((RATE / 2) as f64 - 16.0);

    let mut out = vec![[0.0f32; 2]; 512];
    let rendered = stretch.process(&view, 1.05, &mut out);

    assert!(rendered.frames <= 512);
    assert!(finite(&out));
}

/// A source with nothing in it is a real state — a deck whose decoder has not
/// delivered yet — and must not panic.
#[test]
fn an_empty_source_produces_nothing() {
    let source = TrackSource::Memory(Vec::new());
    let view = source.view();
    let mut stretch = Signalsmith::new();
    let mut out = vec![[0.0f32; 2]; 256];

    let rendered = stretch.process(&view, 1.02, &mut out);
    assert_eq!(rendered.frames, 0);
}

/// Ratios past what the engine will schedule. `MAX_STRETCH` refuses these long
/// before the stretcher sees them, so this is not about sounding good — it is
/// about the block-size arithmetic staying inside its buffers whatever it is
/// handed.
#[test]
fn implausible_ratios_do_not_overrun_anything() {
    let source = tone(RATE * 10);
    let view = source.view();

    for ratio in [0.05f64, 0.5, 1.9, 4.0, 16.0] {
        let mut stretch = Signalsmith::new();
        stretch.reset(0.0);
        let mut out = vec![[0.0f32; 2]; 1024];

        for _ in 0..4 {
            out.fill([0.0; 2]);
            let rendered = stretch.process(&view, ratio, &mut out);
            assert!(finite(&out), "ratio {ratio} produced a non-finite sample");
            assert!(rendered.frames <= out.len());
        }
    }
}

/// A request larger than the stretcher's internal block, which the render path
/// serves in pieces. The seam between those pieces is a place a bug would sit.
#[test]
fn a_block_larger_than_the_internal_maximum_is_served_whole() {
    let source = tone(RATE * 20);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    stretch.reset(0.0);

    // MAX_BLOCK is 4096; ask for well over it.
    let mut out = vec![[0.0f32; 2]; 10_000];
    let rendered = stretch.process(&view, 1.02, &mut out);

    assert_eq!(rendered.frames, 10_000, "a large block came back short");
    assert!(finite(&out));
    // No silent gap where one piece met the next.
    let quiet = out.windows(64).filter(|w| peak(w) < 1e-6).count();
    assert_eq!(quiet, 0, "found a silent seam inside a large block");
}

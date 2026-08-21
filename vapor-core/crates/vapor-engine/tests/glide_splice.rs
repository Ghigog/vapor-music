//! Does the stretcher click when the post-mix glide reaches unity?
//!
//! `TEMPO_GLIDE_SECS` walks the playing deck's ratio back to 1.0 over six
//! seconds after every beat-matched mix. At the moment it arrives,
//! `Signalsmith::process` takes its pass-through branch, which resets the
//! vocoder and starts copying raw source samples. A phase vocoder's output is
//! not sample-aligned with its input, so that hand-over is a candidate
//! discontinuity — a click, at full volume, six seconds after each mix.
//!
//! Measured rather than argued: run a tone through a glide, and look at the
//! sample-to-sample steps around the switch against the steps the tone makes
//! on its own.

use vapor_engine::source::TrackSource;
use vapor_engine::stretch::{from_f32, Any, Quality};

const RATE: f32 = 44_100.0;
const HZ: f32 = 220.0;

fn tone(n: usize) -> Vec<[i16; 2]> {
    (0..n)
        .map(|i| {
            let v = (i as f32 / RATE * HZ * std::f32::consts::TAU).sin() * 0.5;
            let q = from_f32(v);
            [q, q]
        })
        .collect()
}

/// The largest jump between neighbouring samples in a run.
fn worst_step(out: &[[f32; 2]]) -> (f32, usize) {
    out.windows(2)
        .enumerate()
        .map(|(i, w)| ((w[1][0] - w[0][0]).abs(), i))
        .fold((0.0f32, 0), |a, b| if b.0 > a.0 { b } else { a })
}

#[test]
fn the_glide_reaching_unity_does_not_click() {
    let src = TrackSource::Memory(tone(RATE as usize * 8));
    let view = src.view();

    let mut s = Any::new(Quality::Signalsmith);
    s.reset(0.0);

    // A 220 Hz sine at 0.5 moves at most this much between two samples, which
    // is the yardstick: anything far above it is not the signal.
    let natural = (HZ / RATE * std::f32::consts::TAU) * 0.5;

    let mut out = vec![[0.0f32; 2]; 512];
    let mut all: Vec<[f32; 2]> = Vec::new();
    // Glide from 1.03 to exactly 1.0 over ~120 blocks, as `TempoGlide` does,
    // then keep going at unity — the switch happens on the first unity block.
    let blocks = 160;
    for b in 0..blocks {
        let t = (b as f64 / 120.0).min(1.0);
        let ratio = 1.03 + (1.0 - 1.03) * t;
        let r = s.process(&view, ratio, &mut out);
        all.extend_from_slice(&out[..r.frames]);
    }

    let (worst, at) = worst_step(&all);
    println!(
        "natural step {natural:.5}, worst {worst:.5} at sample {at} ({:.1}x natural)",
        worst / natural
    );
    assert!(
        worst < natural * 4.0,
        "a step of {worst:.4} at sample {at} is {:.0}x what the tone itself does — \
         the hand-over to pass-through is audible",
        worst / natural
    );
}

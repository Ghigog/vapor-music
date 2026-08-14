//! Play a beat-matched transition through the default audio device.
//!
//! Proves the real-time output path, which the offline `render` binary does
//! not: a file can be rendered slower than realtime and still sound perfect.
//! This has to keep up with the device callback.
//!
//! Usage:
//!   cargo run --release -p vapor-engine --bin play -- \
//!       <track_a> <track_b> [crossfade|bassswap|filtersweep] [duration]
//!
//! ## Real-time discipline (MIG-010)
//!
//! The mixer renders **inside the audio callback**, not into a buffer
//! beforehand. That is the arrangement a shipping player needs, and it is only
//! safe because `Mixer::render` allocates nothing once set up — asserted by
//! `tests/realtime_safety.rs`, which counts allocations through a custom global
//! allocator rather than trusting inspection.
//!
//! Everything that does allocate — decoding, analysis, loading, scheduling the
//! transition — happens on this thread before the stream starts. That split
//! between a control plane and a render path is the whole of real-time audio
//! design.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use vapor_engine::offline;

/// Largest block the mixer will render in one callback.
///
/// Allocated once before the stream starts; the callback never grows it, since
/// allocating on the audio thread is exactly what this design avoids. Devices
/// ask for far less than this in practice.
const MAX_CALLBACK_FRAMES: usize = 4096;

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut bpm_a = None;
    let mut bpm_b = None;
    let mut args: Vec<String> = Vec::new();
    for a in raw {
        if let Some(v) = a.strip_prefix("--bpm-a=") {
            bpm_a = v.parse().ok();
        } else if let Some(v) = a.strip_prefix("--bpm-b=") {
            bpm_b = v.parse().ok();
        } else {
            args.push(a);
        }
    }
    if args.len() < 2 {
        eprintln!("usage: play <track_a> <track_b> [crossfade|bassswap|filtersweep] [duration]");
        std::process::exit(1);
    }

    let Some(kind) = offline::parse_kind(args.get(2).map(|s| s.as_str())) else {
        eprintln!("unknown transition '{}'", args[2]);
        std::process::exit(1);
    };
    let duration: f32 = args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| kind.default_duration());

    // All the allocating work happens here, before the stream exists.
    let prepared = match offline::prepare_mix(
        std::path::Path::new(&args[0]),
        std::path::Path::new(&args[1]),
        kind,
        duration,
        bpm_a,
        bpm_b,
    ) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    let rate = prepared.sample_rate;
    let total_frames = prepared.total_frames;
    println!(
        "{kind:?}, tempo ratio {:.5} ({:+.2}%)\n{:.1}s at {rate} Hz — rendering in the audio callback",
        prepared.ratio,
        (prepared.ratio - 1.0) * 100.0,
        total_frames as f32 / rate as f32
    );

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no default output device");
    println!("device: {}", device.name().unwrap_or_else(|_| "?".into()));

    let mut config = device
        .default_output_config()
        .expect("no default output config")
        .config();
    config.channels = 2;
    config.sample_rate = cpal::SampleRate(rate);

    // Scratch the callback renders into. Sized once, here, so the callback
    // never needs to grow it.
    let mut scratch = vec![[0.0f32; 2]; MAX_CALLBACK_FRAMES];
    let rendered = Arc::new(AtomicUsize::new(0));
    let cb_rendered = Arc::clone(&rendered);
    let mut mixer = prepared.mixer;

    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let frames = (out.len() / 2).min(scratch.len());

                // Everything below is allocation-free and lock-free. If the
                // device asks for more frames than the scratch holds, the tail
                // is filled with silence rather than allocating a larger buffer
                // on the audio thread.
                mixer.render(&mut scratch[..frames]);

                for (i, frame) in scratch[..frames].iter().enumerate() {
                    out[i * 2] = frame[0];
                    out[i * 2 + 1] = frame[1];
                }
                for v in out[frames * 2..].iter_mut() {
                    *v = 0.0;
                }

                cb_rendered.fetch_add(frames, Ordering::Relaxed);
            },
            |err| eprintln!("stream error: {err}"),
            None,
        )
        .expect("failed to build output stream");

    stream.play().expect("failed to start stream");

    // Wait for playback to drain. A poll loop is fine here — this is a CLI
    // tool, not the engine.
    while rendered.load(Ordering::Relaxed) < total_frames {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    println!("done");
}

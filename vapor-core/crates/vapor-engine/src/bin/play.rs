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
//! The whole mix is rendered into a buffer *before* the stream starts, and the
//! callback only copies out of it. That is deliberate for a spike: it isolates
//! "does audio output work" from "is the engine real-time safe", so a glitch
//! here means a device or buffer-size problem and nothing else.
//!
//! A shipping player must render inside the callback, which means the audio
//! thread may not allocate, lock or block. `Mixer::render` is already
//! allocation-free after construction, but this binary does not yet prove that
//! under a real callback — that is phase 2 work, not something to claim now.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use vapor_engine::offline;

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

    let mix = match offline::render_mix(
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
    let rate = mix.sample_rate;
    let mixed = mix.samples;
    println!(
        "{kind:?}, tempo ratio {:.5} ({:+.2}%)\nrendered {:.1}s at {rate} Hz — starting playback",
        mix.ratio,
        (mix.ratio - 1.0) * 100.0,
        mixed.len() as f32 / rate as f32
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

    let cursor = Arc::new(AtomicUsize::new(0));
    let total = mixed.len();
    let buf = Arc::new(mixed);

    let cb_buf = Arc::clone(&buf);
    let cb_cursor = Arc::clone(&cursor);

    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let frames = out.len() / 2;
                let start = cb_cursor.fetch_add(frames, Ordering::Relaxed);
                for i in 0..frames {
                    let s = cb_buf.get(start + i).copied().unwrap_or([0.0; 2]);
                    out[i * 2] = s[0];
                    out[i * 2 + 1] = s[1];
                }
            },
            |err| eprintln!("stream error: {err}"),
            None,
        )
        .expect("failed to build output stream");

    stream.play().expect("failed to start stream");

    // Wait for playback to drain. A poll loop is fine here — this is a CLI
    // tool, not the engine.
    while cursor.load(Ordering::Relaxed) < total {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    println!("done");
}

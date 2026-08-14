//! Render a beat-matched transition between two real tracks to a WAV file.
//!
//! The automated tests prove alignment numerically on click tracks. This exists
//! so the result can be *listened to* on actual music, which is the only way to
//! judge whether a mix sounds right.
//!
//! Usage:
//!   cargo run --release -p vapor-engine --bin render -- \
//!       <track_a> <track_b> <out.wav> [crossfade|bassswap|filtersweep] [duration]

use std::path::Path;

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
    if args.len() < 3 {
        eprintln!(
            "usage: render <track_a> <track_b> <out.wav> \
             [crossfade|bassswap|filtersweep] [duration]"
        );
        std::process::exit(1);
    }

    let Some(kind) = offline::parse_kind(args.get(3).map(|s| s.as_str())) else {
        eprintln!("unknown transition '{}'", args[3]);
        std::process::exit(1);
    };
    let duration: f32 = args
        .get(4)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| kind.default_duration());

    let mix = match offline::render_mix(
        Path::new(&args[0]),
        Path::new(&args[1]),
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

    println!(
        "{kind:?} over {duration:.1}s starting at {:.1}s\n\
         tempo ratio {:.5} ({:+.2}% on the incoming deck)\n\
         rendered {:.1}s, peak {:.3}",
        mix.start_at,
        mix.ratio,
        (mix.ratio - 1.0) * 100.0,
        mix.samples.len() as f32 / mix.sample_rate as f32,
        mix.peak,
    );

    if let Err(e) = offline::write_wav(Path::new(&args[2]), &mix.samples, mix.sample_rate) {
        eprintln!("cannot write {}: {e}", args[2]);
        std::process::exit(1);
    }
    println!("wrote {}", args[2]);
}

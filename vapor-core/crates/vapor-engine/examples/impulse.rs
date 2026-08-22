//! Where does a click come out of Signalsmith, and what moves it?
//!
//! Deliberately no Vapor code in the path — this drives `signalsmith_stretch`
//! directly, so that whatever it says is about the library and not about the
//! wrapper around it.
//!
//! The experiment: a click at a known input frame, fed as a stream. Some
//! latency-compensation strategy runs first and consumes `P` frames of input
//! off the front. The output the caller keeps therefore begins at input frame
//! `P`, so the click is due out at `(CLICK - P) / ratio`. Anything else is the
//! error being hunted.

use signalsmith_stretch::Stretch;

const SR: u32 = 44_100;
const CLICK: usize = 22_050;
const SECONDS: usize = 6;

/// Interleaved stereo silence with a short burst at [`CLICK`].
///
/// A burst rather than a single sample: one sample is below the stretcher's
/// resolution and can vanish into a window entirely.
fn source() -> Vec<f32> {
    let mut s = vec![0.0f32; SR as usize * SECONDS * 2];
    for i in 0..32 {
        s[(CLICK + i) * 2] = 0.6;
        s[(CLICK + i) * 2 + 1] = 0.6;
    }
    s
}

fn peak(out: &[f32]) -> (usize, f32) {
    out.as_chunks::<2>()
        .0
        .iter()
        .enumerate()
        .map(|(i, f)| (i, f[0].abs()))
        .fold((0, 0.0), |acc, x| if x.1 > acc.1 { x } else { acc })
}

/// How each strategy primes the stretcher.
#[derive(Clone, Copy)]
enum Mode {
    /// No compensation at all — the baseline the error is measured against.
    None,
    /// Push input with no output pulled. Should retire `input_latency`.
    PushInput,
    /// `seek`, the API the library documents for exactly this.
    Seek,
    /// What `Signalsmith::prime` does today: feed `L·ratio` in, throw `L` out.
    PrimeAsWritten,
    /// Discard `Lₒ + Lᵢ/ratio` frames of output and no pre-roll at all —
    /// what the fitted mapping says the answer actually is.
    DiscardOutput,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::None => "none",
            Mode::PushInput => "push_input",
            Mode::Seek => "seek",
            Mode::PrimeAsWritten => "prime_as_written",
            Mode::DiscardOutput => "discard_output",
        }
    }
}

/// Returns (peak frame, expected frame, peak amplitude).
///
/// Each strategy reports two separate numbers, and conflating them is most of
/// why this took four attempts. `origin` is the source frame the caller
/// believes kept-output-frame-0 shows — that is what the error is measured
/// against. `fed` is how much input the priming actually swallowed, which only
/// says where the streaming loop resumes.
fn run(mode: Mode, ratio: f64) -> (usize, f64, f32) {
    let src = source();
    let mut st = Stretch::preset_default(2, SR);
    let li = st.input_latency();
    let lo = st.output_latency();

    let (origin, fed) = match mode {
        Mode::None => (0, 0),
        Mode::PushInput => {
            // Output length zero: add to the stream, take nothing back.
            st.process(&src[..li * 2], &mut []);
            (li, li)
        }
        Mode::Seek => {
            // The library's own answer. Documented as pre-roll that does not
            // move the stream position, so origin stays 0 by contract — one of
            // the things worth checking rather than believing.
            st.seek(&src[..li * 2], ratio);
            (0, 0)
        }
        Mode::PrimeAsWritten => {
            let l = li + lo;
            let n = ((l as f64) * ratio).ceil() as usize;
            let mut sink = vec![0.0f32; l * 2];
            st.process(&src[..n * 2], &mut sink);
            (n, n)
        }
        Mode::DiscardOutput => {
            // Both latencies, each in its own domain: output latency is
            // already output frames, input latency has to be divided by the
            // ratio to become them. Nothing is read from before the start, so
            // no history window is needed and the mix has no hole in it.
            let d = (lo as f64 + li as f64 / ratio).round() as usize;
            let n = (d as f64 * ratio).round() as usize;
            let mut sink = vec![0.0f32; d * 2];
            st.process(&src[..n * 2], &mut sink);
            (0, n)
        }
    };

    // Stream the rest in render-sized blocks, as the audio thread would.
    let want = 4096usize;
    let mut out = Vec::with_capacity(SR as usize * 2 * 2);
    let mut pos = fed;
    let mut scratch = vec![0.0f32; want * 2];
    while out.len() < SR as usize * 2 * 2 {
        let need = (want as f64 * ratio).round() as usize;
        if (pos + need) * 2 > src.len() {
            break;
        }
        st.process(&src[pos * 2..(pos + need) * 2], &mut scratch);
        out.extend_from_slice(&scratch);
        pos += need;
    }

    let (at, amp) = peak(&out);
    (at, (CLICK - origin) as f64 / ratio, amp)
}

fn main() {
    let probe = Stretch::preset_default(2, SR);
    println!(
        "input_latency {}, output_latency {}, total {} ({:.1} ms)\n",
        probe.input_latency(),
        probe.output_latency(),
        probe.input_latency() + probe.output_latency(),
        (probe.input_latency() + probe.output_latency()) as f64 / 44.1,
    );
    drop(probe);

    println!(
        "{:<18} {:>6} {:>9} {:>9} {:>10} {:>7}",
        "mode", "ratio", "peak", "expected", "off (ms)", "amp"
    );
    for mode in [
        Mode::None,
        Mode::PushInput,
        Mode::Seek,
        Mode::PrimeAsWritten,
        Mode::DiscardOutput,
    ] {
        for ratio in [1.02, 1.05, 0.95, 1.0] {
            let (at, expected, amp) = run(mode, ratio);
            println!(
                "{:<18} {:>6.2} {:>9} {:>9.0} {:>10.1} {:>7.3}",
                mode.name(),
                ratio,
                at,
                expected,
                (at as f64 - expected) / 44.1,
                amp,
            );
        }
    }
}

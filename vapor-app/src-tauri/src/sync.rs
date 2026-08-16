//! The thread that runs the drift correction during a mix (TD-21, MIG-009).
//!
//! `vapor_engine::pll` is the pure half — a phase detector and a waveform
//! correlation, neither of which knows where a deck's audio lives. This is the
//! half that does: it reads both playheads and both windows, and hands the
//! mixer a single scalar.
//!
//! ## Why it is a thread and not the supervisor
//!
//! The supervisor polls every 250 ms, which is its own business — it is
//! watching for a track ending. A correction refreshed four times a second is
//! not a loop, it is a series of steps. The Godot original ran the phase
//! detector every frame and refreshed the correlation on a 100 ms timer, and
//! that cadence is what this keeps.
//!
//! ## What was measured, and what it changed
//!
//! The grid half of the original is **inert here**. Both decks advance from the
//! same audio clock, so the distance from a deck's position to a beat in a
//! static grid is exactly what the arithmetic says — there is no error left for
//! it to find, and `tests/pll_drift.rs` measures no difference between running
//! it and not. The waveform correlation is the term that does the work, because
//! it is the only one that looks at the audio rather than at what the analysis
//! claimed about it. See `docs/MIGRATION.md`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use vapor_engine::mixer::BeatGrid;
use vapor_engine::pll::{cross_correlation_offset, Pll, MAX_LAG_SECS};
use vapor_engine::source::Window;

use crate::audio::Link;

/// How often the loop runs.
///
/// The correlation is refreshed on every tick, so this is the original's
/// `_pll_cc_timer` interval rather than its per-frame rate — the grid term that
/// ran per frame has nothing to contribute here.
const TICK: std::time::Duration = std::time::Duration::from_millis(100);

/// Span of audio the correlation compares, from `_apply_pll_sync`.
const CC_WINDOW_SECS: f32 = 0.5;

/// A running drift correction. Dropping it stops the thread.
pub struct DriftCorrection {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Everything the loop needs, gathered on the thread that armed the mix.
pub struct Inputs {
    pub link: Arc<Link>,
    pub outgoing_grid: BeatGrid,
    pub incoming_grid: BeatGrid,
    pub outgoing_window: Arc<Window>,
    pub incoming_window: Arc<Window>,
    /// The incoming deck's stretch, so the phase detector can convert between
    /// the two timelines.
    pub ratio: f64,
    /// Where in the outgoing track the mix begins.
    pub start_time_out: f32,
    /// Where in the incoming track it begins.
    pub cue_in: f32,
    /// Seconds before the loop engages — non-zero only for a Tempo Morph.
    pub delay_secs: f32,
}

impl DriftCorrection {
    /// Start correcting. Returns `None` when the grids cannot be anchored,
    /// which is not a failure — it means there is nothing to correct against.
    pub fn start(inputs: Inputs) -> Option<DriftCorrection> {
        let pll = Pll::arm(
            &inputs.outgoing_grid,
            &inputs.incoming_grid,
            inputs.start_time_out,
            inputs.cue_in,
            inputs.delay_secs,
        )?;

        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("vapor-drift".to_string())
                .spawn(move || run(pll, inputs, &stop))
                .ok()?
        };

        Some(DriftCorrection {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for DriftCorrection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run(mut pll: Pll, inputs: Inputs, stop: &AtomicBool) {
    let rate = inputs.link.sample_rate() as f32;
    let window = (CC_WINDOW_SECS * rate) as usize;
    let pad = (MAX_LAG_SECS * rate) as usize;

    // Allocated once, before the loop: this runs every 100 ms for the length of
    // a mix, and there is no reason to ask the allocator each time.
    let mut x = vec![0.0f32; window + 2 * pad];
    let mut y = vec![0.0f32; window];

    let dt = TICK.as_secs_f32();

    while !stop.load(Ordering::Acquire) {
        std::thread::sleep(TICK);
        pll.advance(dt);

        // The mix ended while this was sleeping. Leaving a correction applied
        // would bias whatever plays next.
        if !inputs.link.transition_armed() {
            inputs.link.set_pll_adjustment(0.0);
            continue;
        }
        if !pll.is_engaged() {
            continue;
        }

        let pos_out = inputs.link.snapshot().position as f32;
        let pos_in = inputs.link.incoming_position() as f32;

        // Both spans are read straight out of the decks' own windows, so this
        // is the audio actually being played rather than a second decode of it.
        let x_from = ((pos_out - CC_WINDOW_SECS / 2.0 - MAX_LAG_SECS) * rate) as i64;
        let y_from = ((pos_in - CC_WINDOW_SECS / 2.0) * rate) as i64;
        if x_from < 0 || y_from < 0 {
            continue;
        }
        inputs.outgoing_window.read_mono(x_from as u64, &mut x);
        inputs.incoming_window.read_mono(y_from as u64, &mut y);

        let cc = cross_correlation_offset(&x, &y, rate);
        let correction = pll.correction(
            &inputs.outgoing_grid,
            &inputs.incoming_grid,
            pos_out,
            pos_in,
            inputs.ratio,
            cc,
        );
        inputs.link.set_pll_adjustment(correction.adjustment);
    }

    // Whatever happens, the deck must not be left running at a corrected rate
    // once there is nothing to correct against.
    inputs.link.set_pll_adjustment(0.0);
}

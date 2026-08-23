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
//! claimed about it. See `docs/FINDINGS.md`.

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
        let Some((x_from, y_from)) = span_starts(pos_out, pos_in, rate) else {
            continue;
        };
        inputs.outgoing_window.read_mono(x_from, &mut x);
        inputs.incoming_window.read_mono(y_from, &mut y);

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

/// Where in each deck's window the two correlation spans begin, or `None` when
/// either would begin before its track does.
///
/// The outgoing span is the incoming one padded by [`MAX_LAG_SECS`] on each
/// side, because `cross_correlation_offset` searches `x` for `y` and can only
/// find it at a lag it was given room for.
///
/// The `None` is a guard, not an optimisation. `Window::read_mono` takes a
/// `u64` and zero-pads anything outside the window rather than refusing, so a
/// negative start cast straight to `u64` would not fail — it would wrap to
/// somewhere near the end of the address space, read zeros, and hand the
/// correlator a confident opinion about silence. Both decks are below the
/// padding for the first third of a second of every mix.
fn span_starts(pos_out: f32, pos_in: f32, rate: f32) -> Option<(u64, u64)> {
    let x_from = ((pos_out - CC_WINDOW_SECS / 2.0 - MAX_LAG_SECS) * rate) as i64;
    let y_from = ((pos_in - CC_WINDOW_SECS / 2.0) * rate) as i64;
    if x_from < 0 || y_from < 0 {
        return None;
    }
    Some((x_from as u64, y_from as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 8_000;

    /// A grid of `count` beats at 120 bpm, starting at time zero.
    fn grid(count: usize) -> BeatGrid {
        BeatGrid {
            bpm: 120.0,
            beats: (0..count).map(|i| i as f32 * 0.5).collect(),
        }
    }

    fn inputs(link: &Arc<Link>, outgoing: BeatGrid, incoming: BeatGrid, start_out: f32) -> Inputs {
        Inputs {
            link: Arc::clone(link),
            outgoing_grid: outgoing,
            incoming_grid: incoming,
            outgoing_window: Arc::new(Window::for_seconds(RATE, 5.0)),
            incoming_window: Arc::new(Window::for_seconds(RATE, 5.0)),
            ratio: 1.0,
            start_time_out: start_out,
            cue_in: 0.0,
            delay_secs: 0.0,
        }
    }

    #[test]
    fn a_mix_with_no_beats_to_anchor_to_starts_no_thread() {
        // Not a failure — an unanalysed track has no grid, and a mix out of
        // one is still a mix. It just has nothing to correct against.
        let link = Arc::new(Link::new(RATE));
        let correction = DriftCorrection::start(inputs(&link, grid(0), grid(0), 0.0));

        assert!(
            correction.is_none(),
            "a correction armed itself against a track with no beats in it"
        );
        assert_eq!(
            Arc::strong_count(&link),
            1,
            "no correction was returned but something is still holding the deck, \
             so a thread was spawned that nothing can stop"
        );
    }

    #[test]
    fn a_mix_beginning_after_the_last_beat_starts_no_thread() {
        // The outro case: a transition scheduled past the end of the detected
        // grid. `beat_at_or_after` has no answer, and inventing one would
        // anchor the loop to a beat that is not there.
        let link = Arc::new(Link::new(RATE));
        let correction = DriftCorrection::start(inputs(&link, grid(8), grid(8), 60.0));

        assert!(
            correction.is_none(),
            "a correction anchored itself to a beat past the end of the grid"
        );
        assert_eq!(Arc::strong_count(&link), 1, "the deck was left held");
    }

    #[test]
    fn dropping_a_correction_stops_its_thread_rather_than_leaving_it_running() {
        let link = Arc::new(Link::new(RATE));
        let correction =
            DriftCorrection::start(inputs(&link, grid(8), grid(8), 0.0)).expect("two grids anchor");
        assert_eq!(
            Arc::strong_count(&link),
            2,
            "the loop is running but is not holding the deck it reads"
        );

        let at = std::time::Instant::now();
        drop(correction);
        let took = at.elapsed();

        // The thread owns everything the loop needs, so a count back at one is
        // proof it returned rather than merely being asked to. A `Drop` that
        // stopped joining would pass a timing check and fail this.
        assert_eq!(
            Arc::strong_count(&link),
            1,
            "the drift thread is still holding the deck after its handle was dropped"
        );
        // One tick is 100 ms and the loop sleeps before it checks, so a second
        // is ten ticks of slack.
        assert!(
            took < std::time::Duration::from_secs(1),
            "dropping a correction took {took:?}, which is longer than the loop's own tick"
        );
    }

    #[test]
    fn neither_span_is_read_before_its_track_begins() {
        // Both decks sit below the padding for the first 0.35 s of every mix,
        // and a Tempo Morph's cue point is frame zero of the incoming track.
        let rate = RATE as f32;
        assert_eq!(
            span_starts(0.0, 0.0, rate),
            None,
            "the correlation read from before the start of both tracks"
        );
        assert_eq!(
            span_starts(10.0, 0.0, rate),
            None,
            "the incoming deck was read from before its own first frame"
        );
        assert_eq!(
            span_starts(0.0, 10.0, rate),
            None,
            "the outgoing deck was read from before its own first frame"
        );
    }

    #[test]
    fn the_guard_lifts_as_soon_as_both_spans_fit() {
        let rate = RATE as f32;
        // 0.35 s is exactly CC_WINDOW_SECS / 2 + MAX_LAG_SECS: the earliest
        // position whose outgoing span starts at or after zero.
        assert_eq!(
            span_starts(0.34, 1.0, rate),
            None,
            "the outgoing span began before the track did"
        );
        assert!(
            span_starts(0.36, 1.0, rate).is_some(),
            "the correlation stayed switched off past the point where it fits"
        );
    }

    #[test]
    fn the_outgoing_span_is_padded_for_the_lag_the_search_covers() {
        // `cross_correlation_offset` searches `x` for `y`, so `x` must reach
        // MAX_LAG_SECS either side of `y` or the lag it reports is clamped by
        // the buffer rather than by the audio.
        let rate = RATE as f32;
        let (x_from, y_from) = span_starts(1.0, 1.0, rate).expect("a second in, both spans fit");

        assert_eq!(
            y_from, 6_000,
            "the incoming span is not centred on the incoming playhead"
        );
        assert_eq!(
            x_from, 5_200,
            "the outgoing span is not centred on the outgoing playhead"
        );
        assert_eq!(
            x_from + (MAX_LAG_SECS * rate) as u64,
            y_from,
            "the outgoing span has lost its lead-in, so the search cannot find \
             the incoming audio arriving early"
        );
    }
}

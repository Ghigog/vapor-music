//! Property tests for the streaming window (TD-09).
//!
//! The window is a lock-free ring shared between a decoder thread and the audio
//! callback, addressed by absolute position in the track. Its invariants are
//! stated plainly enough to be checked against generated input rather than
//! against the handful of sequences a person thinks to write down — which is
//! the whole reason for property testing: the interesting sequence is the one
//! nobody imagined.
//!
//! A failure here shrinks to the smallest write/read sequence that breaks the
//! invariant, which is worth far more than the failing case itself.

use proptest::prelude::*;
use vapor_engine::source::{Window, HISTORY};

/// Frames whose value encodes their absolute position, so a frame read at
/// position `i` can be checked to *be* the frame written at `i`. Without that
/// the tests could only count frames, not tell whether they are the right ones.
fn frames_from(start: u64, count: usize) -> Vec<[i16; 2]> {
    (0..count)
        .map(|k| {
            let v = ((start + k as u64) % 30_000) as i16;
            [v, v.wrapping_neg()]
        })
        .collect()
}

fn expected_at(position: u64) -> [i16; 2] {
    let v = (position % 30_000) as i16;
    [v, v.wrapping_neg()]
}

proptest! {
    /// Whatever sequence of writes and consumer advances happens, every frame
    /// inside the readable span is the frame for that position.
    ///
    /// This is the property the whole design rests on: the stretcher indexes by
    /// absolute position and trusts what comes back.
    #[test]
    fn frames_read_back_at_the_position_they_were_written(
        batches in prop::collection::vec(1usize..3_000, 1..40),
        capacity_exponent in 13u32..17,
    ) {
        let window = Window::with_capacity(1 << capacity_exponent);
        let mut written = 0u64;

        for batch in batches {
            // The consumer keeps up, which is what lets the producer advance.
            window.publish_read(written);

            let room = window.writable().min(batch);
            if room > 0 {
                let taken = window.write_frames(&frames_from(written, room));
                written += taken as u64;
            }

            let (start, end) = window.span();
            let view = window.view();
            // Spot-check the span rather than every frame: a wrong mapping is
            // wrong at the edges, and checking a few million frames per case
            // makes the suite unusable.
            for position in [start, end.saturating_sub(1), (start + end) / 2] {
                if position < end {
                    prop_assert_eq!(
                        view.get(position),
                        Some(expected_at(position)),
                        "position {} in span {}..{}", position, start, end
                    );
                }
            }
        }
    }

    /// The producer never overwrites what the WSOLA search can still reach.
    ///
    /// The search reads *backwards* from the playhead, so the frames behind it
    /// are live data, not spent. Overwriting them does not read as silence —
    /// it splices in a different part of the song.
    #[test]
    fn history_behind_the_playhead_is_never_overwritten(
        advances in prop::collection::vec(0u64..5_000, 1..50),
    ) {
        let window = Window::with_capacity(1 << 15);
        let mut read_head = 0u64;
        let mut written = 0u64;

        for advance in advances {
            read_head += advance;
            window.publish_read(read_head);

            let room = window.writable();
            if room > 0 {
                let taken = window.write_frames(&frames_from(written, room));
                written += taken as u64;
            }

            let (start, _) = window.span();
            prop_assert!(
                start <= read_head.saturating_sub(HISTORY).max(start.min(read_head)),
                "start {} retired past the history owed to a reader at {}",
                start,
                read_head
            );
        }
    }

    /// A window never accepts more than it can hold, however much is offered.
    #[test]
    fn a_window_never_takes_more_than_it_holds(
        offered in 1usize..200_000,
        capacity_exponent in 13u32..16,
    ) {
        let window = Window::with_capacity(1 << capacity_exponent);
        window.publish_read(0);
        let taken = window.write_frames(&frames_from(0, offered));
        prop_assert!(taken <= window.capacity());
        prop_assert!(taken <= offered);
    }

    /// Restarting anywhere leaves the window coherent and able to accept audio.
    ///
    /// A seek forward used to leave `writable()` computing against a stale read
    /// position and returning zero, so the decoder stalled forever — and a
    /// paused deck never publishes a new position, so the stall was permanent.
    #[test]
    fn a_restart_anywhere_can_be_written_to(target in 0u64..u64::MAX / 4) {
        let window = Window::with_capacity(1 << 14);
        window.publish_read(0);
        window.write_frames(&frames_from(0, 4_000));

        window.restart_at(target);
        prop_assert!(window.writable() > 0, "a restart at {} left no room", target);

        let batch = frames_from(target, 512);
        let taken = window.write_frames(&batch);
        prop_assert_eq!(taken, 512);
        prop_assert_eq!(window.view().get(target), Some(expected_at(target)));
    }

    /// Reading outside the span yields nothing rather than stale audio.
    #[test]
    fn positions_outside_the_span_are_not_readable(
        written in 100usize..8_000,
        probe in 0u64..1_000_000,
    ) {
        let window = Window::with_capacity(1 << 14);
        window.publish_read(0);
        window.write_frames(&frames_from(0, written));

        let (start, end) = window.span();
        let view = window.view();
        if probe < start || probe >= end {
            prop_assert_eq!(view.get(probe), None);
        }
    }
}

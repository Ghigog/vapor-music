//! Where a deck's audio comes from — all of it, or a window onto it (TD-09).
//!
//! A deck used to hold `Vec<[i16; 2]>`: the whole decoded track, 55 MB for five
//! minutes and 110 MB with a second deck cued for a transition. This is the
//! same audio seen through a few seconds of window that a decoder thread keeps
//! filled, which costs about 1 MB a deck instead.
//!
//! ## Why this is not a queue
//!
//! The obvious streaming structure is a ring the consumer pops from. That does
//! not work here, and the reason is [`crate::stretch`]: WSOLA picks its splice
//! point by searching ±`SEARCH` frames around the ideal read position for the
//! offset that best correlates with what it has already emitted. It reads
//! *backwards*. A queue that hands out each frame once and forgets it cannot
//! answer that, and a stretcher that cannot search is a stretcher that clicks
//! every 23 ms.
//!
//! So this is a window with **random access over a span**, not a pipe. Frames
//! are addressed by absolute position in the track, the same number the beat
//! grid and the transition scheduler use, and the window keeps
//! [`HISTORY`] frames behind the playhead so the search always has somewhere to
//! look.
//!
//! ## No locks, and no `unsafe` either
//!
//! The audio thread may not block, so the window cannot be a `Mutex<Vec<_>>`;
//! the usual answer is a hand-written ring with `UnsafeCell` and raw pointers.
//! Storing each stereo frame as one [`AtomicU32`] avoids both: two `i16`s pack
//! into 32 bits exactly, relaxed atomic loads compile to plain loads on every
//! platform this ships to, and the whole thing stays safe Rust. It costs the
//! same four bytes a frame the `Vec` did.
//!
//! Publication is the ordinary release/acquire pair: the producer writes frames
//! with relaxed stores and then releases `write`; the consumer acquires `write`
//! and then reads frames. Nothing else is needed, because of the invariant
//! below.
//!
//! ## The invariant that makes reads safe without re-checking
//!
//! The consumer publishes where it is reading *before* it reads
//! ([`Window::publish_read`]), and the producer never retires a frame newer
//! than `read - HISTORY`. Since the stretcher never looks further back than
//! `SEARCH`, and `HISTORY` is comfortably larger, the producer physically
//! cannot overwrite a frame the consumer is about to touch. A view can
//! therefore be snapshotted once per block and indexed with plain bounds
//! checks, rather than re-validating every sample.
//!
//! ## Seeking, and where the waiting happens
//!
//! `AudioDSP::seek_pos` in the Godot build set a pending flag, woke the decoder
//! thread, and **blocked the caller** on a condition variable until the window
//! had been refilled. That is the right shape and the wrong thread: here the
//! caller is the audio callback, which may not wait for anything.
//!
//! So the wait moves to where blocking is legal. A seek is a request the
//! producer picks up; until it is served, the frames are simply not in the
//! window and the deck renders silence rather than stale audio. The control
//! thread does its waiting explicitly and off the audio path — see
//! [`Window::wait_until_ready`], which is what a track load blocks on.
//!
//! One thing falls out of addressing frames absolutely: a seek that lands
//! inside the window needs no refill at all, because the frames already there
//! are the frames for those positions. Scrubbing a few seconds is gapless,
//! where the Godot original flushed and re-decoded on every seek.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// How much audio a streaming deck holds.
///
/// Five seconds, which is `load_cache_at`'s window in `src/audio_dsp.cpp`
/// carried over rather than re-chosen. At 48 kHz stereo `i16` that is 960 KB a
/// deck — the number TD-09 set out to reach — and it is far more slack than a
/// decoder thread needs to stay ahead of playback.
pub const WINDOW_SECS: f32 = 5.0;

/// Frames kept behind the playhead.
///
/// Two things read backwards and both must fit:
///
/// * `stretch::SEARCH`, 256 frames, is how far back the WSOLA correlation
///   reaches for its splice point.
/// * The drift correction's waveform correlation (TD-21) reads
///   `window/2 + max_lag` = 0.35 s behind the playhead, which at 48 kHz is
///   about 16,800 frames and is by far the larger of the two.
///
/// 32,768 frames is 0.68 s at 48 kHz — comfortably over both, with the surplus
/// as scheduling slack so a producer that runs late cannot retire a frame out
/// from under a consumer that has not published its position yet. It costs
/// 128 KB of the window's 1 MiB.
pub const HISTORY: u64 = 32_768;

/// A window onto a track being decoded as it plays.
///
/// Shared between exactly two threads: one producer (a decoder) and one
/// consumer (the audio callback). More of either is not supported and not
/// needed — a deck has one of each.
pub struct Window {
    /// One stereo frame per slot. See the module docs for why these are
    /// atomics rather than an `UnsafeCell<[[i16; 2]]>`.
    slots: Box<[AtomicU32]>,
    mask: u64,
    /// One past the newest frame written.
    write: AtomicU64,
    /// The oldest frame still readable.
    start: AtomicU64,
    /// Where the consumer is reading. Published so the producer knows what it
    /// must not overwrite.
    read: AtomicU64,
    /// The frame a seek was requested to, with a sequence number so that asking
    /// twice for the same position is still two requests.
    seek_target: AtomicU64,
    seek_seq: AtomicU32,
    seek_served: AtomicU32,
    /// Set when the decoder has reached the end of the track.
    complete: AtomicBool,
    /// Total frames. Valid only once `total_known` is set.
    total: AtomicU64,
    total_known: AtomicBool,
}

impl Window {
    /// A window holding at least `frames`, rounded up to a power of two so the
    /// index-to-slot mapping is a mask rather than a division.
    pub fn with_capacity(frames: usize) -> Window {
        let capacity = frames.max(HISTORY as usize * 2).next_power_of_two();
        Window {
            slots: (0..capacity).map(|_| AtomicU32::new(0)).collect(),
            mask: capacity as u64 - 1,
            write: AtomicU64::new(0),
            start: AtomicU64::new(0),
            read: AtomicU64::new(0),
            seek_target: AtomicU64::new(0),
            seek_seq: AtomicU32::new(0),
            seek_served: AtomicU32::new(0),
            complete: AtomicBool::new(false),
            total: AtomicU64::new(0),
            total_known: AtomicBool::new(false),
        }
    }

    /// A window sized for `secs` of audio at `sample_rate`.
    pub fn for_seconds(sample_rate: u32, secs: f32) -> Window {
        Window::with_capacity((sample_rate as f32 * secs) as usize)
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    // ---- consumer side: the audio thread ----

    /// Announce where reading is about to happen.
    ///
    /// Must be called before the frames for a block are read, because it is
    /// what stops the producer retiring them. Cheap by design — one relaxed
    /// store per block.
    pub fn publish_read(&self, frame: u64) {
        self.read.store(frame, Ordering::Release);
    }

    /// Snapshot the readable span. Allocation-free and lock-free.
    pub fn view(&self) -> WindowView<'_> {
        let start = self.start.load(Ordering::Acquire);
        // Acquired *after* `start`, and it is this load that orders the frame
        // writes behind it.
        let end = self.write.load(Ordering::Acquire);
        WindowView {
            slots: &self.slots,
            mask: self.mask,
            start,
            end,
        }
    }

    /// Ask the producer to move the window to `frame`.
    ///
    /// Returns immediately. If the frame is already in the window the producer
    /// will not even need to touch the decoder, and reads succeed straight
    /// away; otherwise they fail until it has caught up, which the deck renders
    /// as silence rather than as the wrong audio.
    pub fn request_seek(&self, frame: u64) {
        self.seek_target.store(frame, Ordering::Relaxed);
        self.seek_seq.fetch_add(1, Ordering::Release);
    }

    /// True while a requested seek has not yet been served.
    pub fn seek_pending(&self) -> bool {
        self.seek_seq.load(Ordering::Acquire) != self.seek_served.load(Ordering::Acquire)
    }

    /// Total frames in the track, once anything knows.
    pub fn total_frames(&self) -> Option<u64> {
        if self.total_known.load(Ordering::Acquire) {
            Some(self.total.load(Ordering::Acquire))
        } else {
            None
        }
    }

    /// True once the decoder has read the track to its end.
    pub fn is_complete(&self) -> bool {
        self.complete.load(Ordering::Acquire)
    }

    // ---- producer side: the decoder thread ----

    /// How many frames may be appended without overwriting audio the consumer
    /// can still reach.
    pub fn writable(&self) -> usize {
        let read = self.read.load(Ordering::Acquire);
        let write = self.write.load(Ordering::Relaxed);
        let keep = read.saturating_sub(HISTORY);
        let room = (keep + self.capacity() as u64).saturating_sub(write) as usize;
        // Never more than the window holds. Immediately after a seek the
        // consumer can be *ahead* of the producer — it publishes where it
        // intends to read, the decoder lands on the packet boundary before it —
        // and without this clamp that arithmetic would invite a single write
        // long enough to lap its own history.
        room.min(self.capacity())
    }

    /// Append frames, returning how many were taken.
    ///
    /// A short return means the consumer has not moved on yet; the producer
    /// should hold the rest and try again rather than dropping them.
    pub fn write_frames(&self, frames: &[[i16; 2]]) -> usize {
        let n = frames.len().min(self.writable());
        if n == 0 {
            return 0;
        }

        let write = self.write.load(Ordering::Relaxed);
        // Retire before overwriting, never after: the consumer must be told a
        // frame is gone before the slot holding it changes.
        let retired = (write + n as u64).saturating_sub(self.capacity() as u64);
        if retired > self.start.load(Ordering::Relaxed) {
            self.start.store(retired, Ordering::Release);
        }

        for (i, frame) in frames[..n].iter().enumerate() {
            let slot = (write + i as u64) & self.mask;
            self.slots[slot as usize].store(pack(*frame), Ordering::Relaxed);
        }

        // Releasing this is what publishes every store above.
        self.write.store(write + n as u64, Ordering::Release);
        n
    }

    /// The frame a seek is waiting to be served at, if any.
    ///
    /// The producer decides what to do about it — a target already inside the
    /// window needs no decoder seek — and then calls [`served_seek`] with where
    /// it actually ended up.
    ///
    /// [`served_seek`]: Self::served_seek
    pub fn pending_seek(&self) -> Option<u64> {
        if self.seek_pending() {
            Some(self.seek_target.load(Ordering::Relaxed))
        } else {
            None
        }
    }

    /// Mark the outstanding seek as served.
    pub fn served_seek(&self) {
        self.seek_served
            .store(self.seek_seq.load(Ordering::Acquire), Ordering::Release);
    }

    /// Abandon the window's contents and resume at `frame`.
    ///
    /// Only needed when the target is outside what the window holds. Frames are
    /// addressed absolutely, so a target already inside it is already correct
    /// and needs nothing done at all.
    pub fn restart_at(&self, frame: u64) {
        self.start.store(frame, Ordering::Release);
        self.write.store(frame, Ordering::Release);
        // The read position moves with it. Leaving it where it was would make
        // `writable` compute against a position in the *old* part of the track:
        // after a seek forward it reports no room at all, so the producer
        // stalls until the consumer publishes again. A paused deck never
        // publishes, so without this a seek while paused would never fill and
        // pressing play would find silence.
        self.read.store(frame, Ordering::Release);
        self.complete.store(false, Ordering::Release);
    }

    /// Read `out.len()` frames from `from`, downmixed to mono, zero-padding
    /// anything outside the window.
    ///
    /// For the drift correction (TD-21), which needs the audio itself rather
    /// than what the analysis said about it. Reading straight from the window
    /// rather than decoding again is the point: those frames are already here,
    /// already at the device's rate, and already the ones being played.
    ///
    /// Zero-padding rather than refusing, because the correlation normalises
    /// and a short window at the very start of a track should weaken its
    /// opinion, not abort it.
    pub fn read_mono(&self, from: u64, out: &mut [f32]) {
        let view = self.view();
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = match view.get(from.wrapping_add(i as u64)) {
                Some(f) => (f[0] as f32 + f[1] as f32) / (2.0 * 32_768.0),
                None => 0.0,
            };
        }
    }

    /// The window currently holds `[start, write)`.
    pub fn span(&self) -> (u64, u64) {
        (
            self.start.load(Ordering::Acquire),
            self.write.load(Ordering::Acquire),
        )
    }

    pub fn set_total(&self, frames: u64) {
        self.total.store(frames, Ordering::Release);
        self.total_known.store(true, Ordering::Release);
    }

    /// Declare the track fully decoded. Also fixes the total, since by now it
    /// is known exactly rather than as whatever the container claimed.
    pub fn set_complete(&self) {
        self.set_total(self.write.load(Ordering::Acquire));
        self.complete.store(true, Ordering::Release);
    }

    /// Block until `frames` are available from `from`, or the track ends.
    ///
    /// **Never call this from the audio thread.** It is the control thread's
    /// half of the seek handshake the Godot build did with a condition
    /// variable: a track is not started until there is something to play, so
    /// playback opens with music rather than with a hole while the decoder
    /// catches up.
    ///
    /// Returns false on timeout, which means a decode slow enough that the
    /// caller should report a problem rather than start a track that stutters.
    pub fn wait_until_ready(&self, from: u64, frames: u64, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let (start, end) = self.span();
            if start <= from && (end >= from + frames || self.is_complete()) {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            // A sleep rather than a condvar: this runs once per track load, so
            // a millisecond of latency is free and a condition variable is a
            // second synchronisation mechanism to get right.
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

/// A snapshot of what a window holds, valid for one block.
pub struct WindowView<'a> {
    slots: &'a [AtomicU32],
    mask: u64,
    start: u64,
    end: u64,
}

impl WindowView<'_> {
    /// The frame at absolute position `i`, if it is readable right now.
    ///
    /// Public so the property tests can check the invariant this type exists
    /// to provide: a frame read at a position is the frame written there.
    #[inline]
    pub fn get(&self, i: u64) -> Option<[i16; 2]> {
        if i < self.start || i >= self.end {
            return None;
        }
        Some(unpack(
            self.slots[(i & self.mask) as usize].load(Ordering::Relaxed),
        ))
    }
}

/// Two `i16` in one `u32`, so a frame is a single atomic.
#[inline]
fn pack(frame: [i16; 2]) -> u32 {
    ((frame[0] as u16 as u32) << 16) | frame[1] as u16 as u32
}

#[inline]
fn unpack(bits: u32) -> [i16; 2] {
    [(bits >> 16) as u16 as i16, bits as u16 as i16]
}

/// A deck's audio: all of it in memory, or a window onto it.
///
/// Both variants stay because both are wanted. Streaming is what playback uses;
/// the whole-buffer form is what the offline tools, the render harness and
/// every beat-alignment test use, and being able to run the identical mixer
/// over both is what lets a test assert the two sound the same.
pub enum TrackSource {
    Empty,
    Memory(Vec<[i16; 2]>),
    Stream(Arc<Window>),
}

impl TrackSource {
    /// Announce the read position, for sources that care.
    pub fn publish_read(&self, frame: u64) {
        if let TrackSource::Stream(w) = self {
            w.publish_read(frame);
        }
    }

    /// Snapshot the readable span for one block.
    pub fn view(&self) -> SourceView<'_> {
        match self {
            TrackSource::Empty => SourceView::Empty,
            TrackSource::Memory(v) => SourceView::Memory(v),
            TrackSource::Stream(w) => SourceView::Stream(w.view(), w.is_complete()),
        }
    }

    /// Total frames, when known.
    pub fn total_frames(&self) -> Option<u64> {
        match self {
            TrackSource::Empty => Some(0),
            TrackSource::Memory(v) => Some(v.len() as u64),
            TrackSource::Stream(w) => w.total_frames(),
        }
    }

    /// The track's length in frames, falling back to how much has been decoded
    /// when the container declared nothing.
    ///
    /// A growing duration is a poor readout, but it beats a track that reports
    /// zero length for its whole life — and every container in the real library
    /// declares a length, so the fallback is for the odd stream that does not.
    pub fn duration_frames(&self) -> u64 {
        self.total_frames().unwrap_or_else(|| match self {
            TrackSource::Stream(w) => w.span().1,
            _ => 0,
        })
    }

    pub fn is_empty(&self) -> bool {
        match self {
            TrackSource::Empty => true,
            TrackSource::Memory(v) => v.is_empty(),
            TrackSource::Stream(_) => false,
        }
    }

    /// Move the source to `frame`. In-memory audio is already all there.
    pub fn seek(&self, frame: u64) {
        if let TrackSource::Stream(w) = self {
            w.request_seek(frame);
        }
    }
}

/// One block's worth of readable audio.
pub enum SourceView<'a> {
    Empty,
    Memory(&'a [[i16; 2]]),
    /// The window's span, plus whether the decoder has finished.
    Stream(WindowView<'a>, bool),
}

impl SourceView<'_> {
    /// The frame at absolute position `i`, if it is readable right now.
    #[inline]
    pub fn get(&self, i: u64) -> Option<[i16; 2]> {
        match self {
            SourceView::Empty => None,
            SourceView::Memory(v) => v.get(i as usize).copied(),
            SourceView::Stream(w, _) => w.get(i),
        }
    }

    /// One past the newest readable frame.
    #[inline]
    pub fn end(&self) -> u64 {
        match self {
            SourceView::Empty => 0,
            SourceView::Memory(v) => v.len() as u64,
            SourceView::Stream(w, _) => w.end,
        }
    }

    /// The oldest readable frame. Zero for audio held whole; for a window it is
    /// how far back the WSOLA search may still reach.
    #[inline]
    pub fn start(&self) -> u64 {
        match self {
            SourceView::Empty => 0,
            SourceView::Memory(_) => 0,
            SourceView::Stream(w, _) => w.start,
        }
    }

    /// Whether `end()` is the end of the *track*, or merely as far as the
    /// decoder has got.
    ///
    /// This is the distinction the whole streaming path turns on: running out
    /// of readable frames means the track finished in one case and the decoder
    /// fell behind in the other, and a player that confused them would skip to
    /// the next song on a hiccup.
    #[inline]
    pub fn is_complete(&self) -> bool {
        match self {
            SourceView::Empty => true,
            SourceView::Memory(_) => true,
            SourceView::Stream(_, complete) => *complete,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.end() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(from: i16, n: usize) -> Vec<[i16; 2]> {
        (0..n)
            .map(|i| [from.wrapping_add(i as i16), from.wrapping_add(i as i16) / 2])
            .collect()
    }

    /// The point of the whole exercise (TD-09), as a number.
    ///
    /// A deck used to cost the length of its track. Now it costs the length of
    /// its window, and a ninety-second interlude and a two-hour DJ set are the
    /// same size.
    #[test]
    fn a_deck_costs_the_same_whatever_the_track_length() {
        let bytes_per_frame = std::mem::size_of::<AtomicU32>();
        assert_eq!(
            bytes_per_frame, 4,
            "a stereo i16 frame must still be 4 bytes"
        );

        let window = Window::for_seconds(48_000, WINDOW_SECS);
        let cost = window.capacity() * bytes_per_frame;

        // 5 s at 48 kHz is 240,000 frames, rounded up to 262,144 for the mask:
        // exactly 1 MiB.
        assert_eq!(cost, 1024 * 1024, "a streaming deck costs {cost} bytes");

        // Against what it replaced: a five-minute track held whole.
        let whole = 5 * 60 * 48_000 * bytes_per_frame;
        assert!(
            whole / cost >= 50,
            "streaming saved only {}x, which is not worth the machinery",
            whole / cost
        );

        // And an hour-long set costs exactly the same, which is the property
        // that actually matters — the old cost had no upper bound at all.
        assert_eq!(
            Window::for_seconds(48_000, WINDOW_SECS).capacity(),
            window.capacity()
        );
    }

    #[test]
    fn a_frame_survives_the_round_trip_through_a_slot() {
        for f in [
            [0i16, 0],
            [1, -1],
            [i16::MIN, i16::MAX],
            [-32_768, 32_767],
            [12_345, -6_789],
        ] {
            assert_eq!(unpack(pack(f)), f, "{f:?} did not survive packing");
        }
    }

    #[test]
    fn written_frames_read_back_at_their_absolute_positions() {
        let w = Window::with_capacity(1024);
        let data = frames(100, 500);
        assert_eq!(w.write_frames(&data), 500);

        let view = w.view();
        for (i, f) in data.iter().enumerate() {
            assert_eq!(view.get(i as u64), Some(*f), "frame {i}");
        }
        assert_eq!(view.get(500), None, "read past the end succeeded");
    }

    /// The property the WSOLA search depends on: after the playhead has moved
    /// on, the frames behind it are still there to correlate against.
    #[test]
    fn history_behind_the_playhead_stays_readable() {
        let w = Window::with_capacity(1 << 15);
        let total = 1 << 14;
        assert_eq!(w.write_frames(&frames(0, total)), total);

        let read = total as u64 - 100;
        w.publish_read(read);

        let view = w.view();
        for back in 1..=super::HISTORY.min(read) {
            assert!(
                view.get(read - back).is_some(),
                "frame {back} behind the playhead had already gone"
            );
        }
    }

    /// The producer must stall rather than overwrite audio the search can still
    /// reach. Without this the correlation reads whatever the decoder has just
    /// written over it — not silence, but the wrong part of the song.
    #[test]
    fn the_producer_refuses_to_overwrite_reachable_history() {
        let w = Window::with_capacity(1 << 13);
        let capacity = w.capacity();
        w.publish_read(0);

        // Fill it completely.
        let written = w.write_frames(&frames(0, capacity * 2));
        assert_eq!(written, capacity, "the window took more than it holds");
        assert_eq!(w.writable(), 0, "a full window still reports space");

        // A consumer that has not yet moved past the history it is owed frees
        // nothing, however far the producer would like to run ahead.
        w.publish_read(HISTORY - 1);
        assert_eq!(
            w.writable(),
            0,
            "history the search can still reach was released"
        );

        // Beyond that, exactly what the consumer has passed becomes writable.
        w.publish_read(HISTORY + 1000);
        assert_eq!(
            w.writable(),
            1000,
            "space freed does not match how far the consumer moved"
        );
    }

    /// Frames are addressed by position in the track, so a window that has
    /// wrapped many times still answers about the right audio.
    #[test]
    fn positions_stay_absolute_across_wraps() {
        let capacity = 1024;
        let w = Window::with_capacity(capacity);

        let mut next = 0u64;
        for round in 0..20u64 {
            w.publish_read(next);
            let batch: Vec<[i16; 2]> = (0..256)
                .map(|i| {
                    let v = (round * 256 + i) as i16;
                    [v, v]
                })
                .collect();
            let n = w.write_frames(&batch);
            next += n as u64;
        }

        let (start, end) = w.span();
        let view = w.view();
        for i in start..end {
            let expected = i as i16;
            assert_eq!(
                view.get(i),
                Some([expected, expected]),
                "position {i} held the wrong audio after wrapping"
            );
        }
    }

    /// A seek to somewhere the window already covers must not disturb it —
    /// this is what makes scrubbing a few seconds gapless rather than a refill.
    #[test]
    fn a_seek_inside_the_window_leaves_the_audio_in_place() {
        let w = Window::with_capacity(1 << 14);
        w.write_frames(&frames(0, 8000));

        w.request_seek(4000);
        assert!(w.seek_pending());

        let target = w.pending_seek().expect("a seek was requested");
        let (start, end) = w.span();
        assert!(
            target >= start && target < end,
            "the target was expected to be inside the window"
        );

        // The producer serves it without restarting, because the frames for
        // those positions are already the right ones.
        w.served_seek();
        assert!(!w.seek_pending());
        assert_eq!(w.view().get(4000), Some(frames(0, 8000)[4000]));
    }

    #[test]
    fn restarting_moves_the_window_and_clears_completion() {
        let w = Window::with_capacity(1024);
        w.write_frames(&frames(0, 512));
        w.set_complete();
        assert!(w.is_complete());

        w.restart_at(100_000);
        assert!(
            !w.is_complete(),
            "a restarted window still claimed to be done"
        );
        assert_eq!(w.span(), (100_000, 100_000));
        assert_eq!(w.view().get(0), None, "audio from before the seek survived");

        let data = frames(7, 64);
        w.write_frames(&data);
        assert_eq!(w.view().get(100_000), Some(data[0]));
    }

    /// A window restarted far ahead of where the consumer last was must accept
    /// audio immediately.
    ///
    /// It did not: `writable` was computed against the stale read position, so
    /// a forward seek reported no room and the producer stalled until the
    /// consumer published again. A paused deck never publishes, so a seek while
    /// paused would have filled nothing and playing would have found silence.
    #[test]
    fn a_window_restarted_ahead_of_the_consumer_still_accepts_audio() {
        let w = Window::with_capacity(1024);
        w.publish_read(0);
        w.write_frames(&frames(0, 512));

        w.restart_at(5_000_000);
        assert!(
            w.writable() > 0,
            "a restarted window reported no room, so the decoder would stall"
        );

        let data = frames(7, 64);
        assert_eq!(w.write_frames(&data), 64);
        assert_eq!(w.view().get(5_000_000), Some(data[0]));
    }

    /// Completion fixes the length at what was actually decoded, which may
    /// differ from whatever the container claimed.
    #[test]
    fn completion_reports_the_length_actually_decoded() {
        let w = Window::with_capacity(4096);
        w.set_total(999_999);
        w.write_frames(&frames(0, 700));
        w.set_complete();
        assert_eq!(w.total_frames(), Some(700));
    }

    /// The distinction the player depends on: no more frames because the track
    /// ended, versus no more frames because the decoder is behind.
    #[test]
    fn an_incomplete_window_is_not_the_end_of_the_track() {
        let w = Arc::new(Window::with_capacity(4096));
        w.write_frames(&frames(0, 100));

        let source = TrackSource::Stream(Arc::clone(&w));
        assert!(
            !source.view().is_complete(),
            "an unfinished decode read as complete"
        );

        w.set_complete();
        assert!(source.view().is_complete());
    }

    #[test]
    fn waiting_gives_up_rather_than_hanging_on_a_dead_producer() {
        let w = Window::with_capacity(4096);
        let waited = w.wait_until_ready(0, 1000, std::time::Duration::from_millis(20));
        assert!(!waited, "waiting claimed success with nothing decoded");
    }

    #[test]
    fn waiting_returns_as_soon_as_the_frames_arrive() {
        let w = Arc::new(Window::with_capacity(1 << 14));
        let producer = Arc::clone(&w);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(5));
            producer.write_frames(&frames(0, 4000));
        });

        assert!(
            w.wait_until_ready(0, 2000, std::time::Duration::from_secs(2)),
            "the wait timed out on a producer that did deliver"
        );
    }

    /// An in-memory source is complete by definition, so the same deck code
    /// works for both without a special case at every call site.
    #[test]
    fn an_in_memory_source_is_always_complete() {
        let source = TrackSource::Memory(frames(0, 10));
        let view = source.view();
        assert!(view.is_complete());
        assert_eq!(view.end(), 10);
        assert_eq!(view.get(3), Some(frames(0, 10)[3]));
        assert_eq!(view.get(10), None);
    }
}

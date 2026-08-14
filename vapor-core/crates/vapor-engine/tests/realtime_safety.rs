//! Proof that the audio path does not allocate (MIG-010).
//!
//! An audio callback runs on a real-time thread with a hard deadline, typically
//! a few milliseconds. Allocating there can block on a lock inside the
//! allocator, and when it does the result is a dropout — not a slowdown. The
//! usual way this is "verified" is by listening and hoping, which finds the
//! problem only on the machines and workloads that happen to trigger it.
//!
//! This measures it instead: a counting global allocator wraps the system one,
//! and the test asserts that a warmed-up `Mixer::render` performs **zero**
//! allocations, across a transition and the tempo glide that follows.
//!
//! It lives in its own integration test file deliberately. Each integration
//! test file is a separate binary, so the global allocator here affects nothing
//! else; installed in a unit test it would count every other test's allocations
//! running in parallel and report noise.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use vapor_engine::mixer::BeatGrid;
use vapor_engine::{Mixer, TransitionType};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static COUNTING: AtomicBool = AtomicBool::new(false);

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

/// Serialises the two tests in this file.
///
/// `COUNTING` and `ALLOCS` are process-wide, and cargo runs tests in a binary
/// in parallel by default, so without this the two would interleave and each
/// would measure the other's allocations. Taking the lock is what makes the
/// result meaningful rather than dependent on `--test-threads`.
static MEASURING: Mutex<()> = Mutex::new(());

/// Begin counting, returning a guard that stops counting when dropped.
fn measure() -> MutexGuard<'static, ()> {
    let guard = MEASURING.lock().unwrap_or_else(|e| e.into_inner());
    ALLOCS.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    guard
}

fn stop_measuring() -> usize {
    COUNTING.store(false, Ordering::Relaxed);
    ALLOCS.load(Ordering::Relaxed)
}

const RATE: f32 = 44100.0;
const BLOCK: usize = 512;

fn click_track(bpm: f32, secs: f32) -> (Vec<[f32; 2]>, BeatGrid) {
    let n = (secs * RATE) as usize;
    let mut samples = vec![[0.0f32; 2]; n];
    let period = 60.0 / bpm;

    let mut beats = Vec::new();
    let mut t = 0.0f32;
    while t < secs {
        beats.push(t);
        let start = (t * RATE) as usize;
        for k in 0..(RATE * 0.005) as usize {
            if start + k >= n {
                break;
            }
            let tt = k as f32 / RATE;
            let env = (1.0 - tt / 0.005).max(0.0);
            let v = env * (2.0 * std::f32::consts::PI * 2000.0 * tt).sin() * 0.8;
            samples[start + k] = [v, v];
        }
        t += period;
    }
    (samples, BeatGrid { bpm, beats })
}

/// The whole point: rendering must be allocation-free once set up.
///
/// Everything that allocates — decoding, loading, scratch buffers, the
/// transition object — happens on the control side before playback, which is
/// exactly the split a real-time audio design requires.
#[test]
fn render_does_not_allocate() {
    let (a_samples, a_grid) = click_track(128.0, 120.0);
    let (b_samples, b_grid) = click_track(126.0, 120.0);

    let mut mixer = Mixer::new(RATE, BLOCK);
    mixer.deck_a.load(a_samples);
    mixer.deck_b.load(b_samples);
    mixer.deck_a.seek_seconds(20.0);
    mixer.deck_a.play();

    let mut block = [[0.0f32; 2]; BLOCK];

    // Warm up outside the measurement: the first render may touch lazily
    // initialised state, and a real player would have done so before audio
    // started.
    for _ in 0..16 {
        mixer.render(&mut block);
    }

    // Scheduling a transition is a control-plane action and may allocate; it is
    // deliberately outside the measured region.
    mixer
        .start_transition(TransitionType::BassSwap, 6.0, &a_grid, &b_grid, 20.0, 8.0)
        .expect("transition should schedule");

    // Measure across the transition and the tempo glide that follows it, which
    // together exercise every branch in the render path: two decks summing, EQ
    // automation, the clipping guard, the limiter, the deck-role swap and the
    // glide.
    let _guard = measure();

    let blocks = ((14.0 * RATE) / BLOCK as f32) as usize;
    for _ in 0..blocks {
        mixer.render(&mut block);
    }

    let count = stop_measuring();

    assert_eq!(
        count, 0,
        "render allocated {count} times across {blocks} blocks; \
         an allocation on the audio thread can block and cause a dropout"
    );

    // Guard against the measurement silently testing nothing.
    assert!(
        !mixer.is_transitioning(),
        "the transition should have completed"
    );
}

/// The counting allocator must actually work, or the test above passes
/// vacuously and proves nothing.
#[test]
fn the_allocation_counter_detects_allocation() {
    let _guard = measure();
    let v: Vec<u8> = Vec::with_capacity(4096);
    let count = stop_measuring();
    drop(v);
    assert!(
        count > 0,
        "the counter failed to observe a known allocation"
    );
}

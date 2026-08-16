//! Signalsmith Stretch on the audio thread (MIG-012).
//!
//! The engine's whole real-time discipline is that `render` neither allocates
//! nor frees, asserted by a counting allocator rather than by inspection. A
//! stretcher is the largest thing on that path, and this one is C++ with its
//! own memory model — so the question is not rhetorical, and the answer
//! decides whether it can be used at all.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use vapor_engine::signalsmith::Signalsmith;
use vapor_engine::source::{SourceView, TrackSource};

/// Counts allocations on this thread only. A process-wide counter measures the
/// test harness as well, which is how an earlier version of this measured zero
/// while allocating freely.
struct Counting;

thread_local! {
    static COUNTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.with(|c| c.get()) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNTING.with(|c| c.get()) {
            FREES.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn tone(frames: usize) -> TrackSource {
    let samples: Vec<[i16; 2]> = (0..frames)
        .map(|i| {
            let t = i as f32 / 44_100.0;
            let v = ((t * 220.0 * std::f32::consts::TAU).sin() * 12_000.0) as i16;
            [v, v]
        })
        .collect();
    TrackSource::Memory(samples)
}

/// The counter has to be able to see an allocation, or "zero" means nothing.
#[test]
fn the_counter_observes_a_known_allocation() {
    ALLOCS.store(0, Ordering::Relaxed);
    COUNTING.with(|c| c.set(true));
    let v: Vec<u8> = Vec::with_capacity(1024);
    COUNTING.with(|c| c.set(false));

    assert!(ALLOCS.load(Ordering::Relaxed) > 0, "the counter is blind");
    drop(v);
}

#[test]
fn stretching_does_not_allocate_once_it_is_running() {
    let source = tone(44_100 * 4);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    let mut out = vec![[0.0f32; 2]; 512];

    // Warm up outside the count: the first block is where any lazily-built
    // internal buffer would be built, and building it once is not the failure
    // this is looking for.
    for _ in 0..8 {
        stretch.process(&view, 1.03, &mut out);
    }

    ALLOCS.store(0, Ordering::Relaxed);
    FREES.store(0, Ordering::Relaxed);
    COUNTING.with(|c| c.set(true));
    for _ in 0..200 {
        stretch.process(&view, 1.03, &mut out);
    }
    COUNTING.with(|c| c.set(false));

    assert_eq!(
        (
            ALLOCS.load(Ordering::Relaxed),
            FREES.load(Ordering::Relaxed)
        ),
        (0, 0),
        "Signalsmith allocated on the audio thread across 200 blocks"
    );
}

/// Unity has to stay a copy. A mix that is not stretching should not be paying
/// for an FFT, and should not be coloured by one either.
#[test]
fn unity_is_a_pass_through() {
    let source = tone(44_100);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    let mut out = vec![[0.0f32; 2]; 256];

    stretch.process(&view, 1.0, &mut out);

    let SourceView::Memory(samples) = &view else {
        panic!("memory source");
    };
    for (i, frame) in out.iter().enumerate() {
        let expected = samples[i][0] as f32 / 32_768.0;
        assert!(
            (frame[0] - expected).abs() < 1e-6,
            "frame {i} was altered at unity"
        );
    }
}

/// The source position is what the mixer schedules cue points from, so it has
/// to advance at the ratio and not at the output rate.
#[test]
fn the_read_position_advances_at_the_ratio() {
    let source = tone(44_100 * 4);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    let mut out = vec![[0.0f32; 2]; 512];

    stretch.reset(0.0);
    for _ in 0..10 {
        stretch.process(&view, 1.05, &mut out);
    }

    let expected = 10.0 * 512.0 * 1.05;
    let actual = stretch.source_position();
    assert!(
        (actual - expected).abs() < 1.0,
        "read position {actual} should be about {expected}"
    );
}

/// Silence in, silence out — and more importantly, no NaN. A stretcher that
/// emits NaN takes the master limiter with it.
#[test]
fn the_output_is_finite() {
    let source = tone(44_100 * 2);
    let view = source.view();
    let mut stretch = Signalsmith::new();
    let mut out = vec![[0.0f32; 2]; 512];

    for ratio in [0.94, 0.98, 1.02, 1.06] {
        stretch.reset(0.0);
        for _ in 0..20 {
            stretch.process(&view, ratio, &mut out);
            for frame in &out {
                assert!(
                    frame[0].is_finite() && frame[1].is_finite(),
                    "non-finite output at ratio {ratio}"
                );
            }
        }
    }
}

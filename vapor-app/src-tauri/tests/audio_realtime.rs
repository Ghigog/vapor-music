//! Proof that the shell's audio path does not allocate (TD-03, MIG-010).
//!
//! `vapor-engine` already proves `Mixer::render` is allocation-free. That
//! guarantee is worth nothing if the plumbing the shell wraps it in allocates,
//! and this is where the plumbing lives: a command queue drained on the audio
//! thread, a track buffer displaced at every track change, and the interleave
//! into the device's own format.
//!
//! The track change is the case that matters and the one inspection misses. A
//! deck loaded the obvious way frees the previous track's samples where it
//! stands — tens of megabytes, inside the callback, producing a dropout that
//! only ever happens at the moment one track becomes the next. That is a bug
//! nobody reproduces on demand and everybody hears eventually.
//!
//! Same method as `vapor-engine`'s: a counting global allocator, in its own
//! integration binary so it measures nothing but this — but counting per
//! thread rather than per process, for the reason recorded below.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use vapor_app_lib::audio::{Engine, Link};

// The counters are thread-local, not global.
//
// A global counter measures the whole process, and cargo runs a binary's tests
// on several threads at once — so a global one also counts the harness
// formatting another test's result line. That is not a hypothetical: with a
// process-wide counter these tests reported 3 allocations for a render path
// that performs none, and passed only under `--test-threads=1`. A test that is
// wrong about which thread it is watching is worse than no test, because it
// invites the real number to be explained away as noise.
//
// `const`-initialised `Cell`s so the thread-local itself never allocates on
// first touch, which would recurse straight back into here.
thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static FREES: Cell<usize> = const { Cell::new(0) };
}

/// `try_with` rather than `with`: during thread teardown the thread-local is
/// already gone, and a panic raised inside the allocator aborts the process.
fn bump(counter: &'static std::thread::LocalKey<Cell<usize>>) {
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
        let _ = counter.try_with(|c| c.set(c.get() + 1));
    }
}

struct CountingAlloc;

// Frees are counted as well as allocations. Returning a buffer to the allocator
// takes the same locks that taking one does, so a `dealloc` on the audio thread
// is exactly as dangerous — and it is the one this module's design exists to
// move elsewhere.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump(&ALLOCS);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        bump(&FREES);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump(&ALLOCS);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

fn measure() {
    ALLOCS.with(|c| c.set(0));
    FREES.with(|c| c.set(0));
    COUNTING.with(|c| c.set(true));
}

/// Returns (allocations, frees) on this thread since [`measure`].
fn stop_measuring() -> (usize, usize) {
    COUNTING.with(|c| c.set(false));
    (ALLOCS.with(Cell::get), FREES.with(Cell::get))
}

const RATE: u32 = 44_100;
const BLOCK: usize = 512;

/// A tone rather than silence, so the limiter and the EQ chain have real signal
/// to work on rather than a path of zeroes that skips work.
fn tone(seconds: f32) -> Vec<[i16; 2]> {
    use vapor_engine::stretch::from_f32;
    let n = (seconds * RATE as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let l = (t * 220.0 * std::f32::consts::TAU).sin() * 0.6;
            let r = (t * 330.0 * std::f32::consts::TAU).sin() * 0.6;
            [from_f32(l), from_f32(r)]
        })
        .collect()
}

fn setup() -> (Arc<Link>, Engine, Vec<f32>) {
    let link = Arc::new(Link::new(RATE));
    let engine = Engine::new(Arc::clone(&link), 2);
    (link, engine, vec![0.0f32; BLOCK * 2])
}

/// The main claim: steady-state playback allocates and frees nothing.
#[test]
fn steady_playback_neither_allocates_nor_frees() {
    let (link, mut engine, mut out) = setup();

    assert!(link.load(tone(20.0), true));
    // Warm up outside the measurement — the first blocks apply the load and
    // touch lazily initialised state, and a real player would have done so
    // before anyone was listening.
    for _ in 0..32 {
        engine.render(&mut out);
    }

    measure();
    for _ in 0..1_000 {
        engine.render(&mut out);
    }
    let (allocs, frees) = stop_measuring();

    assert_eq!(
        (allocs, frees),
        (0, 0),
        "steady playback performed {allocs} allocations and {frees} frees; \
         either can block on a lock inside the allocator and cause a dropout"
    );
}

/// The case inspection misses: swapping a track must not free the old buffer on
/// the audio thread. Loading the naive way would fail exactly here.
#[test]
fn changing_track_does_not_free_on_the_audio_thread() {
    let (link, mut engine, mut out) = setup();

    assert!(link.load(tone(10.0), true));
    for _ in 0..32 {
        engine.render(&mut out);
    }

    // Queued before measuring: enqueueing is the control thread's job and is
    // allowed to allocate. Only the drain is under test.
    assert!(link.load(tone(10.0), true));

    measure();
    for _ in 0..8 {
        engine.render(&mut out);
    }
    let (allocs, frees) = stop_measuring();

    assert_eq!(
        (allocs, frees),
        (0, 0),
        "a track change cost {allocs} allocations and {frees} frees on the \
         audio thread; the displaced buffer must travel back to the control \
         thread to be dropped there"
    );

    // Guard against measuring nothing: the load must actually have been
    // applied, or this passes vacuously.
    assert!(
        (link.snapshot().duration - 10.0).abs() < 0.01,
        "the queued track was never loaded, so nothing was measured"
    );
}

/// Every transport command runs on the audio thread. All of them must be free
/// of allocation, not just the common one.
#[test]
fn transport_commands_do_not_allocate() {
    let (link, mut engine, mut out) = setup();

    assert!(link.load(tone(30.0), true));
    for _ in 0..32 {
        engine.render(&mut out);
    }

    link.pause();
    link.play();
    link.seek(12.5);
    link.set_volume(0.4);
    link.stop();

    measure();
    for _ in 0..8 {
        engine.render(&mut out);
    }
    let (allocs, frees) = stop_measuring();

    assert_eq!((allocs, frees), (0, 0), "a transport command allocated");
}

/// A track that runs out must report exactly one ending, and only because it
/// ended. Anything else here either strands the queue or skips a song.
#[test]
fn a_finished_track_reports_one_ending() {
    let (link, mut engine, mut out) = setup();

    // Shorter than the blocks rendered below, so it is certain to finish.
    assert!(link.load(tone(0.05), true));
    for _ in 0..40 {
        engine.render(&mut out);
    }

    assert!(link.take_ended(), "a finished track reported no ending");
    assert!(!link.take_ended(), "one ending was reported twice");
}

/// A device asking for an empty buffer produces nothing for the same reason a
/// finished track does. Reading that as an ending would advance the queue
/// because a callback came back empty.
#[test]
fn an_empty_callback_is_not_mistaken_for_the_end_of_a_track() {
    let (link, mut engine, mut out) = setup();

    assert!(link.load(tone(20.0), true));
    for _ in 0..8 {
        engine.render(&mut out);
    }

    for _ in 0..8 {
        engine.render(&mut out[..0]);
    }

    assert!(
        !link.take_ended(),
        "an empty callback was reported as the track ending"
    );
}

/// Pausing must not read as an ending either — otherwise pressing pause skips
/// to the next song.
#[test]
fn pausing_is_not_an_ending() {
    let (link, mut engine, mut out) = setup();

    assert!(link.load(tone(20.0), true));
    for _ in 0..8 {
        engine.render(&mut out);
    }

    link.pause();
    for _ in 0..8 {
        engine.render(&mut out);
    }

    assert!(!link.take_ended(), "a pause was reported as an ending");
    assert_eq!(link.snapshot().status, vapor_app_lib::audio::Status::Paused);
}

/// A mix must be arrangeable without allocating on the audio thread either.
///
/// This is the case the design bends around: beat grids are `Vec<f32>`, and if
/// one crossed to the audio thread it would have to be freed there. The
/// alignment is computed on the control side and only two scalars cross, which
/// is what this measures.
#[test]
fn arranging_a_mix_does_not_allocate_on_the_audio_thread() {
    use vapor_engine::mixer::BeatGrid;
    use vapor_engine::{Mixer, TransitionType};

    let (link, mut engine, mut out) = setup();

    assert!(link.load(tone(30.0), true));
    assert!(link.preload(tone(30.0)));
    for _ in 0..32 {
        engine.render(&mut out);
    }

    // Grids exist here, on the control side, and stay here.
    let grid = |bpm: f32| BeatGrid {
        bpm,
        beats: (0..2000).map(|i| i as f32 * 60.0 / bpm).collect(),
    };
    let (a, b) = (grid(128.0), grid(126.0));
    let ratio = Mixer::tempo_ratio(&a, &b).expect("ratio");
    let pos = Mixer::aligned_incoming_position(&a, &b, 2.0, 1.0).expect("position");

    assert!(link.schedule_transition(TransitionType::BassSwap, 4.0, pos, ratio, 1.0, 2.0));

    measure();
    // Long enough to cover the wait, the whole mix and the deck swap.
    for _ in 0..600 {
        engine.render(&mut out);
    }
    let (allocs, frees) = stop_measuring();

    assert_eq!(
        (allocs, frees),
        (0, 0),
        "arranging and running a mix cost {allocs} allocations and {frees} frees"
    );

    // Guard against measuring a mix that never happened.
    assert!(
        link.take_swapped(),
        "the transition never completed, so nothing was measured"
    );
    assert!(!link.transition_armed());
}

/// Cancelling must leave the track that is playing alone — it is what runs
/// whenever a person picks something else while a mix is arranged.
#[test]
fn cancelling_a_mix_leaves_playback_running() {
    use vapor_engine::mixer::BeatGrid;
    use vapor_engine::{Mixer, TransitionType};

    let (link, mut engine, mut out) = setup();

    assert!(link.load(tone(30.0), true));
    assert!(link.preload(tone(30.0)));
    for _ in 0..16 {
        engine.render(&mut out);
    }

    let grid = |bpm: f32| BeatGrid {
        bpm,
        beats: (0..2000).map(|i| i as f32 * 60.0 / bpm).collect(),
    };
    let (a, b) = (grid(128.0), grid(126.0));
    let ratio = Mixer::tempo_ratio(&a, &b).expect("ratio");
    let pos = Mixer::aligned_incoming_position(&a, &b, 10.0, 1.0).expect("position");
    link.schedule_transition(TransitionType::BassSwap, 4.0, pos, ratio, 1.0, 10.0);
    for _ in 0..4 {
        engine.render(&mut out);
    }
    assert!(link.transition_armed(), "the mix was never armed");

    link.cancel_transition();
    for _ in 0..8 {
        engine.render(&mut out);
    }

    assert!(!link.transition_armed(), "the mix survived a cancel");
    assert!(!link.take_swapped(), "a cancelled mix reported a swap");
    assert_eq!(
        link.snapshot().status,
        vapor_app_lib::audio::Status::Playing,
        "cancelling a mix stopped playback"
    );
}

/// Opening a real device, which nothing else here touches.
///
/// Every test above drives an [`Engine`] directly, so none of them would notice
/// if acquiring a device, building the stream or starting it were broken — and
/// that is the part that differs per machine and per platform (MIG-011).
///
/// Silent by construction: no track is loaded, so the mixer renders zeroes.
/// Absent hardware is not a failure — CI runners have no audio device, and the
/// app itself is required to keep working without one.
#[test]
fn a_real_device_opens_and_closes_cleanly() {
    use vapor_app_lib::audio::{Player, Status};

    let Ok(player) = Player::start() else {
        eprintln!("no audio output device on this machine; skipping");
        return;
    };

    assert!(player.sample_rate() > 0, "the device reported no rate");

    let snapshot = player.snapshot();
    assert_eq!(snapshot.status, Status::Idle);
    assert_eq!(snapshot.position, 0.0);

    // Long enough for the device to have called back several times. A stream
    // that failed to start, or a callback that panicked, shows up here.
    std::thread::sleep(std::time::Duration::from_millis(200));

    assert_eq!(
        player.snapshot().status,
        Status::Idle,
        "an idle player reported itself as playing"
    );

    // Dropping stops the thread and releases the device. A deadlock here would
    // hang the test rather than fail it, which is itself the signal.
    drop(player);
}

/// The counting allocator must actually work, or every test above passes
/// vacuously.
#[test]
fn the_allocation_counter_detects_allocation() {
    measure();
    let v: Vec<u8> = Vec::with_capacity(8192);
    let (allocs, _) = stop_measuring();
    drop(v);
    assert!(
        allocs > 0,
        "the counter failed to observe a known allocation"
    );
}

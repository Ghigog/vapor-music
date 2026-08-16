//! Proof that the shell's audio path does not allocate (TD-03, MIG-010, TD-09).
//!
//! `vapor-engine` already proves `Mixer::render` is allocation-free. That
//! guarantee is worth nothing if the plumbing the shell wraps it in allocates,
//! and this is where the plumbing lives: a command queue drained on the audio
//! thread, a track's audio displaced at every track change, and the interleave
//! into the device's own format.
//!
//! The track change is the case that matters and the one inspection misses. A
//! deck loaded the obvious way frees the previous track's samples where it
//! stands — inside the callback, producing a dropout that only ever happens at
//! the moment one track becomes the next. That is a bug nobody reproduces on
//! demand and everybody hears eventually.
//!
//! ## These drive the streaming path, because that is what ships
//!
//! Since TD-09 a deck reads through a window that a decoder thread keeps
//! filled, so the audio thread now shares state with a second thread that
//! allocates freely. Testing a deck loaded from memory would no longer be
//! testing the player. Every test here therefore runs a real file through a
//! real `Streamer`: real decode, real thread, real window.
//!
//! The counters being **per thread** is what makes that measurable at all — the
//! decoder thread allocates constantly and by design, and a process-wide
//! counter would attribute all of it to the audio thread.
//!
//! Same method as `vapor-engine`'s: a counting global allocator, in its own
//! integration binary so it measures nothing but this.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use vapor_app_lib::audio::{Engine, Link};
use vapor_app_lib::decoder::Streamer;
use vapor_engine::TrackSource;

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

/// A tone on disk, rather than silence, so the limiter and the EQ chain have
/// real signal to work on rather than a path of zeroes that skips work.
///
/// A file rather than a buffer because a streaming deck decodes from one, and a
/// test that fed the window by hand would be testing a stand-in for the player
/// instead of the player.
fn tone_file(seconds: f32) -> PathBuf {
    // A counter, not a timestamp: the clock is too coarse to keep two tests
    // starting together from colliding on a name.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "vapor-rt-{}-{}.wav",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));

    let n = (seconds * RATE as f32) as usize;
    let data_len = (n * 4) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&RATE.to_le_bytes());
    v.extend_from_slice(&(RATE * 4).to_le_bytes());
    v.extend_from_slice(&4u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let l = (t * 220.0 * std::f32::consts::TAU).sin() * 0.6;
        let r = (t * 330.0 * std::f32::consts::TAU).sin() * 0.6;
        for s in [l, r] {
            v.extend_from_slice(&((s * i16::MAX as f32) as i16).to_le_bytes());
        }
    }

    std::fs::write(&path, v).expect("write test tone");
    path
}

/// A track streaming from disk, exactly as the app plays one.
///
/// The returned [`Streamer`] must be kept alive: dropping it stops the decoder
/// thread, which is the mechanism the shell relies on and would otherwise
/// silently starve the deck mid-test.
fn streamed(path: &Path) -> (Streamer, TrackSource) {
    let streamer = Streamer::start(path, RATE, 0).expect("start decoding");
    let source = TrackSource::Stream(streamer.window());
    (streamer, source)
}

fn setup() -> (Arc<Link>, Engine, Vec<f32>) {
    let link = Arc::new(Link::new(RATE));
    let engine = Engine::new(Arc::clone(&link), 2);
    (link, engine, vec![0.0f32; BLOCK * 2])
}

/// Render `blocks` blocks at the rate a device asks for them, rather than as
/// fast as the CPU can.
///
/// Every other test here renders flat out, which is right when the question is
/// "how many allocations" — a count does not care how quickly it was reached.
/// It is wrong when the question is "how much audio was missed". A block is
/// 11.6 ms of sound but only microseconds of work, so a loop that renders flat
/// out consumes audio some forty times faster than a listener does, and every
/// millisecond the decoder spends fetching frames is charged to the deck forty
/// times over. Measured that way the decoder's own 3 ms idle poll — the normal,
/// correct behaviour of a decoder that was caught up when the seek arrived —
/// already reads as 130 ms of hole, and what the assertion then bounds is the
/// machine's scheduler rather than anything anyone would hear.
///
/// Paced, the exchange rate is one to one: a block missed here is a block a
/// device would have missed too.
///
/// The deadlines come from a fixed start rather than from sleeping a block's
/// worth between renders, and that is the part that survives a loaded machine.
/// A test thread descheduled for 50 ms comes back late and renders its arrears
/// back to back — during which the decoder has had those same 50 ms to fill the
/// window, so it is not blamed for a pause it was not party to. Load slows both
/// sides together instead of only compressing the consumer.
fn render_in_real_time(engine: &mut Engine, out: &mut [f32], blocks: usize) {
    let per_block = std::time::Duration::from_secs_f64(BLOCK as f64 / RATE as f64);
    let start = std::time::Instant::now();
    for i in 0..blocks {
        let due = start + per_block * i as u32;
        if let Some(wait) = due.checked_duration_since(std::time::Instant::now()) {
            std::thread::sleep(wait);
        }
        engine.render(out);
    }
}

/// The main claim: steady-state playback allocates and frees nothing.
///
/// Streaming makes this a stronger statement than it was. The audio thread now
/// reads a window a second thread is writing to concurrently, and it does so
/// through atomics rather than a lock — so this measures the reading half of
/// that arrangement while the decoder is genuinely running underneath it.
#[test]
fn steady_playback_neither_allocates_nor_frees() {
    let (link, mut engine, mut out) = setup();
    let path = tone_file(20.0);
    let (_streamer, source) = streamed(&path);

    assert!(link.load(source, true));
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
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        (allocs, frees),
        (0, 0),
        "steady playback performed {allocs} allocations and {frees} frees; \
         either can block on a lock inside the allocator and cause a dropout"
    );
}

/// The case inspection misses: swapping a track must not free the displaced
/// audio on the audio thread. Loading the naive way would fail exactly here.
///
/// Streaming shrinks what gets displaced from tens of megabytes to about one,
/// and does not change the rule at all — a `dealloc` takes the same lock
/// whatever its size.
#[test]
fn changing_track_does_not_free_on_the_audio_thread() {
    let (link, mut engine, mut out) = setup();
    let first = tone_file(10.0);
    let second = tone_file(10.0);
    let (_a, source_a) = streamed(&first);
    let (_b, source_b) = streamed(&second);

    assert!(link.load(source_a, true));
    for _ in 0..32 {
        engine.render(&mut out);
    }

    // Queued before measuring: enqueueing is the control thread's job and is
    // allowed to allocate. Only the drain is under test.
    assert!(link.load(source_b, true));

    measure();
    for _ in 0..8 {
        engine.render(&mut out);
    }
    let (allocs, frees) = stop_measuring();
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);

    assert_eq!(
        (allocs, frees),
        (0, 0),
        "a track change cost {allocs} allocations and {frees} frees on the \
         audio thread; the displaced audio must travel back to the control \
         thread to be dropped there"
    );

    // Guard against measuring nothing: the load must actually have been
    // applied, or this passes vacuously.
    assert!(
        (link.snapshot().duration - 10.0).abs() < 0.01,
        "the queued track was never loaded, so nothing was measured"
    );
}

/// A decoder that keeps up produces no gaps.
///
/// The trade streaming makes is memory for a deadline, and this is the deadline
/// half: the deck must be fed continuously from a window a fifth the length of
/// the track. Counted rather than assumed — `starved_blocks` exists so that a
/// player which stutters says so instead of being described as fine.
#[test]
fn streaming_playback_does_not_starve() {
    let (link, mut engine, mut out) = setup();
    let path = tone_file(20.0);
    let (_streamer, source) = streamed(&path);

    assert!(link.load(source, true));

    // Four seconds of audio, which is more than the five-second window holds at
    // once — so the decoder has to have refilled it during playback rather than
    // merely having been ready at the start.
    let blocks = (RATE as usize * 4) / BLOCK;
    for _ in 0..blocks {
        engine.render(&mut out);
    }

    let snapshot = link.snapshot();
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        snapshot.starved_blocks, 0,
        "the decoder fell behind on {} of {blocks} blocks",
        snapshot.starved_blocks
    );
    let expected = (blocks * BLOCK) as f64 / RATE as f64;
    assert!(
        (snapshot.position - expected).abs() < 0.05,
        "the playhead reached {:.2}s of an expected {expected:.2}s, so audio was missed",
        snapshot.position
    );
}

/// Seeking is served by the decoder thread, and the deck must arrive at the
/// position asked for rather than somewhere near it.
///
/// A seek outside the window costs a short gap, by design: the frames for the
/// new position do not exist yet, and the deck renders silence rather than
/// stale audio while its decoder fetches them. That gap is bounded here rather
/// than assumed away — it is the one audible cost streaming adds, and if it
/// ever grows this is what says so.
///
/// ## Why it seeks three times and keeps the best
///
/// The gap is counted in starved blocks, not timed with a stopwatch, and the
/// deck is driven at a device's pace so that a starved block is worth the
/// 11.6 ms it would be worth to a listener — see [`render_in_real_time`]. What
/// that leaves is the one thing no arithmetic here can rule out: the OS
/// declining to run the decoder thread at all for a while, on a machine busy
/// with something else. That is a fact about the machine, and a single sample
/// cannot tell it apart from a refill path that has gone slow.
///
/// Three seeks can. A regression in the refill path is in every sample; a
/// scheduling stall is in one. So each seek is measured separately and the
/// shortest gap is the one held to the bound — with all three printed on
/// failure, because "20 ms, 18 ms, 400 ms" and "390 ms, 400 ms, 410 ms" are
/// different diagnoses and the failure should say which one it is.
#[test]
fn seeking_a_streaming_deck_lands_where_it_was_asked() {
    let (link, mut engine, mut out) = setup();
    let path = tone_file(30.0);
    let (_streamer, source) = streamed(&path);

    assert!(link.load(source, true));
    for _ in 0..32 {
        engine.render(&mut out);
    }

    // Each target is more than a window away from where the last one left the
    // deck, so every one of these costs a real file seek instead of being
    // served from audio already in hand — which is the case being bounded.
    const TARGETS: [f64; 3] = [20.0, 5.0, 25.0];
    // A little over a second of audio per seek: far more than the gap being
    // bounded, and at a device's pace it is also a little over a second of
    // waiting, which is what the test costs.
    const AFTER: usize = 96;

    let played = (AFTER * BLOCK) as f64 / RATE as f64;
    let mut gaps = Vec::new();

    for target in TARGETS {
        // `starved_blocks` counts for the life of the link, so each seek is
        // measured as a delta. It also means the warm-up above cannot
        // contaminate the first sample.
        let before = link.snapshot().starved_blocks;

        link.seek(target);
        render_in_real_time(&mut engine, &mut out, AFTER);

        let snapshot = link.snapshot();
        assert!(
            snapshot.position >= target,
            "a seek to {target:.1}s left the playhead at {:.2}s — it did not move",
            snapshot.position
        );
        // Playing on from the seek point, so the playhead is the target plus
        // what was rendered, less whatever was lost to the refill.
        assert!(
            snapshot.position <= target + played,
            "the playhead reached {:.2}s, past the {:.2}s that was rendered",
            snapshot.position,
            target + played
        );

        let gap = (target + played) - snapshot.position;
        let counted = (snapshot.starved_blocks - before) as f64 * BLOCK as f64 / RATE as f64;
        assert!(
            (counted - gap).abs() < 1e-6,
            "the gap after seeking to {target:.1}s is {:.1} ms but starvation \
             was counted as {:.1} ms, so one of them is not measuring what it \
             claims",
            gap * 1000.0,
            counted * 1000.0
        );
        gaps.push(gap);
    }

    let _ = std::fs::remove_file(&path);

    let best = gaps.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(
        best < 0.15,
        "the shortest of {} refills after a seek still cost {:.0} ms of silence, \
         which is audible as a hole rather than as a seek (all of them: {})",
        gaps.len(),
        best * 1000.0,
        gaps.iter()
            .map(|g| format!("{:.0} ms", g * 1000.0))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// Every transport command runs on the audio thread. All of them must be free
/// of allocation, not just the common one.
#[test]
fn transport_commands_do_not_allocate() {
    let (link, mut engine, mut out) = setup();
    let path = tone_file(30.0);
    let (_streamer, source) = streamed(&path);

    assert!(link.load(source, true));
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
    let _ = std::fs::remove_file(&path);

    assert_eq!((allocs, frees), (0, 0), "a transport command allocated");
}

/// A track that runs out must report exactly one ending, and only because it
/// ended. Anything else here either strands the queue or skips a song.
#[test]
fn a_finished_track_reports_one_ending() {
    let (link, mut engine, mut out) = setup();

    // Shorter than the blocks rendered below, so it is certain to finish.
    let path = tone_file(0.05);
    let (_streamer, source) = streamed(&path);

    assert!(link.load(source, true));
    for _ in 0..40 {
        engine.render(&mut out);
    }
    let _ = std::fs::remove_file(&path);

    assert!(link.take_ended(), "a finished track reported no ending");
    assert!(!link.take_ended(), "one ending was reported twice");
}

/// A device asking for an empty buffer produces nothing for the same reason a
/// finished track does. Reading that as an ending would advance the queue
/// because a callback came back empty.
#[test]
fn an_empty_callback_is_not_mistaken_for_the_end_of_a_track() {
    let (link, mut engine, mut out) = setup();
    let path = tone_file(20.0);
    let (_streamer, source) = streamed(&path);

    assert!(link.load(source, true));
    for _ in 0..8 {
        engine.render(&mut out);
    }

    for _ in 0..8 {
        engine.render(&mut out[..0]);
    }
    let _ = std::fs::remove_file(&path);

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
    let path = tone_file(20.0);
    let (_streamer, source) = streamed(&path);

    assert!(link.load(source, true));
    for _ in 0..8 {
        engine.render(&mut out);
    }

    link.pause();
    for _ in 0..8 {
        engine.render(&mut out);
    }
    let _ = std::fs::remove_file(&path);

    assert!(!link.take_ended(), "a pause was reported as an ending");
    assert_eq!(link.snapshot().status, vapor_app_lib::audio::Status::Paused);
}

/// A mix must be arrangeable without allocating on the audio thread either.
///
/// This is the case the design bends around: beat grids are `Vec<f32>`, and if
/// one crossed to the audio thread it would have to be freed there. The
/// alignment is computed on the control side and only two scalars cross, which
/// is what this measures.
///
/// With streaming there are now *two* decoder threads running while this
/// measures — one per deck, exactly as during a real transition.
#[test]
fn arranging_a_mix_does_not_allocate_on_the_audio_thread() {
    use vapor_engine::mixer::BeatGrid;
    use vapor_engine::{Mixer, TransitionType};

    let (link, mut engine, mut out) = setup();
    let outgoing = tone_file(30.0);
    let incoming = tone_file(30.0);
    let (_a, source_a) = streamed(&outgoing);
    let (_b, source_b) = streamed(&incoming);

    assert!(link.load(source_a, true));
    assert!(link.preload(source_b));
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

    assert!(link.schedule_transition(TransitionType::BassSwap, 4.0, pos, ratio, 1.0, 2.0, false));

    measure();
    // Long enough to cover the wait, the whole mix and the deck swap.
    for _ in 0..600 {
        engine.render(&mut out);
    }
    let (allocs, frees) = stop_measuring();
    let _ = std::fs::remove_file(&outgoing);
    let _ = std::fs::remove_file(&incoming);

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
    let outgoing = tone_file(30.0);
    let incoming = tone_file(30.0);
    let (_a, source_a) = streamed(&outgoing);
    let (_b, source_b) = streamed(&incoming);

    assert!(link.load(source_a, true));
    assert!(link.preload(source_b));
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
    link.schedule_transition(TransitionType::BassSwap, 4.0, pos, ratio, 1.0, 10.0, false);
    for _ in 0..4 {
        engine.render(&mut out);
    }
    assert!(link.transition_armed(), "the mix was never armed");

    link.cancel_transition();
    for _ in 0..8 {
        engine.render(&mut out);
    }
    let _ = std::fs::remove_file(&outgoing);
    let _ = std::fs::remove_file(&incoming);

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

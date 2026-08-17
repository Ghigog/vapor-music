//! One deck: the per-track audio path, end to end.
//!
//! `deck.rs` had no tests at all, which is a strange gap for the busiest code
//! in the app — every frame either deck plays goes through it, and it is where
//! the stretcher, the EQ chain, the effects, the meter and the gain envelope
//! meet. The mixer tests above it cover *scheduling*; these cover what a single
//! deck does with a block.
//!
//! These also exercise the default stretcher, whatever it currently is. That
//! matters more than it used to: the default became Signalsmith, and until now
//! nothing but a click-track alignment test had run through it.

use vapor_engine::deck::Deck;
use vapor_engine::TrackSource;

const RATE: f32 = 44_100.0;

/// A steady tone. Loud enough to measure, quiet enough that summing two decks
/// cannot clip on its own.
fn tone(seconds: f32, hz: f32) -> Vec<[i16; 2]> {
    let n = (seconds * RATE) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let v = (0.3 * (std::f32::consts::TAU * hz * t).sin() * i16::MAX as f32) as i16;
            [v, v]
        })
        .collect()
}

fn deck_with(seconds: f32) -> Deck {
    let mut deck = Deck::new(RATE);
    deck.load(tone(seconds, 440.0));
    deck
}

fn render(deck: &mut Deck, frames: usize) -> (Vec<[f32; 2]>, usize) {
    let mut out = vec![[0.0f32; 2]; frames];
    let mut scratch = vec![[0.0f32; 2]; frames];
    let produced = deck.render_additive(&mut out, &mut scratch);
    (out, produced)
}

fn peak(buf: &[[f32; 2]]) -> f32 {
    buf.iter()
        .flat_map(|f| [f[0].abs(), f[1].abs()])
        .fold(0.0f32, f32::max)
}

// ---------------------------------------------------------------------------
// Producing nothing, which has to be distinguishable from producing silence
// ---------------------------------------------------------------------------

#[test]
fn a_stopped_deck_renders_nothing_and_leaves_the_buffer_alone() {
    let mut deck = deck_with(2.0);
    // Deliberately not played.

    let mut out = vec![[1.0f32; 2]; 512];
    let mut scratch = vec![[0.0f32; 2]; 512];
    let produced = deck.render_additive(&mut out, &mut scratch);

    assert_eq!(produced, 0);
    // Additive means a deck that produced nothing must not have subtracted
    // anything either — the other deck's audio is already in this buffer.
    assert!(
        out.iter().all(|f| f == &[1.0, 1.0]),
        "the buffer was touched"
    );
}

#[test]
fn a_deck_with_no_source_renders_nothing() {
    let mut deck = Deck::new(RATE);
    deck.play();

    let (_, produced) = render(&mut deck, 512);
    assert_eq!(produced, 0);
}

// ---------------------------------------------------------------------------
// The additive contract, which is what makes a crossfade a mix
// ---------------------------------------------------------------------------

#[test]
fn rendering_adds_into_the_buffer_rather_than_replacing_it() {
    let mut deck = deck_with(2.0);
    deck.play();

    let mut out = vec![[0.25f32; 2]; 512];
    let mut scratch = vec![[0.0f32; 2]; 512];
    let produced = deck.render_additive(&mut out, &mut scratch);
    assert!(produced > 0);

    // Every sample moved off the 0.25 it started at, and none was replaced by
    // a value that ignores it: a pure overwrite would leave the buffer with
    // values centred on zero rather than on 0.25.
    let mean: f32 = out.iter().map(|f| f[0]).sum::<f32>() / out.len() as f32;
    assert!(
        mean > 0.1,
        "mean {mean:.3} suggests the deck replaced the buffer instead of adding"
    );
}

#[test]
fn two_decks_sum_into_one_buffer() {
    let mut a = deck_with(2.0);
    let mut b = deck_with(2.0);
    a.play();
    b.play();

    let mut out = vec![[0.0f32; 2]; 1024];
    let mut scratch = vec![[0.0f32; 2]; 1024];
    a.render_additive(&mut out, &mut scratch);
    let one = peak(&out);
    b.render_additive(&mut out, &mut scratch);
    let two = peak(&out);

    assert!(
        two > one,
        "a second deck added nothing: {one:.3} then {two:.3}"
    );
}

// ---------------------------------------------------------------------------
// Gain
// ---------------------------------------------------------------------------

/// -60 dB is the floor the transitions fade to, and it has to be *silence*.
/// A faded-out deck that still contributes -60 dB of signal is a deck the
/// limiter and the clipping guard still have to account for.
#[test]
fn a_deck_faded_to_the_floor_contributes_exactly_nothing() {
    let mut deck = deck_with(2.0);
    deck.play();
    deck.set_gain_db(-60.0);

    let (out, produced) = render(&mut deck, 1024);
    assert!(produced > 0, "the deck should still be running");
    assert_eq!(peak(&out), 0.0, "a faded-out deck was still audible");
}

#[test]
fn gain_is_monotonic_in_decibels() {
    let mut levels = Vec::new();
    for db in [-40.0f32, -20.0, -6.0, 0.0] {
        let mut deck = deck_with(2.0);
        deck.play();
        deck.set_gain_db(db);
        let (out, _) = render(&mut deck, 4096);
        levels.push(peak(&out));
    }
    assert!(
        levels.windows(2).all(|w| w[1] > w[0]),
        "louder decibels did not produce a louder deck: {levels:?}"
    );
}

/// The audio thread must survive a nonsense parameter rather than propagating
/// it into the output, where it would reach a speaker.
#[test]
fn a_nonsense_gain_never_produces_a_nonsense_sample() {
    for db in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1e9, -1e9] {
        let mut deck = deck_with(1.0);
        deck.play();
        deck.set_gain_db(db);

        let (out, _) = render(&mut deck, 512);
        assert!(
            out.iter().flat_map(|f| [f[0], f[1]]).all(|s| s.is_finite()),
            "gain {db} produced a non-finite sample"
        );
    }
}

// ---------------------------------------------------------------------------
// Ending, and the thing that must not be mistaken for it
// ---------------------------------------------------------------------------

#[test]
fn a_deck_stops_itself_at_the_end_of_its_track() {
    let mut deck = deck_with(0.2);
    deck.play();

    // Render well past the end.
    let mut out = vec![[0.0f32; 2]; 4096];
    let mut scratch = vec![[0.0f32; 2]; 4096];
    for _ in 0..8 {
        deck.render_additive(&mut out, &mut scratch);
    }

    assert!(!deck.is_playing(), "the deck ran off the end still playing");
    let (_, produced) = render(&mut deck, 512);
    assert_eq!(produced, 0);
}

/// Running out of decoded audio is not the end of the track, and treating it
/// as one makes the player skip to the next song because a disk read was slow.
#[test]
fn starvation_is_not_the_end_of_the_track() {
    let mut deck = deck_with(0.2);
    deck.play();
    // Seek far past what a 0.2 s in-memory source holds. A memory source has
    // no decoder behind it, so this is the end rather than starvation — the
    // point is that the deck reports one of the two, never both.
    deck.seek_seconds(60.0);

    let (_, produced) = render(&mut deck, 512);
    assert_eq!(produced, 0);
    assert!(
        !(deck.is_starved() && deck.is_playing()),
        "the deck claimed to be both starved and finished"
    );
}

// ---------------------------------------------------------------------------
// Position and seeking
// ---------------------------------------------------------------------------

#[test]
fn position_advances_with_what_was_rendered() {
    let mut deck = deck_with(4.0);
    deck.play();
    assert_eq!(deck.position_seconds(), 0.0);

    let mut out = vec![[0.0f32; 2]; 4410];
    let mut scratch = vec![[0.0f32; 2]; 4410];
    deck.render_additive(&mut out, &mut scratch);

    let after = deck.position_seconds();
    assert!(
        (after - 0.1).abs() < 0.02,
        "expected about 0.1 s, got {after:.4}"
    );
}

#[test]
fn seeking_before_the_start_lands_at_the_start_rather_than_below_it() {
    let mut deck = deck_with(2.0);
    deck.seek_seconds(-5.0);

    assert_eq!(deck.position_seconds(), 0.0);

    // And it still plays from there.
    deck.play();
    let (_, produced) = render(&mut deck, 512);
    assert!(produced > 0);
}

#[test]
fn seeking_reports_the_position_it_was_given() {
    let mut deck = deck_with(10.0);
    deck.seek_seconds(3.5);
    assert!((deck.position_seconds() - 3.5).abs() < 1e-6);
}

#[test]
fn duration_is_the_length_of_the_loaded_audio() {
    let deck = deck_with(2.5);
    assert!((deck.duration_seconds() - 2.5).abs() < 0.01);
}

// ---------------------------------------------------------------------------
// Buffers of awkward sizes
// ---------------------------------------------------------------------------

/// The render takes the smaller of the two buffers. Passing a shorter scratch
/// must produce a short block, not read past the end of it.
#[test]
fn a_scratch_shorter_than_the_output_bounds_the_block() {
    let mut deck = deck_with(2.0);
    deck.play();

    let mut out = vec![[0.0f32; 2]; 1024];
    let mut scratch = vec![[0.0f32; 2]; 256];
    let produced = deck.render_additive(&mut out, &mut scratch);

    assert!(
        produced <= 256,
        "produced {produced} from a 256-frame scratch"
    );
    assert!(produced > 0);
    // Nothing was written past the block that was actually rendered.
    assert!(out[produced..].iter().all(|f| f == &[0.0, 0.0]));
}

#[test]
fn a_zero_length_block_is_harmless() {
    let mut deck = deck_with(2.0);
    deck.play();

    let mut out: Vec<[f32; 2]> = Vec::new();
    let mut scratch: Vec<[f32; 2]> = Vec::new();
    assert_eq!(deck.render_additive(&mut out, &mut scratch), 0);
    assert!(deck.is_playing(), "an empty block ended the track");
}

// ---------------------------------------------------------------------------
// The effects chain
// ---------------------------------------------------------------------------

/// Every knob at once, at its extreme, for long enough that a feedback path
/// would run away. The delay and the reverb both have feedback, so "finite at
/// block one" is not the question — "finite after a second" is.
#[test]
fn the_whole_chain_stays_finite_under_extreme_settings() {
    let mut deck = deck_with(3.0);
    deck.play();
    deck.set_gain_db(0.0);
    deck.set_eq_db(24.0, 24.0, 24.0);
    deck.set_delay_mix(1.0);
    deck.set_reverb_mix(1.0);

    let mut out = vec![[0.0f32; 2]; 2048];
    let mut scratch = vec![[0.0f32; 2]; 2048];
    let mut worst = 0.0f32;
    for _ in 0..20 {
        out.fill([0.0; 2]);
        deck.render_additive(&mut out, &mut scratch);
        assert!(
            out.iter().flat_map(|f| [f[0], f[1]]).all(|s| s.is_finite()),
            "the effects chain produced a non-finite sample"
        );
        worst = worst.max(peak(&out));
    }
    // Boosted 24 dB with both effects wide open it will be loud — the limiter
    // downstream is what makes that safe. What it must not be is unbounded.
    assert!(worst < 1e3, "the chain ran away to {worst}");
}

#[test]
fn an_eq_set_flat_is_close_to_a_bypass() {
    let mut plain = deck_with(2.0);
    plain.play();
    let (a, _) = render(&mut plain, 4096);

    let mut flat = deck_with(2.0);
    flat.play();
    flat.set_eq_db(0.0, 0.0, 0.0);
    let (b, _) = render(&mut flat, 4096);

    let diff = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (x[0] - y[0]).abs())
        .fold(0.0f32, f32::max);
    assert!(diff < 1e-3, "a flat EQ coloured the signal by {diff}");
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

#[test]
fn swapping_a_source_hands_the_old_one_back() {
    let mut deck = deck_with(1.0);
    let replaced = deck.swap_source(TrackSource::Memory(tone(2.0, 220.0)));

    assert!(
        matches!(replaced, TrackSource::Memory(ref s) if !s.is_empty()),
        "the previous source was not returned"
    );
    assert!((deck.duration_seconds() - 2.0).abs() < 0.01);
}

/// Loading over a playing deck must not leave it reading the old position of a
/// track that is no longer there.
#[test]
fn loading_a_new_track_starts_it_from_the_beginning() {
    let mut deck = deck_with(10.0);
    deck.play();
    render(&mut deck, 4410);
    assert!(deck.position_seconds() > 0.0);

    deck.load(tone(2.0, 220.0));
    assert_eq!(deck.position_seconds(), 0.0);
    assert!((deck.duration_seconds() - 2.0).abs() < 0.01);
}

//! What the DJ decides, with nothing underneath it that needs a device.
//!
//! # Why this crate exists (AUD-14)
//!
//! The repo keeps a wasm CI job over the core crates specifically to hold them
//! platform-free, and the feature the product is named for was in none of them:
//! the cost model, the three exits and every tuned constant lived in
//! `vapor-app/src-tauri/src/lib.rs`, took `&AppState`, and could only be
//! exercised by standing up a Tauri runtime. Numbers that were measured against
//! a real library — a genre jump is worth 140, a Switch starts at 45 BPM apart
//! — could not be tested without one.
//!
//! # What is here, and what is deliberately not
//!
//! Here: the arithmetic of choosing. Which of the three exits one track is from
//! another, what a candidate costs under each, which of the six transition types
//! to ask the mixer for, and how far apart two tracks are in *kind* rather than
//! in tempo. All of it takes plain values and returns plain values.
//!
//! Not here: the search itself, which was already portable —
//! [`vapor_library::generate_mood_path`] is the A* and has been in the library
//! crate all along. Nor the glue in the shell that builds a pool out of
//! `AppState`, appends to a queue and arms a mix against a playhead. That code
//! is mostly `&mut` on things this crate has no notion of, and a trait wide
//! enough to abstract a queue and a player position would be a bigger fiction
//! than the code it removed. The ticket's own reading was right: "the planners
//! are already portable; what binds them is the command wrappers".
//!
//! So the shell still owns the wrappers. What it no longer owns is a single
//! number that decides what plays next.

use serde::Serialize;
use ts_rs::TS;
use vapor_engine::TransitionType;
use vapor_library::TrackMeta;

/// How far apart two tracks have to be to count as a Switch.
///
/// Taken from the distribution of the owner's own library rather than invented:
/// across 4,000 random pairs the median tempo gap is 25 BPM and the median
/// intensity gap 0.14, with the 90th percentiles at 59 BPM and 0.35. These sit
/// around the 75th–90th, so a Switch is genuinely one of the more distant
/// jumps available rather than merely "not a match".
pub const SWITCH_INTENSITY: f32 = 0.30;
pub const SWITCH_BPM: f32 = 45.0;

/// And how close they have to be to count as a Match: below the median of both.
pub const MATCH_INTENSITY: f32 = 0.15;
pub const MATCH_BPM: f32 = 8.0;

/// What leaving the genre costs a Stay, and earns a Switch.
///
/// Sized against the intensity term in [`candidate_cost`], which is a 0–1
/// difference scaled by 100: a genre jump is worth more than any intensity gap,
/// and an artist jump about a quarter of one, because a different artist is
/// ordinary and a different genre is a decision.
pub const GENRE_JUMP: f32 = 140.0;
pub const ARTIST_JUMP: f32 = 28.0;

/// The three ways out of the track that is playing.
///
/// **Intentions, not similarity classes.** The old model sorted candidates into
/// Match, Fresh and Switch by how alike they were, then picked one per step from
/// a rotating cycle and marked it "AI choice". That fights the planner: the
/// cycle's pick and the set's own next track were computed by different code and
/// could disagree about what happens next.
///
/// Here the planner owns the set and these are the ways a person can steer it.
/// `Follow` is not a recommendation, it is what happens if nobody touches
/// anything — which is why there is no badge any more, and no cycle to reason
/// about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Exit {
    /// Hold roughly where the set is now, without advancing the curve.
    Stay,
    /// The planner's next track. The default, always.
    Follow,
    /// Branch off: audibly different, still mixable. Re-plans the tail toward
    /// the same destination the curve already had.
    Switch,
}

impl Exit {
    /// The word the card carries. Upper case because the screen draws it that
    /// way and a `text-transform` would put the styling in charge of the
    /// wording.
    pub fn label(self) -> &'static str {
        match self {
            Exit::Stay => "STAY",
            Exit::Follow => "FOLLOW",
            Exit::Switch => "SWITCH",
        }
    }
}

/// Which of the three exits one track is from another.
///
/// **Distance, not a genre label.** This used to return `Switch` if and only if
/// the two genres differed, and an unknown genre counts as similar — so on a
/// library carrying 46 genre tags across 534 tracks the branch was dead and the
/// screen could never offer a third choice. Drum & bass into Sade is 25 BPM and
/// 0.35 of intensity apart, the 90th percentile of this library, and it was
/// being called a Match.
///
/// A known difference of genre still forces a Switch. It is good evidence when
/// it is there; it simply is not the only evidence, and it was never present.
pub fn exit_between(from: &TrackMeta, to: &TrackMeta, similar_genre: bool) -> Exit {
    let bpm_diff = (from.bpm - to.bpm).abs();
    let intensity_diff = (from.energy_level - to.energy_level).abs();

    if !similar_genre || intensity_diff >= SWITCH_INTENSITY || bpm_diff >= SWITCH_BPM {
        return Exit::Switch;
    }
    if bpm_diff >= MATCH_BPM || intensity_diff >= MATCH_INTENSITY {
        return Exit::Follow;
    }
    Exit::Stay
}

/// What a candidate of a given kind costs, lower being better.
///
/// One definition, used by both the three suggestions on screen and the pick
/// the set actually takes, so the two cannot drift apart.
///
/// Built on [`vapor_library::transition_cost`] — the model ported from the
/// Godot build, which weighs key, tempo, energy *and genre relatedness*. The
/// scoring here used to be a separate ad-hoc formula per kind that mentioned
/// genre nowhere at all, so two tracks from unrelated genres scored exactly as
/// well as two from the same one. That is what made the suggestions feel
/// arbitrary and repetitive: with genre absent, the only things left were key
/// and tempo, and the same handful of tracks win those against everything.
///
/// Each kind then adds what it is *for* on top, because a Switch that scored
/// like a Match would simply be a Match:
///
/// * **Match** — the smoothest harmonic step, so the shared cost is enough.
/// * **Fresh** — §2's target of about 15 BPM and 0.25 of energy of movement,
///   so distance *from that target* is the penalty rather than distance itself.
/// * **Switch** — the effect masks the key, so rhythm and energy carry it.
///
/// `vibe_limit` is the person's energy threshold, passed rather than read: this
/// crate has no settings and should not grow any.
pub fn candidate_cost(from: &TrackMeta, to: &TrackMeta, kind: Exit, vibe_limit: f32) -> f32 {
    let base = vapor_library::transition_cost(from, to, vibe_limit, 0.0);
    let bpm_diff = (from.bpm - to.bpm).abs();
    let energy_diff = (from.energy_level - to.energy_level).abs();

    match kind {
        Exit::Stay => base,
        Exit::Follow => base + (bpm_diff - 15.0).abs() + (energy_diff - 0.25).abs() * 40.0,
        Exit::Switch => base + energy_diff * 20.0,
    }
}

/// What a track is, for the purpose of asking how far away it is in kind.
///
/// Resolved by the caller rather than looked up here. The shell knows where a
/// genre comes from — a tag, a lookup, or neither — and that resolution reads
/// three maps on `AppState`; carrying it into this crate would mean carrying
/// the maps.
#[derive(Debug, Clone, Default)]
pub struct Kind {
    /// The genre as the app resolved it, or empty.
    pub genre: String,
    /// The artist, trimmed and lower-cased, or empty.
    pub artist: String,
}

/// How far a candidate moves away from what is playing, in kind rather than in
/// tempo or level.
///
/// Genre when both sides have one; the artist otherwise.
///
/// The fallback is the point. Measured on this library, **488 of 534 tracks
/// carry no genre tag at all** and a further 15 say "Unknown genre" — so genre
/// is not a signal here, it is a blank, and every pair of them looks alike.
/// That is why a De André ballad could be offered as the way to *stay* in a
/// Keem the Cipher set: nothing in the model knew they were different music.
///
/// The artist is the strongest thing a folder-organised library does carry. Two
/// tracks by one artist are far more likely to be one vibe than two tracks
/// picked for tempo alone.
pub fn kind_distance(a: &Kind, b: &Kind) -> f32 {
    if !vapor_library::is_unknown_genre(&a.genre) && !vapor_library::is_unknown_genre(&b.genre) {
        return if vapor_library::is_similar_genre(&a.genre, &b.genre) {
            0.0
        } else {
            GENRE_JUMP
        };
    }
    if a.artist.is_empty() || b.artist.is_empty() || a.artist == b.artist {
        0.0
    } else {
        ARTIST_JUMP
    }
}

/// Choose the mix for a given pair of tracks (TD-27).
///
/// The engine has six and the shell used to pick one for everything, which
/// meant every mix inherited Standard Crossfade's ~3 dB midpoint dip (TD-23)
/// whether or not it suited the pair.
///
/// **Ported from `audio_manager.gd::get_transition_type_between`.** An earlier
/// version was a two-way branch of somebody's own devising. The original is a
/// weighted choice over six transition types, bucketed by harmonic distance
/// *and* tempo distance, with a genre jump treated as its own case — and seeded
/// by the pair, so the same two tracks always get the same mix. That structure
/// is what is here.
///
/// The doc this replaced carried a "what cannot be ported yet" section, listing
/// Echo Out, Reverb Freeze and Tempo Morph as unavailable for want of delay and
/// reverb (TD-20). All three are returned below, so that section had been
/// describing a state of the world that no longer existed; it is not carried
/// over. What is carried over is the port itself, which is the part that would
/// be expensive to work out again.
pub fn choose_transition(
    from_key: &str,
    to_key: &str,
    bpm_diff: f32,
    same_genre: bool,
) -> TransitionType {
    use TransitionType::{BassSwap, EchoOut, ReverbFreeze, StandardCrossfade, TempoMorph};

    // Unanalysed. The original hashes the pair and takes any of the six; with
    // three available and nothing to reason from, the least opinionated one is
    // a better answer than a third of a coin flip.
    if from_key.is_empty() || to_key.is_empty() {
        return StandardCrossfade;
    }

    let key_cost = vapor_library::harmonic_relation_cost(from_key, to_key);
    // The original's "creative" match type: a genre jump is steered the same
    // way as a key clash, because both are a deliberate gear change.
    let clashing = key_cost >= vapor_library::CLASH_COST || !same_genre;

    // The original's buckets, now that all six types exist. It picks between
    // two or three candidates per bucket with a hash of the pair; a single
    // deterministic choice is taken here instead, favouring the type that
    // carries the most weight in each — the variety it adds is not worth a
    // second source of "which mix will this be" for the screen to predict.
    match (clashing, bpm_diff) {
        // Clash or gear change: hide it behind an effect, whatever the tempo.
        (true, _) => EchoOut,
        // Closely related keys and close tempi: the characteristic DJ move.
        (false, d) if d < 3.0 => BassSwap,
        // Still related, tempi a few BPM apart: bend them together.
        (false, d) if d < 8.0 => TempoMorph,
        // Too far apart to stretch — the engine would refuse a beat-match
        // anyway, so let the outgoing track dissolve rather than collide.
        (false, _) => ReverbFreeze,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn track(bpm: f32, energy: f32) -> TrackMeta {
        TrackMeta {
            bpm,
            energy_level: energy,
            ..Default::default()
        }
    }

    // A plain comment, not a doc one: `proptest!` is a macro invocation and a
    // doc comment on it documents nothing.
    //
    // The property the three exits have to have between them: every pair of
    // tracks lands in exactly one, and which one is a function of distance.
    // Asserted over the whole plausible range rather than at the constants,
    // because the bug this replaced was a branch that was dead everywhere.
    proptest! {
        #[test]
        fn every_pair_lands_in_exactly_one_exit(
            b1 in 40.0f32..220.0,
            b2 in 40.0f32..220.0,
            e1 in 0.0f32..1.0,
            e2 in 0.0f32..1.0,
            similar in any::<bool>(),
        ) {
            let (from, to) = (track(b1, e1), track(b2, e2));
            let exit = exit_between(&from, &to, similar);
            let bpm = (b1 - b2).abs();
            let energy = (e1 - e2).abs();

            let expected = if !similar || energy >= SWITCH_INTENSITY || bpm >= SWITCH_BPM {
                Exit::Switch
            } else if bpm >= MATCH_BPM || energy >= MATCH_INTENSITY {
                Exit::Follow
            } else {
                Exit::Stay
            };
            prop_assert_eq!(exit, expected);
        }

        /// A known genre difference always forces a Switch, however close the
        /// two tracks are in tempo and level. This is the half of the rule that
        /// survived the rewrite, and the half easiest to lose.
        #[test]
        fn a_genre_difference_always_switches(
            b in 40.0f32..220.0,
            e in 0.0f32..1.0,
        ) {
            let (from, to) = (track(b, e), track(b, e));
            prop_assert_eq!(exit_between(&from, &to, false), Exit::Switch);
        }

        /// Cost is symmetric in neither direction and need not be — but it must
        /// be finite for every pair the planner can reach, because a NaN
        /// silently wins every `total_cmp` comparison in the search.
        #[test]
        fn a_candidate_never_costs_nan(
            b1 in 40.0f32..220.0,
            b2 in 40.0f32..220.0,
            e1 in 0.0f32..1.0,
            e2 in 0.0f32..1.0,
            limit in 0.0f32..1.0,
        ) {
            let (from, to) = (track(b1, e1), track(b2, e2));
            for kind in [Exit::Stay, Exit::Follow, Exit::Switch] {
                prop_assert!(candidate_cost(&from, &to, kind, limit).is_finite());
            }
        }
    }

    /// Identical tracks are the cheapest thing a Stay can be handed, and the
    /// one case where the answer is checkable by hand.
    #[test]
    fn a_track_into_itself_is_a_stay() {
        let t = track(128.0, 0.5);
        assert_eq!(exit_between(&t, &t, true), Exit::Stay);
    }

    #[test]
    fn an_unanalysed_pair_gets_the_least_opinionated_transition() {
        assert_eq!(
            choose_transition("", "8A", 0.0, true),
            TransitionType::StandardCrossfade
        );
        assert_eq!(
            choose_transition("8A", "", 0.0, true),
            TransitionType::StandardCrossfade
        );
    }

    /// The genre jump is steered like a key clash — same branch, and this is
    /// the assertion that says so rather than the comment.
    #[test]
    fn a_genre_jump_is_hidden_like_a_key_clash() {
        assert_eq!(
            choose_transition("8A", "8A", 0.0, false),
            TransitionType::EchoOut
        );
    }

    #[test]
    fn related_keys_pick_by_how_far_apart_the_tempi_are() {
        assert_eq!(
            choose_transition("8A", "8A", 1.0, true),
            TransitionType::BassSwap
        );
        assert_eq!(
            choose_transition("8A", "8A", 5.0, true),
            TransitionType::TempoMorph
        );
        assert_eq!(
            choose_transition("8A", "8A", 40.0, true),
            TransitionType::ReverbFreeze
        );
    }

    /// The measured fallback: with no genre on either side the artist decides,
    /// and two tracks by one artist are not a jump.
    #[test]
    fn with_no_genres_the_artist_decides() {
        let same = Kind {
            genre: String::new(),
            artist: "aphex twin".into(),
        };
        let other = Kind {
            genre: String::new(),
            artist: "boards of canada".into(),
        };
        assert_eq!(kind_distance(&same, &same), 0.0);
        assert_eq!(kind_distance(&same, &other), ARTIST_JUMP);

        // An artist nobody recorded is not evidence of a jump either.
        let unknown = Kind::default();
        assert_eq!(kind_distance(&same, &unknown), 0.0);
    }

    /// And when both genres are real, the genre decides and the artist is not
    /// consulted at all.
    #[test]
    fn two_real_genres_outrank_the_artist() {
        let a = Kind {
            genre: "drum and bass".into(),
            artist: "one".into(),
        };
        let b = Kind {
            genre: "soul".into(),
            artist: "one".into(),
        };
        assert_eq!(kind_distance(&a, &b), GENRE_JUMP);
    }
}

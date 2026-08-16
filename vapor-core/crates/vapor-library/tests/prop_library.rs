//! Property tests for the pure library logic.
//!
//! Queue, playlist and Camelot arithmetic all have invariants that hold for
//! *every* input rather than for the handful a person writes down. Shuffling
//! is the clearest case: the example test checks one shuffle of one list, and
//! the property checks that no shuffle of any list can lose or duplicate a
//! track — which is what someone would actually notice.

use proptest::prelude::*;
use vapor_library::{camelot, PlaylistStore, Queue};

/// Track hrefs that are distinct, since duplicates make "did it lose one"
/// ambiguous.
fn tracks(max: usize) -> impl Strategy<Value = Vec<String>> {
    (1usize..max).prop_map(|n| (0..n).map(|i| format!("/track/{i}.m4a")).collect())
}

proptest! {
    /// Shuffling and unshuffling returns the original order.
    ///
    /// The permutation is generated in the shell because the core owns no
    /// randomness on purpose, so the core's job is only to hold the original
    /// order and restore it.
    #[test]
    fn unshuffling_restores_the_original_order(
        hrefs in tracks(60),
        seed in any::<u64>(),
    ) {
        let mut queue = Queue::new();
        queue.set_tracks(hrefs.clone(), None);

        // A deterministic permutation from the seed — the shell's job, done
        // here so the test is reproducible from its own inputs.
        let mut order: Vec<usize> = (0..hrefs.len()).collect();
        let mut state = seed | 1;
        for i in (1..order.len()).rev() {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            order.swap(i, (state >> 33) as usize % (i + 1));
        }

        queue.shuffle(&order);
        queue.unshuffle();

        prop_assert_eq!(queue.tracks(), hrefs.as_slice());
    }

    /// Shuffling never loses or invents a track, whatever the permutation.
    #[test]
    fn shuffling_preserves_the_set(hrefs in tracks(60), seed in any::<u64>()) {
        let mut queue = Queue::new();
        queue.set_tracks(hrefs.clone(), None);

        let mut order: Vec<usize> = (0..hrefs.len()).collect();
        let mut state = seed | 1;
        for i in (1..order.len()).rev() {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            order.swap(i, (state >> 33) as usize % (i + 1));
        }
        queue.shuffle(&order);

        let mut before = hrefs.clone();
        let mut after = queue.tracks().to_vec();
        before.sort();
        after.sort();
        prop_assert_eq!(before, after);
    }

    /// Advancing through a queue never yields a track that is not in it, and
    /// never panics however many times it is asked.
    #[test]
    fn advancing_a_queue_stays_inside_it(
        hrefs in tracks(30),
        steps in 1usize..200,
    ) {
        let mut queue = Queue::new();
        queue.set_tracks(hrefs.clone(), None);

        for _ in 0..steps {
            if let Some(next) = queue.next(None) {
                prop_assert!(
                    hrefs.iter().any(|h| h == next),
                    "the queue produced {} which was never in it", next
                );
            }
        }
    }

    /// Reordering a playlist is a permutation: the same tracks, rearranged.
    #[test]
    fn reordering_a_playlist_keeps_every_track(
        hrefs in tracks(40),
        from in 0usize..40,
        to in 0usize..40,
    ) {
        let mut store = PlaylistStore::new();
        store.create("p", "Test");
        for href in &hrefs {
            store.add_track("p", href.clone());
        }

        store.reorder_tracks("p", from, to);

        let playlist = store.get("p").expect("the playlist still exists");
        let mut before = hrefs.clone();
        let mut after = playlist.tracks.clone();
        before.sort();
        after.sort();
        prop_assert_eq!(before, after);
    }

    /// Removing every track one at a time empties a playlist and never panics,
    /// whatever order the removals come in.
    #[test]
    fn removing_tracks_cannot_panic(
        hrefs in tracks(30),
        indices in prop::collection::vec(0usize..40, 0..40),
    ) {
        let mut store = PlaylistStore::new();
        store.create("p", "Test");
        for href in &hrefs {
            store.add_track("p", href.clone());
        }

        for index in indices {
            store.remove_track("p", index);
        }

        // Whatever happened, the playlist is still coherent.
        let playlist = store.get("p").expect("the playlist still exists");
        prop_assert!(playlist.tracks.len() <= hrefs.len());
        for track in &playlist.tracks {
            prop_assert!(hrefs.contains(track));
        }
    }

    /// Wheel distance is symmetric.
    ///
    /// Used for ranking, so an asymmetry would make a route cheaper one way
    /// than the other — which reads as the DJ preferring a direction for no
    /// reason. (`harmonic_relation_cost` is deliberately *not* symmetric: an
    /// energy boost is cheaper than a drop, which is a real musical asymmetry.)
    #[test]
    fn key_distance_is_symmetric(
        a_num in 1u32..=12, a_letter in prop::bool::ANY,
        b_num in 1u32..=12, b_letter in prop::bool::ANY,
    ) {
        let a = format!("{a_num}{}", if a_letter { "A" } else { "B" });
        let b = format!("{b_num}{}", if b_letter { "A" } else { "B" });

        prop_assert_eq!(
            camelot::key_distance(&a, &b),
            camelot::key_distance(&b, &a),
            "{} to {} is not the same distance as the reverse", a, b
        );
    }

    /// A key is zero distance from itself and from nothing else.
    #[test]
    fn only_a_key_itself_is_zero_distance(
        a_num in 1u32..=12, a_letter in prop::bool::ANY,
        b_num in 1u32..=12, b_letter in prop::bool::ANY,
    ) {
        let a = format!("{a_num}{}", if a_letter { "A" } else { "B" });
        let b = format!("{b_num}{}", if b_letter { "A" } else { "B" });

        prop_assert_eq!(camelot::key_distance(&a, &b) == 0.0, a == b, "{} and {}", a, b);
    }

    /// Nonsense is priced as *unknown* rather than as a distance computed from
    /// a misreading — the same rule the UI follows when it renders a dash.
    #[test]
    fn a_malformed_key_is_priced_as_unknown(junk in "\\PC{0,8}") {
        // By characters, not bytes — the same mistake this test found in the
        // code it is testing.
        let mut chars = junk.chars();
        let mode = chars.next_back();
        let digits = chars.as_str();
        let looks_valid = matches!(mode, Some('A' | 'B' | 'a' | 'b'))
            && digits.parse::<u32>().is_ok_and(|n| (1..=12).contains(&n));

        if !looks_valid {
            // The exact figure is the module's business; what matters is that
            // an unreadable key is priced as *unknown* rather than as zero,
            // which would make it look like a perfect match.
            let cost = camelot::key_distance(&junk, "8A");
            prop_assert!(cost > 0.0, "{} was priced as a perfect match", junk);
            prop_assert_eq!(cost, camelot::key_distance("also junk", "8A"));
        }
    }
}

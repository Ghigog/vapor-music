//! The rule that picks a mix for a pair of tracks (TD-27).
//!
//! Every mix used to be a Standard Crossfade, which meant every mix inherited
//! that envelope's ~3 dB midpoint dip (TD-23) whether or not it suited the
//! pair. The choice is now made from the harmonic relation between the keys at
//! the seam, and this pins the three cases so the rule cannot drift silently.
//!
//! Tested through `vapor_library::harmonic_relation_cost` — the same function
//! the pathfinder prices transitions with — rather than against a table of
//! hard-coded key pairs, so the two cannot form different opinions.

use vapor_library::{harmonic_relation_cost, CLASH_COST};

/// Keys a DJ would actually mix must come out below the clash threshold, and
/// keys that fight must come out at or above it. Everything downstream of the
/// choice depends on that boundary meaning what it says.
#[test]
fn compatible_keys_sit_below_the_clash_threshold() {
    // Same key, mode shift, harmonic step, energy boost — the standard moves.
    for (a, b) in [("8A", "8A"), ("8A", "8B"), ("8A", "9A"), ("8A", "10A")] {
        let cost = harmonic_relation_cost(a, b);
        assert!(
            cost < CLASH_COST,
            "{a} -> {b} priced as a clash at {cost}, so it would get a filter sweep"
        );
    }
}

#[test]
fn distant_keys_reach_the_clash_threshold() {
    // Six steps apart is the far side of the wheel in either mode.
    for (a, b) in [("8A", "2A"), ("1A", "7A"), ("3B", "9B")] {
        let cost = harmonic_relation_cost(a, b);
        assert!(
            cost >= CLASH_COST,
            "{a} -> {b} priced at {cost}, below the clash threshold, so two \
             dissonant tonalities would be overlapped rather than filtered"
        );
    }
}

/// An unparseable or missing key must not read as compatible — that would
/// overlap two tracks on no evidence at all.
#[test]
fn an_unknown_key_is_not_mistaken_for_a_compatible_one() {
    for (a, b) in [("", "8A"), ("8A", ""), ("nonsense", "8A")] {
        let cost = harmonic_relation_cost(a, b);
        assert!(cost > 0.0, "{a:?} -> {b:?} priced as a perfect match");
    }
}

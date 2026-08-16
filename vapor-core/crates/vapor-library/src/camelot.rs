//! Camelot wheel keys and harmonic relations.
//!
//! Direct port of the key half of `scripts/services/dj_pathfinder.gd`. Every
//! constant and every branch matches the GDScript, and the tests below are the
//! assertions from `tests/unit/test_dj_pathfinder.gd`.

/// A Camelot key: a number 1–12 and a mode, A (minor) or B (major).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CamelotKey {
    pub number: u8,
    pub is_minor: bool,
}

impl CamelotKey {
    /// Parse `"8A"`, `"12b"`, etc. Case-insensitive, whitespace tolerated.
    ///
    /// Returns `None` for anything malformed or out of range, which callers
    /// treat as "unknown key" rather than substituting a guess — the same
    /// principle as the DSP stub no longer inventing 120 BPM.
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.len() < 2 {
            return None;
        }
        let (digits, mode) = t.split_at(t.len() - 1);
        let is_minor = match mode {
            "A" | "a" => true,
            "B" | "b" => false,
            _ => return None,
        };
        let number: u8 = digits.parse().ok()?;
        if !(1..=12).contains(&number) {
            return None;
        }
        Some(CamelotKey { number, is_minor })
    }

    /// Steps around the wheel, ignoring mode. 12 → 1 is one step.
    fn step_diff(self, other: Self) -> u8 {
        let d = self.number.abs_diff(other.number);
        d.min(12 - d)
    }

    /// True when `other` sits `n` steps clockwise of `self`.
    fn is_ahead_by(self, other: Self, n: u8) -> bool {
        (self.number + n - 1) % 12 + 1 == other.number
    }
}

/// Penalty applied when either key is missing or unparseable.
const UNKNOWN_KEY_COST: f32 = 3.0;
/// Cost of a pair with no usable harmonic relation.
///
/// Public because the shell compares against it to decide whether two tracks
/// can be *overlapped* at all, which is the same question priced here.
pub const CLASH_COST: f32 = 8.0;

/// Plain wheel distance, used for ranking rather than for mixing decisions.
///
/// Same mode costs two per step; crossing modes adds one.
pub fn key_distance(a: &str, b: &str) -> f32 {
    let (Some(ka), Some(kb)) = (CamelotKey::parse(a), CamelotKey::parse(b)) else {
        return UNKNOWN_KEY_COST;
    };
    if ka == kb {
        return 0.0;
    }
    let steps = ka.step_diff(kb) as f32;
    if ka.is_minor == kb.is_minor {
        steps * 2.0
    } else {
        steps * 2.0 + 1.0
    }
}

/// Cost of moving between two keys, weighted by how DJs actually hear the
/// relation rather than by raw wheel distance.
///
/// A mode shift (8A → 8B) is cheaper than a step round the wheel, and an energy
/// boost (+2) is cheaper than an energy drop (−2), which plain distance cannot
/// express since both are two steps.
pub fn harmonic_relation_cost(a: &str, b: &str) -> f32 {
    let (Some(ka), Some(kb)) = (CamelotKey::parse(a), CamelotKey::parse(b)) else {
        return UNKNOWN_KEY_COST;
    };
    if ka == kb {
        return 0.0;
    }

    let steps = ka.step_diff(kb);
    let same_mode = ka.is_minor == kb.is_minor;

    if same_mode {
        match steps {
            1 => 1.5, // harmonic step
            2 => {
                if ka.is_ahead_by(kb, 2) {
                    2.5 // energy boost
                } else {
                    3.0 // energy drop
                }
            }
            // Five steps apart covers both the power fifth (+7) and the
            // subdominant (+5); the Godot version prices them the same.
            5 => 3.0,
            _ => CLASH_COST,
        }
    } else {
        match steps {
            0 => 1.0, // mode shift
            1 => 2.0, // diagonal step
            _ => CLASH_COST,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `test_camelot_key_parsing`.
    #[test]
    fn parses_keys_and_rejects_malformed_ones() {
        let k = CamelotKey::parse("8A").expect("8A is valid");
        assert_eq!(k.number, 8);
        assert!(k.is_minor);

        let k = CamelotKey::parse("12b").expect("12b is valid, case-insensitive");
        assert_eq!(k.number, 12);
        assert!(!k.is_minor);

        assert!(CamelotKey::parse("13A").is_none(), "13 is off the wheel");
        assert!(CamelotKey::parse("").is_none());
        assert!(CamelotKey::parse("A").is_none());
        assert!(CamelotKey::parse("8C").is_none(), "C is not a mode");
        assert!(CamelotKey::parse("0A").is_none());
    }

    /// Ported from `test_camelot_distance`.
    #[test]
    fn key_distance_matches_the_godot_values() {
        assert_eq!(key_distance("8A", "8A"), 0.0);
        assert_eq!(key_distance("8A", "8B"), 1.0);
        assert_eq!(key_distance("8A", "9A"), 2.0);
        assert_eq!(key_distance("12A", "1A"), 2.0, "the wheel wraps");
        assert_eq!(key_distance("8A", "10A"), 4.0);
        assert_eq!(key_distance("8A", "2A"), 12.0, "six steps is the maximum");
        assert_eq!(key_distance("8A", "invalid"), 3.0);
    }

    /// Ported from `test_harmonic_relation_costs`.
    #[test]
    fn harmonic_costs_match_the_godot_values() {
        assert_eq!(harmonic_relation_cost("10A", "10A"), 0.0, "exact match");
        assert_eq!(harmonic_relation_cost("10A", "10B"), 1.0, "mode shift");
        assert_eq!(harmonic_relation_cost("10A", "9A"), 1.5, "harmonic step");
        assert_eq!(harmonic_relation_cost("10A", "9B"), 2.0, "diagonal step");
        assert_eq!(harmonic_relation_cost("10A", "12A"), 2.5, "energy boost +2");
        assert_eq!(harmonic_relation_cost("10A", "8A"), 3.0, "energy drop -2");
        assert_eq!(harmonic_relation_cost("10A", "5A"), 3.0, "power fifth +7");
        assert_eq!(harmonic_relation_cost("10A", "3A"), 3.0, "subdominant +5");
        assert_eq!(harmonic_relation_cost("10A", "2A"), 8.0, "incompatible");
    }

    /// Direction matters: +2 and −2 are the same wheel distance but must not
    /// cost the same, which is the whole reason this function exists alongside
    /// `key_distance`.
    #[test]
    fn energy_boost_is_cheaper_than_energy_drop() {
        assert!(harmonic_relation_cost("10A", "12A") < harmonic_relation_cost("10A", "8A"));
    }

    #[test]
    fn unknown_keys_take_the_default_penalty() {
        assert_eq!(harmonic_relation_cost("", "8A"), 3.0);
        assert_eq!(harmonic_relation_cost("8A", "nonsense"), 3.0);
    }

    /// The relation is symmetric in distance but not in direction; check the
    /// symmetric cases hold both ways round so ranking is stable.
    #[test]
    fn symmetric_relations_are_symmetric() {
        for (a, b) in [("10A", "10B"), ("10A", "9A"), ("10A", "2A")] {
            assert_eq!(
                harmonic_relation_cost(a, b),
                harmonic_relation_cost(b, a),
                "{a} <-> {b}"
            );
        }
    }
}

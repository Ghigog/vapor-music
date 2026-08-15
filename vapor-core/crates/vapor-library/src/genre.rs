//! Genre taxonomy and distance.
//!
//! Port of the taxonomy half of `dj_pathfinder.gd`. Genres form a tree
//! (Club Music → House → Tech House); distance is the number of edges between
//! two genres in that tree, found by breadth-first search.
//!
//! The tree is embedded rather than read from `assets/genre_taxonomy.json` at
//! runtime. The GDScript already carried an identical hardcoded fallback for
//! when the file failed to load, so there were effectively two copies; keeping
//! the compiled-in one means the library has no file dependency and works
//! unchanged in the browser.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

/// Cost returned when a genre is unknown, absent, or not in the tree.
pub const UNRELATED_COST: f32 = 5.0;

/// The taxonomy, matching `assets/genre_taxonomy.json`.
const TAXONOMY: &[(&str, &[&str])] = &[
    ("Club Music", &["House", "Techno"]),
    (
        "House",
        &[
            "Tech House",
            "Deep House",
            "Progressive House",
            "Minimal House",
        ],
    ),
    ("Techno", &["Acid Techno", "Minimal Techno"]),
    ("Bass Music", &["Drum & Bass"]),
    (
        "Drum & Bass",
        &["Liquid DNB", "Neurofunk", "Jungle", "Drum and Bass"],
    ),
    ("Rock", &["Alternative Rock", "Classic Rock", "Hard Rock"]),
];

/// Undirected adjacency, keyed by lowercased genre name.
fn graph() -> &'static HashMap<String, Vec<String>> {
    static GRAPH: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    GRAPH.get_or_init(|| {
        let mut g: HashMap<String, Vec<String>> = HashMap::new();
        let mut link = |a: &str, b: &str| {
            let (a, b) = (a.to_lowercase(), b.to_lowercase());
            g.entry(a.clone()).or_default();
            g.entry(b.clone()).or_default();
            if let Some(v) = g.get_mut(&a) {
                if !v.contains(&b) {
                    v.push(b.clone());
                }
            }
            if let Some(v) = g.get_mut(&b) {
                if !v.contains(&a) {
                    v.push(a);
                }
            }
        };
        for (parent, children) in TAXONOMY {
            for child in *children {
                link(parent, child);
            }
        }
        g
    })
}

/// Resolve a genre string to a node, allowing substring matches.
///
/// The substring fallback is inherited from the GDScript: real tag data is
/// messy ("Progressive House / Melodic") and an exact-match-only lookup sends
/// most of a real library to the unknown-genre penalty.
fn find_node(genre: &str) -> Option<&'static String> {
    let g = graph();
    let key = genre.trim().to_lowercase();
    if let Some((k, _)) = g.get_key_value(&key) {
        return Some(k);
    }
    g.keys()
        .find(|k| k.contains(&key) || key.contains(k.as_str()))
}

/// Edge distance between two genres in the taxonomy.
///
/// Returns [`UNRELATED_COST`] when either is unknown or when no path exists.
pub fn genre_distance(a: &str, b: &str) -> f32 {
    let (ca, cb) = (a.trim().to_lowercase(), b.trim().to_lowercase());

    if ca.is_empty() || cb.is_empty() || ca == "unknown" || cb == "unknown" {
        return UNRELATED_COST;
    }
    if ca == cb {
        return 0.0;
    }

    let (Some(start), Some(goal)) = (find_node(&ca), find_node(&cb)) else {
        return UNRELATED_COST;
    };
    if start == goal {
        return 0.0;
    }

    let g = graph();
    let mut queue = VecDeque::from([(start, 0u32)]);
    let mut seen = HashSet::from([start]);

    while let Some((node, dist)) = queue.pop_front() {
        if node == goal {
            return dist as f32;
        }
        for n in g.get(node).map(|v| v.as_slice()).unwrap_or(&[]) {
            if let Some((k, _)) = g.get_key_value(n) {
                if seen.insert(k) {
                    queue.push_back((k, dist + 1));
                }
            }
        }
    }
    UNRELATED_COST
}

/// Loose similarity check used to bucket "interesting" versus "creative"
/// candidates. Deliberately more permissive than [`genre_distance`].
pub fn is_similar_genre(a: &str, b: &str) -> bool {
    let (ca, cb) = (a.trim().to_lowercase(), b.trim().to_lowercase());
    if ca.is_empty() || cb.is_empty() || ca == "unknown" || cb == "unknown" {
        return false;
    }
    ca == cb || ca.contains(&cb) || cb.contains(&ca)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `test_genre_taxonomy_cost`.
    #[test]
    fn distances_match_the_godot_values() {
        assert_eq!(genre_distance("Tech House", "Techno"), 3.0);
        assert_eq!(genre_distance("House", "Liquid DNB"), 5.0);
    }

    #[test]
    fn identical_genres_cost_nothing() {
        assert_eq!(genre_distance("Techno", "Techno"), 0.0);
        assert_eq!(genre_distance("techno", "  TECHNO "), 0.0);
    }

    #[test]
    fn unknown_genres_take_the_penalty() {
        assert_eq!(genre_distance("", "Techno"), UNRELATED_COST);
        assert_eq!(genre_distance("Unknown", "Techno"), UNRELATED_COST);
        assert_eq!(genre_distance("Polka", "Techno"), UNRELATED_COST);
    }

    /// Siblings are closer than cousins, which is what makes the cost useful
    /// for ranking rather than merely non-zero.
    #[test]
    fn siblings_are_closer_than_distant_relatives() {
        let siblings = genre_distance("Tech House", "Deep House");
        let cousins = genre_distance("Tech House", "Acid Techno");
        assert!(
            siblings < cousins,
            "siblings {siblings} should beat cousins {cousins}"
        );
    }

    #[test]
    fn distance_is_symmetric() {
        for (a, b) in [("Tech House", "Techno"), ("House", "Jungle")] {
            assert_eq!(genre_distance(a, b), genre_distance(b, a), "{a} <-> {b}");
        }
    }

    /// Real tag data is messy; the substring fallback is what keeps a live
    /// library out of the unknown-genre penalty.
    #[test]
    fn messy_tags_still_resolve() {
        assert!(genre_distance("Tech House / Melodic", "Techno") < UNRELATED_COST);
    }

    #[test]
    fn similarity_is_looser_than_distance() {
        assert!(is_similar_genre("Tech House", "tech house"));
        assert!(is_similar_genre("Tech House", "House"));
        assert!(!is_similar_genre("Unknown", "House"));
        assert!(!is_similar_genre("", "House"));
    }
}

//! `vapor-library` — playlists, grouping and the harmonic pathfinder.
//!
//! The third crate of the Rust core, and the one that carries the app's actual
//! differentiator: `dj_pathfinder.gd` decides *which* track plays next and why,
//! while `vapor-engine` only decides how to get there.
//!
//! Ported from GDScript with the existing GUT tests as the specification. This
//! is a refactor: where behaviour and improvement conflict, behaviour wins, and
//! anything worth improving is recorded in `docs/MIGRATION.md` rather than
//! changed in passing.
//!
//! Like the other core crates it has no I/O and no platform code.

pub mod camelot;
pub mod genre;
pub mod pathfinder;
pub mod playlist;
pub mod track;

pub use camelot::{harmonic_relation_cost, key_distance, CamelotKey};
pub use genre::{genre_distance, is_similar_genre};
pub use pathfinder::{generate_mood_path, transition_duration, Curve};
pub use playlist::{CoverSource, Playlist, PlaylistStore};
pub use track::{transition_cost, TrackMeta, DEFAULT_ENERGY_THRESHOLD};

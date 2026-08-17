//! `vapor-library` — playlists, grouping and the harmonic pathfinder.
//!
//! The third crate of the Rust core, and the one that carries the app's actual
//! differentiator: `dj_pathfinder.gd` decides *which* track plays next and why,
//! while `vapor-engine` only decides how to get there.
//!
//! Ported from GDScript with the existing GUT tests as the specification. This
//! is a refactor: where behaviour and improvement conflict, behaviour wins, and
//! anything worth improving is recorded in `docs/FINDINGS.md` rather than
//! changed in passing.
//!
//! Like the other core crates it has no I/O and no platform code.

pub mod camelot;
pub mod genre;
pub mod group;
pub mod index;
pub mod naming;
pub mod pathfinder;
pub mod playlist;
pub mod queue;
pub mod settings;
pub mod sync;
pub mod track;
pub mod webdav;

pub use camelot::{harmonic_relation_cost, key_distance, CamelotKey, CLASH_COST};
pub use genre::{genre_distance, is_similar_genre, octave_correct, tempo_band};
pub use group::{DynamicGroup, Entity, EntityType, Folder, FolderStore, GroupStore};
pub use index::{
    filter, group_rows, matches_any_entity, matches_query, sort_rows, GroupBy, Row, SortKey, Source,
};
pub use naming::{clean_segment, parse_path, strip_track_number, TrackInfo};
pub use pathfinder::{generate_mood_path, transition_duration, Curve};
pub use playlist::{CoverSource, Playlist, PlaylistStore};
pub use queue::{Queue, Repeat};
pub use settings::{
    RemoteConfig, Settings, ThemeMode, MAX_CACHE_BYTES_DEFAULT, MAX_MANUAL_BPM, MIN_CACHE_BYTES,
    MIN_MANUAL_BPM,
};
pub use sync::{
    reconcile, Advert, Delta, DeviceKind, Manifest, PairOutcome, Pairing, Peer, PeerRegistry,
    PlaylistRecord, TrackRecord, Trust, TrustedDevice,
};
pub use track::{transition_cost, TrackMeta, DEFAULT_ENERGY_THRESHOLD};
pub use webdav::{is_audio_path, parse_propfind, AUDIO_EXTENSIONS};

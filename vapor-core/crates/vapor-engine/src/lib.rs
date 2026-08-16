//! `vapor-engine` — the two-deck DJ mixer, in portable Rust.
//!
//! This is the migration's highest-risk component. `vapor-dsp` answered
//! "can we own the analysis?"; this answers "can we own the mixer?"
//!
//! What it replaces from the Godot build:
//!
//! | Godot | Here |
//! |---|---|
//! | `AudioStreamPlayer` x2 on `DeckA`/`DeckB` buses | [`deck::Deck`] x2 |
//! | `AudioEffectEQ6` + LowPass + HighPass per bus | [`biquad::EqChain`] |
//! | `Tween` chains writing `set_bus_volume_db` at frame rate | [`transition::Transition`], a pure function of time |
//! | Rubber Band via GDExtension | [`stretch::Stretcher`] (WSOLA) |
//! | `_process` polling `get_playback_position()` | position derived from the audio clock |
//!
//! Like `vapor-dsp`, this crate has no platform code and no I/O — audio output
//! lives behind a separate binary so the whole engine stays `cargo test`-able
//! and wasm-compatible.

pub mod biquad;
pub mod clipping;
pub mod deck;
pub mod delay;
pub mod limiter;
pub mod mixer;
pub mod offline;
pub mod pll;
/// Signalsmith Stretch. Native only — it is C++ behind a `cc` build and does
/// not compile to wasm, where `stretch`'s WSOLA carries on.
#[cfg(not(target_arch = "wasm32"))]
pub mod signalsmith;
pub mod source;
pub mod stretch;
pub mod transition;

pub use mixer::{BeatGrid, MatchError, Mixer};
pub use source::{TrackSource, Window};
pub use transition::{Transition, TransitionType};

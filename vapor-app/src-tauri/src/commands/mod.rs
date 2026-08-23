//! The IPC surface, one module per domain.
//!
//! `lib.rs` held every command — 97 of them at 11,033 lines — behind a single
//! `Arc<Mutex<AppState>>`. It was the only door to the backend, so any two
//! sessions doing backend work were editing the same file — the structural
//! cause of two of the three commits in this history that carry work their
//! author did not do.
//!
//! It now holds none of them. All 100 `#[tauri::command]`s in the crate are in
//! the modules below; `lib.rs` keeps the state, the IPC types and the helper
//! bodies the commands call, at 9,170 lines.
//!
//! Splitting by domain makes the seam an append-only list: `generate_handler!`
//! gains one line per command, and two additions to it conflict trivially
//! rather than inside a function body.
//!
//! The move is mechanical. Bodies are unchanged; what is new in each module is
//! the import block and the visibility. `pub` rather than `pub(crate)` on the
//! commands is Tauri's requirement, not a widening for its own sake — the
//! `#[command]` macro generates a hidden `__cmd__name` that `generate_handler!`
//! resolves through the module path, and it is not re-exported for a
//! crate-visible function.
//!
//! Shared state stays in `lib.rs` as `crate::AppState`, reachable because a
//! private item is visible to its own module and every descendant.

pub(crate) mod analysis;
pub(crate) mod artwork;
pub(crate) mod cache;
pub(crate) mod data;
pub(crate) mod dj;
pub(crate) mod downloads;
pub(crate) mod folders;
pub(crate) mod groups;
pub(crate) mod library;
pub(crate) mod lookup;
pub(crate) mod playback;
pub(crate) mod playlists;
pub(crate) mod queue;
pub(crate) mod settings;
pub(crate) mod sync;

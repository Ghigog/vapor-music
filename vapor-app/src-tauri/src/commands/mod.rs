//! The IPC surface, one module per domain.
//!
//! `lib.rs` held all 101 commands and 11,000 lines behind a single
//! `Arc<Mutex<AppState>>`. It was the only door to the backend, so any two
//! sessions doing backend work were editing the same file — the structural
//! cause of two of the three commits in this history that carry work their
//! author did not do.
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

pub(crate) mod cache;
pub(crate) mod groups;
pub(crate) mod playlists;
pub(crate) mod queue;

//! Building a blocking HTTP client from anywhere, including a runtime thread.
//!
//! ## The fault this exists to remove
//!
//! `reqwest::blocking::Client` owns a private tokio runtime — that is how a
//! blocking API is built on an async library. Constructing one spins that
//! runtime up and tears a temporary down, and tokio refuses to do the teardown
//! half on a thread that belongs to a runtime. So `Client::builder().build()`
//! **panics** anywhere inside an async context:
//!
//! ```text
//! thread 'tokio-rt-worker' panicked at tokio/src/runtime/blocking/shutdown.rs:
//! Cannot drop a runtime in a context where blocking is not allowed. This
//! happens when a runtime is dropped from within an asynchronous context.
//! ```
//!
//! Tauri runs commands on its async runtime's workers, and several commands
//! build one of these clients — `look_up_track` opening Liner Notes,
//! `sync_shared_document`, `find_album_art`. Each was a panic waiting on a
//! click.
//!
//! ## What is and is not a hazard
//!
//! Measured, not assumed (`tests/` probes, 2026-08-19):
//!
//! * **Building** inside an async context — panics.
//! * **Dropping** inside an async context — fine. The client's runtime lives on
//!   its own thread and winds down there, so the drop does not block.
//! * **Building and dropping inside `spawn_blocking`** — fine. Blocking pool
//!   threads are permitted to block; that is what they are for.
//!
//! An earlier attempt at this file guarded the *drop* and left the build alone,
//! which fixed nothing: the middle line above is why. Only the first line is a
//! bug.
//!
//! ## Why the fix is here rather than at each call site
//!
//! The alternative is a rule — "never build a client on a runtime thread" —
//! kept by every command, including ones added later by someone who has no
//! reason to know a runtime is four types down inside a dependency. One
//! constructor is a place the rule can actually live.
//!
//! Commands that do their network work on a blocking thread anyway should still
//! do so; this makes the wrong thread survivable, not correct. The join below
//! parks a worker for as long as it takes to build a client — milliseconds, and
//! against a command that is about to spend seconds on the network from that
//! same thread.

/// Run `build` somewhere a blocking client can be constructed.
///
/// Off a runtime — the analysis pass, the prefetcher, the decoder threads, and
/// anything inside `spawn_blocking` — this is a direct call and costs nothing,
/// which is the common case. On a runtime worker it hops to a plain thread and
/// waits for the answer.
///
/// `try_current` is the exact question: it succeeds if and only if the calling
/// thread is inside a runtime context, which is the condition tokio panics on.
pub fn build_blocking<T, F>(build: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_err() {
        return build();
    }

    std::thread::Builder::new()
        .name("vapor-http-build".to_string())
        .spawn(build)
        .map_err(|e| format!("could not start a thread to open a connection: {e}"))?
        .join()
        .map_err(|_| "the thread opening a connection stopped unexpectedly".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Result<reqwest::blocking::Client, String> {
        build_blocking(|| {
            reqwest::blocking::Client::builder()
                .build()
                .map_err(|e| e.to_string())
        })
    }

    /// The regression: a Tauri command runs on a runtime worker and builds a
    /// client. This is the click that used to take the process down.
    #[test]
    fn building_inside_an_async_context_does_not_panic() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("a runtime");

        rt.block_on(async {
            client().expect("a client");
        });
    }

    /// And off a runtime it is still an ordinary call.
    #[test]
    fn building_off_a_runtime_works() {
        client().expect("a client");
    }

    /// The bare builder is what panics, which is the whole reason for this
    /// module. Without this the test above passes whether or not
    /// `build_blocking` does anything at all.
    #[test]
    fn the_unwrapped_builder_is_the_thing_that_panics() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("a runtime");

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.block_on(async {
                let _bare = reqwest::blocking::Client::builder().build();
            });
        }))
        .is_err();

        assert!(
            panicked,
            "reqwest no longer panics when built in an async context — check \
             whether this module is still needed before removing it",
        );
    }

    /// The measurement the module docs rest on: dropping is *not* the hazard.
    /// If this ever starts failing, the docs above are wrong and the guard has
    /// to cover drops as well.
    #[test]
    fn dropping_inside_an_async_context_is_not_a_hazard() {
        let built = client().expect("a client");
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("a runtime");

        rt.block_on(async move {
            drop(built);
        });
    }
}

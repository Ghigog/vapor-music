//! Where the WebDAV password is kept, per platform.
//!
//! ## Why this is a module and not two lines in `webdav`
//!
//! `webdav` called `keyring::Entry` directly, and `keyring` is declared in
//! `Cargo.toml` only for macOS, iOS, Windows and Linux. That is not a missing
//! feature flag — it is a crate that does not exist on Android, so the shell
//! did not resolve for that target at all. Every other platform assumption in
//! the shell is at least visible in a `cfg`; this one was invisible until the
//! target was tried.
//!
//! ## The rule this module exists to enforce
//!
//! **A platform with no credential store must fail to compile.**
//!
//! `keyring`'s backends are opt-in features, and with none enabled version 3
//! falls back to a *mock*: `set_password` returns `Ok(())` and keeps the secret
//! inside that one `Entry` object. The app saves in one command and reads in
//! the next — a fresh `Entry` each time — so every read missed while every save
//! reported success. Nothing ever reached the keychain, the Settings screen
//! showed "unchanged", and no test caught it, because a store that can only
//! succeed is a store nothing can catch lying. That was TD-50.
//!
//! Adding a platform to this app must therefore be a decision about where
//! secrets go, taken deliberately, rather than something that compiles quietly
//! and is discovered by a person whose password stopped working. The
//! `compile_error!` at the bottom of this file is that decision point.
//!
//! ## What each platform uses
//!
//! | Platform | Store |
//! |---|---|
//! | macOS, iOS | Keychain Services (`keyring`, `apple-native`) |
//! | Windows | Credential Manager (`keyring`, `windows-native`) |
//! | Linux | Secret Service over D-Bus (`keyring`) |
//! | Android | AES-GCM under an Android Keystore key — see [`android`] |
//!
//! Android is the odd one out because it has no OS credential store of the kind
//! `keyring` wraps. See that module for what it does instead, and for what has
//! and has not been verified about it.

#[cfg(not(target_os = "android"))]
mod desktop;
#[cfg(not(target_os = "android"))]
pub use desktop::{delete, get, set};

// Compiled on Android because it is what Android uses, and compiled *anywhere*
// under `android-check` because JNI is transcribed method signatures and a
// typo in one is a runtime abort, not a compile error, on the only platform
// that runs it. The feature costs a `jni` dependency on the desktop dev build
// and buys type-checking of the whole file on a machine with no NDK — see the
// `android-check` step in `app.yml`.
#[cfg(any(target_os = "android", feature = "android-check"))]
// Nothing on a desktop *calls* it — that is the point — so under the check
// feature every item in it is dead code. Allowed only there: on Android proper
// this stays linted, and an unused constant in that file would mean a step of
// the Keystore dance is missing.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub mod android;
#[cfg(target_os = "android")]
pub use android::{delete, get, set};

/// Anything that went wrong reaching the store.
///
/// One string rather than a taxonomy: every caller does the same thing with it
/// — shows it to a person — and the platform stores disagree about what their
/// error cases even are. The one distinction that matters to a caller is
/// "absent" versus "broken", and that is carried by `get` returning
/// `Ok(None)`.
#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// A platform reaching this line has no credential store chosen for it.
///
/// Deliberately a hard stop. The alternative — falling through to something
/// that compiles — is how a password ends up in plain text or in a mock, and
/// both of those failures are silent at runtime.
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "windows",
    target_os = "linux",
    target_os = "android",
)))]
compile_error!(
    "no credential store is chosen for this target. Add one in src/secrets/ \
     and declare its dependency in Cargo.toml. Do not remove this check: a \
     keyring with no backend silently becomes a mock that reports success and \
     stores nothing (TD-50)."
);

//! The OS credential store, on the platforms that have one.
//!
//! A thin wrapper over `keyring`, which is already the right answer on macOS,
//! Windows and Linux. The only judgement here is that a missing entry is
//! `Ok(None)` rather than an error: "there is no password saved" is an ordinary
//! state of this app — it is the state every install starts in — and making the
//! caller distinguish it from "the keychain is broken" by matching on an error
//! variant is how it got conflated in the first place.

use super::Error;

fn entry(service: &str, username: &str) -> Result<keyring::Entry, Error> {
    keyring::Entry::new(service, username).map_err(|e| Error(e.to_string()))
}

pub fn set(service: &str, username: &str, password: &str) -> Result<(), Error> {
    entry(service, username)?
        .set_password(password)
        .map_err(|e| Error(e.to_string()))
}

/// The stored password, or `Ok(None)` when there is none.
pub fn get(service: &str, username: &str) -> Result<Option<String>, Error> {
    absent_is_none(entry(service, username)?.get_password())
}

/// Remove it. Already absent is success: the caller asked for it gone, and it
/// is gone.
pub fn delete(service: &str, username: &str) -> Result<(), Error> {
    absent_is_done(entry(service, username)?.delete_credential())
}

/// A read, under this module's contract: absent is `Ok(None)`, and everything
/// else that went wrong is an error.
///
/// A separate function only so the contract can be asserted. The arm that
/// matters is the one a working keychain will not produce to order — a store
/// saying "no such entry", and a store saying "I could not be opened" — and
/// the only way to put both in front of this match without a real keychain is
/// to hand it the `keyring::Error` directly.
fn absent_is_none(read: Result<String, keyring::Error>) -> Result<Option<String>, Error> {
    match read {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error(e.to_string())),
    }
}

/// A removal, under the same contract: already gone is `Ok(())`.
fn absent_is_done(removal: Result<(), keyring::Error>) -> Result<(), Error> {
    match removal {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store that could be reached but would not answer — a locked keychain,
    /// a Secret Service with no session bus. Distinct from `NoEntry` in every
    /// way that matters to a caller, and identical to it in shape.
    fn unreachable_store() -> keyring::Error {
        keyring::Error::NoStorageAccess(Box::new(std::io::Error::other(
            "the keychain could not be opened",
        )))
    }

    #[test]
    fn a_password_that_was_never_saved_is_absent_rather_than_an_error() {
        assert!(
            matches!(absent_is_none(Err(keyring::Error::NoEntry)), Ok(None)),
            "an install that has never saved a password reported a broken keychain"
        );
    }

    #[test]
    fn a_store_that_could_not_be_reached_is_an_error_rather_than_an_absent_password() {
        // The conflation this module exists to prevent. Reported as `Ok(None)`
        // the Settings screen says "no password saved" for a keychain that is
        // merely locked, and the person's answer to that is to type the
        // password again into a store that still cannot take it.
        assert!(
            absent_is_none(Err(unreachable_store())).is_err(),
            "a keychain that could not be opened was reported as no password saved"
        );
    }

    #[test]
    fn a_stored_password_comes_back_byte_for_byte() {
        // Leading and trailing spaces and a non-ASCII character, because an
        // app password is not a word: a `trim()` added anywhere on this path
        // would produce a credential that is wrong in a way nothing displays.
        let secret = "  pä ss word\u{a0}\n";
        assert_eq!(
            absent_is_none(Ok(secret.to_string()))
                .expect("a successful read is not an error")
                .as_deref(),
            Some(secret),
            "the password was altered between the store and the caller"
        );
    }

    #[test]
    fn the_stores_own_words_survive_into_the_error() {
        // One string rather than a taxonomy is only tolerable if the string is
        // the store's. An error rendered as "keyring error" tells the person
        // nothing they can act on.
        let message = absent_is_none(Err(unreachable_store()))
            .expect_err("an unreachable store is an error")
            .to_string();
        assert!(
            message.contains("the keychain could not be opened"),
            "the store's own explanation was dropped, leaving: {message}"
        );
    }

    #[test]
    fn deleting_a_password_that_is_not_there_is_success() {
        // Callers delete before saving, and every first save on an install
        // takes this path.
        assert!(
            absent_is_done(Err(keyring::Error::NoEntry)).is_ok(),
            "removing a password that was already gone was reported as a failure"
        );
    }

    #[test]
    fn a_removal_the_store_refused_is_an_error() {
        assert!(
            absent_is_done(Err(unreachable_store())).is_err(),
            "a refused removal was reported as done, so the credential is still there"
        );
    }

    /// Empty attributes are wildcards in the platform stores, which is why
    /// `keyring` rejects them rather than matching everything.
    ///
    /// The point of the assertion is *which* answer comes back: a rejected
    /// entry must be an error, not `Ok(None)`. `Ok(None)` reads as "no
    /// password saved for this account", and the account it is talking about
    /// does not exist.
    ///
    /// macOS only. Windows builds a target name out of the service and user
    /// and so accepts an empty user at construction, which would put this test
    /// in front of the real Credential Manager; Linux's Secret Service needs a
    /// session bus that CI does not have. `webdav`'s keychain tests are gated
    /// for the same reasons.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_empty_username_is_an_error_rather_than_no_password_saved() {
        assert!(
            get("vapor-music-test.invalid", "").is_err(),
            "an entry the store rejected was reported as an account with no password"
        );
        assert!(
            set("vapor-music-test.invalid", "", "secret").is_err(),
            "a password was saved against an entry the store rejects"
        );
    }
}

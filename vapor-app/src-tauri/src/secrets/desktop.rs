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
    match entry(service, username)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error(e.to_string())),
    }
}

/// Remove it. Already absent is success: the caller asked for it gone, and it
/// is gone.
pub fn delete(service: &str, username: &str) -> Result<(), Error> {
    match entry(service, username)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error(e.to_string())),
    }
}

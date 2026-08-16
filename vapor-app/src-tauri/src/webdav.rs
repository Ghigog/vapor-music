//! WebDAV transport.
//!
//! Only the transport. Parsing the response and deciding what counts as an
//! audio file live in `vapor_library::webdav`, which is tested without a
//! network — this module is the part that cannot be.
//!
//! The Godot build hand-wrote a TCP client here: its own chunked-transfer
//! decoder, header splitting and terminal-chunk detection, several hundred
//! lines that a real HTTP client does correctly. None of that survives.
//!
//! ## Credentials
//!
//! The password never enters `Settings` and is never serialised. It lives in
//! the OS keychain and is read at the point of use. The Godot build kept it in
//! a `ConfigFile` encrypted with a key derived in-process, which is obfuscation
//! rather than security — anyone with the binary has the key.

use base64::Engine as _;
use serde::Serialize;

/// Keychain service name. Stable, because changing it strands existing
/// credentials with no way for a person to find them.
const KEYCHAIN_SERVICE: &str = "com.dylangrowcoot.vapormusic.webdav";

/// How deep a PROPFIND descends.
///
/// `infinity` is what the app wants and what many servers refuse — it is a
/// trivial denial-of-service against the server, so most disable it. Depth 1
/// per directory with explicit recursion is the portable approach.
const DEPTH: &str = "1";

#[derive(Debug)]
pub enum DavError {
    NoCredentials,
    Auth,
    Http(String),
    Network(String),
}

impl std::fmt::Display for DavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // These strings reach a person, so they say what to do rather than
            // what went wrong internally.
            DavError::NoCredentials => {
                // Not "add it in Settings": every path that raises this is
                // *already* on the Settings screen, so that sends someone
                // looking for a place they are standing in. Say which field.
                write!(
                    f,
                    "No password saved for this account. Type it in the Password \
                     field above and press Save before scanning."
                )
            }
            DavError::Auth => write!(
                f,
                "The server rejected those credentials. Check the username and app password."
            ),
            DavError::Http(s) => write!(f, "The server returned {s}."),
            DavError::Network(s) => write!(f, "Could not reach the server: {s}"),
        }
    }
}

/// Store a password in the OS keychain, keyed by username.
pub fn save_password(username: &str, password: &str) -> Result<(), DavError> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, username)
        .map_err(|e| DavError::Network(e.to_string()))?;
    entry
        .set_password(password)
        .map_err(|e| DavError::Network(e.to_string()))
}

/// Whether a password is stored for `username`.
///
/// The Settings screen could not previously ask this, so its password box
/// always showed the placeholder "unchanged" — which claims a credential
/// exists whether or not one does. Someone who typed a password, had it not
/// save, and came back to a box reading "unchanged" had no way to tell the
/// difference between "already stored" and "never stored". That is the state
/// this exists to make visible.
///
/// Returns only whether it is there. The password itself never leaves this
/// module.
pub fn has_password(username: &str) -> bool {
    !username.is_empty() && load_password(username).is_ok()
}

fn load_password(username: &str) -> Result<String, DavError> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, username)
        .map_err(|e| DavError::Network(e.to_string()))?;
    entry.get_password().map_err(|_| DavError::NoCredentials)
}

/// Carry a stored password from one username to another.
///
/// Renaming the account used to *delete* the old keychain entry, on the
/// reasoning that a stale entry should not be left behind. Combined with the
/// UI's rule that an empty password box means "leave the password alone", that
/// made a rename destroy the credential: "unchanged" became "gone", and the
/// next scan reported no password saved for an account that had one a moment
/// earlier.
///
/// Moving it is what a rename means. If the password is wrong for the new
/// account the server says so, which is a recoverable answer; a silently empty
/// keychain is not.
pub fn move_password(from: &str, to: &str) -> Result<(), DavError> {
    if from == to {
        return Ok(());
    }
    // Nothing stored is not a failure — there was simply nothing to carry.
    if let Ok(password) = load_password(from) {
        save_password(to, &password)?;
    }
    delete_password(from)
}

/// Remove a stored password. Part of "delete my data" meaning what it says.
pub fn delete_password(username: &str) -> Result<(), DavError> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, username)
        .map_err(|e| DavError::Network(e.to_string()))?;
    // Already absent is success: the caller asked for it gone, and it is.
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(DavError::Network(e.to_string())),
    }
}

/// Fetch one file's bytes.
///
/// Blocking, because its only caller is the analysis pass, which already runs
/// on a blocking thread. Driving an async client from there would mean nesting
/// runtimes for no benefit.
pub fn fetch_blocking(
    remote: &vapor_library::RemoteConfig,
    href: &str,
) -> std::result::Result<Vec<u8>, String> {
    let password = load_password(&remote.username).map_err(|e| e.to_string())?;
    let auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", remote.username, password))
    );
    let origin = remote.url.trim_end_matches('/');

    let client = reqwest::blocking::Client::builder()
        .user_agent("VaporMusic/2.0")
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(format!("{origin}{href}"))
        .header("Authorization", auth)
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("server returned {}", response.status()));
    }
    response
        .bytes()
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub files: Vec<String>,
    /// Directories visited, so a slow scan can report progress honestly rather
    /// than showing an indeterminate spinner.
    pub directories: usize,
}

/// Recursively list audio files under `base`.
///
/// Depth-1 requests per directory rather than one `Depth: infinity` request:
/// servers commonly refuse infinity, and a per-directory walk also lets a
/// single unreadable folder be skipped instead of failing the whole scan.
pub async fn scan(url: &str, username: &str, base: &str) -> Result<ScanResult, DavError> {
    let password = load_password(username)?;
    let client = reqwest::Client::builder()
        .user_agent("VaporMusic/2.0")
        .build()
        .map_err(|e| DavError::Network(e.to_string()))?;

    let auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
    );

    let origin = url.trim_end_matches('/').to_string();
    let mut queue = vec![vapor_library::webdav::normalize_dir(base)];
    let mut files = Vec::new();
    let mut directories = 0usize;
    let mut seen = std::collections::HashSet::new();

    while let Some(dir) = queue.pop() {
        // A server that returns a parent as its own child would otherwise loop
        // forever.
        if !seen.insert(dir.clone()) {
            continue;
        }
        directories += 1;

        let body = match propfind(&client, &origin, &dir, &auth).await {
            Ok(b) => b,
            // One unreadable directory should not lose the whole library.
            Err(DavError::Auth) => return Err(DavError::Auth),
            Err(_) => continue,
        };

        for href in vapor_library::parse_propfind(&body) {
            files.push(href);
        }
        for child in child_directories(&body, &dir) {
            queue.push(child);
        }
    }

    files.sort();
    files.dedup();
    Ok(ScanResult { files, directories })
}

async fn propfind(
    client: &reqwest::Client,
    origin: &str,
    path: &str,
    auth: &str,
) -> Result<String, DavError> {
    let response = client
        .request(
            reqwest::Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid method"),
            format!("{origin}{path}"),
        )
        .header("Authorization", auth)
        .header("Depth", DEPTH)
        .header("Content-Type", "application/xml")
        // Asking for only the fields used keeps responses small; a bare
        // PROPFIND returns every property the server knows.
        .body(
            r#"<?xml version="1.0"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontentlength/></d:prop></d:propfind>"#,
        )
        .send()
        .await
        .map_err(|e| DavError::Network(e.to_string()))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(DavError::Auth);
    }
    if !response.status().is_success() {
        return Err(DavError::Http(response.status().to_string()));
    }

    response
        .text()
        .await
        .map_err(|e| DavError::Network(e.to_string()))
}

/// Child directory hrefs in a PROPFIND body.
///
/// A directory is an href ending in `/`. The request's own path appears in its
/// response and must be excluded, or the walk never terminates.
fn child_directories(xml: &str, current: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = xml.to_lowercase();
    let mut cursor = 0usize;

    while let Some(rel) = lower[cursor..]
        .find("<d:href>")
        .or_else(|| lower[cursor..].find("<href>"))
    {
        let open = cursor + rel;
        let tag = if lower[open..].starts_with("<d:href>") {
            "<d:href>".len()
        } else {
            "<href>".len()
        };
        let start = open + tag;
        let Some(close) = lower[start..].find("</") else {
            break;
        };
        let end = start + close;

        let raw = xml[start..end].trim();
        if raw.ends_with('/') {
            let path = strip_origin(raw);
            if path != current {
                out.push(path);
            }
        }
        cursor = end;
    }
    out
}

fn strip_origin(raw: &str) -> String {
    match raw.split_once("://") {
        Some((_, rest)) => match rest.find('/') {
            Some(i) => rest[i..].to_string(),
            None => "/".to_string(),
        },
        None => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/dav/Music/</d:href></d:response>
  <d:response><d:href>/dav/Music/Bjork/</d:href></d:response>
  <d:response><d:href>/dav/Music/Aphex/</d:href></d:response>
  <d:response><d:href>/dav/Music/track.mp3</d:href></d:response>
</d:multistatus>"#;

    /// The request's own path comes back in its response; including it would
    /// make the walk loop forever.
    #[test]
    fn the_current_directory_is_excluded_from_its_own_children() {
        let kids = child_directories(BODY, "/dav/Music/");
        assert_eq!(kids, vec!["/dav/Music/Bjork/", "/dav/Music/Aphex/"]);
    }

    #[test]
    fn files_are_not_mistaken_for_directories() {
        let kids = child_directories(BODY, "/dav/Music/");
        assert!(
            !kids.iter().any(|k| k.ends_with(".mp3")),
            "a file was queued as a directory: {kids:?}"
        );
    }

    #[test]
    fn absolute_child_urls_reduce_to_paths() {
        let xml = "<d:href>https://host.example/dav/Music/Sub/</d:href>";
        assert_eq!(
            child_directories(xml, "/dav/Music/"),
            vec!["/dav/Music/Sub/"]
        );
    }

    /// The messages are shown to a person, so they must say what to do.
    #[test]
    fn errors_are_actionable() {
        // "Actionable" means naming the field and the action, not naming the
        // screen. This used to assert the text contained "Settings", which it
        // did — while being displayed *on* the Settings screen, sending anyone
        // who read it looking for the place they were already standing in.
        let missing = DavError::NoCredentials.to_string();
        assert!(
            missing.contains("Password"),
            "does not name the field: {missing}"
        );
        assert!(
            missing.contains("Save"),
            "does not name the action: {missing}"
        );
        assert!(
            !missing.contains("in Settings"),
            "still sends the reader to the screen they are on: {missing}"
        );

        assert!(DavError::Auth.to_string().contains("app password"));
    }
}

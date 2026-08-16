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
    /// The configured folder is not on the server. Carries the path that was
    /// asked for, because the answer is almost always visible in it.
    NoSuchFolder(String),
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
            // Names the field and shows the path that was tried, because the
            // mistake is usually legible in it — a Koofr library lives under
            // /dav/Koofr/Music, and /Music looks more reasonable than it is.
            DavError::NoSuchFolder(path) => write!(
                f,
                "The server has no folder at {path}. Check the Folder field — \
                 it is the full path on the server, not the name of the folder."
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
    /// Subdirectories that could not be read and were skipped.
    ///
    /// Reported rather than swallowed: a scan that quietly walked past half a
    /// library still says "found 40 tracks", and there is no way to tell that
    /// from a library with 40 tracks in it.
    pub unreadable: usize,
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
    walk(base, |dir| {
        let (client, origin, auth) = (&client, &origin, &auth);
        async move { propfind(client, origin, &dir, auth).await }
    })
    .await
}

/// The walk itself, with the transport supplied by the caller.
///
/// Separated from `scan` so both of its failure modes can be tested from
/// captured responses instead of against a live server — which is what
/// `docs/TESTING.md` means by a body tests cannot reach. `scan` is then the
/// thin part: credentials, a client, and an origin.
async fn walk<F, Fut>(base: &str, fetch: F) -> Result<ScanResult, DavError>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<String, DavError>>,
{
    let root = vapor_library::webdav::normalize_dir(base);
    let mut queue = vec![root.clone()];
    let mut files = Vec::new();
    let mut directories = 0usize;
    let mut unreadable = 0usize;
    let mut seen = std::collections::HashSet::new();

    while let Some(dir) = queue.pop() {
        // A server that returns a parent as its own child would otherwise loop
        // forever.
        if !seen.insert(dir.clone()) {
            continue;
        }
        directories += 1;

        let body = match fetch(dir.clone()).await {
            Ok(b) => b,
            // The base directory is not "one unreadable folder" — it is the
            // whole request. Skipping it drained the queue and returned an
            // empty success, so a mistyped folder was indistinguishable from
            // an empty library: both said "Found 0 tracks". The folder is the
            // field most likely to be wrong, so that was the common case.
            Err(DavError::Http(status)) if dir == root && status.starts_with("404") => {
                return Err(DavError::NoSuchFolder(root))
            }
            Err(e) if dir == root => return Err(e),
            // Below the root, one unreadable directory should not lose the
            // whole library — but it is counted, and the caller says so.
            Err(DavError::Auth) => return Err(DavError::Auth),
            Err(_) => {
                unreadable += 1;
                continue;
            }
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
    Ok(ScanResult {
        files,
        directories,
        unreadable,
    })
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

    /// A server, as a map of path to canned response.
    ///
    /// `walk` takes its transport as a closure precisely so this can stand in
    /// for one. Anything not in the map answers 404, which is what a real
    /// server does.
    fn server(pages: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pages
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a current-thread runtime")
            .block_on(f)
    }

    fn walk_against(
        base: &str,
        pages: std::collections::HashMap<String, String>,
    ) -> Result<ScanResult, DavError> {
        block_on(walk(base, |dir| {
            let pages = &pages;
            async move {
                pages
                    .get(&dir)
                    .cloned()
                    .ok_or_else(|| DavError::Http("404 Not Found".into()))
            }
        }))
    }

    const ROOT: &str = r#"<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/dav/Music/</d:href></d:response>
  <d:response><d:href>/dav/Music/Aphex/</d:href></d:response>
  <d:response><d:href>/dav/Music/Locked/</d:href></d:response>
</d:multistatus>"#;

    const APHEX: &str = r#"<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/dav/Music/Aphex/</d:href></d:response>
  <d:response><d:href>/dav/Music/Aphex/Xtal.mp3</d:href></d:response>
</d:multistatus>"#;

    /// TD-49, and the reason it was worth finding.
    ///
    /// A mistyped folder used to return `Ok` with no files, because the base
    /// path went through the same "skip a directory we cannot read" branch as
    /// every subdirectory. The screen then said "Found 0 tracks" — which is
    /// also what an empty library says, so there was nothing to tell a person
    /// their path was wrong.
    #[test]
    fn a_folder_that_does_not_exist_is_an_error_not_an_empty_library() {
        let result = walk_against("/dav/Music", server(&[]));

        match result {
            Err(DavError::NoSuchFolder(path)) => assert_eq!(path, "/dav/Music/"),
            Err(e) => panic!("wrong error: {e}"),
            Ok(r) => panic!(
                "a missing folder reported success with {} files",
                r.files.len()
            ),
        }
    }

    /// The message has to be the one that fixes it: name the field, and show
    /// the path that was tried.
    #[test]
    fn the_missing_folder_message_names_the_field_and_the_path() {
        let message = DavError::NoSuchFolder("/Music/".into()).to_string();
        assert!(message.contains("/Music/"), "no path: {message}");
        assert!(
            message.contains("Folder"),
            "does not name the field: {message}"
        );
    }

    /// The other half of the same branch: below the root, a folder that cannot
    /// be read must not lose the rest of the library.
    #[test]
    fn one_unreadable_subdirectory_is_skipped_and_counted() {
        let result = walk_against(
            "/dav/Music",
            server(&[("/dav/Music/", ROOT), ("/dav/Music/Aphex/", APHEX)]),
        )
        .expect("the root was readable, so the scan should succeed");

        // Locked/ answered 404 and was skipped; Aphex/ was still walked.
        assert_eq!(result.files, vec!["/dav/Music/Aphex/Xtal.mp3"]);
        assert_eq!(result.unreadable, 1, "the skipped folder was not counted");
    }

    /// Nothing skipped means nothing to report, so the screen stays quiet.
    #[test]
    fn a_clean_walk_reports_nothing_unreadable() {
        const FLAT: &str = r#"<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/dav/Music/</d:href></d:response>
  <d:response><d:href>/dav/Music/Xtal.mp3</d:href></d:response>
</d:multistatus>"#;

        let result =
            walk_against("/dav/Music", server(&[("/dav/Music/", FLAT)])).expect("readable");

        assert_eq!(result.files, vec!["/dav/Music/Xtal.mp3"]);
        assert_eq!(result.unreadable, 0);
        assert_eq!(result.directories, 1);
    }

    /// Auth still fails the whole scan wherever it happens: every directory is
    /// behind the same credential, so one rejection means all of them.
    #[test]
    fn a_rejected_credential_below_the_root_fails_the_scan() {
        let result = block_on(walk("/dav/Music", |dir| async move {
            if dir == "/dav/Music/" {
                Ok(ROOT.to_string())
            } else {
                Err(DavError::Auth)
            }
        }));
        assert!(
            matches!(result, Err(DavError::Auth)),
            "an auth failure below the root was swallowed as an unreadable folder"
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

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
use std::collections::HashMap;
use std::sync::OnceLock;

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
    // Invalidate, never populate — see `cache`. The next read must reach the
    // keychain, or "saved" would be confirmed by the thing doing the saving.
    forget_cached(username);
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

/// Passwords read from the keychain during this run.
///
/// ## Why the credential is held in memory
///
/// On macOS a keychain read is an authorisation decision, and every one of them
/// can put a system dialog in front of the person using the app. The number of
/// reads is not small: opening Settings asks whether a password exists, every
/// scan needs one, and analysis — which now starts by itself after a scan —
/// needs one per pass. Answering "enter your login password" repeatedly to use
/// an app that is already running is not a security posture, it is an
/// annoyance that teaches people to click through prompts.
///
/// So the credential is read once per username per run and kept. It was
/// already in this process's memory on every fetch; this changes how long, not
/// whether.
///
/// ## What must stay true
///
/// A write **invalidates** and never populates. That matters more than it
/// looks: if saving also seeded the cache, then `save` followed by
/// `has_password` would answer out of memory without the keychain being
/// involved at all — which is precisely the shape of TD-50, where the store
/// lied and every test believed it. The first read after any write goes to the
/// keychain for real.
fn cache() -> &'static std::sync::Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<std::sync::Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Drop any remembered password for `username`.
///
/// Called by every path that changes what is stored, so a stale secret cannot
/// outlive the entry it came from.
fn forget_cached(username: &str) {
    if let Ok(mut map) = cache().lock() {
        map.remove(username);
    }
}

fn load_password(username: &str) -> Result<String, DavError> {
    if let Ok(map) = cache().lock() {
        if let Some(password) = map.get(username) {
            return Ok(password.clone());
        }
    }

    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, username)
        .map_err(|e| DavError::Network(e.to_string()))?;
    let password = entry.get_password().map_err(|_| DavError::NoCredentials)?;

    if let Ok(mut map) = cache().lock() {
        map.insert(username.to_string(), password.clone());
    }
    Ok(password)
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
    forget_cached(username);
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, username)
        .map_err(|e| DavError::Network(e.to_string()))?;
    // Already absent is success: the caller asked for it gone, and it is.
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(DavError::Network(e.to_string())),
    }
}

/// A blocking WebDAV session, held for as long as there is work to do.
///
/// ## Why this is not one function per fetch
///
/// It used to be. Every fetch read the credential out of the keychain and built
/// a fresh HTTP client, which is two costs paid per track:
///
/// * **The keychain.** On macOS a read is an authorisation decision, not a map
///   lookup. Analysing a 97-track library meant 97 of them, and if the person
///   answered the system prompt with "Allow" rather than "Always Allow" they
///   were asked 97 times. That was survivable while analysis only ran when
///   someone pressed a button; it is not, now that a scan starts one.
/// * **The connection.** A new `Client` per file means a new TCP and TLS
///   handshake per file, to a host the last one just finished talking to.
///
/// Both are per-pass facts, not per-track ones, so they are held here and the
/// pass borrows them.
pub struct Fetcher {
    origin: String,
    auth: String,
    client: reqwest::blocking::Client,
}

impl Fetcher {
    /// Read the credential once and open one client.
    pub fn new(remote: &vapor_library::RemoteConfig) -> std::result::Result<Self, String> {
        let password = load_password(&remote.username).map_err(|e| e.to_string())?;
        Ok(Self {
            origin: remote.url.trim_end_matches('/').to_string(),
            auth: format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", remote.username, password))
            ),
            client: reqwest::blocking::Client::builder()
                .user_agent("VaporMusic/2.0")
                .build()
                .map_err(|e| e.to_string())?,
        })
    }

    /// Fetch one file's bytes.
    pub fn fetch(&self, href: &str) -> std::result::Result<Vec<u8>, String> {
        let response = self
            .client
            .get(format!("{}{href}", self.origin))
            .header("Authorization", self.auth.clone())
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

    /// Fetch one file, treating "not there" as an answer rather than an error.
    ///
    /// The shared document (SYNC-006) does not exist until some device writes
    /// one, and a first sync from a library that has never been synced is the
    /// normal case — not a failure to report.
    pub fn fetch_optional(&self, href: &str) -> std::result::Result<Option<Vec<u8>>, String> {
        let response = self
            .client
            .get(format!("{}{href}", self.origin))
            .header("Authorization", self.auth.clone())
            .send()
            .map_err(|e| e.to_string())?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(format!("server returned {}", response.status()));
        }
        response
            .bytes()
            .map(|b| Some(b.to_vec()))
            .map_err(|e| e.to_string())
    }

    /// Write one file, replacing whatever was there.
    ///
    /// A plain PUT. WebDAV has locking, and this deliberately does not use it:
    /// two devices writing the shared document at the same instant is a race
    /// that lock support on the server may or may not exist to solve, and the
    /// merge is additive precisely so that losing one write costs nothing
    /// permanent — the next sync from that device puts its contents back.
    pub fn put(&self, href: &str, bytes: Vec<u8>) -> std::result::Result<(), String> {
        let response = self
            .client
            .put(format!("{}{href}", self.origin))
            .header("Authorization", self.auth.clone())
            .body(bytes)
            .send()
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!(
                "the server refused to store it ({})",
                response.status()
            ));
        }
        Ok(())
    }
}

/// Where the shared document lives, under the library's own folder.
///
/// Beside the music rather than in a hidden corner: a person who opens their
/// storage should be able to see everything the app put there, which is the
/// same reason `Your data` lists the local files.
pub fn shared_document_href(base_folder: &str) -> String {
    let trimmed = base_folder.trim_end_matches('/');
    if trimmed.is_empty() {
        "/vapor_metadata.json".to_string()
    } else {
        format!("{trimmed}/vapor_metadata.json")
    }
}

/// Fetch a single file.
///
/// For the one-off callers — starting a track, pre-loading the next mix — where
/// there is exactly one file to get and nothing to amortise a session over. A
/// loop over many tracks should build a [`Fetcher`] once instead.
pub fn fetch_blocking(
    remote: &vapor_library::RemoteConfig,
    href: &str,
) -> std::result::Result<Vec<u8>, String> {
    Fetcher::new(remote)?.fetch(href)
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
/// The byte ranges of each `<response>` element in a multistatus body.
///
/// Prefix-agnostic: `d:`, `D:`, `lp1:` and no prefix at all are all in use, and
/// which one a server picks is its own business.
fn response_blocks(lower: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut cursor = 0usize;

    while let Some(rel) = lower[cursor..].find("response>") {
        let at = cursor + rel;
        let closing = lower[..at]
            .rfind('<')
            .is_some_and(|lt| lower[lt..at].starts_with("</"));
        if closing {
            let end = at + "response>".len();
            out.push((start, end));
            start = end;
        }
        cursor = at + "response>".len();
    }
    out
}

/// The first href in a range, in its original case.
fn first_href(xml: &str, lower: &str, from: usize, to: usize) -> Option<String> {
    let block = &lower[from..to];
    let rel = block.find("href>")?;
    let open_end = from + rel + "href>".len();
    let close = lower[open_end..to].find("</")?;
    Some(xml[open_end..open_end + close].trim().to_string())
}

/// Subdirectories of `current`, from its PROPFIND response.
///
/// ## Read the answer we asked for
///
/// The request sends `<d:prop><d:resourcetype/>…`, and `<d:collection/>` inside
/// a response's resourcetype is WebDAV's own statement that the thing is a
/// directory. This used to ignore that and infer directory-ness from a trailing
/// slash on the href instead.
///
/// Servers are not obliged to add one, and Koofr does not. So every
/// subdirectory looked like a file, nothing was ever queued, and a scan of a
/// library with albums in folders reported "Found 97 tracks in 1 folders" —
/// the 97 loose files in the root, and not one of the albums below it.
///
/// The fixtures could not have caught it: they were written by hand, carried no
/// resourcetype at all, and gave every directory a trailing slash — encoding
/// the same assumption the code made. The trailing slash remains as a fallback
/// for a server that sends no resourcetype, which is what those fixtures now
/// exercise.
fn child_directories(xml: &str, current: &str) -> Vec<String> {
    let lower = xml.to_lowercase();
    let mut out = Vec::new();
    let current_dir = vapor_library::webdav::normalize_dir(current);

    for (from, to) in response_blocks(&lower) {
        let block = &lower[from..to];

        let is_directory = if block.contains("resourcetype") {
            // Any prefix: "<d:collection/>", "<collection/>", "<lp1:collection/>".
            block.contains("<collection") || block.contains(":collection")
        } else {
            // No resourcetype in the response at all — fall back to the shape
            // of the href, which is all there is left to go on.
            first_href(xml, &lower, from, to)
                .map(|h| h.ends_with('/'))
                .unwrap_or(false)
        };
        if !is_directory {
            continue;
        }

        let Some(href) = first_href(xml, &lower, from, to) else {
            continue;
        };
        // Normalised so the child is a directory path regardless of whether the
        // server put a slash on it, and so the comparison below is like for
        // like — otherwise the current directory re-queues itself under a
        // second spelling and the walk revisits it.
        let path = vapor_library::webdav::normalize_dir(&strip_origin(&href));
        if path != current_dir {
            out.push(path);
        }
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

    /// Some servers answer with full URLs rather than paths, and the href is
    /// what the cache and playlists are keyed on, so both spellings have to
    /// collapse to the same one.
    ///
    /// The body is a real response element rather than a bare `<d:href>`:
    /// directories are now identified by the `<d:collection/>` the request
    /// asks for, which only exists inside one.
    #[test]
    fn absolute_child_urls_reduce_to_paths() {
        let xml = r#"<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>https://host.example/dav/Music/Sub/</d:href>
    <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
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

    /// The test that was missing, and the reason it was missing.
    ///
    /// `keyring`'s platform backends are opt-in features. With none enabled,
    /// version 3 falls back to a **mock** store that keeps the secret inside
    /// the one `Entry` object it was set on — `set_password` returns `Ok(())`
    /// and nothing reaches the keychain. The app saves in one command and
    /// reads in the next, constructing a fresh `Entry` each time, so every
    /// read returned `NoEntry` while every save reported success. The screen
    /// said "Saved. The password is in your keychain" and then, immediately
    /// below, "No password saved for this account".
    ///
    /// Every existing test passed against the mock, because none of them ever
    /// crossed an `Entry` boundary — which is exactly the boundary the app
    /// crosses on every single call. **Reading it back through a new `Entry`
    /// is the whole test**; `save_password(..).is_ok()` asserts nothing.
    ///
    /// Not run on Linux: the secret-service backend needs a session bus and a
    /// running keyring daemon, and the CI container has neither. macOS and
    /// Windows have a store available to any logged-in user.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn a_saved_password_is_readable_by_the_next_command() {
        // Distinctive and obviously disposable: this writes to the real
        // keychain, because a fake keychain is what caused this.
        let user = "vapor-music-test@example.invalid";
        let _ = delete_password(user);

        save_password(user, "an-app-password").expect("saving should succeed");

        // Through the public API, which builds its own Entry — the crossing
        // the mock could not survive.
        assert!(
            has_password(user),
            "a password that was just saved could not be read back: the store \
             is not persisting across Entry instances"
        );
        assert_eq!(
            load_password(user).expect("readable"),
            "an-app-password",
            "the wrong secret came back"
        );

        delete_password(user).expect("cleanup");
        assert!(!has_password(user), "delete did not remove it");
    }

    /// A pass reads the credential when it starts, not once per track.
    ///
    /// On macOS a keychain read is an authorisation decision, so a fetch that
    /// performed one per file asked the system 97 times for a 97-track
    /// library — tolerable while analysis only ran on a button press, and not
    /// once a scan starts one by itself.
    ///
    /// Constructing the session is where the read happens, which is what this
    /// pins: build one, delete the credential, and it still fetches with what
    /// it already holds.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn a_fetcher_holds_the_credential_it_read_at_the_start() {
        let user = "vapor-music-fetcher@example.invalid";
        let _ = delete_password(user);

        assert!(
            Fetcher::new(&remote_for(user)).is_err(),
            "a session was built with no credential to build it from"
        );

        save_password(user, "an-app-password").expect("saving should succeed");
        let fetcher = Fetcher::new(&remote_for(user)).expect("a credential exists");

        // Gone from the store, still held by the session.
        delete_password(user).expect("cleanup");
        assert!(
            !has_password(user),
            "the credential is still in the keychain, so the next assertion \
             would pass without proving anything"
        );
        assert!(
            fetcher.auth.starts_with("Basic "),
            "the session did not keep the credential it read"
        );
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn remote_for(username: &str) -> vapor_library::RemoteConfig {
        vapor_library::RemoteConfig {
            url: "https://example.invalid".to_string(),
            username: username.to_string(),
            folder: "/dav/Music".to_string(),
        }
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

    /// The risk a cache introduces, and the reason writes only invalidate.
    ///
    /// A remembered password must never outlive the entry it came from. Saving
    /// a new one and reading it back has to return the new one — if the cache
    /// were populated on write, or not cleared by it, this would hand back the
    /// old secret indefinitely and the server would reject it with no way to
    /// tell why.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn a_changed_password_is_not_served_from_before() {
        let user = "vapor-music-cache@example.invalid";
        let _ = delete_password(user);

        save_password(user, "first").expect("saving should succeed");
        assert_eq!(load_password(user).expect("readable"), "first");

        save_password(user, "second").expect("saving should succeed");
        assert_eq!(
            load_password(user).expect("readable"),
            "second",
            "the old password was served out of memory after being replaced"
        );

        delete_password(user).expect("cleanup");
        assert!(
            !has_password(user),
            "a deleted password is still being answered for from memory"
        );
    }

    /// A server that says what a thing is instead of hinting with a slash.
    ///
    /// This is the shape Koofr returns: `<d:collection/>` in the resourcetype,
    /// and **no trailing slash** on the collection's href. The hand-written
    /// fixtures above have it the other way round — trailing slashes and no
    /// resourcetype — so both of them agreed with the code and neither could
    /// catch this. A real scan reported "Found 97 tracks in 1 folders": the
    /// loose files in the root, and not one of the albums in folders below it.
    const TYPED: &str = r#"<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/Koofr/Music</d:href>
    <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/Koofr/Music/Boards of Canada</d:href>
    <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/Koofr/Music/track.mp3</d:href>
    <d:propstat><d:prop><d:resourcetype/><d:getcontentlength>123</d:getcontentlength></d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;

    #[test]
    fn a_collection_without_a_trailing_slash_is_still_a_directory() {
        let kids = child_directories(TYPED, "/dav/Koofr/Music/");

        assert_eq!(
            kids,
            vec!["/dav/Koofr/Music/Boards of Canada/"],
            "a subdirectory was missed because its href had no trailing slash"
        );
    }

    #[test]
    fn a_file_with_an_empty_resourcetype_is_not_a_directory() {
        let kids = child_directories(TYPED, "/dav/Koofr/Music/");
        assert!(
            !kids.iter().any(|k| k.contains("track.mp3")),
            "a file was queued as a directory: {kids:?}"
        );
    }

    /// The request's own path comes back in its own response, with or without
    /// the slash. Queuing it again would revisit the directory forever.
    #[test]
    fn the_current_directory_is_excluded_even_when_spelled_differently() {
        // Asked for without the trailing slash, answered without one.
        let kids = child_directories(TYPED, "/dav/Koofr/Music");
        assert!(
            !kids.iter().any(|k| k == "/dav/Koofr/Music/"),
            "the directory queued itself: {kids:?}"
        );
    }

    /// The whole walk, against the typed shape.
    #[test]
    fn a_walk_descends_into_collections_that_have_no_trailing_slash() {
        const ALBUM: &str = r#"<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/Koofr/Music/Boards of Canada</d:href>
    <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/Koofr/Music/Boards of Canada/Roygbiv.mp3</d:href>
    <d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;

        let result = block_on(walk("/dav/Koofr/Music", |dir| async move {
            if dir.contains("Boards of Canada") {
                Ok(ALBUM.to_string())
            } else {
                Ok(TYPED.to_string())
            }
        }))
        .expect("the walk should succeed");

        assert_eq!(result.directories, 2, "the subdirectory was never visited");
        assert!(
            result.files.iter().any(|f| f.ends_with("Roygbiv.mp3")),
            "the track inside the album folder was not found: {:?}",
            result.files
        );
    }
}

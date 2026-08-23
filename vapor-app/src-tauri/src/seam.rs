//! Tests that cross the IPC boundary.
//!
//! Everything else tests one side or the other. The Rust suites drive
//! `AppState` directly; the frontend suites drive React against
//! `src/test/ipc.ts`, a TypeScript reimplementation of this crate. Both were
//! green while `playlists` was unusable, because neither of them ever looked at
//! what a command actually returns.
//!
//! These do. `tauri::test::mock_builder` runs the real `invoke_handler` against
//! real state, so a command goes through the same serialisation the webview
//! receives, and the assertions are written against the JSON — the field names
//! the frontend indexes by, not the Rust field names.
//!
//! `ts-rs` now generates the frontend's types from these structs, so a *shape*
//! mismatch cannot survive `npm run types:check`. What that cannot catch is
//! behaviour: whether creating a playlist makes it appear in the next read,
//! whether a folder move sticks. That is what is here.
//!
//! Chosen by risk, not by count. This is the layer that is expensive to write
//! and slow to run, so it covers the round trips that would strand a user — the
//! reads every screen opens on, the writes a person expects the next read to
//! show, and the answers the frontend indexes by a specific key. It is not, and
//! is not trying to be, one test per command.
//!
//! Widened for AUD-8 on 2026-08-23, from eleven commands to thirty. The e2e
//! suite drives the real UI against `src/test/ipc.ts` and so never reaches Rust
//! at all; until it does, this file is the only thing that looks at the wire.

use super::*;

// The commands moved into `commands/` on 2026-08-22, so the handler list below
// names them by path. A glob import would also work — `generate_handler!`
// resolves the hidden macro `#[tauri::command]` generates, not the function —
// but the compiler does not count that as a use, so it warns on an import the
// build genuinely needs. Paths avoid arguing with it.

use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::utils::acl::ExecutionContext;
use tauri::webview::InvokeRequest;
use tauri::WebviewWindowBuilder;

/// The commands these tests drive. Named once, because the handler list and the
/// ACL both need them and a command allowed but not registered — or the other
/// way round — fails in a way that reads like a bug in the command.
const COMMANDS: &[&str] = &[
    "playlists",
    "create_playlist",
    "rename_playlist",
    "delete_playlist",
    "playlist_folders",
    "create_folder",
    "set_playlist_folder",
    "add_tracks_to_playlist",
    "playlist_rows",
    "dynamic_groups",
    "create_group",
    "add_to_group",
    "group_tracks",
    "settings",
    "set_hide_duplicates",
    "library_view",
    "library_entities",
    "duplicate_count",
    "track_details",
    "search",
    "home_shelves",
    "queue_view",
    "playback_state",
    "cache_status",
    "set_cache_max_bytes",
    "downloaded_tracks",
    "remove_download",
    "local_folders",
    "add_local_folder",
    "sync_view",
];

/// An app with the real handler, a webview to invoke through, and the state
/// directory it is all backed by.
///
/// The webview is built once and handed back: labels are unique per app, so
/// building one per call fails on the second with
/// `WebviewLabelAlreadyExists`.
///
/// A counter rather than a timestamp in the directory name, for the reason the
/// suite in `lib.rs` already documents: macOS resolves the clock coarsely
/// enough that two tests starting together collide.
type Seam = (
    tauri::App<tauri::test::MockRuntime>,
    tauri::WebviewWindow<tauri::test::MockRuntime>,
    std::path::PathBuf,
);

fn seam() -> Seam {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "vapor-seam-test-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("state dir");
    let shared: Shared = Arc::new(Mutex::new(AppState::load(Store::new(dir.clone()))));
    // The mock context carries an empty ACL, and Tauri v2 refuses any command a
    // capability does not name — "not allowed. Plugin not found". The real
    // capabilities come from `generate_context!`, which cannot be expanded a
    // second time in this crate, so the commands under test are allowed
    // explicitly instead.
    let mut context = mock_context(noop_assets());
    for command in COMMANDS {
        context
            .runtime_authority_mut()
            .__allow_command((*command).to_string(), ExecutionContext::Local);
    }

    let app = mock_builder()
        .manage(shared)
        .invoke_handler(tauri::generate_handler![
            crate::commands::playlists::playlists,
            crate::commands::playlists::create_playlist,
            crate::commands::playlists::rename_playlist,
            crate::commands::playlists::delete_playlist,
            crate::commands::playlists::playlist_folders,
            crate::commands::playlists::create_folder,
            crate::commands::playlists::set_playlist_folder,
            crate::commands::playlists::add_tracks_to_playlist,
            crate::commands::playlists::playlist_rows,
            crate::commands::groups::dynamic_groups,
            crate::commands::groups::create_group,
            crate::commands::groups::add_to_group,
            crate::commands::groups::group_tracks,
            crate::commands::settings::settings,
            crate::commands::settings::set_hide_duplicates,
            crate::commands::library::library_view,
            crate::commands::library::library_entities,
            crate::commands::library::duplicate_count,
            crate::commands::library::track_details,
            crate::commands::library::search,
            crate::commands::library::home_shelves,
            crate::commands::queue::queue_view,
            crate::commands::playback::playback_state,
            crate::commands::cache::cache_status,
            crate::commands::cache::set_cache_max_bytes,
            crate::commands::downloads::downloaded_tracks,
            crate::commands::downloads::remove_download,
            crate::commands::folders::local_folders,
            crate::commands::folders::add_local_folder,
            crate::commands::sync::sync_view,
        ])
        // `mock_context`, not `generate_context!`: the real one embeds the
        // Info.plist and can only be expanded once per crate, which `run()`
        // already does. Nothing here reads the config anyway.
        .build(context)
        .expect("build app");
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("webview");
    // The app is handed back with the webview rather than dropped here: it owns
    // the runtime, and the webview does not outlive it.
    (app, webview, dir)
}

/// The origin a real webview invokes from, which is not the same string on every
/// platform.
///
/// Tauri decides whether a request is `ExecutionContext::Local` — the context
/// the ACL above grants — by comparing the request URL against the protocol URL
/// for the platform. On Windows and Android wry cannot register a `tauri://`
/// scheme, so the webview is served from `http://tauri.localhost` instead; on
/// macOS, Linux and iOS it really is `tauri://localhost`.
///
/// Hard-coded to the Unix spelling, every seam test failed on Windows with
/// "not allowed on window \"main\" … allowed on: [windows: \"*\", URL: local]" —
/// the command was allowed, the request just did not count as local. Nobody saw
/// it for as long as it was there, because the test binary could not load on
/// Windows at all and these five never ran.
#[cfg(any(windows, target_os = "android"))]
const LOCAL_URL: &str = "http://tauri.localhost";
#[cfg(not(any(windows, target_os = "android")))]
const LOCAL_URL: &str = "tauri://localhost";

/// Invoke a command and hand back the JSON the webview would have received.
fn call(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let response = get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: LOCAL_URL.parse().expect("url"),
            body: tauri::ipc::InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .unwrap_or_else(|e| panic!("{cmd} failed: {e:?}"));
    response.deserialize::<serde_json::Value>().expect("json")
}

/// Invoke a command that is expected to fail, and hand back the error the
/// webview would have received.
///
/// `Error` is a newtype over `String`, so what arrives is a bare JSON string —
/// a sentence to put in front of a person. The frontend has no error codes to
/// branch on and does not want any; what it needs is for the failure to arrive
/// as a rejection rather than as a successful call returning something empty.
fn call_err(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    match get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: LOCAL_URL.parse().expect("url"),
            body: tauri::ipc::InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    ) {
        Ok(ok) => panic!(
            "{cmd} was expected to fail and returned {:?}",
            ok.deserialize::<serde_json::Value>()
        ),
        Err(e) => e,
    }
}

/// Reach into the state the commands share, to stand something up that no
/// command can.
///
/// `app.rows` is rebuilt by `scan_library`, which walks a WebDAV tree or a
/// folder of real audio — neither of which belongs in a test of the wire. Every
/// library read resolves against those rows, so without them the difference
/// between "the command answered correctly" and "the command answered `[]`" is
/// invisible, which is exactly the failure this file exists to catch.
fn with_state(app: &tauri::App<tauri::test::MockRuntime>, edit: impl FnOnce(&mut AppState)) {
    let shared = app.state::<Shared>();
    let mut state = shared.lock().expect("state");
    edit(&mut state);
}

/// One row of a seeded library.
///
/// Both sources are `File` on purpose: the entity tabs and the grouping headers
/// skip anything whose artist or album is `Unknown`, so a row built with the
/// defaults would vanish from half of what is asserted below and the test would
/// pass on an empty answer.
fn row(href: &str, title: &str, artist: &str, album: &str) -> Row {
    Row {
        href: href.to_string(),
        title: title.to_string(),
        artist: artist.to_string(),
        album: album.to_string(),
        artist_source: vapor_library::index::Source::File,
        album_source: vapor_library::index::Source::File,
        genre: "Ambient".to_string(),
        bpm: 0.0,
        key: String::new(),
        year: 2015,
        manual_pos: 0,
    }
}

/// Four tracks over three records and two artists, in an order that is neither
/// sorted nor grouped — so a sort or a grouping that did not happen is
/// distinguishable from one that did.
fn library() -> Vec<Row> {
    vec![
        row(
            "/frahm/all-melody/02.m4a",
            "Sunson",
            "Nils Frahm",
            "All Melody",
        ),
        row("/aphex/syro/01.m4a", "Minipops 67", "Aphex Twin", "Syro"),
        row(
            "/frahm/all-melody/01.m4a",
            "All Melody",
            "Nils Frahm",
            "All Melody",
        ),
        row("/frahm/spaces/01.m4a", "Says", "Nils Frahm", "Spaces"),
    ]
}

/// The keys `LibraryTable` reads off every row it draws.
fn assert_row_keys(row: &serde_json::Value) {
    for key in [
        "href",
        "title",
        "artist",
        "album",
        "artistSource",
        "albumSource",
        "genre",
        "bpm",
        "key",
        "year",
        "manualPos",
    ] {
        assert!(row.get(key).is_some(), "a row is missing {key}: {row}");
    }
    assert!(
        row.get("artist_source").is_none() && row.get("manual_pos").is_none(),
        "snake_case leaked onto the wire: {row}"
    );
}

/// The bug this whole file exists because of.
///
/// A playlist that has just been created has to come back from the next read,
/// under the key the sidebar filters on. It did not: the rail reads `folderId`
/// and the wire carried `folder_id`, so every playlist looked filed nowhere and
/// rendered nowhere at all.
#[test]
fn a_created_playlist_comes_back_under_the_keys_the_frontend_reads() {
    let (_app, app, _dir) = seam();

    let made = call(
        &app,
        "create_playlist",
        serde_json::json!({ "name": "Late Night" }),
    );
    assert_eq!(made["name"], "Late Night");
    assert!(
        made["id"].is_string(),
        "an id the frontend can address it by"
    );

    let all = call(&app, "playlists", serde_json::json!({}));
    let list = all.as_array().expect("an array");
    assert_eq!(list.len(), 1, "the playlist that was just made");

    let one = &list[0];
    // Exactly the keys `PlaylistRail` and `core.ts` index by. `folderId` is
    // load-bearing: the rail filters `p.folderId === ""` for the top level, and
    // `undefined === ""` is false, which is how a playlist becomes invisible.
    for key in ["id", "name", "customCoverPath", "tracks", "folderId"] {
        assert!(
            one.get(key).is_some(),
            "playlists[0] is missing {key}: {one}"
        );
    }
    assert_eq!(one["folderId"], "", "a new playlist is at the top level");
    assert!(one["tracks"].is_array());
}

#[test]
fn a_playlist_filed_into_a_folder_reports_that_folder() {
    let (_app, app, _dir) = seam();

    let playlist = call(
        &app,
        "create_playlist",
        serde_json::json!({ "name": "Sets" }),
    );
    let folder = call(
        &app,
        "create_folder",
        serde_json::json!({ "name": "Nights" }),
    );
    let folder_id = folder["id"].as_str().expect("folder id").to_string();

    call(
        &app,
        "set_playlist_folder",
        serde_json::json!({ "id": playlist["id"], "folderId": folder_id }),
    );

    let all = call(&app, "playlists", serde_json::json!({}));
    assert_eq!(
        all[0]["folderId"], folder_id,
        "the rail groups by this, so it has to survive the round trip"
    );

    let folders = call(&app, "playlist_folders", serde_json::json!({}));
    assert!(
        folders[0].get("parentId").is_some(),
        "camelCase, not parent_id"
    );
}

#[test]
fn renaming_and_deleting_a_playlist_are_visible_to_the_next_read() {
    let (_app, app, _dir) = seam();
    let made = call(
        &app,
        "create_playlist",
        serde_json::json!({ "name": "First" }),
    );
    let id = made["id"].clone();

    call(
        &app,
        "rename_playlist",
        serde_json::json!({ "id": id, "name": "Second" }),
    );
    let all = call(&app, "playlists", serde_json::json!({}));
    assert_eq!(all[0]["name"], "Second");

    call(&app, "delete_playlist", serde_json::json!({ "id": id }));
    let all = call(&app, "playlists", serde_json::json!({}));
    assert_eq!(all.as_array().expect("array").len(), 0);
}

/// `Entity` went over the wire as `type`, which `SmartGroup.tsx` does not read
/// — it keys its chips on `entityType`, so every chip rendered without a kind.
#[test]
fn a_group_entity_arrives_as_entity_type() {
    let (_app, app, _dir) = seam();

    let group = call(
        &app,
        "create_group",
        serde_json::json!({ "name": "Ambient" }),
    );
    call(
        &app,
        "add_to_group",
        serde_json::json!({
            "id": group["id"],
            "entityType": "artist",
            "value": "Nils Frahm",
        }),
    );

    let groups = call(&app, "dynamic_groups", serde_json::json!({}));
    let entity = &groups[0]["entities"][0];
    assert_eq!(entity["entityType"], "artist", "not `type`: {entity}");
    assert_eq!(entity["value"], "Nils Frahm");
}

/// Settings is the first call the app makes, and every screen reads it.
#[test]
fn settings_arrive_camel_cased_and_numeric() {
    let (_app, app, _dir) = seam();
    let s = call(&app, "settings", serde_json::json!({}));

    for key in [
        "cacheMaxBytes",
        "hideDuplicates",
        "metadataLookupEnabled",
        "djMode",
    ] {
        assert!(s.get(key).is_some(), "settings is missing {key}");
    }
    // Not a string, and not a bigint: `ts-rs` maps u64 to bigint by default and
    // the frontend does arithmetic on this.
    assert!(
        s["cacheMaxBytes"].is_number(),
        "cacheMaxBytes must be a JSON number: {}",
        s["cacheMaxBytes"]
    );
}

// ---------------------------------------------------------------------------
// The library table
// ---------------------------------------------------------------------------

/// `LibraryView` is the one shape on this wire that `ts-rs` does not generate.
///
/// Everything else the frontend reads is a `#[ts(export)]` struct, so
/// `npm run types:check` fails on drift. This one goes the other way — the
/// webview *sends* it — and its TypeScript twin is hand-written in `core.ts`.
/// Nothing checks the two against each other.
///
/// So the assertion is not that the call succeeded. It is that the filter
/// actually took effect: `#[serde(default)]` sits on every field, which means a
/// key the shell does not recognise is silently dropped and the answer comes
/// back unsorted, ungrouped and unfiltered — a successful call that ignored
/// what was asked. `sortKey` arriving as `sort_key` would look exactly like a
/// library that happened to be in that order already.
#[test]
fn the_library_view_reads_the_camel_cased_keys_the_frontend_sends() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| state.rows = library());

    let descending = call(
        &webview,
        "library_view",
        serde_json::json!({ "view": { "sortKey": "title", "ascending": false } }),
    );
    let sections = descending.as_array().expect("sections");
    assert_eq!(sections.len(), 1, "ungrouped is one section: {descending}");
    assert_eq!(sections[0]["header"], "", "and its header is empty");

    let titles: Vec<&str> = sections[0]["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| r["title"].as_str().expect("title"))
        .collect();
    assert_eq!(
        titles,
        ["Sunson", "Says", "Minipops 67", "All Melody"],
        "both `sortKey` and `ascending` have to land, or this is seed order"
    );
    assert_row_keys(&sections[0]["rows"][0]);

    // `groupBy`, separately: it is parsed by a different branch, and a view
    // that sorts but does not group is the Genres-tab bug in another costume.
    let grouped = call(
        &webview,
        "library_view",
        serde_json::json!({ "view": { "groupBy": "artist" } }),
    );
    let headers: Vec<&str> = grouped
        .as_array()
        .expect("sections")
        .iter()
        .map(|s| s["header"].as_str().expect("header"))
        .collect();
    assert_eq!(
        headers,
        ["Nils Frahm", "Aphex Twin"],
        "grouped by artist, in the order the rows arrived"
    );

    // A narrowing filter, which is what opening an album tile sends.
    let one_album = call(
        &webview,
        "library_view",
        serde_json::json!({ "view": { "album": "All Melody" } }),
    );
    assert_eq!(
        one_album[0]["rows"].as_array().expect("rows").len(),
        2,
        "exactly that album, not a substring match: {one_album}"
    );
}

/// The Albums tab draws one tile per album, and reads five keys off each.
///
/// It used to draw one per *track* — "All Melody" was a header with nine tiles
/// under it, none of which was the album. The count is the assertion that says
/// which of the two this is.
#[test]
fn the_albums_tab_gets_one_entity_per_album_not_per_track() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| state.rows = library());

    let albums = call(
        &webview,
        "library_entities",
        serde_json::json!({ "view": { "groupBy": "album" } }),
    );
    let list = albums.as_array().expect("entities");
    assert_eq!(list.len(), 3, "three records, four tracks: {albums}");

    let melody = list
        .iter()
        .find(|e| e["name"] == "All Melody")
        .unwrap_or_else(|| panic!("no All Melody tile in {albums}"));
    assert_eq!(melody["tracks"], 2, "two of its tracks are held");
    assert_eq!(melody["subtitle"], "Nils Frahm", "the album's artist");
    assert!(
        melody["lead"]
            .as_str()
            .is_some_and(|l| l.starts_with("/frahm/all-melody/")),
        "the lead is an href to fetch a cover with: {melody}"
    );

    for key in ["lastPlayed", "totalTracks", "recordType", "incomplete"] {
        assert!(melody.get(key).is_some(), "an album tile is missing {key}");
    }
    // Nobody has looked this album up, so nothing knows how long it is — and an
    // unknown length must never make a tile claim tracks are missing.
    assert_eq!(melody["totalTracks"], 0, "0 means unknown, not empty");
    assert_eq!(melody["incomplete"], false, "never incomplete on a guess");

    let artists = call(
        &webview,
        "library_entities",
        serde_json::json!({ "view": { "groupBy": "artist" } }),
    );
    assert_eq!(
        artists.as_array().expect("entities").len(),
        2,
        "the same call grouped by artist: {artists}"
    );
}

/// Hiding duplicates is a setting the table has to obey, not a filter the
/// table applies for itself.
///
/// Three commands and one round trip: the count that justifies offering the
/// switch, the switch, and the read that has to come back shorter afterwards.
#[test]
fn hiding_duplicates_shortens_the_next_library_read() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| {
        state.rows = vec![
            row("/frahm/spaces/01.m4a", "Says", "Nils Frahm", "Spaces"),
            // The same recording, a second file. Title and artist are the key.
            row("/downloads/says (1).m4a", "Says", "Nils Frahm", "Spaces"),
            row("/aphex/syro/01.m4a", "Minipops 67", "Aphex Twin", "Syro"),
        ];
    });

    assert_eq!(
        call(&webview, "duplicate_count", serde_json::json!({})),
        1,
        "one file is a second copy"
    );

    let saved = call(
        &webview,
        "set_hide_duplicates",
        serde_json::json!({ "enabled": true }),
    );
    assert_eq!(
        saved["hideDuplicates"], true,
        "the setter answers with the whole settings object, which the frontend \
         swaps in wholesale: {saved}"
    );
    assert_eq!(
        call(&webview, "settings", serde_json::json!({}))["hideDuplicates"],
        true,
        "and it survives to the next read"
    );

    let rows = call(&webview, "library_view", serde_json::json!({ "view": {} }));
    assert_eq!(
        rows[0]["rows"].as_array().expect("rows").len(),
        2,
        "the table obeys the setting without being told again: {rows}"
    );
}

/// The detail sheet's own keys, four of which are camelCase and none of which
/// have a fallback on the other side — a missing one renders as `undefined`.
#[test]
fn a_track_detail_sheet_arrives_under_the_keys_it_draws() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| state.rows = library());

    let details = call(
        &webview,
        "track_details",
        serde_json::json!({ "href": "/frahm/all-melody/01.m4a" }),
    );
    assert_eq!(details["title"], "All Melody");
    assert_eq!(details["artist"], "Nils Frahm");
    assert_eq!(
        details["hrefPath"], "/frahm/all-melody/01.m4a",
        "the sovereignty line: where the file actually is"
    );

    for key in [
        "analysed",
        "bpm",
        "bpmIsManual",
        "key",
        "lufs",
        "duration",
        "cueIn",
        "cueOut",
        "energy",
        "beats",
        "waveform",
        "cached",
        "unplayable",
        "cover",
        "notes",
        "tagged",
    ] {
        assert!(
            details.get(key).is_some(),
            "track details are missing {key}: {details}"
        );
    }
    // Nothing has been analysed, and the sheet has to be able to say so rather
    // than print a column of zeroes it cannot distinguish from measurements.
    assert_eq!(details["analysed"], false);
    assert_eq!(details["bpmIsManual"], false);
    assert!(details["waveform"].as_array().expect("array").is_empty());
    assert!(
        details["unplayable"].is_null(),
        "null, not the empty string"
    );
}

/// A failure has to arrive as a rejection carrying a sentence.
///
/// The first defect that ever reached an outside user was an error bar that
/// could not be dismissed, and the shape of the thing in it is the start of
/// that story: a command that answered `Ok` with an empty body would leave the
/// sheet blank instead, which is the other half of the same bug.
#[test]
fn asking_for_a_track_that_is_not_there_is_an_error_not_an_empty_answer() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| state.rows = library());

    let error = call_err(
        &webview,
        "track_details",
        serde_json::json!({ "href": "/gone.m4a" }),
    );
    let message = error
        .as_str()
        .unwrap_or_else(|| panic!("an error is a bare string for a person to read, not {error}"));
    assert!(
        message.contains("not in the library"),
        "and it has to say what went wrong: {message}"
    );
}

/// Search's own ranking, and the two facet lists the chips are built from.
#[test]
fn search_puts_a_title_that_starts_with_the_query_on_top() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| {
        state.rows = vec![
            // Contains "salt" without starting with it, which is the whole
            // point of the comparison below. The first draft of this used
            // "Asphalt Sunday", which reads like it contains "salt" and does
            // not — a-s-p-h-a-l-t — so only one row matched and the ranking
            // being tested never happened.
            row("/a/sea-salt.m4a", "Sea Salt", "Bibio", "Ambivalence"),
            row(
                "/b/salt-flats.m4a",
                "Salt Flats",
                "Bibio",
                "Silver Wilkinson",
            ),
        ];
    });

    let results = call(&webview, "search", serde_json::json!({ "query": "salt" }));
    assert_eq!(
        results["top"]["title"], "Salt Flats",
        "starts-with beats contains: {results}"
    );
    assert_row_keys(&results["top"]);
    assert_eq!(results["total"], 2, "both matched");

    let tracks = results["tracks"].as_array().expect("tracks");
    assert_eq!(
        tracks.len(),
        1,
        "the top result is shown separately, not twice: {results}"
    );
    assert_eq!(tracks[0]["title"], "Sea Salt");

    let artists = results["artists"].as_array().expect("artists");
    assert_eq!(artists[0]["label"], "Bibio", "a facet is label and count");
    assert_eq!(artists[0]["count"], 2);
    assert!(results["albums"].is_array() && results["playlists"].is_array());

    // An empty query is the cleared search box, and it must answer emptily
    // rather than with the whole library.
    let cleared = call(&webview, "search", serde_json::json!({ "query": "  " }));
    assert!(cleared["top"].is_null());
    assert_eq!(cleared["total"], 0);
}

/// The home screen is one call, and a shelf resolves its own size.
///
/// A group holds artists, not tracks, so "3 tracks" under a group is a claim
/// only the shell can make — the frontend has no way to work it out and does
/// not try.
#[test]
fn the_home_shelves_resolve_a_group_against_the_library() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| state.rows = library());

    let playlist = call(
        &webview,
        "create_playlist",
        serde_json::json!({ "name": "Late Night" }),
    );
    call(
        &webview,
        "add_tracks_to_playlist",
        serde_json::json!({
            "id": playlist["id"],
            "hrefs": ["/frahm/all-melody/01.m4a", "/frahm/spaces/01.m4a"],
        }),
    );
    let group = call(
        &webview,
        "create_group",
        serde_json::json!({ "name": "Frahm" }),
    );
    call(
        &webview,
        "add_to_group",
        serde_json::json!({ "id": group["id"], "entityType": "artist", "value": "Nils Frahm" }),
    );

    let home = call(&webview, "home_shelves", serde_json::json!({}));
    assert_eq!(home["tracks"], 4, "the line under the title");

    let shelf = &home["playlists"][0];
    for key in ["id", "title", "subtitle", "lead", "tracks", "plays"] {
        assert!(
            shelf.get(key).is_some(),
            "a shelf is missing {key}: {shelf}"
        );
    }
    assert_eq!(shelf["id"], playlist["id"], "what pressing it opens");
    assert_eq!(shelf["title"], "Late Night");
    assert_eq!(shelf["subtitle"], "2 tracks");
    assert_eq!(shelf["lead"], "/frahm/all-melody/01.m4a");

    assert_eq!(
        home["groups"][0]["tracks"], 3,
        "three Frahm tracks in the library, none of them named by the group"
    );
    assert_eq!(home["groups"][0]["subtitle"], "3 tracks");

    let artists = home["artists"].as_array().expect("artists");
    let frahm = artists
        .iter()
        .find(|s| s["title"] == "Nils Frahm")
        .unwrap_or_else(|| panic!("no Nils Frahm shelf in {artists:?}"));
    assert_eq!(frahm["tracks"], 3);
    assert_eq!(home["albums"].as_array().expect("albums").len(), 3);
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

/// Adding tracks and reading them back are two different shapes.
///
/// `playlists` carries hrefs; `playlist_rows` carries rows resolved against the
/// library, in the playlist's own order rather than the library's. Both sides
/// of that are things the screen depends on and neither is checked anywhere
/// else.
#[test]
fn tracks_added_to_a_playlist_come_back_as_rows_in_that_order() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| state.rows = library());

    let playlist = call(
        &webview,
        "create_playlist",
        serde_json::json!({ "name": "Sets" }),
    );
    let id = playlist["id"].clone();

    let added = call(
        &webview,
        "add_tracks_to_playlist",
        serde_json::json!({
            "id": id,
            // Deliberately not library order: a playlist is ordered by hand.
            "hrefs": ["/frahm/spaces/01.m4a", "/aphex/syro/01.m4a"],
        }),
    );
    assert_eq!(added, 2, "the count is what the toast says");

    let rows = call(&webview, "playlist_rows", serde_json::json!({ "id": id }));
    let list = rows.as_array().expect("rows");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["title"], "Says", "the order it was added in");
    assert_eq!(list[1]["title"], "Minipops 67");
    assert_row_keys(&list[0]);

    // The same tracks again is not an error and not a second copy — the button
    // stays pressable and the answer says nothing happened.
    assert_eq!(
        call(
            &webview,
            "add_tracks_to_playlist",
            serde_json::json!({ "id": id, "hrefs": ["/frahm/spaces/01.m4a"] }),
        ),
        0,
        "already there"
    );
    let rows = call(&webview, "playlist_rows", serde_json::json!({ "id": id }));
    assert_eq!(rows.as_array().expect("rows").len(), 2);
}

/// A group names artists and resolves to tracks, which is the whole feature:
/// a record added tomorrow belongs to it without anyone saying so.
#[test]
fn a_group_resolves_to_the_rows_of_the_artist_it_names() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| state.rows = library());

    let group = call(
        &webview,
        "create_group",
        serde_json::json!({ "name": "Frahm" }),
    );
    call(
        &webview,
        "add_to_group",
        serde_json::json!({ "id": group["id"], "entityType": "artist", "value": "Nils Frahm" }),
    );

    let rows = call(
        &webview,
        "group_tracks",
        serde_json::json!({ "id": group["id"] }),
    );
    let list = rows.as_array().expect("rows");
    assert_eq!(list.len(), 3, "the Frahm rows and only those: {rows}");
    assert!(list.iter().all(|r| r["artist"] == "Nils Frahm"), "{rows}");
    assert_row_keys(&list[0]);

    // A group that does not exist is an empty list, not an error: the screen
    // asks for one it has just deleted often enough that failing would put an
    // error bar in front of a person for nothing.
    let gone = call(
        &webview,
        "group_tracks",
        serde_json::json!({ "id": "group-nope" }),
    );
    assert_eq!(gone.as_array().expect("array").len(), 0);
}

// ---------------------------------------------------------------------------
// Playback, which the transport polls four times a second
// ---------------------------------------------------------------------------

/// The transport names what is playing and what is next, off the shell's own
/// rows — the UI holds no copy of the table.
///
/// Twenty-nine keys, of which twelve are camelCase. `available` is false here
/// because a test machine has no output device, and that is the honest answer:
/// the transport disables itself rather than offering buttons that do nothing.
#[test]
fn the_transport_names_the_playing_track_and_the_one_after_it() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| {
        state.rows = library();
        let hrefs: Vec<String> = state.rows.iter().map(|r| r.href.clone()).collect();
        state.queue.set_tracks(hrefs.clone(), Some(&hrefs[0]));
        state.playing = Some(hrefs[0].clone());
    });

    let playback = call(&webview, "playback_state", serde_json::json!({}));
    assert_eq!(playback["href"], "/frahm/all-melody/02.m4a");
    assert_eq!(playback["title"], "Sunson");
    assert_eq!(playback["artist"], "Nils Frahm");
    assert_eq!(playback["status"], "idle", "lowercase, as `Status` says");

    assert_eq!(
        playback["nextTitle"], "Minipops 67",
        "Now Playing draws this without a second call: {playback}"
    );
    assert_eq!(playback["nextArtist"], "Aphex Twin");
    assert_eq!(playback["nextAlbum"], "Syro");
    assert_eq!(
        playback["nextHref"], "/aphex/syro/01.m4a",
        "and asks for its artwork by href, never inline"
    );

    for key in [
        "beatPeriod",
        "nextBeat",
        "setIndex",
        "setTotal",
        "setEnergy",
        "waveform",
        "loading",
        "mixing",
        "available",
        "level",
        "brightness",
        "position",
        "duration",
        "volume",
        "scope",
    ] {
        assert!(
            playback.get(key).is_some(),
            "playback state is missing {key}: {playback}"
        );
    }
    assert!(playback["beatPeriod"].is_number() && playback["nextBeat"].is_number());
    assert_eq!(
        playback["available"], false,
        "no output device on a test machine, and the UI is told so"
    );
    assert!(playback["error"].is_null());
    // No cover art was ever embedded on this wire — see `queue_view`'s own
    // reason. `null` is the whole payload for a track with no stored sleeve.
    assert!(playback["cover"].is_null());
}

/// The queue screen, including which row to scroll to.
#[test]
fn the_queue_view_marks_the_playing_row_and_carries_no_artwork() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| {
        state.rows = library();
        let hrefs: Vec<String> = state.rows.iter().map(|r| r.href.clone()).collect();
        state.queue.set_tracks(hrefs.clone(), Some(&hrefs[1]));
    });

    let view = call(&webview, "queue_view", serde_json::json!({}));
    let entries = view["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 4);
    assert_eq!(view["current"], 1, "the index the screen scrolls to");
    // `all`, because `Repeat::All` carries `#[default]` — wrapping to the
    // beginning is the deliberate default, not "off". What is being checked
    // here is the shape: the frontend switches on a word, and an enum that
    // serialised as an integer would strand it.
    assert_eq!(view["repeat"], "all", "a word, not a number");
    assert_eq!(view["shuffled"], false);
    assert!(
        view["remainingSecs"].is_number(),
        "camelCase, and a number: {view}"
    );

    for key in ["href", "title", "artist", "bpm", "key", "current"] {
        assert!(
            entries[1].get(key).is_some(),
            "a queue entry is missing {key}: {}",
            entries[1]
        );
    }
    assert_eq!(entries[1]["current"], true, "and only that one");
    assert_eq!(entries[0]["current"], false);
    assert!(
        entries[0].get("cover").is_none(),
        "a cover per entry is hundreds of megabytes on a real library"
    );
}

// ---------------------------------------------------------------------------
// Storage, downloads and the folders the library reads from
// ---------------------------------------------------------------------------

/// The bound comes back sanitised, and the screen has to use what came back.
///
/// The core clamps to `MIN_CACHE_BYTES`, so a slider that keeps its own value
/// and never reads the answer shows a number the app is not using. Two
/// commands, and the second is what proves the first was applied rather than
/// merely acknowledged.
#[test]
fn the_cache_bound_is_sanitised_on_the_way_in_and_reported_back() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| state.rows = library());

    let applied = call(
        &webview,
        "set_cache_max_bytes",
        serde_json::json!({ "bytes": 1024 }),
    );
    let applied = applied
        .as_u64()
        .unwrap_or_else(|| panic!("a JSON number, not a bigint or a string: {applied}"));
    assert!(
        applied > 1024,
        "1 KB is below the floor, and the floor is what is in force: {applied}"
    );

    let status = call(&webview, "cache_status", serde_json::json!({}));
    assert_eq!(
        status["maxBytes"].as_u64(),
        Some(applied),
        "the status has to agree with what the setter answered: {status}"
    );
    assert_eq!(status["tracksTotal"], 4, "the library, not the cache");
    assert_eq!(status["tracksCached"], 0, "nothing is held yet");
    assert!(
        status["bytes"].is_number() && !status["location"].as_str().unwrap_or("").is_empty(),
        "Your Data names the directory it is talking about: {status}"
    );
}

/// Downloads are kept per collection, and dropped per collection.
///
/// `remove_download` answers with how many it actually let go of, which is the
/// only thing that distinguishes "nothing was downloaded" from "the button did
/// nothing".
#[test]
fn a_downloaded_playlist_is_listed_and_then_let_go_of() {
    let (app, webview, _dir) = seam();
    with_state(&app, |state| state.rows = library());

    let playlist = call(
        &webview,
        "create_playlist",
        serde_json::json!({ "name": "For the train" }),
    );
    let id = playlist["id"].clone();
    call(
        &webview,
        "add_tracks_to_playlist",
        serde_json::json!({
            "id": id,
            "hrefs": ["/frahm/spaces/01.m4a", "/aphex/syro/01.m4a"],
        }),
    );
    // Standing in for `download_collection`, which fetches audio over the
    // network. What is being tested here is the wire, not the transfer.
    with_state(&app, |state| {
        state.pinned.insert("/frahm/spaces/01.m4a".to_string());
        state.pinned.insert("/aphex/syro/01.m4a".to_string());
    });

    let mut kept: Vec<String> = call(&webview, "downloaded_tracks", serde_json::json!({}))
        .as_array()
        .expect("array of hrefs")
        .iter()
        .map(|h| h.as_str().expect("href").to_string())
        .collect();
    // A set on the other side of the wire, so the order is not the assertion.
    kept.sort();
    assert_eq!(kept, ["/aphex/syro/01.m4a", "/frahm/spaces/01.m4a"]);

    let removed = call(
        &webview,
        "remove_download",
        serde_json::json!({ "kind": "playlist", "id": id }),
    );
    assert_eq!(removed, 2, "both, because nothing else was keeping them");
    assert_eq!(
        call(&webview, "downloaded_tracks", serde_json::json!({}))
            .as_array()
            .expect("array")
            .len(),
        0
    );
}

/// Adding a folder is the first thing a person does, and the id it comes back
/// with is what every later href is built from.
#[test]
fn a_folder_added_is_a_folder_listed_and_adding_it_twice_is_not() {
    let (app, webview, dir) = seam();
    let path = dir.to_str().expect("utf-8 path").to_string();

    let folders = call(
        &webview,
        "add_local_folder",
        serde_json::json!({ "path": path }),
    );
    let list = folders.as_array().expect("folders");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["path"], path);
    assert_eq!(list[0]["name"], "", "empty means use the folder's own name");
    assert!(
        list[0]["id"].as_str().is_some_and(|id| !id.is_empty()),
        "every href from this folder names this id: {folders}"
    );

    let read_back = call(&webview, "local_folders", serde_json::json!({}));
    assert_eq!(
        read_back, folders,
        "the settings screen reads the same list"
    );

    // Pressing add on a folder you already have is not an error — it is a
    // no-op, with the list as the answer.
    let again = call(
        &webview,
        "add_local_folder",
        serde_json::json!({ "path": path }),
    );
    assert_eq!(again.as_array().expect("folders").len(), 1, "{again}");

    let refused = call_err(
        &webview,
        "add_local_folder",
        serde_json::json!({ "path": dir.join("not-here").to_str().expect("utf-8") }),
    );
    assert!(
        refused
            .as_str()
            .is_some_and(|m| m.contains("not a folder this app can open")),
        "the path comes from the webview and is not trusted: {refused}"
    );
    let _ = app;
}

/// The sync panel draws before anything is paired, and with sync switched off.
///
/// That is the state every install starts in and most stay in, so it is the
/// state the panel has to survive. `deviceId` and `pairingWith` are the two
/// keys it branches on.
#[test]
fn the_sync_panel_gets_a_device_identity_with_sync_switched_off() {
    let (_app, webview, _dir) = seam();

    let view = call(&webview, "sync_view", serde_json::json!({}));
    assert_eq!(view["enabled"], false, "off is the default");
    assert!(
        view["deviceId"].as_str().is_some_and(|id| !id.is_empty()),
        "camelCase, and never empty — a peer addresses this: {view}"
    );
    assert!(view["deviceName"].as_str().is_some_and(|n| !n.is_empty()));
    assert_eq!(view["discovered"].as_array().expect("array").len(), 0);
    assert_eq!(view["trusted"].as_array().expect("array").len(), 0);
    assert!(
        view["pin"].is_null() && view["pairingWith"].is_null(),
        "nothing is being paired: {view}"
    );

    let progress = &view["progress"];
    for key in [
        "running", "peer", "file", "done", "total", "bytes", "elapsed", "error",
    ] {
        assert!(
            progress.get(key).is_some(),
            "sync progress is missing {key}: {progress}"
        );
    }
    assert_eq!(progress["running"], false);
    assert_eq!(progress["error"], "", "an empty string, not null");
}

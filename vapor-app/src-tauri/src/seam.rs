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
//! Deliberately few. This is the layer that is expensive to write and slow to
//! run, so it covers the round trips that would strand a user — not every
//! command.

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
    "dynamic_groups",
    "create_group",
    "add_to_group",
    "settings",
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
            crate::commands::groups::dynamic_groups,
            crate::commands::groups::create_group,
            crate::commands::groups::add_to_group,
            crate::commands::settings::settings,
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

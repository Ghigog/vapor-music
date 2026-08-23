//! Sync and peer commands — pairing with a device on the network, and the
//! shared document on the server.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// Turn local-network sync on or off.
///
/// Turning it on starts the beacon and the server there and then, because
/// "restart the app" is not an answer to "I pressed the switch". Turning it off
/// stops them the same way (TD-58): the trust is cleared and the two threads
/// are stopped and joined, so the machine is neither announcing itself nor
/// holding a port by the time this returns.
#[tauri::command]
pub fn set_sync_enabled(enabled: bool, state: State<'_, Shared>) -> Result<Settings> {
    let shared: Shared = Arc::clone(&state);
    let (start, session) = {
        let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
        let was = app.settings.sync_enabled;
        app.settings.sync_enabled = enabled;
        let session = if enabled {
            None
        } else {
            // Off means off: a device that can no longer be discovered should
            // not still be trusted by one that can.
            app.trust = vapor_library::sync::Trust::new();
            app.pairing = None;
            app.pin = None;
            app.save_trust()?;
            app.sync_session.take()
        };
        app.save_settings()?;
        (enabled && !was, session)
    };

    // Outside the lock. Stopping joins two threads, and one of them takes the
    // peer registry's lock on the way round — holding the app lock across that
    // is a deadlock waiting for the right moment.
    if let Some(session) = session {
        session.stop();
    }

    if start {
        let (id, name, kind, registry) = {
            let app = shared.lock().map_err(|e| Error(e.to_string()))?;
            let kind = if cfg!(any(target_os = "ios", target_os = "android")) {
                vapor_library::sync::DeviceKind::Phone
            } else {
                vapor_library::sync::DeviceKind::Desktop
            };
            let _ = app.store.save("device_id", &app.device_id);
            (
                app.device_id.clone(),
                app.device_name(),
                kind,
                Arc::clone(&app.peers),
            )
        };
        let started = peers::start(
            registry,
            id,
            name,
            kind,
            Arc::new(ServedLibrary(Arc::clone(&shared))),
        );
        if let Ok(mut app) = shared.lock() {
            app.sync_session = started;
        }
    }

    let app = shared.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.settings.clone())
}

#[tauri::command]
pub fn sync_view(state: State<'_, Shared>) -> Result<SyncView> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !app.settings.sync_enabled {
        return Ok(SyncView {
            enabled: false,
            device_id: app.device_id.clone(),
            device_name: app.device_name(),
            discovered: Vec::new(),
            trusted: Vec::new(),
            pin: None,
            pairing_with: None,
            progress: SyncProgress::default(),
        });
    }
    let discovered = app
        .peers
        .lock()
        .map(|mut registry| registry.live(peers::now()).to_vec())
        .unwrap_or_default();

    Ok(SyncView {
        enabled: true,
        device_id: app.device_id.clone(),
        device_name: app.device_name(),
        discovered,
        trusted: app.trust.all().to_vec(),
        pin: app.pin.clone(),
        pairing_with: app.pairing.as_ref().map(|p| p.peer_id().to_string()),
        progress: app.sync.clone(),
    })
}

/// Show a code, for `peer_id` to type in.
///
/// The code is bound to that one device, so a PIN on screen is not an
/// invitation to everything else on the subnet that can see it.
#[tauri::command]
pub fn open_pairing(peer_id: String, state: State<'_, Shared>) -> Result<String> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let pin = peers::new_pin();
    app.pairing = Some(vapor_library::sync::Pairing::begin(
        pin.clone(),
        &peer_id,
        peers::now(),
    ));
    app.pin = Some(pin.clone());
    Ok(pin)
}

#[tauri::command]
pub fn cancel_pairing(state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.pairing = None;
    app.pin = None;
    Ok(())
}

/// Type the code the other device is showing.
#[tauri::command]
pub fn pair_with(peer_id: String, pin: String, state: State<'_, Shared>) -> Result<String> {
    let (address, me, my_name, kind) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        let registry = app.peers.lock().map_err(|e| Error(e.to_string()))?;
        let peer = registry
            .get(&peer_id)
            .ok_or_else(|| Error("That device is no longer on the network.".to_string()))?;
        (
            peer.address.clone(),
            app.device_id.clone(),
            app.device_name(),
            peer.kind,
        )
    };

    // A crafted advert must not be able to point this device at a host on the
    // internet and have it open a connection there.
    if !peers::is_local(&address) {
        return Err(Error(
            "That device is not on this local network.".to_string(),
        ));
    }

    // Half of the key every later reply from that device is checked against
    // (AUD-7). Made before the request goes out, because a pairing that cannot
    // produce one is a pairing worth refusing rather than completing.
    let handshake = peers::Handshake::begin().ok_or_else(|| {
        Error(
            "This device has no source of randomness, so it cannot make a key to \
             check that device's replies with."
                .to_string(),
        )
    })?;

    let (reply, _) = peers::ask(
        &address,
        &peers::Request::Pair {
            device_id: me.clone(),
            name: my_name,
            device_kind: if cfg!(any(target_os = "ios", target_os = "android")) {
                vapor_library::sync::DeviceKind::Phone
            } else {
                vapor_library::sync::DeviceKind::Desktop
            },
            pin,
            public_key: handshake.public_key(),
        },
    )
    .map_err(Error)?;

    match reply {
        peers::Reply::Paired {
            device_id,
            name,
            public_key,
        } => {
            // An empty or unusable half means the other device did not run the
            // exchange — a build from before AUD-7 sends none. Recording the
            // pairing anyway would give a trusted device whose replies nothing
            // could check, which is exactly the state AUD-7 removed.
            let key = handshake
                .finish(&public_key, &me, &device_id)
                .ok_or_else(|| {
                    Error(
                        "That device did not complete the pairing exchange, so nothing it \
                     sent could be checked. It may be running an older version of Vapor."
                            .to_string(),
                    )
                })?;
            let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
            app.trust.add(&device_id, &name, kind, key, peers::now());
            app.save_trust()?;
            Ok(name)
        }
        peers::Reply::Refused { reason } => Err(Error(reason)),
        _ => Err(Error(
            "That device answered with something else.".to_string(),
        )),
    }
}

#[tauri::command]
pub fn forget_peer(peer_id: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let forgotten = app.trust.forget(&peer_id);
    if forgotten {
        app.save_trust()?;
    }
    Ok(forgotten)
}

/// Sync with a paired device, on a thread, reporting progress as it goes.
#[tauri::command]
pub fn sync_with(
    peer_id: String,
    what: Option<SyncWhat>,
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<()> {
    use tauri::Emitter as _;

    let shared: Shared = Arc::clone(&state);
    let what = what.unwrap_or_default();

    let (address, name) = {
        let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;
        if app.sync.running {
            return Err(Error("A sync is already running.".to_string()));
        }
        if !app.trust.allows(&peer_id) {
            // Told apart here and only here. On the wire a stale pairing and an
            // unknown device get the same refusal, so nothing on the subnet can
            // ask which device ids this one knows — but this side is the owner's
            // own screen, and "not paired" would be a baffling thing to read
            // about a device that is sitting in the paired list.
            return Err(Error(if app.trust.needs_repairing(&peer_id) {
                "That device was paired before Vapor started checking who sent a \
                 transfer. Pair with it again."
                    .to_string()
            } else {
                "That device is not paired.".to_string()
            }));
        }
        let registry = app.peers.lock().map_err(|e| Error(e.to_string()))?;
        let peer = registry
            .get(&peer_id)
            .ok_or_else(|| Error("That device is not on the network.".to_string()))?;
        let found = (peer.address.clone(), peer.name.clone());
        drop(registry);

        if !peers::is_local(&found.0) {
            return Err(Error(
                "That device is not on this local network.".to_string(),
            ));
        }
        app.sync = SyncProgress {
            running: true,
            peer: found.1.clone(),
            ..Default::default()
        };
        found
    };

    let started = std::time::Instant::now();
    let spawned = std::thread::Builder::new()
        .name("vapor-sync".into())
        .spawn(move || {
            let outcome = run_sync(&shared, &peer_id, &address, what, started);
            if let Ok(mut app) = shared.lock() {
                app.sync.running = false;
                app.sync.elapsed = started.elapsed().as_secs_f64();
                if let Err(e) = outcome {
                    app.sync.error = e;
                }
            }
            let _ = app_handle.emit("sync-changed", ());
        });

    if let Err(e) = spawned {
        // The thread never started, so nothing will ever clear the flag it was
        // set under. Left running, the dashboard shows a sync that is not
        // happening and refuses to start another.
        let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.sync.running = false;
        app.sync.error = format!("could not start the sync: {e}");
        return Err(Error(format!("could not start the sync: {e}")));
    }
    let _ = name;
    Ok(())
}

/// Pull the shared document, merge it in, and push the result back.
///
/// One round trip does both halves on purpose. Pulling without pushing leaves
/// this device's playlists invisible to every other one; pushing without
/// pulling overwrites theirs. Doing them separately means a person has to know
/// to do both, in the right order.
#[tauri::command]
pub fn sync_shared_document(
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<SharedSyncResult> {
    let (remote_config, href) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        if !app.settings.remote.is_configured() {
            return Err(Error(
                "No server is configured, so there is nowhere to keep it.".to_string(),
            ));
        }
        (
            app.settings.remote.clone(),
            webdav::shared_document_href(&app.settings.remote.folder),
        )
    };

    // Built outside the lock: it reads the keychain and opens a client.
    let fetcher = webdav::Fetcher::new(&remote_config).map_err(Error)?;
    let existing = fetcher.fetch_optional(&href).map_err(Error)?;

    let mut result = SharedSyncResult {
        created: existing.is_none(),
        ..Default::default()
    };

    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

    if let Some(bytes) = existing {
        let incoming: vapor_library::sync::Shared =
            serde_json::from_slice(&bytes).map_err(|e| {
                // Deliberately not overwritten with a fresh one. A document this
                // build cannot read may be one a newer build wrote, and replacing
                // it would delete another device's playlists to fix a parse error.
                Error(format!(
                    "The file on the server could not be read, so nothing was changed: {e}"
                ))
            })?;

        if incoming.version > vapor_library::sync::SHARED_VERSION {
            return Err(Error(
                "That file was written by a newer version of Vapor. Update this device before \
                 syncing, or it would write back less than it read."
                    .to_string(),
            ));
        }

        let AppState {
            playlists,
            folders,
            groups,
            settings,
            tombstones,
            ..
        } = &mut *app;
        let report = vapor_library::sync::merge_shared(
            playlists,
            folders,
            groups,
            &mut settings.bpm_overrides,
            tombstones,
            &incoming,
        );
        result.playlists_added = report.playlists_added;
        result.playlists_extended = report.playlists_extended;
        result.folders_added = report.folders_added;
        result.tempos_added = report.tempos_added;
        result.playlists_deleted = report.playlists_deleted;
        result.folders_deleted = report.folders_deleted;

        if !report.is_empty() {
            app.save_playlists()?;
            app.save_folders()?;
            // A group that arrived from another device is only real once it is
            // on disk — the rail reads the saved store on the next start
            // (AUD-11).
            app.save_groups()?;
            app.save_settings()?;
            // Tombstones learned from the document are kept, so this device
            // passes the deletion on to the next one it syncs with rather than
            // being the place a deletion stops travelling.
            app.save_tombstones()?;
            // A merged playlist changes what the rows mean, and a corrected
            // tempo changes what the table shows.
            let overrides = app.settings.bpm_overrides.clone();
            for row in app.rows.iter_mut() {
                if let Some(bpm) = overrides.get(&row.href) {
                    row.bpm = *bpm;
                }
            }
        }
    }

    let outgoing = shared_document(&app);
    // A tempo that arrived from another device is a correction like any other,
    // and leaves this device's beat grid tracked at the number it replaced. The
    // whole library is offered rather than only the merged hrefs — `stale_grids`
    // is the predicate for what actually needs work, and the report only carries
    // a count.
    let corrected: Vec<String> = if result.tempos_added > 0 {
        app.settings.bpm_overrides.keys().cloned().collect()
    } else {
        Vec::new()
    };
    drop(app);

    if !corrected.is_empty() {
        retrack_grids(&app_handle, state.inner(), corrected);
    }

    let bytes = serde_json::to_vec_pretty(&outgoing).map_err(|e| Error(e.to_string()))?;
    fetcher.put(&href, bytes).map_err(Error)?;

    Ok(result)
}

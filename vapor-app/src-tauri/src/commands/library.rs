//! Library commands — what the library screens read, and rebuilding
//! the index they read from.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// What the library screen opens on.
///
/// One call for all four shelves rather than four, because they are one screen
/// and four round trips is four chances to paint a half-built page.
#[tauri::command]
pub fn home_shelves(state: State<'_, Shared>) -> Result<HomeShelves> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(home_shelves_for(&app))
}

/// Filter, sort and group the library in one call.
///
/// One round trip rather than three: the table re-runs this per keystroke, and
/// the predicates are the same ones a smart playlist uses, so splitting them
/// would let the two disagree.
#[tauri::command]
pub fn library_view(view: LibraryView, state: State<'_, Shared>) -> Result<Vec<LibrarySection>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let mut rows = resolved_rows(&app, &view);

    if let Some(key) = view.sort_key.as_deref().and_then(parse_sort_key) {
        vapor_library::sort_rows(&mut rows, key, view.ascending);
    }

    let group = view
        .group_by
        .as_deref()
        .and_then(parse_group_by)
        .unwrap_or(GroupBy::None);

    Ok(vapor_library::group_rows(&rows, group)
        .into_iter()
        .map(|(header, rows)| LibrarySection {
            header,
            rows: rows.into_iter().cloned().collect(),
        })
        .collect())
}

/// The albums or artists in the library, one entry each.
///
/// Grouping rows and drawing a card per row is what the Albums tab did, and it
/// answers a different question: "which tracks are on this album" rather than
/// "which albums do I have". Tracks whose album or artist is unknown are left
/// out entirely — a tab called Albums listing things that are not albums is
/// the complaint this exists to fix. They remain reachable under Songs.
#[tauri::command]
pub fn library_entities(view: LibraryView, state: State<'_, Shared>) -> Result<Vec<LibraryEntity>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(library_entities_for(&app, &view))
}

#[tauri::command]
pub fn duplicate_count(state: State<'_, Shared>) -> Result<usize> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(duplicate_hrefs(&app).len())
}

#[tauri::command]
pub fn track_details(href: String, state: State<'_, Shared>) -> Result<TrackDetails> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let row = app
        .rows
        .iter()
        .find(|r| r.href == href)
        .ok_or_else(|| Error("That track is not in the library.".to_string()))?;
    let analysis = app.analysis.get(&href);
    let manual = app.settings.bpm_override(&href);
    // What the rest of the app is using for this track. `bpm_is_manual` stays
    // tied to `manual` alone: a genre-resolved octave is this app's inference,
    // not something the person typed, and the detail sheet must not claim it
    // was theirs.
    let in_force = tempo_in_force(&app, &href, analysis);

    Ok(TrackDetails {
        href: href.clone(),
        title: row.title.clone(),
        artist: if row.artist_source == vapor_library::index::Source::Unknown {
            String::new()
        } else {
            row.artist.clone()
        },
        album: if row.album_source == vapor_library::index::Source::Unknown {
            String::new()
        } else {
            row.album.clone()
        },
        year: row.year,
        genre: row.genre.clone(),
        analysed: analysis.is_some(),
        bpm: in_force.or_else(|| analysis.map(|a| a.bpm)).unwrap_or(0.0),
        bpm_is_manual: manual.is_some(),
        key: analysis.map(|a| a.key.clone()).unwrap_or_default(),
        lufs: analysis.map_or(0.0, |a| a.lufs),
        duration: analysis.map_or(0.0, |a| a.duration),
        cue_in: analysis.map_or(0.0, |a| a.cue_in),
        cue_out: analysis.map_or(0.0, |a| a.cue_out),
        energy: analysis.map_or(0.0, |a| a.energy),
        beats: analysis.map_or(0, |a| a.beats.len()),
        waveform: analysis.map(|a| a.waveform.clone()).unwrap_or_default(),
        href_path: href.clone(),
        cached: app.cache.contains(&href),
        unplayable: app.failures.get(&href).cloned(),
        cover: app.covers.get(&href),
        notes: app.tags.get(&href).and_then(|t| t.comment.clone()),
        tagged: app.tags.contains_key(&href),
    })
}

#[tauri::command]
pub fn search(query: String, state: State<'_, Shared>) -> Result<SearchResults> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    if query.trim().is_empty() {
        return Ok(SearchResults {
            top: None,
            tracks: Vec::new(),
            artists: Vec::new(),
            albums: Vec::new(),
            playlists: Vec::new(),
            total: 0,
        });
    }

    // The same predicate the table and smart playlists use, so a search and a
    // filter cannot disagree about what matches.
    let matched: Vec<Row> = vapor_library::filter(&app.rows, &query)
        .into_iter()
        .cloned()
        .map(|mut row| {
            app.apply_tags(&mut row);
            app.apply_analysis(&mut row);
            row
        })
        .collect();

    let needle = query.trim().to_lowercase();
    let facet = |pick: fn(&Row) -> &String| {
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for row in &matched {
            let value = pick(row);
            if !value.is_empty() {
                *counts.entry(value.as_str()).or_default() += 1;
            }
        }
        let mut facets: Vec<Facet> = counts
            .into_iter()
            .map(|(label, count)| Facet {
                label: label.to_string(),
                count,
            })
            .collect();
        // Most evidence first, then alphabetically so the order is stable
        // between identical searches rather than following a hash.
        facets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
        facets.truncate(6);
        facets
    };

    let artists = facet(|r| &r.artist);
    let albums = facet(|r| &r.album);

    // The best row is the one whose title starts with what was typed; failing
    // that, the one that merely contains it. Someone typing "salt" wants
    // "Salt Flats" above "Asphalt Sunday".
    let top = matched
        .iter()
        .find(|r| r.title.to_lowercase().starts_with(&needle))
        .or_else(|| {
            matched
                .iter()
                .find(|r| r.title.to_lowercase().contains(&needle))
        })
        .or(matched.first())
        .cloned();

    let playlists: Vec<vapor_library::Playlist> = app
        .playlists
        .all()
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&needle))
        .cloned()
        .collect();

    let total = matched.len();
    let tracks: Vec<Row> = matched
        .into_iter()
        // The top result is shown separately; repeating it immediately below
        // reads as a duplicate rather than as emphasis.
        .filter(|r| top.as_ref().is_none_or(|t| t.href != r.href))
        .take(SEARCH_LIMIT)
        .collect();

    Ok(SearchResults {
        top,
        tracks,
        artists,
        albums,
        playlists,
        total,
    })
}

/// Walk the configured WebDAV tree and rebuild the library index.
#[tauri::command]
pub async fn scan_library(
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<ScanReport> {
    // Copied out and the lock released before any I/O: holding it across an
    // await blocks every other command for the length of a scan, which can be
    // minutes on a large library.
    let (remote, folders) = {
        let app = state.lock().map_err(|e| Error(e.to_string()))?;
        (app.settings.remote.clone(), app.settings.folders.clone())
    };

    let has_server = remote.is_configured();
    if !has_server && folders.is_empty() {
        return Err(Error(
            "No music yet. Add a folder on this device, or a server, in Settings.".to_string(),
        ));
    }

    let mut rows: Vec<Row> = Vec::new();
    let mut directories = 0usize;
    let mut unreadable = 0usize;
    let mut problems: Vec<String> = Vec::new();

    // Folders first, and not only because they are quicker. They cannot fail
    // the way a network can, so the common case — a laptop with music on it and
    // a NAS that may or may not be awake — produces a usable library before
    // anything is allowed to go wrong.
    for folder in &folders {
        match local::scan(std::path::Path::new(&folder.path)) {
            Ok(found) => {
                directories += found.directories;
                unreadable += found.unreadable;
                rows.extend(
                    found
                        .files
                        .iter()
                        .map(|relative| build_row(&local::href(&folder.id, relative), "")),
                );
            }
            Err(e) => problems.push(format!("{}: {e}", folder.label())),
        }
    }

    if has_server {
        match webdav::scan(&remote.url, &remote.username, &remote.folder).await {
            Ok(found) => {
                directories += found.directories;
                unreadable += found.unreadable;
                rows.extend(
                    found
                        .files
                        .iter()
                        .map(|href| build_row(href, &remote.folder)),
                );
            }
            Err(e) => problems.push(format!("{}: {e}", remote.url)),
        }
    }

    // Every source failing is a failed scan. Some failing is a partial library
    // and a message, which is the difference between "your NAS is asleep" and
    // "nothing works".
    if rows.is_empty() && !problems.is_empty() {
        return Err(Error(problems.join("; ")));
    }

    let report = {
        let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.rows = rows;

        // Saved here rather than at exit: a scan is the only thing that
        // changes the index, and writing it now means a crash mid-analysis
        // still leaves a library to come back to.
        app.save_index()?;

        ScanReport {
            tracks: app.rows.len(),
            directories,
            unreadable,
            problems,
        }
    };

    // Analyse what was just found, without being asked.
    //
    // A scan produces rows that know a filename and nothing else — no tempo,
    // no key, so no Vibe DJ and no blends. That used to wait behind a button on
    // the Settings screen, which is a strange place to have to go to make the
    // library work, and an easy one never to find. Starting here is also the
    // only way the automatic pass can know there is new work.
    //
    // `pending` skips everything already done, so a rescan of a known library
    // costs nothing. The lock is released above first: `start_analysis` takes
    // it, and this mutex is not reentrant.
    let shared: Shared = Arc::clone(&state);
    start_analysis(&app_handle, &shared)?;

    Ok(report)
}

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

/// Which slice of the ordered result to actually send.
///
/// A window over the *flat* result, not over each section: grouping inserts
/// headings into one ordered list of rows, and that list is what the table
/// indexes by. A window per section would mean the caller could not turn a
/// scroll position into a row number.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowWindow {
    /// First row to send, counting from the start of the ordered result.
    #[serde(default)]
    offset: usize,
    /// How many rows to send. `None` is "the rest", which is what every
    /// caller that wants the whole thing sends by sending no window at all.
    #[serde(default)]
    limit: Option<usize>,
}

impl RowWindow {
    /// Rows `[start, end)` of the ordered result, both clamped to its length.
    ///
    /// Clamped rather than refused. A scroll position and a row count arrive
    /// from two different round trips, so a window past the end is an ordinary
    /// consequence of the library shrinking under a search, not a caller bug —
    /// and answering it with an error would put a red box on a table that is
    /// simply between reads.
    fn bounds(&self, total: usize) -> (usize, usize) {
        let start = self.offset.min(total);
        let end = match self.limit {
            Some(limit) => start.saturating_add(limit).min(total),
            None => total,
        };
        (start, end)
    }
}

/// One window of the library, and how long the library is.
///
/// The count is here rather than inferred from `sections` because the two are
/// no longer the same number: the scrollbar needs the extent of the whole
/// result and the table only ever holds a screenful of it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct LibraryPage {
    /// The requested window, still grouped. Sections that fall entirely
    /// outside it are absent, so a window is never padded with empty headings.
    sections: Vec<LibrarySection>,
    /// Rows matching the view before the window was applied — what the
    /// scrollbar measures and what the header counts.
    total: usize,
    /// Where `sections` starts in the ordered result, after clamping. The
    /// caller asked for a number; this is the number it got.
    offset: usize,
}

/// Filter, sort and group the library in one call, and send one window of it.
///
/// One round trip rather than three: the table re-runs this per keystroke, and
/// the predicates are the same ones a smart playlist uses, so splitting them
/// would let the two disagree.
///
/// The window is what AUD-13 added. Filtering, sorting and grouping stay here —
/// a caller that ordered its own window would order each window separately and
/// show a different sort per screenful, which is worse than the payload.
#[tauri::command]
pub fn library_view(
    view: LibraryView,
    window: Option<RowWindow>,
    state: State<'_, Shared>,
) -> Result<LibraryPage> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(library_page(&app, &view, &window.unwrap_or_default()))
}

/// The body of [`library_view`], reachable from a test.
///
/// A `#[tauri::command]` takes `State`, which cannot be built outside a running
/// app, so a window arithmetic bug left in a command body is a bug no test can
/// see.
pub(crate) fn library_page(app: &AppState, view: &LibraryView, window: &RowWindow) -> LibraryPage {
    let mut rows = resolved_rows(app, view);
    let total = rows.len();
    let (start, end) = window.bounds(total);

    // An empty window is a count, and a count does not need an order. Library
    // asks for one per keystroke to fill in "N tracks"; sorting and grouping
    // fifty thousand rows to answer it would be the whole cost of the old call
    // with none of the payload.
    if start == end {
        return LibraryPage {
            sections: Vec::new(),
            total,
            offset: start,
        };
    }

    if let Some(key) = view.sort_key.as_deref().and_then(parse_sort_key) {
        vapor_library::sort_rows(&mut rows, key, view.ascending);
    }

    let group = view
        .group_by
        .as_deref()
        .and_then(parse_group_by)
        .unwrap_or(GroupBy::None);

    // Walk the sections in order, keeping the overlap of each with the window.
    // `seen` is where the section begins in the flat result, which is the
    // number the caller's scroll position is in terms of.
    let mut seen = 0usize;
    let mut sections = Vec::new();
    for (header, rows) in vapor_library::group_rows(&rows, group) {
        let section_start = seen;
        seen += rows.len();
        let lo = start.max(section_start);
        let hi = end.min(seen);
        if lo >= hi {
            continue;
        }
        sections.push(LibrarySection {
            header,
            rows: rows[lo - section_start..hi - section_start]
                .iter()
                .map(|r| (*r).clone())
                .collect(),
        });
    }

    LibraryPage {
        sections,
        total,
        offset: start,
    }
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

/// One line of an opened album: what the release has, and whether we hold it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlbumTrack {
    /// Position on the release, counting from 1.
    ///
    /// From the order the service listed them in — the embedded tracklist
    /// carries no position field of its own, so array order is the only thing
    /// there is to go on. Zero for a track the library holds that the release
    /// does not list.
    pub position: u32,
    pub title: String,
    /// The file, or empty when this track is missing from the library.
    ///
    /// Empty is the whole point of this command: a row that cannot be played
    /// because it is not here, drawn so a person can see the shape of what
    /// they are missing rather than a short list that looks complete.
    pub href: String,
}

/// The full tracklist of an opened album, held and missing alike.
///
/// Returns nothing at all when the album was never matched to a release. That
/// is the ordinary state of an un-identified library, and an empty answer is
/// the signal for the screen to fall back to the plain table of what is there —
/// far better than inventing a tracklist out of the files to hand, which is
/// exactly the list that cannot show a gap.
#[tauri::command]
pub fn album_tracklist(
    album: String,
    lead: String,
    state: State<'_, Shared>,
) -> Result<Vec<AlbumTrack>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(album_tracklist_for(&app, &album, &lead))
}

/// The body of [`album_tracklist`], reachable from a test.
///
/// A `#[tauri::command]` takes `State`, which cannot be built outside a running
/// app, so logic left in a command body is logic no test can see — which is how
/// the Genres tab once shipped grouping by album.
pub(crate) fn album_tracklist_for(app: &AppState, album: &str, lead: &str) -> Vec<AlbumTrack> {
    // The same members the album's tile was built from, keyed the same way —
    // title *plus folder*, so two records sharing a name stay apart.
    // Spelled out rather than `..Default::default()`: `ascending` defaults to
    // *true* when deserialised and would be false from a derived `Default`, so
    // a struct-update here would quietly disagree with every other caller.
    let view = LibraryView {
        query: String::new(),
        sort_key: None,
        ascending: true,
        group_by: None,
        genre: None,
        album: Some(album.to_string()),
        artist: None,
    };
    let rows = resolved_rows(app, &view);
    let want = vapor_library::settings::album_key(album, lead);
    let members: Vec<&Row> = rows
        .iter()
        .filter(|r| vapor_library::settings::album_key(&r.album, &r.href) == want)
        .collect();

    let Some(facts) = release_of(app, &members) else {
        return Vec::new();
    };

    let mut used = vec![false; members.len()];
    let mut out: Vec<AlbumTrack> = facts
        .tracks
        .iter()
        .enumerate()
        .map(|(i, title)| {
            // First unused member whose title matches. "First unused" matters:
            // a release that lists the same title twice — a reprise, an intro
            // and its outro — would otherwise point both lines at one file and
            // report the second as missing.
            let found = members
                .iter()
                .enumerate()
                .position(|(n, r)| !used[n] && vapor_library::same_title(&r.title, title));
            let href = match found {
                Some(n) => {
                    used[n] = true;
                    members[n].href.clone()
                }
                None => String::new(),
            };
            AlbumTrack {
                position: i as u32 + 1,
                title: title.clone(),
                href,
            }
        })
        .collect();

    // Anything held that the release does not list, kept rather than hidden.
    // A bonus track, a different edition, or simply a title the comparison
    // could not match — all three are files that exist, and dropping them
    // would make the opened album unable to play music that is right there.
    for (n, row) in members.iter().enumerate() {
        if !used[n] {
            out.push(AlbumTrack {
                position: 0,
                title: row.title.clone(),
                href: row.href.clone(),
            });
        }
    }

    out
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
    // Resolved, not raw. This read the index row directly, so it showed the
    // path's idea of a track — none of the file's tags, none of the looked-up
    // album, and none of the owner's own corrections. The details sheet is the
    // one screen whose whole job is to say what the app thinks a track is.
    let mut row = row.clone();
    app.apply_metadata(&mut row);
    let row = &row;
    let fixed = app.settings.track_override(&href);
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
        // Through the same chain the rest of the app reads, so the sheet cannot
        // disagree with the Genres tab about what a track is.
        genre: genre_for_row(&app, row),
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
        genre_is_manual: fixed.is_some_and(|o| o.genre.is_some()),
        artist_is_manual: fixed.is_some_and(|o| o.artist.is_some()),
        album_is_manual: fixed.is_some_and(|o| o.album.is_some()),
    })
}

/// Correct what the app thinks a track is.
///
/// `field` is `"genre"`, `"album"` or `"artist"`; an empty `value` clears that
/// correction and hands the field back to whatever the app derives. Refused
/// rather than ignored for an unknown field, so a caller's typo surfaces as a
/// failed command instead of an edit that silently never applies.
///
/// The library is re-read afterwards by the screen, not here: this writes one
/// setting, and the rows every view is built from are derived on demand.
#[tauri::command]
pub fn set_track_override(
    href: String,
    field: String,
    value: String,
    state: State<'_, Shared>,
) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !app.settings.set_track_override(&href, &field, &value) {
        return Err(Error(format!(
            "Could not set {field}: it must be one of genre, album or artist, \
             and at most 200 characters."
        )));
    }
    app.save_settings()?;
    Ok(())
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
            app.apply_metadata(&mut row);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A real `AppState` on a throwaway directory, the same way `lib.rs`'s own
    /// tests build one. A counter rather than a timestamp in the name — macOS
    /// resolves the clock coarsely enough that two tests starting together
    /// collide.
    fn app() -> (AppState, std::path::PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "vapor-libview-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        (AppState::load(Store::new(dir.clone())), dir)
    }

    /// A library of `n` rows with the shape of a real one.
    ///
    /// Names, not `format!("{i}")`: what this measures is a JSON payload, and a
    /// payload of one-character strings would report a fifth of the truth.
    /// Roughly a hundred artists over a few hundred albums, which is what makes
    /// the grouped path do the work it does in the app.
    fn library(n: usize) -> Vec<Row> {
        const ARTISTS: &[&str] = &[
            "Nils Frahm",
            "Aphex Twin",
            "Boards of Canada",
            "Four Tet",
            "Floating Points",
            "Jon Hopkins",
            "Kaitlyn Aurelia Smith",
            "Caterina Barbieri",
        ];
        const GENRES: &[&str] = &["Ambient", "Electronic", "Techno", "Modern Classical"];
        const KEYS: &[&str] = &["8A", "8B", "9A", "9B", "10A", "10B", "11A", "11B"];
        (0..n)
            .map(|i| {
                let artist = ARTISTS[i % ARTISTS.len()];
                let album = format!("{artist} — Collected Works, Volume {}", i / 12 % 500);
                Row {
                    href: format!("/music/library/{artist}/{album}/{:03} track.flac", i % 12),
                    title: format!("A Title Of Roughly Ordinary Length {i}"),
                    artist: artist.to_string(),
                    album,
                    artist_source: vapor_library::index::Source::File,
                    album_source: vapor_library::index::Source::Folder,
                    genres: vec![GENRES[i % GENRES.len()].to_string()],
                    bpm: 60.0 + (i % 120) as f32,
                    key: KEYS[i % KEYS.len()].to_string(),
                    year: 1998 + (i % 28) as u32,
                    manual_pos: i,
                }
            })
            .collect()
    }

    /// A view built the way the frontend sends one — camelCase keys through
    /// serde — because `LibraryView` has no `Default` and because the keys are
    /// half of what this file gets wrong when it gets something wrong.
    fn view(json: serde_json::Value) -> LibraryView {
        serde_json::from_value(json).expect("a library view")
    }

    fn hrefs(page: &LibraryPage) -> Vec<&str> {
        page.sections
            .iter()
            .flat_map(|s| s.rows.iter().map(|r| r.href.as_str()))
            .collect()
    }

    /// No window is the whole library, which is what every caller that has not
    /// asked for one keeps getting.
    #[test]
    fn no_window_is_every_row() {
        let (mut app, _dir) = app();
        app.rows = library(40);
        let page = library_page(&app, &view(serde_json::json!({})), &RowWindow::default());
        assert_eq!(page.total, 40);
        assert_eq!(page.offset, 0);
        assert_eq!(hrefs(&page).len(), 40);
    }

    /// A window is a slice of the *ordered* result, so the rows in it are the
    /// same rows, in the same places, as the unwindowed call would have sent.
    ///
    /// This is the property the whole change rests on. A client that sorted its
    /// own window would satisfy every other test here and still show a
    /// different order per screenful.
    #[test]
    fn a_window_is_the_same_rows_the_whole_call_would_have_sent() {
        let (mut app, _dir) = app();
        app.rows = library(500);
        let view = view(serde_json::json!({ "sortKey": "bpm", "ascending": false }));
        let whole = library_page(&app, &view, &RowWindow::default());
        let all = hrefs(&whole);

        let window = library_page(
            &app,
            &view,
            &RowWindow {
                offset: 120,
                limit: Some(40),
            },
        );
        assert_eq!(
            window.total, 500,
            "the count is of the result, not the slice"
        );
        assert_eq!(window.offset, 120);
        assert_eq!(hrefs(&window), all[120..160], "offset 120, forty rows");
    }

    /// Grouping puts headings in one ordered list; a window cuts across it.
    ///
    /// The sections a window touches come back with their headers intact and
    /// their rows trimmed to the overlap. Sections outside it are absent rather
    /// than empty — an empty heading is a hole on the screen.
    #[test]
    fn a_window_cuts_across_groups_and_keeps_their_headers() {
        let (mut app, _dir) = app();
        app.rows = library(80);
        let view = view(serde_json::json!({ "groupBy": "artist" }));
        let whole = library_page(&app, &view, &RowWindow::default());
        assert_eq!(whole.sections.len(), 8, "eight artists");
        let all = hrefs(&whole);

        let window = library_page(
            &app,
            &view,
            &RowWindow {
                offset: 8,
                limit: Some(14),
            },
        );
        assert_eq!(hrefs(&window), all[8..22]);
        assert!(
            window.sections.len() < whole.sections.len(),
            "a fourteen-row window cannot span all eight groups"
        );
        assert!(
            window.sections.iter().all(|s| !s.rows.is_empty()),
            "no section arrives empty"
        );
        for section in &window.sections {
            let header = whole
                .sections
                .iter()
                .find(|s| s.header == section.header)
                .expect("the header a window keeps is one the whole call had");
            assert!(section.rows.len() <= header.rows.len());
        }
    }

    /// A window past the end is answered, not refused.
    ///
    /// The table's scroll position and its row count come from two different
    /// round trips, so this is what a library shrinking under a search looks
    /// like — ordinary, and not something to put a red box on screen for.
    #[test]
    fn a_window_past_the_end_comes_back_empty_with_the_real_count() {
        let (mut app, _dir) = app();
        app.rows = library(30);
        let page = library_page(
            &app,
            &view(serde_json::json!({})),
            &RowWindow {
                offset: 9_000,
                limit: Some(50),
            },
        );
        assert_eq!(page.total, 30, "the count is still the truth");
        assert_eq!(page.offset, 30, "clamped, so the caller can see it was");
        assert!(page.sections.is_empty());

        // Straddling the end returns the tail rather than nothing.
        let tail = library_page(
            &app,
            &view(serde_json::json!({})),
            &RowWindow {
                offset: 25,
                limit: Some(50),
            },
        );
        assert_eq!(hrefs(&tail).len(), 5);
    }

    /// A zero-row window is how the header asks "how many?".
    ///
    /// Library reads this per keystroke to fill in "N tracks · on this device",
    /// and it used to fetch every row to count them.
    #[test]
    fn a_zero_row_window_is_a_count() {
        let (mut app, _dir) = app();
        app.rows = library(200);
        let page = library_page(
            &app,
            &view(serde_json::json!({ "query": "Nils Frahm" })),
            &RowWindow {
                offset: 0,
                limit: Some(0),
            },
        );
        assert_eq!(page.total, 25, "a Nils Frahm row in every eight");
        assert!(page.sections.is_empty(), "and not one row of payload");
    }

    /// What AUD-13 is about, in bytes and milliseconds.
    ///
    /// Ignored by default: it builds fifty thousand rows and serialises them
    /// tens of times, which is not a thing to pay for on every `cargo test`.
    /// Run it, in release, when the numbers need re-checking:
    ///
    /// ```text
    /// cargo test --release --lib measure_the_library_view_payload -- --ignored --nocapture
    /// ```
    ///
    /// What it does *not* measure is the hop itself — the webview bridge and
    /// the `JSON.parse` on the other side of it, both of which scale with the
    /// same bytes. So the numbers below are the floor, not the bill.
    #[test]
    #[ignore = "a measurement, not a gate — see the doc comment"]
    fn measure_the_library_view_payload() {
        let (mut app, _dir) = app();
        app.rows = library(50_000);
        let view = view(serde_json::json!({ "sortKey": "title" }));

        /// Build and serialise the page eleven times; report the fastest.
        ///
        /// The fastest rather than the mean: every run does the same work, so
        /// the spread is the machine's — an allocator returning pages, another
        /// session's build on the same cores — and the floor is the only part
        /// of it that is about this code.
        fn measure(app: &AppState, view: &LibraryView, window: &RowWindow) -> (usize, f64) {
            let mut bytes = 0;
            let mut best = f64::MAX;
            for _ in 0..11 {
                let started = std::time::Instant::now();
                let page = library_page(app, view, window);
                let json = serde_json::to_vec(&page).expect("serialise");
                best = best.min(started.elapsed().as_secs_f64() * 1000.0);
                bytes = json.len();
            }
            (bytes, best)
        }

        // Before: what the command sent until now — every matching row, and no
        // count, because the rows *were* the count.
        let (before, before_ms) = measure(&app, &view, &RowWindow::default());
        // After: the window the virtualiser renders into, plus a block either
        // side of it, which is what `Songs.tsx` asks for.
        let (after, after_ms) = measure(
            &app,
            &view,
            &RowWindow {
                offset: 0,
                limit: Some(300),
            },
        );
        // And the same window a long way down, which is the one that has to
        // walk past everything above it.
        let (deep, deep_ms) = measure(
            &app,
            &view,
            &RowWindow {
                offset: 40_000,
                limit: Some(300),
            },
        );
        // And the count Library asks for per keystroke to fill in its header.
        let (count, count_ms) = measure(
            &app,
            &view,
            &RowWindow {
                offset: 0,
                limit: Some(0),
            },
        );

        // The floor under all four: `resolved_rows` clones every matching row
        // and applies tags and analysis to each. A window cannot skip it —
        // sorting by tempo needs the tempo of rows the window does not contain
        // — so this is what is left to fix after AUD-13, and it is most of the
        // time above.
        let mut resolve_ms = f64::MAX;
        for _ in 0..11 {
            let started = std::time::Instant::now();
            let resolved = resolved_rows(&app, &view);
            resolve_ms = resolve_ms.min(started.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(resolved.len(), 50_000);
        }

        println!("rows                    {}", 50_000);
        println!("resolve only      {:>10}         {resolve_ms:>7.2} ms", "-");
        println!("whole library     {before:>10} bytes  {before_ms:>7.2} ms");
        println!("300 rows at 0     {after:>10} bytes  {after_ms:>7.2} ms");
        println!("300 rows at 40k   {deep:>10} bytes  {deep_ms:>7.2} ms");
        println!("count only        {count:>10} bytes  {count_ms:>7.2} ms");
        println!(
            "a window is {:.2}% of the payload, a count {:.4}%",
            after as f64 / before as f64 * 100.0,
            count as f64 / before as f64 * 100.0,
        );

        assert!(after * 20 < before, "the point of the exercise");
    }

    /// An opened album shows the whole record, not just the files to hand.
    ///
    /// The thing a short list cannot do is show a gap. Holding 2 of 14 and
    /// drawing two rows looks like a complete album with two tracks on it; the
    /// point of this is that the other twelve are visible and greyed.
    #[test]
    fn an_opened_album_lists_the_tracks_it_is_missing() {
        let (mut app, dir) = app();
        app.albums.insert(
            302127,
            metadata::AlbumFacts {
                id: 302127,
                title: "Discovery".to_string(),
                artist: "Daft Punk".to_string(),
                record_type: "album".to_string(),
                nb_tracks: 4,
                tracks: vec![
                    "One More Time".to_string(),
                    "Aerodynamic".to_string(),
                    "Digital Love".to_string(),
                    "Harder, Better, Faster, Stronger".to_string(),
                ],
            },
        );

        // Two of the four, and each written the way a filename writes it —
        // one with a bracketed aside, one with the commas dropped.
        for (href, title) in [
            ("/Music/Daft Punk/Discovery/01.mp3", "One More Time"),
            (
                "/Music/Daft Punk/Discovery/04.mp3",
                "Harder Better Faster Stronger (Remaster)",
            ),
        ] {
            let mut r = Row {
                href: href.to_string(),
                title: title.to_string(),
                album: "Discovery".to_string(),
                album_source: vapor_library::index::Source::Folder,
                ..Default::default()
            };
            r.artist = "Daft Punk".to_string();
            r.artist_source = vapor_library::index::Source::Folder;
            app.rows.push(r);
            app.looked
                .entry(href.to_string())
                .or_default()
                .deezer_album_id = 302127;
        }

        let got = album_tracklist_for(&app, "Discovery", "/Music/Daft Punk/Discovery/01.mp3");

        assert_eq!(got.len(), 4, "the whole record, not just what is held");
        let titles: Vec<&str> = got.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            vec![
                "One More Time",
                "Aerodynamic",
                "Digital Love",
                "Harder, Better, Faster, Stronger"
            ],
            "album order, from the order the service listed them in"
        );

        // Held: matched despite the punctuation and the bracketed aside.
        assert_eq!(got[0].href, "/Music/Daft Punk/Discovery/01.mp3");
        assert_eq!(got[3].href, "/Music/Daft Punk/Discovery/04.mp3");
        // Missing: an empty href is what the screen greys out.
        assert!(got[1].href.is_empty(), "Aerodynamic is not in the library");
        assert!(got[2].href.is_empty(), "Digital Love is not in the library");
        assert_eq!(got[0].position, 1);
        assert_eq!(got[3].position, 4);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A release that lists one title twice does not point both lines at one
    /// file. An intro and its reprise are two tracks; matching greedily would
    /// claim the second was missing while showing the first twice.
    #[test]
    fn a_repeated_title_consumes_two_files_not_one() {
        let (mut app, dir) = app();
        app.albums.insert(
            5,
            metadata::AlbumFacts {
                id: 5,
                title: "Doubled".to_string(),
                artist: "An Artist".to_string(),
                record_type: "album".to_string(),
                nb_tracks: 2,
                tracks: vec!["Theme".to_string(), "Theme".to_string()],
            },
        );
        for href in ["/Music/A/Doubled/01.mp3", "/Music/A/Doubled/02.mp3"] {
            let mut r = Row {
                href: href.to_string(),
                title: "Theme".to_string(),
                album: "Doubled".to_string(),
                album_source: vapor_library::index::Source::Folder,
                ..Default::default()
            };
            r.artist = "An Artist".to_string();
            r.artist_source = vapor_library::index::Source::Folder;
            app.rows.push(r);
            app.looked
                .entry(href.to_string())
                .or_default()
                .deezer_album_id = 5;
        }

        let got = album_tracklist_for(&app, "Doubled", "/Music/A/Doubled/01.mp3");
        assert_eq!(got.len(), 2);
        assert!(!got[0].href.is_empty());
        assert!(
            !got[1].href.is_empty(),
            "the second Theme was reported missing while the first was used twice"
        );
        assert_ne!(got[0].href, got[1].href, "one file filled both lines");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// An un-identified album returns nothing, so the screen can fall back to
    /// the plain table rather than draw a tracklist invented from the files —
    /// which is exactly the list that cannot show a gap.
    #[test]
    fn an_album_that_was_never_looked_up_yields_no_tracklist() {
        let (mut app, dir) = app();
        let mut r = Row {
            href: "/Music/A/B/01.mp3".to_string(),
            title: "A Track".to_string(),
            album: "B".to_string(),
            album_source: vapor_library::index::Source::Folder,
            ..Default::default()
        };
        r.artist = "A".to_string();
        r.artist_source = vapor_library::index::Source::Folder;
        app.rows.push(r);

        assert!(album_tracklist_for(&app, "B", "/Music/A/B/01.mp3").is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }
}

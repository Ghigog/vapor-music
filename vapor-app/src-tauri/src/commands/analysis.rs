//! Analysis commands — running it, watching it, and correcting it.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// Correct a track's tempo by hand, or clear the correction with 0.
///
/// A refused value is an error rather than a silent no-op: the person is
/// looking at the number they just typed, and a correction that appeared to be
/// accepted but was not is worse than no correction at all.
///
/// The number is stored here and returns immediately; the beat grid it implies
/// is re-tracked in the background by [`retrack_after_correction`], because a
/// correction that only changed the label would leave mixing aligned to the
/// tempo it was made to reject.
#[tauri::command]
pub fn set_bpm_override(
    href: String,
    bpm: f32,
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !app.settings.set_bpm_override(&href, bpm) {
        return Err(Error(format!(
            "A BPM has to be between {} and {}.",
            vapor_library::MIN_MANUAL_BPM as u32,
            vapor_library::MAX_MANUAL_BPM as u32
        )));
    }
    app.save_settings()?;
    drop(app);

    retrack_grids(&app_handle, state.inner(), vec![href]);
    Ok(())
}

#[tauri::command]
pub fn analysis_status(state: State<'_, Shared>) -> Result<AnalysisStatus> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let (analysed, total) = analysis_counts(&app);
    Ok(AnalysisStatus {
        analysed,
        total,
        running: app.analysing,
        current: app.analysing_title.clone(),
        stopped_because: app.analysis_stopped_because.clone(),
    })
}

/// Every track analysis could not describe, and why.
///
/// Both kinds together, permanent first, because the person looking at this
/// asked "which ones didn't work" and does not yet know there are two kinds.
/// Within each kind, ordered by title so the list is the same twice running.
///
/// Joined against `rows` so the modal names tracks. A failure whose row has
/// gone — the file was removed from the library after it failed — keeps its
/// href as the title rather than being dropped: it is still a thing the count
/// is missing, and silently omitting it makes the numbers disagree.
#[tauri::command]
pub fn analysis_failures(state: State<'_, Shared>) -> Result<Vec<AnalysisFailure>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(failure_list(&app))
}

#[tauri::command]
pub fn cancel_analysis(state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.cancel.stop();
    // Reported stopped now, not when the pass gets round to noticing.
    //
    // The pass clears this itself on the way out and will do so again, which is
    // harmless. But it winds down asynchronously — it has a chunk of a download
    // to finish reading — and a screen that goes on saying "Listening…" until
    // then is a screen ignoring the button that was just pressed.
    app.analysing = false;
    app.analysing_title = String::new();
    Ok(())
}

/// Analyse everything not already done, emitting progress as it goes.
///
/// Runs on a blocking thread rather than the async runtime: analysis is
/// CPU-bound, and occupying an async worker for ten minutes starves every other
/// task sharing it.
#[tauri::command]
pub async fn analyse_library(app_handle: tauri::AppHandle, state: State<'_, Shared>) -> Result<()> {
    let shared: Shared = Arc::clone(&state);
    start_analysis(&app_handle, &shared)
}

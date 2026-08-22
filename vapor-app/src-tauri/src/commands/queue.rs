//! Queue commands — what is playing next, and reordering it.

use tauri::State;

use crate::{queue_view_for, Error, QueueState, QueueView, Result, Shared};

#[tauri::command]
pub fn queue_state(state: State<'_, Shared>) -> Result<QueueState> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(QueueState {
        current: app.queue.current().map(str::to_string),
        tracks: app.queue.tracks().to_vec(),
        next: app.queue.peek_next(None).map(str::to_string),
    })
}

#[tauri::command]
pub fn queue_view(state: State<'_, Shared>) -> Result<QueueView> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(queue_view_for(&app))
}

#[tauri::command]
pub fn remove_from_queue(href: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.queue.remove(&href))
}

#[tauri::command]
pub fn move_in_queue(from: usize, to: usize, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.queue.move_track(from, to))
}

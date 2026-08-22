//! Dynamic group commands — saved sets of artists, albums and genres.

use tauri::State;

use crate::{new_id, tracks_in_group, Error, Result, Row, Shared};

#[tauri::command]
pub fn dynamic_groups(state: State<'_, Shared>) -> Result<Vec<vapor_library::DynamicGroup>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.groups.all().to_vec())
}

#[tauri::command]
pub fn create_group(name: String, state: State<'_, Shared>) -> Result<vapor_library::DynamicGroup> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(Error("A group needs a name.".to_string()));
    }
    let id = new_id("group");
    let created = app.groups.create(id, name).clone();
    app.save_groups()?;
    Ok(created)
}

#[tauri::command]
pub fn rename_group(id: String, name: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(Error("A group needs a name.".to_string()));
    }
    let renamed = app.groups.rename(&id, name);
    if renamed {
        app.save_groups()?;
    }
    Ok(renamed)
}

#[tauri::command]
pub fn delete_group(id: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let Some(gone) = app.groups.delete(&id) else {
        return Ok(false);
    };
    // `group.rs` asks the caller to clear any cover override keyed to this id,
    // because a stale image outliving its group is what the GDScript's inline
    // call prevented. There is nothing to clear yet: overrides are keyed by
    // album and href, and a group has neither — it has no artwork of its own.
    // When it gets some, this is where letting go of it belongs.
    let _ = gone;
    app.save_groups()?;
    Ok(true)
}

/// Add an artist, album or genre to a group.
///
/// A track is refused rather than quietly turned into its album: a group holds
/// entities, and resolving one for the caller would make the set contain
/// something nobody put in it.
#[tauri::command]
pub fn add_to_group(
    id: String,
    entity_type: String,
    value: String,
    state: State<'_, Shared>,
) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let Some(kind) = vapor_library::EntityType::parse(&entity_type) else {
        return Err(Error(format!(
            "A dynamic group holds artists, albums and genres. \"{entity_type}\" is none of those."
        )));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(Error("There is nothing named here to add.".to_string()));
    }
    let added = app.groups.add_entity(&id, kind, value);
    if added {
        app.save_groups()?;
    }
    Ok(added)
}

#[tauri::command]
pub fn remove_from_group(
    id: String,
    entity_type: String,
    value: String,
    state: State<'_, Shared>,
) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let Some(kind) = vapor_library::EntityType::parse(&entity_type) else {
        return Ok(false);
    };
    let removed = app.groups.remove_entity(&id, kind, &value);
    if removed {
        app.save_groups()?;
    }
    Ok(removed)
}

#[tauri::command]
pub fn reorder_groups(from: usize, to: usize, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    let moved = app.groups.reorder(from, to);
    if moved {
        app.save_groups()?;
    }
    Ok(moved)
}

/// Every track a group currently resolves to.
///
/// Worked out on read rather than stored, which is the point of the feature: a
/// record added to the library after the group was made belongs to it without
/// anyone saying so.
#[tauri::command]
pub fn group_tracks(id: String, state: State<'_, Shared>) -> Result<Vec<Row>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let Some(group) = app.groups.get(&id) else {
        return Ok(Vec::new());
    };
    Ok(tracks_in_group(&app, group))
}

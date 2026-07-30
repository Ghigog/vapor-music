## playlist_folder_service.gd
## Autoload — persists Playlist Folders: a thin organizational layer over
## PlaylistService. A folder never owns tracks; a playlist just carries a
## folder_id field (see PlaylistService.set_playlist_folder) pointing at one
## of these. parent_id exists so nesting is representable without a future
## migration, but the UI only renders a single level for now.
extends Node

const FileUtil = preload("res://scripts/services/file_util.gd")

signal folder_created(folder: Dictionary)
signal folder_deleted(id: String)
signal folder_renamed(id: String, new_name: String)
signal folders_loaded()

## A var, not a const, so tests can point it at a scratch file (mirrors
## PlaylistService.playlist_file_path) instead of touching real user data.
var folders_file_path := "user://playlist_folders.json"

## Key: folder ID (String), Value: {id, name, parent_id}
var folders: Dictionary = {}


func _ready() -> void:
	load_folders()


func load_folders() -> void:
	folders = {}
	if FileAccess.file_exists(folders_file_path):
		var file := FileAccess.open(folders_file_path, FileAccess.READ)
		if file:
			var parsed = JSON.parse_string(file.get_as_text())
			file.close()
			if parsed is Dictionary:
				folders = parsed
	folders_loaded.emit()


func save_folders() -> void:
	FileUtil.write_string_atomic(folders_file_path, JSON.stringify(folders, "\t"))


## Named to match DynamicGroupService.get_dynamic_groups()'s precedent — avoids
## any ambiguity with Node's own scene-tree "groups" concept, even though
## "folders" itself doesn't collide with a native method.
func get_playlist_folders() -> Array:
	var result := []
	for key in folders:
		result.append(folders[key])
	return result


func get_folder(id: String) -> Dictionary:
	return folders.get(id, {})


func create_folder(name: String, parent_id: String = "") -> Dictionary:
	var id := "folder_" + str(Time.get_ticks_usec()) + "_" + str(randi() % 1000)
	var new_folder := {
		"id": id,
		"name": name,
		"parent_id": parent_id,
	}
	var new_dict := {id: new_folder}
	for key in folders:
		new_dict[key] = folders[key]
	folders = new_dict
	save_folders()
	folder_created.emit(new_folder)
	return new_folder


## Deleting a folder never deletes its playlists — PlaylistService un-files
## them back to the root (folder_id = "") first.
func delete_folder(id: String) -> void:
	if not folders.has(id):
		return
	if is_instance_valid(PlaylistService):
		for playlist in PlaylistService.get_playlists():
			if playlist.get("folder_id", "") == id:
				PlaylistService.set_playlist_folder(playlist.id, "")
	folders.erase(id)
	save_folders()
	folder_deleted.emit(id)


func rename_folder(id: String, new_name: String) -> void:
	if folders.has(id):
		folders[id]["name"] = new_name
		save_folders()
		folder_renamed.emit(id, new_name)

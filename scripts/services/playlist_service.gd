extends Node

const FileUtil = preload("res://scripts/services/file_util.gd")

signal playlist_created(playlist: Dictionary)
signal playlist_deleted(id: String)
signal playlist_renamed(id: String, new_name: String)
signal playlist_tracks_updated(id: String)
signal playlist_cover_updated(id: String, new_cover_path: String)
signal playlists_loaded()
signal active_playlist_changed(id: String)
signal playlist_folder_changed(id: String, folder_id: String)

var playlist_file_path = "user://playlists.json"

# Key: playlist ID (String), Value: playlist Dictionary
var playlists: Dictionary = {}
var active_playlist_id: String = "":
	set(val):
		if active_playlist_id != val:
			active_playlist_id = val
			active_playlist_changed.emit(val)

func _ready() -> void:
	load_playlists()

func load_playlists() -> void:
	playlists = {}
	if FileAccess.file_exists(playlist_file_path):
		var file = FileAccess.open(playlist_file_path, FileAccess.READ)
		if file:
			var content = file.get_as_text()
			file.close()
			var parsed = JSON.parse_string(content)
			if parsed is Dictionary:
				playlists = parsed
				playlists_loaded.emit()
				return
			elif parsed is Array:
				for p in parsed:
					if p is Dictionary and p.has("id"):
						playlists[p["id"]] = p
				playlists_loaded.emit()
				return
	playlists_loaded.emit()

func save_playlists() -> void:
	FileUtil.write_string_atomic(playlist_file_path, JSON.stringify(playlists, "\t"))

func get_playlists() -> Array:
	var result = []
	for key in playlists:
		result.append(playlists[key])
	return result

func get_playlist(id: String) -> Dictionary:
	if playlists.has(id):
		return playlists[id]
	return {}

func create_playlist(name: String, folder_id: String = "") -> Dictionary:
	var id = "playlist_" + str(Time.get_ticks_usec()) + "_" + str(randi() % 1000)
	var new_playlist = {
		"id": id,
		"name": name,
		"custom_cover_path": "",
		"tracks": [],
		"folder_id": folder_id
	}
	var new_dict = { id: new_playlist }
	for key in playlists:
		new_dict[key] = playlists[key]
	playlists = new_dict
	save_playlists()
	playlist_created.emit(new_playlist)
	return new_playlist

## Snapshot of a library view: the given hrefs, in the given order, as a
## normal (fully editable) playlist.
func create_snapshot_playlist(name: String, hrefs: Array) -> Dictionary:
	var playlist = create_playlist(name)
	playlist.tracks = hrefs.duplicate()
	save_playlists()
	playlist_tracks_updated.emit(playlist.id)
	return playlist

func delete_playlist(id: String) -> void:
	if playlists.has(id):
		var playlist = playlists[id]
		var cover_path = playlist.get("custom_cover_path", "")
		if not cover_path.is_empty() and cover_path.begins_with("user://playlist_images/"):
			if FileAccess.file_exists(cover_path):
				DirAccess.remove_absolute(cover_path)
		playlists.erase(id)
		save_playlists()
		playlist_deleted.emit(id)

func rename_playlist(id: String, new_name: String) -> void:
	if playlists.has(id):
		playlists[id]["name"] = new_name
		save_playlists()
		playlist_renamed.emit(id, new_name)

func add_track_to_playlist(playlist_id: String, track_href: String) -> void:
	var playlist = get_playlist(playlist_id)
	if not playlist.is_empty():
		playlist.tracks.append(track_href)
		save_playlists()
		playlist_tracks_updated.emit(playlist_id)

func add_track_to_playlist_at_index(playlist_id: String, track_href: String, index: int) -> void:
	var playlist = get_playlist(playlist_id)
	if not playlist.is_empty():
		if index >= 0 and index <= playlist.tracks.size():
			playlist.tracks.insert(index, track_href)
			save_playlists()
			playlist_tracks_updated.emit(playlist_id)

## Batched multi-track add for the table's bulk-select "Add to..." action.
## Unlike add_track_to_playlist, dedups against what's already in the
## playlist (looping the single-track method for N hrefs would both create
## duplicate entries and save+emit N times) and saves/emits exactly once.
func add_tracks_to_playlist(playlist_id: String, hrefs: Array) -> void:
	var playlist = get_playlist(playlist_id)
	if playlist.is_empty():
		return
	var existing: Dictionary = {}
	for href in playlist.tracks:
		existing[href] = true
	var added := false
	for href in hrefs:
		if not existing.has(href):
			playlist.tracks.append(href)
			existing[href] = true
			added = true
	if added:
		save_playlists()
		playlist_tracks_updated.emit(playlist_id)

func remove_track_from_playlist(id: String, index: int) -> void:
	if playlists.has(id):
		var tracks = playlists[id]["tracks"]
		if index >= 0 and index < tracks.size():
			tracks.remove_at(index)
			save_playlists()
			playlist_tracks_updated.emit(id)

## Batched multi-index remove for the table's bulk-select "Remove" action.
## Removes highest index first so earlier positions in the same batch never
## shift out from under a later removal — looping remove_track_from_playlist
## for N indices would both save N times and remove the WRONG tracks once
## the first removal shifted everything after it down by one.
func remove_tracks_from_playlist(id: String, indices: Array) -> void:
	if not playlists.has(id):
		return
	var tracks: Array = playlists[id]["tracks"]
	var sorted_indices: Array = indices.duplicate()
	sorted_indices.sort()
	sorted_indices.reverse()
	var removed := false
	for index in sorted_indices:
		if index >= 0 and index < tracks.size():
			tracks.remove_at(index)
			removed = true
	if removed:
		save_playlists()
		playlist_tracks_updated.emit(id)

func reorder_track_in_playlist(id: String, from_index: int, to_index: int) -> void:
	if playlists.has(id):
		var tracks = playlists[id]["tracks"]
		if from_index >= 0 and from_index < tracks.size() and to_index >= 0 and to_index < tracks.size():
			var track = tracks[from_index]
			tracks.remove_at(from_index)
			tracks.insert(to_index, track)
			save_playlists()
			playlist_tracks_updated.emit(id)

func set_playlist_custom_cover(id: String, source_image_path: String) -> void:
	if playlists.has(id):
		if source_image_path.is_empty():
			var old_cover = playlists[id].get("custom_cover_path", "")
			if not old_cover.is_empty() and old_cover.begins_with("user://playlist_images/"):
				if FileAccess.file_exists(old_cover):
					DirAccess.remove_absolute(old_cover)
			playlists[id]["custom_cover_path"] = ""
			save_playlists()
			playlist_cover_updated.emit(id, "")
			return
			
		var ext = source_image_path.get_extension().to_lower()
		if ext.is_empty() or ext.length() > 4:
			ext = "jpg"
			
		var dest_dir = "user://playlist_images/"
		if not DirAccess.dir_exists_absolute(dest_dir):
			DirAccess.make_dir_recursive_absolute(dest_dir)
			
		var hash = FileAccess.get_md5(source_image_path)
		if hash.is_empty():
			hash = str(Time.get_ticks_usec()).md5_text()
			
		var dest_path = dest_dir + hash + "." + ext
		var err = DirAccess.copy_absolute(source_image_path, dest_path)
		if err == OK:
			playlists[id]["custom_cover_path"] = dest_path
			save_playlists()
			playlist_cover_updated.emit(id, dest_path)
		else:
			print("PlaylistService Error: Failed to copy custom cover art from %s to %s (error: %d)" % [source_image_path, dest_path, err])

func get_playlist_cover_path(id: String) -> String:
	if playlists.has(id):
		var playlist = playlists[id]
		var custom_path = playlist.get("custom_cover_path", "")
		if not custom_path.is_empty():
			return custom_path
			
		var tracks = playlist.get("tracks", [])
		if not tracks.is_empty():
			var first_track = tracks[0]
			if is_instance_valid(MetadataService):
				var cached = MetadataService.get_cached_metadata(first_track)
				if not cached.is_empty():
					var art = cached.get("album_art_local", "")
					if not art.is_empty():
						return art
					art = cached.get("artist_image_local", "")
					if not art.is_empty():
						return art
	return ""

func reorder_playlists(from_index: int, to_index: int) -> void:
	var keys = playlists.keys()
	if from_index >= 0 and from_index < keys.size() and to_index >= 0 and to_index < keys.size():
		var key = keys[from_index]
		keys.remove_at(from_index)
		keys.insert(to_index, key)

		var new_dict = {}
		for k in keys:
			new_dict[k] = playlists[k]
		playlists = new_dict
		save_playlists()
		playlists_loaded.emit()

func set_playlist_folder(id: String, folder_id: String) -> void:
	if playlists.has(id) and playlists[id].get("folder_id", "") != folder_id:
		playlists[id]["folder_id"] = folder_id
		save_playlists()
		playlist_folder_changed.emit(id, folder_id)

## Id-based reorder: repositions playlist_id immediately before/after
## target_id in the master order. Unlike reorder_playlists' index pair, this
## stays correct no matter what subset of rows the UI is currently showing —
## once folders can filter the sidebar's playlist list, an on-screen row
## index no longer corresponds to an index into the full playlists dict, but
## an id always does.
func reorder_playlist_relative(playlist_id: String, target_id: String, place_before: bool) -> void:
	if playlist_id == target_id or not playlists.has(playlist_id) or not playlists.has(target_id):
		return
	var keys = playlists.keys()
	keys.erase(playlist_id)
	var idx = keys.find(target_id)
	if idx == -1:
		return
	if not place_before:
		idx += 1
	keys.insert(idx, playlist_id)
	var new_dict = {}
	for k in keys:
		new_dict[k] = playlists[k]
	playlists = new_dict
	save_playlists()
	playlists_loaded.emit()

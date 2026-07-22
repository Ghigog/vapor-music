## dynamic_group_service.gd
## Autoload — persists Dynamic Groups: named collections of ENTITIES (artists,
## albums, genres — not individual tracks). A group's track membership is
## never stored; it is resolved live at view time via TrackIndex.matches_entity,
## so a group stays current as the library changes. This is what distinguishes
## a dynamic group from a playlist: a playlist is a list of tracks (manual or
## frozen), a dynamic group is a list of entities whose matching tracks are
## always recomputed.
extends Node

signal group_created(group: Dictionary)
signal group_deleted(id: String)
signal group_renamed(id: String, new_name: String)
signal group_entities_updated(id: String)
signal groups_loaded()
signal active_group_changed(id: String)

const GROUPS_FILE := "user://dynamic_groups.json"

## Key: group ID (String), Value: {id, name, entities: [{type, value}]}
var groups: Dictionary = {}
var active_group_id: String = "":
	set(val):
		if active_group_id != val:
			active_group_id = val
			active_group_changed.emit(val)


func _ready() -> void:
	load_groups()


func load_groups() -> void:
	groups = {}
	if FileAccess.file_exists(GROUPS_FILE):
		var file := FileAccess.open(GROUPS_FILE, FileAccess.READ)
		if file:
			var parsed = JSON.parse_string(file.get_as_text())
			file.close()
			if parsed is Dictionary:
				groups = parsed
	groups_loaded.emit()


func save_groups() -> void:
	var file := FileAccess.open(GROUPS_FILE, FileAccess.WRITE)
	if file:
		file.store_string(JSON.stringify(groups, "\t"))
		file.close()


## Named to avoid colliding with Node's own native get_groups() (the "groups"
## a node belongs to in the SceneTree sense — an unrelated concept).
func get_dynamic_groups() -> Array:
	var result := []
	for key in groups:
		result.append(groups[key])
	return result


func get_group(id: String) -> Dictionary:
	return groups.get(id, {})


func create_group(name: String) -> Dictionary:
	var id := "group_" + str(Time.get_ticks_usec()) + "_" + str(randi() % 1000)
	var new_group := {
		"id": id,
		"name": name,
		"entities": [],
	}
	var new_dict := {id: new_group}
	for key in groups:
		new_dict[key] = groups[key]
	groups = new_dict
	save_groups()
	group_created.emit(new_group)
	return new_group


func delete_group(id: String) -> void:
	if groups.has(id):
		groups.erase(id)
		save_groups()
		# The group's custom cover (if any) lives in MetadataService's generic
		# override store, keyed "dynamic_group|<id>" — clear it so a future
		# group id can never inherit a stale image.
		if is_instance_valid(MetadataService):
			MetadataService.clear_custom_image("dynamic_group", id)
		group_deleted.emit(id)


func rename_group(id: String, new_name: String) -> void:
	if groups.has(id):
		groups[id]["name"] = new_name
		save_groups()
		group_renamed.emit(id, new_name)


func has_entity(id: String, entity_type: String, value: String) -> bool:
	for e in get_group(id).get("entities", []):
		if e.get("type") == entity_type and e.get("value") == value:
			return true
	return false


## Adding is idempotent — dragging the same artist onto a group twice is a
## no-op, not a duplicate row.
func add_entity(id: String, entity_type: String, value: String) -> void:
	if not groups.has(id) or value.is_empty():
		return
	if has_entity(id, entity_type, value):
		return
	groups[id]["entities"].append({"type": entity_type, "value": value})
	save_groups()
	group_entities_updated.emit(id)


func remove_entity(id: String, entity_type: String, value: String) -> void:
	if not groups.has(id):
		return
	var entities: Array = groups[id]["entities"]
	for i in entities.size():
		if entities[i].get("type") == entity_type and entities[i].get("value") == value:
			entities.remove_at(i)
			save_groups()
			group_entities_updated.emit(id)
			return


## Custom cover (via MetadataService's generic per-kind override store) if
## set, else a best-effort cover borrowed from the first artist/album entity's
## already-cached image — cache-only, no network fetch triggered from here.
## Genre entities have no natural image and fall through to "".
func get_group_cover_path(id: String) -> String:
	if is_instance_valid(MetadataService):
		var custom: String = MetadataService.get_custom_image("dynamic_group", id)
		if not custom.is_empty():
			return custom

	if not is_instance_valid(MetadataService):
		return ""
	for e in get_group(id).get("entities", []):
		var etype: String = e.get("type", "")
		var value: String = e.get("value", "")
		if etype != "artist" and etype != "album":
			continue
		var match_field := "artist_name" if etype == "artist" else "album_name"
		var art_field := "artist_image_local" if etype == "artist" else "album_art_local"
		for href in MetadataService.cache:
			var item = MetadataService.cache[href]
			if item is Dictionary and item.get(match_field, "") == value:
				var art: String = item.get(art_field, "")
				if not art.is_empty():
					return art
	return ""


func reorder_groups(from_index: int, to_index: int) -> void:
	var keys = groups.keys()
	if from_index >= 0 and from_index < keys.size() and to_index >= 0 and to_index < keys.size():
		var key = keys[from_index]
		keys.remove_at(from_index)
		keys.insert(to_index, key)
		var new_dict := {}
		for k in keys:
			new_dict[k] = groups[k]
		groups = new_dict
		save_groups()
		groups_loaded.emit()

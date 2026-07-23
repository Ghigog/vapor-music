## sidebar_folder_item.gd
## A folder header row in the sidebar's Playlists section. Click toggles the
## folder's own playlists open/closed; it is a drop target for filing a
## dragged playlist into this folder (one of the two "moved" meanings a drop
## can have here — the other, reordering, is handled by the playlist rows
## themselves via PlaylistService.reorder_playlist_relative).
extends Button

var folder_id: String = ""
var folder_name: String = ""

## ▲ / ▼ are already proven safe in this app's font stack (header sort
## arrows) — reused here rather than risking an unverified disclosure-triangle
## glyph falling back to a mismatched baseline (DESIGN_LANGUAGE §14.2).
const EXPANDED_GLYPH := "▼"
const COLLAPSED_GLYPH := "▲"


func _ready() -> void:
	mouse_exited.connect(func() -> void: modulate.a = 1.0)


func set_expanded(expanded: bool) -> void:
	var glyph = EXPANDED_GLYPH if expanded else COLLAPSED_GLYPH
	text = "    %s  %s" % [glyph, folder_name]


func _can_drop_data(_at_position: Vector2, data: Variant) -> bool:
	if data is Dictionary and data.get("type") == "playlist_reorder":
		modulate.a = 0.7
		return true
	return false


func _drop_data(_at_position: Vector2, data: Variant) -> void:
	modulate.a = 1.0
	if data.get("type") == "playlist_reorder":
		var playlist_id: String = data.get("playlist_id", "")
		if not playlist_id.is_empty():
			PlaylistService.set_playlist_folder(playlist_id, folder_id)

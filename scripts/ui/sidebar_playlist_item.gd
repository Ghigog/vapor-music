extends Button

const ICON_PLAYLIST_ITEM := preload("res://assets/icon/playlist-item-cropped.png")
const ICON_GRAB := preload("res://assets/icon/grab-cropped.png")

## The grab icon ships pointing the wrong way for a drag handle — rotated
## 90° clockwise here. Cached lazily since every row needs the same rotation.
static var _icon_grab_rotated_cache: ImageTexture = null

static func _icon_grab_rotated() -> ImageTexture:
	if _icon_grab_rotated_cache == null:
		var img := ICON_GRAB.get_image().duplicate()
		img.rotate_90(0) # 0 = clockwise. This build doesn't expose Image.ClockDirection to GDScript.
		_icon_grab_rotated_cache = ImageTexture.create_from_image(img)
	return _icon_grab_rotated_cache

var playlist_id: String = ""
var playlist_name: String = ""
## Which folder (if any) this row currently belongs to — "" for root. Set by
## sidebar._add_playlist_row from the playlist's own folder_id each rebuild.
var folder_id: String = ""

func _ready() -> void:
	# Hover never fires on touch, so there's no other cue that this row is
	# draggable there — show the grab icon permanently instead (§14.6).
	icon = _icon_grab_rotated() if PlatformManager.is_touch_primary() else ICON_PLAYLIST_ITEM
	if not PlatformManager.is_touch_primary():
		mouse_entered.connect(_on_mouse_entered)
		mouse_exited.connect(_on_mouse_exited)

func _on_mouse_entered() -> void:
	icon = _icon_grab_rotated()

func _on_mouse_exited() -> void:
	icon = ICON_PLAYLIST_ITEM

	var sidebar = get_tree().current_scene.find_child("Sidebar", true, false)
	if sidebar and sidebar.has_method("hide_drop_indicator"):
		sidebar.hide_drop_indicator()

func _get_drag_data(_at_position: Vector2) -> Variant:
	var data = {
		"type": "playlist_reorder",
		"playlist_id": playlist_id,
		"source_folder_id": folder_id,
		"source_item": self,
	}

	var preview = Label.new()
	preview.text = playlist_name
	preview.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	preview.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	preview.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)

	var container = PanelContainer.new()
	container.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_SM, 0.85))
	container.add_child(preview)
	set_drag_preview(container)

	return data

func _can_drop_data(at_position: Vector2, data: Variant) -> bool:
	if not (data is Dictionary and (data.get("type") == "track" or data.get("type") == "playlist_reorder")):
		return false

	if data.get("type") == "playlist_reorder":
		if data.get("playlist_id", "") == playlist_id:
			return false

		var is_above = at_position.y < size.y * 0.5
		var sidebar = get_tree().current_scene.find_child("Sidebar", true, false)
		if sidebar and sidebar.has_method("show_drop_indicator"):
			sidebar.show_drop_indicator(get_parent(), data.get("source_item"), self, is_above)
		# Read back at drop time — _can_drop_data fires continuously while
		# hovering, so this stays current with the pointer's last position.
		data["target_is_above"] = is_above

	return true

func _drop_data(_at_position: Vector2, data: Variant) -> void:
	var sidebar = get_tree().current_scene.find_child("Sidebar", true, false)
	if sidebar and sidebar.has_method("hide_drop_indicator"):
		sidebar.hide_drop_indicator()

	var type = data.get("type", "")
	if type == "track":
		var track_href = data.get("href", "")
		if not track_href.is_empty() and not playlist_id.is_empty():
			PlaylistService.add_track_to_playlist(playlist_id, track_href)
	elif type == "playlist_reorder":
		var src_id = data.get("playlist_id", "")
		if src_id != "" and src_id != playlist_id:
			var above: bool = data.get("target_is_above", true)
			PlaylistService.reorder_playlist_relative(src_id, playlist_id, above)
			# Dropping onto a row in a different scope (root vs. a folder, or
			# a different folder) also files the dragged playlist there — the
			# row you dropped it beside tells you which list it now belongs to.
			if data.get("source_folder_id", "") != folder_id:
				PlaylistService.set_playlist_folder(src_id, folder_id)

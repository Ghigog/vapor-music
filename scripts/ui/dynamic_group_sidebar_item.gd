extends Button
## dynamic_group_sidebar_item.gd
## Mirrors sidebar_playlist_item.gd for Dynamic Groups: drag-to-reorder among
## groups, and accepts a dragged library entity (an artist/album/genre group
## header) as a drop, adding it to this group. Never accepts a dragged TRACK —
## a dynamic group holds entities, not tracks; that distinction is the whole
## point of the feature.

const ICON_GROUP_ITEM := preload("res://assets/icon/group-item-cropped.png")
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

var group_id: String = ""
var group_name: String = ""

func _ready() -> void:
	icon = ICON_GROUP_ITEM
	mouse_entered.connect(_on_mouse_entered)
	mouse_exited.connect(_on_mouse_exited)

func _on_mouse_entered() -> void:
	icon = _icon_grab_rotated()

func _on_mouse_exited() -> void:
	icon = ICON_GROUP_ITEM

	var sidebar = get_tree().current_scene.find_child("Sidebar", true, false)
	if sidebar and sidebar.has_method("hide_group_drop_indicator"):
		sidebar.hide_group_drop_indicator()

func _get_drag_data(_at_position: Vector2) -> Variant:
	var data = {
		"type": "dynamic_group_reorder",
		"group_id": group_id,
		"original_source_index": get_index(),
	}

	var preview = Label.new()
	preview.text = group_name
	preview.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	preview.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	preview.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)

	var container = PanelContainer.new()
	container.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_SM, 0.85))
	container.add_child(preview)
	set_drag_preview(container)

	return data

func _can_drop_data(at_position: Vector2, data: Variant) -> bool:
	if data is Dictionary and data.get("type") == "dynamic_group_reorder":
		var source_idx = data.get("original_source_index", -1)
		var is_above = at_position.y < size.y * 0.5

		var sidebar = get_tree().current_scene.find_child("Sidebar", true, false)
		if sidebar and sidebar.has_method("show_group_drop_indicator"):
			sidebar.show_group_drop_indicator(source_idx, get_index(), is_above)

			var drop_idx = get_index()
			if not is_above:
				drop_idx += 1
			if source_idx < drop_idx:
				drop_idx -= 1
			data.target_drop_index = drop_idx

	return data is Dictionary and (data.get("type") == "entity" or data.get("type") == "dynamic_group_reorder")

func _drop_data(_at_position: Vector2, data: Variant) -> void:
	var sidebar = get_tree().current_scene.find_child("Sidebar", true, false)
	if sidebar and sidebar.has_method("hide_group_drop_indicator"):
		sidebar.hide_group_drop_indicator()

	var type = data.get("type", "")
	if type == "entity":
		var entity_type = data.get("entity_type", "")
		var value = data.get("value", "")
		if not entity_type.is_empty() and not value.is_empty() and not group_id.is_empty():
			DynamicGroupService.add_entity(group_id, entity_type, value)
	elif type == "dynamic_group_reorder":
		var orig_idx = data.get("original_source_index", -1)
		var final_idx = data.get("target_drop_index", -1)
		if orig_idx != -1 and final_idx != -1 and orig_idx != final_idx:
			DynamicGroupService.reorder_groups(orig_idx, final_idx)

extends Button

var playlist_id: String = ""
var playlist_name: String = ""

func _ready() -> void:
	mouse_entered.connect(_on_mouse_entered)
	mouse_exited.connect(_on_mouse_exited)

func _on_mouse_entered() -> void:
	text = "    ☰  " + playlist_name

func _on_mouse_exited() -> void:
	text = "    ♪  " + playlist_name
	
	var sidebar = get_tree().current_scene.find_child("Sidebar", true, false)
	if sidebar and sidebar.has_method("hide_drop_indicator"):
		sidebar.hide_drop_indicator()

func _get_drag_data(_at_position: Vector2) -> Variant:
	var data = {
		"type": "playlist_reorder",
		"playlist_id": playlist_id,
		"original_source_index": get_index(),
		"source_index": get_index()
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
	if data is Dictionary and data.get("type") == "playlist_reorder":
		var source_idx = data.get("original_source_index", -1)
		var is_above = at_position.y < size.y * 0.5
		
		var sidebar = get_tree().current_scene.find_child("Sidebar", true, false)
		if sidebar and sidebar.has_method("show_drop_indicator"):
			sidebar.show_drop_indicator(source_idx, get_index(), is_above)
			
			var drop_idx = get_index()
			if not is_above:
				drop_idx += 1
			if source_idx < drop_idx:
				drop_idx -= 1
			data.target_drop_index = drop_idx
			
	return data is Dictionary and (data.get("type") == "track" or data.get("type") == "playlist_reorder")

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
		var orig_idx = data.get("original_source_index", -1)
		var final_idx = data.get("target_drop_index", -1)
		if orig_idx != -1 and final_idx != -1 and orig_idx != final_idx:
			PlaylistService.reorder_playlists(orig_idx, final_idx)

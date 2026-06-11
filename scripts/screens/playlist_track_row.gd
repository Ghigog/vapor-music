extends PanelContainer

signal reorder_requested(source_idx: int, target_idx: int)
signal track_dropped_at(track_href: String, target_idx: int)
signal play_requested(index: int)
signal remove_requested(index: int)

var index: int = -1
var track_href: String = ""
var track_title: String = ""
var track_artist: String = ""

var remove_btn: Button
var title_label: Label
var artist_label: Label

func setup(p_index: int, p_href: String, p_title: String, p_artist: String) -> void:
	index = p_index
	track_href = p_href
	track_title = p_title
	track_artist = p_artist
	
	if title_label:
		title_label.text = track_title
	if artist_label:
		artist_label.text = track_artist

func _ready() -> void:
	var theme = ThemeManager.current_theme
	
	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 12)
	margin.add_theme_constant_override("margin_right", 12)
	margin.add_theme_constant_override("margin_top", 6)
	margin.add_theme_constant_override("margin_bottom", 6)
	margin.mouse_filter = Control.MOUSE_FILTER_PASS
	add_child(margin)
	
	var hbox = HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 16)
	hbox.mouse_filter = Control.MOUSE_FILTER_PASS
	margin.add_child(hbox)
	
	# Title / Artist info area
	var info_vbox = VBoxContainer.new()
	info_vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	info_vbox.mouse_filter = Control.MOUSE_FILTER_PASS
	hbox.add_child(info_vbox)
	
	title_label = Label.new()
	title_label.text = track_title
	title_label.add_theme_font_override("font", theme.font_ui)
	title_label.add_theme_font_size_override("font_size", theme.TYPE_SM)
	title_label.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	info_vbox.add_child(title_label)
	
	artist_label = Label.new()
	artist_label.text = track_artist
	artist_label.add_theme_font_override("font", theme.font_ui)
	artist_label.add_theme_font_size_override("font_size", theme.TYPE_XS)
	artist_label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	info_vbox.add_child(artist_label)
	
	# Remove button (shows on hover)
	remove_btn = Button.new()
	remove_btn.text = "✕"
	remove_btn.flat = true
	remove_btn.visible = false
	remove_btn.custom_minimum_size = Vector2(24, 24)
	remove_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	remove_btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
	remove_btn.add_theme_font_override("font", theme.font_ui)
	remove_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	remove_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	remove_btn.pressed.connect(func(): remove_requested.emit(index))
	hbox.add_child(remove_btn)
	
	# Connect hover signals
	mouse_entered.connect(_on_mouse_entered)
	mouse_exited.connect(_on_mouse_exited)
	
	# Style normal panel
	add_theme_stylebox_override("panel", ThemeManager.make_transparent())
	mouse_filter = Control.MOUSE_FILTER_PASS

func _on_mouse_entered() -> void:
	add_theme_stylebox_override("panel", ThemeManager.make_nav_item_hover())
	remove_btn.visible = true

func _on_mouse_exited() -> void:
	var local_m_pos = get_local_mouse_position()
	var rect = Rect2(Vector2.ZERO, size)
	if rect.has_point(local_m_pos):
		return # Hovering a child element like the remove button
		
	add_theme_stylebox_override("panel", ThemeManager.make_transparent())
	remove_btn.visible = false
	
	var node = get_parent()
	var screen = null
	while node:
		if node.name == "PlaylistScreen":
			screen = node
			break
		node = node.get_parent()
	if screen and screen.has_method("hide_drop_indicator"):
		screen.hide_drop_indicator()

func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		var local_m_pos = get_local_mouse_position()
		if not remove_btn.get_rect().has_point(local_m_pos):
			play_requested.emit(index)

func _get_drag_data(_at_position: Vector2) -> Variant:
	var data = {
		"type": "playlist_track_reorder",
		"original_source_index": index,
		"source_index": index
	}
	
	var preview = Label.new()
	preview.text = track_title
	preview.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	preview.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	preview.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	
	var container = PanelContainer.new()
	container.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_SM, 0.85))
	container.add_child(preview)
	set_drag_preview(container)
	
	return data

func _can_drop_data(at_position: Vector2, data: Variant) -> bool:
	if data is Dictionary and data.get("type") == "playlist_track_reorder":
		var source_idx = data.get("original_source_index", -1)
		var is_above = at_position.y < size.y * 0.5
		
		var node = get_parent()
		var screen = null
		while node:
			if node.name == "PlaylistScreen":
				screen = node
				break
			node = node.get_parent()
			
		if screen and screen.has_method("show_drop_indicator"):
			screen.show_drop_indicator(source_idx, index, is_above)
			var drop_idx = index
			if not is_above:
				drop_idx += 1
			if source_idx < drop_idx:
				drop_idx -= 1
			data.target_drop_index = drop_idx
			
	return data is Dictionary and (data.get("type") == "playlist_track_reorder" or data.get("type") == "track")

func _drop_data(_at_position: Vector2, data: Variant) -> void:
	var node = get_parent()
	var screen = null
	while node:
		if node.name == "PlaylistScreen":
			screen = node
			break
		node = node.get_parent()
		
	if screen and screen.has_method("hide_drop_indicator"):
		screen.hide_drop_indicator()
		
	if data.get("type") == "playlist_track_reorder":
		var orig_idx = data.get("original_source_index", -1)
		var final_idx = data.get("target_drop_index", -1)
		if orig_idx != -1 and final_idx != -1 and orig_idx != final_idx:
			reorder_requested.emit(orig_idx, final_idx)
	elif data.get("type") == "track":
		var href = data.get("href", "")
		if not href.is_empty():
			track_dropped_at.emit(href, index)

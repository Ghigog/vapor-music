extends PanelContainer

signal reorder_requested(source_idx: int, target_idx: int)
signal track_dropped_at(track_href: String, target_idx: int)
signal play_requested(index: int)
signal remove_requested(index: int)

var index: int = -1
var track_href: String = ""
var track_title: String = ""
var track_artist: String = ""

@onready var title_label: Label = $Margin/HBox/InfoVBox/TitleLabel
@onready var artist_label: Label = $Margin/HBox/InfoVBox/ArtistLabel
@onready var remove_btn: Button = $Margin/HBox/RemoveBtn


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
	
	title_label.text = track_title
	title_label.add_theme_font_override("font", theme.font_ui)
	title_label.add_theme_font_size_override("font_size", theme.TYPE_SM)
	title_label.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	
	artist_label.text = track_artist
	artist_label.add_theme_font_override("font", theme.font_ui)
	artist_label.add_theme_font_size_override("font_size", theme.TYPE_XS)
	artist_label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	
	# Hover-revealed on pointer input, but hover never fires on touch — always
	# visible there instead (§14.6, matches dynamic_group_screen.gd's entity
	# card remove button).
	remove_btn.visible = PlatformManager.is_touch_primary()
	# Scene bakes 24x24. Enforce the touch minimum on BOTH axes for icon buttons —
	# this one removes a track, so a mis-tap is destructive and worth the space.
	remove_btn.custom_minimum_size = ThemeManager.min_touch_size(remove_btn.custom_minimum_size)
	remove_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	remove_btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
	remove_btn.add_theme_font_override("font", theme.font_ui)
	remove_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	remove_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	
	if not remove_btn.pressed.is_connected(_on_remove_btn_pressed):
		remove_btn.pressed.connect(_on_remove_btn_pressed)
	
	# Connect hover signals
	if not mouse_entered.is_connected(_on_mouse_entered):
		mouse_entered.connect(_on_mouse_entered)
	if not mouse_exited.is_connected(_on_mouse_exited):
		mouse_exited.connect(_on_mouse_exited)
	
	# Style normal panel
	add_theme_stylebox_override("panel", ThemeManager.make_transparent())
	mouse_filter = Control.MOUSE_FILTER_PASS

func _on_remove_btn_pressed() -> void:
	remove_requested.emit(index)

func _on_mouse_entered() -> void:
	add_theme_stylebox_override("panel", ThemeManager.make_nav_item_hover())
	if not PlatformManager.is_touch_primary():
		remove_btn.visible = true

func _on_mouse_exited() -> void:
	var local_m_pos = get_local_mouse_position()
	var rect = Rect2(Vector2.ZERO, size)
	if rect.has_point(local_m_pos):
		return # Hovering a child element like the remove button

	add_theme_stylebox_override("panel", ThemeManager.make_transparent())
	if not PlatformManager.is_touch_primary():
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

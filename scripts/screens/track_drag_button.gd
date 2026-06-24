extends Button
## track_drag_button.gd
## Button that handles dragging library tracks.

var href: String = ""
var track_title: String = ""

func _get_drag_data(_at_position: Vector2) -> Variant:
	var data = {
		"type": "track",
		"href": href
	}
	
	var preview = Label.new()
	preview.text = "♫  " + track_title
	preview.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	preview.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	preview.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	
	var container = PanelContainer.new()
	container.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_SM, 0.85))
	container.add_child(preview)
	set_drag_preview(container)
	
	return data

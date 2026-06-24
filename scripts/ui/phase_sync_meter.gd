extends Control
# phase_sync_meter.gd
# Custom vector visualizer for real-time phase alignment between tracks.

var phase_error: float = 0.0:
	set(val):
		phase_error = val
		queue_redraw()

func _ready() -> void:
	resized.connect(queue_redraw)
	ThemeManager.theme_changed.connect(queue_redraw)

func get_indicator_ratio() -> float:
	if abs(phase_error) <= 5.0:
		return 0.5
	var clamped = clampf(phase_error, -50.0, 50.0)
	return (clamped + 50.0) / 100.0

func _draw() -> void:
	var theme = ThemeManager.current_theme
	if not theme:
		return
		
	var font_ui = theme.font_ui
	if not font_ui:
		return
		
	var W = size.x
	var H = size.y
	
	# Draw background box
	var bg_sb = ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.25)
	draw_style_box(bg_sb, Rect2(0, 0, W, H))
	
	# Draw horizontal track line
	var track_color = Color(theme.GLASS_BORDER_SUBTLE.r, theme.GLASS_BORDER_SUBTLE.g, theme.GLASS_BORDER_SUBTLE.b, 0.5)
	var track_y = H / 2.0
	draw_line(Vector2(15.0, track_y), Vector2(W - 15.0, track_y), track_color, 2.0)
	
	# Draw center marker (0ms)
	var marker_color = theme.TEXT_SECONDARY
	draw_line(Vector2(W / 2.0, track_y - 6.0), Vector2(W / 2.0, track_y + 6.0), marker_color, 2.0)
	
	# Determine color based on phase error
	var error_abs = abs(phase_error)
	var color: Color
	if error_abs <= 5.0:
		color = theme.SEMANTIC_SUCCESS
	elif error_abs <= 20.0:
		color = theme.SEMANTIC_WARNING
	else:
		color = theme.SEMANTIC_ERROR
		
	# Draw indicator position
	var ratio = get_indicator_ratio()
	var indicator_x = lerpf(15.0, W - 15.0, ratio)
	var indicator_y = track_y
	
	# Draw glow circle
	var glow_color = Color(color.r, color.g, color.b, 0.2)
	draw_circle(Vector2(indicator_x, indicator_y), 10.0, glow_color)
	draw_circle(Vector2(indicator_x, indicator_y), 5.0, color)
	
	# Draw text label in the bottom left
	var font_size = int(theme.TYPE_2XS)
	var label_text = ""
	if error_abs <= 5.0:
		label_text = "Phase Lock"
	else:
		label_text = "Phase Error: %+d ms" % int(phase_error)
		
	draw_string(font_ui, Vector2(12.0, H - 6.0), label_text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)

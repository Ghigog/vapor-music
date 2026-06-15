extends Control
# camelot_wheel.gd
# Custom vector visualizer for Camelot Wheel keys

var current_key: String = ""
var next_key: String = ""
var queued_keys: Array[String] = []

func _ready() -> void:
	# Trigger redraw on resize
	resized.connect(queue_redraw)
	ThemeManager.theme_changed.connect(queue_redraw)

func update_keys(curr: String, nxt: String, queued: Array[String]) -> void:
	current_key = curr
	next_key = nxt
	queued_keys = queued
	queue_redraw()

func _draw() -> void:
	var theme = ThemeManager.current_theme
	if not theme:
		return
		
	var font = theme.font_mono
	if not font:
		return
		
	var size_min = minf(size.x, size.y)
	var center = size / 2.0
	
	var r_outer = size_min / 2.0 - 12.0
	var r_mid = r_outer * 0.68
	var r_inner = r_outer * 0.38
	
	var font_size = int(theme.TYPE_2XS)
	
	# Mapping Camelot number to clock sectors: 12 is at the top (-PI/2)
	# Sector angle ranges: (N - 3.5) * PI/6 to (N - 2.5) * PI/6
	
	# Draw all 24 sectors
	for is_outer in [true, false]:
		var r1 = r_mid if is_outer else r_inner
		var r2 = r_outer if is_outer else r_mid
		var letter = "B" if is_outer else "A"
		
		for N in range(1, 13):
			var key_str = str(N) + letter
			
			var mid_angle = (N - 3.0) * (PI / 6.0)
			var start_angle = (N - 3.5) * (PI / 6.0)
			var end_angle = (N - 2.5) * (PI / 6.0)
			
			# Determine visual state/color
			var bg_color = theme.BG_ELEVATED
			bg_color.a = 0.35
			var border_color = theme.GLASS_BORDER_SUBTLE
			var border_width = 1.0
			var text_color = theme.TEXT_TERTIARY
			
			var is_current = (key_str == current_key)
			var is_next = (key_str == next_key)
			var is_queued = queued_keys.has(key_str)
			
			if is_current:
				bg_color = theme.ACCENT_SURFACE
				bg_color.a = 0.75
				border_color = theme.ACCENT_CORE
				border_width = 2.0
				text_color = theme.TEXT_PRIMARY
			elif is_next:
				bg_color = theme.AQUA_GLOW
				bg_color.a = 0.65
				border_color = theme.AQUA_CORE
				border_width = 2.0
				text_color = theme.TEXT_PRIMARY
			elif is_queued:
				bg_color = theme.GLASS_TINT
				bg_color.a = 0.35
				border_color = theme.GLASS_BORDER
				border_width = 1.0
				text_color = theme.TEXT_SECONDARY
				
			# Draw the annular sector polygon
			_draw_annular_sector(center, r1, r2, start_angle, end_angle, bg_color, true)
			_draw_annular_sector(center, r1, r2, start_angle, end_angle, border_color, false, border_width)
			
			# Draw Text label
			var text_radius = (r1 + r2) / 2.0
			var label_pos = center + Vector2(cos(mid_angle), sin(mid_angle)) * text_radius
			var label_size = font.get_string_size(key_str, HORIZONTAL_ALIGNMENT_CENTER, -1, font_size)
			var text_offset = Vector2(-label_size.x / 2.0, font.get_ascent(font_size) / 2.0 - label_size.y / 4.0)
			
			# Draw a highlight circle behind text if current/next to make it pop
			if is_current or is_next:
				draw_circle(label_pos, maxf(label_size.x, label_size.y) * 0.75, theme.BG_VOID)
				
			draw_string(font, label_pos + text_offset, key_str, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, text_color)
			
	# Draw mix connection path from current_key to next_key
	if not current_key.is_empty() and not next_key.is_empty() and current_key != next_key:
		var p_start = _get_key_center(center, current_key, r_inner, r_mid, r_outer)
		var p_end = _get_key_center(center, next_key, r_inner, r_mid, r_outer)
		
		# Draw thick bridge line
		draw_line(p_start, p_end, theme.AQUA_CORE, 3.0, true)
		
		# Draw start dot
		draw_circle(p_start, 4.0, theme.ACCENT_CORE)
		
		# Draw end arrow head
		var dir = (p_end - p_start).normalized()
		var p_left = p_end - dir * 10.0 + dir.rotated(PI/2.0 + PI/6.0) * 6.0
		var p_right = p_end - dir * 10.0 + dir.rotated(-PI/2.0 - PI/6.0) * 6.0
		var arrow_poly = PackedVector2Array([p_end, p_left, p_right])
		draw_polygon(arrow_poly, [theme.AQUA_CORE])

func _draw_annular_sector(center: Vector2, inner_radius: float, outer_radius: float, start_angle: float, end_angle: float, color: Color, fill: bool = true, width: float = 1.0) -> void:
	var points := PackedVector2Array()
	var steps := 24
	for i in range(steps + 1):
		var angle = lerp(start_angle, end_angle, float(i) / steps)
		points.append(center + Vector2(cos(angle), sin(angle)) * outer_radius)
	for i in range(steps, -1, -1):
		var angle = lerp(start_angle, end_angle, float(i) / steps)
		points.append(center + Vector2(cos(angle), sin(angle)) * inner_radius)
		
	if fill:
		draw_polygon(points, [color])
	else:
		points.append(points[0]) # Close path
		draw_polyline(points, color, width, true)

func _get_key_center(center: Vector2, key: String, r_inner: float, r_mid: float, r_outer: float) -> Vector2:
	if key.length() < 2:
		return center
	var letter = key[-1]
	var N = int(key.left(-1))
	var is_outer = (letter == "B")
	
	var r1 = r_mid if is_outer else r_inner
	var r2 = r_outer if is_outer else r_mid
	var text_radius = (r1 + r2) / 2.0
	
	var mid_angle = (N - 3.0) * (PI / 6.0)
	return center + Vector2(cos(mid_angle), sin(mid_angle)) * text_radius

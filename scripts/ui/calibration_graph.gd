extends Control
class_name CalibrationGraph

## Custom control to draw the corrective headphone EQ frequency response curve.

var preamp: float = 0.0
var bands: Array = []
var active: bool = false

# Layout margins
const MARGIN_LEFT = 40
const MARGIN_RIGHT = 20
const MARGIN_TOP = 20
const MARGIN_BOTTOM = 30

# Range limits
const FREQ_MIN = 20.0
const FREQ_MAX = 20000.0
const DB_MIN = -15.0
const DB_MAX = 15.0

# Sampling rate used for response calculation
const FS = 44100.0

# Grid line configurations
const GRID_FREQS = [20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0]
const GRID_DBS = [-12.0, -6.0, 0.0, 6.0, 12.0]

func _ready() -> void:
	custom_minimum_size = Vector2(0, 160)
	ThemeManager.theme_changed.connect(queue_redraw)

## Sets the profile dictionary containing preamp and bands list.
func set_profile(profile: Dictionary) -> void:
	preamp = profile.get("preamp", 0.0)
	bands = profile.get("bands", [])
	queue_redraw()

## Clears the graph profile.
func clear_profile() -> void:
	preamp = 0.0
	bands = []
	queue_redraw()

## Sets the active state of the calibration (greys out curve if bypassed).
func set_active(is_active: bool) -> void:
	active = is_active
	queue_redraw()

func _draw() -> void:
	var w = size.x - MARGIN_LEFT - MARGIN_RIGHT
	var h = size.y - MARGIN_TOP - MARGIN_BOTTOM
	
	if w <= 0 or h <= 0:
		return

	# Draw graph background
	var bg_rect = Rect2(MARGIN_LEFT, MARGIN_TOP, w, h)
	draw_rect(bg_rect, Color(0, 0, 0, 0.25), true)
	
	var grid_color = Color(1, 1, 1, 0.08)
	var text_color = ThemeManager.current_theme.TEXT_SECONDARY
	var font = ThemeManager.current_theme.font_ui
	var font_size = ThemeManager.current_theme.TYPE_2XS

	# Draw DB grid lines (horizontal)
	for db in GRID_DBS:
		var y = _db_to_y(db, h)
		draw_line(Vector2(MARGIN_LEFT, y), Vector2(MARGIN_LEFT + w, y), grid_color, 1.0)
		
		# Label
		var label = "%+d" % int(db) if db != 0 else "0"
		var text_sz = font.get_string_size(label, HORIZONTAL_ALIGNMENT_RIGHT, -1, font_size)
		draw_string(font, Vector2(MARGIN_LEFT - text_sz.x - 6, y + text_sz.y * 0.35), label, HORIZONTAL_ALIGNMENT_RIGHT, -1, font_size, text_color)

	# Draw Frequency grid lines (vertical)
	for freq in GRID_FREQS:
		var x = _freq_to_x(freq, w)
		draw_line(Vector2(x, MARGIN_TOP), Vector2(x, MARGIN_TOP + h), grid_color, 1.0)
		
		# Label
		var label = ""
		if freq >= 1000.0:
			label = "%dk" % int(freq / 1000.0)
		else:
			label = str(int(freq))
			
		var text_sz = font.get_string_size(label, HORIZONTAL_ALIGNMENT_CENTER, -1, font_size)
		draw_string(font, Vector2(x - text_sz.x / 2.0, MARGIN_TOP + h + text_sz.y + 4), label, HORIZONTAL_ALIGNMENT_CENTER, -1, font_size, text_color)

	# Draw border
	draw_rect(bg_rect, grid_color, false, 1.5)

	# Draw preamp line
	var preamp_y = _db_to_y(preamp, h)
	var preamp_color = Color(1, 1, 1, 0.4 if active else 0.15)
	_draw_dashed_line(Vector2(MARGIN_LEFT, preamp_y), Vector2(MARGIN_LEFT + w, preamp_y), preamp_color, 1.0, 4.0, 4.0)

	# Compute and draw response curve
	var curve_points = PackedVector2Array()
	var step_size = 2 # Draw every 2 pixels for performance and detail
	
	for px in range(0, int(w) + 1, step_size):
		var x = MARGIN_LEFT + px
		var freq = _x_to_freq(px, w)
		var db = _calculate_composite_db(freq)
		var y = _db_to_y(db, h)
		curve_points.append(Vector2(x, y))
		
	# Draw active / inactive curve
	if curve_points.size() > 1:
		var curve_color = ThemeManager.current_theme.ACCENT_CORE if active else Color(0.5, 0.5, 0.5, 0.3)
		# Draw a glowing backing line if active
		if active:
			var glow_color = Color(curve_color.r, curve_color.g, curve_color.b, 0.15)
			draw_polyline(curve_points, glow_color, 4.0)
		draw_polyline(curve_points, curve_color, 1.5)

# Coordinates Conversions
func _freq_to_x(freq: float, width: float) -> float:
	var log_min = log(FREQ_MIN) / log(10)
	var log_max = log(FREQ_MAX) / log(10)
	var log_f = log(clamp(freq, FREQ_MIN, FREQ_MAX)) / log(10)
	var pct = (log_f - log_min) / (log_max - log_min)
	return MARGIN_LEFT + pct * width

func _x_to_freq(px: float, width: float) -> float:
	var log_min = log(FREQ_MIN) / log(10)
	var log_max = log(FREQ_MAX) / log(10)
	var pct = px / width
	var log_f = log_min + pct * (log_max - log_min)
	return pow(10.0, log_f)

func _db_to_y(db: float, height: float) -> float:
	var pct = (db - DB_MIN) / (DB_MAX - DB_MIN)
	return MARGIN_TOP + height * (1.0 - pct)

# Biquad Magnitude Calculator
func _calculate_composite_db(f: float) -> float:
	var total_db = preamp
	for band in bands:
		if not band.get("enabled", true):
			continue
		total_db += _get_band_gain_db(f, band)
	return clamp(total_db, DB_MIN - 10.0, DB_MAX + 10.0)

func _get_band_gain_db(f: float, band: Dictionary) -> float:
	var f0 = band.get("frequency", 1000.0)
	var gain_db = band.get("gain", 0.0)
	var q = band.get("q", 1.0)
	var type = band.get("type", "peaking")
	
	if abs(gain_db) < 0.01:
		return 0.0
		
	var w0 = 2.0 * PI * f0 / FS
	var alpha = sin(w0) / (2.0 * q)
	var A = pow(10.0, gain_db / 40.0)
	
	var b0 = 0.0
	var b1 = 0.0
	var b2 = 0.0
	var a0 = 0.0
	var a1 = 0.0
	var a2 = 0.0
	
	match type:
		"peaking":
			b0 = 1.0 + alpha * A
			b1 = -2.0 * cos(w0)
			b2 = 1.0 - alpha * A
			a0 = 1.0 + alpha / A
			a1 = -2.0 * cos(w0)
			a2 = 1.0 - alpha / A
		"low_shelf":
			var cos_w0 = cos(w0)
			var sqrt_A = sqrt(A)
			b0 = A * ((A + 1.0) - (A - 1.0) * cos_w0 + 2.0 * sqrt_A * alpha)
			b1 = 2.0 * A * ((A - 1.0) - (A + 1.0) * cos_w0)
			b2 = A * ((A + 1.0) - (A - 1.0) * cos_w0 - 2.0 * sqrt_A * alpha)
			a0 = (A + 1.0) + (A - 1.0) * cos_w0 + 2.0 * sqrt_A * alpha
			a1 = -2.0 * ((A - 1.0) + (A + 1.0) * cos_w0)
			a2 = (A + 1.0) + (A - 1.0) * cos_w0 - 2.0 * sqrt_A * alpha
		"high_shelf":
			var cos_w0 = cos(w0)
			var sqrt_A = sqrt(A)
			b0 = A * ((A + 1.0) + (A - 1.0) * cos_w0 + 2.0 * sqrt_A * alpha)
			b1 = -2.0 * A * ((A - 1.0) + (A + 1.0) * cos_w0)
			b2 = A * ((A + 1.0) + (A - 1.0) * cos_w0 - 2.0 * sqrt_A * alpha)
			a0 = (A + 1.0) - (A - 1.0) * cos_w0 + 2.0 * sqrt_A * alpha
			a1 = 2.0 * ((A - 1.0) - (A + 1.0) * cos_w0)
			a2 = (A + 1.0) - (A - 1.0) * cos_w0 - 2.0 * sqrt_A * alpha
		"notch":
			b0 = 1.0
			b1 = -2.0 * cos(w0)
			b2 = 1.0
			a0 = 1.0 + alpha
			a1 = -2.0 * cos(w0)
			a2 = 1.0 - alpha
		"low_pass":
			b0 = (1.0 - cos(w0)) / 2.0
			b1 = 1.0 - cos(w0)
			b2 = (1.0 - cos(w0)) / 2.0
			a0 = 1.0 + alpha
			a1 = -2.0 * cos(w0)
			a2 = 1.0 - alpha
		"high_pass":
			b0 = (1.0 + cos(w0)) / 2.0
			b1 = -(1.0 + cos(w0))
			b2 = (1.0 + cos(w0)) / 2.0
			a0 = 1.0 + alpha
			a1 = -2.0 * cos(w0)
			a2 = 1.0 - alpha
		_:
			return 0.0
			
	var w = 2.0 * PI * f / FS
	var cos_w = cos(w)
	var cos_2w = cos(2.0 * w)
	var sin_w = sin(w)
	var sin_2w = sin(2.0 * w)
	
	var num_r = b0 + b1 * cos_w + b2 * cos_2w
	var num_i = -b1 * sin_w - b2 * sin_2w
	
	var den_r = a0 + a1 * cos_w + a2 * cos_2w
	var den_i = -a1 * sin_w - a2 * sin_2w
	
	var num_mag_sq = num_r * num_r + num_i * num_i
	var den_mag_sq = den_r * den_r + den_i * den_i
	
	if den_mag_sq < 1e-12:
		return 0.0
		
	var mag = sqrt(num_mag_sq / den_mag_sq)
	if mag < 1e-6:
		return -120.0
	return 20.0 * log(mag) / log(10.0)

# Helper to draw dashed lines
func _draw_dashed_line(from: Vector2, to: Vector2, color: Color, width: float, dash_length: float, gap_length: float) -> void:
	var dir = (to - from).normalized()
	var total_dist = from.distance_to(to)
	var dist = 0.0
	var draw = true
	
	while dist < total_dist:
		var step = dash_length if draw else gap_length
		if dist + step > total_dist:
			step = total_dist - dist
		if draw:
			draw_line(from + dir * dist, from + dir * (dist + step), color, width)
		dist += step
		draw = not draw

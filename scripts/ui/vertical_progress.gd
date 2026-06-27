## vertical_progress.gd
## Custom vertical progress bar running top-to-bottom.
## Drawn directly on the separator line.
extends Control

signal drag_started
signal drag_ended(value_changed: bool)
signal value_changed(new_value: float)

var value: float = 0.0:
	set(val):
		var old = value
		value = clampf(val, 0.0, max_value)
		if value != old:
			queue_redraw()

var max_value: float = 1.0:
	set(val):
		max_value = maxf(val, 1.0)
		queue_redraw()

var is_loading: bool = false:
	set(val):
		is_loading = val
		queue_redraw()
		set_process(is_loading)
		if is_loading:
			_loading_time = 0.0

var _track_loading := false
var _transitioning := false
var _dragging := false
var _hovered := false
var _loading_time := 0.0

var peaks: PackedFloat32Array = PackedFloat32Array()
var _current_track_href: String = ""
var _fetched_peaks_for_track: bool = false
var max_amplitude: float = 20.0

## When true the control is oriented horizontally (spine at y = h/2, time flows
## left → right). Used for the mobile/portrait seek bar above the mini-player.
var horizontal: bool = false:
	set(val):
		horizontal = val
		queue_redraw()


func _ready() -> void:
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	ThemeManager.theme_changed.connect(queue_redraw)
	mouse_entered.connect(func(): _hovered = true; queue_redraw())
	mouse_exited.connect(func(): _hovered = false; queue_redraw())
	
	AudioManager.loading_track.connect(func(loading):
		_track_loading = loading
		if loading:
			peaks = PackedFloat32Array()
			_current_track_href = ""
			_fetched_peaks_for_track = false
		_update_loading_state()
	)
	if AudioManager.has_signal("transition_started"):
		AudioManager.transition_started.connect(func(_next_track, _type):
			_transitioning = true
			_update_loading_state()
		)
	if AudioManager.has_signal("transition_completed"):
		AudioManager.transition_completed.connect(func(_track):
			_transitioning = false
			_update_loading_state()
		)
	AudioManager.track_changed.connect(func(_name):
		peaks = PackedFloat32Array()
		_current_track_href = ""
		_fetched_peaks_for_track = false
		queue_redraw()
	)
	set_process(false)


func _update_loading_state() -> void:
	is_loading = _track_loading or _transitioning


func _process(delta: float) -> void:
	if is_loading:
		_loading_time += delta
		queue_redraw()


func _smooth_peaks(raw: PackedFloat32Array) -> PackedFloat32Array:
	if raw.is_empty():
		return raw
	var smoothed = PackedFloat32Array()
	var size_raw = raw.size()
	smoothed.resize(size_raw)
	
	# Moving average window of 5 for nice curves with visible peaks/valleys
	var window = 5
	var half_win = window / 2
	for i in range(size_raw):
		var sum = 0.0
		var count = 0
		for j in range(i - half_win, i + half_win + 1):
			if j >= 0 and j < size_raw:
				sum += raw[j]
				count += 1
		smoothed[i] = sum / float(count)
	return smoothed


func _check_and_fetch_peaks() -> void:
	var active_player = AudioManager.active_player
	var has_track = is_instance_valid(active_player) and active_player.stream != null
	
	if not has_track:
		if not peaks.is_empty():
			peaks = PackedFloat32Array()
			_fetched_peaks_for_track = false
			queue_redraw()
		return
		
	if peaks.is_empty() and not is_loading and not _fetched_peaks_for_track:
		var dsp = AudioManager.dsp_a if active_player == AudioManager.player_a else AudioManager.dsp_b
		if dsp and dsp.get_cache_sample_count() > 0:
			_fetched_peaks_for_track = true
			var raw_peaks = dsp.get_waveform_peaks(150)
			if not raw_peaks.is_empty():
				peaks = _smooth_peaks(raw_peaks)
				queue_redraw()



func _draw() -> void:
	var theme: ThemeData = ThemeManager.current_theme
	var w: float = size.x
	var h: float = size.y
	
	# Check and fetch peaks for background waveform
	_check_and_fetch_peaks()
	
	if horizontal:
		# ----------------------------------------------------------------
		# Horizontal mode — spine at y = h/2, time flows left → right
		# ----------------------------------------------------------------
		var y_mid: float = h / 2.0
		var num_bins: int = peaks.size()
		
		# Waveform fill + outlines
		if num_bins > 0:
			var wave_color: Color = Color(theme.AQUA_CORE.r, theme.AQUA_CORE.g, theme.AQUA_CORE.b, 0.22)
			var outline_color: Color = Color(theme.AQUA_CORE.r, theme.AQUA_CORE.g, theme.AQUA_CORE.b, 0.30)
			
			var poly_pts: PackedVector2Array = PackedVector2Array()
			poly_pts.append(Vector2(0, y_mid))
			for i in range(num_bins):
				var x: float = float(i) / float(num_bins - 1) * w
				var offset: float = maxf(1.0, peaks[i] * max_amplitude)
				poly_pts.append(Vector2(x, y_mid - offset))
			poly_pts.append(Vector2(w, y_mid))
			for i in range(num_bins - 1, -1, -1):
				var x: float = float(i) / float(num_bins - 1) * w
				var offset: float = maxf(1.0, peaks[i] * max_amplitude)
				poly_pts.append(Vector2(x, y_mid + offset))
			draw_polygon(poly_pts, PackedColorArray([wave_color]))
			
			var top_pts: PackedVector2Array = PackedVector2Array()
			var bot_pts: PackedVector2Array = PackedVector2Array()
			for i in range(num_bins):
				var x: float = float(i) / float(num_bins - 1) * w
				var offset: float = maxf(1.0, peaks[i] * max_amplitude)
				top_pts.append(Vector2(x, y_mid - offset))
				bot_pts.append(Vector2(x, y_mid + offset))
			draw_polyline(top_pts, outline_color, 1.0, true)
			draw_polyline(bot_pts, outline_color, 1.0, true)
		
		# Track line
		var track_color: Color = Color(theme.GLASS_BORDER_SUBTLE.r, theme.GLASS_BORDER_SUBTLE.g, theme.GLASS_BORDER_SUBTLE.b, 0.25)
		draw_line(Vector2(0, y_mid), Vector2(w, y_mid), track_color, 1.0)
		
		# Transition pin
		if AudioManager.smart_mixing_enabled and AudioManager.has_method("get_transition_trigger_time"):
			var trigger_time: float = AudioManager.get_transition_trigger_time()
			if trigger_time > 0.0 and max_value > 0.0:
				var trigger_ratio: float = trigger_time / max_value
				if trigger_ratio < 1.0:
					var trigger_x: float = trigger_ratio * w
					var pin_color: Color = theme.ACCENT_CORE
					draw_line(Vector2(trigger_x, y_mid), Vector2(trigger_x, h + 4.0), pin_color, 2.0)
					draw_circle(Vector2(trigger_x, y_mid), 3.0, pin_color)
		
		if is_loading:
			var ping_pong: float = 0.5 + 0.5 * sin(_loading_time * (2.0 * PI / 1.5))
			var current_x: float = ping_pong * w
			draw_circle(Vector2(current_x, y_mid), 5.0, theme.AQUA_CORE)
		else:
			var ratio: float = 0.0
			if max_value > 0.0:
				ratio = value / max_value
			var current_x: float = ratio * w
			var fill_color: Color = theme.ACCENT_CORE
			if _hovered or _dragging:
				fill_color = theme.ACCENT_BRIGHT
			if current_x > 0.0:
				draw_line(Vector2(0, y_mid), Vector2(current_x, y_mid), fill_color, 2.0)
			var thumb_radius: float = 4.0
			if _dragging:   thumb_radius = 7.0
			elif _hovered:  thumb_radius = 6.0
			draw_circle(Vector2(current_x, y_mid), thumb_radius, fill_color)
		return
	
	# ----------------------------------------------------------------
	# Vertical mode (original behaviour, unchanged)
	# ----------------------------------------------------------------
	
	# Draw waveform in background if present (very subtle, simplified nice curves)
	if not peaks.is_empty():
		var x_mid: float = w / 2.0
		var num_bins: int = peaks.size()
		
		# Low-opacity color themes: 22% fill opacity, 30% outline opacity
		var wave_color: Color = Color(theme.AQUA_CORE.r, theme.AQUA_CORE.g, theme.AQUA_CORE.b, 0.22)
		var outline_color: Color = Color(theme.AQUA_CORE.r, theme.AQUA_CORE.g, theme.AQUA_CORE.b, 0.30)
		
		# 1. Build polygon for filled wave
		var poly_pts: PackedVector2Array = PackedVector2Array()
		poly_pts.append(Vector2(x_mid, 0))
		for i in range(num_bins):
			var y: float = float(i) / float(num_bins - 1) * h
			var offset: float = maxf(1.0, peaks[i] * max_amplitude)
			poly_pts.append(Vector2(x_mid + offset, y))
		poly_pts.append(Vector2(x_mid, h))
		for i in range(num_bins - 1, -1, -1):
			var y: float = float(i) / float(num_bins - 1) * h
			var offset: float = maxf(1.0, peaks[i] * max_amplitude)
			poly_pts.append(Vector2(x_mid - offset, y))
			
		draw_polygon(poly_pts, PackedColorArray([wave_color]))
		
		# 2. Draw left and right outlines using polyline for curves
		var left_pts: PackedVector2Array = PackedVector2Array()
		var right_pts: PackedVector2Array = PackedVector2Array()
		for i in range(num_bins):
			var y: float = float(i) / float(num_bins - 1) * h
			var offset: float = maxf(1.0, peaks[i] * max_amplitude)
			left_pts.append(Vector2(x_mid - offset, y))
			right_pts.append(Vector2(x_mid + offset, y))
			
		draw_polyline(left_pts, outline_color, 1.0, true)
		draw_polyline(right_pts, outline_color, 1.0, true)
	
	# Draw the track line (very subtle)
	var track_color: Color = Color(theme.GLASS_BORDER_SUBTLE.r, theme.GLASS_BORDER_SUBTLE.g, theme.GLASS_BORDER_SUBTLE.b, 0.25)
	draw_line(Vector2(w / 2.0, 0), Vector2(w / 2.0, h), track_color, 1.0)
	
	# Draw the transition pin/dot if smart mixing is enabled
	if AudioManager.smart_mixing_enabled and AudioManager.has_method("get_transition_trigger_time"):
		var trigger_time: float = AudioManager.get_transition_trigger_time()
		if trigger_time > 0.0 and max_value > 0.0:
			var trigger_ratio: float = trigger_time / max_value
			if trigger_ratio < 1.0:
				var trigger_y: float = trigger_ratio * h
				var pin_color: Color = theme.ACCENT_CORE
				# Draw a line from seeker into the timeline/content
				draw_line(Vector2(w / 2.0, trigger_y), Vector2(w + 4.0, trigger_y), pin_color, 2.0)
				# Draw a small dot
				draw_circle(Vector2(w / 2.0, trigger_y), 3.0, pin_color)
	
	if is_loading:
		# Draw the loading spot (ball/pill) moving up and down
		# Period of 1.5 seconds for a full round trip
		var ping_pong: float = 0.5 + 0.5 * sin(_loading_time * (2.0 * PI / 1.5))
		var current_y: float = ping_pong * h
		var fill_color: Color = theme.AQUA_CORE
		
		# Draw the loading spot
		var thumb_radius: float = 5.0
		draw_circle(Vector2(w / 2.0, current_y), thumb_radius, fill_color)
	else:
		# Draw the filled progress line (ACCENT_CORE) from top (0) to current position
		var ratio: float = 0.0
		if max_value > 0.0:
			ratio = value / max_value
		
		var current_y: float = ratio * h
		var fill_color: Color = theme.ACCENT_CORE
		if _hovered or _dragging:
			fill_color = theme.ACCENT_BRIGHT
			
		if current_y > 0.0:
			draw_line(Vector2(w / 2.0, 0), Vector2(w / 2.0, current_y), fill_color, 2.0)
			
		# Draw the grabber circle thumb
		var thumb_radius: float = 4.0
		if _dragging:
			thumb_radius = 7.0
		elif _hovered:
			thumb_radius = 6.0
			
		draw_circle(Vector2(w / 2.0, current_y), thumb_radius, fill_color)


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				_dragging = true
				drag_started.emit()
				if horizontal:
					_update_value_from_pos_h(event.position.x)
				else:
					_update_value_from_pos(event.position.y)
				get_viewport().set_input_as_handled()
			else:
				if _dragging:
					_dragging = false
					drag_ended.emit(true)
					get_viewport().set_input_as_handled()
					queue_redraw()
					
	elif event is InputEventMouseMotion and _dragging:
		if horizontal:
			_update_value_from_pos_h(event.position.x)
		else:
			_update_value_from_pos(event.position.y)
		get_viewport().set_input_as_handled()


func _update_value_from_pos(y_pos: float) -> void:
	var ratio: float = clampf(y_pos / size.y, 0.0, 1.0)
	value = ratio * max_value
	value_changed.emit(value)


func _update_value_from_pos_h(x_pos: float) -> void:
	var ratio: float = clampf(x_pos / size.x, 0.0, 1.0)
	value = ratio * max_value
	value_changed.emit(value)

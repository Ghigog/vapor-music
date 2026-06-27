extends Control
class_name WaveformVisualizer
# waveform_visualizer.gd
# Custom Audacity-style frosted waveform downsampler & visualizer.

var peaks: PackedFloat32Array = PackedFloat32Array()
var playback_position: float = 0.0
var duration: float = 0.0
var playhead_ratio: float = 0.0:
	set(val):
		playhead_ratio = clampf(val, 0.0, 1.0)
		queue_redraw()

var _cached_bg_sb: StyleBoxFlat = null

func _ready() -> void:
	resized.connect(queue_redraw)
	ThemeManager.theme_changed.connect(func():
		_cached_bg_sb = null
		queue_redraw()
	)

func set_peaks(new_peaks: PackedFloat32Array) -> void:
	peaks = new_peaks
	queue_redraw()

func clear_peaks() -> void:
	peaks = PackedFloat32Array()
	playback_position = 0.0
	duration = 0.0
	playhead_ratio = 0.0
	queue_redraw()

func set_playback_state(pos: float, dur: float) -> void:
	playback_position = pos
	duration = dur
	if duration > 0.0:
		playhead_ratio = playback_position / duration
	else:
		playhead_ratio = 0.0

func _draw() -> void:
	var theme = ThemeManager.current_theme
	if not theme:
		return
		
	var W = size.x
	var H = size.y
	var center_y = H / 2.0
	
	# Draw glass panel background
	if not _cached_bg_sb:
		_cached_bg_sb = ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.15)
	draw_style_box(_cached_bg_sb, Rect2(0, 0, W, H))

	if peaks.is_empty():
		# Draw simple horizontal line indicating standby
		var standby_color = Color(theme.GLASS_BORDER_SUBTLE.r, theme.GLASS_BORDER_SUBTLE.g, theme.GLASS_BORDER_SUBTLE.b, 0.4)
		draw_line(Vector2(12.0, center_y), Vector2(W - 12.0, center_y), standby_color, 2.0)
		return

	var aqua_color = theme.AQUA_CORE
	var glass_color = Color(aqua_color.r, aqua_color.g, aqua_color.b, 0.25)
	var active_color = aqua_color
	var playhead_color = theme.ACCENT_CORE

	var num_bins = peaks.size()
	# Leave 12px margin on left and right for aesthetics (within glass card)
	var margin = 12.0
	var drawable_width = W - (margin * 2.0)
	var bin_width = drawable_width / float(num_bins)
	var playhead_x = margin + (playhead_ratio * drawable_width)

	for i in range(num_bins):
		var x = margin + (i + 0.5) * bin_width
		var val = peaks[i]
		
		# Balanced double-sided waveform, using 85% of half-height for margins
		var half_len = val * (H / 2.0) * 0.85
		var start = Vector2(x, center_y - half_len)
		var end = Vector2(x, center_y + half_len)
		
		# Color is active if to the left of the playhead, glass if to the right
		var col = active_color if x <= playhead_x else glass_color
		var line_w = maxf(1.0, bin_width - 0.5)
		draw_line(start, end, col, line_w)
		
	# Draw playhead indicator line
	draw_line(Vector2(playhead_x, 2.0), Vector2(playhead_x, H - 2.0), playhead_color, 2.0)

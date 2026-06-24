extends Control
# transition_timeline.gd
# Custom vector visualizer for the transition crossover timeline

var outgoing_title: String = "No Track"
var incoming_title: String = "No Track"
var outgoing_bpm: float = 0.0
var incoming_bpm: float = 0.0
var transition_duration: float = 0.0
var transition_type: String = "Standard Crossfade"

var is_transitioning: bool = false
var crossfader_val: float = 0.0

var playback_pos: float = 0.0
var track_length: float = 0.0
var trigger_time: float = 0.0

var outgoing_peaks: PackedFloat32Array = PackedFloat32Array()
var incoming_peaks: PackedFloat32Array = PackedFloat32Array()
var outgoing_duration: float = 0.0
var incoming_duration: float = 0.0
var incoming_cue_in: float = 0.0

func _ready() -> void:
	resized.connect(queue_redraw)
	ThemeManager.theme_changed.connect(queue_redraw)

func update_transition_info(out_t: String, in_t: String, out_b: float, in_b: float, dur: float, type: String) -> void:
	outgoing_title = out_t
	incoming_title = in_t
	outgoing_bpm = out_b
	incoming_bpm = in_b
	transition_duration = dur
	transition_type = type
	queue_redraw()

func update_playback_state(trans: bool, fader: float, pos: float, length: float, trig: float) -> void:
	is_transitioning = trans
	crossfader_val = fader
	playback_pos = pos
	track_length = length
	trigger_time = trig
	queue_redraw()

func _draw() -> void:
	var theme = ThemeManager.current_theme
	if not theme:
		return
		
	var font_disp = theme.font_display
	var font_ui = theme.font_ui
	var font_mono = theme.font_mono
	if not font_ui or not font_mono:
		return
		
	var W = size.x
	var H = size.y
	
	# Fallback duration if not set
	var out_dur = outgoing_duration if outgoing_duration > 0.0 else (track_length if track_length > 0.0 else 180.0)
	var in_dur = incoming_duration if incoming_duration > 0.0 else 180.0
	
	# Determine scale: entire W corresponds to out_dur
	var scale = out_dur / W if W > 0 else 1.0
	
	var trig = trigger_time
	if trig <= 0.0:
		trig = out_dur - transition_duration
		if trig < 0.0: trig = out_dur * 0.8
		
	var crossover_start = (trig / out_dur) * W
	var crossover_end = ((trig + transition_duration) / out_dur) * W
	crossover_start = clamp(crossover_start, 0.0, W)
	crossover_end = clamp(crossover_end, crossover_start, W)
	
	# Draw timeline background box
	var bg_sb = ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.25)
	draw_style_box(bg_sb, Rect2(0, 0, W, H))
	
	# Outgoing Track Box (Top Half) - Full Width
	var deck_a_color = theme.ACCENT_SURFACE
	deck_a_color.a = 0.15
	draw_rect(Rect2(0, 4, W, H / 2.0 - 6.0), deck_a_color, true)
	draw_rect(Rect2(0, 4, W, H / 2.0 - 6.0), theme.GLASS_BORDER_SUBTLE, false, 1.0)
	
	# Incoming Track Box (Bottom Half) - Starts at transition trigger point
	var deck_b_color = theme.AQUA_GLOW
	deck_b_color.a = 0.1
	draw_rect(Rect2(crossover_start, H / 2.0 + 2.0, W - crossover_start, H / 2.0 - 6.0), deck_b_color, true)
	draw_rect(Rect2(crossover_start, H / 2.0 + 2.0, W - crossover_start, H / 2.0 - 6.0), theme.GLASS_BORDER_SUBTLE, false, 1.0)
	
	# Highlight the transition crossover zone (background fill)
	var crossover_color = theme.ACCENT_CORE
	crossover_color.a = 0.06
	draw_rect(Rect2(crossover_start, 4, crossover_end - crossover_start, H - 8.0), crossover_color, true)
	
	# Draw the running playback cursor
	var cursor_x := -1.0
	var cursor_lbl := ""
	
	cursor_x = (playback_pos / out_dur) * W if out_dur > 0.0 else 0.0
	cursor_x = clamp(cursor_x, 0.0, W)
	
	if is_transitioning:
		cursor_lbl = "Blending (%d%%)" % int(crossfader_val * 100.0)
	else:
		if playback_pos >= trig - 5.0 and trig > 0.0:
			cursor_lbl = "Ready"
		else:
			cursor_lbl = "Playing"
			
	# Draw Outgoing Waveform (Top half) - Across full width
	if not outgoing_peaks.is_empty() and out_dur > 0.0:
		var box_h = H / 2.0 - 6.0
		var center_y = 4.0 + box_h / 2.0
		var bar_w = 1.0
		
		var active_col = theme.ACCENT_CORE
		var inactive_col = Color(active_col.r, active_col.g, active_col.b, 0.25)
		
		for x_pixel in range(0, int(W)):
			var t = (float(x_pixel) / W) * out_dur
			var idx = int((t / out_dur) * outgoing_peaks.size())
			idx = clamp(idx, 0, outgoing_peaks.size() - 1)
			var val = outgoing_peaks[idx]
			
			var half_len = val * (box_h / 2.0) * 0.8
			var start = Vector2(x_pixel, center_y - half_len)
			var end = Vector2(x_pixel, center_y + half_len)
			
			var col = active_col if x_pixel <= cursor_x else inactive_col
			draw_line(start, end, col, bar_w)
			
	# Draw Incoming Waveform (Bottom half) - Starts at crossover_start
	if not incoming_peaks.is_empty() and in_dur > 0.0:
		var box_h = H / 2.0 - 6.0
		var center_y = (H / 2.0 + 2.0) + box_h / 2.0
		var bar_w = 1.0
		
		var active_col = theme.AQUA_CORE
		var inactive_col = Color(active_col.r, active_col.g, active_col.b, 0.25)
		
		for x_pixel in range(int(crossover_start), int(W)):
			var t_outgoing = (float(x_pixel) / W) * out_dur
			var t_incoming = incoming_cue_in + (t_outgoing - trig)
			if t_incoming >= 0.0 and t_incoming <= in_dur:
				var idx = int((t_incoming / in_dur) * incoming_peaks.size())
				idx = clamp(idx, 0, incoming_peaks.size() - 1)
				var val = incoming_peaks[idx]
				
				var half_len = val * (box_h / 2.0) * 0.8
				var start = Vector2(x_pixel, center_y - half_len)
				var end = Vector2(x_pixel, center_y + half_len)
				
				var col = active_col if (is_transitioning and x_pixel <= cursor_x) else inactive_col
				draw_line(start, end, col, bar_w)
				
	# Draw Transition Start vertical dashed line
	if crossover_start > 0.0 and crossover_start < W:
		var trans_col = Color(theme.ACCENT_CORE.r, theme.ACCENT_CORE.g, theme.ACCENT_CORE.b, 0.5)
		draw_line(Vector2(crossover_start, 0), Vector2(crossover_start, H), trans_col, 1.0)
		
	# Draw Track Labels
	var font_size_title = int(theme.TYPE_XS)
	var font_size_detail = int(theme.TYPE_2XS)
	
	var max_lbl_w = W * 0.25
	
	var out_lbl_full = "OUTGOING: " + outgoing_title.get_file().uri_decode().get_basename()
	var out_lbl = _truncate_text(font_ui, out_lbl_full, max_lbl_w, font_size_title)
	draw_string(font_ui, Vector2(12, H * 0.25), out_lbl, HORIZONTAL_ALIGNMENT_LEFT, max_lbl_w, font_size_title, theme.TEXT_PRIMARY)
	if outgoing_bpm > 0.0:
		var out_bpm_lbl = "%d BPM" % int(outgoing_bpm)
		draw_string(font_mono, Vector2(12, H * 0.4), out_bpm_lbl, HORIZONTAL_ALIGNMENT_LEFT, max_lbl_w, font_size_detail, theme.TEXT_SECONDARY)
		
	var in_lbl_full = "INCOMING: " + incoming_title.get_file().uri_decode().get_basename()
	var in_lbl = _truncate_text(font_ui, in_lbl_full, max_lbl_w, font_size_title)
	draw_string(font_ui, Vector2(W - 12 - max_lbl_w, H * 0.72), in_lbl, HORIZONTAL_ALIGNMENT_RIGHT, max_lbl_w, font_size_title, theme.TEXT_PRIMARY)
	if incoming_bpm > 0.0:
		var in_bpm_lbl = "%d BPM" % int(incoming_bpm)
		draw_string(font_mono, Vector2(W - 12 - max_lbl_w, H * 0.88), in_bpm_lbl, HORIZONTAL_ALIGNMENT_RIGHT, max_lbl_w, font_size_detail, theme.TEXT_SECONDARY)
		
	# Draw crossfade volume envelope lines inside the crossover zone
	if transition_duration > 0.0 and out_dur > 0.0 and (crossover_end > crossover_start):
		var pts_a = PackedVector2Array()
		var pts_b = PackedVector2Array()
		
		var steps := 20
		for i in range(steps + 1):
			var t = float(i) / steps
			var x_pos = lerpf(crossover_start, crossover_end, t)
			
			# Outgoing volume curve (smooth cosine shape)
			var vol_a = cos(t * PI / 2.0)
			var y_a = lerpf(H / 2.0 - 10.0, 8.0, vol_a)
			pts_a.append(Vector2(x_pos, y_a))
			
			# Incoming volume curve (smooth sine shape)
			var vol_b = sin(t * PI / 2.0)
			var y_b = lerpf(H - 8.0, H / 2.0 + 6.0, vol_b)
			pts_b.append(Vector2(x_pos, y_b))
			
		draw_polyline(pts_a, theme.ACCENT_DIM, 2.0, true)
		draw_polyline(pts_b, theme.AQUA_CORE, 2.0, true)
		
	# Draw BPM offset and transition duration labels in the middle of crossover
	var mid_x = (crossover_start + crossover_end) / 2.0
	var offset_y = H / 2.0
	
	var dur_lbl = "Crossover: %.1fs" % transition_duration
	var dur_sz = font_ui.get_string_size(dur_lbl, HORIZONTAL_ALIGNMENT_CENTER, -1, font_size_detail)
	draw_rect(Rect2(mid_x - dur_sz.x / 2.0 - 4.0, offset_y - dur_sz.y - 12.0, dur_sz.x + 8.0, dur_sz.y + 4.0), theme.BG_VOID, true)
	draw_string(font_ui, Vector2(mid_x - dur_sz.x / 2.0, offset_y - 12.0), dur_lbl, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size_detail, theme.TEXT_SECONDARY)
	
	if outgoing_bpm > 0.0 and incoming_bpm > 0.0:
		var bpm_diff = incoming_bpm - outgoing_bpm
		var percent = (bpm_diff / outgoing_bpm) * 100.0
		var bpm_lbl = "%+d BPM (%+.1f%%)" % [int(bpm_diff), percent]
		if abs(percent) < 0.05:
			bpm_lbl = "BPM Sync: Matches"
		var bpm_sz = font_mono.get_string_size(bpm_lbl, HORIZONTAL_ALIGNMENT_CENTER, -1, font_size_detail)
		draw_rect(Rect2(mid_x - bpm_sz.x / 2.0 - 4.0, offset_y + 8.0, bpm_sz.x + 8.0, bpm_sz.y + 4.0), theme.BG_VOID, true)
		var bpm_color = theme.TEXT_TERTIARY if abs(percent) < 0.05 else (theme.SEMANTIC_WARNING if percent > 0.0 else theme.AQUA_CORE)
		draw_string(font_mono, Vector2(mid_x - bpm_sz.x / 2.0, offset_y + 8.0 + font_mono.get_ascent(font_size_detail)), bpm_lbl, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size_detail, bpm_color)
		
	if cursor_x > 0.0:
		# Draw cursor vertical line
		draw_line(Vector2(cursor_x, 0), Vector2(cursor_x, H), theme.ACCENT_BRIGHT, 2.0)
		draw_circle(Vector2(cursor_x, H / 2.0), 5.0, theme.ACCENT_BRIGHT)
		
		# Draw cursor label bubble
		var cur_sz = font_mono.get_string_size(cursor_lbl, HORIZONTAL_ALIGNMENT_CENTER, -1, font_size_detail)
		var bub_rect = Rect2(cursor_x - cur_sz.x / 2.0 - 6.0, 6.0, cur_sz.x + 12.0, cur_sz.y + 2.0)
		draw_rect(bub_rect, theme.ACCENT_CORE, true)
		draw_string(font_mono, Vector2(cursor_x - cur_sz.x / 2.0, 6.0 + font_mono.get_ascent(font_size_detail)), cursor_lbl, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size_detail, theme.TEXT_INVERSE)


func _truncate_text(font: Font, text: String, max_width: float, font_size: int) -> String:
	if font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x <= max_width:
		return text
		
	var ellipsis = "..."
	var ellipsis_w = font.get_string_size(ellipsis, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
	if ellipsis_w >= max_width:
		return ""
		
	var truncated = text
	while truncated.length() > 0 and font.get_string_size(truncated + ellipsis, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x > max_width:
		truncated = truncated.left(truncated.length() - 1)
		
	return truncated + ellipsis

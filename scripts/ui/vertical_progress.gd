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

var _dragging := false
var _hovered := false

func _ready() -> void:
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	ThemeManager.theme_changed.connect(queue_redraw)
	mouse_entered.connect(func(): _hovered = true; queue_redraw())
	mouse_exited.connect(func(): _hovered = false; queue_redraw())


func _draw() -> void:
	var theme = ThemeManager.current_theme
	var w = size.x
	var h = size.y
	
	# Draw the track line (very subtle)
	var track_color = Color(theme.GLASS_BORDER_SUBTLE.r, theme.GLASS_BORDER_SUBTLE.g, theme.GLASS_BORDER_SUBTLE.b, 0.25)
	draw_line(Vector2(w / 2.0, 0), Vector2(w / 2.0, h), track_color, 1.0)
	
	# Draw the filled progress line (ACCENT_CORE) from top (0) to current position
	var ratio = 0.0
	if max_value > 0.0:
		ratio = value / max_value
	
	var current_y = ratio * h
	var fill_color = theme.ACCENT_CORE
	if _hovered or _dragging:
		fill_color = theme.ACCENT_BRIGHT
		
	if current_y > 0.0:
		draw_line(Vector2(w / 2.0, 0), Vector2(w / 2.0, current_y), fill_color, 2.0)
		
	# Draw the grabber circle thumb
	var thumb_radius = 4.0
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
				_update_value_from_pos(event.position.y)
				get_viewport().set_input_as_handled()
			else:
				if _dragging:
					_dragging = false
					drag_ended.emit(true)
					get_viewport().set_input_as_handled()
					queue_redraw()
					
	elif event is InputEventMouseMotion and _dragging:
		_update_value_from_pos(event.position.y)
		get_viewport().set_input_as_handled()


func _update_value_from_pos(y_pos: float) -> void:
	var ratio = clampf(y_pos / size.y, 0.0, 1.0)
	value = ratio * max_value
	value_changed.emit(value)

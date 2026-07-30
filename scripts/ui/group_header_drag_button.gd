extends Button
const ICON_GROUP := preload("res://assets/icon/group-cropped.png")
## group_header_drag_button.gd
## Attached to EVERY library table group header row, not just draggable ones —
## entity_type/entity_value stay empty for the unknown-bucket/GROUP_NONE case,
## and every gesture below is a no-op when they're empty. Where they're set,
## the whole group — e.g. "Björk" or "Dance" — can be dragged onto a sidebar
## Dynamic Group, or long-pressed/right-clicked to open the same "Add to
## Group" picker the drag produces. Distinct from track_drag_button: the
## payload is an entity reference (type + value), not a track href.
##
## Callers must invoke _setup_gestures() explicitly after set_script(), since
## _ready() is unreliable for a script attached at runtime to an in-tree node.

var entity_type: String = ""
var entity_value: String = ""

## See track_drag_button.gd's identical field — same reason.
var suppress_next_click := false

signal long_pressed(at_position: Vector2)
signal right_clicked(at_position: Vector2)

const LONG_PRESS_SEC := 0.5
const MOVE_CANCEL_PX := 10.0

var _press_timer: Timer
var _press_start_pos := Vector2.ZERO


func _setup_gestures() -> void:
	_press_timer = Timer.new()
	_press_timer.one_shot = true
	_press_timer.wait_time = LONG_PRESS_SEC
	add_child(_press_timer)
	_press_timer.timeout.connect(_on_long_press_timeout)
	gui_input.connect(_on_gesture_gui_input)


func _on_gesture_gui_input(event: InputEvent) -> void:
	if entity_type.is_empty() or entity_value.is_empty():
		return
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_RIGHT and event.pressed:
			right_clicked.emit(get_global_mouse_position())
		elif event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				_press_start_pos = event.position
				_press_timer.start()
			else:
				_press_timer.stop()
	elif event is InputEventMouseMotion and not _press_timer.is_stopped():
		if event.position.distance_to(_press_start_pos) > MOVE_CANCEL_PX:
			_press_timer.stop()


func _on_long_press_timeout() -> void:
	suppress_next_click = true
	long_pressed.emit(get_global_mouse_position())


func _get_drag_data(_at_position: Vector2) -> Variant:
	if _press_timer:
		_press_timer.stop()
	if entity_type.is_empty() or entity_value.is_empty():
		return null
	var data = {
		"type": "entity",
		"entity_type": entity_type,
		"value": entity_value,
	}

	var preview_icon = TextureRect.new()
	preview_icon.texture = ICON_GROUP
	preview_icon.custom_minimum_size = Vector2(16, 16)
	preview_icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	preview_icon.self_modulate = ThemeManager.current_theme.TEXT_PRIMARY

	var preview_label = Label.new()
	preview_label.text = entity_value
	preview_label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	preview_label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	preview_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)

	var preview_hbox = HBoxContainer.new()
	preview_hbox.add_theme_constant_override("separation", 6)
	preview_hbox.add_child(preview_icon)
	preview_hbox.add_child(preview_label)

	var container = PanelContainer.new()
	container.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_SM, 0.85))
	container.add_child(preview_hbox)
	set_drag_preview(container)

	return data

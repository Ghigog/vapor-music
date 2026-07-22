extends Button
## track_drag_button.gd
## Button that handles dragging library tracks, plus the two touch/desktop
## equivalents of "drop this on a playlist": long-press (mobile/touch) and
## right-click (desktop) both open the "Add to Playlist" picker. See
## _setup_gestures() — callers must invoke it explicitly after set_script(),
## since _ready() is unreliable for a script attached at runtime to a node
## already in the tree.

var href: String = ""
var track_title: String = ""

## Set true the instant a long-press fires, so the eventual release doesn't
## ALSO register as a normal click (which would start playback right as the
## picker opens). Callers must check-and-clear this at the top of their own
## `pressed` handler.
var suppress_next_click := false

signal long_pressed(at_position: Vector2)
signal right_clicked(at_position: Vector2)

const LONG_PRESS_SEC := 0.5
## Movement past this (px) while held reads as a drag attempt, not a long
## press — the two gestures are mutually exclusive by how far the pointer
## travels, not by racing each other.
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
	# A real drag starting is itself proof this wasn't a long press.
	if _press_timer:
		_press_timer.stop()
	var data = {
		"type": "track",
		"href": href
	}
	# In a manual-mode TrackTable, rows also carry their position and table
	# identity so a drop within the same table becomes a reorder while the
	# sidebar's add-to-playlist drop keeps working off type/href alone.
	var extras = get_meta("drag_extras", {})
	for key in extras:
		data[key] = extras[key]
	
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


## Drop handling exists only for rows inside a manual-mode TrackTable, which
## registers itself in the "drop_table" meta. Library rows have no table meta
## and accept nothing.
func _can_drop_data(at_position: Vector2, data: Variant) -> bool:
	var table = get_meta("drop_table", null)
	if table == null or not is_instance_valid(table):
		return false
	if not table.can_row_accept_drop(data):
		return false
	table.preview_row_drop(self, at_position, data)
	return true


func _drop_data(at_position: Vector2, data: Variant) -> void:
	var table = get_meta("drop_table", null)
	if table == null or not is_instance_valid(table):
		return
	table.handle_row_drop(self, at_position, data)

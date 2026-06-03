## main.gd
## Root scene controller.
##
## Responsibilities:
##   - Applies the void background colour from ThemeManager.
##   - Reads PlatformManager to choose between sidebar (desktop) and
##     floating tab-bar (mobile) navigation models.
##   - Listens to PlatformManager.layout_changed and reflows the layout
##     when the window is resized across a breakpoint.
##   - Listens to NavManager.navigation_requested and swaps the visible screen.

extends Control


# ---------------------------------------------------------------------------
# Scene references — paths must match scenes/main.tscn hierarchy.
# ---------------------------------------------------------------------------

@onready var background:       ColorRect = $Background
@onready var app_window_frame: PanelContainer = $AppWindowFrame
@onready var sidebar:          Control   = $AppWindowFrame/LayoutRoot/Sidebar
@onready var content_frame:    Control   = $AppWindowFrame/LayoutRoot/ContentFrame
@onready var screen_container: Control   = $AppWindowFrame/LayoutRoot/ContentFrame/ScreenContainer
@onready var mini_player:      Control   = $AppWindowFrame/LayoutRoot/MiniPlayer
@onready var setup_wizard:     Control   = $SetupWizard


# Screens — all present in the scene tree; only one is visible at a time.
@onready var library_screen:  Control = $AppWindowFrame/LayoutRoot/ContentFrame/ScreenContainer/LibraryScreen
@onready var search_screen:   Control = $AppWindowFrame/LayoutRoot/ContentFrame/ScreenContainer/SearchScreen
@onready var settings_screen: Control = $AppWindowFrame/LayoutRoot/ContentFrame/ScreenContainer/SettingsScreen

## Maps screen name strings to their scene-tree Control nodes.
var _screens: Dictionary = {}

# Window resizing variables
const RESIZE_BORDER = 12
const MIN_WINDOW_SIZE = Vector2i(380, 400)


var _is_resizing := false
var _resize_mode := ""
var _start_mouse_pos := Vector2i()
var _start_window_position := Vector2i()
var _start_window_size := Vector2i()

@onready var vertical_progress: Control = $AppWindowFrame/LayoutRoot/VerticalProgress



# ---------------------------------------------------------------------------
# Lifecycle
# ---------------------------------------------------------------------------

var _vp_dragging := false
var _custom_window_stylebox: StyleBoxFlat = null

func _ready() -> void:
	get_window().min_size = MIN_WINDOW_SIZE
	if app_window_frame.has_theme_stylebox_override("panel"):
		var sb = app_window_frame.get_theme_stylebox("panel")
		if sb is StyleBoxFlat:
			_custom_window_stylebox = sb.duplicate()
			app_window_frame.add_theme_stylebox_override("panel", _custom_window_stylebox)

	_register_screens()
	_apply_background()
	app_window_frame.resized.connect(func():
		var mat = app_window_frame.material as ShaderMaterial
		if mat:
			mat.set_shader_parameter("container_size", app_window_frame.size)
	)
	get_tree().process_frame.connect(func():
		var mat = app_window_frame.material as ShaderMaterial
		if mat:
			mat.set_shader_parameter("container_size", app_window_frame.size)
	, CONNECT_ONE_SHOT)
	_connect_signals()
	_connect_vp_signals()
	if PlatformManager.is_desktop():
		_create_resize_handles()
	_update_layout()
	# Show the default screen without pushing history.
	_show_screen(NavManager.current_screen)
	_check_setup()

func _connect_vp_signals() -> void:
	if not vertical_progress:
		return
	vertical_progress.drag_started.connect(func(): _vp_dragging = true)
	vertical_progress.drag_ended.connect(func(_val_changed):
		_vp_dragging = false
		AudioManager.scroll_track(vertical_progress.value)
	)
	vertical_progress.value_changed.connect(func(new_val):
		if _vp_dragging:
			# If dragging, optionally update audio position or let drag_ended handle it.
			pass
	)
	AudioManager.track_changed.connect(func(_name):
		vertical_progress.max_value = AudioManager.current_track_length
		vertical_progress.value = 0.0
	)
	AudioManager.playback_toggled.connect(func(_is_playing):
		vertical_progress.max_value = AudioManager.current_track_length
	)
func _process(_delta: float) -> void:
	if PlatformManager.is_desktop() and vertical_progress and vertical_progress.visible and AudioManager.is_playing and not _vp_dragging:
		if AudioManager.player and AudioManager.player.is_inside_tree():
			vertical_progress.value = AudioManager.player.get_playback_position()



func _check_setup() -> void:
	if SettingsManager.has_credentials():
		setup_wizard.visible = false
		_trigger_library_scan()
	else:
		setup_wizard.visible = true

func _on_wizard_completed() -> void:
	setup_wizard.visible = false
	_trigger_library_scan()

func _on_wizard_cancelled() -> void:
	setup_wizard.visible = false

func _trigger_library_scan() -> void:
	library_screen.show_loading()
	WebDAVService.scan_music_directory()



# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

func _register_screens() -> void:
	_screens = {
		"library":  library_screen,
		"search":   search_screen,
		"settings": settings_screen,
	}


func _apply_background() -> void:
	background.color = ThemeManager.current_theme.BG_VOID
	
	# Apply glass panel style box to AppWindowFrame
	if _custom_window_stylebox:
		_custom_window_stylebox.bg_color = Color(ThemeManager.current_theme.BG_GLASS.r, ThemeManager.current_theme.BG_GLASS.g, ThemeManager.current_theme.BG_GLASS.b, _custom_window_stylebox.bg_color.a)
		_custom_window_stylebox.border_color = Color(ThemeManager.current_theme.GLASS_BORDER.r, ThemeManager.current_theme.GLASS_BORDER.g, ThemeManager.current_theme.GLASS_BORDER.b, _custom_window_stylebox.border_color.a)
	else:
		var glass_style = ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_LG, 0.55)
		app_window_frame.add_theme_stylebox_override("panel", glass_style)
	
	# Load and assign our custom glass shader (reusing the editor material if present)
	var mat = app_window_frame.material as ShaderMaterial
	if not mat:
		var shader = load("res://assets/shaders/premium_glass.gdshader") as Shader
		if shader:
			mat = ShaderMaterial.new()
			mat.shader = shader
			app_window_frame.material = mat
			
	if mat:
		mat.set_shader_parameter("blur_amount", 4.0)
		mat.set_shader_parameter("saturation_boost", 1.3)
		mat.set_shader_parameter("grain_amount", 0.04)
		mat.set_shader_parameter("border_thickness", 1.5)
		mat.set_shader_parameter("border_color_highlight", Color(1.0, 1.0, 1.0, 0.45))
		mat.set_shader_parameter("border_color_shadow", Color(0.0, 0.0, 0.0, 0.15))
		mat.set_shader_parameter("shimmer_amount", 0.12)
		mat.set_shader_parameter("container_size", app_window_frame.size)
		mat.set_shader_parameter("corner_radius", float(ThemeManager.current_theme.RADIUS_LG))


func _connect_signals() -> void:
	PlatformManager.layout_changed.connect(_on_layout_changed)
	NavManager.navigation_requested.connect(_on_navigation_requested)
	NavManager.setup_wizard_requested.connect(func():
		if setup_wizard.has_method("refresh_fields_and_visibility"):
			setup_wizard.refresh_fields_and_visibility()
		setup_wizard.visible = true
	)
	setup_wizard.wizard_completed.connect(_on_wizard_completed)
	setup_wizard.wizard_cancelled.connect(_on_wizard_cancelled)
	ThemeManager.theme_changed.connect(_apply_background)
	ThemeManager.theme_changed.connect(_update_layout)




# ---------------------------------------------------------------------------
# Responsive Layout
# ---------------------------------------------------------------------------

## Called on _ready and whenever PlatformManager fires layout_changed.
func _update_layout() -> void:
	if PlatformManager.should_show_sidebar():
		_apply_desktop_layout()
	else:
		_apply_mobile_layout()


## Desktop layout: sidebar pinned to the left, content fills the remainder.
func _apply_desktop_layout() -> void:
	var sw  := float(ThemeManager.current_theme.SIDEBAR_WIDTH)

	sidebar.visible   = true
	# Sidebar: anchored to left, width = sw
	sidebar.anchor_left   = 0.0
	sidebar.anchor_top    = 0.0
	sidebar.anchor_right  = 0.0
	sidebar.anchor_bottom = 1.0
	sidebar.offset_left   = 0.0
	sidebar.offset_top    = 0.0
	sidebar.offset_right  = sw
	sidebar.offset_bottom = 0.0

	mini_player.visible = false

	# Content frame: starts at sidebar right-edge, spans full height
	content_frame.anchor_left   = 0.0
	content_frame.anchor_top    = 0.0
	content_frame.anchor_right  = 1.0
	content_frame.anchor_bottom = 1.0
	content_frame.offset_left   = sw
	content_frame.offset_top    = 0.0
	content_frame.offset_right  = 0.0
	content_frame.offset_bottom = 0.0

	# Vertical progress bar: centered along the separator boundary (x = sw)
	if vertical_progress:
		vertical_progress.visible = true
		vertical_progress.anchor_left = 0.0
		vertical_progress.anchor_top = 0.0
		vertical_progress.anchor_right = 0.0
		vertical_progress.anchor_bottom = 1.0
		vertical_progress.offset_left = sw - 8.0
		vertical_progress.offset_top = 0.0
		vertical_progress.offset_right = sw + 8.0
		vertical_progress.offset_bottom = 0.0


## Mobile layout: full-width content; bottom bar flush.
func _apply_mobile_layout() -> void:
	var mph := float(ThemeManager.current_theme.MINI_PLAYER_HEIGHT)

	sidebar.visible = false
	if vertical_progress:
		vertical_progress.visible = false

	# Content fills full width, stops above mini-player.
	content_frame.anchor_left   = 0.0
	content_frame.anchor_top    = 0.0
	content_frame.anchor_right  = 1.0
	content_frame.anchor_bottom = 1.0
	content_frame.offset_left   = 0.0
	content_frame.offset_top    = 0.0
	content_frame.offset_right  = 0.0
	content_frame.offset_bottom = -mph

	# Mini-player sits flush at the bottom.
	mini_player.visible = true
	mini_player.anchor_left   = 0.0
	mini_player.anchor_top    = 1.0
	mini_player.anchor_right  = 1.0
	mini_player.anchor_bottom = 1.0
	mini_player.offset_left   = 0.0
	mini_player.offset_top    = -mph
	mini_player.offset_right  = 0.0
	mini_player.offset_bottom = 0.0




# ---------------------------------------------------------------------------
# Screen Switching
# ---------------------------------------------------------------------------

## Shows the screen matching [param screen_name] and hides all others.
func _show_screen(screen_name: String) -> void:
	for tab_name: String in _screens:
		_screens[tab_name].visible = (tab_name == screen_name)


# ---------------------------------------------------------------------------
# Signal Handlers
# ---------------------------------------------------------------------------

func _on_layout_changed(_bp_name: String) -> void:
	_update_layout()


func _on_navigation_requested(screen_name: String) -> void:
	_show_screen(screen_name)


# ---------------------------------------------------------------------------
# Custom Window Resizing
# ---------------------------------------------------------------------------

func _create_resize_handles() -> void:
	var layout_root = $AppWindowFrame/LayoutRoot
	if not layout_root:
		return
		
	var handles = {
		"top_left": {
			"cursor": Control.CURSOR_FDIAGSIZE,
			"anchor_left": 0.0, "anchor_top": 0.0, "anchor_right": 0.0, "anchor_bottom": 0.0,
			"offset_left": -6.0, "offset_top": -6.0, "offset_right": 6.0, "offset_bottom": 6.0
		},
		"top_right": {
			"cursor": Control.CURSOR_BDIAGSIZE,
			"anchor_left": 1.0, "anchor_top": 0.0, "anchor_right": 1.0, "anchor_bottom": 0.0,
			"offset_left": -6.0, "offset_top": -6.0, "offset_right": 6.0, "offset_bottom": 6.0
		},
		"bottom_left": {
			"cursor": Control.CURSOR_BDIAGSIZE,
			"anchor_left": 0.0, "anchor_top": 1.0, "anchor_right": 0.0, "anchor_bottom": 1.0,
			"offset_left": -6.0, "offset_top": -6.0, "offset_right": 6.0, "offset_bottom": 6.0
		},
		"bottom_right": {
			"cursor": Control.CURSOR_FDIAGSIZE,
			"anchor_left": 1.0, "anchor_top": 1.0, "anchor_right": 1.0, "anchor_bottom": 1.0,
			"offset_left": -6.0, "offset_top": -6.0, "offset_right": 6.0, "offset_bottom": 6.0
		},
		"left": {
			"cursor": Control.CURSOR_HSIZE,
			"anchor_left": 0.0, "anchor_top": 0.0, "anchor_right": 0.0, "anchor_bottom": 1.0,
			"offset_left": -6.0, "offset_top": 6.0, "offset_right": 6.0, "offset_bottom": -6.0
		},
		"right": {
			"cursor": Control.CURSOR_HSIZE,
			"anchor_left": 1.0, "anchor_top": 0.0, "anchor_right": 1.0, "anchor_bottom": 1.0,
			"offset_left": -6.0, "offset_top": 6.0, "offset_right": 6.0, "offset_bottom": -6.0
		},
		"top": {
			"cursor": Control.CURSOR_VSIZE,
			"anchor_left": 0.0, "anchor_top": 0.0, "anchor_right": 1.0, "anchor_bottom": 0.0,
			"offset_left": 6.0, "offset_top": -6.0, "offset_right": -6.0, "offset_bottom": 6.0
		},
		"bottom": {
			"cursor": Control.CURSOR_VSIZE,
			"anchor_left": 0.0, "anchor_top": 1.0, "anchor_right": 1.0, "anchor_bottom": 1.0,
			"offset_left": 6.0, "offset_top": -6.0, "offset_right": -6.0, "offset_bottom": 6.0
		}
	}
	
	for dir in handles:
		var cfg = handles[dir]
		var ctrl = Control.new()
		ctrl.name = "ResizeHandle_" + dir
		ctrl.mouse_default_cursor_shape = cfg.cursor
		ctrl.anchor_left = cfg.anchor_left
		ctrl.anchor_top = cfg.anchor_top
		ctrl.anchor_right = cfg.anchor_right
		ctrl.anchor_bottom = cfg.anchor_bottom
		ctrl.offset_left = cfg.offset_left
		ctrl.offset_top = cfg.offset_top
		ctrl.offset_right = cfg.offset_right
		ctrl.offset_bottom = cfg.offset_bottom
		
		# Wire up click drag
		ctrl.gui_input.connect(func(event: InputEvent):
			if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
				if event.pressed:
					_is_resizing = true
					_resize_mode = dir
					_start_mouse_pos = DisplayServer.mouse_get_position()
					_start_window_position = get_window().position
					_start_window_size = get_window().size
					ctrl.accept_event()
		)
		
		layout_root.add_child(ctrl)


func _input(event: InputEvent) -> void:
	if not PlatformManager.is_desktop():
		return
		
	if _is_resizing:
		if event is InputEventMouseMotion:
			_handle_resize(event)
		elif event is InputEventMouseButton:
			if event.button_index == MOUSE_BUTTON_LEFT and not event.pressed:
				_is_resizing = false
				_resize_mode = ""


func _handle_resize(_event: InputEventMouseMotion) -> void:
	var curr_mouse_pos = DisplayServer.mouse_get_position()
	var diff = curr_mouse_pos - _start_mouse_pos
	
	var new_size = _start_window_size
	var new_pos = _start_window_position
	
	# Handle X axis
	if "left" in _resize_mode:
		var target_w = _start_window_size.x - diff.x
		if target_w >= MIN_WINDOW_SIZE.x:
			new_size.x = target_w
			new_pos.x = _start_window_position.x + diff.x
	elif "right" in _resize_mode:
		new_size.x = max(_start_window_size.x + diff.x, MIN_WINDOW_SIZE.x)
		
	# Handle Y axis
	if "top" in _resize_mode:
		var target_h = _start_window_size.y - diff.y
		if target_h >= MIN_WINDOW_SIZE.y:
			new_size.y = target_h
			new_pos.y = _start_window_position.y + diff.y
	elif "bottom" in _resize_mode:
		new_size.y = max(_start_window_size.y + diff.y, MIN_WINDOW_SIZE.y)
		
	get_window().size = new_size
	get_window().position = new_pos

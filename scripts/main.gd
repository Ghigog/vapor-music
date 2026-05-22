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
@onready var sidebar:          Control   = $Sidebar
@onready var content_frame:    Control   = $ContentFrame
@onready var screen_container: Control   = $ContentFrame/ScreenContainer
@onready var mini_player:      Control   = $MiniPlayer
@onready var tab_bar:          Control   = $TabBar
@onready var setup_wizard:     Control   = $SetupWizard

# Screens — all present in the scene tree; only one is visible at a time.
@onready var library_screen:  Control = $ContentFrame/ScreenContainer/LibraryScreen
@onready var search_screen:   Control = $ContentFrame/ScreenContainer/SearchScreen
@onready var settings_screen: Control = $ContentFrame/ScreenContainer/SettingsScreen

## Maps screen name strings to their scene-tree Control nodes.
var _screens: Dictionary = {}


# ---------------------------------------------------------------------------
# Lifecycle
# ---------------------------------------------------------------------------

func _ready() -> void:
	_register_screens()
	_apply_background()
	_connect_signals()
	_update_layout()
	# Show the default screen without pushing history.
	_show_screen(NavManager.current_screen)
	
	_check_setup()

func _check_setup() -> void:
	if SettingsManager.has_credentials():
		setup_wizard.visible = false
		_trigger_library_scan()
	else:
		setup_wizard.visible = true
		setup_wizard.wizard_completed.connect(_on_wizard_completed)

func _on_wizard_completed() -> void:
	setup_wizard.visible = false
	_trigger_library_scan()

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
	background.color = ThemeManager.BG_VOID


func _connect_signals() -> void:
	PlatformManager.layout_changed.connect(_on_layout_changed)
	NavManager.navigation_requested.connect(_on_navigation_requested)


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
	var sw  := float(ThemeManager.SIDEBAR_WIDTH)
	var mph := float(ThemeManager.MINI_PLAYER_HEIGHT)

	sidebar.visible   = true
	tab_bar.visible   = false

	# Content frame: starts at sidebar right-edge, ends above mini-player.
	content_frame.anchor_left   = 0.0
	content_frame.anchor_top    = 0.0
	content_frame.anchor_right  = 1.0
	content_frame.anchor_bottom = 1.0
	content_frame.offset_left   = sw
	content_frame.offset_top    = 0.0
	content_frame.offset_right  = 0.0
	content_frame.offset_bottom = -mph

	# Mini-player: anchored to the bottom, to the right of the sidebar.
	mini_player.anchor_left   = 0.0
	mini_player.anchor_top    = 1.0
	mini_player.anchor_right  = 1.0
	mini_player.anchor_bottom = 1.0
	mini_player.offset_left   = sw
	mini_player.offset_top    = -mph
	mini_player.offset_right  = 0.0
	mini_player.offset_bottom = 0.0


## Mobile layout: full-width content; tab bar floats above bottom inset.
func _apply_mobile_layout() -> void:
	var mph := float(ThemeManager.MINI_PLAYER_HEIGHT)
	# Extra clearance below the floating tab-bar pill.
	var tab_clearance := float(ThemeManager.NAV_BAR_HEIGHT_MOBILE) \
		+ float(ThemeManager.SPACE_4) * 2.0

	sidebar.visible = false
	tab_bar.visible = true

	# Content fills full width, stops above mini-player.
	content_frame.anchor_left   = 0.0
	content_frame.anchor_top    = 0.0
	content_frame.anchor_right  = 1.0
	content_frame.anchor_bottom = 1.0
	content_frame.offset_left   = 0.0
	content_frame.offset_top    = 0.0
	content_frame.offset_right  = 0.0
	content_frame.offset_bottom = -(mph + tab_clearance)

	# Mini-player sits directly above the tab bar.
	mini_player.anchor_left   = 0.0
	mini_player.anchor_top    = 1.0
	mini_player.anchor_right  = 1.0
	mini_player.anchor_bottom = 1.0
	mini_player.offset_left   = 0.0
	mini_player.offset_top    = -(mph + tab_clearance)
	mini_player.offset_right  = 0.0
	mini_player.offset_bottom = -tab_clearance


# ---------------------------------------------------------------------------
# Screen Switching
# ---------------------------------------------------------------------------

## Shows the screen matching [param screen_name] and hides all others.
func _show_screen(screen_name: String) -> void:
	for name: String in _screens:
		_screens[name].visible = (name == screen_name)


# ---------------------------------------------------------------------------
# Signal Handlers
# ---------------------------------------------------------------------------

func _on_layout_changed(_bp_name: String) -> void:
	_update_layout()


func _on_navigation_requested(screen_name: String) -> void:
	_show_screen(screen_name)

## sidebar.gd
## Desktop navigation sidebar controller.
## Applies design tokens from ThemeManager to the panel and nav buttons,
## and keeps active-item highlights in sync with NavManager signals.
extends PanelContainer

@onready var app_name:     Label  = $VBox/Header/LogoContainer/AppName
@onready var nav_library:  Button = $VBox/NavItems/NavLibrary
@onready var nav_search:   Button = $VBox/NavItems/NavSearch
@onready var nav_settings: Button = $VBox/NavItems/NavSettings

## Maps screen-name strings to their corresponding Button nodes.
var _nav_buttons: Dictionary = {}


func _ready() -> void:
	_apply_panel_style()
	_apply_logo_style()
	_register_nav_buttons()
	_connect_signals()
	_set_active_nav(NavManager.current_screen)
	ThemeManager.theme_changed.connect(_apply_panel_style)
	ThemeManager.theme_changed.connect(_apply_logo_style)
	ThemeManager.theme_changed.connect(_refresh_nav_button_styles)


# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_panel_style() -> void:
	add_theme_stylebox_override("panel", ThemeManager.make_nav_panel())
	custom_minimum_size.x = ThemeManager.current_theme.SIDEBAR_WIDTH


func _apply_logo_style() -> void:
	app_name.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	app_name.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	app_name.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_MD)


# ---------------------------------------------------------------------------
# Nav Buttons
# ---------------------------------------------------------------------------

func _register_nav_buttons() -> void:
	_nav_buttons = {
		"library":  nav_library,
		"search":   nav_search,
		"settings": nav_settings,
	}
	for screen_name: String in _nav_buttons:
		var btn: Button = _nav_buttons[screen_name]
		btn.pressed.connect(_on_nav_pressed.bind(screen_name))


func _refresh_nav_button_styles() -> void:
	for nav in _nav_buttons:
		var btn = _nav_buttons[nav]
		var is_active = (nav == NavManager.current_screen)
		_style_nav_button(btn, is_active)

func _style_nav_button(btn: Button, active: bool) -> void:
	btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	btn.custom_minimum_size.y = ThemeManager.current_theme.TOUCH_TARGET_MIN
	btn.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	btn.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	if active:
		btn.add_theme_stylebox_override("normal",  ThemeManager.make_nav_item_active())
		btn.add_theme_stylebox_override("hover",   ThemeManager.make_nav_item_active())
		btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_active())
		btn.add_theme_color_override("font_color",       ThemeManager.current_theme.ACCENT_CORE)
		btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	else:
		btn.add_theme_stylebox_override("normal",  ThemeManager.make_transparent())
		btn.add_theme_stylebox_override("hover",   ThemeManager.make_nav_item_hover())
		btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
		btn.add_theme_color_override("font_color",       ThemeManager.current_theme.TEXT_TERTIARY)
		btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.TEXT_SECONDARY)
	btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())


func _set_active_nav(screen_name: String) -> void:
	for tab_name: String in _nav_buttons:
		_style_nav_button(_nav_buttons[tab_name], tab_name == screen_name)


# ---------------------------------------------------------------------------
# Signal Handlers
# ---------------------------------------------------------------------------

func _connect_signals() -> void:
	NavManager.navigation_requested.connect(_on_navigation_requested)


func _on_nav_pressed(screen_name: String) -> void:
	NavManager.navigate_to(screen_name)


func _on_navigation_requested(screen_name: String) -> void:
	_set_active_nav(screen_name)

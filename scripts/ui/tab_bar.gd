## tab_bar.gd
## Mobile floating glass-pill tab bar controller.
## Positions the pill at the bottom-centre of the viewport and keeps
## the active-tab highlight in sync with NavManager signals.
extends Control

@onready var glass_pill:   PanelContainer = $GlassPill
@onready var tab_library:  Button         = $GlassPill/TabRow/TabLibrary
@onready var tab_search:   Button         = $GlassPill/TabRow/TabSearch
@onready var tab_settings: Button         = $GlassPill/TabRow/TabSettings

## Maps screen-name strings to their tab Button nodes.
var _tabs: Dictionary = {}


func _ready() -> void:
	_tabs = {
		"library":  tab_library,
		"search":   tab_search,
		"settings": tab_settings,
	}
	_apply_pill_style()
	_setup_tab_buttons()
	_connect_signals()
	_position_pill()
	_set_active_tab(NavManager.current_screen)


# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_pill_style() -> void:
	glass_pill.add_theme_stylebox_override(
		"panel",
		ThemeManager.make_glass_panel(ThemeManager.RADIUS_PILL, 0.85)
	)


func _setup_tab_buttons() -> void:
	for tab_name: String in _tabs:
		var btn: Button = _tabs[tab_name]
		btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		btn.custom_minimum_size.y = ThemeManager.NAV_BAR_HEIGHT_MOBILE
		btn.add_theme_font_override("font", ThemeManager.font_ui)
		btn.add_theme_font_size_override("font_size", ThemeManager.TYPE_2XS)
		btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		btn.pressed.connect(_on_tab_pressed.bind(tab_name))


func _style_tab_button(btn: Button, active: bool) -> void:
	var color := ThemeManager.ACCENT_CORE if active else ThemeManager.TEXT_TERTIARY
	btn.add_theme_stylebox_override("normal",  ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("hover",   ThemeManager.make_nav_item_hover())
	btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	btn.add_theme_color_override("font_color",       color)
	btn.add_theme_color_override("font_hover_color", ThemeManager.TEXT_SECONDARY)


func _set_active_tab(screen_name: String) -> void:
	for tab_name: String in _tabs:
		_style_tab_button(_tabs[tab_name], tab_name == screen_name)


# ---------------------------------------------------------------------------
# Positioning — pill floats above the bottom safe-area inset.
# ---------------------------------------------------------------------------

func _position_pill() -> void:
	var vp  := PlatformManager.get_viewport_size()
	var pw  := minf(vp.x - float(ThemeManager.SPACE_8) * 2.0, 420.0)
	var ph  := float(ThemeManager.NAV_BAR_HEIGHT_MOBILE)
	var gap := float(ThemeManager.SPACE_4)

	glass_pill.anchor_left   = 0.5
	glass_pill.anchor_right  = 0.5
	glass_pill.anchor_top    = 1.0
	glass_pill.anchor_bottom = 1.0
	glass_pill.offset_left   = -pw / 2.0
	glass_pill.offset_right  =  pw / 2.0
	glass_pill.offset_top    = -(ph + gap)
	glass_pill.offset_bottom = -gap
	glass_pill.custom_minimum_size = Vector2(pw, ph)


# ---------------------------------------------------------------------------
# Signal Handlers
# ---------------------------------------------------------------------------

func _connect_signals() -> void:
	NavManager.navigation_requested.connect(_on_navigation_requested)
	PlatformManager.viewport_resized.connect(_on_viewport_resized)


func _on_tab_pressed(screen_name: String) -> void:
	NavManager.navigate_to(screen_name)


func _on_navigation_requested(screen_name: String) -> void:
	_set_active_tab(screen_name)


func _on_viewport_resized(_size: Vector2) -> void:
	_position_pill()

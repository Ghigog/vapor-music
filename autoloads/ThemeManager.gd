## ThemeManager.gd
## Autoload singleton — single source of truth for all design tokens defined in
## docs/DESIGN_LANGUAGE.md. Every colour, size, duration, and StyleBox used in
## the app should originate here.
##
## Usage:
##   ThemeManager.BG_VOID          → Color
##   ThemeManager.make_glass_panel()  → StyleBoxFlat
##   ThemeManager.font_ui          → Font (SystemFont fallback)

extends Node
signal theme_changed
var current_theme: ThemeData = preload("res://assets/themes/default_dark.tres")

func _ready() -> void:
	_setup_fonts()


## Creates SystemFont resources that prefer the design-specified typefaces
## and fall back gracefully on systems where they are not installed.
func _setup_fonts() -> void:
	current_theme.font_ui = _make_system_font(
		["Inter", "Segoe UI", "SF Pro Text", "Helvetica Neue", "Arial"]
	)
	current_theme.font_display = _make_system_font(
		["Outfit", "Nunito", "Segoe UI", "SF Pro Display", "Helvetica Neue"]
	)
	current_theme.font_mono = _make_system_font(
		["JetBrains Mono", "Cascadia Code", "Consolas", "Courier New"]
	)


func _make_system_font(names: Array[String]) -> SystemFont:
	var f := SystemFont.new()
	f.font_names = names
	f.antialiasing = TextServer.FONT_ANTIALIASING_LCD
	return f


# ---------------------------------------------------------------------------
# StyleBox Factory Methods
## All reusable StyleBox instances are created fresh per call so that callers
## can mutate them without side-effects. For hot paths, cache the result.
# ---------------------------------------------------------------------------

## Returns a StyleBoxFlat configured as a Vapor glass panel.
## [param radius]  Corner radius in pixels. Defaults to RADIUS_MD (16px).
## [param alpha]   Background alpha. Defaults to 0.55 (standard glass density).
func make_glass_panel(radius: int = current_theme.RADIUS_MD, alpha: float = 0.55) -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = Color(current_theme.BG_GLASS.r, current_theme.BG_GLASS.g, current_theme.BG_GLASS.b, alpha)
	s.border_color = current_theme.GLASS_BORDER
	s.set_border_width_all(1)
	s.set_corner_radius_all(radius)
	s.shadow_color = Color(0.024, 0.024, 0.039, 0.50)
	s.shadow_size = 16
	s.shadow_offset = Vector2(0, 4)
	return s


## Returns a StyleBoxFlat for the solid dark sidebar / nav background.
func make_nav_panel() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = current_theme.BG_BASE
	# Hairline right-edge separator
	s.border_color = Color(1.0, 1.0, 1.0, 0.05)
	s.border_width_right = 1
	s.set_corner_radius_all(0)
	return s


## Returns a StyleBoxFlat for an active / selected navigation item.
func make_nav_item_active() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = current_theme.ACCENT_SURFACE
	s.border_color = Color(current_theme.ACCENT_CORE.r, current_theme.ACCENT_CORE.g, current_theme.ACCENT_CORE.b, 0.30)
	s.border_width_left = 2
	s.set_corner_radius_all(current_theme.RADIUS_SM)
	return s


## Returns a StyleBoxFlat for a hovered-over navigation item.
func make_nav_item_hover() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = Color(1.0, 1.0, 1.0, 0.04)
	s.set_corner_radius_all(current_theme.RADIUS_SM)
	return s


## Returns an empty / transparent StyleBoxEmpty.
func make_transparent() -> StyleBoxEmpty:
	return StyleBoxEmpty.new()


## Returns a StyleBoxFlat for the primary CTA button.
## [param filled]  If true, fills with ACCENT_CORE. Otherwise uses tinted glass.
func make_cta_button(filled: bool = false) -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = current_theme.ACCENT_CORE if filled else current_theme.ACCENT_SURFACE
	s.border_color = current_theme.ACCENT_CORE
	s.set_border_width_all(1)
	s.set_corner_radius_all(current_theme.RADIUS_SM)
	return s


## Returns a StyleBoxFlat for the album-art circular placeholder.
func make_circle_placeholder() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = current_theme.BG_ELEVATED
	s.border_color = current_theme.GLASS_BORDER
	s.set_border_width_all(1)
	s.set_corner_radius_all(current_theme.RADIUS_PILL)
	return s

func load_theme(path: String) -> void:
	# Load the new resource
	var new_theme = load(path)
	if new_theme is ThemeData:
		current_theme = new_theme
		# Re-apply system fonts to the new resource
		_setup_fonts() 
		# Notify all listening UI components to refresh
		theme_changed.emit()

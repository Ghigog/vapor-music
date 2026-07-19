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
	# GRAY (not LCD) antialiasing: LCD subpixel rendering assumes an opaque
	# background and produces dark colour-fringe halos when composited over a
	# transparent window. GRAY uses pure alpha coverage and works correctly on
	# any background colour, including fully transparent ones.
	f.antialiasing = TextServer.FONT_ANTIALIASING_GRAY
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
	# Neutral dark shadow — disabled completely to match flat Apple widgets look.
	s.shadow_color = Color(0.0, 0.0, 0.0, 0.0)
	s.shadow_size = 0
	s.shadow_offset = Vector2(0, 0)
	
	s.content_margin_left = 12
	s.content_margin_top = 12
	s.content_margin_right = 12
	s.content_margin_bottom = 12
	return s


## Returns a StyleBoxFlat for the solid dark sidebar / nav background.
func make_nav_panel() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = current_theme.BG_BASE
	# Hairline right-edge separator — reads from theme so it inverts correctly
	# between the dark and light glass palettes.
	s.border_color = current_theme.GLASS_BORDER_SUBTLE
	s.border_width_right = 1
	s.set_corner_radius_all(0)
	s.set_corner_radius(0, current_theme.RADIUS_LG) # CORNER_TOP_LEFT
	s.set_corner_radius(3, current_theme.RADIUS_LG) # CORNER_BOTTOM_LEFT
	
	s.content_margin_left = 16
	s.content_margin_top = 16
	s.content_margin_right = 0
	s.content_margin_bottom = 16
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
	# Use GLASS_TINT so the hover is a subtle warm-neutral wash in both modes
	# rather than a hard-coded white that looks wrong on light backgrounds.
	s.bg_color = current_theme.GLASS_TINT
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


## Returns the minimum height an interactive control should occupy.
##
## On touch-primary hardware nothing may render below TOUCH_TARGET_MIN (44 px) —
## a finger has no pixel precision. On pointer hardware the caller's own compact
## value stands, so desktop keeps its dense layout.
##
## Note this keys off INPUT hardware, not layout width: a tablet in landscape gets
## the desktop sidebar layout but still needs finger-sized targets. See
## PlatformManager's header for why those two concerns are kept separate.
##
## [param compact]  The height to use when a precise pointer is available.
func min_touch_height(compact: int) -> int:
	if is_instance_valid(PlatformManager) and PlatformManager.is_touch_primary():
		return maxi(compact, current_theme.TOUCH_TARGET_MIN)
	return compact


## Square variant of min_touch_height() for icon buttons — enforces the minimum on
## both axes, since a 44x24 target is still only 24 px tall to a fingertip.
func min_touch_size(compact: Vector2) -> Vector2:
	if is_instance_valid(PlatformManager) and PlatformManager.is_touch_primary():
		var m := float(current_theme.TOUCH_TARGET_MIN)
		return Vector2(maxf(compact.x, m), maxf(compact.y, m))
	return compact


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
		# Re-apply base font size if SettingsManager exists
		var sm = get_node_or_null("/root/SettingsManager")
		if sm and "base_font_size" in sm:
			set_base_font_size(sm.base_font_size)
		else:
			theme_changed.emit()

func set_base_font_size(size: int) -> void:
	current_theme.TYPE_2XS = max(6, size - 5)
	current_theme.TYPE_XS = max(8, size - 3)
	current_theme.TYPE_SM = max(9, size - 2)
	current_theme.TYPE_BASE = size
	current_theme.TYPE_MD = size + 2
	current_theme.TYPE_LG = size + 4 # 4 points bigger than default (base size) for titles
	current_theme.TYPE_XL = size + 8
	current_theme.TYPE_2XL = size + 14
	current_theme.TYPE_DISPLAY = size + 20
	theme_changed.emit()

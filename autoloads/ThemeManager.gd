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
		_apply_theme(new_theme)


## Builds a full ThemeData palette from just two user-picked colors, then
## applies it as the active theme. This is the "dynamic theme" entry point
## used by the Settings color pickers — see docs/theme_system.md §7.
## [param base_color]    The main surface tone (what the user sees as panels/cards).
## [param accent_color]  The interactive/highlight color.
func apply_custom_colors(base_color: Color, accent_color: Color) -> void:
	_apply_theme(generate_theme_from_colors(base_color, accent_color))


## Derives a complete ThemeData resource from a base surface color and an
## accent color. Layout scales, semantic colors, and fonts are left at their
## ThemeData defaults — only the color tokens are computed. Whether the theme
## reads as "dark" or "light" is decided automatically from the base color's
## luminance, so text and glass borders always stay legible.
func generate_theme_from_colors(base_color: Color, accent_color: Color) -> ThemeData:
	var t := ThemeData.new()
	var is_dark := base_color.get_luminance() < 0.5

	# Background layers — BG_ELEVATED is the base color as picked; the other
	# layers are lightened/darkened steps away from it. BG_VOID stays fully
	# transparent (it's the clear color behind the OS-level frosted window).
	var base_layer := base_color.darkened(0.08) if is_dark else base_color.darkened(0.02)
	var float_layer := base_color.lightened(0.10) if is_dark else base_color.lightened(0.02)
	t.BG_VOID = Color(base_color.r, base_color.g, base_color.b, 0.0)
	t.BG_BASE = Color(base_layer.r, base_layer.g, base_layer.b, 0.65)
	t.BG_ELEVATED = Color(base_color.r, base_color.g, base_color.b, 1.0)
	t.BG_FLOAT = Color(float_layer.r, float_layer.g, float_layer.b, 1.0)
	t.BG_GLASS = Color(base_color.r, base_color.g, base_color.b, 0.55)

	# Text hierarchy and glass borders flip between white-on-dark and
	# black-on-light depending on the base color's luminance.
	if is_dark:
		t.TEXT_PRIMARY = Color(1, 1, 1, 1)
		t.TEXT_SECONDARY = Color(1, 1, 1, 0.6)
		t.TEXT_TERTIARY = Color(1, 1, 1, 0.35)
		t.TEXT_DISABLED = Color(1, 1, 1, 0.18)
		t.TEXT_INVERSE = Color(0.08, 0.08, 0.08, 1)
		t.GLASS_TINT = Color(1, 1, 1, 0.03)
		t.GLASS_BORDER = Color(1, 1, 1, 0.15)
		t.GLASS_BORDER_SUBTLE = Color(1, 1, 1, 0.06)
		t.GLASS_SHIMMER = Color(1, 1, 1, 0.05)
	else:
		t.TEXT_PRIMARY = Color(0.149, 0.149, 0.149, 1)
		t.TEXT_SECONDARY = Color(0.557, 0.557, 0.576, 1)
		t.TEXT_TERTIARY = Color(0.557, 0.557, 0.576, 0.6)
		t.TEXT_DISABLED = Color(0.557, 0.557, 0.576, 0.3)
		t.TEXT_INVERSE = Color(1, 1, 1, 1)
		t.GLASS_TINT = Color(0.957, 0.957, 0.965, 0.05)
		t.GLASS_BORDER = Color(1, 1, 1, 0.8)
		t.GLASS_BORDER_SUBTLE = Color(0, 0, 0, 0.06)
		t.GLASS_SHIMMER = Color(1, 1, 1, 0.08)

	# Accent family — every interactive/highlight tone derives from the one
	# accent color. The secondary "Aqua" group aliases it, matching the
	# convention already used by the default presets (see theme_system.md).
	t.ACCENT_CORE = accent_color
	t.ACCENT_BRIGHT = accent_color.lightened(0.2)
	t.ACCENT_DIM = accent_color.darkened(0.18)
	t.ACCENT_GLOW = Color(accent_color.r, accent_color.g, accent_color.b, 0.35)
	t.ACCENT_SURFACE = Color(accent_color.r, accent_color.g, accent_color.b, 0.12)
	t.AQUA_CORE = t.ACCENT_CORE
	t.AQUA_DIM = t.ACCENT_DIM
	t.AQUA_GLOW = Color(accent_color.r, accent_color.g, accent_color.b, 0.3)

	return t


## Shared tail for load_theme()/apply_custom_colors(): swaps in the resource,
## rebuilds fonts, and re-applies the user's font-size scale before notifying
## listeners so a single theme_changed carries a fully consistent state.
func _apply_theme(new_theme: ThemeData) -> void:
	current_theme = new_theme
	_setup_fonts()
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

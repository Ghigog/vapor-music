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


# ---------------------------------------------------------------------------
# Background Layers — Dark Mode (primary)
# ---------------------------------------------------------------------------
## True background — the "room" behind everything.
const BG_VOID     := Color(0.0235, 0.0235, 0.0392, 1.0)   # #06060A
## Primary app surface — sidebar, navigation bars.
const BG_BASE     := Color(0.051,  0.051,  0.078,  1.0)   # #0D0D14
## Slightly elevated — card backgrounds.
const BG_ELEVATED := Color(0.071,  0.071,  0.110,  1.0)   # #12121C
## Floating — modals, drawers, overlay panels.
const BG_FLOAT    := Color(0.102,  0.102,  0.157,  1.0)   # #1A1A28
## Frosted glass base — combine with backdrop blur shader.
const BG_GLASS    := Color(0.071,  0.071,  0.110,  0.55)  # rgba(18,18,28,0.55)


# ---------------------------------------------------------------------------
# Glass Surface Tints
# ---------------------------------------------------------------------------
## Cool blue-tinted overlay applied to every frosted surface.
const GLASS_TINT          := Color(0.471, 0.549, 1.000, 0.04)
## Top/left edge highlight — simulates light hitting glass.
const GLASS_BORDER        := Color(1.0,   1.0,   1.0,   0.10)
## Bottom/right edge shadow.
const GLASS_BORDER_SUBTLE := Color(1.0,   1.0,   1.0,   0.05)
## Subtle gradient sheen across glass panels.
const GLASS_SHIMMER       := Color(1.0,   1.0,   1.0,   0.06)
## Standard backdrop blur radius (px).
const GLASS_BLUR          := 24.0
## Heavy blur for primary modals and Now Playing full-screen.
const GLASS_BLUR_HEAVY    := 40.0


# ---------------------------------------------------------------------------
# Text Colors
# ---------------------------------------------------------------------------
## Headings, track titles, primary labels. Warm-cool white — never pure white.
const TEXT_PRIMARY   := Color(0.941, 0.941, 1.000, 1.00)  # #F0F0FF
## Artist names, subtitles, secondary metadata.
const TEXT_SECONDARY := Color(0.941, 0.941, 1.000, 0.65)
## Timestamps, inactive tabs, hints.
const TEXT_TERTIARY  := Color(0.941, 0.941, 1.000, 0.35)
## Disabled states.
const TEXT_DISABLED  := Color(0.941, 0.941, 1.000, 0.18)
## Text on light/accent surfaces.
const TEXT_INVERSE   := Color(0.024, 0.024, 0.039, 1.00)  # #06060A


# ---------------------------------------------------------------------------
# Primary Accent — Aurora Blue-Violet  (#7B6EF6)
# ---------------------------------------------------------------------------
## Primary interactive elements, active states, progress fills.
const ACCENT_CORE    := Color(0.482, 0.431, 0.965, 1.00)  # #7B6EF6
## Hover states, focus rings, highlights.
const ACCENT_BRIGHT  := Color(0.608, 0.557, 1.000, 1.00)  # #9B8EFF
## Pressed states, deep accents.
const ACCENT_DIM     := Color(0.353, 0.306, 0.831, 1.00)  # #5A4ED4
## Ambient glow — used in box-shadows and drop shadows.
const ACCENT_GLOW    := Color(0.482, 0.431, 0.965, 0.35)
## Tinted glass panels, selected states.
const ACCENT_SURFACE := Color(0.482, 0.431, 0.965, 0.12)


# ---------------------------------------------------------------------------
# Secondary Accent — Vapor Aqua  (#3DD6C8)
## Waveform visualiser, AI DJ pulse, cloud sync animation.
# ---------------------------------------------------------------------------
const AQUA_CORE := Color(0.239, 0.839, 0.784, 1.00)  # #3DD6C8
const AQUA_DIM  := Color(0.165, 0.659, 0.612, 1.00)  # #2AA89C
const AQUA_GLOW := Color(0.239, 0.839, 0.784, 0.30)


# ---------------------------------------------------------------------------
# Semantic Colors
## Always used as glows and tinted surfaces — never as flat saturated blocks.
# ---------------------------------------------------------------------------
const SEMANTIC_SUCCESS := Color(0.204, 0.827, 0.600, 1.0)  # #34D399
const SEMANTIC_WARNING := Color(0.984, 0.749, 0.141, 1.0)  # #FBBF24
const SEMANTIC_ERROR   := Color(0.973, 0.443, 0.443, 1.0)  # #F87171
const SEMANTIC_INFO    := Color(0.376, 0.647, 0.980, 1.0)  # #60A5FA


# ---------------------------------------------------------------------------
# Border Radius Scale (px)
# ---------------------------------------------------------------------------
const RADIUS_XS   := 6
const RADIUS_SM   := 10
const RADIUS_MD   := 16
const RADIUS_LG   := 24
const RADIUS_XL   := 32
const RADIUS_2XL  := 48
const RADIUS_PILL := 9999


# ---------------------------------------------------------------------------
# Spacing Scale — 4px grid (px)
# ---------------------------------------------------------------------------
const SPACE_1  := 4
const SPACE_2  := 8
const SPACE_3  := 12
const SPACE_4  := 16
const SPACE_5  := 20
const SPACE_6  := 24
const SPACE_8  := 32
const SPACE_10 := 40
const SPACE_12 := 48
const SPACE_16 := 64


# ---------------------------------------------------------------------------
# Typography Scale (px)
# ---------------------------------------------------------------------------
const TYPE_2XS     := 11
const TYPE_XS      := 13
const TYPE_SM      := 14
const TYPE_BASE    := 16
const TYPE_MD      := 18
const TYPE_LG      := 22
const TYPE_XL      := 28
const TYPE_2XL     := 36
const TYPE_DISPLAY := 48


# ---------------------------------------------------------------------------
# Motion — Duration (seconds)
# ---------------------------------------------------------------------------
const DURATION_INSTANT     := 0.08
const DURATION_FAST        := 0.16
const DURATION_NORMAL      := 0.24
const DURATION_SLOW        := 0.40
const DURATION_ATMOSPHERIC := 1.00


# ---------------------------------------------------------------------------
# Component Constants (px)
# ---------------------------------------------------------------------------
const SIDEBAR_WIDTH           := 240
const SIDEBAR_WIDTH_COLLAPSED := 68
const NAV_BAR_HEIGHT_DESKTOP  := 64
const NAV_BAR_HEIGHT_MOBILE   := 56
const MINI_PLAYER_HEIGHT      := 72
const TOUCH_TARGET_MIN        := 44


# ---------------------------------------------------------------------------
# Fonts — SystemFont with priority fallback chains.
## TODO: Replace SystemFont with FontFile once Inter, Outfit, and
##       JetBrains Mono TTFs are added to res://fonts/.
# ---------------------------------------------------------------------------
## Inter — primary UI typeface.
var font_ui: Font
## Outfit — display / title typeface.
var font_display: Font
## JetBrains Mono — BPM, key labels, timestamps, technical metadata.
var font_mono: Font


func _ready() -> void:
	_setup_fonts()


## Creates SystemFont resources that prefer the design-specified typefaces
## and fall back gracefully on systems where they are not installed.
func _setup_fonts() -> void:
	font_ui = _make_system_font(
		["Inter", "Segoe UI", "SF Pro Text", "Helvetica Neue", "Arial"]
	)
	font_display = _make_system_font(
		["Outfit", "Nunito", "Segoe UI", "SF Pro Display", "Helvetica Neue"]
	)
	font_mono = _make_system_font(
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
func make_glass_panel(radius: int = RADIUS_MD, alpha: float = 0.55) -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = Color(BG_GLASS.r, BG_GLASS.g, BG_GLASS.b, alpha)
	s.border_color = GLASS_BORDER
	s.set_border_width_all(1)
	s.set_corner_radius_all(radius)
	s.shadow_color = Color(0.024, 0.024, 0.039, 0.50)
	s.shadow_size = 16
	s.shadow_offset = Vector2(0, 4)
	return s


## Returns a StyleBoxFlat for the solid dark sidebar / nav background.
func make_nav_panel() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = BG_BASE
	# Hairline right-edge separator
	s.border_color = Color(1.0, 1.0, 1.0, 0.05)
	s.border_width_right = 1
	s.set_corner_radius_all(0)
	return s


## Returns a StyleBoxFlat for an active / selected navigation item.
func make_nav_item_active() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = ACCENT_SURFACE
	s.border_color = Color(ACCENT_CORE.r, ACCENT_CORE.g, ACCENT_CORE.b, 0.30)
	s.border_width_left = 2
	s.set_corner_radius_all(RADIUS_SM)
	return s


## Returns a StyleBoxFlat for a hovered-over navigation item.
func make_nav_item_hover() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = Color(1.0, 1.0, 1.0, 0.04)
	s.set_corner_radius_all(RADIUS_SM)
	return s


## Returns an empty / transparent StyleBoxEmpty.
func make_transparent() -> StyleBoxEmpty:
	return StyleBoxEmpty.new()


## Returns a StyleBoxFlat for the primary CTA button.
## [param filled]  If true, fills with ACCENT_CORE. Otherwise uses tinted glass.
func make_cta_button(filled: bool = false) -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = ACCENT_CORE if filled else ACCENT_SURFACE
	s.border_color = ACCENT_CORE
	s.set_border_width_all(1)
	s.set_corner_radius_all(RADIUS_SM)
	return s


## Returns a StyleBoxFlat for the album-art circular placeholder.
func make_circle_placeholder() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = BG_ELEVATED
	s.border_color = GLASS_BORDER
	s.set_border_width_all(1)
	s.set_corner_radius_all(RADIUS_PILL)
	return s

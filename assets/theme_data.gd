# theme_data.gd
class_name ThemeData extends Resource

@export_subgroup("Background Layers — Dark Mode")
## True background — the "room" behind everything.
@export var BG_VOID		:= Color(0.0235, 0.0235, 0.0392, 1.0)   # #06060A
## Primary app surface — sidebar, navigation bars.
@export var BG_BASE     := Color(0.051,  0.051,  0.078,  1.0)   # #0D0D14
## Slightly elevated — card backgrounds.
@export var BG_ELEVATED := Color(0.071,  0.071,  0.110,  1.0)   # #12121C
## Floating — modals, drawers, overlay panels.
@export var BG_FLOAT    := Color(0.102,  0.102,  0.157,  1.0)   # #1A1A28
## Frosted glass base — combine with backdrop blur shader.
@export var BG_GLASS    := Color(0.071,  0.071,  0.110,  0.55)  # rgba(18,18,28,0.55)



@export_subgroup("Glass Surface Tints")
## Cool blue-tinted overlay applied to every frosted surface.
@export var GLASS_TINT          := Color(0.471, 0.549, 1.000, 0.04)
## Top/left edge highlight — simulates light hitting glass.
@export var GLASS_BORDER        := Color(1.0,   1.0,   1.0,   0.10)
## Bottom/right edge shadow.
@export var GLASS_BORDER_SUBTLE := Color(1.0,   1.0,   1.0,   0.05)
## Subtle gradient sheen across glass panels.
@export var GLASS_SHIMMER       := Color(1.0,   1.0,   1.0,   0.06)
## Standard backdrop blur radius (px).
@export var GLASS_BLUR          := 24.0
## Heavy blur for primary modals and Now Playing full-screen.
@export var GLASS_BLUR_HEAVY    := 40.0


@export_subgroup("Text Colors")
## Headings, track titles, primary labels. Warm-cool white — never pure white.
@export var TEXT_PRIMARY   := Color(0.941, 0.941, 1.000, 1.00)  # #F0F0FF
## Artist names, subtitles, secondary metadata.
@export var TEXT_SECONDARY := Color(0.941, 0.941, 1.000, 0.65)
## Timestamps, inactive tabs, hints.
@export var TEXT_TERTIARY  := Color(0.941, 0.941, 1.000, 0.35)
## Disabled states.
@export var TEXT_DISABLED  := Color(0.941, 0.941, 1.000, 0.18)
## Text on light/accent surfaces.
@export var TEXT_INVERSE   := Color(0.024, 0.024, 0.039, 1.00)  # #06060A


# ---------------------------------------------------------------------------
#  — Aurora Blue-Violet  (#7B6EF6)
# ---------------------------------------------------------------------------
@export_subgroup("Primary Accent")
## Primary interactive elements, active states, progress fills.
@export var ACCENT_CORE    := Color(0.482, 0.431, 0.965, 1.00)  # #7B6EF6
## Hover states, focus rings, highlights.
@export var ACCENT_BRIGHT  := Color(0.608, 0.557, 1.000, 1.00)  # #9B8EFF
## Pressed states, deep accents.
@export var ACCENT_DIM     := Color(0.353, 0.306, 0.831, 1.00)  # #5A4ED4
## Ambient glow — used in box-shadows and drop shadows.
@export var ACCENT_GLOW    := Color(0.482, 0.431, 0.965, 0.35)
## Tinted glass panels, selected states.
@export var ACCENT_SURFACE := Color(0.482, 0.431, 0.965, 0.12)


# ---------------------------------------------------------------------------
#  — Vapor Aqua  (#3DD6C8)
## Waveform visualiser, AI DJ pulse, cloud sync animation.
# ---------------------------------------------------------------------------
@export_subgroup("Secondary Accent")
@export var AQUA_CORE := Color(0.239, 0.839, 0.784, 1.00)  # #3DD6C8
@export var AQUA_DIM  := Color(0.165, 0.659, 0.612, 1.00)  # #2AA89C
@export var AQUA_GLOW := Color(0.239, 0.839, 0.784, 0.30)


# ---------------------------------------------------------------------------
# 
## Always used as glows and tinted surfaces — never as flat saturated blocks.
# ---------------------------------------------------------------------------
@export_subgroup("Semantic Colors")
@export var SEMANTIC_SUCCESS := Color(0.204, 0.827, 0.600, 1.0)  # #34D399
@export var SEMANTIC_WARNING := Color(0.984, 0.749, 0.141, 1.0)  # #FBBF24
@export var SEMANTIC_ERROR   := Color(0.973, 0.443, 0.443, 1.0)  # #F87171
@export var SEMANTIC_INFO    := Color(0.376, 0.647, 0.980, 1.0)  # #60A5FA


# ---------------------------------------------------------------------------
#
# ---------------------------------------------------------------------------
@export_subgroup("Border Radius Scale (px)")
@export var RADIUS_XS   := 6
@export var RADIUS_SM   := 10
@export var RADIUS_MD   := 16
@export var RADIUS_LG   := 24
@export var RADIUS_XL   := 32
@export var RADIUS_2XL  := 48
@export var RADIUS_PILL := 9999


# ---------------------------------------------------------------------------
# 
# ---------------------------------------------------------------------------
@export_subgroup("Spacing Scale — 4px grid (px)")
@export var SPACE_1  := 4
@export var SPACE_2  := 8
@export var SPACE_3  := 12
@export var SPACE_4  := 16
@export var SPACE_5  := 20
@export var SPACE_6  := 24
@export var SPACE_8  := 32
@export var SPACE_10 := 40
@export var SPACE_12 := 48
@export var SPACE_16 := 64


# ---------------------------------------------------------------------------
# 
# ---------------------------------------------------------------------------
@export_subgroup("Typography Scale (px)")
@export var TYPE_2XS     := 11
@export var TYPE_XS      := 13
@export var TYPE_SM      := 14
@export var TYPE_BASE    := 16
@export var TYPE_MD      := 18
@export var TYPE_LG      := 22
@export var TYPE_XL      := 28
@export var TYPE_2XL     := 36
@export var TYPE_DISPLAY := 48


# ---------------------------------------------------------------------------
# 
# ---------------------------------------------------------------------------
@export_subgroup("Motion — Duration (seconds)")
@export var DURATION_INSTANT     := 0.08
@export var DURATION_FAST        := 0.16
@export var DURATION_NORMAL      := 0.24
@export var DURATION_SLOW        := 0.40
@export var DURATION_ATMOSPHERIC := 1.00


# ---------------------------------------------------------------------------
# 
# ---------------------------------------------------------------------------
@export_subgroup("Component Constants (px)")
@export var SIDEBAR_WIDTH           := 240
@export var SIDEBAR_WIDTH_COLLAPSED := 68
@export var NAV_BAR_HEIGHT_DESKTOP  := 64
@export var NAV_BAR_HEIGHT_MOBILE   := 56
@export var MINI_PLAYER_HEIGHT      := 72
@export var TOUCH_TARGET_MIN        := 44


# ---------------------------------------------------------------------------
#  — SystemFont with priority fallback chains.
## TODO: Replace SystemFont with FontFile once Inter, Outfit, and
##       JetBrains Mono TTFs are added to res://fonts/.
# ---------------------------------------------------------------------------
@export_subgroup("Fonts")
## Inter — primary UI typeface.
@export var font_ui: Font
## Outfit — display / title typeface.
@export var font_display: Font
## JetBrains Mono — BPM, key labels, timestamps, technical metadata.
@export var font_mono: Font

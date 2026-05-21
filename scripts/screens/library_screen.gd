## library_screen.gd
## Main music library browser screen.
## MVP: polished empty state with a CTA to add music.
## Phase 2: grid / list of imported tracks, albums, and playlists.
extends Control

@onready var icon_label:    Label  = $Center/EmptyState/IconLabel
@onready var heading:       Label  = $Center/EmptyState/HeadingLabel
@onready var body:          Label  = $Center/EmptyState/BodyLabel
@onready var add_music_btn: Button = $Center/EmptyState/BtnRow/AddMusicBtn


func _ready() -> void:
	_apply_styles()


# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_styles() -> void:
	# Large centred music-note icon (80 px, tertiary colour per design language).
	icon_label.add_theme_color_override("font_color", ThemeManager.TEXT_TERTIARY)
	icon_label.add_theme_font_size_override("font_size", 80)

	# Empty-state heading — Outfit display font, type-lg.
	heading.add_theme_color_override("font_color", ThemeManager.TEXT_PRIMARY)
	heading.add_theme_font_override("font", ThemeManager.font_display)
	heading.add_theme_font_size_override("font_size", ThemeManager.TYPE_LG)

	# Body copy — Inter, type-sm, secondary colour.
	body.add_theme_color_override("font_color", ThemeManager.TEXT_SECONDARY)
	body.add_theme_font_override("font", ThemeManager.font_ui)
	body.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)

	# CTA button — glass border at rest, filled accent on hover.
	add_music_btn.add_theme_stylebox_override("normal",  ThemeManager.make_cta_button(false))
	add_music_btn.add_theme_stylebox_override("hover",   ThemeManager.make_cta_button(true))
	add_music_btn.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	add_music_btn.add_theme_stylebox_override("focus",   ThemeManager.make_transparent())
	add_music_btn.add_theme_color_override("font_color",       ThemeManager.ACCENT_BRIGHT)
	add_music_btn.add_theme_color_override("font_hover_color", ThemeManager.TEXT_INVERSE)
	add_music_btn.add_theme_font_override("font", ThemeManager.font_ui)
	add_music_btn.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)

	add_music_btn.pressed.connect(_on_add_music_pressed)


# ---------------------------------------------------------------------------
# Handlers
# ---------------------------------------------------------------------------

func _on_add_music_pressed() -> void:
	# TODO (phase 2): Open native file-picker via DisplayServer or a GDNative plugin.
	pass

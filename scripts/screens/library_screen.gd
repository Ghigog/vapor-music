## library_screen.gd
## Main music library browser screen.
## Shows a loading state while WebDAV is scanning, a populated track list on
## success, or an empty state CTA if no audio files are found.
extends Control

@onready var icon_label:    Label           = $Center/EmptyState/IconLabel
@onready var heading:       Label           = $Center/EmptyState/HeadingLabel
@onready var body:          Label           = $Center/EmptyState/BodyLabel
@onready var add_music_btn: Button          = $Center/EmptyState/BtnRow/AddMusicBtn
@onready var center_panel:  CenterContainer = $Center
@onready var status_label:  Label           = %StatusLabel
@onready var track_scroll:  ScrollContainer = %TrackScroll
@onready var track_list:    VBoxContainer   = %TrackList


func _ready() -> void:
	_apply_styles()
	WebDAVService.library_scanned.connect(_on_library_scanned)


# ---------------------------------------------------------------------------
# Public API — called by main.gd
# ---------------------------------------------------------------------------

## Shows a "Scanning…" message and hides other panels.
func show_loading() -> void:
	center_panel.visible = false
	track_scroll.visible = false
	status_label.text    = "Scanning your library…"
	status_label.add_theme_color_override("font_color", ThemeManager.TEXT_SECONDARY)
	status_label.visible = true


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

	# Style the status label
	status_label.add_theme_font_override("font", ThemeManager.font_ui)
	status_label.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)


# ---------------------------------------------------------------------------
# Signal Handlers
# ---------------------------------------------------------------------------

## Receives the scanned file list from WebDAVService.
func _on_library_scanned(files: Array) -> void:
	status_label.visible = false

	if files.is_empty():
		center_panel.visible = true
		track_scroll.visible = false
		return

	# Clear previous results.
	for child in track_list.get_children():
		child.queue_free()

	# Populate track rows.
	for href: String in files:
		var label := Label.new()
		label.text = href.get_file()  # Show just the filename, not the full path.
		label.add_theme_font_override("font", ThemeManager.font_ui)
		label.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)
		label.add_theme_color_override("font_color", ThemeManager.TEXT_PRIMARY)
		track_list.add_child(label)

	center_panel.visible = false
	track_scroll.visible = true


func _on_add_music_pressed() -> void:
	# TODO (phase 2): Open native file-picker or re-open wizard.
	pass

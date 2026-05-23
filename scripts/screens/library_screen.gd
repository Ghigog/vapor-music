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
	
	# Ensure this main root screen node fills up the entire right-hand viewport panel
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	
	if SettingsManager.has_credentials():
		show_loading()
		var active_folder: String = SettingsManager.webdav_folder if "webdav_folder" in SettingsManager else "Music"
		WebDAVService.scan_music_directory(active_folder)

# ---------------------------------------------------------------------------
# Public API — called by main.gd
# ---------------------------------------------------------------------------

## Shows a "Scanning…" message and hides other panels.
func show_loading() -> void:
	center_panel.visible = false
	track_scroll.visible = false
	
	# FIX 1: Hard-center the status label dynamically so it doesn't float top-left
	status_label.text = "Scanning your library…"
	status_label.add_theme_color_override("font_color", ThemeManager.TEXT_SECONDARY)
	status_label.visible = true
	status_label.set_anchors_and_offsets_preset(Control.PRESET_CENTER)

# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_styles() -> void:
	icon_label.add_theme_color_override("font_color", ThemeManager.TEXT_TERTIARY)
	icon_label.add_theme_font_size_override("font_size", 80)

	heading.add_theme_color_override("font_color", ThemeManager.TEXT_PRIMARY)
	heading.add_theme_font_override("font", ThemeManager.font_display)
	heading.add_theme_font_size_override("font_size", ThemeManager.TYPE_LG)

	body.add_theme_color_override("font_color", ThemeManager.TEXT_SECONDARY)
	body.add_theme_font_override("font", ThemeManager.font_ui)
	body.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)

	add_music_btn.add_theme_stylebox_override("normal",  ThemeManager.make_cta_button(false))
	add_music_btn.add_theme_stylebox_override("hover",   ThemeManager.make_cta_button(true))
	add_music_btn.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	add_music_btn.add_theme_stylebox_override("focus",   ThemeManager.make_transparent())
	add_music_btn.add_theme_color_override("font_color",       ThemeManager.ACCENT_BRIGHT)
	add_music_btn.add_theme_color_override("font_hover_color", ThemeManager.TEXT_INVERSE)
	add_music_btn.add_theme_font_override("font", ThemeManager.font_ui)
	add_music_btn.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)

	if not add_music_btn.pressed.is_connected(_on_add_music_pressed):
		add_music_btn.pressed.connect(_on_add_music_pressed)

	status_label.add_theme_font_override("font", ThemeManager.font_ui)
	status_label.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)

# ---------------------------------------------------------------------------
# Signal Handlers
# ---------------------------------------------------------------------------

## Receives the scanned file list from WebDAVService.
func _on_library_scanned(files: Array) -> void:
	status_label.visible = false

	if files.is_empty():
		var active_folder: String = SettingsManager.webdav_folder if "webdav_folder" in SettingsManager else "Music"
		heading.text = "No Tracks Found"
		body.text = "Your WebDAV directory '%s' is empty or has no loose audio files." % active_folder
		center_panel.visible = true
		track_scroll.visible = false
		return

	# Clear previous track list rows
	for child in track_list.get_children():
		child.queue_free()

	# Populate track rows
	for href: String in files:
		var label := Label.new()
		label.text = href.get_file().uri_decode()
		
		# Apply typography styling match rules
		label.add_theme_font_override("font", ThemeManager.font_ui)
		label.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)
		label.add_theme_color_override("font_color", ThemeManager.TEXT_PRIMARY)
		
		# Explicitly tell the label node to expand and occupy its horizontal alignment block
		label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		
		track_list.add_child(label)

	# --- FIX 2: FORCE EXPAND SCROLL CONTAINERS TO OCCUPY SPACE ---
	center_panel.visible = false
	
	# Force both containers to explicitly stretch across the workspace panel space
	track_scroll.visible = true
	track_scroll.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	track_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	track_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	
	track_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	track_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	
	# Force an instant layout redraw right now
	track_list.queue_sort()

func _on_add_music_pressed() -> void:
	if SettingsManager.has_credentials():
		show_loading()
		var active_folder: String = SettingsManager.webdav_folder if "webdav_folder" in SettingsManager else "Music"
		WebDAVService.scan_music_directory(active_folder)
	else:
		body.text = "Please open the connection menu or main configuration wizard to link a WebDAV provider."

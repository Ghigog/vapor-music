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
	ThemeManager.theme_changed.connect(_apply_styles)
	
	# Safe connection layout wrapper
	if not WebDAVService.library_scanned.is_connected(_on_library_scanned):
		WebDAVService.library_scanned.connect(_on_library_scanned)
	
	# Set our viewport dimension bounds 
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	
	# REMOVED: WebDAVService.scan_music_directory() is gone from here!
	# We let our UI buttons and main.gd make the explicit network calls cleanly.
# ---------------------------------------------------------------------------
# Public API — called by main.gd
# ---------------------------------------------------------------------------

## Shows a "Scanning…" message and hides other panels.
func show_loading() -> void:
	center_panel.visible = false
	track_scroll.visible = false
	
	# FIX 1: Hard-center the status label dynamically so it doesn't float top-left
	status_label.text = "Scanning your library…"
	status_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	status_label.visible = true
	status_label.set_anchors_and_offsets_preset(Control.PRESET_CENTER)

# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_styles() -> void:
	icon_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
	icon_label.add_theme_font_size_override("font_size", 80)

	heading.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	heading.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	heading.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_LG)

	body.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	body.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	body.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

	add_music_btn.add_theme_stylebox_override("normal",  ThemeManager.make_cta_button(false))
	add_music_btn.add_theme_stylebox_override("hover",   ThemeManager.make_cta_button(true))
	add_music_btn.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	add_music_btn.add_theme_stylebox_override("focus",   ThemeManager.make_transparent())
	add_music_btn.add_theme_color_override("font_color",       ThemeManager.current_theme.ACCENT_BRIGHT)
	add_music_btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.TEXT_INVERSE)
	add_music_btn.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	add_music_btn.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

	if not add_music_btn.pressed.is_connected(_on_add_music_pressed):
		add_music_btn.pressed.connect(_on_add_music_pressed)

	status_label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	status_label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

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

	# Populate track rows using Flat interactive buttons
	for href: String in files:
		var raw_filename := href.get_file().uri_decode()
		
		# Strip off the ".mp3" or ".flac" extensions cleanly
		var display_name := raw_filename.get_basename()
		
		# OPTIONAL CLEANUP: Remove "Vanilla - Origin - " if you just want song titles in the list
		if " - " in display_name:
			var parts := display_name.split(" - ")
			# Looks for a structure like ["Vanilla", "Origin", "01 Past..."]
			if parts.size() >= 3:
				display_name = parts[2] # Just show the track number + song name
			else:
				display_name = parts[parts.size() - 1]

		var track_btn := Button.new()
		track_btn.text = "   ▶   " + display_name
		track_btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
		track_btn.flat = true
		
		# Standard layout typography constraints
		track_btn.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		track_btn.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
		track_btn.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
		track_btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.ACCENT_BRIGHT)
		
		# Pipe track context into play mechanics on click
		track_btn.pressed.connect(
			func(): AudioManager.play_track(href, files)
		)
		
		track_btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		track_list.add_child(track_btn)
	
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

## library_screen.gd
## Main music library browser screen.
## Shows a loading state while WebDAV is scanning, a populated track list on
## success, or an empty state CTA if no audio files are found.
extends Control

const LIBRARY_ROW_SCENE = preload("res://scenes/screens/library/library_row.tscn")


@onready var icon_label:    Label           = $Center/EmptyState/IconLabel
@onready var heading:       Label           = $Center/EmptyState/HeadingLabel
@onready var body:          Label           = $Center/EmptyState/BodyLabel
@onready var add_music_btn: Button          = $Center/EmptyState/BtnRow/AddMusicBtn
@onready var center_panel:  CenterContainer = $Center
@onready var status_label:  Label           = %StatusLabel
@onready var track_scroll:  ScrollContainer = %TrackScroll
@onready var track_list:    VBoxContainer   = %TrackList

var _scanned_files: Array = []

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

	if not _scanned_files.is_empty():
		_rebuild_tree()

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
		_scanned_files = []
		return

	_scanned_files = files
	_rebuild_tree()

## Clears and re-renders the full Artist/Album/Track tree using current theme colours.
## Called both from _on_library_scanned and from _apply_styles on theme change.
func _rebuild_tree() -> void:
	for child in track_list.get_children():
		child.queue_free()

	# Organise files into Artist -> Album -> Track
	var library_tree := {}
	for href: String in _scanned_files:
		var info := _parse_track_info(href)
		var artist: String = info.artist
		var album: String  = info.album
		var track: String  = info.track
		if not library_tree.has(artist):
			library_tree[artist] = {}
		if not library_tree[artist].has(album):
			library_tree[artist][album] = []
		library_tree[artist][album].append({"href": href, "title": track})

	var artists: Array = library_tree.keys()
	artists.sort_custom(func(a: String, b: String) -> bool: return a.to_lower() < b.to_lower())

	for artist_name: String in artists:
		var artist_container := VBoxContainer.new()
		artist_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		track_list.add_child(artist_container)

		# --- Artist row: transparent Button (hitbox only) + Label (rendered text) ---
		var artist_row := _make_row_button(
			"▶  👤  " + artist_name,
			4,  # left padding units
			ThemeManager.current_theme.font_display,
			ThemeManager.current_theme.TYPE_MD,
			ThemeManager.current_theme.TEXT_PRIMARY
		)
		artist_container.add_child(artist_row.container)

		var albums_container := VBoxContainer.new()
		albums_container.visible = false
		albums_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		artist_container.add_child(albums_container)

		artist_row.button.pressed.connect(func() -> void:
			var expanded: bool = not albums_container.visible
			albums_container.visible = expanded
			artist_row.label.text = ("▼  👤  " if expanded else "▶  👤  ") + artist_name
			MetadataService.focus_artist(artist_name)
		)

		var albums_map: Dictionary = library_tree[artist_name]
		var albums: Array = albums_map.keys()
		albums.sort_custom(func(a: String, b: String) -> bool: return a.to_lower() < b.to_lower())

		for album_name: String in albums:
			var album_sub_container := VBoxContainer.new()
			album_sub_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			albums_container.add_child(album_sub_container)

			# --- Album row ---
			var album_row := _make_row_button(
				"▶  💿  " + album_name,
				12,
				ThemeManager.current_theme.font_ui,
				ThemeManager.current_theme.TYPE_SM,
				ThemeManager.current_theme.TEXT_SECONDARY
			)
			album_sub_container.add_child(album_row.container)

			var tracks_container := VBoxContainer.new()
			tracks_container.visible = false
			tracks_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			album_sub_container.add_child(tracks_container)

			album_row.button.pressed.connect(func() -> void:
				var expanded: bool = not tracks_container.visible
				tracks_container.visible = expanded
				album_row.label.text = ("▼  💿  " if expanded else "▶  💿  ") + album_name
				MetadataService.focus_album(artist_name, album_name)
			)

			var tracks: Array = albums_map[album_name]
			tracks.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
				return (a.title as String).to_lower() < (b.title as String).to_lower()
			)

			for track_info: Dictionary in tracks:
				var track_row := _make_row_button(
					"♫  " + (track_info.title as String),
					22,
					ThemeManager.current_theme.font_ui,
					ThemeManager.current_theme.TYPE_SM,
					ThemeManager.current_theme.TEXT_SECONDARY,
					true,
					track_info.href as String,
					track_info.title as String
				)
				track_row.button.pressed.connect(func() -> void:
					AudioManager.play_track(track_info.href as String, _scanned_files)
					MetadataService.focus_track(track_info.href as String, artist_name, album_name, track_info.title as String)
				)
				# Highlight label on hover via button's mouse_entered / mouse_exited signals
				track_row.button.mouse_entered.connect(func() -> void:
					track_row.label.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
				)
				track_row.button.mouse_exited.connect(func() -> void:
					track_row.label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
				)
				tracks_container.add_child(track_row.container)

	center_panel.visible = false
	track_scroll.visible = true
	track_scroll.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	track_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	track_scroll.size_flags_vertical   = Control.SIZE_EXPAND_FILL
	track_list.size_flags_horizontal   = Control.SIZE_EXPAND_FILL
	track_list.size_flags_vertical     = Control.SIZE_EXPAND_FILL
	track_list.queue_sort()

## Builds a row consisting of a full-width transparent Button (as the hitbox) and
## a sibling Label rendered on top, avoiding the visual artifacts that flat Buttons
## produce over transparent backgrounds. Returns a Dictionary with keys:
##   container (HBoxContainer), button (Button), label (Label)
func _make_row_button(
	text: String,
	left_padding: int,
	font: Font,
	font_size: int,
	color: Color,
	is_track: bool = false,
	track_href: String = "",
	track_title: String = ""
) -> Dictionary:
	var row = LIBRARY_ROW_SCENE.instantiate()
	row.add_theme_constant_override("margin_left", left_padding * 4)
	row.add_theme_constant_override("margin_top", 2)
	row.add_theme_constant_override("margin_right", 8)
	row.add_theme_constant_override("margin_bottom", 2)
	
	var btn = row.get_node("Stack/Button") as Button
	var lbl = row.get_node("Stack/Label") as Label
	
	if is_track:
		btn.set_script(preload("res://scripts/screens/track_drag_button.gd"))
		btn.href = track_href
		btn.track_title = track_title
		
	btn.flat = true
	btn.text = ""
	btn.add_theme_stylebox_override("normal",   ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("hover",    ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("pressed",  ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("focus",    ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("disabled", ThemeManager.make_transparent())
	
	lbl.text = text
	lbl.vertical_alignment    = VERTICAL_ALIGNMENT_CENTER
	lbl.add_theme_font_override("font", font)
	lbl.add_theme_font_size_override("font_size", font_size)
	lbl.add_theme_color_override("font_color", color)
	
	return {"container": row, "button": btn, "label": lbl}

## Helper to identify numeric/alphanumeric track number prefixes (e.g., "01", "1", "A1", "1-01")
func _is_track_number_prefix(s: String) -> bool:
	var clean := s.strip_edges()
	if clean.is_valid_int():
		return true
	var regex := RegEx.new()
	regex.compile("^[A-Za-z]?\\d+[-a-zA-Z]?$")
	var match_obj := regex.search(clean)
	return match_obj != null

func _parse_track_info(href: String) -> Dictionary:
	if is_instance_valid(MetadataService):
		return MetadataService.parse_track_info(href)
		
	# Fallback in case MetadataService is not loaded/valid
	var raw_filename := href.get_file().uri_decode()
	return {
		"artist": "Unknown Artist",
		"album": "Unknown Album",
		"track": raw_filename.get_basename()
	}


func _on_add_music_pressed() -> void:
	if SettingsManager.has_credentials():
		show_loading()
		var active_folder: String = SettingsManager.webdav_folder if "webdav_folder" in SettingsManager else "Music"
		WebDAVService.scan_music_directory(active_folder)
	else:
		body.text = "Please open the connection menu or main configuration wizard to link a WebDAV provider."

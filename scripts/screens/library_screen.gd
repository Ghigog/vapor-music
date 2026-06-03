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
					ThemeManager.current_theme.TEXT_SECONDARY
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
	color: Color
) -> Dictionary:
	# MarginContainer provides indentation for the row.
	var row := MarginContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("margin_left", left_padding * 4)
	row.add_theme_constant_override("margin_top", 2)
	row.add_theme_constant_override("margin_right", 8)
	row.add_theme_constant_override("margin_bottom", 2)

	# Inner Control stacks Button (hitbox) and Label (display) using PRESET_FULL_RECT
	# so both fill the same area. The Label sits on top of the invisible Button.
	var stack := Control.new()
	stack.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	stack.custom_minimum_size   = Vector2(0, 28)

	var btn := Button.new()
	btn.flat = true
	btn.text = ""  # Text is on the Label, not the Button.
	btn.add_theme_stylebox_override("normal",   ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("hover",    ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("pressed",  ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("focus",    ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("disabled", ThemeManager.make_transparent())
	btn.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var lbl := Label.new()
	lbl.text = text
	lbl.vertical_alignment    = VERTICAL_ALIGNMENT_CENTER
	lbl.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	lbl.mouse_filter          = Control.MOUSE_FILTER_IGNORE  # Pass clicks through to Button
	lbl.add_theme_font_override("font", font)
	lbl.add_theme_font_size_override("font_size", font_size)
	lbl.add_theme_color_override("font_color", color)
	lbl.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	stack.add_child(btn)
	stack.add_child(lbl)
	row.add_child(stack)

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

## Smart metadata parser that handles structured filenames and path/directory fallbacks
func _parse_track_info(href: String) -> Dictionary:
	# Check metadata service cache first
	if is_instance_valid(MetadataService):
		var cached := MetadataService.get_cached_metadata(href)
		if not cached.is_empty():
			var cached_artist: String = cached.get("artist_name", "")
			var cached_album: String = cached.get("album_name", "")
			var cached_track: String = cached.get("track_title", "")
			if not cached_artist.is_empty() and cached_artist != "Unknown Artist" \
					and not cached_album.is_empty() and cached_album != "Unknown Album":
				return {
					"artist": cached_artist,
					"album": cached_album,
					"track": cached_track if not cached_track.is_empty() else href.get_file().uri_decode().get_basename()
				}

	var raw_filename := href.get_file().uri_decode()
	var display_name := raw_filename.get_basename()
	
	var file_artist := ""
	var file_album := ""
	var file_track := display_name
	
	# Parse path segments for directory structure fallback
	var decoded_path := href.uri_decode()
	var path_segments := []
	for segment in decoded_path.split("/"):
		if not segment.is_empty():
			path_segments.append(segment)
			
	var base_folder := "Music"
	if SettingsManager.has_credentials() and "webdav_folder" in SettingsManager:
		base_folder = SettingsManager.webdav_folder
		
	var relative_start := -1
	for i in range(path_segments.size()):
		if path_segments[i].to_lower() == base_folder.to_lower():
			relative_start = i + 1
			break
			
	var relative_segments := []
	if relative_start != -1 and relative_start < path_segments.size():
		relative_segments = path_segments.slice(relative_start, path_segments.size() - 1)
	else:
		if path_segments.size() >= 2:
			relative_segments = path_segments.slice(0, path_segments.size() - 1)
			
	var folder_artist := ""
	var folder_album := ""
	
	if relative_segments.size() >= 2:
		folder_artist = relative_segments[0].strip_edges()
		folder_album = relative_segments[1].strip_edges()
	elif relative_segments.size() == 1:
		var seg: String = relative_segments[0].strip_edges()
		var clean_seg := seg.replace("–", "-").replace("—", "-")
		if " - " in clean_seg:
			var parts: PackedStringArray = clean_seg.split(" - ")
			if parts.size() == 2:
				folder_artist = parts[0].strip_edges()
				folder_album = parts[1].strip_edges()
			else:
				folder_album = seg
		else:
			folder_album = seg

	# Parse structured filename "Artist - Album - Track" or "Artist - Track"
	var clean_display := display_name.replace("–", "-").replace("—", "-")
	if " - " in clean_display:
		var raw_parts := clean_display.split(" - ")
		# If the first part is a track number, remove it to normalize parsing
		if raw_parts.size() > 1 and _is_track_number_prefix(raw_parts[0]):
			raw_parts.remove_at(0)
			
		if raw_parts.size() >= 3:
			file_artist = raw_parts[0].strip_edges()
			file_album = raw_parts[1].strip_edges()
			var track_parts := []
			for i in range(2, raw_parts.size()):
				track_parts.append(raw_parts[i])
			file_track = " - ".join(track_parts).strip_edges()
		elif raw_parts.size() == 2:
			file_artist = raw_parts[0].strip_edges()
			file_track = raw_parts[1].strip_edges()
		elif raw_parts.size() == 1:
			file_track = raw_parts[0].strip_edges()

	# Combine information with sensible priority: file-parsed first, then folder-derived.
	var artist := "Unknown Artist"
	if not file_artist.is_empty():
		artist = file_artist
	elif not folder_artist.is_empty():
		artist = folder_artist
		
	var album := "Unknown Album"
	if not file_album.is_empty():
		album = file_album
	elif not folder_album.is_empty():
		album = folder_album
		
	return {
		"artist": artist,
		"album": album,
		"track": file_track.strip_edges()
	}

func _on_add_music_pressed() -> void:
	if SettingsManager.has_credentials():
		show_loading()
		var active_folder: String = SettingsManager.webdav_folder if "webdav_folder" in SettingsManager else "Music"
		WebDAVService.scan_music_directory(active_folder)
	else:
		body.text = "Please open the connection menu or main configuration wizard to link a WebDAV provider."

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

## Row roles — determine which theme tokens a row's label uses.
const ROLE_ARTIST := "artist"
const ROLE_ALBUM  := "album"
const ROLE_TRACK  := "track"

## Artist → Album → [ {href, title} ]. Built once per scan by _build_data_model().
var _library_tree: Dictionary = {}

## Artist names, pre-sorted case-insensitively.
var _sorted_artists: Array = []

## Every row label built so far, as {label: Label, role: String}. Lets a theme
## change restyle in place instead of rebuilding the tree.
var _styled_rows: Array[Dictionary] = []

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

	# Restyle existing rows rather than rebuilding the tree — a theme change is
	# not a structural change, and rebuilding would discard the user's expansion
	# state along with every node.
	_restyle_rows()

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


## Groups the scanned file list into the Artist → Album → Track data model.
## Pure data — builds no nodes. Sorting happens here so the builders don't repeat it.
func _build_data_model() -> void:
	_library_tree.clear()

	for href: String in _scanned_files:
		var info := _parse_track_info(href)
		var artist: String = info.artist
		var album: String  = info.album
		var track: String  = info.track
		if not _library_tree.has(artist):
			_library_tree[artist] = {}
		if not _library_tree[artist].has(album):
			_library_tree[artist][album] = []
		_library_tree[artist][album].append({"href": href, "title": track})

	_sorted_artists = _library_tree.keys()
	_sorted_artists.sort_custom(func(a: String, b: String) -> bool: return a.to_lower() < b.to_lower())


## Renders the library as a lazily-expanded Artist → Album → Track tree.
##
## Only artist rows are built up front. Album rows are built the first time an
## artist is expanded, track rows the first time an album is expanded. Building
## the whole tree eagerly meant a 5,000-track library constructed tens of
## thousands of nodes and ~15,000 signal connections in a single frame — all of
## it invisible, since albums and tracks start collapsed.
##
## Called only when the underlying file list changes. Theme changes go through
## _restyle_rows() instead, which does not touch the node structure.
func _rebuild_tree() -> void:
	for child in track_list.get_children():
		child.queue_free()
	_styled_rows.clear()

	_build_data_model()

	for artist_name: String in _sorted_artists:
		var artist_container := VBoxContainer.new()
		artist_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		track_list.add_child(artist_container)

		# --- Artist row: transparent Button (hitbox only) + Label (rendered text) ---
		var artist_row := _make_row_button("▶  👤  " + artist_name, 4, ROLE_ARTIST)
		artist_container.add_child(artist_row.container)

		var albums_container := VBoxContainer.new()
		albums_container.visible = false
		albums_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		# Expansion state lives on the node, not in a captured local — GDScript
		# lambdas capture locals by value, so a captured bool would never update.
		albums_container.set_meta("built", false)
		artist_container.add_child(albums_container)

		artist_row.button.pressed.connect(func() -> void:
			var expanded: bool = not albums_container.visible
			if expanded and not albums_container.get_meta("built"):
				albums_container.set_meta("built", true)
				_build_albums(artist_name, albums_container)
			albums_container.visible = expanded
			artist_row.label.text = ("▼  👤  " if expanded else "▶  👤  ") + artist_name
			MetadataService.focus_artist(artist_name)
		)

	center_panel.visible = false
	track_scroll.visible = true
	track_scroll.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	track_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	track_scroll.size_flags_vertical   = Control.SIZE_EXPAND_FILL
	track_list.size_flags_horizontal   = Control.SIZE_EXPAND_FILL
	track_list.size_flags_vertical     = Control.SIZE_EXPAND_FILL
	track_list.queue_sort()


## Builds the album rows for one artist. Called once, on first expand.
func _build_albums(artist_name: String, albums_container: VBoxContainer) -> void:
	var albums_map: Dictionary = _library_tree.get(artist_name, {})
	var albums: Array = albums_map.keys()
	albums.sort_custom(func(a: String, b: String) -> bool: return a.to_lower() < b.to_lower())

	for album_name: String in albums:
		var album_sub_container := VBoxContainer.new()
		album_sub_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		albums_container.add_child(album_sub_container)

		var album_row := _make_row_button("▶  💿  " + album_name, 12, ROLE_ALBUM)
		album_sub_container.add_child(album_row.container)

		var tracks_container := VBoxContainer.new()
		tracks_container.visible = false
		tracks_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		tracks_container.set_meta("built", false)
		album_sub_container.add_child(tracks_container)

		album_row.button.pressed.connect(func() -> void:
			var expanded: bool = not tracks_container.visible
			if expanded and not tracks_container.get_meta("built"):
				tracks_container.set_meta("built", true)
				_build_tracks(artist_name, album_name, tracks_container)
			tracks_container.visible = expanded
			album_row.label.text = ("▼  💿  " if expanded else "▶  💿  ") + album_name
			MetadataService.focus_album(artist_name, album_name)
		)


## Builds the track rows for one album. Called once, on first expand.
func _build_tracks(artist_name: String, album_name: String, tracks_container: VBoxContainer) -> void:
	var tracks: Array = _library_tree.get(artist_name, {}).get(album_name, [])
	tracks.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return (a.title as String).to_lower() < (b.title as String).to_lower()
	)

	for track_info: Dictionary in tracks:
		var track_row := _make_row_button(
			"♫  " + (track_info.title as String),
			22,
			ROLE_TRACK,
			true,
			track_info.href as String,
			track_info.title as String
		)
		track_row.button.pressed.connect(func() -> void:
			AudioManager.play_track(track_info.href as String, _scanned_files)
			MetadataService.focus_track(track_info.href as String, artist_name, album_name, track_info.title as String)
		)
		# Highlight label on hover. These read ThemeManager at call time, so they
		# stay correct across theme changes without needing to be reconnected.
		track_row.button.mouse_entered.connect(func() -> void:
			track_row.label.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
		)
		track_row.button.mouse_exited.connect(func() -> void:
			track_row.label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
		)
		tracks_container.add_child(track_row.container)


## Returns the font / size / colour triple for a row role under the active theme.
func _style_for_role(role: String) -> Dictionary:
	var theme = ThemeManager.current_theme
	match role:
		ROLE_ARTIST:
			return {"font": theme.font_display, "size": theme.TYPE_MD, "color": theme.TEXT_PRIMARY}
		_:
			# Albums and tracks share the secondary treatment.
			return {"font": theme.font_ui, "size": theme.TYPE_SM, "color": theme.TEXT_SECONDARY}


## Reapplies theme tokens to every row already built.
##
## Replaces the previous behaviour, where a theme change called _rebuild_tree()
## and tore down the entire library — including every expanded section the user
## had opened. Restyling in place is both far cheaper and preserves that state.
func _restyle_rows() -> void:
	var live_rows: Array[Dictionary] = []
	for entry: Dictionary in _styled_rows:
		var lbl: Label = entry.label
		if not is_instance_valid(lbl):
			continue
		var style := _style_for_role(entry.role)
		lbl.add_theme_font_override("font", style.font)
		lbl.add_theme_font_size_override("font_size", style.size)
		lbl.add_theme_color_override("font_color", style.color)
		live_rows.append(entry)
	_styled_rows = live_rows

## Builds a row consisting of a full-width transparent Button (as the hitbox) and
## a sibling Label rendered on top, avoiding the visual artifacts that flat Buttons
## produce over transparent backgrounds. Returns a Dictionary with keys:
##   container (HBoxContainer), button (Button), label (Label)
func _make_row_button(
	text: String,
	left_padding: int,
	role: String,
	is_track: bool = false,
	track_href: String = "",
	track_title: String = ""
) -> Dictionary:
	var row = LIBRARY_ROW_SCENE.instantiate()
	row.add_theme_constant_override("margin_left", left_padding * 4)
	row.add_theme_constant_override("margin_top", 2)
	row.add_theme_constant_override("margin_right", 8)
	row.add_theme_constant_override("margin_bottom", 2)
	
	var stack = row.get_node("Stack") as Control
	var btn = row.get_node("Stack/Button") as Button
	var lbl = row.get_node("Stack/Label") as Label

	# The scene bakes a 28 px row — fine for a mouse, too small for a fingertip.
	# On touch hardware this lifts to TOUCH_TARGET_MIN. The library is the app's
	# densest tap surface, so it's the one that matters most.
	stack.custom_minimum_size.y = ThemeManager.min_touch_height(int(stack.custom_minimum_size.y))

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
	lbl.vertical_alignment = VERTICAL_ALIGNMENT_CENTER

	var style := _style_for_role(role)
	lbl.add_theme_font_override("font", style.font)
	lbl.add_theme_font_size_override("font_size", style.size)
	lbl.add_theme_color_override("font_color", style.color)

	# Register for in-place restyling on theme change.
	_styled_rows.append({"label": lbl, "role": role})

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

## library_screen.gd
## Main music library browser screen — a thin host around TrackTable.
##
## Owns the scan lifecycle (loading / empty / populated) and the TrackIndex;
## all rendering, filtering, grouping, and sorting live in TrackTable, the
## same component the playlist screen hosts in manual mode.
extends Control

const TrackIndex = preload("res://scripts/services/track_index.gd")
const TrackTable = preload("res://scripts/ui/track_table.gd")
const GlassModal = preload("res://scripts/ui/glass_modal.gd")
const ICON_LIBRARY := preload("res://assets/icon/library-cropped.png")

@onready var icon_label:    TextureRect     = $Center/EmptyState/IconLabel
@onready var heading:       Label           = $Center/EmptyState/HeadingLabel
@onready var body:          Label           = $Center/EmptyState/BodyLabel
@onready var add_music_btn: Button          = $Center/EmptyState/BtnRow/AddMusicBtn
@onready var center_panel:  CenterContainer = $Center
@onready var status_label:  Label           = %StatusLabel
@onready var layout:        VBoxContainer   = %Layout

var _scanned_files: Array = []
var _index := TrackIndex.new()
var _table: VBoxContainer


func _ready() -> void:
	_table = TrackTable.new()
	_table.show_save_view = true
	_table.state_key = "library"
	layout.add_child(_table)
	_table.play_requested.connect(_on_table_play_requested)
	_table.save_selection_requested.connect(_on_save_selection_requested)
	_table.artist_focused.connect(func(artist_name: String) -> void:
		if is_instance_valid(MetadataService):
			MetadataService.focus_artist(artist_name)
	)
	_table.album_focused.connect(func(album_name: String) -> void:
		if is_instance_valid(MetadataService):
			# focus_album scans the cache by album name; the artist parameter
			# only matters for the network fallback, so empty is fine here.
			MetadataService.focus_album("", album_name)
	)

	_apply_styles()
	ThemeManager.theme_changed.connect(_apply_styles)

	if not WebDAVService.library_scanned.is_connected(_on_library_scanned):
		WebDAVService.library_scanned.connect(_on_library_scanned)

	var ms = get_node_or_null("/root/MetadataService")
	if ms and ms.has_signal("metadata_updated"):
		ms.metadata_updated.connect(_on_metadata_updated)

	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)


# ---------------------------------------------------------------------------
# Public API — called by main.gd
# ---------------------------------------------------------------------------

## Shows a "Scanning…" message and hides other panels.
func show_loading() -> void:
	center_panel.visible = false
	layout.visible = false
	status_label.text = "Scanning your library…"
	status_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	status_label.visible = true
	status_label.set_anchors_and_offsets_preset(Control.PRESET_CENTER)


# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_styles() -> void:
	var theme = ThemeManager.current_theme

	icon_label.texture = ICON_LIBRARY
	icon_label.self_modulate = theme.TEXT_TERTIARY

	heading.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	heading.add_theme_font_override("font", theme.font_display)
	heading.add_theme_font_size_override("font_size", theme.TYPE_LG)

	body.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	body.add_theme_font_override("font", theme.font_ui)
	body.add_theme_font_size_override("font_size", theme.TYPE_SM)

	add_music_btn.add_theme_stylebox_override("normal",  ThemeManager.make_cta_button(false))
	add_music_btn.add_theme_stylebox_override("hover",   ThemeManager.make_cta_button(true))
	add_music_btn.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	add_music_btn.add_theme_stylebox_override("focus",   ThemeManager.make_transparent())
	add_music_btn.add_theme_color_override("font_color",       theme.ACCENT_BRIGHT)
	add_music_btn.add_theme_color_override("font_hover_color", theme.TEXT_INVERSE)
	add_music_btn.add_theme_font_override("font", theme.font_ui)
	add_music_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)

	if not add_music_btn.pressed.is_connected(_on_add_music_pressed):
		add_music_btn.pressed.connect(_on_add_music_pressed)

	status_label.add_theme_font_override("font", theme.font_ui)
	status_label.add_theme_font_size_override("font_size", theme.TYPE_SM)


# ---------------------------------------------------------------------------
# Signal handlers
# ---------------------------------------------------------------------------

func _on_library_scanned(files: Array) -> void:
	status_label.visible = false

	if files.is_empty():
		var active_folder: String = SettingsManager.webdav_folder if "webdav_folder" in SettingsManager else "Music"
		heading.text = "No Tracks Found"
		body.text = "Your WebDAV directory '%s' is empty or has no loose audio files." % active_folder
		center_panel.visible = true
		layout.visible = false
		_scanned_files = []
		return

	_scanned_files = files
	_index.build(files)
	center_panel.visible = false
	layout.visible = true
	_table.set_rows(_index.rows.duplicate())


func _on_metadata_updated(href: String, _meta: Dictionary) -> void:
	var fresh := _index.refresh(href)
	if not fresh.is_empty():
		_table.refresh_row(fresh)


## The ✓ of selection mode: name the ticked tracks and freeze them, in view
## order, into a normal playlist.
func _on_save_selection_requested(hrefs: Array) -> void:
	GlassModal.prompt_name(self, hrefs.size(), func(name: String) -> void:
		PlaylistService.create_snapshot_playlist(name, hrefs)
	)


func _on_table_play_requested(row: Dictionary, queue: Array) -> void:
	AudioManager.play_track(row.href, queue)
	if is_instance_valid(MetadataService):
		MetadataService.focus_track(row.href, row.artist, row.album, row.title)


func _on_add_music_pressed() -> void:
	if SettingsManager.has_credentials():
		show_loading()
		var active_folder: String = SettingsManager.webdav_folder if "webdav_folder" in SettingsManager else "Music"
		WebDAVService.scan_music_directory(active_folder)
	else:
		body.text = "Please open the connection menu or main configuration wizard to link a WebDAV provider."

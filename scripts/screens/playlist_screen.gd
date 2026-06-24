extends Control

const PLAYLIST_TRACK_ROW = preload("res://scenes/screens/playlist/playlist_track_row.tscn")


@onready var CoverContainer: PanelContainer = $Margin/VBox/HeaderHBox/CoverContainer
@onready var CoverTexture: TextureRect = $Margin/VBox/HeaderHBox/CoverContainer/CoverTexture
@onready var PencilBtn: Button = $Margin/VBox/HeaderHBox/CoverContainer/PencilBtn
@onready var TitleEdit: LineEdit = $Margin/VBox/HeaderHBox/InfoVBox/TitleEdit
@onready var MetaLabel: Label = $Margin/VBox/HeaderHBox/InfoVBox/MetaLabel
@onready var PlayBtn: Button = $Margin/VBox/HeaderHBox/InfoVBox/ActionHBox/PlayBtn
@onready var DeleteBtn: Button = $Margin/VBox/HeaderHBox/InfoVBox/ActionHBox/DeleteBtn
@onready var Scroll: ScrollContainer = $Margin/VBox/Scroll
@onready var TrackList: VBoxContainer = $Margin/VBox/Scroll/TrackList
@onready var EmptyState: PanelContainer = $Margin/VBox/EmptyState
@onready var EmptyIcon: Label = $Margin/VBox/EmptyState/EmptyVBox/EmptyIcon
@onready var EmptyHeading: Label = $Margin/VBox/EmptyState/EmptyVBox/EmptyHeading
@onready var EmptyBody: Label = $Margin/VBox/EmptyState/EmptyVBox/EmptyBody
@onready var FileDialogNode: FileDialog = $FileDialog

var drop_indicator: ColorRect = null
var active_drag_source_row: Control = null

func _ready() -> void:
	visible = (NavManager.current_screen == "playlist")
	NavManager.navigation_requested.connect(_on_navigation_requested)
	ThemeManager.theme_changed.connect(_apply_styles)
	
	if PlaylistService:
		PlaylistService.playlist_tracks_updated.connect(_on_playlist_tracks_updated)
		PlaylistService.playlist_cover_updated.connect(_on_playlist_cover_updated)
		PlaylistService.playlist_renamed.connect(_on_playlist_renamed)
		PlaylistService.playlists_loaded.connect(_refresh_playlist)
		PlaylistService.active_playlist_changed.connect(func(_id): _refresh_playlist())
		
	# Cover Hover & Edit Wiring
	PencilBtn.visible = false
	CoverContainer.mouse_entered.connect(func(): PencilBtn.visible = true)
	CoverContainer.mouse_exited.connect(func():
		var local_m_pos = CoverContainer.get_local_mouse_position()
		var rect = Rect2(Vector2.ZERO, CoverContainer.size)
		if not rect.has_point(local_m_pos):
			PencilBtn.visible = false
	)
	PencilBtn.pressed.connect(func(): FileDialogNode.popup_centered_ratio(0.6))
	FileDialogNode.file_selected.connect(_on_cover_file_selected)
	
	# Initialize Drop Indicator
	drop_indicator = ColorRect.new()
	drop_indicator.custom_minimum_size.y = 3
	drop_indicator.mouse_filter = Control.MOUSE_FILTER_IGNORE
	drop_indicator.visible = false
	TrackList.add_child(drop_indicator)
	
	# OS drag and drop files connection
	get_tree().get_root().files_dropped.connect(_on_files_dropped)
	
	# Title Edit wiring
	TitleEdit.text_submitted.connect(_on_title_submitted)
	TitleEdit.focus_exited.connect(_on_title_focus_exited)
	
	# Actions
	PlayBtn.pressed.connect(_on_play_pressed)
	DeleteBtn.pressed.connect(_on_delete_pressed)
	
	_apply_styles()
	_refresh_playlist()

func show_drop_indicator(source_idx: int, target_idx: int, above: bool) -> void:
	if not drop_indicator:
		return
		
	# Apply styling from current theme if needed
	drop_indicator.color = ThemeManager.current_theme.ACCENT_CORE
	
	if active_drag_source_row and is_instance_valid(active_drag_source_row):
		active_drag_source_row.modulate.a = 1.0
	if source_idx >= 0 and source_idx < TrackList.get_child_count():
		active_drag_source_row = TrackList.get_child(source_idx)
		active_drag_source_row.modulate.a = 0.4
		
	drop_indicator.visible = true
	var new_idx = target_idx
	if not above:
		new_idx += 1
	new_idx = clamp(new_idx, 0, TrackList.get_child_count() - 1)
	TrackList.move_child(drop_indicator, new_idx)

func hide_drop_indicator() -> void:
	if drop_indicator:
		drop_indicator.visible = false
	if active_drag_source_row and is_instance_valid(active_drag_source_row):
		active_drag_source_row.modulate.a = 1.0
		active_drag_source_row = null

func _on_navigation_requested(screen_name: String) -> void:
	visible = (screen_name == "playlist")
	if visible:
		_refresh_playlist()

func _apply_styles() -> void:
	var theme = ThemeManager.current_theme
	
	# Apply glass styleboxes
	CoverContainer.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD, 0.4))
	EmptyState.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_LG, 0.25))
	
	# Font styling
	TitleEdit.add_theme_font_override("font", theme.font_display)
	TitleEdit.add_theme_font_size_override("font_size", theme.TYPE_LG)
	TitleEdit.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	TitleEdit.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	TitleEdit.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	
	MetaLabel.add_theme_font_override("font", theme.font_ui)
	MetaLabel.add_theme_font_size_override("font_size", theme.TYPE_SM)
	MetaLabel.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	
	PlayBtn.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
	PlayBtn.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	PlayBtn.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	PlayBtn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	PlayBtn.add_theme_color_override("font_color", theme.ACCENT_BRIGHT)
	PlayBtn.add_theme_color_override("font_hover_color", theme.TEXT_INVERSE)
	PlayBtn.add_theme_font_override("font", theme.font_ui)
	PlayBtn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	
	DeleteBtn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	DeleteBtn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	DeleteBtn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	DeleteBtn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	DeleteBtn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	DeleteBtn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
	DeleteBtn.add_theme_font_override("font", theme.font_ui)
	DeleteBtn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	
	PencilBtn.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.6))
	PencilBtn.add_theme_stylebox_override("hover", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.9))
	PencilBtn.add_theme_stylebox_override("pressed", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.9))
	PencilBtn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	PencilBtn.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	PencilBtn.add_theme_font_override("font", theme.font_ui)
	
	EmptyIcon.add_theme_font_size_override("font_size", 48)
	EmptyIcon.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	
	EmptyHeading.add_theme_font_override("font", theme.font_display)
	EmptyHeading.add_theme_font_size_override("font_size", theme.TYPE_MD)
	EmptyHeading.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	
	EmptyBody.add_theme_font_override("font", theme.font_ui)
	EmptyBody.add_theme_font_size_override("font_size", theme.TYPE_SM)
	EmptyBody.add_theme_color_override("font_color", theme.TEXT_SECONDARY)

func _refresh_playlist() -> void:
	if not visible:
		return
	if PlaylistService.active_playlist_id.is_empty():
		return
	var playlist = PlaylistService.get_playlist(PlaylistService.active_playlist_id)
	if playlist.is_empty():
		return
		
	TitleEdit.text = playlist.name
	
	# Load cover
	var cover_path = PlaylistService.get_playlist_cover_path(PlaylistService.active_playlist_id)
	_load_cover_image(cover_path)
	
	# Clear track list
	for child in TrackList.get_children():
		if child != drop_indicator:
			child.queue_free()
		
	var tracks = playlist.tracks
	MetaLabel.text = "%d tracks" % tracks.size()
	
	if tracks.is_empty():
		Scroll.visible = false
		EmptyState.visible = true
	else:
		Scroll.visible = true
		EmptyState.visible = false
		
		for i in range(tracks.size()):
			var href = tracks[i]
			var title = ""
			var artist = "Unknown Artist"
			
			if MetadataService:
				var meta = MetadataService.get_cached_metadata(href)
				if not meta.is_empty():
					title = meta.get("track_title", "")
					artist = meta.get("artist_name", "Unknown Artist")
					
			if title.is_empty():
				var file_name = href.get_file().get_basename()
				if " - " in file_name:
					var parts = file_name.split(" - ")
					artist = parts[0].strip_edges()
					title = parts[1].strip_edges()
				else:
					title = file_name
					
			var row = PLAYLIST_TRACK_ROW.instantiate()
			TrackList.add_child(row)
			row.setup(i, href, title, artist)
			
			row.reorder_requested.connect(_on_row_reorder_requested)
			row.track_dropped_at.connect(_on_row_track_dropped_at)
			row.play_requested.connect(_on_row_play_requested)
			row.remove_requested.connect(_on_row_remove_requested)

func _load_cover_image(path: String) -> void:
	if path.is_empty() or not FileAccess.file_exists(path):
		CoverTexture.texture = null
		return
		
	var img = Image.load_from_file(path)
	if img:
		var tex = ImageTexture.create_from_image(img)
		CoverTexture.texture = tex
	else:
		CoverTexture.texture = null

func _on_playlist_tracks_updated(id: String) -> void:
	if id == PlaylistService.active_playlist_id:
		_refresh_playlist()

func _on_playlist_cover_updated(id: String, new_cover_path: String) -> void:
	if id == PlaylistService.active_playlist_id:
		_load_cover_image(new_cover_path)

func _on_playlist_renamed(id: String, new_name: String) -> void:
	if id == PlaylistService.active_playlist_id:
		TitleEdit.text = new_name

func _on_cover_file_selected(path: String) -> void:
	if not PlaylistService.active_playlist_id.is_empty():
		PlaylistService.set_playlist_custom_cover(PlaylistService.active_playlist_id, path)

func _on_files_dropped(files: PackedStringArray) -> void:
	if files.is_empty() or PlaylistService.active_playlist_id.is_empty():
		return
	var file_path = files[0]
	var ext = file_path.get_extension().to_lower()
	if ext in ["png", "jpg", "jpeg", "webp"]:
		var m_pos = get_global_mouse_position()
		if CoverContainer.get_global_rect().has_point(m_pos):
			PlaylistService.set_playlist_custom_cover(PlaylistService.active_playlist_id, file_path)

func _on_title_submitted(new_text: String) -> void:
	var clean = new_text.strip_edges()
	if not clean.is_empty() and not PlaylistService.active_playlist_id.is_empty():
		PlaylistService.rename_playlist(PlaylistService.active_playlist_id, clean)
	else:
		_refresh_playlist()

func _on_title_focus_exited() -> void:
	var clean = TitleEdit.text.strip_edges()
	if not clean.is_empty() and not PlaylistService.active_playlist_id.is_empty():
		PlaylistService.rename_playlist(PlaylistService.active_playlist_id, clean)
	else:
		_refresh_playlist()

func _on_play_pressed() -> void:
	if PlaylistService.active_playlist_id.is_empty():
		return
	var playlist = PlaylistService.get_playlist(PlaylistService.active_playlist_id)
	if not playlist.is_empty() and not playlist.tracks.is_empty():
		AudioManager.play_track(playlist.tracks[0], playlist.tracks)

func _on_delete_pressed() -> void:
	if PlaylistService.active_playlist_id.is_empty():
		return
	var id = PlaylistService.active_playlist_id
	PlaylistService.delete_playlist(id)
	NavManager.navigate_to("library")

# Drag-and-drop on screen level to append tracks
func _can_drop_data(_at_position: Vector2, data: Variant) -> bool:
	return data is Dictionary and data.get("type") == "track"

func _drop_data(_at_position: Vector2, data: Variant) -> void:
	var href = data.get("href", "")
	if not href.is_empty() and not PlaylistService.active_playlist_id.is_empty():
		PlaylistService.add_track_to_playlist(PlaylistService.active_playlist_id, href)

func _on_row_reorder_requested(source_idx: int, target_idx: int) -> void:
	PlaylistService.reorder_track_in_playlist(PlaylistService.active_playlist_id, source_idx, target_idx)

func _on_row_track_dropped_at(track_href: String, target_idx: int) -> void:
	PlaylistService.add_track_to_playlist_at_index(PlaylistService.active_playlist_id, track_href, target_idx)

func _on_row_play_requested(index: int) -> void:
	var playlist = PlaylistService.get_playlist(PlaylistService.active_playlist_id)
	if not playlist.is_empty() and index < playlist.tracks.size():
		AudioManager.play_track(playlist.tracks[index], playlist.tracks)

func _on_row_remove_requested(index: int) -> void:
	PlaylistService.remove_track_from_playlist(PlaylistService.active_playlist_id, index)

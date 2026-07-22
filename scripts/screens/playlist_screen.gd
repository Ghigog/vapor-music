extends Control

const TrackIndex = preload("res://scripts/services/track_index.gd")
const TrackTable = preload("res://scripts/ui/track_table.gd")
const GlassModal = preload("res://scripts/ui/glass_modal.gd")


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

## Shared row builder — playlists derive their rows exactly like the library
## does, so both screens show identical (honest) names for the same track.
var _row_builder := TrackIndex.new()
var _table: VBoxContainer

## Narrow layouts have no sidebar, so the title (rename in place) and delete
## live in this compact strip above the table (§14.7). Wide layouts get both
## from the sidebar and never show it.
var _compact_header: HBoxContainer
var _compact_title: LineEdit


func _setup_table(vbox: VBoxContainer) -> void:
	_table = TrackTable.new()
	_table.manual_mode = true
	vbox.add_child(_table)
	vbox.move_child(_table, Scroll.get_index())
	_table.play_requested.connect(_on_table_play_requested)
	_table.remove_requested.connect(func(manual_index: int) -> void:
		PlaylistService.remove_track_from_playlist(PlaylistService.active_playlist_id, manual_index)
	)
	_table.reorder_requested.connect(func(from_index: int, to_index: int) -> void:
		PlaylistService.reorder_track_in_playlist(PlaylistService.active_playlist_id, from_index, to_index)
	)
	_table.insert_requested.connect(func(href: String, at_index: int) -> void:
		PlaylistService.add_track_to_playlist_at_index(PlaylistService.active_playlist_id, href, at_index)
	)

func _ready() -> void:
	visible = (NavManager.current_screen == "playlist")
	NavManager.navigation_requested.connect(_on_navigation_requested)
	ThemeManager.theme_changed.connect(_apply_styles)

	# The header (cover, title, play/delete buttons) was dead space: the title
	# lives in the sidebar, the cover lives in the sidebar preview slot,
	# playing is clicking a track, and deleting moved to the sidebar ✕.
	$Margin/VBox/HeaderHBox.visible = false

	# A playlist IS the track table in manual mode — same columns, search,
	# sort, and honesty rules as the library, plus reorder/remove/insert.
	Scroll.visible = false
	var vbox := $Margin/VBox as VBoxContainer
	_setup_table(vbox)

	_build_compact_header(vbox)
	PlatformManager.layout_changed.connect(func(_bp: String) -> void: _update_compact_header())

	if PlaylistService:
		PlaylistService.playlist_tracks_updated.connect(_on_playlist_tracks_updated)
		PlaylistService.playlist_cover_updated.connect(_on_playlist_cover_updated)
		PlaylistService.playlist_renamed.connect(_on_playlist_renamed)
		PlaylistService.playlists_loaded.connect(_refresh_playlist)
		PlaylistService.active_playlist_changed.connect(func(_id): _refresh_playlist())

	var ms = get_node_or_null("/root/MetadataService")
	if ms and ms.has_signal("metadata_updated"):
		ms.metadata_updated.connect(func(href: String, _meta: Dictionary) -> void:
			if visible:
				_table.refresh_row(_row_builder.build_row(href))
		)
		
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

func _build_compact_header(vbox: VBoxContainer) -> void:
	var theme = ThemeManager.current_theme
	_compact_header = HBoxContainer.new()
	_compact_header.add_theme_constant_override("separation", theme.SPACE_2)
	vbox.add_child(_compact_header)
	vbox.move_child(_compact_header, _table.get_index())

	_compact_title = LineEdit.new()
	_compact_title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_compact_title.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	_compact_title.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	_compact_title.add_theme_font_override("font", theme.font_display)
	_compact_title.add_theme_font_size_override("font_size", theme.TYPE_MD)
	_compact_title.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	_compact_header.add_child(_compact_title)
	var commit_title := func() -> void:
		var clean: String = _compact_title.text.strip_edges()
		if not clean.is_empty() and not PlaylistService.active_playlist_id.is_empty():
			PlaylistService.rename_playlist(PlaylistService.active_playlist_id, clean)
	_compact_title.text_submitted.connect(func(_t: String) -> void: commit_title.call())
	_compact_title.focus_exited.connect(commit_title)

	var del_btn := Button.new()
	del_btn.text = "✕"
	del_btn.flat = true
	del_btn.tooltip_text = "Delete playlist"
	del_btn.custom_minimum_size = ThemeManager.min_touch_size(Vector2(32, 32))
	del_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	del_btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	del_btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	del_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	del_btn.add_theme_font_override("font", theme.font_ui)
	del_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	del_btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
	_compact_header.add_child(del_btn)
	del_btn.pressed.connect(func() -> void:
		var pl: Dictionary = PlaylistService.get_playlist(PlaylistService.active_playlist_id)
		if pl.is_empty():
			return
		GlassModal.confirm(self, "Delete Playlist",
			"Delete playlist \"%s\"? This can't be undone." % pl.name,
			"Delete",
			func() -> void:
				NavManager.navigate_to("library")
				PlaylistService.delete_playlist(pl.id)
		)
	)

	_update_compact_header()


func _update_compact_header() -> void:
	_compact_header.visible = not PlatformManager.should_show_sidebar()


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
	if _compact_title:
		_compact_title.text = playlist.name
	
	# Load cover
	var cover_path = PlaylistService.get_playlist_cover_path(PlaylistService.active_playlist_id)
	_load_cover_image(cover_path)
	
	var tracks = playlist.tracks
	MetaLabel.text = "%d tracks" % tracks.size()

	if tracks.is_empty():
		_table.visible = false
		EmptyState.visible = true
	else:
		_table.visible = true
		EmptyState.visible = false

		# Same rows the library table shows, plus manual_pos — the playlist's
		# one piece of state the library doesn't have: the crafted order.
		var rows: Array = []
		for i in range(tracks.size()):
			var row := _row_builder.build_row(tracks[i])
			row.manual_pos = i
			rows.append(row)
		_table.set_rows(rows)

func _load_cover_image(path: String) -> void:
	if path.is_empty() or not FileAccess.file_exists(path):
		CoverTexture.texture = null
		return
		
	var img: Image = Image.load_from_file(path)
	if img:
		if img.get_width() > 512 or img.get_height() > 512:
			img.resize(512, 512, Image.INTERPOLATE_LANCZOS)
		var tex: ImageTexture = ImageTexture.create_from_image(img)
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

## Queue is the table's current view order — a temporarily BPM-sorted playlist
## plays in the order on screen, while the stored manual order stays untouched.
func _on_table_play_requested(row: Dictionary, queue: Array) -> void:
	AudioManager.play_track(row.href, queue)
	if is_instance_valid(MetadataService):
		MetadataService.focus_track(row.href, row.artist, row.album, row.title)

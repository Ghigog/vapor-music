## sidebar.gd
## Desktop navigation sidebar controller.
## Applies design tokens from ThemeManager to the panel and nav buttons,
## and keeps active-item highlights in sync with NavManager signals.
extends PanelContainer

const GlassModal = preload("res://scripts/ui/glass_modal.gd")

@onready var app_name:     Label  = $VBox/Header/HeaderHBox/LogoContainer/AppName
@onready var nav_library:  Button = $VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/NavLibrary
@onready var nav_vibe:   Button = $VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/NavVibe
@onready var nav_settings: Button = $VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/NavSettings
@onready var toggle_playlists_btn: Button = $VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/PlaylistsHeader/TogglePlaylistsBtn
@onready var add_playlist_btn: Button = $VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/PlaylistsHeader/AddPlaylistBtn
@onready var playlists_container: VBoxContainer = $VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/PlaylistsContainer
@onready var toggle_dynamic_groups_btn: Button = $VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/DynamicGroupsHeader/ToggleDynamicGroupsBtn
@onready var add_dynamic_group_btn: Button = $VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/DynamicGroupsHeader/AddDynamicGroupBtn
@onready var dynamic_groups_container: VBoxContainer = $VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/DynamicGroupsContainer

@onready var track_title_label: Label = $VBox/NowPlayingMargin/NowPlaying/HBox/Info/TrackTitle
@onready var artist_label: Label = $VBox/NowPlayingMargin/NowPlaying/HBox/Info/ArtistLabel
@onready var play_pause_btn: Button = $VBox/PlayerTilesMargin/PlayerTiles/PlayPauseBtn
@onready var backward_btn: Button = $VBox/PlayerTilesMargin/PlayerTiles/BackwardBtn
@onready var forward_btn: Button = $VBox/PlayerTilesMargin/PlayerTiles/ForwardBtn
@onready var shuffle_btn: Button = $VBox/PlayerTilesMargin/PlayerTiles/ShuffleBtn


@onready var preview_margin: MarginContainer = $VBox/PreviewMargin
@onready var preview_square: AspectRatioContainer = $VBox/PreviewMargin/PreviewSquare
@onready var preview_texture: TextureRect = $VBox/PreviewMargin/PreviewSquare/PreviewTexture
@onready var blur_overlay: Panel = $VBox/PreviewMargin/PreviewSquare/BlurOverlay
@onready var lyrics_scroll: ScrollContainer = $VBox/PreviewMargin/PreviewSquare/LyricsScroll
@onready var lyrics_container: VBoxContainer = $VBox/PreviewMargin/PreviewSquare/LyricsScroll/LyricsContainer

var lyrics_list: Array = []
var active_line_index: int = -1
var is_synced_lyrics := false


var drop_indicator: ColorRect = null
var active_drag_source_item: Control = null

## Folder expand/collapse state, keyed by folder id (default expanded — not
## persisted, mirrors the Playlists/Dynamic Groups section toggles).
var _expanded_folders: Dictionary = {}

## Separate drop-indicator state for the Dynamic Groups list — distinct from
## the Playlists one above so reordering one list never touches the other.
var group_drop_indicator: ColorRect = null
var active_group_drag_source_item: Control = null

## What the output monitor currently shows: "artist" | "album" | "track" |
## "playlist" plus its id (name / href / playlist id). Drives the custom-image
## import/reset behavior on the preview slot.
var _focused_kind := ""
var _focused_id := ""
var _slot_icon: Button = null
var _image_dialog: FileDialog = null

## Maps screen-name strings to their corresponding Button nodes.
var _nav_buttons: Dictionary = {}
var _custom_stylebox: StyleBoxFlat = null
var _playlists_visible := true
var _dynamic_groups_visible := true

## Per-instance copy of the premium_glass material. See _setup_glass_material().
var _glass_material: ShaderMaterial = null


func _ready() -> void:
	if has_theme_stylebox_override("panel"):
		var sb = get_theme_stylebox("panel")
		if sb is StyleBoxFlat:
			_custom_stylebox = sb.duplicate()
			add_theme_stylebox_override("panel", _custom_stylebox)

	# Enable drag clicks to pass through to the PanelContainer background
	$VBox.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Header.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Header/HeaderHBox/LogoContainer.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Scroll.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Scroll/ScrollMargin/ScrollVBox.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Scroll/ScrollMargin/ScrollVBox/NavItems.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/NowPlayingMargin/NowPlaying.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/NowPlayingMargin/NowPlaying/HBox.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/PlayerTilesMargin/PlayerTiles.mouse_filter = Control.MOUSE_FILTER_PASS

	# Clicking the now-playing labels re-focuses the current track, bringing
	# its art and lyrics back into the output monitor after browsing away.
	var np_info := $VBox/NowPlayingMargin/NowPlaying/HBox/Info as Control
	np_info.mouse_filter = Control.MOUSE_FILTER_STOP
	np_info.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	np_info.tooltip_text = "Show the current track"
	np_info.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			if is_instance_valid(MetadataService) \
					and AudioManager.current_track_index != -1 \
					and AudioManager.current_track_index < AudioManager.current_playlist.size():
				MetadataService.focus_track_by_href(AudioManager.current_playlist[AudioManager.current_track_index])
			get_viewport().set_input_as_handled()
	)

	if preview_margin:
		preview_margin.visible = false

	_apply_panel_style()
	_apply_logo_style()
	_register_nav_buttons()
	_connect_signals()
	_connect_audio_signals()
	_connect_metadata_signals()
	_set_active_nav(NavManager.current_screen)
	_style_player_buttons()
	_setup_blur_shader()
	_setup_glass_material()
	_setup_playlists_ui()
	_setup_dynamic_groups_ui()
	_setup_preview_slot()
	ThemeManager.theme_changed.connect(_apply_panel_style)
	ThemeManager.theme_changed.connect(_apply_logo_style)
	ThemeManager.theme_changed.connect(_refresh_nav_button_styles)
	ThemeManager.theme_changed.connect(_style_player_buttons)
	ThemeManager.theme_changed.connect(_style_playlists_header_buttons)
	ThemeManager.theme_changed.connect(_style_dynamic_groups_header_buttons)



# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_panel_style() -> void:
	if _custom_stylebox:
		_custom_stylebox.bg_color = ThemeManager.current_theme.BG_BASE
		_custom_stylebox.border_color = ThemeManager.current_theme.GLASS_BORDER_SUBTLE
		# DESIGN RULE: the sidebar's right edge is ALWAYS square — the seek bar
		# runs flush along it, and rounded right corners break that line. Only
		# the left corners follow the window frame curve. (See DESIGN_LANGUAGE.md.)
		_custom_stylebox.corner_radius_top_right = 0
		_custom_stylebox.corner_radius_bottom_right = 0
	else:
		add_theme_stylebox_override("panel", ThemeManager.make_nav_panel())
	var sw = ThemeManager.current_theme.SIDEBAR_WIDTH
	custom_minimum_size.x = sw
	# The preview texture uses EXPAND_IGNORE_SIZE, which reports a zero
	# minimum — without an explicit minimum height the whole "output monitor"
	# collapses to a 0-height rect: visible in the tree, invisible on screen.
	# Square it against the sidebar width minus the 16px side margins.
	if preview_square:
		preview_square.custom_minimum_size.y = sw - 32


func _apply_logo_style() -> void:
	app_name.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	app_name.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	app_name.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_MD)


# ---------------------------------------------------------------------------
# Nav Buttons
# ---------------------------------------------------------------------------

func _register_nav_buttons() -> void:
	_nav_buttons = {
		"library":  nav_library,
		"vibe":     nav_vibe,
		"settings": nav_settings,
	}
	for screen_name: String in _nav_buttons:
		var btn: Button = _nav_buttons[screen_name]
		btn.pressed.connect(_on_nav_pressed.bind(screen_name))


func _refresh_nav_button_styles() -> void:
	for nav in _nav_buttons:
		var btn = _nav_buttons[nav]
		var is_active = (nav == NavManager.current_screen)
		_style_nav_button(btn, is_active)
	_refresh_playlists_styles()
	_refresh_dynamic_groups_styles()

func _style_nav_button(btn: Button, active: bool) -> void:
	btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	btn.custom_minimum_size.y = ThemeManager.min_touch_height(
		int(ThemeManager.current_theme.TOUCH_TARGET_MIN * 0.5))
	btn.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	btn.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	if active:
		btn.add_theme_stylebox_override("normal",  ThemeManager.make_nav_item_active())
		btn.add_theme_stylebox_override("hover",   ThemeManager.make_nav_item_active())
		btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_active())
		btn.add_theme_color_override("font_color",       ThemeManager.current_theme.ACCENT_CORE)
		btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	else:
		btn.add_theme_stylebox_override("normal",  ThemeManager.make_transparent())
		btn.add_theme_stylebox_override("hover",   ThemeManager.make_nav_item_hover())
		btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
		btn.add_theme_color_override("font_color",       ThemeManager.current_theme.TEXT_TERTIARY)
		btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.TEXT_SECONDARY)
	btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())


func _set_active_nav(screen_name: String) -> void:
	for tab_name: String in _nav_buttons:
		_style_nav_button(_nav_buttons[tab_name], tab_name == screen_name)
	# Playlist and dynamic-group items are nav items too — their highlight
	# depends on the active screen just as much as the fixed buttons do.
	# Without this, navigating away left the last-opened one looking selected.
	_refresh_playlists_styles()
	_refresh_dynamic_groups_styles()


# ---------------------------------------------------------------------------
# Signal Handlers
# ---------------------------------------------------------------------------

func _connect_signals() -> void:
	NavManager.navigation_requested.connect(_on_navigation_requested)


func _on_nav_pressed(screen_name: String) -> void:
	NavManager.navigate_to(screen_name)


func _on_navigation_requested(screen_name: String) -> void:
	_set_active_nav(screen_name)


# ---------------------------------------------------------------------------
# Custom Window Dragging
# ---------------------------------------------------------------------------

func _gui_input(event: InputEvent) -> void:
	if not PlatformManager.is_desktop():
		return
		
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
		get_window().start_drag()
		get_viewport().set_input_as_handled()


# ---------------------------------------------------------------------------
# Player Controls & Now Playing
# ---------------------------------------------------------------------------

func _connect_audio_signals() -> void:
	AudioManager.track_changed.connect(_on_track_changed)
	AudioManager.playback_toggled.connect(_on_playback_toggled)
	AudioManager.position_changed.connect(_on_position_changed)

	play_pause_btn.pressed.connect(func(): AudioManager.toggle_play())
	backward_btn.pressed.connect(func(): AudioManager.play_previous())
	forward_btn.pressed.connect(func(): AudioManager.play_next())
	
	shuffle_btn.button_pressed = AudioManager.smart_mixing_enabled
	shuffle_btn.toggled.connect(func(pressed): AudioManager.smart_mixing_enabled = pressed)
	AudioManager.smart_mixing_toggled.connect(func(enabled):
		shuffle_btn.button_pressed = enabled
	)



func _style_player_buttons() -> void:
	if not is_inside_tree():
		return
		
	var theme = ThemeManager.current_theme
	
	# Typography & colors for labels
	track_title_label.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	track_title_label.add_theme_font_override("font", theme.font_ui)
	track_title_label.add_theme_font_size_override("font_size", theme.TYPE_SM)
	
	artist_label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	artist_label.add_theme_font_override("font", theme.font_ui)
	artist_label.add_theme_font_size_override("font_size", theme.TYPE_XS)
	
	# Tile border styling
	var border_color = theme.GLASS_BORDER_SUBTLE
	
	var tile_normal = StyleBoxFlat.new()
	tile_normal.bg_color = theme.BG_ELEVATED
	tile_normal.border_color = border_color
	tile_normal.set_border_width_all(1)
	tile_normal.set_corner_radius_all(0)
	tile_normal.content_margin_left = 4
	tile_normal.content_margin_right = 4
	tile_normal.content_margin_top = 4
	tile_normal.content_margin_bottom = 4
	
	var tile_hover = StyleBoxFlat.new()
	tile_hover.bg_color = theme.GLASS_TINT
	tile_hover.border_color = theme.ACCENT_CORE
	tile_hover.set_border_width_all(1)
	tile_hover.set_corner_radius_all(0)
	tile_hover.content_margin_left = 4
	tile_hover.content_margin_right = 4
	tile_hover.content_margin_top = 4
	tile_hover.content_margin_bottom = 4
	
	var tile_pressed = StyleBoxFlat.new()
	tile_pressed.bg_color = theme.ACCENT_SURFACE
	tile_pressed.border_color = theme.ACCENT_BRIGHT
	tile_pressed.set_border_width_all(1)
	tile_pressed.set_corner_radius_all(0)
	tile_pressed.content_margin_left = 4
	tile_pressed.content_margin_right = 4
	tile_pressed.content_margin_top = 4
	tile_pressed.content_margin_bottom = 4
	
	for btn in [play_pause_btn, backward_btn, forward_btn, shuffle_btn]:
		# Scene bakes 32 px tall. These are transport controls — the most-tapped
		# things in the sidebar — so lift them to the touch minimum on touch
		# hardware. Width already expands to fill, so only height needs enforcing.
		btn.custom_minimum_size.y = ThemeManager.min_touch_height(int(btn.custom_minimum_size.y))

		var normal_style = tile_normal.duplicate()
		var hover_style = tile_hover.duplicate()
		var pressed_style = tile_pressed.duplicate()

		btn.add_theme_stylebox_override("normal", normal_style)
		btn.add_theme_stylebox_override("hover", hover_style)
		btn.add_theme_stylebox_override("pressed", pressed_style)
		btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		btn.add_theme_font_override("font", theme.font_ui)
		btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
		btn.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
		btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
		btn.add_theme_color_override("font_pressed_color", theme.ACCENT_DIM)


func _on_track_changed(track_name: String) -> void:
	# Resolve names from the href through parse_track_info — the single source
	# of truth — rather than string-splitting the emitted filename. The split
	# showed "Rise.m4a" as the title (extension and all) with the track-number
	# prefix folded into the artist.
	var href := ""
	if AudioManager.current_track_index != -1 and AudioManager.current_track_index < AudioManager.current_playlist.size():
		href = AudioManager.current_playlist[AudioManager.current_track_index]

	var clean_name := track_name
	var artist_name := ""
	if not href.is_empty() and is_instance_valid(MetadataService):
		var info: Dictionary = MetadataService.parse_track_info(href)
		if not (info.track as String).is_empty():
			clean_name = info.track
		if info.artist != "Unknown Artist":
			artist_name = info.artist
	elif " - " in track_name:
		var parts = track_name.split(" - ")
		clean_name = parts[parts.size() - 1]
		if parts.size() > 1:
			artist_name = parts[0]
			for i in range(1, parts.size() - 1):
				artist_name += " - " + parts[i]

	track_title_label.text = clean_name
	artist_label.text = artist_name if artist_name != "" else "Unknown Artist"

	if not href.is_empty() and is_instance_valid(MetadataService):
		MetadataService.focus_track_by_href(href)



func _on_playback_toggled(is_playing: bool) -> void:
	if is_playing:
		play_pause_btn.text = "⏸"
	else:
		play_pause_btn.text = "▶"

func _connect_metadata_signals() -> void:
	var ms = get_node_or_null("/root/MetadataService")
	if ms:
		if ms.has_signal("artist_focused"):
			ms.artist_focused.connect(_on_artist_focused)
		if ms.has_signal("album_focused"):
			ms.album_focused.connect(_on_album_focused)
		if ms.has_signal("track_focused"):
			ms.track_focused.connect(_on_track_focused)
		if ms.has_signal("lyrics_fetched"):
			ms.lyrics_fetched.connect(_on_lyrics_fetched)

func _on_lyrics_fetched(href: String, lyrics: Dictionary) -> void:
	if AudioManager.current_track_index != -1 and AudioManager.current_track_index < AudioManager.current_playlist.size():
		var current_href = AudioManager.current_playlist[AudioManager.current_track_index]
		if href == current_href:
			# Update lyrics display on-the-fly
			_on_track_focused(href, "", "", "", lyrics, "")

## Makes the premium_glass material unique to this node and keeps its
## container_size in sync with the sidebar's actual dimensions.
##
## The scene assigns a ShaderMaterial whose container_size is baked to
## (1232, 672) — the app frame's size, not the sidebar's. Nothing updated it:
## main.gd's resize handler drives app_window_frame.material, which is a
## *different* material created at runtime in _apply_background(). So the
## sidebar rendered its rounded-box SDF against dimensions ~5x its real width,
## which distorts the corner radius and border into anisotropic ellipses.
##
## duplicate() guards against the material being shared if the scene is ever
## instanced more than once — each instance needs its own container_size.
func _setup_glass_material() -> void:
	var mat := material as ShaderMaterial
	if not mat:
		return

	_glass_material = mat.duplicate() as ShaderMaterial
	material = _glass_material

	# Left corners follow the window frame; right edge stays square so the
	# seek bar can run flush along it. (tl, tr, br, bl)
	var r := float(ThemeManager.current_theme.RADIUS_LG)
	_glass_material.set_shader_parameter("corner_radii", Vector4(r, 0.0, 0.0, r))

	resized.connect(_update_glass_size)
	_update_glass_size()


func _update_glass_size() -> void:
	if _glass_material:
		_glass_material.set_shader_parameter("container_size", size)


func _setup_blur_shader() -> void:
	var shader = load("res://assets/shaders/blur.gdshader") as Shader
	if shader:
		var mat = ShaderMaterial.new()
		mat.shader = shader
		blur_overlay.material = mat

func _load_image_to_texture(path: String) -> void:
	if path.is_empty() or not FileAccess.file_exists(path):
		preview_texture.texture = null
		return
		
	var img: Image = Image.load_from_file(path)
	if img:
		if img.get_width() > 512 or img.get_height() > 512:
			img.resize(512, 512, Image.INTERPOLATE_LANCZOS)
		var tex: ImageTexture = ImageTexture.create_from_image(img)
		preview_texture.texture = tex
	else:
		preview_texture.texture = null

## The preview square is the app's "output monitor": it always shows the image
## of whatever was focused last — artist, album, playlist, or track. Tracks
## additionally overlay lyrics via _on_track_focused; everything else is a
## plain image through here. Passing kind/id records what is focused so the
## slot's import/reset affordance targets the right entity.
func show_preview_image(path: String, kind: String = "", id: String = "") -> void:
	if not kind.is_empty():
		_focused_kind = kind
		_focused_id = id
	_load_image_to_texture(path)
	blur_overlay.visible = false
	lyrics_scroll.visible = false
	preview_margin.visible = not path.is_empty()
	_update_slot_icon()

func _on_artist_focused(artist: String, image_path: String) -> void:
	show_preview_image(_with_custom_override("artist", artist, image_path), "artist", artist)

func _on_album_focused(_artist: String, album: String, image_path: String) -> void:
	show_preview_image(_with_custom_override("album", album, image_path), "album", album)

## Returns the user-imported override for an entity when one exists, else the
## default image the metadata pipeline resolved.
func _with_custom_override(kind: String, id: String, default_path: String) -> String:
	if is_instance_valid(MetadataService):
		var custom: String = MetadataService.get_custom_image(kind, id)
		if not custom.is_empty():
			return custom
	return default_path


# ---------------------------------------------------------------------------
# Preview slot: custom image import / reset
# ---------------------------------------------------------------------------

func _setup_preview_slot() -> void:
	# Overlay layer: the AspectRatioContainer sizes it to the square itself,
	# and it is the LAST child so the icon draws above the lyrics overlay.
	var overlay_layer := Control.new()
	overlay_layer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	preview_square.add_child(overlay_layer)

	_slot_icon = Button.new()
	_slot_icon.visible = false
	var icon_bg := StyleBoxFlat.new()
	icon_bg.bg_color = Color(0, 0, 0, 0.55)
	icon_bg.set_corner_radius_all(ThemeManager.current_theme.RADIUS_PILL)
	var icon_bg_hover: StyleBoxFlat = icon_bg.duplicate()
	icon_bg_hover.bg_color = Color(0, 0, 0, 0.8)
	_slot_icon.add_theme_stylebox_override("normal", icon_bg)
	_slot_icon.add_theme_stylebox_override("hover", icon_bg_hover)
	_slot_icon.add_theme_stylebox_override("pressed", icon_bg_hover)
	_slot_icon.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	_slot_icon.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	_slot_icon.add_theme_color_override("font_color", Color(1, 1, 1, 0.9))
	overlay_layer.add_child(_slot_icon)
	# Quiet 28px chip tucked into the bottom-right corner — a hint, not a
	# billboard over the artwork.
	_slot_icon.anchor_left = 1.0
	_slot_icon.anchor_top = 1.0
	_slot_icon.anchor_right = 1.0
	_slot_icon.anchor_bottom = 1.0
	_slot_icon.offset_left = -36.0
	_slot_icon.offset_top = -36.0
	_slot_icon.offset_right = -8.0
	_slot_icon.offset_bottom = -8.0
	_slot_icon.pressed.connect(_on_slot_icon_pressed)

	preview_margin.mouse_filter = Control.MOUSE_FILTER_PASS
	preview_margin.mouse_entered.connect(func() -> void: _update_slot_icon(true))
	preview_margin.mouse_exited.connect(func() -> void:
		if not preview_margin.get_global_rect().has_point(get_global_mouse_position()):
			_slot_icon.visible = false
	)

	_image_dialog = FileDialog.new()
	_image_dialog.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	_image_dialog.access = FileDialog.ACCESS_FILESYSTEM
	# The OS file browser, not Godot's UI-toolkit one — picking a file is the
	# platform's job and its dialog beats anything we'd restyle.
	_image_dialog.use_native_dialog = true
	_image_dialog.filters = PackedStringArray(["*.png, *.jpg, *.jpeg, *.webp ; Images"])
	add_child(_image_dialog)
	_image_dialog.file_selected.connect(_apply_custom_image)

	get_tree().root.files_dropped.connect(_on_files_dropped_on_slot)

	if is_instance_valid(MetadataService) and MetadataService.has_signal("custom_image_changed"):
		MetadataService.custom_image_changed.connect(func(kind: String, id: String, _path: String) -> void:
			if kind == _focused_kind and id == _focused_id:
				_refresh_focused_preview()
		)
	if PlaylistService:
		PlaylistService.playlist_cover_updated.connect(func(id: String, _path: String) -> void:
			if _focused_kind == "playlist" and id == _focused_id:
				_refresh_focused_preview()
		)


func _focused_has_custom() -> bool:
	if _focused_kind == "playlist":
		var pl: Dictionary = PlaylistService.get_playlist(_focused_id)
		return not (pl.get("custom_cover_path", "") as String).is_empty()
	return is_instance_valid(MetadataService) \
		and not MetadataService.get_custom_image(_focused_kind, _focused_id).is_empty()


func _update_slot_icon(show_now: bool = false) -> void:
	if _slot_icon == null:
		return
	if not preview_margin.visible or _focused_kind.is_empty():
		_slot_icon.visible = false
		return
	# Touch has no hover: the chip is always present when the slot shows (§14.7).
	if show_now or PlatformManager.is_touch_primary():
		_slot_icon.visible = true
	if not _slot_icon.visible:
		return
	if _focused_has_custom():
		_slot_icon.text = "↺"
		_slot_icon.tooltip_text = "Reset to the default image"
	else:
		_slot_icon.text = "⇩"
		_slot_icon.tooltip_text = "Drop an image here — or click — to set a custom one"


func _on_slot_icon_pressed() -> void:
	if _focused_kind.is_empty():
		return
	if _focused_has_custom():
		if _focused_kind == "playlist":
			PlaylistService.set_playlist_custom_cover(_focused_id, "")
		else:
			MetadataService.clear_custom_image(_focused_kind, _focused_id)
	else:
		_image_dialog.popup_centered_ratio(0.6)


func _apply_custom_image(path: String) -> void:
	if _focused_kind.is_empty():
		return
	if not path.get_extension().to_lower() in ["png", "jpg", "jpeg", "webp"]:
		return
	if _focused_kind == "playlist":
		PlaylistService.set_playlist_custom_cover(_focused_id, path)
	else:
		MetadataService.set_custom_image(_focused_kind, _focused_id, path)


func _on_files_dropped_on_slot(files: PackedStringArray) -> void:
	if files.is_empty() or _focused_kind.is_empty() or not preview_margin.visible:
		return
	if preview_square.get_global_rect().has_point(get_global_mouse_position()):
		_apply_custom_image(files[0])


## Re-resolves the focused entity's image after an import/reset. Async kinds
## re-run their focus flow, which re-emits through the normal handlers.
func _refresh_focused_preview() -> void:
	match _focused_kind:
		"playlist":
			show_preview_image(PlaylistService.get_playlist_cover_path(_focused_id), "playlist", _focused_id)
		"dynamic_group":
			show_preview_image(DynamicGroupService.get_group_cover_path(_focused_id), "dynamic_group", _focused_id)
		"artist":
			MetadataService.focus_artist(_focused_id)
		"album":
			MetadataService.focus_album("", _focused_id)
		"track":
			MetadataService.focus_track_by_href(_focused_id)

func _on_track_focused(href: String, _artist: String, _album: String, _title: String, lyrics: Dictionary, image_path: String) -> void:
	if not href.is_empty():
		_focused_kind = "track"
		_focused_id = href
		image_path = _with_custom_override("track", href, image_path)
	_load_image_to_texture(image_path)
	
	# Clear previous lyrics
	for child in lyrics_container.get_children():
		child.queue_free()
		
	lyrics_list = []
	active_line_index = -1
	is_synced_lyrics = false
	
	if lyrics.is_empty():
		var label = Label.new()
		label.text = "[No Lyrics Found]"
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
		label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_XS)
		lyrics_container.add_child(label)
	elif lyrics.get("synced", false):
		is_synced_lyrics = true
		lyrics_list = lyrics.get("lines", [])
		for line in lyrics_list:
			var label = Label.new()
			label.text = line.get("text", "")
			label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
			label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
			label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
			label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
			label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_XS)
			lyrics_container.add_child(label)
	else:
		var label = Label.new()
		label.text = lyrics.get("plain", "")
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
		label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_XS)
		lyrics_container.add_child(label)
		
	blur_overlay.visible = true
	lyrics_scroll.visible = true
	preview_margin.visible = true
	_update_slot_icon()

## Driven by AudioManager.position_changed rather than a per-frame poll.
##
## 10 Hz is ample for synced lyrics — lines are seconds apart — and it takes the
## linear scan in _update_lyrics_scroller() off the frame path.
func _on_position_changed(pos: float) -> void:
	if is_synced_lyrics and lyrics_scroll.visible:
		if AudioManager.player and AudioManager.player.is_inside_tree():
			_update_lyrics_scroller(pos)

func _update_lyrics_scroller(song_time: float) -> void:
	if lyrics_list.is_empty():
		return
		
	var target_index: int = -1
	for i in range(lyrics_list.size()):
		if song_time >= lyrics_list[i]["time"]:
			target_index = i
		else:
			break
			
	if target_index != active_line_index and target_index != -1:
		active_line_index = target_index
		var children = lyrics_container.get_children()
		for i in range(children.size()):
			var label = children[i] as Label
			if not label: continue
			
			if i == active_line_index:
				# Highlight active line
				label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
				label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
				label.add_theme_color_override("font_outline_color", ThemeManager.current_theme.ACCENT_CORE)
				label.add_theme_constant_override("outline_size", 2)
				
				# Smoothly center scroller
				var target_y = label.position.y - (lyrics_scroll.size.y / 2.0) + (label.size.y / 2.0)
				var tween = create_tween().set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_OUT)
				tween.tween_property(lyrics_scroll, "scroll_vertical", int(max(0, target_y)), 0.3)
			else:
				# Dim inactive lines
				label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
				label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_XS)
				label.remove_theme_color_override("font_outline_color")
				label.remove_theme_constant_override("outline_size")

func _setup_playlists_ui() -> void:
	toggle_playlists_btn.pressed.connect(_on_toggle_playlists_pressed)
	add_playlist_btn.pressed.connect(_on_add_playlist_pressed)
	add_playlist_btn.tooltip_text = "New Playlist (right-click for a folder)"
	# One "+" — same as Dynamic Groups' — not two crowded side by side.
	# Folder creation rides the same right-click/long-press vocabulary the
	# app already uses for secondary actions elsewhere (track rows, group
	# headers): right-click on desktop; touch still gets an explicit button
	# in the mobile popup, since there's no hidden gesture there.
	add_playlist_btn.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_RIGHT:
			_on_add_folder_pressed()
			get_viewport().set_input_as_handled()
	)

	if PlaylistService:
		PlaylistService.playlist_created.connect(func(_p): call_deferred(&"_rebuild_playlists_list"))
		PlaylistService.playlist_deleted.connect(func(_id): call_deferred(&"_rebuild_playlists_list"))
		PlaylistService.playlist_renamed.connect(func(_id, _name): call_deferred(&"_rebuild_playlists_list"))
		PlaylistService.playlists_loaded.connect(func(): call_deferred(&"_rebuild_playlists_list"))
		PlaylistService.playlist_folder_changed.connect(func(_id, _folder_id): call_deferred(&"_rebuild_playlists_list"))
		# Switching playlists while already on the playlist screen is a no-op for
		# NavManager.navigate_to(), so navigation_requested never fires and the
		# highlight would stay on the previously-opened playlist.
		PlaylistService.active_playlist_changed.connect(func(_id): _refresh_playlists_styles())

	if PlaylistFolderService:
		PlaylistFolderService.folder_created.connect(func(_f): call_deferred(&"_rebuild_playlists_list"))
		PlaylistFolderService.folder_deleted.connect(func(_id): call_deferred(&"_rebuild_playlists_list"))
		PlaylistFolderService.folder_renamed.connect(func(_id, _name): call_deferred(&"_rebuild_playlists_list"))
		PlaylistFolderService.folders_loaded.connect(func(): call_deferred(&"_rebuild_playlists_list"))

	# Initialize Drop Indicator for Playlists — shared across the root list and
	# every folder's nested list; show_drop_indicator reparents it into
	# whichever container is currently being dragged over.
	drop_indicator = ColorRect.new()
	drop_indicator.custom_minimum_size.y = 3
	drop_indicator.mouse_filter = Control.MOUSE_FILTER_IGNORE
	drop_indicator.visible = false
	playlists_container.add_child(drop_indicator)

	# Playlist deletion always confirms via the glass modal (_show_delete_modal)
	# — one mis-click on the ✕ must never lose a crafted set.

	_style_playlists_header_buttons()
	_rebuild_playlists_list()

func _style_playlists_header_buttons() -> void:
	var theme = ThemeManager.current_theme
	
	toggle_playlists_btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	toggle_playlists_btn.custom_minimum_size.y = ThemeManager.min_touch_height(
		int(theme.TOUCH_TARGET_MIN * 0.5))
	toggle_playlists_btn.add_theme_font_override("font", theme.font_ui)
	toggle_playlists_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	toggle_playlists_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	toggle_playlists_btn.add_theme_color_override("font_hover_color", theme.TEXT_SECONDARY)
	toggle_playlists_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	toggle_playlists_btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	toggle_playlists_btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	toggle_playlists_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	
	add_playlist_btn.flat = true
	# A bare "+" glyph — intrinsically ~17 px wide, so both axes need enforcing.
	add_playlist_btn.custom_minimum_size = ThemeManager.min_touch_size(
		Vector2(add_playlist_btn.custom_minimum_size.x, theme.TOUCH_TARGET_MIN * 0.5))
	add_playlist_btn.add_theme_font_override("font", theme.font_ui)
	add_playlist_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	add_playlist_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	add_playlist_btn.add_theme_color_override("font_hover_color", theme.TEXT_SECONDARY)
	add_playlist_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	add_playlist_btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	add_playlist_btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	add_playlist_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

	add_playlist_btn.visible = _playlists_visible

func _on_toggle_playlists_pressed() -> void:
	_playlists_visible = not _playlists_visible
	playlists_container.visible = _playlists_visible
	add_playlist_btn.visible = _playlists_visible

func _on_add_playlist_pressed() -> void:
	for child in playlists_container.get_children():
		if child is LineEdit and child.has_meta("is_creation"):
			child.grab_focus()
			return
			
	var line_edit = LineEdit.new()
	line_edit.placeholder_text = "New Playlist..."
	line_edit.set_meta("is_creation", true)
	_style_inline_edit(line_edit)
	
	playlists_container.add_child(line_edit)
	
	line_edit.set_meta("committed", false)
	var commit = func(new_text: String):
		if line_edit.get_meta("committed", false):
			return
		line_edit.set_meta("committed", true)
		var clean = new_text.strip_edges()
		if not clean.is_empty():
			PlaylistService.create_playlist(clean)
		else:
			line_edit.queue_free()
			# Deferred: this commit can fire from focus loss during a rebuild's
			# teardown, and rebuilding the list mid-teardown eats the fresh rows.
			call_deferred(&"_rebuild_playlists_list")
	
	line_edit.text_submitted.connect(commit)
	line_edit.focus_exited.connect(func(): commit.call(line_edit.text))
	
	line_edit.grab_focus.call_deferred()

func _on_add_folder_pressed() -> void:
	for child in playlists_container.get_children():
		if child is LineEdit and child.has_meta("is_folder_creation"):
			child.grab_focus()
			return

	var line_edit = LineEdit.new()
	line_edit.placeholder_text = "New Folder..."
	line_edit.set_meta("is_folder_creation", true)
	_style_inline_edit(line_edit)

	playlists_container.add_child(line_edit)

	line_edit.set_meta("committed", false)
	var commit = func(new_text: String):
		if line_edit.get_meta("committed", false):
			return
		line_edit.set_meta("committed", true)
		var clean = new_text.strip_edges()
		if not clean.is_empty():
			PlaylistFolderService.create_folder(clean)
		else:
			line_edit.queue_free()
			call_deferred(&"_rebuild_playlists_list")

	line_edit.text_submitted.connect(commit)
	line_edit.focus_exited.connect(func(): commit.call(line_edit.text))

	line_edit.grab_focus.call_deferred()

## Single-level folders (agreed for v1 — data model's parent_id already
## supports nesting for later): every folder renders at the top of the
## Playlists list, each with its own nested container of member playlists;
## playlists with no folder_id render below, same as before folders existed.
func _rebuild_playlists_list() -> void:
	for child in playlists_container.get_children():
		if child != drop_indicator:
			child.queue_free()

	if not PlaylistService:
		return

	var all_playlists = PlaylistService.get_playlists()

	if PlaylistFolderService:
		for folder in PlaylistFolderService.get_playlist_folders():
			_add_folder_section(folder, all_playlists)

	for playlist in all_playlists:
		if (playlist.get("folder_id", "") as String).is_empty():
			_add_playlist_row(playlists_container, playlist, "")

	_refresh_playlists_styles()

func _add_folder_section(folder: Dictionary, all_playlists: Array) -> void:
	var header_btn := Button.new()
	header_btn.set_script(preload("res://scripts/ui/sidebar_folder_item.gd"))
	header_btn.folder_id = folder.id
	header_btn.folder_name = folder.name
	header_btn.set_expanded(_expanded_folders.get(folder.id, true))
	playlists_container.add_child(header_btn)

	# Indents member playlists so the folder reads as a container, not just
	# another row of the same weight.
	var content_margin := MarginContainer.new()
	content_margin.add_theme_constant_override("margin_left", 16)
	content_margin.visible = _expanded_folders.get(folder.id, true)
	playlists_container.add_child(content_margin)

	var content := VBoxContainer.new()
	content.add_theme_constant_override("separation", 0)
	content_margin.add_child(content)

	for playlist in all_playlists:
		if playlist.get("folder_id", "") == folder.id:
			_add_playlist_row(content, playlist, folder.id)

	header_btn.pressed.connect(func():
		var expanded = not _expanded_folders.get(folder.id, true)
		_expanded_folders[folder.id] = expanded
		content_margin.visible = expanded
		header_btn.set_expanded(expanded)
	)

	header_btn.gui_input.connect(func(event):
		if event is InputEventMouseButton and event.double_click and event.button_index == MOUSE_BUTTON_LEFT:
			_show_inline_rename_folder(folder, header_btn)
	)

	var del_btn := Button.new()
	del_btn.text = "✕"
	del_btn.flat = true
	del_btn.visible = false
	del_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	del_btn.add_theme_stylebox_override("hover", ThemeManager.make_transparent())
	del_btn.add_theme_stylebox_override("pressed", ThemeManager.make_transparent())
	del_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	del_btn.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	del_btn.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_XS)
	del_btn.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
	del_btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	header_btn.add_child(del_btn)
	del_btn.anchor_left = 1.0
	del_btn.anchor_right = 1.0
	del_btn.anchor_top = 0.0
	del_btn.anchor_bottom = 1.0
	del_btn.offset_left = -30.0
	del_btn.offset_right = -6.0
	del_btn.offset_top = 0.0
	del_btn.offset_bottom = 0.0
	del_btn.pressed.connect(func() -> void:
		_show_delete_folder_modal(folder)
	)
	if PlatformManager.is_touch_primary():
		del_btn.visible = true
	else:
		header_btn.mouse_entered.connect(func() -> void: del_btn.visible = true)
		header_btn.mouse_exited.connect(func() -> void:
			if not Rect2(Vector2.ZERO, header_btn.size).has_point(header_btn.get_local_mouse_position()):
				del_btn.visible = false
		)

func _add_playlist_row(container: VBoxContainer, playlist: Dictionary, folder_id: String) -> void:
	var item_btn = Button.new()
	item_btn.set_script(preload("res://scripts/ui/sidebar_playlist_item.gd"))
	item_btn.playlist_id = playlist.id
	item_btn.playlist_name = playlist.name
	item_btn.folder_id = folder_id
	item_btn.text = "    ♪  " + playlist.name
	container.add_child(item_btn)

	item_btn.pressed.connect(func():
		PlaylistService.active_playlist_id = playlist.id
		NavManager.navigate_to("playlist")
		# Clicking a playlist points the output monitor at its cover; a
		# later track click overwrites it, clicking the playlist again
		# brings it back.
		show_preview_image(PlaylistService.get_playlist_cover_path(playlist.id), "playlist", playlist.id)
	)

	item_btn.gui_input.connect(func(event):
		if event is InputEventMouseButton and event.double_click and event.button_index == MOUSE_BUTTON_LEFT:
			_show_inline_rename(playlist, item_btn)
	)

	var del_btn := Button.new()
	del_btn.text = "✕"
	del_btn.flat = true
	del_btn.visible = false
	del_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	del_btn.add_theme_stylebox_override("hover", ThemeManager.make_transparent())
	del_btn.add_theme_stylebox_override("pressed", ThemeManager.make_transparent())
	del_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	del_btn.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	del_btn.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_XS)
	del_btn.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
	del_btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	item_btn.add_child(del_btn)
	# DESIGN RULE: hover controls span the row's full height and sit INSIDE
	# the row highlight (inset from the rounded edge), so the glyph centers
	# on the row and never floats outside the hover state.
	del_btn.anchor_left = 1.0
	del_btn.anchor_right = 1.0
	del_btn.anchor_top = 0.0
	del_btn.anchor_bottom = 1.0
	del_btn.offset_left = -30.0
	del_btn.offset_right = -6.0
	del_btn.offset_top = 0.0
	del_btn.offset_bottom = 0.0
	del_btn.pressed.connect(func() -> void:
		_show_delete_modal(playlist)
	)
	# Touch has no hover — the ✕ stays visible on touch hardware and is
	# hover-revealed only where a pointer exists (§14.7).
	if PlatformManager.is_touch_primary():
		del_btn.visible = true
	else:
		item_btn.mouse_entered.connect(func() -> void: del_btn.visible = true)
		item_btn.mouse_exited.connect(func() -> void:
			# Entering the ✕ itself fires the parent's mouse_exited — keep
			# the button while the pointer is still inside the row.
			if not Rect2(Vector2.ZERO, item_btn.size).has_point(item_btn.get_local_mouse_position()):
				del_btn.visible = false
		)

## container is whichever VBoxContainer target_item actually lives in — the
## root playlists_container, or a folder's own nested content container.
## Since a dragged playlist can be repositioned within either scope (or move
## between them), the indicator must follow the row being hovered rather than
## assuming a single fixed list.
func show_drop_indicator(container: Node, source_item: Control, target_item: Control, above: bool) -> void:
	if not drop_indicator:
		return

	drop_indicator.color = ThemeManager.current_theme.ACCENT_CORE

	if active_drag_source_item and is_instance_valid(active_drag_source_item):
		active_drag_source_item.modulate.a = 1.0
	active_drag_source_item = source_item
	if active_drag_source_item and is_instance_valid(active_drag_source_item):
		active_drag_source_item.modulate.a = 0.4

	if drop_indicator.get_parent() != container:
		if drop_indicator.get_parent():
			drop_indicator.get_parent().remove_child(drop_indicator)
		container.add_child(drop_indicator)

	drop_indicator.visible = true
	var new_idx = target_item.get_index()
	if not above:
		new_idx += 1
	new_idx = clamp(new_idx, 0, container.get_child_count() - 1)
	container.move_child(drop_indicator, new_idx)

func hide_drop_indicator() -> void:
	if drop_indicator:
		drop_indicator.visible = false
	if active_drag_source_item and is_instance_valid(active_drag_source_item):
		active_drag_source_item.modulate.a = 1.0
		active_drag_source_item = null

## Styles an inline LineEdit to the exact metrics of the nav row it replaces.
## DESIGN RULE: inline edits never change row height or push the surrounding
## menu around — editing happens in place, at the row's own size.
func _style_inline_edit(line_edit: LineEdit) -> void:
	var theme = ThemeManager.current_theme
	var sb := StyleBoxFlat.new()
	sb.bg_color = theme.GLASS_TINT
	sb.border_color = theme.ACCENT_CORE
	sb.set_border_width_all(1)
	sb.set_corner_radius_all(theme.RADIUS_XS)
	sb.content_margin_left = 8
	sb.content_margin_right = 8
	sb.content_margin_top = 1
	sb.content_margin_bottom = 1
	line_edit.add_theme_stylebox_override("normal", sb)
	line_edit.add_theme_stylebox_override("focus", sb)
	line_edit.add_theme_font_override("font", theme.font_ui)
	line_edit.add_theme_font_size_override("font_size", theme.TYPE_SM)
	line_edit.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	line_edit.custom_minimum_size.y = ThemeManager.min_touch_height(int(theme.TOUCH_TARGET_MIN * 0.5))


func _show_delete_modal(playlist: Dictionary) -> void:
	GlassModal.confirm(self, "Delete Playlist",
		"Delete playlist \"%s\"? This can't be undone." % playlist.name,
		"Delete",
		func() -> void:
			if PlaylistService.active_playlist_id == playlist.id and NavManager.current_screen == "playlist":
				NavManager.navigate_to("library")
			PlaylistService.delete_playlist(playlist.id)
	)


func _show_inline_rename(playlist: Dictionary, item_btn: Button) -> void:
	var line_edit = LineEdit.new()
	line_edit.text = playlist.name
	line_edit.alignment = HORIZONTAL_ALIGNMENT_LEFT
	_style_inline_edit(line_edit)

	# The row's own parent — playlists_container for a root playlist, or a
	# folder's nested content container — not assumed to be a fixed list.
	var parent := item_btn.get_parent()
	var idx = item_btn.get_index()
	parent.add_child(line_edit)
	parent.move_child(line_edit, idx)
	item_btn.visible = false

	line_edit.set_meta("committed", false)
	var commit = func(new_text: String):
		if line_edit.get_meta("committed", false):
			return
		line_edit.set_meta("committed", true)
		var clean = new_text.strip_edges()
		if not clean.is_empty() and clean != playlist.name:
			PlaylistService.rename_playlist(playlist.id, clean)
		else:
			# Deferred for the same teardown-reentrancy reason as creation.
			call_deferred(&"_rebuild_playlists_list")

	line_edit.text_submitted.connect(commit)
	line_edit.focus_exited.connect(func(): commit.call(line_edit.text))

	line_edit.grab_focus.call_deferred()
	line_edit.select_all.call_deferred()

func _show_delete_folder_modal(folder: Dictionary) -> void:
	GlassModal.confirm(self, "Delete Folder",
		"Delete folder \"%s\"? Its playlists move back to the top level — nothing is deleted." % folder.name,
		"Delete",
		func() -> void:
			PlaylistFolderService.delete_folder(folder.id)
	)

func _show_inline_rename_folder(folder: Dictionary, header_btn: Button) -> void:
	var line_edit = LineEdit.new()
	line_edit.text = folder.name
	line_edit.alignment = HORIZONTAL_ALIGNMENT_LEFT
	_style_inline_edit(line_edit)

	var idx = header_btn.get_index()
	playlists_container.add_child(line_edit)
	playlists_container.move_child(line_edit, idx)
	header_btn.visible = false

	line_edit.set_meta("committed", false)
	var commit = func(new_text: String):
		if line_edit.get_meta("committed", false):
			return
		line_edit.set_meta("committed", true)
		var clean = new_text.strip_edges()
		if not clean.is_empty() and clean != folder.name:
			PlaylistFolderService.rename_folder(folder.id, clean)
		else:
			call_deferred(&"_rebuild_playlists_list")

	line_edit.text_submitted.connect(commit)
	line_edit.focus_exited.connect(func(): commit.call(line_edit.text))

	line_edit.grab_focus.call_deferred()
	line_edit.select_all.call_deferred()

## Recurses into folder content containers (MarginContainer > VBoxContainer)
## so nested playlist rows get the same active-state highlight as root ones.
func _refresh_playlists_styles() -> void:
	_style_playlist_tree(playlists_container)

func _style_playlist_tree(node: Node) -> void:
	for child in node.get_children():
		if child is Button and child.has_method("set_expanded"):
			child.set_expanded(_expanded_folders.get(child.folder_id, true))
			_style_folder_header(child)
		elif child is Button and "playlist_id" in child:
			var is_active = (NavManager.current_screen == "playlist" and PlaylistService.active_playlist_id == child.playlist_id)
			_style_nav_button(child, is_active)
		elif child.get_child_count() > 0:
			_style_playlist_tree(child)

func _style_folder_header(btn: Button) -> void:
	var theme = ThemeManager.current_theme
	btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	btn.custom_minimum_size.y = ThemeManager.min_touch_height(int(theme.TOUCH_TARGET_MIN * 0.5))
	btn.add_theme_font_override("font", theme.font_ui)
	btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	btn.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	btn.add_theme_color_override("font_hover_color", theme.TEXT_PRIMARY)


# ---------------------------------------------------------------------------
# Dynamic Groups — mirrors the Playlists block above. A dynamic group holds
# ENTITIES (artist/album/genre values), not tracks: dragging a library group
# header onto one here adds it live; opening the group shows those entities
# as cards (dynamic_group_screen.gd), never a track list directly.
# ---------------------------------------------------------------------------

func _setup_dynamic_groups_ui() -> void:
	toggle_dynamic_groups_btn.pressed.connect(_on_toggle_dynamic_groups_pressed)
	add_dynamic_group_btn.pressed.connect(_on_add_dynamic_group_pressed)

	if DynamicGroupService:
		DynamicGroupService.group_created.connect(func(_g): call_deferred(&"_rebuild_dynamic_groups_list"))
		DynamicGroupService.group_deleted.connect(func(_id): call_deferred(&"_rebuild_dynamic_groups_list"))
		DynamicGroupService.group_renamed.connect(func(_id, _name): call_deferred(&"_rebuild_dynamic_groups_list"))
		DynamicGroupService.groups_loaded.connect(func(): call_deferred(&"_rebuild_dynamic_groups_list"))
		DynamicGroupService.active_group_changed.connect(func(_id): _refresh_dynamic_groups_styles())

	group_drop_indicator = ColorRect.new()
	group_drop_indicator.custom_minimum_size.y = 3
	group_drop_indicator.mouse_filter = Control.MOUSE_FILTER_IGNORE
	group_drop_indicator.visible = false
	dynamic_groups_container.add_child(group_drop_indicator)

	_style_dynamic_groups_header_buttons()
	_rebuild_dynamic_groups_list()

func _style_dynamic_groups_header_buttons() -> void:
	var theme = ThemeManager.current_theme

	toggle_dynamic_groups_btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	toggle_dynamic_groups_btn.custom_minimum_size.y = ThemeManager.min_touch_height(
		int(theme.TOUCH_TARGET_MIN * 0.5))
	toggle_dynamic_groups_btn.add_theme_font_override("font", theme.font_ui)
	toggle_dynamic_groups_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	toggle_dynamic_groups_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	toggle_dynamic_groups_btn.add_theme_color_override("font_hover_color", theme.TEXT_SECONDARY)
	toggle_dynamic_groups_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	toggle_dynamic_groups_btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	toggle_dynamic_groups_btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	toggle_dynamic_groups_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

	add_dynamic_group_btn.flat = true
	add_dynamic_group_btn.custom_minimum_size = ThemeManager.min_touch_size(
		Vector2(add_dynamic_group_btn.custom_minimum_size.x, theme.TOUCH_TARGET_MIN * 0.5))
	add_dynamic_group_btn.add_theme_font_override("font", theme.font_ui)
	add_dynamic_group_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	add_dynamic_group_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	add_dynamic_group_btn.add_theme_color_override("font_hover_color", theme.TEXT_SECONDARY)
	add_dynamic_group_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	add_dynamic_group_btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	add_dynamic_group_btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	add_dynamic_group_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

	add_dynamic_group_btn.visible = _dynamic_groups_visible

func _on_toggle_dynamic_groups_pressed() -> void:
	_dynamic_groups_visible = not _dynamic_groups_visible
	dynamic_groups_container.visible = _dynamic_groups_visible
	add_dynamic_group_btn.visible = _dynamic_groups_visible

func _on_add_dynamic_group_pressed() -> void:
	for child in dynamic_groups_container.get_children():
		if child is LineEdit and child.has_meta("is_creation"):
			child.grab_focus()
			return

	var line_edit = LineEdit.new()
	line_edit.placeholder_text = "New Group..."
	line_edit.set_meta("is_creation", true)
	_style_inline_edit(line_edit)

	dynamic_groups_container.add_child(line_edit)

	line_edit.set_meta("committed", false)
	var commit = func(new_text: String):
		if line_edit.get_meta("committed", false):
			return
		line_edit.set_meta("committed", true)
		var clean = new_text.strip_edges()
		if not clean.is_empty():
			DynamicGroupService.create_group(clean)
		else:
			line_edit.queue_free()
			call_deferred(&"_rebuild_dynamic_groups_list")

	line_edit.text_submitted.connect(commit)
	line_edit.focus_exited.connect(func(): commit.call(line_edit.text))

	line_edit.grab_focus.call_deferred()

func _rebuild_dynamic_groups_list() -> void:
	for child in dynamic_groups_container.get_children():
		if child != group_drop_indicator:
			child.queue_free()

	if not DynamicGroupService:
		return

	var groups_list = DynamicGroupService.get_dynamic_groups()
	for group in groups_list:
		var item_btn = Button.new()
		item_btn.set_script(preload("res://scripts/ui/dynamic_group_sidebar_item.gd"))
		item_btn.group_id = group.id
		item_btn.group_name = group.name
		item_btn.text = "    ⚡  " + group.name
		dynamic_groups_container.add_child(item_btn)

		item_btn.pressed.connect(func():
			DynamicGroupService.active_group_id = group.id
			NavManager.navigate_to("dynamic_group")
			show_preview_image(DynamicGroupService.get_group_cover_path(group.id), "dynamic_group", group.id)
		)

		item_btn.gui_input.connect(func(event):
			if event is InputEventMouseButton and event.double_click and event.button_index == MOUSE_BUTTON_LEFT:
				_show_inline_rename_group(group, item_btn)
		)

		var del_btn := Button.new()
		del_btn.text = "✕"
		del_btn.flat = true
		del_btn.visible = false
		del_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
		del_btn.add_theme_stylebox_override("hover", ThemeManager.make_transparent())
		del_btn.add_theme_stylebox_override("pressed", ThemeManager.make_transparent())
		del_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		del_btn.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		del_btn.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_XS)
		del_btn.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
		del_btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.ACCENT_BRIGHT)
		item_btn.add_child(del_btn)
		del_btn.anchor_left = 1.0
		del_btn.anchor_right = 1.0
		del_btn.anchor_top = 0.0
		del_btn.anchor_bottom = 1.0
		del_btn.offset_left = -30.0
		del_btn.offset_right = -6.0
		del_btn.offset_top = 0.0
		del_btn.offset_bottom = 0.0
		del_btn.pressed.connect(func() -> void:
			_show_delete_group_modal(group)
		)
		if PlatformManager.is_touch_primary():
			del_btn.visible = true
		else:
			item_btn.mouse_entered.connect(func() -> void: del_btn.visible = true)
			item_btn.mouse_exited.connect(func() -> void:
				if not Rect2(Vector2.ZERO, item_btn.size).has_point(item_btn.get_local_mouse_position()):
					del_btn.visible = false
			)

	_refresh_dynamic_groups_styles()

func show_group_drop_indicator(source_idx: int, target_idx: int, above: bool) -> void:
	if not group_drop_indicator:
		return

	group_drop_indicator.color = ThemeManager.current_theme.ACCENT_CORE

	if active_group_drag_source_item and is_instance_valid(active_group_drag_source_item):
		active_group_drag_source_item.modulate.a = 1.0
	if source_idx >= 0 and source_idx < dynamic_groups_container.get_child_count():
		active_group_drag_source_item = dynamic_groups_container.get_child(source_idx)
		active_group_drag_source_item.modulate.a = 0.4

	group_drop_indicator.visible = true
	var new_idx = target_idx
	if not above:
		new_idx += 1
	new_idx = clamp(new_idx, 0, dynamic_groups_container.get_child_count() - 1)
	dynamic_groups_container.move_child(group_drop_indicator, new_idx)

func hide_group_drop_indicator() -> void:
	if group_drop_indicator:
		group_drop_indicator.visible = false
	if active_group_drag_source_item and is_instance_valid(active_group_drag_source_item):
		active_group_drag_source_item.modulate.a = 1.0
		active_group_drag_source_item = null

func _show_delete_group_modal(group: Dictionary) -> void:
	GlassModal.confirm(self, "Delete Dynamic Group",
		"Delete group \"%s\"? This can't be undone." % group.name,
		"Delete",
		func() -> void:
			if DynamicGroupService.active_group_id == group.id and NavManager.current_screen == "dynamic_group":
				NavManager.navigate_to("library")
			DynamicGroupService.delete_group(group.id)
	)

func _show_inline_rename_group(group: Dictionary, item_btn: Button) -> void:
	var line_edit = LineEdit.new()
	line_edit.text = group.name
	line_edit.alignment = HORIZONTAL_ALIGNMENT_LEFT
	_style_inline_edit(line_edit)

	var idx = item_btn.get_index()
	dynamic_groups_container.add_child(line_edit)
	dynamic_groups_container.move_child(line_edit, idx)
	item_btn.visible = false

	line_edit.set_meta("committed", false)
	var commit = func(new_text: String):
		if line_edit.get_meta("committed", false):
			return
		line_edit.set_meta("committed", true)
		var clean = new_text.strip_edges()
		if not clean.is_empty() and clean != group.name:
			DynamicGroupService.rename_group(group.id, clean)
		else:
			call_deferred(&"_rebuild_dynamic_groups_list")

	line_edit.text_submitted.connect(commit)
	line_edit.focus_exited.connect(func(): commit.call(line_edit.text))

	line_edit.grab_focus.call_deferred()
	line_edit.select_all.call_deferred()

func _refresh_dynamic_groups_styles() -> void:
	for child in dynamic_groups_container.get_children():
		if child is Button and child.has_method("_can_drop_data"):
			var is_active = (NavManager.current_screen == "dynamic_group" and DynamicGroupService.active_group_id == child.group_id)
			_style_nav_button(child, is_active)

## sidebar.gd
## Desktop navigation sidebar controller.
## Applies design tokens from ThemeManager to the panel and nav buttons,
## and keeps active-item highlights in sync with NavManager signals.
extends PanelContainer

@onready var app_name:     Label  = $VBox/Header/LogoContainer/AppName
@onready var nav_library:  Button = $VBox/Scroll/ScrollVBox/NavItems/NavLibrary
@onready var nav_vibe:   Button = $VBox/Scroll/ScrollVBox/NavItems/NavVibe
@onready var nav_settings: Button = $VBox/Scroll/ScrollVBox/NavItems/NavSettings
@onready var toggle_playlists_btn: Button = $VBox/Scroll/ScrollVBox/NavItems/PlaylistsHeader/TogglePlaylistsBtn
@onready var add_playlist_btn: Button = $VBox/Scroll/ScrollVBox/NavItems/PlaylistsHeader/AddPlaylistBtn
@onready var playlists_container: VBoxContainer = $VBox/Scroll/ScrollVBox/NavItems/PlaylistsContainer

@onready var track_title_label: Label = $VBox/NowPlaying/HBox/Info/TrackTitle
@onready var artist_label: Label = $VBox/NowPlaying/HBox/Info/ArtistLabel
@onready var play_pause_btn: Button = $VBox/PlayerTiles/PlayPauseBtn
@onready var backward_btn: Button = $VBox/PlayerTiles/ControlsHBox/BackwardBtn
@onready var forward_btn: Button = $VBox/PlayerTiles/ControlsHBox/ForwardBtn
@onready var shuffle_btn: Button = $VBox/PlayerTiles/ShuffleBtn


@onready var preview_square: AspectRatioContainer = $VBox/Scroll/ScrollVBox/PreviewSquare
@onready var preview_texture: TextureRect = $VBox/Scroll/ScrollVBox/PreviewSquare/PreviewTexture
@onready var blur_overlay: Panel = $VBox/Scroll/ScrollVBox/PreviewSquare/BlurOverlay
@onready var lyrics_scroll: ScrollContainer = $VBox/Scroll/ScrollVBox/PreviewSquare/LyricsScroll
@onready var lyrics_container: VBoxContainer = $VBox/Scroll/ScrollVBox/PreviewSquare/LyricsScroll/LyricsContainer

var lyrics_list: Array = []
var active_line_index: int = -1
var is_synced_lyrics := false


var drop_indicator: ColorRect = null
var active_drag_source_item: Control = null

## Maps screen-name strings to their corresponding Button nodes.
var _nav_buttons: Dictionary = {}
var _dragging := false
var _custom_stylebox: StyleBoxFlat = null
var _playlists_visible := true


func _ready() -> void:
	if has_theme_stylebox_override("panel"):
		var sb = get_theme_stylebox("panel")
		if sb is StyleBoxFlat:
			_custom_stylebox = sb.duplicate()
			add_theme_stylebox_override("panel", _custom_stylebox)

	# Enable drag clicks to pass through to the PanelContainer background
	$VBox.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Header.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Header/LogoContainer.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Scroll.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Scroll/ScrollVBox.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/Scroll/ScrollVBox/NavItems.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/NowPlaying.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/NowPlaying/HBox.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/NowPlaying/HBox/Info.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/PlayerTiles.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/PlayerTiles/ControlsHBox.mouse_filter = Control.MOUSE_FILTER_PASS

	preview_square.visible = false

	_apply_panel_style()
	_apply_logo_style()
	_register_nav_buttons()
	_connect_signals()
	_connect_audio_signals()
	_connect_metadata_signals()
	_set_active_nav(NavManager.current_screen)
	_style_player_buttons()
	_setup_blur_shader()
	_setup_playlists_ui()
	ThemeManager.theme_changed.connect(_apply_panel_style)
	ThemeManager.theme_changed.connect(_apply_logo_style)
	ThemeManager.theme_changed.connect(_refresh_nav_button_styles)
	ThemeManager.theme_changed.connect(_style_player_buttons)
	ThemeManager.theme_changed.connect(_style_playlists_header_buttons)



# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_panel_style() -> void:
	if _custom_stylebox:
		_custom_stylebox.bg_color = ThemeManager.current_theme.BG_BASE
		_custom_stylebox.border_color = ThemeManager.current_theme.GLASS_BORDER_SUBTLE
	else:
		add_theme_stylebox_override("panel", ThemeManager.make_nav_panel())
	var sw = ThemeManager.current_theme.SIDEBAR_WIDTH
	custom_minimum_size.x = sw
	if preview_square:
		var sq_size = float(sw) - 32.0
		preview_square.custom_minimum_size = Vector2(sq_size, sq_size)


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

func _style_nav_button(btn: Button, active: bool) -> void:
	btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	btn.custom_minimum_size.y = ThemeManager.current_theme.TOUCH_TARGET_MIN
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
		
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				_dragging = true
				get_viewport().set_input_as_handled()
			else:
				if _dragging:
					_dragging = false
					get_viewport().set_input_as_handled()
	elif event is InputEventMouseMotion and _dragging:
		get_window().position += Vector2i(event.relative)
		get_viewport().set_input_as_handled()


# ---------------------------------------------------------------------------
# Player Controls & Now Playing
# ---------------------------------------------------------------------------

func _connect_audio_signals() -> void:
	AudioManager.track_changed.connect(_on_track_changed)
	AudioManager.playback_toggled.connect(_on_playback_toggled)
	
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
		var normal_style = tile_normal.duplicate()
		var hover_style = tile_hover.duplicate()
		var pressed_style = tile_pressed.duplicate()
		
		if btn == shuffle_btn:
			normal_style.set_corner_radius(3, theme.RADIUS_LG) # CORNER_BOTTOM_LEFT
			normal_style.set_corner_radius(2, theme.RADIUS_LG) # CORNER_BOTTOM_RIGHT
			hover_style.set_corner_radius(3, theme.RADIUS_LG)
			hover_style.set_corner_radius(2, theme.RADIUS_LG)
			pressed_style.set_corner_radius(3, theme.RADIUS_LG)
			pressed_style.set_corner_radius(2, theme.RADIUS_LG)
			
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
	var clean_name = track_name
	var artist_name = ""
	if " - " in track_name:
		var parts = track_name.split(" - ")
		clean_name = parts[parts.size() - 1]
		if parts.size() > 1:
			artist_name = parts[0]
			for i in range(1, parts.size() - 1):
				artist_name += " - " + parts[i]
				
	track_title_label.text = clean_name
	artist_label.text = artist_name if artist_name != "" else "Unknown Artist"

	if is_instance_valid(MetadataService) and AudioManager.current_track_index != -1 and AudioManager.current_track_index < AudioManager.current_playlist.size():
		var href = AudioManager.current_playlist[AudioManager.current_track_index]
		MetadataService.focus_track_by_href(href)



func _on_playback_toggled(is_playing: bool) -> void:
	if is_playing:
		play_pause_btn.text = "⏸ Pause"
	else:
		play_pause_btn.text = "▶ Play"

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
			_on_track_focused("", "", "", lyrics, "")

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
		
	var img = Image.load_from_file(path)
	if img:
		var tex = ImageTexture.create_from_image(img)
		preview_texture.texture = tex
	else:
		preview_texture.texture = null

func _on_artist_focused(_artist: String, image_path: String) -> void:
	_load_image_to_texture(image_path)
	blur_overlay.visible = false
	lyrics_scroll.visible = false

func _on_album_focused(_artist: String, _album: String, image_path: String) -> void:
	_load_image_to_texture(image_path)
	blur_overlay.visible = false
	lyrics_scroll.visible = false

func _on_track_focused(_artist: String, _album: String, _title: String, lyrics: Dictionary, image_path: String) -> void:
	if not image_path.is_empty():
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

func _process(_delta: float) -> void:
	if is_synced_lyrics and lyrics_scroll.visible and AudioManager.is_playing:
		if AudioManager.player and AudioManager.player.is_inside_tree():
			var song_time = AudioManager.get_playback_position()
			_update_lyrics_scroller(song_time)

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
	
	if PlaylistService:
		PlaylistService.playlist_created.connect(func(_p): call_deferred(&"_rebuild_playlists_list"))
		PlaylistService.playlist_deleted.connect(func(_id): call_deferred(&"_rebuild_playlists_list"))
		PlaylistService.playlist_renamed.connect(func(_id, _name): call_deferred(&"_rebuild_playlists_list"))
		PlaylistService.playlists_loaded.connect(func(): call_deferred(&"_rebuild_playlists_list"))
	
	# Initialize Drop Indicator for Playlists
	drop_indicator = ColorRect.new()
	drop_indicator.custom_minimum_size.y = 3
	drop_indicator.mouse_filter = Control.MOUSE_FILTER_IGNORE
	drop_indicator.visible = false
	playlists_container.add_child(drop_indicator)
	
	_style_playlists_header_buttons()
	_rebuild_playlists_list()

func _style_playlists_header_buttons() -> void:
	var theme = ThemeManager.current_theme
	
	toggle_playlists_btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	toggle_playlists_btn.custom_minimum_size.y = theme.TOUCH_TARGET_MIN
	toggle_playlists_btn.add_theme_font_override("font", theme.font_ui)
	toggle_playlists_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	toggle_playlists_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	toggle_playlists_btn.add_theme_color_override("font_hover_color", theme.TEXT_SECONDARY)
	toggle_playlists_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	toggle_playlists_btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	toggle_playlists_btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	toggle_playlists_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	
	add_playlist_btn.flat = true
	add_playlist_btn.custom_minimum_size.y = theme.TOUCH_TARGET_MIN
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
	line_edit.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_SM, 0.3))
	line_edit.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	line_edit.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	line_edit.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	
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
	
	line_edit.text_submitted.connect(commit)
	line_edit.focus_exited.connect(func(): commit.call(line_edit.text))
	
	line_edit.grab_focus.call_deferred()

func _rebuild_playlists_list() -> void:
	for child in playlists_container.get_children():
		if child != drop_indicator:
			child.queue_free()
		
	if not PlaylistService:
		return
		
	var playlists_list = PlaylistService.get_playlists()
	for playlist in playlists_list:
		var item_btn = Button.new()
		item_btn.set_script(preload("res://scripts/ui/sidebar_playlist_item.gd"))
		item_btn.playlist_id = playlist.id
		item_btn.playlist_name = playlist.name
		item_btn.text = "    ♪  " + playlist.name
		playlists_container.add_child(item_btn)
		
		item_btn.pressed.connect(func():
			PlaylistService.active_playlist_id = playlist.id
			NavManager.navigate_to("playlist")
		)
		
		item_btn.gui_input.connect(func(event):
			if event is InputEventMouseButton and event.double_click and event.button_index == MOUSE_BUTTON_LEFT:
				_show_inline_rename(playlist, item_btn)
		)
		
	_refresh_playlists_styles()

func show_drop_indicator(source_idx: int, target_idx: int, above: bool) -> void:
	if not drop_indicator:
		return
		
	drop_indicator.color = ThemeManager.current_theme.ACCENT_CORE
	
	if active_drag_source_item and is_instance_valid(active_drag_source_item):
		active_drag_source_item.modulate.a = 1.0
	if source_idx >= 0 and source_idx < playlists_container.get_child_count():
		active_drag_source_item = playlists_container.get_child(source_idx)
		active_drag_source_item.modulate.a = 0.4
		
	drop_indicator.visible = true
	var new_idx = target_idx
	if not above:
		new_idx += 1
	new_idx = clamp(new_idx, 0, playlists_container.get_child_count() - 1)
	playlists_container.move_child(drop_indicator, new_idx)

func hide_drop_indicator() -> void:
	if drop_indicator:
		drop_indicator.visible = false
	if active_drag_source_item and is_instance_valid(active_drag_source_item):
		active_drag_source_item.modulate.a = 1.0
		active_drag_source_item = null

func _show_inline_rename(playlist: Dictionary, item_btn: Button) -> void:
	var line_edit = LineEdit.new()
	line_edit.text = playlist.name
	line_edit.alignment = HORIZONTAL_ALIGNMENT_LEFT
	line_edit.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_SM, 0.3))
	line_edit.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	line_edit.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	line_edit.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	
	var idx = item_btn.get_index()
	playlists_container.add_child(line_edit)
	playlists_container.move_child(line_edit, idx)
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
			_rebuild_playlists_list()

	line_edit.text_submitted.connect(commit)
	line_edit.focus_exited.connect(func(): commit.call(line_edit.text))
	
	line_edit.grab_focus.call_deferred()
	line_edit.select_all.call_deferred()

func _refresh_playlists_styles() -> void:
	for child in playlists_container.get_children():
		if child is Button and child.has_method("_can_drop_data"):
			var is_active = (NavManager.current_screen == "playlist" and PlaylistService.active_playlist_id == child.playlist_id)
			_style_nav_button(child, is_active)

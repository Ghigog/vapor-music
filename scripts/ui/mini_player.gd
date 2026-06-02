## mini_player.gd
## Bottom UI controller component handling merged navigation and playback.
extends Control

@onready var progress_bar: HSlider = $VBox/ProgressBar
@onready var loading_bar: ProgressBar = $LoadingBar

@onready var nav_library: Button = $VBox/HBox/NavLibrary
@onready var nav_search: Button = $VBox/HBox/NavSearch
@onready var backward_btn: Button = $VBox/HBox/BackwardBtn
@onready var play_pause_btn: Button = $VBox/HBox/PlayPauseBtn
@onready var forward_btn: Button = $VBox/HBox/ForwardBtn
@onready var nav_settings: Button = $VBox/HBox/NavSettings
@onready var future_btn: Button = $VBox/HBox/FutureBtn

var dragging = false
var _panel_dragging = false


func _ready() -> void:
	# Configure container mouse filters to pass clicks through to panel background for window dragging
	$VBox.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/HBox.mouse_filter = Control.MOUSE_FILTER_PASS
	
	# Connect to our global audio singleton signals
	AudioManager.playback_toggled.connect(_on_playback_toggled)
	AudioManager.loading_track.connect(_on_loading_track)
	
	# Connect to dynamic ThemeManager updates
	ThemeManager.theme_changed.connect(_apply_styles)
	_apply_styles()


func _process(delta: float) -> void:
	if AudioManager.is_playing and !dragging:
		if AudioManager.player and AudioManager.player.is_inside_tree():
			progress_bar.value = AudioManager.player.get_playback_position()


# ---------------------------------------------------------------------------
# Theme System & Dynamic Styling
# ---------------------------------------------------------------------------

func _apply_styles() -> void:
	if not is_inside_tree():
		return
		
	var theme := ThemeManager.current_theme
	var is_dark := theme.BG_VOID.v < 0.5
	
	# 1. Root Container Style (Glass panel, rounded only at bottom corners to match main window)
	var style = ThemeManager.make_glass_panel(0)
	style.shadow_size = 0
	style.set_corner_radius(2, theme.RADIUS_LG) # CORNER_BOTTOM_RIGHT
	style.set_corner_radius(3, theme.RADIUS_LG) # CORNER_BOTTOM_LEFT
	add_theme_stylebox_override("panel", style)
	custom_minimum_size.y = theme.MINI_PLAYER_HEIGHT

	
	# 2. Layout Container spacing
	if has_node("VBox/HBox"):
		$VBox/HBox.add_theme_constant_override("separation", theme.SPACE_3)
		
	# 3. Style all buttons similarly
	var hover_bg := Color(1.0, 1.0, 1.0, 0.06) if is_dark else Color(0.0, 0.0, 0.0, 0.06)
	var pressed_bg := Color(1.0, 1.0, 1.0, 0.12) if is_dark else Color(0.0, 0.0, 0.0, 0.12)
	
	var buttons = [nav_library, nav_search, backward_btn, play_pause_btn, forward_btn, nav_settings, future_btn]
	for btn in buttons:
		if btn:
			btn.custom_minimum_size = Vector2(theme.TOUCH_TARGET_MIN, theme.TOUCH_TARGET_MIN)
			btn.add_theme_font_override("font", theme.font_ui)
			btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
			btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
			btn.add_theme_stylebox_override("hover", _make_circle_button_style(hover_bg))
			btn.add_theme_stylebox_override("pressed", _make_circle_button_style(pressed_bg))
			btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
			
			btn.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
			btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
			btn.add_theme_color_override("font_pressed_color", theme.ACCENT_DIM)
			btn.add_theme_color_override("font_focus_color", theme.TEXT_SECONDARY)

	# 4. Scrubber Bar (HSlider / progress_bar)
	if progress_bar:
		var slider_bg := StyleBoxFlat.new()
		slider_bg.bg_color = Color(1.0, 1.0, 1.0, 0.12) if is_dark else Color(0.0, 0.0, 0.0, 0.12)
		slider_bg.content_margin_top = 2
		slider_bg.content_margin_bottom = 2
		slider_bg.expand_margin_top = -2
		slider_bg.expand_margin_bottom = -2
		slider_bg.set_corner_radius_all(theme.RADIUS_PILL)
		progress_bar.add_theme_stylebox_override("slider", slider_bg)
		
		var slider_fill := StyleBoxFlat.new()
		slider_fill.bg_color = theme.ACCENT_CORE
		slider_fill.content_margin_top = 2
		slider_fill.content_margin_bottom = 2
		slider_fill.expand_margin_top = -2
		slider_fill.expand_margin_bottom = -2
		slider_fill.set_corner_radius_all(theme.RADIUS_PILL)
		progress_bar.add_theme_stylebox_override("grabber_area", slider_fill)
		
		var slider_fill_hl := StyleBoxFlat.new()
		slider_fill_hl.bg_color = theme.ACCENT_BRIGHT
		slider_fill_hl.content_margin_top = 2
		slider_fill_hl.content_margin_bottom = 2
		slider_fill_hl.expand_margin_top = -2
		slider_fill_hl.expand_margin_bottom = -2
		slider_fill_hl.set_corner_radius_all(theme.RADIUS_PILL)
		progress_bar.add_theme_stylebox_override("grabber_area_highlight", slider_fill_hl)
		
		# Custom generated circle textures for the grabber thumb (visible at rest, scales on hover)
		var grabber_normal := _create_circle_texture(4, theme.ACCENT_CORE)
		var grabber_hover := _create_circle_texture(7, theme.ACCENT_BRIGHT)
		progress_bar.add_theme_icon_override("grabber", grabber_normal)
		progress_bar.add_theme_icon_override("grabber_highlight", grabber_hover)


	# 5. LoadingBar (ProgressBar)
	if loading_bar:
		var loading_bg := StyleBoxFlat.new()
		loading_bg.bg_color = Color(1.0, 1.0, 1.0, 0.05) if is_dark else Color(0.0, 0.0, 0.0, 0.05)
		loading_bg.set_corner_radius_all(theme.RADIUS_PILL)
		loading_bar.add_theme_stylebox_override("background", loading_bg)
		
		var loading_fill := StyleBoxFlat.new()
		loading_fill.bg_color = theme.AQUA_CORE
		loading_fill.set_corner_radius_all(theme.RADIUS_PILL)
		loading_bar.add_theme_stylebox_override("fill", loading_fill)


func _make_circle_button_style(bg_color: Color, border_color: Color = Color.TRANSPARENT, margin_left: int = 0) -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = bg_color
	s.border_color = border_color
	if border_color != Color.TRANSPARENT:
		s.set_border_width_all(1)
	s.set_corner_radius_all(ThemeManager.current_theme.RADIUS_PILL)
	s.content_margin_left = margin_left
	return s


func _create_circle_texture(radius: int, color: Color) -> ImageTexture:
	var size := radius * 2
	var img := Image.create(size, size, false, Image.FORMAT_RGBA8)
	for y in range(size):
		for x in range(size):
			var dist := Vector2(x - radius + 0.5, y - radius + 0.5).length()
			if dist < radius:
				var alpha := clampf(radius - dist, 0.0, 1.0)
				img.set_pixel(x, y, Color(color.r, color.g, color.b, color.a * alpha))
	return ImageTexture.create_from_image(img)


# ---------------------------------------------------------------------------
# Navigation Click Handlers
# ---------------------------------------------------------------------------

func _on_nav_library_pressed() -> void:
	NavManager.navigate_to("library")


func _on_nav_search_pressed() -> void:
	NavManager.navigate_to("search")


func _on_nav_settings_pressed() -> void:
	NavManager.navigate_to("settings")


# ---------------------------------------------------------------------------
# Audio / Playback Handlers
# ---------------------------------------------------------------------------

func _on_playback_toggled(is_playing: bool) -> void:
	progress_bar.max_value = AudioManager.current_track_length
	if play_pause_btn:
		if is_playing:
			play_pause_btn.text = "⏸"
		else:
			play_pause_btn.text = "▶"


func _on_play_pause_pressed() -> void:
	AudioManager.toggle_play()


func _on_progress_bar_drag_started() -> void:
	dragging = true


func _on_progress_bar_drag_ended(value_changed: bool) -> void:
	dragging = false
	AudioManager.scroll_track(progress_bar.value)


func _on_progress_bar_value_changed(value: float) -> void:
	pass


func _on_loading_track(is_loading: bool) -> void:
	if loading_bar:
		loading_bar.visible = is_loading


func _on_backward_btn_pressed() -> void:
	AudioManager.play_previous()


func _on_forward_btn_pressed() -> void:
	AudioManager.play_next()


# ---------------------------------------------------------------------------
# Bottom Bar Window Dragging
# ---------------------------------------------------------------------------

func _gui_input(event: InputEvent) -> void:
	if not PlatformManager.is_desktop():
		return
		
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				_panel_dragging = true
				get_viewport().set_input_as_handled()
			else:
				if _panel_dragging:
					_panel_dragging = false
					get_viewport().set_input_as_handled()
	elif event is InputEventMouseMotion and _panel_dragging:
		get_window().position += Vector2i(event.relative)
		get_viewport().set_input_as_handled()

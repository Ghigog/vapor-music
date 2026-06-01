## mini_player.gd
## Bottom UI controller component handling active state indicators and playback toggles.
## Hooks into ThemeManager to dynamically adapt colors, fonts, sizes, and StyleBoxes.
extends Control

# Hardened onready node paths referencing your exact scene tree hierarchy
@onready var track_title_label: Label = $HBox/ProgressBar/TrackInfo/TrackTitle
@onready var artist_label: Label = $HBox/ProgressBar/TrackInfo/ArtistLabel
@onready var play_pause_btn: Button = $HBox/PlayPauseBtn
@onready var progress_bar: HSlider = $HBox/ProgressBar
@onready var loading_bar: ProgressBar = $LoadingBar
@onready var album_art: Panel = $HBox/AlbumArt
@onready var backward_btn: Button = $HBox/BackwardBtn
@onready var forward_btn: Button = $HBox/ForwardBtn

var dragging = false


func _ready() -> void:
	# Connect to our global audio singleton signals
	AudioManager.track_changed.connect(_on_track_changed)
	AudioManager.playback_toggled.connect(_on_playback_toggled)
	AudioManager.loading_track.connect(_on_loading_track)
	
	if play_pause_btn:
		play_pause_btn.pressed.connect(_on_play_pause_pressed)
		
	# Connect to dynamic ThemeManager updates
	ThemeManager.theme_changed.connect(_apply_styles)
	_apply_styles()


func _process(delta: float) -> void:
	if AudioManager.is_playing and !dragging:
		progress_bar.value = AudioManager.player.get_playback_position()


# ---------------------------------------------------------------------------
# Theme System & Dynamic Styling
# ---------------------------------------------------------------------------

## Applies all visual tokens from the active ThemeManager theme to the miniplayer controls.
func _apply_styles() -> void:
	if not is_inside_tree():
		return
		
	var theme := ThemeManager.current_theme
	var is_dark := theme.BG_VOID.v < 0.5
	
	# 1. Root Container Style (Glass panel, --radius-lg)
	add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_LG))
	custom_minimum_size.y = theme.MINI_PLAYER_HEIGHT
	
	# 2. Layout Container spacing
	if has_node("HBox"):
		$HBox.add_theme_constant_override("separation", theme.SPACE_3)
		
	# 3. Album Art Placeholder (Circular, 40x40)
	if album_art:
		album_art.add_theme_stylebox_override("panel", ThemeManager.make_circle_placeholder())
		album_art.custom_minimum_size = Vector2(40, 40)
		
	# 4. Standard Navigation Buttons (Backward, Forward)
	var hover_bg := Color(1.0, 1.0, 1.0, 0.06) if is_dark else Color(0.0, 0.0, 0.0, 0.06)
	var pressed_bg := Color(1.0, 1.0, 1.0, 0.12) if is_dark else Color(0.0, 0.0, 0.0, 0.12)
	
	for btn in [backward_btn, forward_btn]:
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
			
	# 5. Play/Pause Button (Signature styled circular button per §7.3)
	if play_pause_btn:
		play_pause_btn.custom_minimum_size = Vector2(theme.TOUCH_TARGET_MIN, theme.TOUCH_TARGET_MIN)
		play_pause_btn.add_theme_font_override("font", theme.font_ui)
		play_pause_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
		
		var play_normal := _make_circle_button_style(theme.ACCENT_SURFACE, theme.ACCENT_CORE)
		var play_hover := _make_circle_button_style(theme.ACCENT_SURFACE * 1.5, theme.ACCENT_BRIGHT)
		var play_pressed := _make_circle_button_style(theme.ACCENT_SURFACE * 0.8, theme.ACCENT_DIM)
		
		play_pause_btn.add_theme_stylebox_override("normal", play_normal)
		play_pause_btn.add_theme_stylebox_override("hover", play_hover)
		play_pause_btn.add_theme_stylebox_override("pressed", play_pressed)
		play_pause_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		
		play_pause_btn.add_theme_color_override("font_color", theme.ACCENT_BRIGHT)
		play_pause_btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
		play_pause_btn.add_theme_color_override("font_pressed_color", theme.ACCENT_DIM)
		play_pause_btn.add_theme_color_override("font_focus_color", theme.ACCENT_BRIGHT)

	# 6. Scrubber Bar (HSlider / progress_bar)
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
		
		# Custom generated circle textures for the grabber thumb (invisible at rest, 14px on hover)
		var grabber_normal := _create_circle_texture(3, Color(0, 0, 0, 0))
		var grabber_hover := _create_circle_texture(7, theme.ACCENT_BRIGHT)
		progress_bar.add_theme_icon_override("grabber", grabber_normal)
		progress_bar.add_theme_icon_override("grabber_highlight", grabber_hover)

	# 7. LoadingBar (ProgressBar)
	if loading_bar:
		var loading_bg := StyleBoxFlat.new()
		loading_bg.bg_color = Color(1.0, 1.0, 1.0, 0.05) if is_dark else Color(0.0, 0.0, 0.0, 0.05)
		loading_bg.set_corner_radius_all(theme.RADIUS_PILL)
		loading_bar.add_theme_stylebox_override("background", loading_bg)
		
		var loading_fill := StyleBoxFlat.new()
		loading_fill.bg_color = theme.AQUA_CORE
		loading_fill.set_corner_radius_all(theme.RADIUS_PILL)
		loading_bar.add_theme_stylebox_override("fill", loading_fill)

	# 8. Typography and Labels (TrackTitle, ArtistLabel)
	if track_title_label:
		track_title_label.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
		track_title_label.add_theme_font_override("font", theme.font_ui)
		track_title_label.add_theme_font_size_override("font_size", theme.TYPE_SM)
		
	if artist_label:
		artist_label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
		artist_label.add_theme_font_override("font", theme.font_ui)
		artist_label.add_theme_font_size_override("font_size", theme.TYPE_XS)


## Returns a circular flat StyleBox for button highlights.
func _make_circle_button_style(bg_color: Color, border_color: Color = Color.TRANSPARENT) -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = bg_color
	s.border_color = border_color
	if border_color != Color.TRANSPARENT:
		s.set_border_width_all(1)
	s.set_corner_radius_all(ThemeManager.current_theme.RADIUS_PILL)
	return s


## Generates a smooth, antialiased circular texture of a given radius and color.
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
# Audio / Signal Event Handlers
# ---------------------------------------------------------------------------

func _on_track_changed(track_name: String) -> void:
	if track_title_label:
		# If the filename contains a long prefix like "Vanilla - Origin - ", 
		# we clean it up so just the song title displays nicely in the small bar!
		var clean_name := track_name
		var artist_name := ""
		if " - " in track_name:
			var parts := track_name.split(" - ")
			clean_name = parts[parts.size() - 1] # Grabs just the song name at the end
			if parts.size() > 1:
				# Assemble artist name from previous parts
				artist_name = parts[0]
				for i in range(1, parts.size() - 1):
					artist_name += " - " + parts[i]
			
		track_title_label.text = clean_name
		if artist_label:
			artist_label.text = artist_name if artist_name != "" else "Unknown Artist"


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


func _on_loading_track(is_loading: bool) -> void:
	if loading_bar:
		loading_bar.visible = is_loading


func _on_backward_btn_pressed() -> void:
	AudioManager.play_previous()


func _on_forward_btn_pressed() -> void:
	AudioManager.play_next()

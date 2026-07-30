## mini_player.gd
## Bottom UI controller component handling merged navigation and playback.
extends Control

const ICON_LIBRARY := preload("res://assets/icon/library-cropped.png")
const ICON_VIBE := preload("res://assets/icon/vibe-cropped.png")
const ICON_PLAYLISTS := preload("res://assets/icon/playlist-cropped.png")
const ICON_SETTINGS := preload("res://assets/icon/settings-cropped.png")
const ICON_PLAY := preload("res://assets/icon/play-cropped.png")
const ICON_PAUSE := preload("res://assets/icon/pause-cropped.png")
const ICON_NEXT := preload("res://assets/icon/next-cropped.png")

## "Previous" is just "Next" mirrored — no separate asset. Cached lazily
## since flipping is a one-off cost, not something to redo per instance.
static var _icon_prev_cache: ImageTexture = null

static func _icon_prev() -> ImageTexture:
	if _icon_prev_cache == null:
		var img := ICON_NEXT.get_image().duplicate()
		img.flip_x()
		_icon_prev_cache = ImageTexture.create_from_image(img)
	return _icon_prev_cache

@onready var progress_bar: HSlider = $VBox/ProgressBar
@onready var loading_bar: ProgressBar = $VBox/LoadingBar

@onready var nav_library: Button = $VBox/HBox/NavLibrary
@onready var nav_vibe: Button = $VBox/HBox/NavVibe
@onready var nav_playlists: Button = $VBox/HBox/NavPlaylists
@onready var backward_btn: Button = $VBox/HBox/BackwardBtn
@onready var play_pause_btn: Button = $VBox/HBox/PlayPauseBtn
@onready var forward_btn: Button = $VBox/HBox/ForwardBtn
@onready var nav_settings: Button = $VBox/HBox/NavSettings
@onready var shuffle_btn: CheckBox = $VBox/HBox/ShuffleBtn

var playlist_popup: Control = null


var dragging = false
var _custom_stylebox: StyleBoxFlat = null
var _track_loading = false
var _transitioning = false


func _ready() -> void:
	if nav_library:   nav_library.icon   = ICON_LIBRARY
	if nav_vibe:      nav_vibe.icon      = ICON_VIBE
	if nav_playlists: nav_playlists.icon = ICON_PLAYLISTS
	if nav_settings:  nav_settings.icon  = ICON_SETTINGS
	# Smart Mixing / shuffle is the same concept as Vibe (the AI DJ), so it
	# reuses that icon everywhere rather than a distinct glyph.
	if shuffle_btn: shuffle_btn.icon = ICON_VIBE
	# Button draws icon AND text side by side — these buttons only ever want
	# the icon, so their scene-default glyph text must be cleared or it
	# renders a second, duplicate arrow next to the real icon.
	if play_pause_btn: play_pause_btn.icon = ICON_PLAY
	if play_pause_btn: play_pause_btn.text = ""
	if backward_btn:  backward_btn.icon = _icon_prev()
	if backward_btn:  backward_btn.text = ""
	if forward_btn:   forward_btn.icon = ICON_NEXT
	if forward_btn:   forward_btn.text = ""
	# These three are always icon-only (never grow text like the nav buttons
	# below do) — Button.icon_alignment defaults to LEFT independently of the
	# text alignment, so without this the icon sits pinned to the button's
	# left edge instead of centered in it.
	for icon_only_btn in [play_pause_btn, backward_btn, forward_btn]:
		if icon_only_btn:
			icon_only_btn.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER

	if has_theme_stylebox_override("panel"):
		var sb = get_theme_stylebox("panel")
		if sb is StyleBoxFlat:
			_custom_stylebox = sb.duplicate()
			add_theme_stylebox_override("panel", _custom_stylebox)

	# Configure container mouse filters to pass clicks through to panel background for window dragging
	$VBox.mouse_filter = Control.MOUSE_FILTER_PASS
	$VBox/HBox.mouse_filter = Control.MOUSE_FILTER_PASS
	
	# Connect to our global audio singleton signals
	AudioManager.playback_toggled.connect(_on_playback_toggled)
	AudioManager.loading_track.connect(_on_loading_track)
	if AudioManager.has_signal("transition_started"):
		AudioManager.transition_started.connect(func(_next_track, _type):
			_transitioning = true
			_update_progress_visibility()
		)
	if AudioManager.has_signal("transition_completed"):
		AudioManager.transition_completed.connect(func(_track):
			_transitioning = false
			_update_progress_visibility()
		)
	AudioManager.length_changed.connect(func(length):
		progress_bar.max_value = length
	)
	
	if shuffle_btn:
		shuffle_btn.button_pressed = AudioManager.smart_mixing_enabled
		shuffle_btn.toggled.connect(func(pressed):
			AudioManager.smart_mixing_enabled = pressed
		)
		AudioManager.smart_mixing_toggled.connect(func(enabled):
			shuffle_btn.button_pressed = enabled
		)
	_update_progress_visibility()

	NavManager.navigation_requested.connect(func(_screen_name):
		if playlist_popup:
			playlist_popup.hide_popup()
	)
	
	# Connect to dynamic ThemeManager updates
	ThemeManager.theme_changed.connect(_apply_styles)
	_apply_styles()
	
	AudioManager.position_changed.connect(_on_position_changed)

	# Connect to resized signal for responsive layout adjustments
	resized.connect(_on_resized)
	_on_resized()


## Driven by AudioManager.position_changed rather than a per-frame poll.
func _on_position_changed(pos: float) -> void:
	if not dragging:
		if AudioManager.player and AudioManager.player.is_inside_tree():
			progress_bar.value = pos


# ---------------------------------------------------------------------------
# Theme System & Dynamic Styling
# ---------------------------------------------------------------------------

func _apply_styles() -> void:
	if not is_inside_tree():
		return
		
	var theme := ThemeManager.current_theme
	var is_dark := theme.BG_VOID.v < 0.5
	
	# 1. Root Container Style (Glass panel, rounded only at bottom corners to match main window)
	var style: StyleBoxFlat
	if _custom_stylebox:
		style = _custom_stylebox
		style.bg_color = Color(theme.BG_GLASS.r, theme.BG_GLASS.g, theme.BG_GLASS.b, style.bg_color.a)
		style.border_color = Color(theme.GLASS_BORDER.r, theme.GLASS_BORDER.g, theme.GLASS_BORDER.b, style.border_color.a)
	else:
		style = ThemeManager.make_glass_panel(0)
		style.shadow_size = 0
		add_theme_stylebox_override("panel", style)

	# Prevent borders on left, right, and bottom from clashing/overlapping with the parent window border
	style.set_border_width_all(0)
	style.border_width_top = 1
	# Adjust corner radius to fit inside the parent window's corner radius (RADIUS_LG - margins)
	var inner_radius = max(0, theme.RADIUS_LG - 12)
	style.set_corner_radius(0, 0) # CORNER_TOP_LEFT
	style.set_corner_radius(1, 0) # CORNER_TOP_RIGHT
	style.set_corner_radius(2, inner_radius) # CORNER_BOTTOM_RIGHT
	style.set_corner_radius(3, inner_radius) # CORNER_BOTTOM_LEFT

	# Zero ALL content margins so the ProgressBar sits flush against the
	# top border line of the panel — no 12 px gap from make_glass_panel default.
	# content_margin_top = -2: pulls the VBox up 2 px so the 4 px HSlider's
	# centre (slider_y + 2) lands exactly on the panel border (y = 0).
	style.content_margin_top = -2
	style.content_margin_left = 0
	style.content_margin_right = 0
	style.content_margin_bottom = 0

	custom_minimum_size.y = theme.MINI_PLAYER_HEIGHT

	
	# 2. Layout Container spacing is handled dynamically in _on_resized() based on width.
		
	# 3. Style all buttons similarly
	var hover_bg := Color(1.0, 1.0, 1.0, 0.06) if is_dark else Color(0.0, 0.0, 0.0, 0.06)
	var pressed_bg := Color(1.0, 1.0, 1.0, 0.12) if is_dark else Color(0.0, 0.0, 0.0, 0.12)
	
	var buttons = [nav_library, nav_vibe, nav_playlists, backward_btn, play_pause_btn, forward_btn, nav_settings, shuffle_btn]
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

			# Icons are plain white silhouettes (assets/icon/*.png) so they can be
			# dynamically tinted here — same colour roles as the font overrides above.
			btn.add_theme_color_override("icon_normal_color", theme.TEXT_SECONDARY)
			btn.add_theme_color_override("icon_hover_color", theme.ACCENT_BRIGHT)
			btn.add_theme_color_override("icon_pressed_color", theme.ACCENT_DIM)
			btn.add_theme_color_override("icon_focus_color", theme.TEXT_SECONDARY)
			# Source PNGs are 512x512 — Button.icon draws at native resolution
			# unless capped, so without this the icon overflows the button entirely.
			btn.add_theme_constant_override("icon_max_width", 20)

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

	# Re-apply responsive sizing so that a theme change doesn't reset narrow-mode
	# button sizes and flags that were set by _on_resized().
	_on_resized()

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


func _on_nav_vibe_pressed() -> void:
	NavManager.navigate_to("vibe")


func _on_nav_settings_pressed() -> void:
	NavManager.navigate_to("settings")


# ---------------------------------------------------------------------------
# Audio / Playback Handlers
# ---------------------------------------------------------------------------

func _on_playback_toggled(is_playing: bool) -> void:
	progress_bar.max_value = AudioManager.current_track_length
	if play_pause_btn:
		if is_playing:
			play_pause_btn.icon = ICON_PAUSE
		else:
			play_pause_btn.icon = ICON_PLAY


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
	_track_loading = is_loading
	_update_progress_visibility()


func _update_progress_visibility() -> void:
	# Local progress/loading indicators are disabled/hidden because they are
	# now fully replaced by the custom horizontal VerticalProgress seeker.
	if loading_bar:
		loading_bar.visible = false
	if progress_bar:
		progress_bar.visible = false


func _on_backward_btn_pressed() -> void:
	AudioManager.play_previous()


func _on_forward_btn_pressed() -> void:
	AudioManager.play_next()


func _on_shuffle_pressed() -> void:
	pass


# ---------------------------------------------------------------------------
# Bottom Bar Window Dragging
# ---------------------------------------------------------------------------

func _gui_input(event: InputEvent) -> void:
	if not PlatformManager.is_desktop():
		return
		
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
		get_window().start_drag()
		get_viewport().set_input_as_handled()


func _on_resized() -> void:
	if not is_inside_tree():
		return
	var theme := ThemeManager.current_theme
	var w := size.x
	
	var hbox: HBoxContainer = $VBox/HBox if has_node("VBox/HBox") else null
	
	if w < 540:
		if nav_library:   nav_library.text   = ""
		if nav_vibe:      nav_vibe.text      = ""
		if nav_playlists: nav_playlists.text = ""
		if nav_settings:  nav_settings.text  = ""
		if shuffle_btn:   shuffle_btn.text   = ""
		# Icon-only now — Button.icon_alignment defaults to LEFT regardless of
		# text, so without this the icon sits pinned to the button's left edge.
		for nav_btn in [nav_library, nav_vibe, nav_playlists, nav_settings, shuffle_btn]:
			if nav_btn:
				nav_btn.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER

		# In narrow mode: zero separation so buttons sit flush/squished,
		# and switch to SIZE_SHRINK_CENTER so they don't expand to fill the panel.
		if hbox:
			hbox.add_theme_constant_override("separation", 0)
		
		# Each button: do NOT expand horizontally — pack them tightly.
		var buttons = [nav_library, nav_vibe, nav_playlists, backward_btn, play_pause_btn, forward_btn, nav_settings, shuffle_btn]
		for btn in buttons:
			if btn:
				btn.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
				# Reduce minimum size slightly to help fit all 8 at narrow widths.
				btn.custom_minimum_size = Vector2(38, theme.TOUCH_TARGET_MIN)
	else:
		if nav_library:   nav_library.text   = "Library"
		if nav_vibe:      nav_vibe.text      = "Vibe"
		if nav_playlists: nav_playlists.text = "Playlists"
		if nav_settings:  nav_settings.text  = "Settings"
		if shuffle_btn:   shuffle_btn.text   = "Smart Mixing"
		# Text is back — icon-then-label reads better left-aligned than centered.
		for nav_btn in [nav_library, nav_vibe, nav_playlists, nav_settings, shuffle_btn]:
			if nav_btn:
				nav_btn.icon_alignment = HORIZONTAL_ALIGNMENT_LEFT

		if hbox:
			hbox.add_theme_constant_override("separation", theme.SPACE_2)
		
		# Wide mode: restore expansion and normal sizing.
		var buttons = [nav_library, nav_vibe, nav_playlists, backward_btn, play_pause_btn, forward_btn, nav_settings, shuffle_btn]
		for btn in buttons:
			if btn:
				btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
				btn.custom_minimum_size = Vector2(theme.TOUCH_TARGET_MIN, theme.TOUCH_TARGET_MIN)


func _on_nav_playlists_pressed() -> void:
	if not playlist_popup:
		var popup_scene = load("res://scenes/ui/mini_player/playlist_popup.tscn")
		if popup_scene:
			playlist_popup = popup_scene.instantiate()
			playlist_popup.top_level = true
			add_child(playlist_popup)
			
	if playlist_popup:
		playlist_popup.toggle_popup(nav_playlists)

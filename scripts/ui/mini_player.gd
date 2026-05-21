## mini_player.gd
## Persistent mini-player strip shown at the bottom of every screen.
## MVP state: displays a polished placeholder (no track loaded).
## Future: AudioEngine signals will populate track info and album art.
extends PanelContainer

@onready var album_art:    Panel  = $HBox/AlbumArt
@onready var track_title:  Label  = $HBox/TrackInfo/TrackTitle
@onready var artist_label: Label  = $HBox/TrackInfo/ArtistLabel
@onready var play_pause:   Button = $HBox/PlayPauseBtn
@onready var like_btn:     Button = $HBox/LikeBtn


func _ready() -> void:
	_apply_styles()
	_apply_placeholder_state()


# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_styles() -> void:
	# Glass strip with horizontal content padding.
	var panel_style := ThemeManager.make_glass_panel(ThemeManager.RADIUS_LG, 0.90)
	panel_style.content_margin_left   = ThemeManager.SPACE_4
	panel_style.content_margin_right  = ThemeManager.SPACE_4
	panel_style.content_margin_top    = ThemeManager.SPACE_3
	panel_style.content_margin_bottom = ThemeManager.SPACE_3
	add_theme_stylebox_override("panel", panel_style)
	custom_minimum_size.y = ThemeManager.MINI_PLAYER_HEIGHT

	# Album art circle placeholder.
	album_art.add_theme_stylebox_override("panel", ThemeManager.make_circle_placeholder())
	album_art.custom_minimum_size = Vector2(40.0, 40.0)

	# Track title.
	track_title.add_theme_color_override("font_color", ThemeManager.TEXT_PRIMARY)
	track_title.add_theme_font_override("font", ThemeManager.font_ui)
	track_title.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)

	# Artist label.
	artist_label.add_theme_color_override("font_color", ThemeManager.TEXT_TERTIARY)
	artist_label.add_theme_font_override("font", ThemeManager.font_ui)
	artist_label.add_theme_font_size_override("font_size", ThemeManager.TYPE_XS)

	# Play / Pause button — circular glass pill.
	play_pause.add_theme_stylebox_override("normal",  ThemeManager.make_glass_panel(ThemeManager.RADIUS_PILL, 0.30))
	play_pause.add_theme_stylebox_override("hover",   ThemeManager.make_glass_panel(ThemeManager.RADIUS_PILL, 0.50))
	play_pause.add_theme_stylebox_override("pressed", ThemeManager.make_glass_panel(ThemeManager.RADIUS_PILL, 0.70))
	play_pause.add_theme_stylebox_override("focus",   ThemeManager.make_transparent())
	play_pause.add_theme_color_override("font_color", ThemeManager.ACCENT_BRIGHT)
	play_pause.add_theme_font_size_override("font_size", ThemeManager.TYPE_MD)
	play_pause.custom_minimum_size = Vector2(44.0, 44.0)

	# Like / heart button.
	like_btn.add_theme_stylebox_override("normal",  ThemeManager.make_transparent())
	like_btn.add_theme_stylebox_override("hover",   ThemeManager.make_nav_item_hover())
	like_btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	like_btn.add_theme_stylebox_override("focus",   ThemeManager.make_transparent())
	like_btn.add_theme_color_override("font_color",       ThemeManager.TEXT_TERTIARY)
	like_btn.add_theme_color_override("font_hover_color", ThemeManager.SEMANTIC_ERROR)
	like_btn.add_theme_font_size_override("font_size", ThemeManager.TYPE_MD)
	like_btn.custom_minimum_size = Vector2(44.0, 44.0)


# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

func _apply_placeholder_state() -> void:
	track_title.text    = "No track playing"
	artist_label.text   = "Open your library to begin"
	play_pause.text     = "▶"
	play_pause.disabled = true
	like_btn.text       = "♡"
	like_btn.disabled   = true


## Called by AudioEngine (future) to display a newly loaded track.
## [param title]  Track display title.
## [param artist] Artist display name.
func set_track(title: String, artist: String) -> void:
	track_title.text    = title
	artist_label.text   = artist
	play_pause.disabled = false
	like_btn.disabled   = false


## Reflects the current playback state on the play/pause icon.
## [param playing] True while audio is actively playing.
func set_playing(playing: bool) -> void:
	play_pause.text = "⏸" if playing else "▶"

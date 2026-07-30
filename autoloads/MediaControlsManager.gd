## MediaControlsManager.gd
## Autoload singleton — bridges AudioManager playback state to OS-level media
## controls: macOS Control Center's Now Playing widget + hardware media keys
## (via MPRemoteCommandCenter/MPNowPlayingInfoCenter), and the Windows System
## Media Transport Controls, plus the in-app space-bar play/pause shortcut.
##
## Uses the same optional-native-extension pattern as AudioDSP (see
## audio_manager.gd): if the platform's MediaControls GDExtension class isn't
## registered, falls back to a no-op stub so this autoload works unchanged on
## platforms without a native bridge (Linux, Android, iOS).

extends Node

const MEDIA_CONTROLS_STUB = preload("res://scripts/services/media_controls_stub.gd")

var _native: Node


func _ready() -> void:
	if ClassDB.class_exists("MediaControls"):
		_native = ClassDB.instantiate("MediaControls")
	else:
		_native = MEDIA_CONTROLS_STUB.new()
	add_child(_native)

	_native.play_requested.connect(_on_play_requested)
	_native.pause_requested.connect(_on_pause_requested)
	_native.toggle_requested.connect(_on_toggle_requested)
	_native.next_requested.connect(_on_next_requested)
	_native.previous_requested.connect(_on_previous_requested)
	_native.seek_requested.connect(_on_seek_requested)

	AudioManager.track_changed.connect(_on_track_changed)
	AudioManager.playback_toggled.connect(_on_playback_toggled)
	AudioManager.position_changed.connect(_on_position_changed)

	_native.register_controls()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo and event.keycode == KEY_SPACE:
		AudioManager.toggle_play()
		get_viewport().set_input_as_handled()


# ---------------------------------------------------------------------------
# OS → app (hardware keys / Control Center / SMTC)
# ---------------------------------------------------------------------------

func _on_play_requested() -> void:
	if not AudioManager.is_playing:
		AudioManager.toggle_play()

func _on_pause_requested() -> void:
	if AudioManager.is_playing:
		AudioManager.toggle_play()

func _on_toggle_requested() -> void:
	AudioManager.toggle_play()

func _on_next_requested() -> void:
	AudioManager.play_next()

func _on_previous_requested() -> void:
	AudioManager.play_previous()

func _on_seek_requested(position: float) -> void:
	AudioManager.scroll_track(position)


# ---------------------------------------------------------------------------
# App → OS (Now Playing info)
# ---------------------------------------------------------------------------

func _on_track_changed(track_name: String) -> void:
	var title := track_name
	var artist := "Unknown Artist"
	var album := "Unknown Album"
	var artwork_path := ""

	if AudioManager.current_track_index >= 0 and AudioManager.current_track_index < AudioManager.current_playlist.size():
		var href: String = AudioManager.current_playlist[AudioManager.current_track_index]
		var meta: Dictionary = MetadataService.get_cached_metadata(href)
		title = meta.get("track_title", title)
		artist = meta.get("artist_name", artist)
		album = meta.get("album_name", album)
		artwork_path = meta.get("album_art_local", "")

	_native.update_now_playing_info(title, artist, album, AudioManager.current_track_length, artwork_path)
	_native.update_playback_state(AudioManager.is_playing, AudioManager.get_playback_position(), AudioManager.current_track_length)


func _on_playback_toggled(is_playing: bool) -> void:
	_native.update_playback_state(is_playing, AudioManager.get_playback_position(), AudioManager.current_track_length)


func _on_position_changed(position: float) -> void:
	_native.update_playback_state(AudioManager.is_playing, position, AudioManager.current_track_length)

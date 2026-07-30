extends Node

# Stub implementation of MediaControls GDExtension class.
# Used when the native class is not compiled/registered on the platform
# (Linux, Android, iOS, or a Windows build that hasn't shipped the extension yet).

signal play_requested()
signal pause_requested()
signal toggle_requested()
signal next_requested()
signal previous_requested()
signal seek_requested(position: float)

func register_controls() -> void:
	pass

func unregister_controls() -> void:
	pass

func update_now_playing_info(_title: String, _artist: String, _album: String, _duration: float, _artwork_path: String) -> void:
	pass

func update_playback_state(_is_playing: bool, _position: float, _duration: float) -> void:
	pass

func clear_now_playing() -> void:
	pass

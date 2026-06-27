extends Node

# Stub implementation of AudioDSP GDExtension class
# Used when C++ AudioDSP is not compiled/registered on the platform (Android/iOS/etc.)

var player: AudioStreamPlayer = null

var _file_path := ""
var _duration := 0.0
var _playback_position := 0.0
var _finished := false

func get_library_version() -> String:
	return "Stub (Essentia & Rubber Band fallback)"

func analyze_file(_file_path_param: String) -> Dictionary:
	# Return a basic default analysis dictionary
	return {
		"bpm": 120.0,
		"camelot_key": "8A",
		"key": "Am",
		"beat_grid": [],
		"segments": {
			"intro": [0.0, 10.0],
			"outro": [180.0, 190.0]
		},
		"transients": []
	}

func stretch_buffer(input_samples: PackedFloat32Array, _ratio: float) -> PackedFloat32Array:
	return input_samples

func analyze_samples(_samples: PackedFloat32Array, _sample_rate: float) -> Dictionary:
	return {}

func load_file(file_path_param: String, duration_hint: float = 0.0) -> bool:
	_file_path = file_path_param
	_duration = duration_hint if duration_hint > 0.0 else 180.0
	_playback_position = 0.0
	_finished = false
	return true

func get_next_chunk(num_frames: int, _ratio: float) -> PackedVector2Array:
	var chunk := PackedVector2Array()
	chunk.resize(num_frames)
	_playback_position += float(num_frames) / 44100.0
	if _playback_position >= _duration:
		_finished = true
	return chunk

func seek_pos(position_seconds: float) -> void:
	_playback_position = clamp(position_seconds, 0.0, get_duration())
	if is_instance_valid(player) and player.playing:
		player.seek(position_seconds)
	_finished = _playback_position >= get_duration()

func get_playback_position() -> float:
	if is_instance_valid(player) and player.playing:
		return player.get_playback_position()
	return _playback_position

func get_duration() -> float:
	if is_instance_valid(player) and player.stream:
		var stream_len = player.stream.get_length()
		if stream_len > 0.0:
			return stream_len
	return _duration

func is_finished() -> bool:
	if is_instance_valid(player):
		return not player.playing
	return _finished

func clear_stream() -> void:
	_file_path = ""
	_playback_position = 0.0
	_finished = true

func get_cache_sample_count() -> int:
	# Positive number to satisfy "dsp.get_cache_sample_count() > 0" checks
	return 44100

func get_cross_correlation_offset(_other_dsp: Object, _self_time: float, _other_time: float, _window_size_sec: float) -> float:
	return 0.0

func get_samples_in_range(_start_time: float, _duration_sec: float) -> Array:
	return []

func get_waveform_peaks(num_bins: int) -> PackedFloat32Array:
	var peaks := PackedFloat32Array()
	peaks.resize(num_bins)
	for i in range(num_bins):
		peaks[i] = 0.1 # Default dummy flat waveform peak
	return peaks

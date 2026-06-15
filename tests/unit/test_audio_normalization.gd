extends GutTest

var dsp: Node
var manager: Node

func before_each() -> void:
	if ClassDB.class_exists("AudioDSP"):
		dsp = ClassDB.instantiate("AudioDSP")
	manager = load("res://scripts/services/audio_manager.gd").new()
	add_child(manager)

func after_each() -> void:
	if is_instance_valid(dsp):
		dsp.free()
	if is_instance_valid(manager):
		manager.queue_free()

func test_silence_detection_threshold() -> void:
	assert_not_null(dsp, "AudioDSP should be instantiated")
	
	var sample_rate := 44100.0
	var silence_secs := 2.0
	var active_secs := 1.0
	
	var total_samples = int((silence_secs + active_secs) * sample_rate)
	var samples := PackedFloat32Array()
	samples.resize(total_samples)
	
	# Fill first 2 seconds with silence (0.0)
	var silence_samples = int(silence_secs * sample_rate)
	for i in range(silence_samples):
		samples[i] = 0.0
		
	# Fill next 1 second with a sine wave of peak amplitude 0.1 (-20 dB, active)
	for i in range(silence_samples, total_samples):
		var t = (i - silence_samples) / sample_rate
		samples[i] = 0.1 * sin(2.0 * PI * 997.0 * t)
		
	var results = dsp.analyze_samples(samples, sample_rate)
	assert_false(results.is_empty(), "Analysis results should not be empty")
	
	var cue_in = results.get("cue_in", -1.0)
	var cue_out = results.get("cue_out", -1.0)
	
	assert_almost_eq(cue_in, 2.0, 0.02, "cue_in should be detected at exactly 2.0 seconds (within 10ms window tolerance)")
	assert_almost_eq(cue_out, 3.0, 0.02, "cue_out should be detected at exactly 3.0 seconds")

func test_lufs_loudness_calculation() -> void:
	assert_not_null(dsp, "AudioDSP should be instantiated")
	
	var sample_rate := 44100.0
	var duration := 1.0
	var total_samples = int(duration * sample_rate)
	
	var samples := PackedFloat32Array()
	samples.resize(total_samples)
	
	# Generate a 997 Hz sine wave at peak amplitude 1.0 (0 dBFS peak)
	for i in range(total_samples):
		var t = i / sample_rate
		samples[i] = 1.0 * sin(2.0 * PI * 997.0 * t)
		
	var results = dsp.analyze_samples(samples, sample_rate)
	assert_false(results.is_empty(), "Analysis results should not be empty")
	
	var lufs = results.get("lufs", -100.0)
	# Stereo full-scale sine wave would be -3.01 LUFS. Applied to one channel in mono,
	# EBU R128 loudness should equal exactly -3.01 LUFS within a 0.5dB tolerance.
	assert_almost_eq(lufs, -3.01, 0.5, "LUFS calculation for 0 dBFS peak sine wave should match reference target -3.01 LUFS within 0.5dB")

func test_gain_multiplier_calculation() -> void:
	var metadata_service = get_node_or_null("/root/MetadataService")
	var original_cache = {}
	if is_instance_valid(metadata_service):
		original_cache = metadata_service.cache.duplicate()
		metadata_service.cache = {}
		
		# Mock track metadata with -10.0 LUFS (needs -4.0 dB gain to match -14.0 LUFS)
		metadata_service.cache["test_track_loud.mp3"] = {
			"lufs": -10.0,
			"cue_in": 0.0,
			"cue_out": 30.0
		}
		# Mock track metadata with -18.0 LUFS (needs +4.0 dB gain to match -14.0 LUFS)
		metadata_service.cache["test_track_quiet.mp3"] = {
			"lufs": -18.0,
			"cue_in": 0.0,
			"cue_out": 30.0
		}
		# Mock track metadata with -14.0 LUFS (needs 0.0 dB gain)
		metadata_service.cache["test_track_perfect.mp3"] = {
			"lufs": -14.0,
			"cue_in": 0.0,
			"cue_out": 30.0
		}
		# Mock track metadata with 0.0 LUFS (extremely loud, needs -14.0 dB gain, should clamp to -12.0 dB)
		metadata_service.cache["test_track_extreme_loud.mp3"] = {
			"lufs": 0.0,
			"cue_in": 0.0,
			"cue_out": 30.0
		}
		# Mock track metadata with -30.0 LUFS (extremely quiet, needs +16.0 dB gain, should clamp to +6.0 dB)
		metadata_service.cache["test_track_extreme_quiet.mp3"] = {
			"lufs": -30.0,
			"cue_in": 0.0,
			"cue_out": 30.0
		}
		# Mock track metadata with missing LUFS key (should default to 0.0 dB gain)
		metadata_service.cache["test_track_no_lufs.mp3"] = {
			"cue_in": 0.0,
			"cue_out": 30.0
		}
	
	# Test Loud Track (-10 LUFS -> -14 target means -4.0 dB gain)
	manager._load_and_stream_remote_file("test_track_loud.mp3", manager.player_a, false)
	assert_almost_eq(manager.player_a.volume_db, -4.0, 0.01, "Loud track pre-fade gain should be attenuated to -4.0 dB")
	
	# Test Quiet Track (-18 LUFS -> -14 target means +4.0 dB gain)
	manager._load_and_stream_remote_file("test_track_quiet.mp3", manager.player_a, false)
	assert_almost_eq(manager.player_a.volume_db, 4.0, 0.01, "Quiet track pre-fade gain should be boosted to +4.0 dB")
	
	# Test Perfect Track (-14 LUFS -> -14 target means 0.0 dB gain)
	manager._load_and_stream_remote_file("test_track_perfect.mp3", manager.player_a, false)
	assert_almost_eq(manager.player_a.volume_db, 0.0, 0.01, "Perfect track pre-fade gain should be exactly 0.0 dB")
	
	# Test Extreme Loud Track (0.0 LUFS -> -14 target means -14.0 dB, clamped to -12.0 dB)
	manager._load_and_stream_remote_file("test_track_extreme_loud.mp3", manager.player_a, false)
	assert_almost_eq(manager.player_a.volume_db, -12.0, 0.01, "Extreme loud track pre-fade gain should be clamped to -12.0 dB")
	
	# Test Extreme Quiet Track (-30.0 LUFS -> -14 target means +16.0 dB, clamped to +6.0 dB)
	manager._load_and_stream_remote_file("test_track_extreme_quiet.mp3", manager.player_a, false)
	assert_almost_eq(manager.player_a.volume_db, 6.0, 0.01, "Extreme quiet track pre-fade gain should be clamped to +6.0 dB")
	
	# Test No LUFS Track (no cached LUFS value -> defaults to 0.0 dB gain)
	manager._load_and_stream_remote_file("test_track_no_lufs.mp3", manager.player_a, false)
	assert_almost_eq(manager.player_a.volume_db, 0.0, 0.01, "Track with missing LUFS should default to 0.0 dB gain")
	
	# Test No Metadata Track (not in metadata cache -> defaults to 0.0 dB gain)
	manager._load_and_stream_remote_file("test_track_no_metadata.mp3", manager.player_a, false)
	assert_almost_eq(manager.player_a.volume_db, 0.0, 0.01, "Track with no metadata cache entry should default to 0.0 dB gain")
	
	# Restore metadata cache
	if is_instance_valid(metadata_service):
		metadata_service.cache = original_cache


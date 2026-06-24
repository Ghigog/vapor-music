extends GutTest

func test_audio_dsp_node_registration() -> void:
	assert_true(ClassDB.class_exists("AudioDSP"), "AudioDSP class should be registered in Godot ClassDB")

func test_audio_dsp_node_instantiation() -> void:
	var dsp = AudioDSP.new()
	assert_not_null(dsp, "AudioDSP.new() should return a valid instance")
	assert_true(dsp is Node, "AudioDSP should inherit from Node")
	dsp.free()

func test_audio_dsp_method_presence() -> void:
	var dsp = AudioDSP.new()
	assert_true(dsp.has_method("get_library_version"), "AudioDSP should have get_library_version()")
	assert_true(dsp.has_method("analyze_file"), "AudioDSP should have analyze_file()")
	assert_true(dsp.has_method("stretch_buffer"), "AudioDSP should have stretch_buffer()")
	dsp.free()

func test_library_version_query() -> void:
	var dsp = AudioDSP.new()
	var version = dsp.get_library_version()
	assert_string_contains(version, "Essentia", "Version should mention Essentia")
	assert_string_contains(version, "Rubber Band", "Version should mention Rubber Band")
	dsp.free()

func test_analyze_file_and_free() -> void:
	var dsp = AudioDSP.new()
	var results = dsp.analyze_file("tests/unit/test_track.wav")
	assert_false(results.is_empty(), "Results should not be empty")
	dsp.free()

func test_streaming_low_memory_consumption() -> void:
	var dsp = AudioDSP.new()
	var path = ProjectSettings.globalize_path("res://tests/unit/test_track.wav")
	
	assert_true(dsp.has_method("get_cache_sample_count"), "AudioDSP should have get_cache_sample_count()")
	
	var success = dsp.load_file(path)
	assert_true(success, "Should successfully load test track")
	
	# Give background thread a moment to perform initial cache load
	var attempts = 0
	while dsp.get_cache_sample_count() == 0 and attempts < 15:
		await get_tree().create_timer(0.05).timeout
		attempts += 1
	
	var cache_samples = dsp.get_cache_sample_count()
	assert_gt(cache_samples, 0, "Cache should have samples loaded")
	assert_true(cache_samples <= 220500, "Cache size should be bounded by 5 seconds of audio")
	
	# Give background thread a moment to run and populate buffer
	await get_tree().create_timer(0.5).timeout
	
	# Verify that as we pull chunks, the cache stays bounded
	for i in range(10):
		var chunk = dsp.get_next_chunk(1024, 1.0)
		assert_gt(chunk.size(), 0, "Should retrieve non-empty chunk")
		var current_cache_samples = dsp.get_cache_sample_count()
		assert_true(current_cache_samples <= 220500, "Cache size should remain bounded during playback")
		
	# Verify seeking updates cache and keeps it bounded
	dsp.seek_pos(2.0)
	# Give background thread a moment to run and populate buffer after seek
	await get_tree().create_timer(0.1).timeout
	
	var post_seek_cache_samples = dsp.get_cache_sample_count()
	assert_true(post_seek_cache_samples <= 220500, "Cache size should remain bounded after seek")
	
	dsp.free()

func test_cross_correlation_offset() -> void:
	var dsp1 = AudioDSP.new()
	var dsp2 = AudioDSP.new()
	var path = ProjectSettings.globalize_path("res://tests/unit/test_track.wav")
	
	var s1 = dsp1.load_file(path)
	var s2 = dsp2.load_file(path)
	assert_true(s1 and s2, "Both tracks should load successfully")
	
	# Give background thread a moment to start and pre-cache
	await get_tree().create_timer(0.2).timeout
	
	# Case 1: Identical positions -> offset should be 0.0
	var offset_zero = dsp1.get_cross_correlation_offset(dsp2, 2.0, 2.0, 0.5)
	assert_almost_eq(offset_zero, 0.0, 0.005, "Offset of identical positions should be ~0")
	
	# Case 2: dsp2 is 50ms (0.05s) ahead.
	# dsp1 is at 2.0s, dsp2 is at 2.05s.
	# get_cross_correlation_offset should find that dsp2 matches dsp1 in the future by 0.05s
	dsp1.seek_pos(2.0)
	dsp2.seek_pos(2.05)
	await get_tree().create_timer(0.2).timeout
	
	var offset_ahead = dsp1.get_cross_correlation_offset(dsp2, 2.0, 2.05, 0.5)
	assert_almost_eq(offset_ahead, 0.05, 0.015, "Offset should accurately reflect the 50ms lead")
	
	# Case 3: dsp2 is 30ms (0.03s) behind.
	# dsp1 is at 2.0s, dsp2 is at 1.97s.
	# get_cross_correlation_offset should find that dsp2 matches dsp1 in the past by -0.03s
	dsp1.seek_pos(2.0)
	dsp2.seek_pos(1.97)
	await get_tree().create_timer(0.2).timeout
	
	var offset_behind = dsp1.get_cross_correlation_offset(dsp2, 2.0, 1.97, 0.5)
	assert_almost_eq(offset_behind, -0.03, 0.015, "Offset should accurately reflect the 30ms lag")
	
	dsp1.free()
	dsp2.free()

func _generate_test_ambient_drum_wav(path: String, ambient_duration: float, drum_duration: float) -> void:
	var file = FileAccess.open(path, FileAccess.WRITE)
	if not file:
		return
	
	var sample_rate: int = 44100
	var num_channels: int = 1
	var bits_per_sample: int = 16
	
	var total_duration = ambient_duration + drum_duration
	var total_samples: int = int(sample_rate * total_duration)
	var data_size: int = total_samples * num_channels * (bits_per_sample / 8)
	var file_size: int = 36 + data_size
	
	# WAV header
	file.store_string("RIFF")
	file.store_32(file_size)
	file.store_string("WAVE")
	file.store_string("fmt ")
	file.store_32(16) # Chunk size
	file.store_16(1) # Audio format (PCM)
	file.store_16(num_channels)
	file.store_32(sample_rate)
	file.store_32(sample_rate * num_channels * (bits_per_sample / 8)) # Byte rate
	file.store_16(num_channels * (bits_per_sample / 8)) # Block align
	file.store_16(bits_per_sample)
	file.store_string("data")
	file.store_32(data_size)
	
	# Generate samples
	# Segment 1: Ambient pad (low-frequency quiet hum)
	# Low-amplitude sine wave at 110Hz, amplitude 0.03
	var ambient_samples: int = int(sample_rate * ambient_duration)
	for i in range(ambient_samples):
		var t = float(i) / sample_rate
		var val = 0.03 * sin(2.0 * PI * 110.0 * t)
		var sample_val = int(val * 32767.0)
		file.store_16(sample_val)
	
	# Segment 2: Heavy kick drum intro (repeating kick sweeps + hi-hat noise)
	var drum_samples: int = int(sample_rate * drum_duration)
	var kick_interval_sec = 0.5 # 120 BPM
	var kick_interval_samples = int(sample_rate * kick_interval_sec)
	
	var hat_interval_sec = 0.25
	var hat_interval_samples = int(sample_rate * hat_interval_sec)
	
	for i in range(drum_samples):
		var t_in_segment = float(i) / sample_rate
		var kick_index = int(i / kick_interval_samples)
		var sample_in_kick = i % kick_interval_samples
		var t_in_kick = float(sample_in_kick) / sample_rate
		
		var val = 0.0
		# Each kick sweep lasts for 150 ms
		if t_in_kick < 0.150:
			# Sweeping frequency from 150Hz down to 40Hz
			var freq = 150.0 - (150.0 - 40.0) * (t_in_kick / 0.150)
			# Amplitude envelope fading out over the sweep
			var env = 0.8 * (1.0 - t_in_kick / 0.150)
			val = env * sin(2.0 * PI * freq * t_in_kick)
		
		# Hi-hat (short burst of high-frequency noise) every 0.25 seconds
		var sample_in_hat = i % hat_interval_samples
		var t_in_hat = float(sample_in_hat) / sample_rate
		if t_in_hat < 0.030:
			var hat_env = 0.2 * (1.0 - t_in_hat / 0.030)
			val += hat_env * randf_range(-1.0, 1.0)
		
		# Add a tiny bit of background synth pad to the drums too
		var t_total = ambient_duration + t_in_segment
		val += 0.03 * sin(2.0 * PI * 110.0 * t_total)
		
		var sample_val = int(clamp(val, -1.0, 1.0) * 32767.0)
		file.store_16(sample_val)
		
	file.close()

func test_self_healing_segment_boundaries() -> void:
	var dsp = AudioDSP.new()
	var test_path = "user://temp_ambient_intro.wav"
	var global_path = ProjectSettings.globalize_path(test_path)
	
	# Generate the test track: 15 seconds of quiet pad, 65 seconds of kick drums
	_generate_test_ambient_drum_wav(global_path, 15.0, 65.0)
	
	assert_true(FileAccess.file_exists(global_path), "Test WAV file should be generated")
	
	var results = dsp.analyze_file(global_path)
	assert_false(results.is_empty(), "Analysis results should not be empty")
	
	assert_true(results.has("segments"), "Results should contain segments")
	var segments = results["segments"]
	assert_true(segments.has("intro"), "Segments should contain intro")
	
	var intro_seg = segments["intro"]
	assert_eq(intro_seg.size(), 2, "Intro segment should have start and end time")
	
	var intro_end = intro_seg[1]
	# The heavy kick drum starts at 15.0 seconds.
	# We expect intro_end to be approximately 15.0 seconds, rather than 0.0 seconds.
	assert_almost_eq(intro_end, 15.0, 1.5, "intro_end should be detected close to 15.0s (where the kick drum starts)")
	
	# Clean up
	DirAccess.remove_absolute(global_path)
	dsp.free()

func test_get_waveform_peaks() -> void:
	var dsp = AudioDSP.new()
	var path = ProjectSettings.globalize_path("res://tests/unit/test_track.wav")
	
	var success = dsp.load_file(path)
	assert_true(success, "Should successfully load test track")
	
	# Give background thread a moment to perform initial cache load
	var attempts = 0
	while dsp.get_cache_sample_count() == 0 and attempts < 15:
		await get_tree().create_timer(0.05).timeout
		attempts += 1
		
	# Wait briefly for full caching of this short test track
	await get_tree().create_timer(0.2).timeout
	
	var peaks = dsp.get_waveform_peaks(1000)
	assert_eq(peaks.size(), 1000, "Should return exactly 1000 peak points")
	
	var all_between_zero_and_one = true
	var non_zero_found = false
	for val in peaks:
		if val < 0.0 or val > 1.0:
			all_between_zero_and_one = false
		if val > 0.0:
			non_zero_found = true
			
	assert_true(all_between_zero_and_one, "All peak values should be between 0.0 and 1.0")
	assert_true(non_zero_found, "Should have found some non-zero peaks for a valid track")
	
	dsp.free()


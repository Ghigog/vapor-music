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


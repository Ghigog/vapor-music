extends GutTest

const TEST_CACHE_PATH = "user://test_analyzer_metadata_cache.json"

var analyzer: Node

func before_each() -> void:
	# Temporary mock binding for testing
	if is_instance_valid(MetadataService):
		MetadataService.cache = {}
		MetadataService.set("CACHE_FILE_PATH", TEST_CACHE_PATH)
	
	analyzer = load("res://scripts/services/audio_analyzer.gd").new()
	add_child_autofree(analyzer)

func after_each() -> void:
	if FileAccess.file_exists(TEST_CACHE_PATH):
		DirAccess.remove_absolute(TEST_CACHE_PATH)
		
	# Clean up any test files using their MD5-hashed paths
	var wav_path = "user://audio_cache/" + "test_track.wav".md5_text() + ".wav"
	if FileAccess.file_exists(wav_path):
		DirAccess.remove_absolute(wav_path)
		
	var mp3_path = "user://audio_cache/" + "test_track.mp3".md5_text() + ".mp3"
	if FileAccess.file_exists(mp3_path):
		DirAccess.remove_absolute(mp3_path)

func test_analysis_defaults_for_missing_file() -> void:
	var results = analyzer._perform_analysis("nonexistent.mp3")
	assert_true(results.is_empty(), "Should return empty dictionary if file does not exist")

func test_analyze_mock_wav_file() -> void:
	var href = "test_track.wav"
	var temp_path = "user://audio_cache/" + href.md5_text() + ".wav"
	
	if not DirAccess.dir_exists_absolute("user://audio_cache/"):
		DirAccess.make_dir_recursive_absolute("user://audio_cache/")
		
	var file = FileAccess.open(temp_path, FileAccess.WRITE)
	if not file:
		fail_test("Could not write test WAV file")
		return
		
	# Write mock WAV header & small data chunk
	file.store_string("RIFF")
	file.store_32(40) # size
	file.store_string("WAVE")
	file.store_string("fmt ")
	file.store_32(16) # subchunk size
	file.store_16(1) # PCM format
	file.store_16(1) # mono channel
	file.store_32(44100) # sample rate
	file.store_32(88200) # byte rate
	file.store_16(2) # block align
	file.store_16(16) # bits per sample
	file.store_string("data")
	file.store_32(2048) # data size
	
	# Write 1024 mock silent sample bytes (mono, 16-bit)
	for i in range(1024):
		file.store_16(0)
		
	file.close()
	
	# Analyze enqueued track mock
	var results = analyzer._perform_analysis(href)
	assert_false(results.is_empty(), "Results should not be empty")
	assert_eq(results.bpm, 120.0, "WAV BPM should default to 120 on silent loops")
	assert_eq(results.musical_key, "8A", "WAV key should default to 8A")
	assert_eq(results.energy_graph.size(), 20, "Should generate a 20-point energy graph")

func test_analyze_mock_mp3_file() -> void:
	var href = "test_track.mp3"
	var temp_path = "user://audio_cache/" + href.md5_text() + ".mp3"
	
	if not DirAccess.dir_exists_absolute("user://audio_cache/"):
		DirAccess.make_dir_recursive_absolute("user://audio_cache/")
		
	var file = FileAccess.open(temp_path, FileAccess.WRITE)
	if not file:
		fail_test("Could not write test MP3 file")
		return
		
	# Write basic ID3 header
	file.store_string("ID3")
	file.store_16(0x0300) # v2.3
	file.store_8(0) # flags
	file.store_32(0) # size (synchsafe)
	
	# Write some dummy bytes for payload hashing
	for i in range(100):
		file.store_8(i % 256)
		
	file.close()
	
	# Mock MetadataService cache entry
	MetadataService.cache[href] = {
		"artist_name": "Test Artist",
		"album_name": "Test Album",
		"track_title": "Test Track",
		"bpm": 0.0,
		"musical_key": "",
		"genre": "Rock"
	}
	
	var results = analyzer._perform_analysis(href)
	assert_false(results.is_empty(), "Results should not be empty")
	assert_true(results.bpm > 0.0, "Should generate a positive BPM")
	assert_true(results.musical_key != "", "Should assign a musical key")
	assert_eq(results.energy_graph.size(), 20, "Should generate a 20-point energy graph")

func test_thread_worker_signals_and_cache_update() -> void:
	var href = "test_track.mp3"
	var temp_path = "user://audio_cache/" + href.md5_text() + ".mp3"
	
	# Setup mock cache
	MetadataService.cache[href] = {
		"artist_name": "Artist",
		"album_name": "Album",
		"track_title": "Track",
		"bpm": 0.0,
		"musical_key": ""
	}
	
	# Create dummy MP3 file for thread to process
	var file = FileAccess.open(temp_path, FileAccess.WRITE)
	file.store_string("ID3")
	file.store_16(0x0300)
	file.store_8(0)
	file.store_32(0)
	for i in range(100):
		file.store_8(i % 256)
	file.close()
	
	watch_signals(analyzer)
	
	# Enqueue track and wait for thread to finish processing
	analyzer.analyze_track(href)
	
	var start_time = Time.get_ticks_msec()
	while analyzer._queue.size() > 0 or get_signal_emit_count(analyzer, "analysis_completed") == 0:
		await get_tree().process_frame
		if Time.get_ticks_msec() - start_time > 2000:
			fail_test("Thread processing timed out")
			return
			
	assert_signal_emitted(analyzer, "analysis_completed")
	
	var cached_meta = MetadataService.get_cached_metadata(href)
	assert_true(cached_meta.get("bpm", 0.0) > 0.0, "BPM should be cached in database")
	assert_true(cached_meta.get("musical_key", "") != "", "Key should be cached in database")
	assert_eq(cached_meta.get("energy_graph", []).size(), 20, "Energy graph should be cached in database")

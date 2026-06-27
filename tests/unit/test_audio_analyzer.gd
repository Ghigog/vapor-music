extends GutTest

const TEST_CACHE_PATH = "user://test_analyzer_metadata_cache.json"
const TEST_CACHE_DIR = "user://test_audio_cache/"

var analyzer: Node
var _original_cache: Dictionary = {}
var _original_path: String = ""

func before_each() -> void:
	# Temporary mock binding for testing
	if is_instance_valid(MetadataService):
		_original_cache = MetadataService.cache.duplicate()
		_original_path = MetadataService.cache_file_path
		MetadataService.cache = {}
		MetadataService.cache_file_path = TEST_CACHE_PATH
		
	if not DirAccess.dir_exists_absolute(TEST_CACHE_DIR):
		DirAccess.make_dir_recursive_absolute(TEST_CACHE_DIR)
	
	analyzer = load("res://scripts/services/audio_analyzer.gd").new()
	analyzer.cache_dir = TEST_CACHE_DIR
	add_child(analyzer)

func after_each() -> void:
	if is_instance_valid(analyzer):
		analyzer.queue_free()
		await get_tree().process_frame
		
	if FileAccess.file_exists(TEST_CACHE_PATH):
		DirAccess.remove_absolute(TEST_CACHE_PATH)
		
	if is_instance_valid(MetadataService):
		MetadataService.cache = _original_cache
		MetadataService.cache_file_path = _original_path
		
	# Clean up test cache directory and files
	var dir = DirAccess.open(TEST_CACHE_DIR)
	if dir:
		dir.list_dir_begin()
		var file_name = dir.get_next()
		while file_name != "":
			if not dir.current_is_dir():
				dir.remove(file_name)
			file_name = dir.get_next()
		dir.list_dir_end()
		DirAccess.remove_absolute(TEST_CACHE_DIR)

func test_analysis_defaults_for_missing_file() -> void:
	var results = analyzer._perform_analysis("nonexistent.mp3")
	assert_true(results.is_empty(), "Should return empty dictionary if file does not exist")

func test_analyze_real_wav_file() -> void:
	var href = "test_track.wav"
	var cache_path = TEST_CACHE_DIR + href.md5_text() + ".wav"
	
	var err = DirAccess.copy_absolute("res://tests/unit/test_track.wav", cache_path)
	assert_eq(err, OK, "Should copy test WAV file to cache")
	
	# Analyze enqueued track
	var results = analyzer._perform_analysis(href)
	assert_false(results.is_empty(), "Results should not be empty")
	
	print("Real WAV Analysis Results: ", results)
	
	assert_true(results.get("bpm", 0.0) > 0.0, "BPM should be estimated")
	assert_true(results.get("musical_key", "") != "", "Key should be resolved")
	assert_eq(results.get("energy_graph", []).size(), 20, "Should generate a 20-point energy graph")
	assert_true(results.get("beat_grid", []).size() > 0, "Should extract a beat grid")
	assert_true(results.get("downbeats", []).size() > 0, "Should extract downbeats")
	assert_true(results.get("segments", {}).has("intro"), "Should have intro segment")
	assert_true(results.get("segments", {}).has("outro"), "Should have outro segment")
	assert_true(results.get("segments", {}).has("drop"), "Should have drop segment")
	assert_almost_eq(results.get("duration", 0.0), 5.0, 0.2, "Duration should be approximately 5.0 seconds")

func test_analyze_real_mp3_file() -> void:
	var href = "test_track.mp3"
	var cache_path = TEST_CACHE_DIR + href.md5_text() + ".mp3"
	
	var err = DirAccess.copy_absolute("res://tests/unit/test_track.mp3", cache_path)
	assert_eq(err, OK, "Should copy test MP3 file to cache")
	
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
	
	print("Real MP3 Analysis Results: ", results)
	
	assert_true(results.get("bpm", 0.0) > 0.0, "BPM should be estimated")
	assert_true(results.get("musical_key", "") != "", "Key should be resolved")
	assert_eq(results.get("energy_graph", []).size(), 20, "Should generate a 20-point energy graph")
	assert_true(results.get("beat_grid", []).size() > 0, "Should extract a beat grid")
	assert_almost_eq(results.get("duration", 0.0), 5.0, 0.2, "Duration should be approximately 5.0 seconds")

func test_thread_worker_signals_and_cache_update() -> void:
	var href = "test_track.mp3"
	var temp_path = TEST_CACHE_DIR + href.md5_text() + ".mp3"
	
	var err = DirAccess.copy_absolute("res://tests/unit/test_track.mp3", temp_path)
	assert_eq(err, OK, "Should copy test MP3 file to cache")
	
	# Setup mock cache
	MetadataService.cache[href] = {
		"artist_name": "Artist",
		"album_name": "Album",
		"track_title": "Track",
		"bpm": 0.0,
		"musical_key": ""
	}
	
	watch_signals(analyzer)
	
	# Enqueue track and wait for thread to finish processing
	analyzer.analyze_track(href)
	
	var start_time = Time.get_ticks_msec()
	while analyzer._queue.size() > 0 or get_signal_emit_count(analyzer, "analysis_completed") == 0:
		await get_tree().process_frame
		if Time.get_ticks_msec() - start_time > 8000:
			fail_test("Thread processing timed out")
			return
			
	assert_signal_emitted(analyzer, "analysis_completed")
	
	var cached_meta = MetadataService.get_cached_metadata(href)
	assert_true(cached_meta.get("bpm", 0.0) > 0.0, "BPM should be cached in database")
	assert_true(cached_meta.get("musical_key", "") != "", "Key should be cached in database")
	assert_eq(cached_meta.get("energy_graph", []).size(), 20, "Energy graph should be cached in database")

func test_cache_pruning() -> void:
	var valid_href = "valid_track.mp3"
	var invalid_href = "invalid_track.mp3"
	
	if not DirAccess.dir_exists_absolute(TEST_CACHE_DIR):
		DirAccess.make_dir_recursive_absolute(TEST_CACHE_DIR)
		
	var valid_path = TEST_CACHE_DIR + valid_href.md5_text() + ".mp3"
	var invalid_path = TEST_CACHE_DIR + invalid_href.md5_text() + ".mp3"
	
	# Write mock files
	var f1 = FileAccess.open(valid_path, FileAccess.WRITE)
	f1.store_string("dummy")
	f1.close()
	
	var f2 = FileAccess.open(invalid_path, FileAccess.WRITE)
	f2.store_string("dummy")
	f2.close()
	
	assert_true(FileAccess.file_exists(valid_path))
	assert_true(FileAccess.file_exists(invalid_path))
	
	# Run pruning only keeping valid_href
	analyzer.prune_orphaned_cache_files([valid_href])
	
	assert_true(FileAccess.file_exists(valid_path), "Valid cached file should remain")
	assert_false(FileAccess.file_exists(invalid_path), "Orphaned cached file should be pruned")
	
	# Clean up
	DirAccess.remove_absolute(valid_path)

func test_prefetch_state_toggles() -> void:
	var hrefs = ["track1.mp3", "track2.mp3"]
	
	assert_false(analyzer.background_caching_active, "Should not be active initially")
	
	analyzer.start_prefetching(hrefs)
	assert_true(analyzer.background_caching_active, "Should set active to true")
	
	analyzer.stop_prefetching()
	assert_false(analyzer.background_caching_active, "Should set active to false after stop")

func test_prefetch_lookahead_update_and_cancellation() -> void:
	var hrefs1 = ["track1.mp3", "track2.mp3"]
	var hrefs2 = ["track2.mp3", "track3.mp3"]
	
	# Start prefetching first set
	analyzer.start_prefetching(hrefs1)
	assert_true(analyzer.background_caching_active, "Should be active")
	assert_eq(analyzer.current_download_href, "track1.mp3", "Should be downloading track1.mp3")
	assert_eq(analyzer._prefetch_queue, ["track2.mp3"], "Queue should contain track2.mp3")
	
	# Update prefetching with new set that doesn't contain track1.mp3 (cancelled)
	analyzer.start_prefetching(hrefs2)
	# Since track1.mp3 is not in hrefs2, it should be cancelled!
	# Now, track2.mp3 and track3.mp3 are uncached.
	# track2.mp3 is popped as the next download.
	assert_eq(analyzer.current_download_href, "track2.mp3", "Should start downloading track2.mp3 after cancelling track1.mp3")
	assert_eq(analyzer._prefetch_queue, ["track3.mp3"], "Queue should contain track3.mp3")
	
	# Stop prefetching to clean up
	analyzer.stop_prefetching()

func test_prefetch_lookahead_update_keeps_active() -> void:
	var hrefs1 = ["track1.mp3", "track2.mp3"]
	var hrefs2 = ["track1.mp3", "track3.mp3"]
	
	# Start prefetching first set
	analyzer.start_prefetching(hrefs1)
	assert_true(analyzer.background_caching_active, "Should be active")
	assert_eq(analyzer.current_download_href, "track1.mp3", "Should be downloading track1.mp3")
	assert_eq(analyzer._prefetch_queue, ["track2.mp3"], "Queue should contain track2.mp3")
	
	# Update prefetching with new set that still contains track1.mp3 (kept)
	analyzer.start_prefetching(hrefs2)
	assert_eq(analyzer.current_download_href, "track1.mp3", "Should keep downloading track1.mp3")
	assert_eq(analyzer._prefetch_queue, ["track3.mp3"], "Queue should now contain track3.mp3 instead of track2.mp3")
	
	# Stop prefetching to clean up
	analyzer.stop_prefetching()

func test_prefetch_does_not_prune_cache() -> void:
	var valid_href = "track1.mp3"
	var other_href = "track2.mp3"
	
	var valid_path = analyzer.cache_dir + valid_href.md5_text() + ".mp3"
	var other_path = analyzer.cache_dir + other_href.md5_text() + ".mp3"
	
	var f1 = FileAccess.open(valid_path, FileAccess.WRITE)
	f1.store_string("dummy")
	f1.close()
	
	var f2 = FileAccess.open(other_path, FileAccess.WRITE)
	f2.store_string("dummy")
	f2.close()
	
	assert_true(FileAccess.file_exists(valid_path))
	assert_true(FileAccess.file_exists(other_path))
	
	# Start prefetching only valid_href. If prefetching pruned, it would delete other_href.
	analyzer.start_prefetching([valid_href])
	
	assert_true(FileAccess.file_exists(valid_path), "Prefetched track should remain cached")
	assert_true(FileAccess.file_exists(other_path), "Other track should NOT be pruned during prefetching")
	
	# Clean up
	DirAccess.remove_absolute(valid_path)
	DirAccess.remove_absolute(other_path)
	analyzer.stop_prefetching()

func test_prefetch_priority_override() -> void:
	var original_override = AudioManager.upcoming_track_override
	var original_playlist = AudioManager.current_playlist
	var original_index = AudioManager.current_track_index
	
	AudioManager.current_playlist = ["track1.mp3", "track2.mp3", "track3.mp3"]
	AudioManager.current_track_index = 0
	AudioManager.upcoming_track_override = ""
	
	var hrefs = ["track1.mp3", "track2.mp3", "track3.mp3"]
	
	# Start prefetching. It starts downloading track2.mp3 (the next track in the playlist).
	analyzer.start_prefetching(hrefs)
	assert_eq(analyzer.current_download_href, "track2.mp3", "Should start downloading track2.mp3")
	
	# Now, if we trigger prefetching again, but the next track (track3.mp3) is not cached,
	# it should immediately cancel track2.mp3 and prioritize track3.mp3!
	AudioManager.upcoming_track_override = "track3.mp3"
	analyzer.start_prefetching(hrefs)
	
	assert_eq(analyzer.current_download_href, "track3.mp3", "Should cancel current download and prioritize track3.mp3")
	assert_eq(analyzer._prefetch_queue, ["track1.mp3", "track2.mp3"], "Queue should contain the remaining tracks")
	
	# Clean up
	analyzer.stop_prefetching()
	AudioManager.upcoming_track_override = original_override
	AudioManager.current_playlist = original_playlist
	AudioManager.current_track_index = original_index

func test_automatic_prefetch_on_library_scan_with_uncached() -> void:
	var hrefs = ["uncached_track1.mp3", "uncached_track2.mp3"]
	
	# Verify prefetching is inactive initially
	assert_false(analyzer.background_caching_active, "Prefetching should not be active initially")
	
	# Trigger library scanned. Since these tracks are uncached, it should automatically start prefetching
	analyzer._on_library_scanned(hrefs)
	
	assert_true(analyzer.background_caching_active, "Prefetching should automatically start for uncached tracks")
	
	# Clean up
	analyzer.stop_prefetching()

func test_no_automatic_prefetch_when_all_cached() -> void:
	var href = "cached_track.mp3"
	var cache_path = analyzer.cache_dir + href.md5_text() + ".mp3"
	
	# Create a dummy cached file
	var file = FileAccess.open(cache_path, FileAccess.WRITE)
	file.store_string("dummy content")
	file.close()
	
	assert_true(FileAccess.file_exists(cache_path), "Dummy cached track should exist")
	assert_false(analyzer.background_caching_active, "Prefetching should not be active initially")
	
	# Trigger library scanned. Since all tracks are cached, it should NOT start prefetching
	analyzer._on_library_scanned([href])
	
	assert_false(analyzer.background_caching_active, "Prefetching should not start when all tracks are already cached")
	
	# Clean up
	DirAccess.remove_absolute(cache_path)




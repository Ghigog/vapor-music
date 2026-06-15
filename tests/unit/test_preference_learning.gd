extends GutTest

const DJPathfinder = preload("res://scripts/services/dj_pathfinder.gd")

var _test_history_path := "user://transition_history.json"

func before_each() -> void:
	# Clean up any existing history file
	if FileAccess.file_exists(_test_history_path):
		DirAccess.remove_absolute(ProjectSettings.globalize_path(_test_history_path))
	DJPathfinder._history_cache = []
	DJPathfinder._history_loaded = false

func after_each() -> void:
	if FileAccess.file_exists(_test_history_path):
		DirAccess.remove_absolute(ProjectSettings.globalize_path(_test_history_path))
	DJPathfinder._history_cache = []
	DJPathfinder._history_loaded = false

func _write_test_history(entries: Array) -> void:
	var file = FileAccess.open(_test_history_path, FileAccess.WRITE)
	file.store_string(JSON.stringify(entries, "\t"))
	file.close()

func test_skip_history_logging() -> void:
	# Simulate a transition by populating AudioManager's state
	var am = AudioManager
	am._transition_history_file = _test_history_path
	am._last_transition_info = {
		"track_a": "/music/song_a.mp3",
		"track_b": "/music/song_b.mp3",
		"transition_type": "Echo Out",
		"finished": false,
		"end_time_msec": -1,
		"logged": false
	}
	am.is_transitioning = true
	
	# Trigger a skip check (simulating the user pressing next during transition)
	am._check_and_log_skip()
	
	# Verify the skip was logged
	assert_true(FileAccess.file_exists(_test_history_path), "Transition history file should exist")
	
	var file = FileAccess.open(_test_history_path, FileAccess.READ)
	var content = file.get_as_text()
	file.close()
	var json = JSON.new()
	assert_eq(json.parse(content), OK, "JSON should parse successfully")
	
	var history = json.data
	assert_true(history is Array, "History should be an array")
	assert_eq(history.size(), 1, "Should have exactly 1 history entry")
	
	var entry = history[0]
	assert_eq(entry["track_a"], "/music/song_a.mp3", "Track A should match")
	assert_eq(entry["track_b"], "/music/song_b.mp3", "Track B should match")
	assert_eq(entry["transition_type"], "Echo Out", "Transition type should match")
	assert_eq(entry["outcome"], "skipped", "Outcome should be skipped")
	
	# Cleanup state
	am.is_transitioning = false
	am._last_transition_info = {}

func test_skip_not_logged_after_10_seconds() -> void:
	var am = AudioManager
	am._transition_history_file = _test_history_path
	am._last_transition_info = {
		"track_a": "/music/song_a.mp3",
		"track_b": "/music/song_b.mp3",
		"transition_type": "Bass Swap",
		"finished": true,
		"end_time_msec": Time.get_ticks_msec() - 15000,  # 15 seconds ago
		"logged": false
	}
	am.is_transitioning = false
	
	# Trigger a skip check -- should NOT log since 15s > 10s window
	am._check_and_log_skip()
	
	assert_false(FileAccess.file_exists(_test_history_path), "No history file should be created when skip is outside 10s window")
	
	# Cleanup state
	am._last_transition_info = {}

func test_skip_logged_within_10_seconds() -> void:
	var am = AudioManager
	am._transition_history_file = _test_history_path
	am._last_transition_info = {
		"track_a": "/music/song_c.mp3",
		"track_b": "/music/song_d.mp3",
		"transition_type": "Filter Sweep",
		"finished": true,
		"end_time_msec": Time.get_ticks_msec() - 5000,  # 5 seconds ago
		"logged": false
	}
	am.is_transitioning = false
	
	# Trigger a skip check -- should log since 5s < 10s
	am._check_and_log_skip()
	
	assert_true(FileAccess.file_exists(_test_history_path), "History file should exist")
	
	var file = FileAccess.open(_test_history_path, FileAccess.READ)
	var content = file.get_as_text()
	file.close()
	var json = JSON.new()
	json.parse(content)
	var history = json.data
	assert_eq(history.size(), 1, "Should have 1 entry")
	assert_eq(history[0]["outcome"], "skipped", "Outcome should be skipped")
	
	# Cleanup state
	am._last_transition_info = {}

func test_preference_cost_penalty() -> void:
	# Write a skip entry to history
	var skip_entries = [
		{
			"track_a": "/music/track_x.mp3",
			"track_b": "/music/track_y.mp3",
			"transition_type": "Echo Out",
			"outcome": "skipped",
			"timestamp": 1000000
		}
	]
	_write_test_history(skip_entries)
	
	# Force reload so pathfinder picks up the test file
	DJPathfinder.reload_history_cache()
	
	# Calculate costs with and without href keys
	var track_a = {"bpm": 120.0, "musical_key": "8A", "energy_level": 0.5, "genre": "House", "href": "/music/track_x.mp3"}
	var track_b = {"bpm": 120.0, "musical_key": "8A", "energy_level": 0.5, "genre": "House", "href": "/music/track_y.mp3"}
	var track_c = {"bpm": 120.0, "musical_key": "8A", "energy_level": 0.5, "genre": "House", "href": "/music/track_z.mp3"}
	
	var cost_skipped = DJPathfinder.calculate_transition_cost(track_a, track_b)
	var cost_normal = DJPathfinder.calculate_transition_cost(track_a, track_c)
	
	assert_true(cost_skipped > cost_normal, "Skipped transition should have higher cost than a non-skipped one")
	assert_true(cost_skipped - cost_normal >= 15.0, "The skip penalty should be at least 15.0")

func test_multiple_skips_stack_penalty() -> void:
	var skip_entries = [
		{
			"track_a": "/music/track_x.mp3",
			"track_b": "/music/track_y.mp3",
			"transition_type": "Echo Out",
			"outcome": "skipped",
			"timestamp": 1000000
		},
		{
			"track_a": "/music/track_x.mp3",
			"track_b": "/music/track_y.mp3",
			"transition_type": "Bass Swap",
			"outcome": "skipped",
			"timestamp": 1000100
		}
	]
	_write_test_history(skip_entries)
	
	DJPathfinder.reload_history_cache()
	
	var penalty = DJPathfinder.get_skip_penalty("/music/track_x.mp3", "/music/track_y.mp3")
	assert_eq(penalty, 30.0, "Two skips should result in a penalty of 30.0")

func test_completed_transitions_no_penalty() -> void:
	var entries = [
		{
			"track_a": "/music/track_x.mp3",
			"track_b": "/music/track_y.mp3",
			"transition_type": "Echo Out",
			"outcome": "completed",
			"timestamp": 1000000
		}
	]
	_write_test_history(entries)
	
	DJPathfinder.reload_history_cache()
	
	var penalty = DJPathfinder.get_skip_penalty("/music/track_x.mp3", "/music/track_y.mp3")
	assert_eq(penalty, 0.0, "Completed transitions should have no penalty")

## test_audio_manager.gd
## GUT unit tests for AudioManager debounce skip controls.
extends GutTest

func before_each() -> void:
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3", "song3.mp3"]
	AudioManager.current_track_index = 0
	if AudioManager._debounce_timer:
		AudioManager._debounce_timer.stop()

func after_each() -> void:
	if AudioManager._debounce_timer:
		AudioManager._debounce_timer.stop()

func test_debounce_timer_starts_on_play_next() -> void:
	var signal_watcher = watch_signals(AudioManager)
	
	AudioManager.play_next()
	
	assert_eq(AudioManager.current_track_index, 1, "Track index should increment")
	assert_true(AudioManager._debounce_timer.time_left > 0, "Debounce timer should be active and running")
	assert_eq(AudioManager._debounce_timer.wait_time, 3.0, "Debounce timer should wait 3 seconds")
	assert_signal_emitted_with_parameters(AudioManager, "track_changed", ["song2.mp3"], 0)

func test_debounce_timer_starts_on_play_previous() -> void:
	var signal_watcher = watch_signals(AudioManager)
	
	AudioManager.play_previous()
	
	assert_eq(AudioManager.current_track_index, 2, "Track index should loop to the end")
	assert_true(AudioManager._debounce_timer.time_left > 0, "Debounce timer should be active and running")
	assert_signal_emitted_with_parameters(AudioManager, "track_changed", ["song3.mp3"], 0)

func test_play_track_cancels_debounce_timer() -> void:
	AudioManager.play_next()
	assert_true(AudioManager._debounce_timer.time_left > 0, "Debounce timer should be running after play_next")
	
	# Stub _load_and_stream_remote_file to avoid real network requests in unit test
	var original_load = AudioManager._load_and_stream_remote_file
	# We can't easily stub/mock functions in GDScript without class_db or overriding the method.
	# But we can check if play_track stops the timer.
	AudioManager.play_track("song1.mp3", ["song1.mp3", "song2.mp3"])
	assert_true(AudioManager._debounce_timer.is_stopped(), "Debounce timer should be stopped when play_track is called directly")

func test_smart_mixing_toggled_signal() -> void:
	watch_signals(AudioManager)
	AudioManager.smart_mixing_enabled = true
	assert_signal_emitted(AudioManager, "smart_mixing_toggled")
	assert_true(AudioManager.smart_mixing_enabled)
	AudioManager.smart_mixing_enabled = false
	assert_signal_emitted_with_parameters(AudioManager, "smart_mixing_toggled", [false])

func test_smart_mixing_sequence_path() -> void:
	AudioManager.smart_mixing_step_index = 0
	AudioManager.update_preferred_type()
	assert_eq(AudioManager.preferred_type_for_current_track, "perfect")
	
	AudioManager.smart_mixing_step_index = 1
	AudioManager.update_preferred_type()
	assert_eq(AudioManager.preferred_type_for_current_track, "interesting")
	
	AudioManager.smart_mixing_step_index = 2
	AudioManager.update_preferred_type()
	assert_eq(AudioManager.preferred_type_for_current_track, "perfect")
	
	AudioManager.smart_mixing_step_index = 3
	AudioManager.update_preferred_type()
	var choice = AudioManager.preferred_type_for_current_track
	assert_true(choice == "creative" or choice == "interesting")


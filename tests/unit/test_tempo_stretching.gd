## test_tempo_stretching.gd
## GUT unit tests for AudioManager time-stretching and pitch-locked tempo morphing.
extends GutTest

func before_each() -> void:
	AudioManager._block_finished_signal = true
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3", "song3.mp3"]
	AudioManager.current_track_index = 0
	AudioManager.is_transitioning = false
	if AudioManager.player_a:
		AudioManager.player_a.stop()
		AudioManager.player_a.pitch_scale = 1.0
	if AudioManager.player_b:
		AudioManager.player_b.stop()
		AudioManager.player_b.pitch_scale = 1.0
	if AudioManager.dsp_a:
		AudioManager.dsp_a.clear_stream()
	if AudioManager.dsp_b:
		AudioManager.dsp_b.clear_stream()
	AudioManager._speed_scale_a = 1.0
	AudioManager._speed_scale_b = 1.0
	AudioManager.active_player = AudioManager.player_a
	AudioManager.player = AudioManager.active_player
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	if AudioManager._pitch_ramp_tween and AudioManager._pitch_ramp_tween.is_valid():
		AudioManager._pitch_ramp_tween.kill()
	AudioManager.is_playing = true
	AudioManager.transition_duration = 0.1

func after_each() -> void:
	AudioManager._block_finished_signal = false
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	if AudioManager._pitch_ramp_tween and AudioManager._pitch_ramp_tween.is_valid():
		AudioManager._pitch_ramp_tween.kill()
	# Clean up cache
	MetadataService.cache.erase("song1.mp3")
	MetadataService.cache.erase("song2.mp3")

func test_tempo_morph_keeps_pitch_scale_at_1() -> void:
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	
	# Add mock metadata to cache for BPM lookup
	var meta_a = {"bpm": 128.0, "genre": "House"}
	var meta_b = {"bpm": 120.0, "genre": "House"}
	MetadataService.cache["song1.mp3"] = meta_a
	MetadataService.cache["song2.mp3"] = meta_b
	
	# Set current playlist index to 1 so outgoing is playlist[0], incoming is playlist[1]
	AudioManager.current_track_index = 1
	
	# Start a Tempo Morph transition
	AudioManager._run_deck_transition(deck_a, deck_b, "Tempo Morph")
	assert_not_null(AudioManager.active_tween, "Tween should be created")
	AudioManager.active_tween.pause()
	
	# Verify initial state of pitch scale (locked at 1.0)
	assert_eq(deck_a.pitch_scale, 1.0, "Outgoing deck pitch scale should start at 1.0")
	assert_eq(deck_b.pitch_scale, 1.0, "Incoming deck pitch scale should start at 1.0")
	
	# Step 0.03s for speed scale tween to progress (duration is 0.1s, ramp time is 0.025s)
	AudioManager.active_tween.custom_step(0.03)
	
	# Outgoing BPM: 128 -> Target 124 -> ratio is 124 / 128 = 0.96875
	# Incoming BPM: 120 -> Target 124 -> ratio is 124 / 120 = 1.0333
	assert_lt(AudioManager._speed_scale_a, 0.99, "Outgoing deck speed scale should scale down")
	assert_gt(AudioManager._speed_scale_b, 1.01, "Incoming deck speed scale should scale up")
	assert_eq(deck_a.pitch_scale, 1.0, "Outgoing deck pitch scale should remain locked at 1.0 during morph")
	assert_eq(deck_b.pitch_scale, 1.0, "Incoming deck pitch scale should remain locked at 1.0 during morph")
	
	# Step to finish transition
	AudioManager.active_tween.custom_step(0.1)
	assert_eq(deck_a.pitch_scale, 1.0, "Outgoing deck pitch scale should remain locked at 1.0 after morph")
	assert_eq(deck_b.pitch_scale, 1.0, "Incoming deck pitch scale should remain locked at 1.0 after morph")

func test_post_transition_speed_glide() -> void:
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	
	# Add mock metadata to cache for BPM lookup
	var meta_a = {"bpm": 128.0, "genre": "House"}
	var meta_b = {"bpm": 120.0, "genre": "House"}
	MetadataService.cache["song1.mp3"] = meta_a
	MetadataService.cache["song2.mp3"] = meta_b
	
	AudioManager.current_track_index = 1
	
	# Start a Tempo Morph transition
	AudioManager._run_deck_transition(deck_a, deck_b, "Tempo Morph")
	assert_not_null(AudioManager.active_tween, "Tween should be created")
	AudioManager.active_tween.pause()
	
	# Step to finish the transition immediately (0.13s)
	AudioManager.active_tween.custom_step(0.13)
	
	assert_eq(AudioManager.active_player, deck_b, "Deck B should now be the active player")
	assert_not_null(AudioManager._pitch_ramp_tween, "Post-transition pitch ramp tween should be created")
	AudioManager._pitch_ramp_tween.pause()
	
	# Morph speed of Deck B was target_bpm / bpm_in = 124 / 120 = 1.03333
	assert_almost_eq(AudioManager._speed_scale_b, 1.03333, 0.001, "Deck B speed scale should start glide at morphed ratio")
	
	# Step post-transition pitch ramp tween by 0.05s (half of the 0.1s test glide duration)
	AudioManager._pitch_ramp_tween.custom_step(0.05)
	assert_gt(AudioManager._speed_scale_b, 1.0, "Deck B speed scale should still be above 1.0 during glide")
	assert_lt(AudioManager._speed_scale_b, 1.033, "Deck B speed scale should be gliding down toward 1.0")
	assert_eq(deck_b.pitch_scale, 1.0, "Deck B pitch scale should remain locked at 1.0 during glide")
	
	# Step post-transition pitch ramp tween by another 0.05s (completes the glide)
	AudioManager._pitch_ramp_tween.custom_step(0.05)
	assert_eq(AudioManager._speed_scale_b, 1.0, "Deck B speed scale should be restored to 1.0 after glide")
	assert_eq(deck_b.pitch_scale, 1.0, "Deck B pitch scale should remain locked at 1.0 after glide completed")

func test_set_speed_scale_does_not_change_pitch_scale() -> void:
	# Verify that manually changing speed scale variables does not touch player pitch_scale properties
	AudioManager._speed_scale_a = 1.15
	AudioManager._speed_scale_b = 0.85
	
	assert_eq(AudioManager.player_a.pitch_scale, 1.0, "Player A pitch scale should remain 1.0 when speed scale is changed")
	assert_eq(AudioManager.player_b.pitch_scale, 1.0, "Player B pitch scale should remain 1.0 when speed scale is changed")

func test_threaded_time_stretching_non_blocking() -> void:
	var dsp = AudioDSP.new()
	var path = ProjectSettings.globalize_path("res://tests/unit/test_track.wav")
	var success = dsp.load_file(path)
	assert_true(success, "Should successfully load test track wav")
	
	# Verify initial state
	assert_almost_eq(dsp.get_playback_position(), 0.0, 0.01, "Playback position should start at 0.0")
	assert_false(dsp.is_finished(), "Should not be finished at start")
	
	# Give the background thread a moment to run and populate the buffer.
	var attempts = 0
	while dsp.get_cache_sample_count() == 0 and attempts < 15:
		await get_tree().create_timer(0.05).timeout
		attempts += 1
	
	# Get a chunk at normal speed (ratio = 1.0)
	var chunk = dsp.get_next_chunk(1024, 1.0)
	assert_gt(chunk.size(), 0, "Should retrieve non-empty chunk from the buffer")
	assert_almost_eq(dsp.get_playback_position(), 1024.0 / 44100.0, 0.01, "Playback position should update correctly")
	
	# Seek to a position
	dsp.seek_pos(2.0)
	assert_almost_eq(dsp.get_playback_position(), 2.0, 0.01, "Playback position should update to seek position")
	
	# Wait for background thread to fill buffer after seek
	await get_tree().create_timer(0.05).timeout
	
	var chunk_after_seek = dsp.get_next_chunk(512, 1.2)
	assert_gt(chunk_after_seek.size(), 0, "Should retrieve chunk after seeking")
	
	# Expected position = 2.0 + (512.0 / 44100.0) / 1.2
	assert_almost_eq(dsp.get_playback_position(), 2.0 + (512.0 / 44100.0) / 1.2, 0.01, "Playback position should update correctly after speed ratio chunk pull")
	
	dsp.clear_stream()
	dsp.free()

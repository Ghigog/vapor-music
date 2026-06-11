## test_dj_transitions.gd
## GUT unit and integration tests for AudioManager transition functionality (Deck A/B, EQ, Filters).
extends GutTest

func before_each() -> void:
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3", "song3.mp3"]
	AudioManager.current_track_index = 0
	AudioManager.is_transitioning = false
	AudioManager.upcoming_track_override = ""
	AudioManager.active_player = AudioManager.player_a
	AudioManager.player = AudioManager.active_player
	AudioManager.transition_duration = 0.1
	
	# Reset bus volumes and effects to clear state
	AudioManager._reset_bus_effects(AudioServer.get_bus_index("DeckA"))
	AudioManager._reset_bus_effects(AudioServer.get_bus_index("DeckB"))

func test_deck_audio_buses_configured() -> void:
	var bus_a = AudioServer.get_bus_index("DeckA")
	var bus_b = AudioServer.get_bus_index("DeckB")
	
	assert_ne(bus_a, -1, "DeckA bus should exist")
	assert_ne(bus_b, -1, "DeckB bus should exist")
	
	# Check effects on DeckA
	var eq = AudioServer.get_bus_effect(bus_a, 0)
	var lp = AudioServer.get_bus_effect(bus_a, 1)
	var hp = AudioServer.get_bus_effect(bus_a, 2)
	
	assert_true(eq is AudioEffectEQ6, "Effect 0 should be EQ6")
	assert_true(lp is AudioEffectLowPassFilter, "Effect 1 should be LowPassFilter")
	assert_true(hp is AudioEffectHighPassFilter, "Effect 2 should be HighPassFilter")

func test_reset_bus_effects() -> void:
	var bus_a = AudioServer.get_bus_index("DeckA")
	
	# Modify effects
	var eq = AudioServer.get_bus_effect(bus_a, 0) as AudioEffectEQ6
	eq.set_band_gain_db(0, -10.0)
	
	var lp = AudioServer.get_bus_effect(bus_a, 1) as AudioEffectLowPassFilter
	lp.cutoff_hz = 500.0
	
	var hp = AudioServer.get_bus_effect(bus_a, 2) as AudioEffectHighPassFilter
	hp.cutoff_hz = 1000.0
	
	# Reset
	AudioManager._reset_bus_effects(bus_a)
	
	assert_eq(eq.get_band_gain_db(0), 0.0, "EQ band gain should be reset to 0")
	assert_eq(lp.cutoff_hz, 20000.0, "Lowpass cutoff should be reset to 20000")
	assert_eq(hp.cutoff_hz, 10.0, "Highpass cutoff should be reset to 10")

func test_upcoming_track_override_used_in_transition() -> void:
	AudioManager.upcoming_track_override = "song3.mp3"
	
	# We can mock the loading method to avoid downloading files
	var original_load = AudioManager._load_and_stream_remote_file
	
	# Stub the loading function so it doesn't try to download from webdav
	# Instead, we manually trigger the transition sequence by mocking or overriding.
	# But in GUT, we can check the path selection logic inside start_transition.
	# Since start_transition is async (it awaits _load_and_stream_remote_file), we can watch signals
	# and make sure it selects 'song3.mp3'.
	
	var signal_watcher = watch_signals(AudioManager)
	
	# We bypass the actual web request in the mock below by just calling it or stubbing it.
	# Let's verify that when we start a transition, the transition_started signal is emitted
	# with the override track name.
	
	# Let's inspect AudioManager.start_transition. It does:
	# var incoming_player = player_b if active_player == player_a else player_a
	# transition_started.emit(next_track_href, transition_type)
	# await _load_and_stream_remote_file(...)
	# Since it awaits, it will pause. But we can check that it emitted `transition_started`
	# with the correct track and type before it actually requests the network.
	
	AudioManager.start_transition()
	
	assert_signal_emitted(AudioManager, "transition_started", "Should emit transition_started signal")
	var signal_args = get_signal_parameters(AudioManager, "transition_started", 0)
	assert_eq(signal_args[0], "song3.mp3", "Transition track should be the override 'song3.mp3'")
	assert_true(signal_args[1] in ["Standard Crossfade", "Bass Swap", "Filter Sweep"], "Transition type should be valid")

func test_transition_switches_active_players() -> void:
	# We want to test that _run_deck_transition switches active_player at the end.
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	
	assert_eq(AudioManager.active_player, deck_a, "Deck A should start as active")
	
	# Run a transition manually
	AudioManager._run_deck_transition(deck_a, deck_b, "Standard Crossfade")
	
	# Yield/Wait for Tween to finish (takes 8 seconds, but we can speed it up or mock/test it directly).
	# Wait, in unit tests, 8 seconds is a long time. But in GUT, we can test that the final state
	# is updated. Let's see if we can yield or just call it with a custom quick duration if supported.
	# Actually, since _run_deck_transition uses `create_tween()`, during headless unit testing it will execute.
	# Let's wait for a short duration or we can check the initial crossfade setup.
	
	var bus_a = AudioServer.get_bus_index("DeckA")
	var bus_b = AudioServer.get_bus_index("DeckB")
	
	# Check immediately after starting the transition that volumes are set for crossfade initiation
	assert_eq(AudioServer.get_bus_volume_db(bus_a), 0.0, "Outgoing bus volume should start at 0dB")
	assert_eq(AudioServer.get_bus_volume_db(bus_b), -60.0, "Incoming bus volume should start at -60dB")
	
	# Let's wait for the tween to finish. With 0.1s transition_duration, we only need to wait 0.15s.
	await wait_seconds(0.15)
	
	assert_eq(AudioManager.active_player, deck_b, "Active player should have switched to Deck B")
	assert_eq(AudioManager.is_transitioning, false, "Should no longer be transitioning")

func test_upcoming_transition_updates_on_override() -> void:
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3", "song3.mp3"]
	AudioManager.current_track_index = 0
	
	# Set upcoming track override
	AudioManager.upcoming_track_override = "song3.mp3"
	
	# Assert that get_next_track_href() returns the override
	assert_eq(AudioManager.get_next_track_href(), "song3.mp3", "Next track href should be the override")
	
	# Assert that upcoming transition type is determined
	assert_true(AudioManager.upcoming_transition_type in ["Standard Crossfade", "Bass Swap", "Filter Sweep", "Echo Out", "Reverb Freeze", "Tempo Morph"], "Upcoming transition type should be set and valid")


func test_setup_audio_buses_restores_missing_effects() -> void:
	var bus_a = AudioServer.get_bus_index("DeckA")
	assert_ne(bus_a, -1)
	
	# Artificially remove Lowpass filter (index 1)
	AudioServer.remove_bus_effect(bus_a, 1)
	
	# Run setup again
	AudioManager._setup_audio_buses()
	
	# Verify that lowpass has been restored at index 1 and highpass is still at 2
	var eq = AudioServer.get_bus_effect(bus_a, 0)
	var lp = AudioServer.get_bus_effect(bus_a, 1)
	var hp = AudioServer.get_bus_effect(bus_a, 2)
	
	assert_true(eq is AudioEffectEQ6, "Effect 0 should be EQ6")
	assert_true(lp is AudioEffectLowPassFilter, "Effect 1 should be LowPassFilter")
	assert_true(hp is AudioEffectHighPassFilter, "Effect 2 should be HighPassFilter")


func test_bass_swap_volume_envelope() -> void:
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	var bus_a = AudioServer.get_bus_index("DeckA")
	var bus_b = AudioServer.get_bus_index("DeckB")
	
	# Start a Bass Swap transition
	AudioManager._run_deck_transition(deck_a, deck_b, "Bass Swap")
	
	# Verify initial state
	assert_eq(AudioServer.get_bus_volume_db(bus_a), 0.0, "Outgoing volume starts at 0dB")
	assert_eq(AudioServer.get_bus_volume_db(bus_b), -60.0, "Incoming volume starts at -60dB")
	
	# Wait 0.03s (less than the 0.05s midpoint of 0.1s transition)
	await wait_seconds(0.03)
	
	# Outgoing should still be at 0.0dB because fade starts at 0.05s
	assert_eq(AudioServer.get_bus_volume_db(bus_a), 0.0, "Outgoing volume should stay at 0dB before midpoint")
	# Incoming should be fading in (between -60.0dB and 0.0dB)
	assert_gt(AudioServer.get_bus_volume_db(bus_b), -60.0, "Incoming volume should fade in from -60dB")
	assert_lt(AudioServer.get_bus_volume_db(bus_b), 0.0, "Incoming volume should be in progress")
	
	# Clean up to avoid leaking running tweens
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false


func test_filter_sweep_volume_envelope() -> void:
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	var bus_a = AudioServer.get_bus_index("DeckA")
	var bus_b = AudioServer.get_bus_index("DeckB")
	
	# Start a Filter Sweep transition
	AudioManager._run_deck_transition(deck_a, deck_b, "Filter Sweep")
	
	# Verify initial state
	assert_eq(AudioServer.get_bus_volume_db(bus_a), 0.0, "Outgoing volume starts at 0dB")
	assert_eq(AudioServer.get_bus_volume_db(bus_b), -60.0, "Incoming volume starts at -60dB")
	
	# Wait 0.03s (less than the 0.0625s delay of 0.1s transition)
	await wait_seconds(0.03)
	
	# Outgoing should still be at 0.0dB because fade starts at 0.0625s
	assert_eq(AudioServer.get_bus_volume_db(bus_a), 0.0, "Outgoing volume should stay at 0dB before delay")
	# Incoming should be fading in (between -60.0dB and 0.0dB)
	assert_gt(AudioServer.get_bus_volume_db(bus_b), -60.0, "Incoming volume should fade in from -60dB")
	assert_lt(AudioServer.get_bus_volume_db(bus_b), 0.0, "Incoming volume should be in progress")
	
	# Clean up to avoid leaking running tweens
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false


func test_playback_history_navigation() -> void:
	# Clear initial history
	AudioManager.playback_history.clear()
	AudioManager.history_pointer = -1
	
	var playlist = ["song1.mp3", "song2.mp3", "song3.mp3"]
	
	# Manually play tracks
	AudioManager.play_track("song1.mp3", playlist)
	assert_eq(AudioManager.playback_history.size(), 1)
	assert_eq(AudioManager.history_pointer, 0)
	
	AudioManager.play_track("song2.mp3", playlist)
	assert_eq(AudioManager.playback_history.size(), 2)
	assert_eq(AudioManager.history_pointer, 1)
	
	AudioManager.play_track("song3.mp3", playlist)
	assert_eq(AudioManager.playback_history.size(), 3)
	assert_eq(AudioManager.history_pointer, 2)
	
	# Trigger previous to go back in history (from song3 to song2)
	AudioManager.play_previous()
	assert_eq(AudioManager.history_pointer, 1, "Pointer should decrement to 1")
	assert_eq(AudioManager.current_track_index, 1, "Should set index to song2.mp3")
	
	# Trigger previous to go back in history (from song2 to song1)
	AudioManager.play_previous()
	assert_eq(AudioManager.history_pointer, 0, "Pointer should decrement to 0")
	assert_eq(AudioManager.current_track_index, 0, "Should set index to song1.mp3")
	
	# Now let's test play_next() when pointer is inside history (from 0 to 1)
	AudioManager.play_next()
	assert_eq(AudioManager.history_pointer, 1, "Pointer should increment to 1")
	assert_eq(AudioManager.current_track_index, 1, "Should set index to song2.mp3")
	
	# Stop debounce timer to clean up test run
	if AudioManager._debounce_timer:
		AudioManager._debounce_timer.stop()


func test_echo_out_envelope() -> void:
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	var bus_a = AudioServer.get_bus_index("DeckA")
	var bus_b = AudioServer.get_bus_index("DeckB")
	
	# Start an Echo Out transition
	AudioManager._run_deck_transition(deck_a, deck_b, "Echo Out")
	
	var delay_a = AudioServer.get_bus_effect(bus_a, 3) as AudioEffectDelay
	assert_not_null(delay_a, "Delay effect should be configured at index 3")
	assert_eq(delay_a.dry, 1.0, "Outgoing delay dry should start at 1.0")
	assert_eq(delay_a.feedback_level_db, -10.0, "Feedback level should be high")
	
	# Wait 0.03s (less than the 0.05s midpoint of 0.1s transition)
	await wait_seconds(0.03)
	assert_eq(delay_a.dry, 1.0, "Outgoing delay dry should remain 1.0 before midpoint")
	
	# Wait until after midpoint (at 0.05s)
	await wait_seconds(0.04)
	assert_lt(delay_a.dry, 0.5, "Outgoing delay dry should be tweened toward 0.0 after midpoint")
	
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false


func test_reverb_freeze_envelope() -> void:
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	var bus_a = AudioServer.get_bus_index("DeckA")
	var bus_b = AudioServer.get_bus_index("DeckB")
	
	# Start a Reverb Freeze transition
	AudioManager._run_deck_transition(deck_a, deck_b, "Reverb Freeze")
	
	var reverb_a = AudioServer.get_bus_effect(bus_a, 4) as AudioEffectReverb
	assert_not_null(reverb_a, "Reverb effect should be configured at index 4")
	assert_eq(reverb_a.wet, 0.0, "Outgoing reverb wet should start at 0.0")
	assert_eq(reverb_a.dry, 1.0, "Outgoing reverb dry should start at 1.0")
	
	# Wait 0.03s (less than 0.05s midpoint)
	await wait_seconds(0.03)
	assert_gt(reverb_a.wet, 0.0, "Outgoing reverb wet should ramp up")
	assert_eq(reverb_a.dry, 1.0, "Outgoing reverb dry should remain 1.0 before midpoint")
	
	# Wait until after midpoint
	await wait_seconds(0.04)
	assert_lt(reverb_a.dry, 0.5, "Outgoing reverb dry should cut toward 0.0 after midpoint")
	
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false


func test_tempo_morph_envelope() -> void:
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	
	# Add mock metadata to cache for BPM lookup
	var meta_a = {"bpm": 128.0, "genre": "House"}
	var meta_b = {"bpm": 120.0, "genre": "House"}
	MetadataService.cache[AudioManager.current_playlist[0]] = meta_a
	MetadataService.cache[AudioManager.current_playlist[1]] = meta_b
	
	# Set current playlist index to 1 so outgoing is playlist[0], incoming is playlist[1]
	AudioManager.current_track_index = 1
	
	# Start a Tempo Morph transition
	AudioManager._run_deck_transition(deck_a, deck_b, "Tempo Morph")
	
	# Midpoint BPM target = (128 + 120) / 2 = 124
	# Outgoing pitch_scale target = 124 / 128 = 0.96875
	# Incoming pitch_scale target = 124 / 120 = 1.0333
	
	# Wait 0.03s for pitch tween to progress (duration is 0.1s, ramp time is 0.025s)
	await wait_seconds(0.03)
	
	assert_lt(deck_a.pitch_scale, 0.99, "Outgoing deck pitch scale should scale down")
	assert_gt(deck_b.pitch_scale, 1.01, "Incoming deck pitch scale should scale up")
	
	# Let the transition finish (takes 0.1s total, so wait another 0.1s)
	await wait_seconds(0.15)
	
	assert_eq(AudioManager.active_player, deck_b, "Deck B should now be active player")
	
	# In tests, post-transition pitch ramp tween takes 0.1s to restore pitch scale back to 1.0
	await wait_seconds(0.15)
	assert_eq(deck_b.pitch_scale, 1.0, "Active player pitch scale should be restored to 1.0")
	
	# Clean up cache
	MetadataService.cache.erase(AudioManager.current_playlist[0])
	MetadataService.cache.erase(AudioManager.current_playlist[1])
	
	# Stop debounce timer to clean up test run
	if AudioManager._debounce_timer:
		AudioManager._debounce_timer.stop()


func test_pause_during_transition_pauses_both_players_and_tween() -> void:
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	deck_a.stop()
	deck_b.stop()
	deck_a.stream = AudioStreamGenerator.new()
	deck_b.stream = AudioStreamGenerator.new()
	
	# Outgoing deck must be playing to test pausing it
	deck_a.play()
	
	# Ensure they are playing first
	var original_is_playing = AudioManager.is_playing
	AudioManager.is_playing = true
	
	# Start transition
	AudioManager._run_deck_transition(deck_a, deck_b, "Standard Crossfade")
	
	# Pause
	AudioManager.toggle_play()
	
	assert_false(AudioManager.is_playing, "is_playing should be false")
	assert_true(deck_a.stream_paused, "Deck A should be paused")
	assert_true(deck_b.stream_paused, "Deck B should be paused")
	assert_true(AudioManager.active_tween == null or not AudioManager.active_tween.is_running(), "Active tween should be paused")
	
	# Resume
	AudioManager.toggle_play()
	
	assert_true(AudioManager.is_playing, "is_playing should be true")
	assert_false(deck_a.stream_paused, "Deck A should not be paused")
	assert_false(deck_b.stream_paused, "Deck B should not be paused")
	
	# Clean up
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false
	AudioManager.is_playing = original_is_playing


func test_manual_skip_triggers_immediate_transition() -> void:
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3", "song3.mp3"]
	AudioManager.current_track_index = 0
	var original_is_playing = AudioManager.is_playing
	AudioManager.is_playing = true
	
	# Ensure the player has stream so get_playback_position and remaining works
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	deck_a.stop()
	deck_b.stop()
	deck_a.stream = AudioStreamGenerator.new()
	deck_a.play()
	
	AudioManager.active_player = deck_a
	
	var original_duration = AudioManager.transition_duration
	AudioManager.transition_duration = 0.1
	
	# Start transition with force_immediate = true (simulated skip button)
	AudioManager.start_transition(true)
	
	# Wait for loading to yield
	await wait_seconds(0.05)
	
	# Since force_immediate is true, it should immediately trigger _run_deck_transition
	# active_player should start switching/transitioning.
	assert_true(AudioManager.is_transitioning, "Should be transitioning")
	assert_not_null(AudioManager.active_tween, "Active tween should be created immediately")
	
	# Clean up
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false
	AudioManager.transition_duration = original_duration
	AudioManager.is_playing = original_is_playing


func test_incoming_player_not_playing_during_loading() -> void:
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3", "song3.mp3"]
	AudioManager.current_track_index = 0
	var original_is_playing = AudioManager.is_playing
	AudioManager.is_playing = true
	
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	deck_a.stop()
	deck_b.stop()
	deck_a.stream = AudioStreamGenerator.new()
	deck_a.play()
	AudioManager.active_player = deck_a
	AudioManager.current_track_length = 30.0 # Set track length to prevent wait loop from breaking immediately
	
	var original_duration = AudioManager.transition_duration
	AudioManager.transition_duration = 5.0 # Set long duration so wait loop runs
	
	# Trigger a standard transition (force_immediate = false)
	AudioManager.start_transition(false)
	
	# Wait for loading to finish and enter the wait loop
	await wait_seconds(0.05)
	
	# At this point, transition has loaded, but is waiting for outro
	# Confirm the incoming player (deck_b) is NOT playing (it should be stopped/paused)
	assert_false(deck_b.playing, "Incoming deck should not be playing during the wait loop")
	
	# Clean up
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false
	AudioManager.transition_duration = original_duration
	AudioManager.is_playing = original_is_playing

func test_transition_key_clash_override() -> void:
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3"]
	AudioManager.current_track_index = 0
	
	# song1 (10A) -> song2 (2A) is incompatible (clashing key)
	# BPM diff is 2.0 (small), but keys clash, so Reverb Freeze or Echo Out should be selected
	var meta_a = {"bpm": 120.0, "musical_key": "10A", "genre": "House"}
	var meta_b = {"bpm": 122.0, "musical_key": "2A", "genre": "House"}
	MetadataService.cache[AudioManager.current_playlist[0]] = meta_a
	MetadataService.cache[AudioManager.current_playlist[1]] = meta_b
	
	AudioManager._update_upcoming_transition()
	
	assert_eq(AudioManager.upcoming_transition_type, "Reverb Freeze", "Should override to Reverb Freeze to mask key clash")
	
	# Clean up cache
	MetadataService.cache.erase(AudioManager.current_playlist[0])
	MetadataService.cache.erase(AudioManager.current_playlist[1])






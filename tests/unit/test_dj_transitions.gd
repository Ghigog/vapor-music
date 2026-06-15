## test_dj_transitions.gd
## GUT unit and integration tests for AudioManager transition functionality (Deck A/B, EQ, Filters).
extends GutTest

var _original_smart_mixing = false

func before_each() -> void:
	_original_smart_mixing = AudioManager.smart_mixing_enabled
	AudioManager.smart_mixing_enabled = true
	AudioManager._block_finished_signal = true
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3", "song3.mp3"]
	AudioManager.current_track_index = 0
	AudioManager.is_transitioning = false
	AudioManager.upcoming_track_override = ""
	if AudioManager.player_a:
		AudioManager.player_a.stop()
	if AudioManager.player_b:
		AudioManager.player_b.stop()
	if AudioManager.dsp_a:
		AudioManager.dsp_a.clear_stream()
	if AudioManager.dsp_b:
		AudioManager.dsp_b.clear_stream()
	AudioManager.active_player = AudioManager.player_a
	AudioManager.player = AudioManager.active_player
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	if AudioManager._pitch_ramp_tween and AudioManager._pitch_ramp_tween.is_valid():
		AudioManager._pitch_ramp_tween.kill()
	AudioManager.is_playing = true
	AudioManager.transition_duration = 0.1
	
	# Reset bus volumes and effects to clear state
	AudioManager._reset_bus_effects(AudioServer.get_bus_index("DeckA"))
	AudioManager._reset_bus_effects(AudioServer.get_bus_index("DeckB"))

func after_each() -> void:
	AudioManager._block_finished_signal = false
	AudioManager.smart_mixing_enabled = _original_smart_mixing

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
	assert_true(signal_args[1] in ["Standard Crossfade", "Bass Swap", "Filter Sweep", "Echo Out", "Reverb Freeze", "Tempo Morph"], "Transition type should be valid")

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
	assert_not_null(AudioManager.active_tween, "Tween should be created")
	AudioManager.active_tween.pause()
	
	# Verify initial state
	assert_eq(AudioServer.get_bus_volume_db(bus_a), 0.0, "Outgoing volume starts at 0dB")
	assert_eq(AudioServer.get_bus_volume_db(bus_b), -60.0, "Incoming volume starts at -60dB")
	
	# Step 0.03s (less than the 0.05s midpoint of 0.1s transition)
	AudioManager.active_tween.custom_step(0.03)
	
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
	assert_not_null(AudioManager.active_tween, "Tween should be created")
	AudioManager.active_tween.pause()
	
	# Verify initial state
	assert_eq(AudioServer.get_bus_volume_db(bus_a), 0.0, "Outgoing volume starts at 0dB")
	assert_eq(AudioServer.get_bus_volume_db(bus_b), -60.0, "Incoming volume starts at -60dB")
	
	# Step 0.03s (less than the 0.0625s delay of 0.1s transition)
	AudioManager.active_tween.custom_step(0.03)
	
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
	assert_not_null(AudioManager.active_tween, "Tween should be created")
	AudioManager.active_tween.pause()
	
	var delay_a = AudioServer.get_bus_effect(bus_a, 3) as AudioEffectDelay
	assert_not_null(delay_a, "Delay effect should be configured at index 3")
	assert_eq(delay_a.dry, 1.0, "Outgoing delay dry should start at 1.0")
	assert_eq(delay_a.feedback_level_db, -10.0, "Feedback level should be high")
	
	# Step 0.03s (less than the 0.05s midpoint of 0.1s transition)
	AudioManager.active_tween.custom_step(0.03)
	assert_eq(delay_a.dry, 1.0, "Outgoing delay dry should remain 1.0 before midpoint")
	
	# Step 0.04s (total 0.07s, which is after midpoint at 0.05s)
	AudioManager.active_tween.custom_step(0.04)
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
	assert_not_null(AudioManager.active_tween, "Tween should be created")
	AudioManager.active_tween.pause()
	
	var reverb_a = AudioServer.get_bus_effect(bus_a, 4) as AudioEffectReverb
	assert_not_null(reverb_a, "Reverb effect should be configured at index 4")
	assert_eq(reverb_a.wet, 0.0, "Outgoing reverb wet should start at 0.0")
	assert_eq(reverb_a.dry, 1.0, "Outgoing reverb dry should start at 1.0")
	
	# Step 0.03s (less than 0.05s midpoint)
	AudioManager.active_tween.custom_step(0.03)
	assert_gt(reverb_a.wet, 0.0, "Outgoing reverb wet should ramp up")
	assert_eq(reverb_a.dry, 1.0, "Outgoing reverb dry should remain 1.0 before midpoint")
	
	# Step 0.04s (total 0.07s)
	AudioManager.active_tween.custom_step(0.04)
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
	assert_not_null(AudioManager.active_tween, "Tween should be created")
	AudioManager.active_tween.pause()
	
	# Step 0.03s for speed scale tween to progress (duration is 0.1s, ramp time is 0.025s)
	AudioManager.active_tween.custom_step(0.03)
	
	assert_lt(AudioManager._speed_scale_a, 0.99, "Outgoing deck speed scale should scale down")
	assert_gt(AudioManager._speed_scale_b, 1.01, "Incoming deck speed scale should scale up")
	assert_eq(deck_a.pitch_scale, 1.0, "Outgoing deck pitch scale should remain locked at 1.0")
	assert_eq(deck_b.pitch_scale, 1.0, "Incoming deck pitch scale should remain locked at 1.0")
	
	# Finish transition by stepping another 0.1s
	AudioManager.active_tween.custom_step(0.1)
	
	assert_eq(AudioManager.active_player, deck_b, "Deck B should now be active player")
	
	# Pause and step post-transition pitch ramp tween
	if AudioManager._pitch_ramp_tween and AudioManager._pitch_ramp_tween.is_valid():
		AudioManager._pitch_ramp_tween.pause()
		AudioManager._pitch_ramp_tween.custom_step(0.15)
		
	assert_eq(AudioManager._speed_scale_b, 1.0, "Active player speed scale should be restored to 1.0")
	assert_eq(deck_b.pitch_scale, 1.0, "Active player pitch scale should remain locked at 1.0")
	
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
	AudioManager.is_playing = false
	
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
	var original_sm = AudioManager.smart_mixing_enabled
	AudioManager.smart_mixing_enabled = true
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
	AudioManager.smart_mixing_enabled = original_sm

func test_transition_trigger_alignment() -> void:
	# Mock metadata cache for track 1 and track 2
	var href_a = "song1.mp3"
	var href_b = "song2.mp3"
	
	var downbeats_a = []
	var beat_grid_a = []
	# Create a mock beat grid with 120 BPM (0.5s per beat, 2.0s per bar)
	# 8-bar loop is 16.0 seconds (8 * 2.0s)
	# We populate 100 beats
	for i in range(100):
		beat_grid_a.append(i * 0.5)
		if i % 4 == 0:
			downbeats_a.append(i * 0.5)
			
	var downbeats_b = [0.0, 2.0, 4.0]
	
	var meta_a = {
		"bpm": 120.0,
		"beat_grid": beat_grid_a,
		"downbeats": downbeats_a,
		"segments": {
			"outro": [30.0, 50.0] # Outro starts at 30.0s
		}
	}
	var meta_b = {
		"bpm": 120.0,
		"beat_grid": [0.0, 0.5, 1.0],
		"downbeats": downbeats_b,
		"cue_in": 0.0
	}
	
	MetadataService.cache[href_a] = meta_a
	MetadataService.cache[href_b] = meta_b
	
	# Target 8-bar boundary downbeat >= 30.0s:
	# Downbeat indices are multiples of 4 (e.g. 0, 4, 8, 12... in beats).
	# 8-bar loop boundaries are multiples of 32 (e.g. 0, 32, 64 in beats).
	# beat_grid_a[32] = 16.0s (Bar 9 downbeat)
	# beat_grid_a[64] = 32.0s (Bar 17 downbeat) -> lands >= outro_start (30.0s)!
	# So the first 8-bar boundary downbeat >= 30.0s is at 32.0s (beat index 64).
	#
	# Since first_downbeat_in = 0.0, cue_in = 0.0, time_to_beat = 0.0.
	# The trigger time should be exactly 32.0 - 0.0 = 32.0s.
	
	var trigger_time = AudioManager.get_aligned_trigger_time(10.0, 120.0, downbeats_a, 0.0, downbeats_b, 30.0)
	assert_eq(trigger_time, 32.0, "Trigger time should align to the 8-bar loop boundary at 32.0s")
	
	# Clean up cache
	MetadataService.cache.erase(href_a)
	MetadataService.cache.erase(href_b)

func test_vocal_clash_detection_and_masking() -> void:
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3"]
	AudioManager.current_track_index = 1
	
	# Both outgoing and incoming tracks have vocals
	var meta_a = {
		"bpm": 120.0,
		"vocal_presence": true,
		"genre": "House",
		"downbeats": [0.0, 2.0],
		"beat_grid": [0.0, 0.5, 1.0, 1.5]
	}
	var meta_b = {
		"bpm": 120.0,
		"vocal_presence": true,
		"genre": "House",
		"downbeats": [0.0, 2.0],
		"beat_grid": [0.0, 0.5, 1.0, 1.5]
	}
	MetadataService.cache[AudioManager.current_playlist[0]] = meta_a
	MetadataService.cache[AudioManager.current_playlist[1]] = meta_b
	
	var deck_a = AudioManager.player_a
	var deck_b = AudioManager.player_b
	var bus_a = AudioServer.get_bus_index("DeckA")
	
	# Start standard transition
	AudioManager.is_transitioning = true
	AudioManager._run_deck_transition(deck_a, deck_b, "Standard Crossfade")
	
	# Assert that a mid-range EQ attenuation is applied
	var eq = AudioServer.get_bus_effect(bus_a, 0) as AudioEffectEQ
	assert_not_null(eq, "EQ effect should exist")
	
	# Mid-range bands are band 2 and band 3
	# Let's pause tween to examine the start/mid-step values
	assert_not_null(AudioManager.active_tween, "Tween should be created")
	AudioManager.active_tween.pause()
	
	# Step mid-way to see EQ attenuation applied
	AudioManager.active_tween.custom_step(0.05) # half of 0.1s duration
	
	var gain_band_2 = eq.get_band_gain_db(2)
	var gain_band_3 = eq.get_band_gain_db(3)
	
	# We expect the attenuation to be at least -20dB (e.g. -24dB is applied)
	assert_lt(gain_band_2, -20.0, "Band 2 attenuation should be at least -20dB")
	assert_lt(gain_band_3, -20.0, "Band 3 attenuation should be at least -20dB")
	
	# Clean up
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false
	
	MetadataService.cache.erase(AudioManager.current_playlist[0])
	MetadataService.cache.erase(AudioManager.current_playlist[1])

func test_phrase_adaptive_transition_durations() -> void:
	var original_smart_mixing = AudioManager.smart_mixing_enabled
	AudioManager.smart_mixing_enabled = true
	var original_td = AudioManager.transition_duration
	AudioManager.transition_duration = 0.0

	var track_out = "res://tests/unit/song_out.mp3"
	var track_in = "res://tests/unit/song_in.mp3"
	
	# 1. Normal case: outro duration = 12s, intro duration = 16s -> dynamic duration = 12s
	MetadataService.cache[track_out] = {
		"segments": {
			"outro": [180.0, 192.0]
		}
	}
	MetadataService.cache[track_in] = {
		"segments": {
			"intro": [0.0, 16.0]
		}
	}
	
	var duration = AudioManager.get_transition_duration("Bass Swap", track_out, track_in)
	assert_eq(duration, 12.0, "Duration should be set to 12.0s (min of 12 and 16)")
	
	# 2. Clamped minimum case: outro = 2s, intro = 10s -> dynamic duration = 3.0s (clamped min)
	MetadataService.cache[track_out] = {
		"segments": {
			"outro": [180.0, 182.0]
		}
	}
	duration = AudioManager.get_transition_duration("Bass Swap", track_out, track_in)
	assert_eq(duration, 3.0, "Duration should be clamped to minimum 3.0s")
	
	# 3. Clamped maximum case: outro = 20s, intro = 18s -> dynamic duration = 16.0s (clamped max)
	MetadataService.cache[track_out] = {
		"segments": {
			"outro": [180.0, 200.0]
		}
	}
	MetadataService.cache[track_in] = {
		"segments": {
			"intro": [0.0, 18.0]
		}
	}
	duration = AudioManager.get_transition_duration("Bass Swap", track_out, track_in)
	assert_eq(duration, 16.0, "Duration should be clamped to maximum 16.0s")
	
	# 4. Incomplete/Missing metadata fallback case:
	# Outgoing track has no segments -> should fallback to default "Bass Swap" duration (6.0s)
	MetadataService.cache[track_out] = {}
	duration = AudioManager.get_transition_duration("Bass Swap", track_out, track_in)
	assert_eq(duration, 6.0, "Duration should fallback to Bass Swap default (6.0s) if outgoing segments are missing")
	
	# 5. Integration test: start transition and verify _active_transition_duration is bound
	MetadataService.cache[track_out] = {
		"segments": {
			"outro": [180.0, 192.0]
		}
	}
	MetadataService.cache[track_in] = {
		"segments": {
			"intro": [0.0, 16.0]
		}
	}
	
	AudioManager.current_playlist = [track_out, track_in]
	AudioManager.current_track_index = 1
	AudioManager._outgoing_href = track_out
	AudioManager._incoming_href = track_in
	
	AudioManager._run_deck_transition(AudioManager.player_a, AudioManager.player_b, "Bass Swap")
	assert_eq(AudioManager._active_transition_duration, 12.0, "_active_transition_duration should be bound to 12.0s")
	
	# Clean up
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false
	AudioManager.transition_duration = original_td
	AudioManager.smart_mixing_enabled = original_smart_mixing
	MetadataService.cache.erase(track_out)
	MetadataService.cache.erase(track_in)

func test_transition_loop_exits_on_finished_dsp() -> void:
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3"]
	AudioManager.current_track_index = 0
	var original_is_playing = AudioManager.is_playing
	AudioManager.is_playing = true
	
	var deck_a = AudioManager.player_a
	deck_a.stop()
	deck_a.stream = AudioStreamGenerator.new()
	deck_a.play()
	AudioManager.active_player = deck_a
	
	var path = ProjectSettings.globalize_path("res://tests/unit/test_track.wav")
	AudioManager.dsp_a.load_file(path)
	
	# Seek past the end to trigger is_finished()
	AudioManager.dsp_a.seek_pos(6.0)
	
	var original_td = AudioManager.transition_duration
	AudioManager.transition_duration = 5.0
	
	# Start transition.
	AudioManager.start_transition(false)
	
	await wait_seconds(0.05)
	
	# It should have exited wait loop and started transition immediately because DSP finished
	assert_true(AudioManager.is_transitioning, "Should be transitioning")
	assert_not_null(AudioManager.active_tween, "Should have created active tween immediately")
	
	# Clean up
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false
	AudioManager.transition_duration = original_td
	AudioManager.is_playing = original_is_playing

func test_transition_loop_exits_on_stall() -> void:
	MetadataService.cache.erase("song1.mp3")
	MetadataService.cache.erase("song2.mp3")
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3"]
	AudioManager.current_track_index = 0
	var original_is_playing = AudioManager.is_playing
	AudioManager.is_playing = true
	
	var deck_a = AudioManager.player_a
	deck_a.stop()
	deck_a.stream = AudioStreamGenerator.new()
	deck_a.play()
	AudioManager.active_player = deck_a
	
	AudioManager.dsp_a.clear_stream()
	
	var original_track_length = AudioManager.current_track_length
	AudioManager.current_track_length = 30.0
	
	var original_td = AudioManager.transition_duration
	AudioManager.transition_duration = 5.0
	
	# Start transition.
	AudioManager.start_transition(false)
	
	await wait_seconds(0.1)
	assert_true(AudioManager.is_transitioning)
	assert_true(AudioManager.active_tween == null or not AudioManager.active_tween.is_valid(), "Should be waiting and not have active tween yet")
	
	# Wait for stall detector to trigger (3.0s threshold)
	await wait_seconds(3.2)
	
	assert_true(AudioManager.is_transitioning, "Should be transitioning")
	assert_true(AudioManager.active_tween != null and AudioManager.active_tween.is_valid(), "Active tween should be created and valid after stall detection breaks the loop")
	
	# Clean up
	if AudioManager.active_tween and AudioManager.active_tween.is_valid():
		AudioManager.active_tween.kill()
	AudioManager.is_transitioning = false
	AudioManager.transition_duration = original_td
	AudioManager.is_playing = original_is_playing
	AudioManager.current_track_length = original_track_length

func test_transition_with_smart_mixing_disabled_cuts_immediately() -> void:
	var deck_a = AudioManager.player_a
	
	assert_eq(AudioManager.active_player, deck_a, "Deck A should start as active")
	
	var original_smart_mixing = AudioManager.smart_mixing_enabled
	var original_transition_duration = AudioManager.transition_duration
	var original_is_playing = AudioManager.is_playing
	
	AudioManager.smart_mixing_enabled = false
	AudioManager.transition_duration = 0.0
	AudioManager.is_playing = false
	
	# Test start_transition(true) with smart mixing disabled
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3"]
	AudioManager.current_track_index = 0
	AudioManager.active_player = deck_a
	
	# Start transition with force_immediate = true
	AudioManager.start_transition(true)
	
	# Wait for mock loading yield
	await wait_seconds(0.05)
	
	# It should have run immediately and not be transitioning
	assert_eq(AudioManager.is_transitioning, false, "Should not be transitioning after start_transition when smart mixing is disabled")
	assert_eq(AudioManager.current_track_index, 1, "Should have immediately switched to the next track index")
	assert_true(AudioManager.active_player.stream != null, "Active player should have a stream loaded")
	
	# Restore everything at the end
	AudioManager.smart_mixing_enabled = original_smart_mixing
	AudioManager.transition_duration = original_transition_duration
	AudioManager.is_playing = original_is_playing








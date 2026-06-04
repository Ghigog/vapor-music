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
	
	# Let's wait for the tween to finish. In GUT we can await wait_seconds for 8.5 seconds to verify.
	await wait_seconds(8.5)
	
	assert_eq(AudioManager.active_player, deck_b, "Active player should have switched to Deck B")
	assert_eq(AudioManager.is_transitioning, false, "Should no longer be transitioning")

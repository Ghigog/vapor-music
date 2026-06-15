## test_subtractive_eq.gd
## GUT unit tests for subtractive mixing, dynamic EQ, and RMS clipping prevention.
extends GutTest

func before_each() -> void:
	AudioManager._block_finished_signal = true
	AudioManager.current_playlist = ["song1.mp3", "song2.mp3"]
	AudioManager.current_track_index = 1
	AudioManager.is_transitioning = false
	if AudioManager.player_a:
		AudioManager.player_a.stop()
	if AudioManager.player_b:
		AudioManager.player_b.stop()
	AudioManager.active_player = AudioManager.player_a
	AudioManager.player = AudioManager.active_player
	
	# Reset bus effects to clear state
	AudioManager._reset_bus_effects(AudioServer.get_bus_index("DeckA"))
	AudioManager._reset_bus_effects(AudioServer.get_bus_index("DeckB"))

func after_each() -> void:
	AudioManager._block_finished_signal = false
	AudioManager.is_transitioning = false
	AudioManager._outgoing_player = null
	AudioManager._incoming_player = null
	AudioManager._active_transition_duration = 0.0
	AudioManager.subtractive_eq_crossfade_duration = 1.0

func test_subtractive_bass_gain_formula() -> void:
	AudioManager._outgoing_player = AudioManager.player_a
	AudioManager._incoming_player = AudioManager.player_b
	AudioManager.is_transitioning = true
	
	var out_bus = AudioManager._outgoing_player.bus
	var in_bus = AudioManager._incoming_player.bus
	
	# Test at X = 0.0
	AudioManager.crossfader = -1.0
	AudioManager.crossfader = 0.0
	var gain_out_0 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_0 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	var combined_0 = gain_out_0 + gain_in_0
	assert_almost_eq(combined_0, 1.0, 0.01, "Combined bass energy should be exactly 1.0 (0dB) at X = 0.0")
	
	# Test at X = 0.5
	AudioManager.crossfader = 0.5
	var gain_out_05 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_05 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	var combined_05 = gain_out_05 + gain_in_05
	assert_almost_eq(combined_05, 1.0, 0.01, "Combined bass energy should be exactly 1.0 (0dB) at X = 0.5")
	
	# Test at X = 1.0
	AudioManager.crossfader = 1.0
	var gain_out_1 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_1 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	var combined_1 = gain_out_1 + gain_in_1
	assert_almost_eq(combined_1, 1.0, 0.01, "Combined bass energy should be exactly 1.0 (0dB) at X = 1.0")

func test_rms_clipping_prevention() -> void:
	AudioManager._outgoing_player = AudioManager.player_a
	AudioManager._incoming_player = AudioManager.player_b
	AudioManager.is_transitioning = true
	
	# Set crossfader to midpoint
	AudioManager.crossfader = 0.5
	
	# Set high-amplitude inputs for Low, Mid, High bands on both channels
	# Reference is 0.5, so 0.8 is above reference
	AudioManager._last_rms["DeckA"] = { "low": 0.8, "mid": 0.8, "high": 0.8 }
	AudioManager._last_rms["DeckB"] = { "low": 0.8, "mid": 0.8, "high": 0.8 }
	
	# Set bus fader volumes to 0dB (1.0 linear)
	AudioServer.set_bus_volume_db(AudioServer.get_bus_index("DeckA"), 0.0)
	AudioServer.set_bus_volume_db(AudioServer.get_bus_index("DeckB"), 0.0)
	
	# Run dynamic EQ processing
	AudioManager._process_clipping_prevention()
	
	# Verify that outgoing dynamic attenuation has been applied (i.e. attenuation < 0dB)
	# Check the attenuation values in _clipping_attenuation_db
	for b in range(6):
		var atten = AudioManager._clipping_attenuation_db[b]
		assert_lt(atten, 0.0, "Dynamic attenuation should be negative (applied compression) for band %d" % b)

func test_smooth_subtractive_bass_crossfade() -> void:
	AudioManager._outgoing_player = AudioManager.player_a
	AudioManager._incoming_player = AudioManager.player_b
	AudioManager.is_transitioning = true
	
	# Set active transition duration to 0 to trigger the fallback width of 0.1
	AudioManager._active_transition_duration = 0.0
	
	var out_bus = AudioManager._outgoing_player.bus
	var in_bus = AudioManager._incoming_player.bus
	
	# Test at X = 0.45 (boundary start)
	AudioManager.crossfader = 0.45
	var gain_out_45 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_45 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	assert_almost_eq(gain_out_45, 1.0, 0.01, "Outgoing bass should be 1.0 at X = 0.45")
	assert_almost_eq(gain_in_45, 0.01, 0.001, "Incoming bass should be 0.01 (-40dB) at X = 0.45")
	
	# Test at X = 0.48 (intermediate point)
	AudioManager.crossfader = 0.48
	var gain_out_48 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_48 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	assert_almost_eq(gain_out_48, 0.7, 0.01, "Outgoing bass should be 0.7 at X = 0.48")
	assert_almost_eq(gain_in_48, 0.3, 0.01, "Incoming bass should be 0.3 at X = 0.48")
	assert_almost_eq(gain_out_48 + gain_in_48, 1.0, 0.01, "Combined bass energy should be 1.0")
	
	# Test at X = 0.50 (midpoint)
	AudioManager.crossfader = 0.50
	var gain_out_50 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_50 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	assert_almost_eq(gain_out_50, 0.5, 0.01, "Outgoing bass should be 0.5 at X = 0.50")
	assert_almost_eq(gain_in_50, 0.5, 0.01, "Incoming bass should be 0.5 at X = 0.50")
	assert_almost_eq(gain_out_50 + gain_in_50, 1.0, 0.01, "Combined bass energy should be 1.0")
	
	# Test at X = 0.52 (intermediate point)
	AudioManager.crossfader = 0.52
	var gain_out_52 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_52 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	assert_almost_eq(gain_out_52, 0.3, 0.01, "Outgoing bass should be 0.3 at X = 0.52")
	assert_almost_eq(gain_in_52, 0.7, 0.01, "Incoming bass should be 0.7 at X = 0.52")
	assert_almost_eq(gain_out_52 + gain_in_52, 1.0, 0.01, "Combined bass energy should be 1.0")
	
	# Test at X = 0.55 (boundary end)
	AudioManager.crossfader = 0.55
	var gain_out_55 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_55 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	assert_almost_eq(gain_out_55, 0.01, 0.001, "Outgoing bass should be 0.01 (-40dB) at X = 0.55")
	assert_almost_eq(gain_in_55, 1.0, 0.01, "Incoming bass should be 1.0 at X = 0.55")

func test_smooth_subtractive_bass_crossfade_configurable_duration() -> void:
	AudioManager._outgoing_player = AudioManager.player_a
	AudioManager._incoming_player = AudioManager.player_b
	AudioManager.is_transitioning = true
	
	var out_bus = AudioManager._outgoing_player.bus
	var in_bus = AudioManager._incoming_player.bus
	
	# 6.0s transition duration, 1.2s crossfade duration -> width = 0.2
	# low_bound = 0.4, high_bound = 0.6
	AudioManager._active_transition_duration = 6.0
	AudioManager.subtractive_eq_crossfade_duration = 1.2
	
	# Test at X = 0.4 (boundary start)
	AudioManager.crossfader = 0.4
	var gain_out_4 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_4 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	assert_almost_eq(gain_out_4, 1.0, 0.01, "Outgoing bass should be 1.0 at X = 0.4")
	assert_almost_eq(gain_in_4, 0.01, 0.001, "Incoming bass should be 0.01 (-40dB) at X = 0.4")
	
	# Test at X = 0.45
	AudioManager.crossfader = 0.45
	var gain_out_45 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_45 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	assert_almost_eq(gain_out_45, 0.75, 0.01, "Outgoing bass should be 0.75 at X = 0.45")
	assert_almost_eq(gain_in_45, 0.25, 0.01, "Incoming bass should be 0.25 at X = 0.45")
	
	# Test at X = 0.6 (boundary end)
	AudioManager.crossfader = 0.6
	var gain_out_6 = db_to_linear(AudioManager._transition_eq_gains[out_bus][0])
	var gain_in_6 = db_to_linear(AudioManager._transition_eq_gains[in_bus][0])
	assert_almost_eq(gain_out_6, 0.01, 0.001, "Outgoing bass should be 0.01 (-40dB) at X = 0.6")
	assert_almost_eq(gain_in_6, 1.0, 0.01, "Incoming bass should be 1.0 at X = 0.6")

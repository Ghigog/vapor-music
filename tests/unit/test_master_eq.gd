## test_master_eq.gd
## GUT unit tests for AudioManager Master Multiband Parametric EQ Layout.
extends GutTest

func before_each() -> void:
	# Run setup_audio_buses to ensure Master layout is clean
	AudioManager._setup_audio_buses()
	AudioManager.clear_eq_bands()

func after_each() -> void:
	AudioManager.clear_eq_bands()

func test_master_eq_layout_initialized() -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	assert_ne(master_idx, -1, "Master bus should exist")
	
	var count = AudioServer.get_bus_effect_count(master_idx)
	assert_gte(count, 11, "Master bus should have at least 11 effects (1 preamp + 10 bands)")
	
	var preamp = AudioServer.get_bus_effect(master_idx, 0)
	assert_true(preamp is AudioEffectAmplify, "Effect 0 should be Preamp (AudioEffectAmplify)")
	
	for i in range(10):
		var pos = i + 1
		var filter = AudioServer.get_bus_effect(master_idx, pos)
		assert_true(filter is AudioEffectFilter, "Effect " + str(pos) + " should be an AudioEffectFilter subclass")
		assert_false(AudioServer.is_bus_effect_enabled(master_idx, pos), "Filter band " + str(i) + " should be bypassed by default")

func test_set_preamp_gain() -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	AudioManager.set_preamp_gain(-3.5)
	
	var preamp = AudioServer.get_bus_effect(master_idx, 0) as AudioEffectAmplify
	assert_eq(preamp.volume_db, -3.5, "Preamp volume_db should update correctly")

func test_set_eq_band_updates_filter_parameters() -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	
	# Set a band to 1000 Hz, +3 dB gain, Q = 1.4, peaking filter
	AudioManager.set_eq_band(3, 1000.0, 3.0, 1.4, "peaking")
	
	var filter = AudioServer.get_bus_effect(master_idx, 4) as AudioEffectFilter
	assert_eq(filter.cutoff_hz, 1000.0, "Cutoff frequency should be updated")
	assert_almost_eq(filter.resonance, 1.4, 0.0001, "Resonance (Q) should be updated")
	assert_almost_eq(filter.gain, db_to_linear(3.0), 0.0001, "Linear gain multiplier should be updated correctly")
	assert_true(AudioServer.is_bus_effect_enabled(master_idx, 4), "Filter should not be bypassed when active")

func test_set_eq_band_subclass_replacement() -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	
	# Set band 0 to low shelf (lsc)
	AudioManager.set_eq_band(0, 100.0, -5.0, 0.7, "low_shelf")
	var filter_lsc = AudioServer.get_bus_effect(master_idx, 1)
	assert_true(filter_lsc is AudioEffectLowPassFilter, "Should instantiate AudioEffectLowPassFilter for low_shelf")
	
	# Set band 0 to high shelf (hsc)
	AudioManager.set_eq_band(0, 5000.0, 2.0, 0.7, "high_shelf")
	var filter_hsc = AudioServer.get_bus_effect(master_idx, 1)
	assert_true(filter_hsc is AudioEffectHighPassFilter, "Should replace with AudioEffectHighPassFilter for high_shelf")

	# Set band 0 to notch (not)
	AudioManager.set_eq_band(0, 1000.0, -12.0, 2.0, "notch")
	var filter_not = AudioServer.get_bus_effect(master_idx, 1)
	assert_true(filter_not is AudioEffectNotchFilter, "Should replace with AudioEffectNotchFilter for notch")

func test_set_eq_band_bypasses_neutral_gain() -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	
	# Setting band with neutral gain (0.0 dB) should bypass the effect
	AudioManager.set_eq_band(5, 500.0, 0.0, 1.0, "peaking")
	
	var filter = AudioServer.get_bus_effect(master_idx, 6) as AudioEffectFilter
	assert_false(AudioServer.is_bus_effect_enabled(master_idx, 6), "Filter should be bypassed when gain is neutral (0.0 dB)")

func test_clear_eq_bands() -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	
	# Configure a preamp gain and a few bands
	AudioManager.set_preamp_gain(-6.0)
	AudioManager.set_eq_band(1, 200.0, 4.0, 1.0)
	AudioManager.set_eq_band(2, 2000.0, -3.0, 0.7)
	
	# Clear
	AudioManager.clear_eq_bands()
	
	var preamp = AudioServer.get_bus_effect(master_idx, 0) as AudioEffectAmplify
	assert_eq(preamp.volume_db, 0.0, "Preamp volume should reset to 0 dB")
	
	for i in range(10):
		var pos = i + 1
		var filter = AudioServer.get_bus_effect(master_idx, pos) as AudioEffectFilter
		assert_false(AudioServer.is_bus_effect_enabled(master_idx, pos), "All filter bands should be bypassed after clear")
		assert_eq(filter.gain, 1.0, "Filter gain should reset to 1.0 (neutral)")

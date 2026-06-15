## test_headphone_settings.gd
## GUT unit tests for SettingsManager and AudioManager headphone calibration integration.
extends GutTest

func before_each() -> void:
	AudioManager._setup_audio_buses()
	AudioManager.clear_eq_bands()
	
	# Clear out setting values before test run
	SettingsManager.headphone_calibration_enabled = false
	SettingsManager.headphone_profile = ""
	SettingsManager.save_settings()

func after_each() -> void:
	AudioManager.clear_eq_bands()
	
	# Reset settings to default
	SettingsManager.headphone_calibration_enabled = false
	SettingsManager.headphone_profile = ""
	SettingsManager.save_settings()

func test_settings_manager_persists_headphone_calibration() -> void:
	SettingsManager.save_headphone_calibration_enabled(true)
	SettingsManager.save_headphone_profile("Sony WH-1000XM4")
	
	# Reload settings to verify persistence
	SettingsManager.load_settings()
	
	assert_true(SettingsManager.headphone_calibration_enabled, "Calibration enabled should be true after reload")
	assert_eq(SettingsManager.headphone_profile, "Sony WH-1000XM4", "Headphone profile should be 'Sony WH-1000XM4' after reload")

func test_audio_manager_applies_headphone_profile() -> void:
	SettingsManager.save_headphone_calibration_enabled(true)
	SettingsManager.save_headphone_profile("Sennheiser HD 600")
	
	AudioManager.update_calibration_state()
	
	# Verify that EQ bands have been updated in AudioServer
	var master_idx = AudioServer.get_bus_index("Master")
	
	# Check preamp gain
	var preamp = AudioServer.get_bus_effect(master_idx, 0) as AudioEffectAmplify
	assert_almost_eq(preamp.volume_db, -6.4, 0.0001, "Preamp volume_db should be set to -6.4 dB for Sennheiser HD 600")
	
	# Verify active EQ bands are enabled
	# Sennheiser HD 600 has 6 filters (preamp + 6 bands). So band 0 to 5 should be configured.
	# Let's check band 0: 31 Hz, -3.4 dB, Q = 1.40, peaking
	var filter1 = AudioServer.get_bus_effect(master_idx, 1) as AudioEffectFilter
	assert_true(AudioServer.is_bus_effect_enabled(master_idx, 1), "Band 1 should be enabled")
	assert_almost_eq(filter1.cutoff_hz, 31.0, 0.0001)
	assert_almost_eq(filter1.resonance, 1.4, 0.0001)
	assert_almost_eq(filter1.gain, db_to_linear(-3.4), 0.0001)

func test_audio_manager_clears_on_disabled_calibration() -> void:
	# First enable and apply
	SettingsManager.save_headphone_calibration_enabled(true)
	SettingsManager.save_headphone_profile("Sennheiser HD 600")
	AudioManager.update_calibration_state()
	
	# Disable and update
	SettingsManager.save_headphone_calibration_enabled(false)
	AudioManager.update_calibration_state()
	
	var master_idx = AudioServer.get_bus_index("Master")
	
	# Preamp should be reset to 0.0
	var preamp = AudioServer.get_bus_effect(master_idx, 0) as AudioEffectAmplify
	assert_eq(preamp.volume_db, 0.0, "Preamp should reset to 0.0 when calibration is disabled")
	
	# All bands should be bypassed/disabled
	for i in range(10):
		var pos = i + 1
		assert_false(AudioServer.is_bus_effect_enabled(master_idx, pos), "Band " + str(pos) + " should be bypassed")

## test_autoeq_parser.gd
## GUT unit tests for AutoEQ Calibration Profile Parser (EQ-002).
extends GutTest

const AutoEQParser = preload("res://scripts/services/autoeq_parser.gd")

func before_each() -> void:
	AudioManager._setup_audio_buses()
	AudioManager.clear_eq_bands()

func after_each() -> void:
	AudioManager.clear_eq_bands()

func test_parse_standard_autoeq_profile() -> void:
	var profile_text = """
Preamp: -6.5 dB
Filter 1: ON PK Fc 31 Hz Gain -3.4 dB Q 1.40
Filter 2: ON LSC Fc 62 Hz Gain 2.1 dB Q 1.40
Filter 3: ON HSC Fc 12000 Hz Gain 1.5 dB Q 0.70
Filter 4: OFF PK Fc 1000 Hz Gain 5.0 dB Q 1.00
"""
	var profile = AutoEQParser.parse_profile_text(profile_text)
	
	assert_true(profile is Dictionary, "Parsed profile should be a Dictionary")
	# Headroom check:
	# Active filters: Filter 1 (Gain -3.4), Filter 2 (Gain 2.1), Filter 3 (Gain 1.5). Filter 4 is OFF.
	# Max active positive gain is 2.1. Safe preamp is -2.1.
	# Parsed preamp is -6.5. min(-6.5, -2.1) = -6.5.
	assert_almost_eq(profile["preamp"], -6.5, 0.0001, "Preamp should remain -6.5 dB since it covers the 2.1 dB boost")
	assert_eq(profile["bands"].size(), 4, "Should parse 4 bands")
	
	var b1 = profile["bands"][0]
	assert_almost_eq(b1["frequency"], 31.0, 0.0001)
	assert_almost_eq(b1["gain"], -3.4, 0.0001)
	assert_almost_eq(b1["q"], 1.4, 0.0001)
	assert_eq(b1["type"], "peaking")
	assert_true(b1["enabled"])
	
	var b2 = profile["bands"][1]
	assert_almost_eq(b2["frequency"], 62.0, 0.0001)
	assert_almost_eq(b2["gain"], 2.1, 0.0001)
	assert_almost_eq(b2["q"], 1.4, 0.0001)
	assert_eq(b2["type"], "low_shelf")
	assert_true(b2["enabled"])

	var b3 = profile["bands"][2]
	assert_almost_eq(b3["frequency"], 12000.0, 0.0001)
	assert_almost_eq(b3["gain"], 1.5, 0.0001)
	assert_almost_eq(b3["q"], 0.7, 0.0001)
	assert_eq(b3["type"], "high_shelf")
	assert_true(b3["enabled"])

	var b4 = profile["bands"][3]
	assert_false(b4["enabled"], "Filter 4 should be parsed as disabled")

func test_clipping_prevention_preamp_adjustment() -> void:
	# Scenario 1: Max positive gain of active filters (5.0 dB) exceeds parsed preamp (-3.5 dB)
	var text_insufficient_headroom = """
Preamp: -3.5 dB
Filter 1: ON PK Fc 100 Hz Gain 5.0 dB Q 1.0
Filter 2: ON PK Fc 200 Hz Gain 2.0 dB Q 1.0
"""
	var p1 = AutoEQParser.parse_profile_text(text_insufficient_headroom)
	# Max active boost = 5.0 dB. Safe preamp = -5.0 dB.
	# min(-3.5, -5.0) = -5.0 dB.
	assert_almost_eq(p1["preamp"], -5.0, 0.0001, "Preamp should be lowered to -5.0 dB to prevent clipping")

	# Scenario 2: Max positive gain of active filters (3.0 dB) is covered by parsed preamp (-4.0 dB)
	var text_sufficient_headroom = """
Preamp: -4.0 dB
Filter 1: ON PK Fc 100 Hz Gain 3.0 dB Q 1.0
"""
	var p2 = AutoEQParser.parse_profile_text(text_sufficient_headroom)
	# min(-4.0, -3.0) = -4.0 dB.
	assert_almost_eq(p2["preamp"], -4.0, 0.0001, "Preamp should remain -4.0 dB")

	# Scenario 3: No preamp specified, max positive gain is 4.5 dB
	var text_no_preamp = """
Filter 1: ON PK Fc 100 Hz Gain 4.5 dB Q 1.0
"""
	var p3 = AutoEQParser.parse_profile_text(text_no_preamp)
	# Defaults to -4.5 dB.
	assert_almost_eq(p3["preamp"], -4.5, 0.0001, "Preamp should default to -4.5 dB to match max boost")

	# Scenario 4: All active filters have negative or neutral gains, preamp specified
	var text_all_negative = """
Preamp: -1.5 dB
Filter 1: ON PK Fc 100 Hz Gain -3.0 dB Q 1.0
Filter 2: ON PK Fc 200 Hz Gain 0.0 dB Q 1.0
"""
	var p4 = AutoEQParser.parse_profile_text(text_all_negative)
	# min(-1.5, 0.0) = -1.5 dB.
	assert_almost_eq(p4["preamp"], -1.5, 0.0001, "Preamp should remain -1.5 dB when all gains are negative/neutral")

	# Scenario 5: All active filters have negative or neutral gains, no preamp specified
	var text_all_negative_no_preamp = """
Filter 1: ON PK Fc 100 Hz Gain -3.0 dB Q 1.0
"""
	var p5 = AutoEQParser.parse_profile_text(text_all_negative_no_preamp)
	assert_almost_eq(p5["preamp"], 0.0, 0.0001, "Preamp should default to 0.0 dB when all gains are negative/neutral and no preamp specified")

func test_value_validation_and_clamping() -> void:
	# Test out of bound values clamping
	var text = """
Preamp: 30.0 dB
Filter 1: ON PK Fc 10 Hz Gain 35.0 dB Q 0.005
Filter 2: ON PK Fc 25000 Hz Gain -35.0 dB Q 15.0
"""
	var profile = AutoEQParser.parse_profile_text(text)
	
	# Preamp parsed: 30.0. Max active boost (clamped): 24.0. Safe preamp: -24.0.
	# min(30.0, -24.0) = -24.0 dB. Clamped to [-60.0, 24.0] -> -24.0.
	assert_almost_eq(profile["preamp"], -24.0, 0.0001)
	
	var b1 = profile["bands"][0]
	assert_almost_eq(b1["frequency"], 20.0, 0.0001, "Frequency below 20Hz should be clamped to 20Hz")
	assert_almost_eq(b1["gain"], 24.0, 0.0001, "Gain above 24dB should be clamped to 24dB")
	assert_almost_eq(b1["q"], 0.01, 0.0001, "Q below 0.01 should be clamped to 0.01")
	
	var b2 = profile["bands"][1]
	assert_almost_eq(b2["frequency"], 22000.0, 0.0001, "Frequency above 22000Hz should be clamped to 22000Hz")
	assert_almost_eq(b2["gain"], -24.0, 0.0001, "Gain below -24dB should be clamped to -24dB")
	assert_almost_eq(b2["q"], 10.0, 0.0001, "Q above 10.0 should be clamped to 10.0")

func test_malformed_and_loose_formatting() -> void:
	# Parse files with different format variations: missing 'dB' or 'Hz', extra spaces, different case
	var loose_text = """
# This is a comment
PREAMP: -5
filter: PK Fc 150 Gain -2.5 Q 0.8
PK Fc 220 Gain 4 Q 1.2
"""
	var profile = AutoEQParser.parse_profile_text(loose_text)
	
	assert_almost_eq(profile["preamp"], -5.0, 0.0001)
	assert_eq(profile["bands"].size(), 2)
	
	var b1 = profile["bands"][0]
	assert_almost_eq(b1["frequency"], 150.0, 0.0001)
	assert_almost_eq(b1["gain"], -2.5, 0.0001)
	assert_almost_eq(b1["q"], 0.8, 0.0001)
	assert_eq(b1["type"], "peaking")
	assert_true(b1["enabled"])

	var b2 = profile["bands"][1]
	assert_almost_eq(b2["frequency"], 220.0, 0.0001)
	assert_almost_eq(b2["gain"], 4.0, 0.0001)
	assert_almost_eq(b2["q"], 1.2, 0.0001)
	assert_eq(b2["type"], "peaking")
	assert_true(b2["enabled"])

func test_apply_eq_profile_integration() -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	assert_ne(master_idx, -1)
	
	var profile = {
		"preamp": -5.5,
		"bands": [
			{"frequency": 100.0, "gain": 4.5, "q": 1.2, "type": "peaking", "enabled": true},
			{"frequency": 1000.0, "gain": -3.0, "q": 0.7, "type": "low_shelf", "enabled": true},
			{"frequency": 5000.0, "gain": 1.5, "q": 1.5, "type": "high_shelf", "enabled": false}, # Disabled, should be skipped
			{"frequency": 8000.0, "gain": -2.0, "q": 2.0, "type": "notch", "enabled": true}
		]
	}
	
	AudioManager.apply_eq_profile(profile)
	
	# Verify preamp
	var preamp = AudioServer.get_bus_effect(master_idx, 0) as AudioEffectAmplify
	assert_almost_eq(preamp.volume_db, -5.5, 0.0001)
	
	# Band 0: frequency=100.0, gain=4.5 (linear = db_to_linear(4.5)), q=1.2, peaking (AudioEffectBandLimitFilter)
	var filter0 = AudioServer.get_bus_effect(master_idx, 1)
	assert_true(filter0 is AudioEffectBandLimitFilter)
	assert_almost_eq(filter0.cutoff_hz, 100.0, 0.0001)
	assert_almost_eq(filter0.gain, db_to_linear(4.5), 0.0001)
	assert_almost_eq(filter0.resonance, 1.2, 0.0001)
	assert_true(AudioServer.is_bus_effect_enabled(master_idx, 1))
	
	# Band 1: frequency=1000.0, gain=-3.0, q=0.7, low_shelf (AudioEffectLowPassFilter)
	var filter1 = AudioServer.get_bus_effect(master_idx, 2)
	assert_true(filter1 is AudioEffectLowPassFilter)
	assert_almost_eq(filter1.cutoff_hz, 1000.0, 0.0001)
	assert_almost_eq(filter1.gain, db_to_linear(-3.0), 0.0001)
	assert_almost_eq(filter1.resonance, 0.7, 0.0001)
	assert_true(AudioServer.is_bus_effect_enabled(master_idx, 2))
	
	# Band 2: Disabled, should remain bypassed/neutral in slot 3
	var filter2 = AudioServer.get_bus_effect(master_idx, 3)
	assert_false(AudioServer.is_bus_effect_enabled(master_idx, 3))
	
	# Band 3: frequency=8000.0, gain=-2.0, q=2.0, notch (AudioEffectNotchFilter)
	var filter3 = AudioServer.get_bus_effect(master_idx, 4)
	assert_true(filter3 is AudioEffectNotchFilter)
	assert_almost_eq(filter3.cutoff_hz, 8000.0, 0.0001)
	assert_almost_eq(filter3.gain, db_to_linear(-2.0), 0.0001)
	assert_almost_eq(filter3.resonance, 2.0, 0.0001)
	assert_true(AudioServer.is_bus_effect_enabled(master_idx, 4))
	
	# Remaining bands (4-9) should be bypassed
	for i in range(4, 10):
		var pos = i + 1
		assert_false(AudioServer.is_bus_effect_enabled(master_idx, pos), "Remaining band " + str(i) + " should be bypassed")

func test_parse_profile_file_io() -> void:
	var temp_path = "user://temp_autoeq_profile.txt"
	var text = "Preamp: -4.2 dB\nFilter 1: ON PK Fc 120 Hz Gain 3.0 dB Q 1.1\n"
	
	var file = FileAccess.open(temp_path, FileAccess.WRITE)
	assert_true(file != null, "File should be opened successfully")
	file.store_string(text)
	file.close()
	
	var profile = AutoEQParser.parse_profile_file(temp_path)
	assert_true(profile is Dictionary, "Parsed profile should be a Dictionary")
	# preamp min(-4.2, -3.0) = -4.2
	assert_almost_eq(profile["preamp"], -4.2, 0.0001)
	assert_eq(profile["bands"].size(), 1)
	
	var band = profile["bands"][0]
	assert_almost_eq(band["frequency"], 120.0, 0.0001)
	assert_almost_eq(band["gain"], 3.0, 0.0001)
	assert_almost_eq(band["q"], 1.1, 0.0001)
	assert_eq(band["type"], "peaking")
	
	# Clean up
	var dir = DirAccess.open("user://")
	if dir:
		dir.remove("temp_autoeq_profile.txt")

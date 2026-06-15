## test_beat_sync_pll.gd
## GUT unit tests for AudioManager beat grid sync and PLL deck alignment.
extends GutTest

func test_beat_alignment_offset_calculation() -> void:
	# Define test cases for get_aligned_trigger_time
	
	# Case 1: Normal downbeat alignment
	var pos_out = 9.2
	var bpm_out = 120.0
	var downbeats_out = [0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0]
	var cue_in_in = 0.5
	var downbeats_in = [1.0, 3.0, 5.0]
	# first_downbeat_in = 1.0 (since 1.0 >= 0.5)
	# time_to_beat = 1.0 - 0.5 = 0.5
	# we want db_out - 0.5 >= 9.2 => db_out >= 9.7 => db_out = 10.0
	# trigger time = 10.0 - 0.5 = 9.5
	var trigger_time = AudioManager.get_aligned_trigger_time(pos_out, bpm_out, downbeats_out, cue_in_in, downbeats_in)
	assert_eq(trigger_time, 9.5, "Trigger time should be 9.5s")
	
	# Case 2: Outgoing position is ahead of all downbeats (triggers virtual projection)
	pos_out = 13.0
	trigger_time = AudioManager.get_aligned_trigger_time(pos_out, bpm_out, downbeats_out, cue_in_in, downbeats_in)
	# last_db = 12.0
	# bar_duration = 2.0
	# we search for temp_db - 0.5 >= 13.0 => temp_db >= 13.5
	# temp_db starting at 12.0:
	# 12.0 + 2.0 = 14.0 >= 13.5 (yes)
	# trigger time = 14.0 - 0.5 = 13.5
	assert_eq(trigger_time, 13.5, "Trigger time should project to 13.5s")

	# Case 3: Empty downbeats array (triggers virtual projection from 0)
	var empty_downbeats = []
	trigger_time = AudioManager.get_aligned_trigger_time(pos_out, bpm_out, empty_downbeats, cue_in_in, downbeats_in)
	# bar_duration = 2.0
	# temp_db starts at 0.0, adds 2.0 until temp_db - 0.5 >= 13.0 => temp_db >= 13.5
	# 0, 2, 4, 6, 8, 10, 12, 14
	# trigger time = 14.0 - 0.5 = 13.5
	assert_eq(trigger_time, 13.5, "Trigger time should project to 13.5s with empty downbeats")

func test_pll_drift_correction_loop() -> void:
	# Mock the initial PLL indices
	AudioManager._pll_db_in_idx = 0
	AudioManager._pll_db_out_idx = 0
	
	var beat_grid_out = [0.0, 1.0, 2.0, 3.0]
	var beat_grid_in = [0.0, 1.0, 2.0, 3.0]
	
	# Case 1: No drift
	var pos_out = 1.0
	var pos_in = 1.0
	var res = AudioManager.calculate_pll_adjustment(pos_out, pos_in, beat_grid_out, beat_grid_in, 1.0)
	assert_eq(res["adjustment"], 0.0, "Adjustment should be 0 with no drift")
	assert_eq(res["phase_error_ms"], 0.0, "Phase error should be 0")
	
	# Case 2: Incoming is lagging (running behind) -> needs positive adjustment (speed up)
	pos_out = 1.0
	pos_in = 0.9 # lagging by 0.1s
	res = AudioManager.calculate_pll_adjustment(pos_out, pos_in, beat_grid_out, beat_grid_in, 1.0)
	assert_gt(res["adjustment"], 0.0, "Adjustment should be positive when incoming is lagging")
	# phase error = dt_in - dt_out = (1.0 - 0.9) - (1.0 - 1.0) = 0.1s = 100ms
	assert_almost_eq(res["phase_error_ms"], 100.0, 0.01, "Phase error should be 100ms")
	# adjustment = 0.1 * 0.15 = 0.015, clamped to 0.005
	assert_eq(res["adjustment"], 0.005, "Adjustment should be clamped to max 0.005")
	
	# Case 3: Incoming is leading (running ahead) -> needs negative adjustment (slow down)
	pos_out = 1.0
	pos_in = 1.1 # leading by 0.1s
	res = AudioManager.calculate_pll_adjustment(pos_out, pos_in, beat_grid_out, beat_grid_in, 1.0)
	assert_lt(res["adjustment"], 0.0, "Adjustment should be negative when incoming is leading")
	assert_almost_eq(res["phase_error_ms"], -100.0, 0.01, "Phase error should be -100ms")
	# adjustment = -0.1 * 0.15 = -0.015, clamped to -0.005
	assert_eq(res["adjustment"], -0.005, "Adjustment should be clamped to min -0.005")
	
	# Case 4: Small drift within non-clamped bounds
	pos_out = 1.0
	pos_in = 0.98 # lagging by 0.02s
	res = AudioManager.calculate_pll_adjustment(pos_out, pos_in, beat_grid_out, beat_grid_in, 1.0)
	assert_almost_eq(res["phase_error_ms"], 20.0, 0.01, "Phase error should be 20ms")
	# adjustment = 0.02 * 0.15 = 0.003
	assert_almost_eq(res["adjustment"], 0.003, 0.0001, "Adjustment should be 0.003")
	
	# Clear mock state
	AudioManager._pll_db_in_idx = -1
	AudioManager._pll_db_out_idx = -1

func test_pll_adjustment_with_cross_correlation() -> void:
	# Mock the initial PLL indices
	AudioManager._pll_db_in_idx = 0
	AudioManager._pll_db_out_idx = 0
	
	var beat_grid_out = [0.0, 1.0, 2.0, 3.0]
	var beat_grid_in = [0.0, 1.0, 2.0, 3.0]
	
	# Case 1: Grid says aligned, but cross-correlation says incoming is 50ms ahead (needs to slow down)
	var pos_out = 1.0
	var pos_in = 1.0
	var cc_offset = 0.05
	var res = AudioManager.calculate_pll_adjustment(pos_out, pos_in, beat_grid_out, beat_grid_in, 1.0, cc_offset)
	assert_almost_eq(res["phase_error_ms"], -50.0, 0.01, "Phase error should be -50ms with 50ms CC lead")
	assert_lt(res["adjustment"], 0.0, "Adjustment should be negative to slow down leading deck")
	
	# Case 2: Grid says aligned, but cross-correlation says incoming is 30ms behind (needs to speed up)
	cc_offset = -0.03
	res = AudioManager.calculate_pll_adjustment(pos_out, pos_in, beat_grid_out, beat_grid_in, 1.0, cc_offset)
	assert_almost_eq(res["phase_error_ms"], 30.0, 0.01, "Phase error should be 30ms with 30ms CC lag")
	assert_gt(res["adjustment"], 0.0, "Adjustment should be positive to speed up lagging deck")
	
	# Case 3: Grid says lagging by 100ms, but cross-correlation says actual lag is only 50ms (corrects beatgrid error)
	pos_in = 0.9
	cc_offset = 0.05
	res = AudioManager.calculate_pll_adjustment(pos_out, pos_in, beat_grid_out, beat_grid_in, 1.0, cc_offset)
	assert_almost_eq(res["phase_error_ms"], 50.0, 0.01, "Phase error should be 50ms (adjusted from 100ms grid error)")
	
	# Clear mock state
	AudioManager._pll_db_in_idx = -1
	AudioManager._pll_db_out_idx = -1

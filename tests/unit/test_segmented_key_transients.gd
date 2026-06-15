extends GutTest

const DJPathfinder = preload("res://scripts/services/dj_pathfinder.gd")
const AudioManagerClass = preload("res://scripts/services/audio_manager.gd")

func test_outro_key_matching() -> void:
	# Track A has global key 8A, but modulates to 9A in the outro segment
	var track_a = {
		"bpm": 120.0,
		"musical_key": "8A",
		"outro_key": "9A",
		"energy_level": 0.5,
		"genre": "Rock"
	}
	
	# Track B has key 9A (perfect compatibility with 9A outro, rather than 8A intro)
	var track_b = {
		"bpm": 120.0,
		"musical_key": "9A",
		"energy_level": 0.5,
		"genre": "Rock"
	}
	
	# Track C has key 8A (compatible with 8A, but incompatible / higher cost for 9A outro)
	var track_c = {
		"bpm": 120.0,
		"musical_key": "8A",
		"energy_level": 0.5,
		"genre": "Rock"
	}
	
	var cost_ab = DJPathfinder.calculate_transition_cost(track_a, track_b)
	var cost_ac = DJPathfinder.calculate_transition_cost(track_a, track_c)
	
	assert_true(cost_ab < cost_ac, "Transition cost from A to B (key 9A) should be lower than A to C (key 8A) because A modulates to 9A in outro")
	
	# Verify smart matches
	var dummy_ms = Node.new()
	var script = GDScript.new()
	script.source_code = "extends Node\nfunc get_cached_metadata(href: String) -> Dictionary:\n\tif href == 'track_a': return " + str(track_a) + "\n\tif href == 'track_b': return " + str(track_b) + "\n\tif href == 'track_c': return " + str(track_c) + "\n\treturn {}"
	script.reload()
	dummy_ms.set_script(script)
	
	var playlist = ["track_a", "track_b", "track_c"]
	var matches = DJPathfinder.calculate_smart_matches("track_a", playlist, dummy_ms)
	
	assert_eq(matches.get("perfect", {}).get("href", ""), "track_b", "Perfect match should be track_b due to compatible outro key 9A")
	
	dummy_ms.free()

func test_transient_peak_avoidance() -> void:
	var audio_manager = AudioManagerClass.new()
	
	var bpm_out = 120.0
	var beat_duration = 60.0 / bpm_out # 0.5 seconds per beat
	
	# Standard downbeats starting at 0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0
	var downbeats_out = [0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0]
	var downbeats_in = [0.0, 2.0, 4.0]
	
	var pos_out = 5.0
	var cue_in = 0.0
	var outro_start = 12.0
	
	# The default target downbeat landing on 8-bar phrasing loop boundary:
	# k % 8 == 0 -> downbeats_out[0] (0.0), downbeats_out[8] (16.0)
	# 16.0 >= outro_start (12.0) and 16.0 >= pos_out (5.0), so 16.0 is selected.
	
	# Mock a major transient right at the planned trigger downbeat (16.0)
	var transients_out = [16.0]
	var transients_in = []
	
	var trigger_time = audio_manager.get_aligned_trigger_time(pos_out, bpm_out, downbeats_out, cue_in, downbeats_in, outro_start, transients_out, transients_in)
	
	# Since 16.0 has a transient collision (distance 0.0 < 0.5s safety margin),
	# it should be micro-shifted by 4 beats (4 * 0.5s = 2.0s).
	# Since 16.0 - 2.0 = 14.0 >= pos_out (5.0), it should shift earlier to 14.0.
	assert_eq(trigger_time, 14.0, "Trigger time should be micro-shifted by 4 beats (2.0s) earlier to avoid transient at 16.0")
	
	# Test case where shifting earlier goes below pos_out:
	# Let's place pos_out at 15.0. 16.0 - 2.0 = 14.0 < 15.0.
	# In this case, it should shift later to 18.0.
	var pos_out_late = 15.0
	var trigger_time_late = audio_manager.get_aligned_trigger_time(pos_out_late, bpm_out, downbeats_out, cue_in, downbeats_in, outro_start, transients_out, transients_in)
	assert_eq(trigger_time_late, 18.0, "Trigger time should be micro-shifted later to 18.0 because shifting earlier goes below current play position")
	
	audio_manager.free()

func test_multi_pass_transient_peak_avoidance() -> void:
	var audio_manager = AudioManagerClass.new()
	
	var bpm_out = 120.0
	var beat_duration = 60.0 / bpm_out # 0.5 seconds per beat
	
	var downbeats_out = [0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0]
	var downbeats_in = [0.0, 2.0, 4.0]
	
	var pos_out = 5.0
	var cue_in = 0.0
	var outro_start = 12.0
	
	# Scenario 1: Initial candidate is 16.0.
	# We place transients at 16.0 and 14.0 (consecutive bar boundaries).
	# With pos_out = 5.0, it should first try to shift to 14.0 (collision),
	# then successfully shift to 12.0 (no collision).
	var transients_out_1 = [16.0, 14.0]
	var transients_in_1 = []
	
	var trigger_time_1 = audio_manager.get_aligned_trigger_time(pos_out, bpm_out, downbeats_out, cue_in, downbeats_in, outro_start, transients_out_1, transients_in_1)
	assert_eq(trigger_time_1, 12.0, "Trigger time should shift twice to 12.0 to avoid transients at 16.0 and 14.0")
	
	# Scenario 2: Initial candidate is 16.0.
	# Transients at 16.0 and 14.0. pos_out = 13.0.
	# First shift to 14.0 is valid, but 14.0 is a collision.
	# Second shift would go to 12.0, which is below pos_out (13.0).
	# So it must reset and shift later to 18.0.
	var pos_out_2 = 13.0
	var trigger_time_2 = audio_manager.get_aligned_trigger_time(pos_out_2, bpm_out, downbeats_out, cue_in, downbeats_in, outro_start, transients_out_1, transients_in_1)
	assert_eq(trigger_time_2, 18.0, "Trigger time should reset and shift later to 18.0 when shifting earlier goes below pos_out")
	
	# Scenario 3: Initial candidate is 16.0.
	# Transients at 16.0, 14.0, 18.0. pos_out = 13.0.
	# Shift earlier to 14.0 (collision).
	# Can't shift earlier to 12.0 (below pos_out).
	# Shift later to 18.0 (collision).
	# Shift later to 20.0 (no collision).
	var transients_out_3 = [16.0, 14.0, 18.0]
	var trigger_time_3 = audio_manager.get_aligned_trigger_time(pos_out_2, bpm_out, downbeats_out, cue_in, downbeats_in, outro_start, transients_out_3, transients_in_1)
	assert_eq(trigger_time_3, 20.0, "Trigger time should shift to 20.0 after checking 16.0, 14.0, and 18.0")

	# Scenario 4: Max 4 attempts.
	# Transients at 16.0, 14.0, 18.0, 20.0. pos_out = 13.0.
	# Check 16.0 (collision) -> check 14.0 (collision) -> check 18.0 (collision) -> check 20.0 (collision) -> max attempts reached, returns last computed (22.0).
	var transients_out_4 = [16.0, 14.0, 18.0, 20.0]
	var trigger_time_4 = audio_manager.get_aligned_trigger_time(pos_out_2, bpm_out, downbeats_out, cue_in, downbeats_in, outro_start, transients_out_4, transients_in_1)
	assert_eq(trigger_time_4, 22.0, "Trigger time should fall back to last computed (22.0) when all 4 attempts find collisions")
	
	audio_manager.free()

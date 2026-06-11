extends GutTest

const DJPathfinder = preload("res://scripts/services/dj_pathfinder.gd")

func test_camelot_key_parsing() -> void:
	var key_8a = DJPathfinder.CamelotKey.new("8A")
	assert_true(key_8a.valid, "8A should be a valid key")
	assert_eq(key_8a.number, 8)
	assert_eq(key_8a.mode, "A")
	
	var key_12b = DJPathfinder.CamelotKey.new("12b")
	assert_true(key_12b.valid, "12b should be parsed as valid and normalized to uppercase")
	assert_eq(key_12b.number, 12)
	assert_eq(key_12b.mode, "B")
	
	var key_invalid = DJPathfinder.CamelotKey.new("13A")
	assert_false(key_invalid.valid, "13A should be invalid")
	
	var key_empty = DJPathfinder.CamelotKey.new("")
	assert_false(key_empty.valid, "Empty string key should be invalid")

func test_camelot_distance() -> void:
	# Identical keys
	assert_eq(DJPathfinder.get_key_distance("8A", "8A"), 0.0, "Identical keys should have 0 distance")
	
	# Same number, opposite letter (compatible transition)
	assert_eq(DJPathfinder.get_key_distance("8A", "8B"), 1.0, "Same number opposite mode should have distance 1")
	
	# Adjacent number, same letter (compatible transition)
	assert_eq(DJPathfinder.get_key_distance("8A", "9A"), 2.0, "Adjacent number same mode should have distance 2")
	assert_eq(DJPathfinder.get_key_distance("12A", "1A"), 2.0, "Wheel wrapping: 12A to 1A should have distance 2")
	
	# Non-adjacent keys
	assert_eq(DJPathfinder.get_key_distance("8A", "10A"), 4.0, "Difference of 2 steps same mode should have distance 4")
	assert_eq(DJPathfinder.get_key_distance("8A", "2A"), 12.0, "Difference of 6 steps (maximum) same mode should have distance 12")
	
	# Invalid keys should return default penalty
	assert_eq(DJPathfinder.get_key_distance("8A", "invalid"), 3.0, "Invalid key should return default penalty")

func test_transition_cost() -> void:
	var track_a = {"bpm": 120.0, "musical_key": "8A", "energy_level": 0.5}
	var track_b = {"bpm": 122.0, "musical_key": "8B", "energy_level": 0.6}
	var track_c = {"bpm": 140.0, "musical_key": "2A", "energy_level": 0.9}
	
	var cost_ab = DJPathfinder.calculate_transition_cost(track_a, track_b)
	var cost_ac = DJPathfinder.calculate_transition_cost(track_a, track_c)
	
	assert_true(cost_ab < cost_ac, "A compatible transition should have lower cost than an incompatible transition")

func test_mood_path_generator() -> void:
	var tracks_meta = {
		"song_a.mp3": {"bpm": 120.0, "musical_key": "8A", "energy_level": 0.5},
		"song_b.mp3": {"bpm": 121.0, "musical_key": "9A", "energy_level": 0.55},
		"song_c.mp3": {"bpm": 122.0, "musical_key": "9B", "energy_level": 0.6},
		"song_d.mp3": {"bpm": 128.0, "musical_key": "3A", "energy_level": 0.8}
	}
	
	# Start with song_a.mp3
	var path = DJPathfinder.generate_mood_path(tracks_meta, "song_a.mp3")
	
	assert_eq(path.size(), 4, "Path should contain all 4 songs")
	assert_eq(path[0], "song_a.mp3", "Path should start with song_a.mp3")
	assert_eq(path[1], "song_b.mp3", "Next song should be song_b.mp3 (BPM 121, Key 9A is compatible with 8A)")
	assert_eq(path[2], "song_c.mp3", "Next song should be song_c.mp3 (BPM 122, Key 9B is compatible with 9A)")
	assert_eq(path[3], "song_d.mp3", "Final song should be song_d.mp3 (less compatible, placed last)")

class DummyMetadataService extends Node:
	var cache = {}
	func get_cached_metadata(href: String) -> Dictionary:
		return cache.get(href, {})

func test_smart_matches() -> void:
	var dummy_ms = DummyMetadataService.new()
	dummy_ms.cache = {
		"song_a.mp3": {"bpm": 120.0, "musical_key": "8A", "energy_level": 0.5, "genre": "Rock"},
		# Perfect Match (close BPM/energy, identical key, same genre)
		"song_b.mp3": {"bpm": 121.0, "musical_key": "8A", "energy_level": 0.52, "genre": "Rock"},
		# Interesting Match (same genre, changes BPM by 14 and energy by 0.22)
		"song_c.mp3": {"bpm": 134.0, "musical_key": "8B", "energy_level": 0.72, "genre": "Rock"},
		# Creative Match (different genre, matches BPM and energy closely but different key)
		"song_d.mp3": {"bpm": 120.5, "musical_key": "9A", "energy_level": 0.51, "genre": "Electronic"},
	}
	
	var playlist = ["song_a.mp3", "song_b.mp3", "song_c.mp3", "song_d.mp3"]
	var matches = DJPathfinder.calculate_smart_matches("song_a.mp3", playlist, dummy_ms)

	
	assert_eq(matches.get("perfect", {}).get("href", ""), "song_b.mp3", "Perfect match should be song_b.mp3")
	assert_eq(matches.get("interesting", {}).get("href", ""), "song_c.mp3", "Interesting match should be song_c.mp3")
	assert_eq(matches.get("creative", {}).get("href", ""), "song_d.mp3", "Creative match should be song_d.mp3")
	
	dummy_ms.free()

func test_harmonic_relation_costs() -> void:
	assert_eq(DJPathfinder.get_harmonic_relation_cost("10A", "10A"), 0.0, "Exact match should be 0.0")
	assert_eq(DJPathfinder.get_harmonic_relation_cost("10A", "10B"), 1.0, "Mode shift should be 1.0")
	assert_eq(DJPathfinder.get_harmonic_relation_cost("10A", "9A"), 1.5, "Harmonic step should be 1.5")
	assert_eq(DJPathfinder.get_harmonic_relation_cost("10A", "9B"), 2.0, "Diagonal step should be 2.0")
	assert_eq(DJPathfinder.get_harmonic_relation_cost("10A", "12A"), 2.5, "Energy boost (+2 steps) should be 2.5")
	assert_eq(DJPathfinder.get_harmonic_relation_cost("10A", "8A"), 3.0, "Energy drop (-2 steps) should be 3.0")
	assert_eq(DJPathfinder.get_harmonic_relation_cost("10A", "5A"), 3.0, "Power Fifth Mix (+7 steps) should be 3.0")
	assert_eq(DJPathfinder.get_harmonic_relation_cost("10A", "3A"), 3.0, "Subdominant Mix (+5 steps) should be 3.0")
	assert_eq(DJPathfinder.get_harmonic_relation_cost("10A", "2A"), 8.0, "Incompatible keys should be 8.0")


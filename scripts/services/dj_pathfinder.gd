extends Object
class_name DJPathfinder

# Weights for transition cost calculation
const WEIGHT_KEY = 2.5
const WEIGHT_BPM = 0.15
const WEIGHT_ENERGY = 6.0

## Helper structure to represent parsed key
class CamelotKey:
	var number: int = 0
	var mode: String = ""
	var valid: bool = false

	func _init(key_str: String) -> void:
		var clean = key_str.strip_edges()
		var regex = RegEx.new()
		regex.compile("^(\\d+)([ABab])$")
		var match_obj = regex.search(clean)
		if match_obj:
			number = match_obj.get_string(1).to_int()
			mode = match_obj.get_string(2).to_upper()
			if number >= 1 and number <= 12 and (mode == "A" or mode == "B"):
				valid = true

## Calculates Camelot Wheel distance between two key strings
static func get_key_distance(key_a_str: String, key_b_str: String) -> float:
	var key_a = CamelotKey.new(key_a_str)
	var key_b = CamelotKey.new(key_b_str)
	
	if not key_a.valid or not key_b.valid:
		return 3.0 # Default penalty cost for missing/invalid keys
		
	if key_a.number == key_b.number and key_a.mode == key_b.mode:
		return 0.0
		
	var diff = abs(key_a.number - key_b.number)
	var step_diff = min(diff, 12 - diff)
	
	if key_a.mode == key_b.mode:
		return float(step_diff * 2)
	else:
		return float(step_diff * 2 + 1)

## Calculates the transition cost between two track metadata dictionaries
static func calculate_transition_cost(track_a: Dictionary, track_b: Dictionary) -> float:
	var key_a = track_a.get("musical_key", "")
	var key_b = track_b.get("musical_key", "")
	var key_dist = get_key_distance(key_a, key_b)
	
	var bpm_a = track_a.get("bpm", 120.0)
	var bpm_b = track_b.get("bpm", 120.0)
	var bpm_diff = absf(bpm_a - bpm_b)
	
	var energy_a = track_a.get("energy_level", 0.5)
	var energy_b = track_b.get("energy_level", 0.5)
	var energy_diff = absf(energy_a - energy_b)
	
	return (key_dist * WEIGHT_KEY) + (bpm_diff * WEIGHT_BPM) + (energy_diff * WEIGHT_ENERGY)

## Solves the optimal playlist order starting from a specific track
## and returning a list of sorted track hrefs.
static func generate_mood_path(tracks_meta: Dictionary, start_href: String = "") -> Array[String]:
	var result: Array[String] = []
	var unvisited: Array[String] = []
	
	for href in tracks_meta.keys():
		unvisited.append(href)
		
	if unvisited.is_empty():
		return result
		
	# Select starting track
	var current_href = start_href
	if current_href.is_empty() or not unvisited.has(current_href):
		# Default to first track or random starting track
		current_href = unvisited[0]
		
	unvisited.erase(current_href)
	result.append(current_href)
	
	# Greedy search for next closest track
	while not unvisited.is_empty():
		var current_meta = tracks_meta.get(current_href, {})
		var best_next_href = unvisited[0]
		var best_cost = calculate_transition_cost(current_meta, tracks_meta.get(best_next_href, {}))
		
		for i in range(1, unvisited.size()):
			var next_href = unvisited[i]
			var cost = calculate_transition_cost(current_meta, tracks_meta.get(next_href, {}))
			if cost < best_cost:
				best_cost = cost
				best_next_href = next_href
				
		current_href = best_next_href
		unvisited.erase(current_href)
		result.append(current_href)
		
	return result

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

## Calculates Camelot Wheel harmonic relation cost between two key strings
static func get_harmonic_relation_cost(key_a_str: String, key_b_str: String) -> float:
	var key_a = CamelotKey.new(key_a_str)
	var key_b = CamelotKey.new(key_b_str)
	
	if not key_a.valid or not key_b.valid:
		return 3.0 # Default penalty cost for missing/invalid keys
		
	if key_a.number == key_b.number and key_a.mode == key_b.mode:
		return 0.0 # Exact Match
		
	var diff = abs(key_a.number - key_b.number)
	var step_diff = min(diff, 12 - diff)
	
	var same_mode = (key_a.mode == key_b.mode)
	
	if same_mode:
		if step_diff == 1:
			return 1.5 # Harmonic Step (adjacent number)
		elif step_diff == 2:
			# Check if it is an Energy Boost (+2 steps) or Energy Drop (-2 steps)
			if (key_a.number + 2 - 1) % 12 + 1 == key_b.number:
				return 2.5 # Energy Boost (+2 steps)
			else:
				return 3.0 # Energy Drop (-2 steps)
		elif step_diff == 5:
			# Power Mix (+7 steps) / Subdominant (+5 steps)
			if (key_a.number + 7 - 1) % 12 + 1 == key_b.number:
				return 3.0 # Power Fifth Mix (+7)
			elif (key_a.number + 5 - 1) % 12 + 1 == key_b.number:
				return 3.0 # Subdominant Mix (+5)
			else:
				return 3.0
		else:
			return 8.0 # Incompatible key clash
	else:
		# Different modes
		if step_diff == 0:
			return 1.0 # Mode Shift (same number, opposite mode)
		elif step_diff == 1:
			return 2.0 # Diagonal Step (e.g. 10A -> 9B)
		else:
			return 8.0 # Incompatible key clash

## Calculates the transition cost between two track metadata dictionaries
static func calculate_transition_cost(track_a: Dictionary, track_b: Dictionary) -> float:
	var key_a = track_a.get("musical_key", "")
	var key_b = track_b.get("musical_key", "")
	var key_cost = get_harmonic_relation_cost(key_a, key_b)
	
	var bpm_a = track_a.get("bpm", 120.0)
	var bpm_b = track_b.get("bpm", 120.0)
	var bpm_diff = absf(bpm_a - bpm_b)
	
	var energy_a = track_a.get("energy_level", 0.5)
	var energy_b = track_b.get("energy_level", 0.5)
	var energy_diff = absf(energy_a - energy_b)
	
	return (key_cost * WEIGHT_KEY) + (bpm_diff * WEIGHT_BPM) + (energy_diff * WEIGHT_ENERGY)

## Helper to check if two genres are similar
static func is_similar_genre(genre_a: String, genre_b: String) -> bool:
	var clean_a = genre_a.strip_edges().to_lower()
	var clean_b = genre_b.strip_edges().to_lower()
	if clean_a.is_empty() or clean_b.is_empty() or clean_a == "unknown" or clean_b == "unknown":
		return false
	return clean_a == clean_b or clean_a.contains(clean_b) or clean_b.contains(clean_a)

## Calculates three distinct candidates:
## - perfect: lowest overall transition cost
## - interesting: similar genre, different BPM (target 15 BPM diff) and energy (target 0.25 diff)
## - creative: different genre, closest BPM & energy (standard cost)
static func calculate_smart_matches(current_href: String, playlist: Array, metadata_service: Node) -> Dictionary:
	var results := {}
	
	if playlist.size() <= 1:
		return results
		
	var current_meta = {}
	if is_instance_valid(metadata_service):
		current_meta = metadata_service.get_cached_metadata(current_href)
	if current_meta.is_empty() or current_meta.get("bpm", 0.0) <= 0.0:
		return results
		
	var current_genre = current_meta.get("genre", "Unknown")
	
	# Extract all valid candidate tracks
	var candidates := []
	for href in playlist:
		if href == current_href:
			continue
		var meta = {}
		if is_instance_valid(metadata_service):
			meta = metadata_service.get_cached_metadata(href)
		if meta.is_empty() or meta.get("bpm", 0.0) <= 0.0:
			continue
		candidates.append({"href": href, "meta": meta})
		
	if candidates.is_empty():
		return results
		
	# 1. Perfect Match (restricted to harmonically compatible keys: cost <= 2.0)
	var perfect_candidates = []
	for c in candidates:
		var key_cost = get_harmonic_relation_cost(current_meta.get("musical_key", ""), c.meta.get("musical_key", ""))
		if key_cost <= 2.0:
			perfect_candidates.append(c)
			
	if perfect_candidates.is_empty():
		perfect_candidates = candidates.duplicate()
		
	perfect_candidates.sort_custom(func(a, b):
		var cost_a = calculate_transition_cost(current_meta, a.meta)
		var cost_b = calculate_transition_cost(current_meta, b.meta)
		return cost_a < cost_b
	)
	var perfect_match = perfect_candidates[0]
	perfect_match["cost"] = calculate_transition_cost(current_meta, perfect_match.meta)
	results["perfect"] = perfect_match
	
	# Remove perfect match from candidates for the next choices if we have other options
	var remaining_candidates = []
	for c in candidates:
		if c.href != perfect_match.href:
			remaining_candidates.append(c)
			
	if remaining_candidates.is_empty():
		remaining_candidates = candidates.duplicate()
		
	# 2. Interesting Match (same genre, target 15 BPM change, 0.25 energy change, key modulation cost >= 2.0 and <= 3.0)
	var interesting_candidates = []
	for c in remaining_candidates:
		if is_similar_genre(current_genre, c.meta.get("genre", "Unknown")):
			var key_cost = get_harmonic_relation_cost(current_meta.get("musical_key", ""), c.meta.get("musical_key", ""))
			if key_cost >= 2.0 and key_cost <= 3.0:
				interesting_candidates.append(c)
				
	if interesting_candidates.is_empty():
		for c in remaining_candidates:
			if is_similar_genre(current_genre, c.meta.get("genre", "Unknown")):
				interesting_candidates.append(c)
				
	if interesting_candidates.is_empty():
		interesting_candidates = remaining_candidates.duplicate()
		
	var get_interesting_cost = func(cand_meta: Dictionary) -> float:
		var key_cost = get_harmonic_relation_cost(current_meta.get("musical_key", ""), cand_meta.get("musical_key", ""))
		var bpm_diff = absf(current_meta.get("bpm", 120.0) - cand_meta.get("bpm", 120.0))
		var energy_diff = absf(current_meta.get("energy_level", 0.5) - cand_meta.get("energy_level", 0.5))
		return (key_cost * WEIGHT_KEY) + (absf(15.0 - bpm_diff) * WEIGHT_BPM) + (absf(0.25 - energy_diff) * WEIGHT_ENERGY)
		
	interesting_candidates.sort_custom(func(a, b):
		return get_interesting_cost.call(a.meta) < get_interesting_cost.call(b.meta)
	)
	var interesting_match = interesting_candidates[0]
	interesting_match["cost"] = get_interesting_cost.call(interesting_match.meta)
	results["interesting"] = interesting_match
	
	# Remove interesting match from remaining candidates for Creative match
	var final_candidates = []
	for c in remaining_candidates:
		if c.href != interesting_match.href:
			final_candidates.append(c)
			
	if final_candidates.is_empty():
		final_candidates = remaining_candidates.duplicate()
		
	# 3. Creative Match (different genre, retain BPM/energy, ignore key distance cost component)
	var creative_candidates = []
	for c in final_candidates:
		if not is_similar_genre(current_genre, c.meta.get("genre", "Unknown")):
			creative_candidates.append(c)
			
	if creative_candidates.is_empty():
		creative_candidates = final_candidates.duplicate()
		
	var get_creative_cost = func(cand_meta: Dictionary) -> float:
		var bpm_diff = absf(current_meta.get("bpm", 120.0) - cand_meta.get("bpm", 120.0))
		var energy_diff = absf(current_meta.get("energy_level", 0.5) - cand_meta.get("energy_level", 0.5))
		return (bpm_diff * WEIGHT_BPM) + (energy_diff * WEIGHT_ENERGY)
		
	creative_candidates.sort_custom(func(a, b):
		return get_creative_cost.call(a.meta) < get_creative_cost.call(b.meta)
	)
	var creative_match = creative_candidates[0]
	creative_match["cost"] = get_creative_cost.call(creative_match.meta)
	results["creative"] = creative_match
	
	return results


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

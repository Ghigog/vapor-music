extends Object
class_name DJPathfinder

# Weights for transition cost calculation
const WEIGHT_KEY = 2.5
const WEIGHT_BPM = 0.15
const WEIGHT_ENERGY = 6.0
const WEIGHT_GENRE = 3.0
const WEIGHT_CURVE_ENERGY = 8.0
const WEIGHT_CURVE_BPM = 0.3

class AStarState:
	var path: Array[String]
	var g_score: float
	var h_score: float
	var f_score: float
	
	func _init(p: Array[String], g: float, h: float) -> void:
		path = p
		g_score = g
		h_score = h
		f_score = g + h

static var _taxonomy_graph: Dictionary = {}
static var _taxonomy_initialized := false

static func init_taxonomy() -> void:
	if _taxonomy_initialized:
		return
		
	var path = "res://assets/genre_taxonomy.json"
	var json_data = null
	if FileAccess.file_exists(path):
		var file = FileAccess.open(path, FileAccess.READ)
		if file:
			var content = file.get_as_text()
			file.close()
			var json = JSON.new()
			if json.parse(content) == OK:
				json_data = json.data
				
	# Fallback if file load failed or empty
	if not json_data:
		json_data = {
			"Club Music": {
				"House": {
					"Tech House": {},
					"Deep House": {},
					"Progressive House": {},
					"Minimal House": {}
				},
				"Techno": {
					"Acid Techno": {},
					"Minimal Techno": {}
				}
			},
			"Bass Music": {
				"Drum & Bass": {
					"Liquid DNB": {},
					"Neurofunk": {},
					"Jungle": {},
					"Drum and Bass": {}
				}
			},
			"Rock": {
				"Alternative Rock": {},
				"Classic Rock": {},
				"Hard Rock": {}
			}
		}
		
	_build_graph_from_tree(json_data, "")
	_taxonomy_initialized = true

static func _build_graph_from_tree(tree: Dictionary, parent: String) -> void:
	for key in tree.keys():
		var node = key.strip_edges()
		var node_std = node.to_lower()
		
		if not _taxonomy_graph.has(node_std):
			_taxonomy_graph[node_std] = {
				"original_name": key,
				"neighbors": []
			}
			
		if parent != "":
			var parent_std = parent.to_lower()
			if not _taxonomy_graph[node_std]["neighbors"].has(parent_std):
				_taxonomy_graph[node_std]["neighbors"].append(parent_std)
			if _taxonomy_graph.has(parent_std) and not _taxonomy_graph[parent_std]["neighbors"].has(node_std):
				_taxonomy_graph[parent_std]["neighbors"].append(node_std)
				
		var child_tree = tree[key]
		if child_tree is Dictionary:
			_build_graph_from_tree(child_tree, node)

static func get_genre_distance(genre_a: String, genre_b: String) -> float:
	init_taxonomy()
	
	var clean_a = genre_a.strip_edges().to_lower()
	var clean_b = genre_b.strip_edges().to_lower()
	
	if clean_a.is_empty() or clean_b.is_empty() or clean_a == "unknown" or clean_b == "unknown":
		return 5.0 # Max default penalty for unknown genres
		
	if clean_a == clean_b:
		return 0.0
		
	# Try exact match or partial match in graph
	var node_a = _find_graph_node(clean_a)
	var node_b = _find_graph_node(clean_b)
	
	if node_a.is_empty() or node_b.is_empty():
		return 5.0 # High penalty if not in tree
		
	if node_a == node_b:
		return 0.0
		
	# BFS to find shortest path
	var queue := []
	var visited := {}
	
	queue.append({"node": node_a, "dist": 0})
	visited[node_a] = true
	
	while not queue.is_empty():
		var curr = queue.pop_front()
		var curr_node = curr["node"]
		var curr_dist = curr["dist"]
		
		if curr_node == node_b:
			return float(curr_dist)
			
		var neighbors = _taxonomy_graph[curr_node]["neighbors"]
		for n in neighbors:
			if not visited.has(n):
				visited[n] = true
				queue.append({"node": n, "dist": curr_dist + 1})
				
	return 5.0

static func _find_graph_node(genre: String) -> String:
	if _taxonomy_graph.has(genre):
		return genre
		
	# Try partial match
	for g in _taxonomy_graph.keys():
		if g.contains(genre) or genre.contains(g):
			return g
			
	return ""

static var _history_cache: Array = []
static var _history_loaded := false

static func load_history_cache() -> void:
	_history_loaded = true
	var path = "user://transition_history.json"
	if not FileAccess.file_exists(path):
		_history_cache = []
		return
	var file = FileAccess.open(path, FileAccess.READ)
	if not file:
		_history_cache = []
		return
	var content = file.get_as_text()
	file.close()
	var json = JSON.new()
	if json.parse(content) == OK:
		if json.data is Array:
			_history_cache = json.data
			return
	_history_cache = []

static func reload_history_cache() -> void:
	_history_loaded = false
	load_history_cache()

static func get_skip_penalty(href_a: String, href_b: String) -> float:
	if not _history_loaded:
		load_history_cache()
	if href_a.is_empty() or href_b.is_empty():
		return 0.0
	var penalty := 0.0
	for entry in _history_cache:
		if entry is Dictionary and entry.get("outcome", "") == "skipped":
			if entry.get("track_a", "") == href_a and entry.get("track_b", "") == href_b:
				penalty += 15.0
	return penalty

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
	var key_a = track_a.get("outro_key", "")
	if key_a.is_empty():
		key_a = track_a.get("segment_keys", {}).get("outro", "")
	if key_a.is_empty():
		key_a = track_a.get("musical_key", "")
		
	var key_b = track_b.get("intro_key", "")
	if key_b.is_empty():
		key_b = track_b.get("segment_keys", {}).get("intro", "")
	if key_b.is_empty():
		key_b = track_b.get("musical_key", "")
		
	var key_cost = get_harmonic_relation_cost(key_a, key_b)
	
	var bpm_a = track_a.get("bpm", 120.0)
	var bpm_b = track_b.get("bpm", 120.0)
	var bpm_diff = absf(bpm_a - bpm_b)
	
	var energy_a = track_a.get("energy_level", 0.5)
	var energy_b = track_b.get("energy_level", 0.5)
	var energy_diff = absf(energy_a - energy_b)
	
	var energy_threshold = 0.5
	var main_loop = Engine.get_main_loop()
	if main_loop is SceneTree:
		var audio_mgr = main_loop.root.get_node_or_null("AudioManager")
		if audio_mgr and "energy_threshold" in audio_mgr:
			energy_threshold = audio_mgr.energy_threshold
	var extra_energy_penalty := 0.0
	if energy_diff > energy_threshold:
		extra_energy_penalty = 100.0 * (energy_diff - energy_threshold)
	
	var genre_a = track_a.get("genre", "Unknown")
	var genre_b = track_b.get("genre", "Unknown")
	var genre_cost = get_genre_distance(genre_a, genre_b)
	
	var skip_penalty_val = get_skip_penalty(track_a.get("href", ""), track_b.get("href", ""))
	
	return (key_cost * WEIGHT_KEY) + (bpm_diff * WEIGHT_BPM) + (energy_diff * WEIGHT_ENERGY) + (genre_cost * WEIGHT_GENRE) + skip_penalty_val + extra_energy_penalty

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
		var key_a = current_meta.get("outro_key", "")
		if key_a.is_empty():
			key_a = current_meta.get("segment_keys", {}).get("outro", "")
		if key_a.is_empty():
			key_a = current_meta.get("musical_key", "")
			
		var key_b = c.meta.get("intro_key", "")
		if key_b.is_empty():
			key_b = c.meta.get("segment_keys", {}).get("intro", "")
		if key_b.is_empty():
			key_b = c.meta.get("musical_key", "")
			
		var key_cost = get_harmonic_relation_cost(key_a, key_b)
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
			var key_a = current_meta.get("outro_key", "")
			if key_a.is_empty():
				key_a = current_meta.get("segment_keys", {}).get("outro", "")
			if key_a.is_empty():
				key_a = current_meta.get("musical_key", "")
				
			var key_b = c.meta.get("intro_key", "")
			if key_b.is_empty():
				key_b = c.meta.get("segment_keys", {}).get("intro", "")
			if key_b.is_empty():
				key_b = c.meta.get("musical_key", "")
				
			var key_cost = get_harmonic_relation_cost(key_a, key_b)
			if key_cost >= 2.0 and key_cost <= 3.0:
				interesting_candidates.append(c)
				
	if interesting_candidates.is_empty():
		for c in remaining_candidates:
			if is_similar_genre(current_genre, c.meta.get("genre", "Unknown")):
				interesting_candidates.append(c)
				
	if interesting_candidates.is_empty():
		interesting_candidates = remaining_candidates.duplicate()
		
	var get_interesting_cost = func(cand_meta: Dictionary) -> float:
		var key_a = current_meta.get("outro_key", "")
		if key_a.is_empty():
			key_a = current_meta.get("segment_keys", {}).get("outro", "")
		if key_a.is_empty():
			key_a = current_meta.get("musical_key", "")
			
		var key_b = cand_meta.get("intro_key", "")
		if key_b.is_empty():
			key_b = cand_meta.get("segment_keys", {}).get("intro", "")
		if key_b.is_empty():
			key_b = cand_meta.get("musical_key", "")
			
		var key_cost = get_harmonic_relation_cost(key_a, key_b)
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


# Binary search insertion to keep open_list sorted by f_score (ascending)
static func _insert_state(open_list: Array, state: AStarState) -> void:
	var low = 0
	var high = open_list.size() - 1
	var insert_idx = open_list.size()
	
	while low <= high:
		var mid = (low + high) / 2
		if open_list[mid].f_score > state.f_score:
			insert_idx = mid
			high = mid - 1
		else:
			low = mid + 1
			
	open_list.insert(insert_idx, state)

static func get_target_energy(curve: String, start_energy: float, index: int, total_steps: int) -> float:
	if total_steps <= 1:
		return start_energy
	var t = float(index) / float(total_steps - 1)
	match curve.to_lower():
		"build", "build vibe", "build_vibe":
			return lerpf(start_energy, minf(start_energy + 0.4, 1.0), t)
		"chill", "chill down", "chill_down":
			return lerpf(start_energy, maxf(start_energy - 0.4, 0.0), t)
		"wave":
			return clampf(start_energy + 0.3 * sin(t * PI * 2.0), 0.0, 1.0)
		_:
			return start_energy

static func get_target_bpm(curve: String, start_bpm: float, index: int, total_steps: int) -> float:
	if total_steps <= 1:
		return start_bpm
	var t = float(index) / float(total_steps - 1)
	match curve.to_lower():
		"build", "build vibe", "build_vibe":
			return lerpf(start_bpm, start_bpm + 15.0, t)
		"chill", "chill down", "chill_down":
			return lerpf(start_bpm, start_bpm - 15.0, t)
		"wave":
			return start_bpm + 10.0 * sin(t * PI * 2.0)
		_:
			return start_bpm

static func get_heuristic(current_path_size: int, total_steps: int, current_energy: float, target_end_energy: float, current_bpm: float, target_end_bpm: float) -> float:
	var steps_left = total_steps - current_path_size
	if steps_left <= 0:
		return 0.0
	var energy_diff = absf(current_energy - target_end_energy)
	var bpm_diff = absf(current_bpm - target_end_bpm)
	return (energy_diff * WEIGHT_CURVE_ENERGY + bpm_diff * WEIGHT_CURVE_BPM) * (float(steps_left) / float(total_steps))

static func calculate_transition_duration(meta_out: Dictionary, meta_in: Dictionary) -> float:
	var outro = meta_out.get("segments", {}).get("outro", [])
	var intro = meta_in.get("segments", {}).get("intro", [])
	
	var outro_len := 0.0
	if outro.size() >= 2:
		outro_len = outro[1] - outro[0]
		
	var intro_len := 0.0
	if intro.size() >= 2:
		intro_len = intro[1] - intro[0]
		
	if outro_len > 0.0 and intro_len > 0.0:
		return clampf(minf(outro_len, intro_len), 3.0, 15.0)
	elif outro_len > 0.0:
		return clampf(outro_len, 3.0, 15.0)
	elif intro_len > 0.0:
		return clampf(intro_len, 3.0, 15.0)
		
	return 5.0

## Solves the optimal playlist order starting from a specific track
## and returning a list of sorted track hrefs.
static func generate_mood_path(tracks_meta: Dictionary, start_href: String = "", target_curve: String = "build") -> Array[String]:
	reload_history_cache()
	if tracks_meta.is_empty():
		return []
		
	# Inject href keys into metadata copies so skip penalty can be evaluated
	var enriched_meta: Dictionary = {}
	for href in tracks_meta:
		var meta_copy = tracks_meta[href].duplicate()
		meta_copy["href"] = href
		enriched_meta[href] = meta_copy
	tracks_meta = enriched_meta
	
	var keys = tracks_meta.keys()
	var start_track = start_href
	if start_track.is_empty() or not tracks_meta.has(start_track):
		start_track = keys[0]
		
	var N = tracks_meta.size()
	var M = min(10, N)
	
	var start_meta = tracks_meta[start_track]
	var start_energy = start_meta.get("energy_level", 0.5)
	var start_bpm = start_meta.get("bpm", 120.0)
	
	var open_list: Array[AStarState] = []
	var start_state = AStarState.new([start_track], 0.0, 0.0)
	open_list.append(start_state)
	
	var best_state: AStarState = start_state
	var expansions := 0
	const MAX_EXPANSIONS = 1500
	
	while not open_list.is_empty():
		expansions += 1
		if expansions > MAX_EXPANSIONS:
			break
			
		var curr_state = open_list.pop_front()
		
		if curr_state.path.size() > best_state.path.size():
			best_state = curr_state
		elif curr_state.path.size() == best_state.path.size() and curr_state.g_score < best_state.g_score:
			best_state = curr_state
			
		if curr_state.path.size() == M:
			best_state = curr_state
			break
			
		var last_href = curr_state.path[-1]
		var last_meta = tracks_meta[last_href]
		
		# Gather unvisited tracks
		var unvisited_candidates := []
		for href in keys:
			if not curr_state.path.has(href):
				unvisited_candidates.append(href)
				
		var next_idx = curr_state.path.size()
		var target_energy = get_target_energy(target_curve, start_energy, next_idx, M)
		var target_bpm = get_target_bpm(target_curve, start_bpm, next_idx, M)
		
		var candidate_scores := []
		for href in unvisited_candidates:
			var cand_meta = tracks_meta[href]
			var t_cost = calculate_transition_cost(last_meta, cand_meta)
			
			var cand_energy = cand_meta.get("energy_level", 0.5)
			var cand_bpm = cand_meta.get("bpm", 120.0)
			
			var e_dev = absf(cand_energy - target_energy)
			var b_dev = absf(cand_bpm - target_bpm)
			var curve_cost = (e_dev * WEIGHT_CURVE_ENERGY) + (b_dev * WEIGHT_CURVE_BPM)
			
			var total_step_cost = t_cost + curve_cost
			candidate_scores.append({"href": href, "cost": total_step_cost, "meta": cand_meta})
			
		candidate_scores.sort_custom(func(a, b): return a.cost < b.cost)
		
		var K = min(6, candidate_scores.size())
		for j in range(K):
			var cand = candidate_scores[j]
			var next_href = cand.href
			var new_path = curr_state.path.duplicate()
			new_path.append(next_href)
			
			var new_g = curr_state.g_score + cand.cost
			
			var end_target_energy = get_target_energy(target_curve, start_energy, M - 1, M)
			var end_target_bpm = get_target_bpm(target_curve, start_bpm, M - 1, M)
			
			var new_h = get_heuristic(
				new_path.size(), 
				M, 
				cand.meta.get("energy_level", 0.5), 
				end_target_energy, 
				cand.meta.get("bpm", 120.0), 
				end_target_bpm
			)
			
			var next_state = AStarState.new(new_path, new_g, new_h)
			_insert_state(open_list, next_state)
			
	var path = best_state.path.duplicate()
	var remaining_unvisited := []
	for href in keys:
		if not path.has(href):
			remaining_unvisited.append(href)
			
	# Greedy append of remaining tracks
	var current_href = path[-1]
	while not remaining_unvisited.is_empty():
		var current_meta = tracks_meta[current_href]
		var best_next_href = remaining_unvisited[0]
		var best_cost = calculate_transition_cost(current_meta, tracks_meta[best_next_href])
		
		for i in range(1, remaining_unvisited.size()):
			var next_href = remaining_unvisited[i]
			var cost = calculate_transition_cost(current_meta, tracks_meta[next_href])
			if cost < best_cost:
				best_cost = cost
				best_next_href = next_href
				
		current_href = best_next_href
		remaining_unvisited.erase(current_href)
		path.append(current_href)
		
	return path

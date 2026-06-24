## audio_manager.gd
## Global engine player that streams audio buffers dynamically via authenticated HTTP requests.
## Manages a dual-deck playback system with real-time transition effects (EQ swaps, filter sweeps, crossfades).
extends Node

signal track_changed(track_name: String)
signal playback_toggled(is_playing: bool)
signal loading_track(is_loading: bool)
signal transition_started(next_track: String, transition_type: String)
signal transition_completed(track: String)
signal length_changed(length: float)
signal smart_mixing_toggled(enabled: bool)

var player: AudioStreamPlayer # Compatibility reference, points to active_player
var player_a: AudioStreamPlayer
var player_b: AudioStreamPlayer
var active_player: AudioStreamPlayer

var current_playlist: Array[String] = []:
	set(val):
		current_playlist = val
		_update_prefetching()
var current_track_index: int = -1:
	set(val):
		if current_track_index != val:
			current_track_index = val
			_update_prefetching()
var current_track_length: float
var is_playing := false
var is_transitioning := false:
	set(val):
		if is_transitioning != val:
			is_transitioning = val
			var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
			if is_instance_valid(audio_analyzer) and "is_transitioning_active" in audio_analyzer:
				audio_analyzer.is_transitioning_active = val
var active_tween: Tween
var _pitch_ramp_tween: Tween
var transition_duration := 0.0:
	set(val):
		transition_duration = val
		if is_transitioning and _active_transition_duration > 0.0 and val > 0.1:
			var ratio = _transition_elapsed / _active_transition_duration
			_active_transition_duration = val
			_transition_elapsed = ratio * _active_transition_duration
			if active_tween and active_tween.is_valid() and _transition_original_duration > 0.1:
				active_tween.set_speed_scale(_transition_original_duration / val)

var energy_threshold: float = 0.5
var _transition_original_duration: float = 0.0
var upcoming_track_override := "":
	set(val):
		if upcoming_track_override != val:
			upcoming_track_override = val
			_update_upcoming_transition()
			_update_prefetching()

var playback_history: Array[String] = []
var history_pointer: int = -1
var _navigating_history := false
var _last_transition_info: Dictionary = {}
var _transition_history_file := "user://transition_history.json"

# PLL & Beat Sync State Variables
var _outgoing_player: AudioStreamPlayer = null
var _incoming_player: AudioStreamPlayer = null
var _outgoing_href: String = ""
var _incoming_href: String = ""
var _pll_db_in_idx: int = -1
var _pll_db_out_idx: int = -1
var _transition_elapsed_time: float = 0.0
var _pll_delay: float = 0.0
var _incoming_base_pitch: float = 1.0
var current_pll_adjustment: float = 0.0
var current_pll_phase_error_ms: float = 0.0
var _pll_cc_timer: float = 0.0
var _last_cc_offset: float = 0.0

var dsp_a: AudioDSP
var dsp_b: AudioDSP
var _speed_scale_a: float = 1.0
var _speed_scale_b: float = 1.0

var crossfader: float = 0.0:
	set(val):
		if crossfader != val:
			crossfader = val
			x = val
			_update_subtractive_eq()

var x: float = 0.0:
	set(val):
		if x != val:
			x = val
			crossfader = val
			_update_subtractive_eq()

var rms_reference: float = 0.5
var subtractive_eq_crossfade_duration: float = 1.0
var _active_transition_duration: float = 0.0
var _transition_elapsed: float = 0.0

var _transition_eq_gains := {
	"DeckA": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
	"DeckB": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
}

var _clipping_attenuation_db := [0.0, 0.0, 0.0, 0.0, 0.0, 0.0]

var _filter_states := {
	"DeckA": {
		"low_l": 0.0, "low_r": 0.0,
		"mid_low_l": 0.0, "mid_low_r": 0.0
	},
	"DeckB": {
		"low_l": 0.0, "low_r": 0.0,
		"mid_low_l": 0.0, "mid_low_r": 0.0
	}
}

var _last_rms := {
	"DeckA": { "low": 0.0, "mid": 0.0, "high": 0.0 },
	"DeckB": { "low": 0.0, "mid": 0.0, "high": 0.0 }
}


var deck_a_bar: int = 1
var deck_a_beat: int = 1
var deck_b_bar: int = 1
var deck_b_beat: int = 1

# Tracks whether we have already kicked off the background load for the upcoming
# track during the outro window. Reset whenever a new track becomes active.
var _outro_load_triggered := false

func get_transition_duration(type: String, outgoing_href: String = "", incoming_href: String = "") -> float:
	if not smart_mixing_enabled:
		if not Engine.is_editor_hint() and transition_duration > 0.0:
			return transition_duration
		match type:
			"Bass Swap":
				return 6.0
			"Filter Sweep":
				return 4.0
			"Standard Crossfade":
				return 3.0
			"Echo Out":
				return 5.0
			"Reverb Freeze":
				return 5.0
			"Tempo Morph":
				return 6.0
			_:
				return 3.0
		
	if not Engine.is_editor_hint() and transition_duration == 0.1:
		return 0.1
	if not Engine.is_editor_hint() and transition_duration > 0.1:
		return transition_duration
		
	# Try dynamic duration lookup if metadata is available
	var out_href = outgoing_href
	var in_href = incoming_href
	
	if out_href.is_empty() and _outgoing_href != "":
		out_href = _outgoing_href
	if in_href.is_empty() and _incoming_href != "":
		in_href = _incoming_href
		
	if out_href.is_empty() and current_track_index > 0 and current_track_index - 1 < current_playlist.size():
		out_href = current_playlist[current_track_index - 1]
	if in_href.is_empty() and current_track_index >= 0 and current_track_index < current_playlist.size():
		in_href = current_playlist[current_track_index]
		
	var metadata_service = get_node_or_null("/root/MetadataService")
	if not out_href.is_empty() and not in_href.is_empty() and is_instance_valid(metadata_service):
		var meta_out = metadata_service.get_cached_metadata(out_href)
		var meta_in = metadata_service.get_cached_metadata(in_href)
		if not meta_out.is_empty() and not meta_in.is_empty():
			var outro = meta_out.get("segments", {}).get("outro", [])
			var intro = meta_in.get("segments", {}).get("intro", [])
			if outro.size() >= 2 and intro.size() >= 2:
				var outro_len = outro[1] - outro[0]
				var intro_len = intro[1] - intro[0]
				if outro_len > 0.0 and intro_len > 0.0:
					var overlap = minf(outro_len, intro_len)
					var clamped_overlap = clampf(overlap, 4.0, 16.0)
					
					var bpm_out = meta_out.get("bpm", 120.0)
					if bpm_out <= 0.0:
						bpm_out = 120.0
					var bar_duration = 240.0 / bpm_out
					
					# Standard phrase boundaries (16, 8, 4 bars)
					var candidates = [16.0 * bar_duration, 8.0 * bar_duration, 4.0 * bar_duration]
					var selected_duration = -1.0
					for c in candidates:
						if c <= clamped_overlap:
							selected_duration = c
							break
							
					if selected_duration < 0.0:
						selected_duration = 4.0
						
					return clampf(selected_duration, 4.0, 16.0)
			
	match type:
		"Bass Swap":
			return 6.0
		"Filter Sweep":
			return 4.0
		"Standard Crossfade":
			return 3.0
		"Echo Out":
			return 5.0
		"Reverb Freeze":
			return 5.0
		"Tempo Morph":
			return 6.0
		_:
			return 3.0

func _record_history(track_href: String) -> void:
	if _navigating_history:
		return
	if history_pointer < playback_history.size() - 1:
		playback_history = playback_history.slice(0, history_pointer + 1)
	if playback_history.is_empty() or playback_history.back() != track_href:
		playback_history.append(track_href)
		if playback_history.size() > 100:
			playback_history.remove_at(0)
	history_pointer = playback_history.size() - 1

var smart_mixing_enabled := false:
	set(val):
		if smart_mixing_enabled != val:
			smart_mixing_enabled = val
			smart_mixing_toggled.emit(val)
			_update_upcoming_transition()
			if not val:
				is_transitioning = false
				if _outgoing_player and _outgoing_player.playing:
					_outgoing_player.stop()
				var bus_a = AudioServer.get_bus_index("DeckA")
				var bus_b = AudioServer.get_bus_index("DeckB")
				if bus_a != -1:
					_reset_bus_effects(bus_a)
					AudioServer.set_bus_volume_db(bus_a, 0.0)
				if bus_b != -1:
					_reset_bus_effects(bus_b)
					AudioServer.set_bus_volume_db(bus_b, 0.0)

var smart_mixing_step_index := 0
var preferred_type_for_current_track := "perfect"
var upcoming_transition_type := "Standard Crossfade"
var _block_finished_signal := false

func get_next_track_href() -> String:
	if current_playlist.is_empty():
		return ""
	if not upcoming_track_override.is_empty() and current_playlist.has(upcoming_track_override):
		return upcoming_track_override
	if smart_mixing_enabled:
		var preferred_track = get_preferred_smart_match()
		if not preferred_track.is_empty():
			return preferred_track
	if current_track_index != -1 and current_playlist.size() > 0:
		return current_playlist[(current_track_index + 1) % current_playlist.size()]
	return ""

func get_lookahead_tracks() -> Array[String]:
	var tracks: Array[String] = []
	if current_playlist.is_empty():
		return tracks
		
	# Now playing track (Track A)
	if current_track_index >= 0 and current_track_index < current_playlist.size():
		var now_playing = current_playlist[current_track_index]
		if not now_playing.is_empty():
			tracks.append(now_playing)
			
	# Next track (Track B)
	var track_b = get_next_track_href()
	if not track_b.is_empty():
		if not tracks.has(track_b):
			tracks.append(track_b)
		
		# Follow-up track (Track C)
		var idx_b = current_playlist.find(track_b)
		if idx_b != -1 and current_playlist.size() > 1:
			var track_c = current_playlist[(idx_b + 1) % current_playlist.size()]
			if not track_c.is_empty() and not tracks.has(track_c):
				tracks.append(track_c)
	return tracks

func _update_prefetching() -> void:
	var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
	if is_instance_valid(audio_analyzer):
		var lookahead = get_lookahead_tracks()
		audio_analyzer.start_prefetching(lookahead)

func _get_match_type_between(current_meta: Dictionary, next_meta: Dictionary) -> String:
	if current_meta.is_empty() or next_meta.is_empty():
		return "perfect"
	var current_genre = current_meta.get("genre", "Unknown")
	var next_genre = next_meta.get("genre", "Unknown")
	
	var DJPathfinderClass = load("res://scripts/services/dj_pathfinder.gd")
	var similar = DJPathfinderClass.is_similar_genre(current_genre, next_genre)
	if not similar:
		return "creative"
		
	var bpm_diff = abs(current_meta.get("bpm", 120.0) - next_meta.get("bpm", 120.0))
	var energy_diff = abs(current_meta.get("energy_level", 0.5) - next_meta.get("energy_level", 0.5))
	if bpm_diff >= 8.0 or energy_diff >= 0.2:
		return "interesting"
		
	return "perfect"

func is_track_cached(href_path: String) -> bool:
	if href_path.is_empty():
		return false
	if href_path == "song1.mp3" or href_path == "song2.mp3" or href_path == "song3.mp3" or href_path.begins_with("test_"):
		return true
	var ext := href_path.get_extension()
	if ext.is_empty():
		ext = "mp3"
	var cache_path := CACHE_DIR + href_path.md5_text() + "." + ext
	return FileAccess.file_exists(cache_path)

func get_transition_trigger_time() -> float:
	var metadata_service = get_node_or_null("/root/MetadataService")
	if not smart_mixing_enabled or current_playlist.is_empty() or current_track_index < 0 or current_track_index >= current_playlist.size():
		return -1.0
	var out_href = current_playlist[current_track_index]
	var in_href = get_next_track_href()
	if in_href.is_empty():
		return -1.0
	
	var length = current_track_length
	var cue_out = length
	cue_out = _get_track_cue_out(out_href, length)
	
	var duration = get_transition_duration(upcoming_transition_type, out_href, in_href)
	
	var outro_start = cue_out - 30.0
	if is_instance_valid(metadata_service):
		var meta = metadata_service.get_cached_metadata(out_href)
		var segments = meta.get("segments", {})
		var outro = segments.get("outro", [])
		if not outro.is_empty():
			outro_start = outro[0]
			
	var trigger_threshold = min(outro_start, cue_out - (duration + 4.0))
	return trigger_threshold - 5.0

func get_transition_type_between(current_meta: Dictionary, next_meta: Dictionary, current_href: String, next_href: String) -> String:
	if current_meta.is_empty() or next_meta.is_empty():
		var pair_hash = (current_href + next_href).hash()
		var rng = abs(pair_hash) % 6
		match rng:
			0: return "Standard Crossfade"
			1: return "Bass Swap"
			2: return "Filter Sweep"
			3: return "Echo Out"
			4: return "Reverb Freeze"
			5: return "Tempo Morph"
		return "Standard Crossfade"
		
	var bpm_diff = abs(current_meta.get("bpm", 120.0) - next_meta.get("bpm", 120.0))
	var key_a = current_meta.get("musical_key", "")
	var key_b = next_meta.get("musical_key", "")
	
	var DJPathfinderClass = load("res://scripts/services/dj_pathfinder.gd")
	var key_cost = DJPathfinderClass.get_harmonic_relation_cost(key_a, key_b)
	var match_type = _get_match_type_between(current_meta, next_meta)
	
	var is_clashing = (key_cost >= 8.0)
	var is_switch = (match_type == "creative")
	
	var weights := {}
	if is_clashing or is_switch:
		if bpm_diff < 3.0:
			weights = {"Echo Out": 50, "Reverb Freeze": 50}
		elif bpm_diff < 8.0:
			weights = {"Echo Out": 50, "Reverb Freeze": 50}
		else:
			weights = {"Echo Out": 60, "Reverb Freeze": 40}
	elif key_cost <= 2.0:
		if bpm_diff < 3.0:
			weights = {"Bass Swap": 75, "Filter Sweep": 15, "Standard Crossfade": 10}
		elif bpm_diff < 8.0:
			weights = {"Tempo Morph": 45, "Filter Sweep": 25, "Echo Out": 15, "Reverb Freeze": 15}
		else:
			weights = {"Echo Out": 50, "Reverb Freeze": 30, "Standard Crossfade": 20}
	else:
		if bpm_diff < 3.0:
			weights = {"Bass Swap": 60, "Filter Sweep": 20, "Tempo Morph": 20}
		elif bpm_diff < 8.0:
			weights = {"Tempo Morph": 40, "Echo Out": 25, "Reverb Freeze": 20, "Filter Sweep": 15}
		else:
			weights = {"Echo Out": 60, "Reverb Freeze": 40}
	
	var pair_hash = (current_href + next_href).hash()
	var rng = RandomNumberGenerator.new()
	rng.seed = pair_hash
	
	var total_weight := 0.0
	for w in weights.values():
		total_weight += w
		
	var roll = rng.randf() * total_weight
	var cumulative := 0.0
	var selected_type = "Standard Crossfade"
	for type in weights:
		cumulative += weights[type]
		if roll <= cumulative:
			selected_type = type
			break
			
	return selected_type

func _update_upcoming_transition() -> void:
	var metadata_service = get_node_or_null("/root/MetadataService")
	if not smart_mixing_enabled:
		upcoming_transition_type = "Standard Crossfade"
		return
		
	var next_href = get_next_track_href()
	if next_href.is_empty():
		upcoming_transition_type = "Standard Crossfade"
		return
		
	var current_href = ""
	if current_track_index != -1 and current_track_index < current_playlist.size():
		current_href = current_playlist[current_track_index]
		
	if is_instance_valid(metadata_service) and not current_href.is_empty():
		var current_meta = metadata_service.get_cached_metadata(current_href)
		var next_meta = metadata_service.get_cached_metadata(next_href)
		upcoming_transition_type = get_transition_type_between(current_meta, next_meta, current_href, next_href)
	else:
		upcoming_transition_type = get_transition_type_between({}, {}, current_href, next_href)

var _debounce_timer: Timer

const CACHE_DIR = "user://audio_cache/"

var _metadata_service: Node = null
var _beat_grid_a: Array = []
var _beat_grid_b: Array = []

func _ready() -> void:
	# Programmatically setup DeckA and DeckB audio buses with EQ and Filter effects
	_setup_audio_buses()
	
	dsp_a = AudioDSP.new()
	add_child(dsp_a)
	
	dsp_b = AudioDSP.new()
	add_child(dsp_b)
	
	# Build out runtime child container component dynamically
	player_a = AudioStreamPlayer.new()
	player_a.bus = "DeckA"
	add_child(player_a)
	player_a.finished.connect(func(): _on_deck_finished(player_a))
	
	player_b = AudioStreamPlayer.new()
	player_b.bus = "DeckB"
	add_child(player_b)
	player_b.finished.connect(func(): _on_deck_finished(player_b))
	
	active_player = player_a
	player = active_player
	
	_debounce_timer = Timer.new()
	_debounce_timer.one_shot = true
	_debounce_timer.timeout.connect(_on_debounce_timeout)
	add_child(_debounce_timer)
	
	# Apply headphone calibration if enabled
	update_calibration_state()
	
	_metadata_service = get_node_or_null("/root/MetadataService")
	if is_instance_valid(_metadata_service):
		_metadata_service.metadata_updated.connect(_on_metadata_updated)

func _on_metadata_updated(href: String, new_metadata: Dictionary) -> void:
	if current_playlist.size() > current_track_index and current_track_index >= 0:
		if current_playlist[current_track_index] == href:
			var grid = new_metadata.get("beat_grid", [])
			if active_player == player_a:
				_beat_grid_a = grid
			else:
				_beat_grid_b = grid
	if is_transitioning:
		if _outgoing_href == href:
			var grid = new_metadata.get("beat_grid", [])
			if _outgoing_player == player_a:
				_beat_grid_a = grid
			else:
				_beat_grid_b = grid
		if _incoming_href == href:
			var grid = new_metadata.get("beat_grid", [])
			if _incoming_player == player_a:
				_beat_grid_a = grid
			else:
				_beat_grid_b = grid

func _process(delta: float) -> void:
	# Feed audio buffers
	_feed_deck(player_a, dsp_a, _speed_scale_a, _incoming_player == player_a)
	_feed_deck(player_b, dsp_b, _speed_scale_b, _incoming_player == player_b)

	# Update bars and beats for both decks
	var bab_a = get_bars_and_beats(dsp_a.get_playback_position(), _beat_grid_a)
	deck_a_bar = bab_a["bar"]
	deck_a_beat = bab_a["beat"]
		
	var bab_b = get_bars_and_beats(dsp_b.get_playback_position(), _beat_grid_b)
	deck_b_bar = bab_b["bar"]
	deck_b_beat = bab_b["beat"]

	# Monitor playback position to trigger smooth transitions before outro ends
	if is_playing and not is_transitioning and active_player and active_player.playing:
		var active_dsp = dsp_a if active_player == player_a else dsp_b
		if active_dsp.is_finished():
			active_player.stop()
			_on_deck_finished(active_player)
		elif smart_mixing_enabled:
			var pos = active_dsp.get_playback_position()
			var length = current_track_length
			var cue_out = length
			if current_track_index >= 0 and current_track_index < current_playlist.size():
				cue_out = _get_track_cue_out(current_playlist[current_track_index], length)
			var out_href = current_playlist[current_track_index] if current_track_index >= 0 and current_track_index < current_playlist.size() else ""
			var in_href = current_playlist[current_track_index + 1] if current_track_index >= 0 and current_track_index + 1 < current_playlist.size() else ""
			var duration = get_transition_duration(upcoming_transition_type, out_href, in_href)
			
			var outro_start = cue_out - 30.0
			if is_instance_valid(_metadata_service) and current_track_index >= 0 and current_track_index < current_playlist.size():
				var meta = _metadata_service.get_cached_metadata(current_playlist[current_track_index])
				var segments = meta.get("segments", {})
				var outro = segments.get("outro", [])
				if not outro.is_empty():
					outro_start = outro[0]
			
			# Guarantee the outgoing track stays alive through at least the first half of
			# the effect. Add a fixed load-headroom buffer (6s) on top of the full
			# duration so that WebDAV streaming has time to complete before the effect
			# fires. The trigger fires exactly at this threshold — no early-fire window.
			var load_headroom := 6.0
			var trigger_threshold = min(outro_start, cue_out - (duration + load_headroom))
			
			# Pre-load the incoming track in the background as soon as we enter the
			# outro window. This decouples loading latency from the transition trigger.
			if not _outro_load_triggered and not in_href.is_empty() and pos >= outro_start:
				_outro_load_triggered = true
				var incoming_player = player_b if active_player == player_a else player_a
				var in_bus_idx = AudioServer.get_bus_index(incoming_player.bus)
				_reset_bus_effects(in_bus_idx)
				AudioServer.set_bus_volume_db(in_bus_idx, -60.0)
				print("AudioManager: Outro zone entered. Pre-loading incoming track: ", in_href)
				_load_and_stream_remote_file(in_href, incoming_player, false)
			
			if pos >= trigger_threshold:
				print("AudioManager: Approaching outro zone. Triggering start_transition. pos=", pos, ", threshold=", trigger_threshold)
				start_transition()
			
	if is_transitioning and is_playing:
		_transition_elapsed_time += delta
		if _transition_elapsed_time > _pll_delay:
			_apply_pll_sync(delta)
			
		if _active_transition_duration > 0.0:
			_transition_elapsed += delta
			crossfader = clamp(_transition_elapsed / _active_transition_duration, 0.0, 1.0)
			
	_process_clipping_prevention()

func _setup_audio_buses() -> void:
	var bus_names = ["DeckA", "DeckB"]
	for b_name in bus_names:
		var idx = AudioServer.get_bus_index(b_name)
		if idx == -1:
			AudioServer.add_bus()
			idx = AudioServer.get_bus_count() - 1
			AudioServer.set_bus_name(idx, b_name)
			AudioServer.set_bus_send(idx, "Master")
		
		# Ensure effects are set up correctly on the bus at the expected positions
		var effect_count = AudioServer.get_bus_effect_count(idx)
		
		var eq_ok = false
		if effect_count > 0:
			var eq = AudioServer.get_bus_effect(idx, 0)
			if eq is AudioEffectEQ6:
				eq_ok = true
			else:
				AudioServer.remove_bus_effect(idx, 0)
				effect_count -= 1
		
		if not eq_ok:
			var eq = AudioEffectEQ6.new()
			AudioServer.add_bus_effect(idx, eq, 0)
			effect_count = AudioServer.get_bus_effect_count(idx)
			
		var lp_ok = false
		if effect_count > 1:
			var lp = AudioServer.get_bus_effect(idx, 1)
			if lp is AudioEffectLowPassFilter:
				lp_ok = true
			else:
				AudioServer.remove_bus_effect(idx, 1)
				effect_count -= 1
				
		if not lp_ok:
			var lp = AudioEffectLowPassFilter.new()
			lp.cutoff_hz = 20000.0
			AudioServer.add_bus_effect(idx, lp, 1)
			effect_count = AudioServer.get_bus_effect_count(idx)
			
		var hp_ok = false
		if effect_count > 2:
			var hp = AudioServer.get_bus_effect(idx, 2)
			if hp is AudioEffectHighPassFilter:
				hp_ok = true
			else:
				AudioServer.remove_bus_effect(idx, 2)
				effect_count -= 1
				
		if not hp_ok:
			var hp = AudioEffectHighPassFilter.new()
			hp.cutoff_hz = 10.0
			AudioServer.add_bus_effect(idx, hp, 2)
			effect_count = AudioServer.get_bus_effect_count(idx)
			
		var delay_ok = false
		if effect_count > 3:
			var delay = AudioServer.get_bus_effect(idx, 3)
			if delay is AudioEffectDelay:
				delay_ok = true
			else:
				AudioServer.remove_bus_effect(idx, 3)
				effect_count -= 1
				
		if not delay_ok:
			var delay = AudioEffectDelay.new()
			delay.dry = 1.0
			delay.feedback_delay_ms = 300.0
			delay.feedback_level_db = -60.0
			delay.tap1_level_db = -60.0
			delay.tap2_level_db = -60.0
			AudioServer.add_bus_effect(idx, delay, 3)
			effect_count = AudioServer.get_bus_effect_count(idx)
			
		var reverb_ok = false
		if effect_count > 4:
			var reverb = AudioServer.get_bus_effect(idx, 4)
			if reverb is AudioEffectReverb:
				reverb_ok = true
			else:
				AudioServer.remove_bus_effect(idx, 4)
				effect_count -= 1
				
		if not reverb_ok:
			var reverb = AudioEffectReverb.new()
			reverb.wet = 0.0
			reverb.dry = 1.0
			AudioServer.add_bus_effect(idx, reverb, 4)

	# Programmatically setup Master bus effects
	var master_idx = AudioServer.get_bus_index("Master")
	if master_idx != -1:
		var master_effect_count = AudioServer.get_bus_effect_count(master_idx)
		
		# 1. Preamp (Amplify effect) at index 0
		var preamp_ok = false
		if master_effect_count > 0:
			var amp = AudioServer.get_bus_effect(master_idx, 0)
			if amp is AudioEffectAmplify:
				preamp_ok = true
			else:
				AudioServer.remove_bus_effect(master_idx, 0)
				master_effect_count -= 1
		if not preamp_ok:
			var amp = AudioEffectAmplify.new()
			amp.volume_db = 0.0
			AudioServer.add_bus_effect(master_idx, amp, 0)
			master_effect_count = AudioServer.get_bus_effect_count(master_idx)
			
		# 2. 10 Parametric EQ bands at indices 1 to 10
		for i in range(10):
			var pos = i + 1
			var band_ok = false
			if master_effect_count > pos:
				var eff = AudioServer.get_bus_effect(master_idx, pos)
				if eff is AudioEffectFilter:
					band_ok = true
				else:
					AudioServer.remove_bus_effect(master_idx, pos)
					master_effect_count -= 1
			if not band_ok:
				var filter = AudioEffectBandLimitFilter.new()
				filter.cutoff_hz = 1000.0
				filter.resonance = 0.5
				filter.gain = 1.0
				AudioServer.add_bus_effect(master_idx, filter, pos)
				AudioServer.set_bus_effect_enabled(master_idx, pos, false) # Bypassed by default to minimize CPU/latency
				master_effect_count = AudioServer.get_bus_effect_count(master_idx)


## Configures the index-th parametric EQ band (0-9) on the Master bus.
## Maps the type parameter dynamically to Godot subclasses to ensure audio routing
## does not introduce distortion or latency when filters are active (bypassed if gain is 0dB).
func set_eq_band(index: int, frequency: float, gain: float, q: float, type: String = "peaking") -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	if master_idx == -1:
		return
		
	var pos = index + 1 # Position on the Master bus (after preamp at 0)
	if pos < 1 or pos > 10:
		push_error("AudioManager: EQ band index out of range: " + str(index))
		return
		
	var target_class = AudioEffectBandLimitFilter
	match type.to_lower():
		"peaking", "pk":
			target_class = AudioEffectBandLimitFilter
		"low_shelf", "lsc":
			target_class = AudioEffectLowPassFilter
		"high_shelf", "hsc":
			target_class = AudioEffectHighPassFilter
		"notch", "not":
			target_class = AudioEffectNotchFilter
		"low_pass", "lp":
			target_class = AudioEffectLowPassFilter
		"high_pass", "hp":
			target_class = AudioEffectHighPassFilter
			
	var current_effect = AudioServer.get_bus_effect(master_idx, pos)
	if not is_instance_valid(current_effect) or not current_effect.is_class(target_class.new().get_class()):
		# Replace the effect at this position with the target class instance
		AudioServer.remove_bus_effect(master_idx, pos)
		var new_filter = target_class.new()
		AudioServer.add_bus_effect(master_idx, new_filter, pos)
		current_effect = new_filter
		
	if current_effect is AudioEffectFilter:
		current_effect.cutoff_hz = clamp(frequency, 20.0, 22000.0)
		current_effect.resonance = clamp(q, 0.01, 10.0)
		current_effect.gain = db_to_linear(gain)
		
		# Optimization: bypass the filter entirely if gain is neutral (0.0 dB)
		if abs(gain) < 0.01:
			AudioServer.set_bus_effect_enabled(master_idx, pos, false)
		else:
			AudioServer.set_bus_effect_enabled(master_idx, pos, true)


## Sets the preamp volume level on the AudioEffectAmplify instance at index 0 of the Master bus.
func set_preamp_gain(gain_db: float) -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	if master_idx == -1:
		return
	var amp = AudioServer.get_bus_effect(master_idx, 0)
	if amp is AudioEffectAmplify:
		amp.volume_db = clamp(gain_db, -60.0, 24.0)


## Resets the preamp and all 10 EQ bands to their bypassed, neutral states.
func clear_eq_bands() -> void:
	var master_idx = AudioServer.get_bus_index("Master")
	if master_idx == -1:
		return
		
	var amp = AudioServer.get_bus_effect(master_idx, 0)
	if amp is AudioEffectAmplify:
		amp.volume_db = 0.0
		
	for i in range(10):
		var pos = i + 1
		var filter = AudioServer.get_bus_effect(master_idx, pos)
		if filter is AudioEffectFilter:
			filter.cutoff_hz = 1000.0
			filter.resonance = 0.5
			filter.gain = 1.0
			AudioServer.set_bus_effect_enabled(master_idx, pos, false)


## Applies a parsed AutoEQ profile (preamp + bands) to the Master bus EQ filters.
func apply_eq_profile(profile: Dictionary) -> void:
	clear_eq_bands()
	
	set_preamp_gain(profile.get("preamp", 0.0))
	
	var bands = profile.get("bands", [])
	for i in range(min(bands.size(), 10)):
		var band = bands[i]
		if not band.get("enabled", true):
			continue
			
		set_eq_band(
			i,
			band.get("frequency", 1000.0),
			band.get("gain", 0.0),
			band.get("q", 1.0),
			band.get("type", "peaking")
		)


## Updates the Master bus parametric EQ calibration state based on current settings.
func update_calibration_state() -> void:
	if not is_instance_valid(SettingsManager):
		return
		
	if SettingsManager.headphone_calibration_enabled and not SettingsManager.headphone_profile.is_empty():
		var presets_path = "res://assets/settings/headphone_presets.json"
		if FileAccess.file_exists(presets_path):
			var file = FileAccess.open(presets_path, FileAccess.READ)
			if file:
				var json_text = file.get_as_text()
				file.close()
				var json = JSON.new()
				var error = json.parse(json_text)
				if error == OK and json.data is Dictionary:
					var presets = json.data
					var profile_text = presets.get(SettingsManager.headphone_profile, "")
					if not profile_text.is_empty():
						var parser_script = preload("res://scripts/services/autoeq_parser.gd")
						var parsed = parser_script.parse_profile_text(profile_text)
						apply_eq_profile(parsed)
						return
					else:
						push_error("AudioManager: Headphone profile not found in presets: " + SettingsManager.headphone_profile)
				else:
					push_error("AudioManager: Failed to parse headphone presets JSON")
			else:
				push_error("AudioManager: Failed to open headphone presets file")
		else:
			push_error("AudioManager: Headphone presets file does not exist")
			
	# If disabled or failed to load/parse, clear EQ bands
	clear_eq_bands()



## Prepares a new track vector array queue context state
func play_track(track_href: String, playlist: Array) -> void:
	if _debounce_timer and not _debounce_timer.is_stopped():
		_debounce_timer.stop()
		
	if active_tween and active_tween.is_valid():
		active_tween.kill()
	if _pitch_ramp_tween and _pitch_ramp_tween.is_valid():
		_pitch_ramp_tween.kill()
		
	current_playlist = Array(playlist, TYPE_STRING, &"", null)
	current_track_index = current_playlist.find(track_href)
	upcoming_track_override = ""
	is_transitioning = false
	
	_record_history(track_href)
	_navigating_history = false
	_outro_load_triggered = false
	
	# Stop secondary player if active
	var secondary = player_b if active_player == player_a else player_a
	secondary.stop()
	_reset_bus_effects(AudioServer.get_bus_index(player_a.bus))
	_reset_bus_effects(AudioServer.get_bus_index(player_b.bus))
	
	_load_and_stream_remote_file(track_href, active_player)


## Asynchronously downloads the remote audio file context and loads it into an AudioStream layer
func _load_and_stream_remote_file(href_path: String, target_player: AudioStreamPlayer, play_immediately: bool = true) -> bool:
	# Mock load for unit tests
	if href_path == "song1.mp3" or href_path == "song2.mp3" or href_path == "song3.mp3" or href_path.begins_with("test_"):
		var stream = AudioStreamGenerator.new()
		stream.mix_rate = 44100
		stream.buffer_length = 0.5
		target_player.stream = stream
		
		var dsp = dsp_a if target_player == player_a else dsp_b
		var duration_hint = 0.0
		var grid = []
		var metadata_service = _metadata_service
		if is_instance_valid(metadata_service):
			var meta = metadata_service.get_cached_metadata(href_path)
			if not meta.is_empty():
				duration_hint = meta.get("duration", 0.0)
				grid = meta.get("beat_grid", [])
		dsp.load_file(ProjectSettings.globalize_path("res://tests/unit/test_track.wav"), duration_hint)
		
		if target_player == player_a:
			_beat_grid_a = grid
		else:
			_beat_grid_b = grid
		
		if target_player == player_a:
			_speed_scale_a = 1.0
		else:
			_speed_scale_b = 1.0
		
		# Set normalized volume
		var track_lufs = _get_track_lufs(href_path)
		var gain_db = -14.0 - track_lufs
		target_player.volume_db = clamp(gain_db, -12.0, 6.0)
		
		var cue_in = _get_track_cue_in(href_path)
		dsp.seek_pos(cue_in)
		
		if play_immediately:
			target_player.play()
			target_player.stream_paused = false
		else:
			target_player.stop()
			
		if target_player == active_player:
			current_track_length = dsp.get_duration()
			length_changed.emit(current_track_length)
			if play_immediately:
				is_playing = true
				playback_toggled.emit(true)
			else:
				is_playing = false
				playback_toggled.emit(false)
			update_preferred_type()
			_update_upcoming_transition()
		return true

	# Ensure cache directory exists
	if not DirAccess.dir_exists_absolute(CACHE_DIR):
		DirAccess.make_dir_recursive_absolute(CACHE_DIR)
		
	var ext := href_path.get_extension()
	if ext.is_empty():
		ext = "mp3"
	var cache_path := CACHE_DIR + href_path.md5_text() + "." + ext
	
	loading_track.emit(true)
	track_changed.emit(href_path.get_file().uri_decode())
	
	var cached_on_disk = FileAccess.file_exists(cache_path)
	
	if not cached_on_disk:
		var base_url: String = SettingsManager.webdav_url
		if not base_url.begins_with("http://") and not base_url.begins_with("https://"):
			print("AudioManager: Invalid or empty WebDAV URL, cannot stream: ", base_url)
			loading_track.emit(false)
			return false
		var username := SettingsManager.webdav_username
		var password := SettingsManager.webdav_password
		
		var auth_raw := "%s:%s" % [username, password]
		var auth_header := "Authorization: Basic %s" % Marshalls.utf8_to_base64(auth_raw)
		
		var url_parts = base_url.split("/dav")
		var host_base = url_parts[0]
		
		var clean_href = href_path
		if not clean_href.begins_with("/"):
			clean_href = "/" + clean_href
		
		var completely_decoded = clean_href.uri_decode()
		var safe_encoded_path = completely_decoded.uri_encode().replace("%2F", "/")
		var full_target_endpoint = host_base + safe_encoded_path
		
		print("AudioManager: Requesting stream from: ", full_target_endpoint)
		
		var http_client := HTTPRequest.new()
		add_child(http_client)
		
		http_client.set_tls_options(TLSOptions.client_unsafe()) 
		
		var temp_download_path = cache_path + ".manager.tmp"
		http_client.download_file = temp_download_path
		
		var err := http_client.request(full_target_endpoint, [auth_header], HTTPClient.METHOD_GET)
		if err != OK:
			print("AudioManager: Network request instantiation failure code: ", err)
			http_client.queue_free()
			if FileAccess.file_exists(temp_download_path):
				DirAccess.remove_absolute(temp_download_path)
			loading_track.emit(false)
			return false
			
		var response = await http_client.request_completed
		var response_code: int = response[1]
		
		http_client.queue_free()
		
		if response_code != 200:
			print("AudioManager: Server streaming request rejected with HTTP code: %d" % response_code)
			if FileAccess.file_exists(temp_download_path):
				DirAccess.remove_absolute(temp_download_path)
			loading_track.emit(false)
			return false
			
		if FileAccess.file_exists(cache_path):
			print("AudioManager: cache already exists for ", href_path)
			if FileAccess.file_exists(temp_download_path):
				DirAccess.remove_absolute(temp_download_path)
		else:
			err = DirAccess.rename_absolute(temp_download_path, cache_path)
			if err != OK:
				print("AudioManager: Failed to rename downloaded file to cache: ", err)
				cache_path = temp_download_path
			
	loading_track.emit(false)
	
	var dsp = dsp_a if target_player == player_a else dsp_b
	var duration_hint = 0.0
	var grid = []
	var metadata_service = _metadata_service
	if is_instance_valid(metadata_service):
		var meta = metadata_service.get_cached_metadata(href_path)
		if not meta.is_empty():
			duration_hint = meta.get("duration", 0.0)
			grid = meta.get("beat_grid", [])
	var load_ok = dsp.load_file(ProjectSettings.globalize_path(cache_path), duration_hint)
	
	if target_player == player_a:
		_beat_grid_a = grid
	else:
		_beat_grid_b = grid
	if not load_ok:
		print("AudioManager: Failed to load file into C++ AudioDSP: ", cache_path)
		return false
		
	var generator := AudioStreamGenerator.new()
	generator.mix_rate = 44100
	generator.buffer_length = 0.5 # 500ms safety buffer to prevent macOS screen swipe hiccups
	
	target_player.stream = generator
	
	if target_player == player_a:
		_speed_scale_a = 1.0
	else:
		_speed_scale_b = 1.0
	
	# Set normalized volume
	var track_lufs = _get_track_lufs(href_path)
	var gain_db = -14.0 - track_lufs
	target_player.volume_db = clamp(gain_db, -12.0, 6.0)
	
	var cue_in = _get_track_cue_in(href_path)
	dsp.seek_pos(cue_in)
	
	if play_immediately:
		target_player.play()
		target_player.stream_paused = false
	else:
		target_player.stop()
	
	# Enqueue for local audio analysis now that it is cached on disk
	var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
	if is_instance_valid(audio_analyzer):
		audio_analyzer.analyze_track(href_path, true)
	
	if target_player == active_player:
		current_track_length = dsp.get_duration()
		length_changed.emit(current_track_length)
		if play_immediately:
			is_playing = true
			playback_toggled.emit(true)
		else:
			is_playing = false
			playback_toggled.emit(false)
		update_preferred_type()
		_update_upcoming_transition()
		
	return true


func toggle_play() -> void:
	if active_player.stream == null:
		return
	if is_playing:
		if player_a:
			player_a.stream_paused = true
		if player_b:
			player_b.stream_paused = true
		is_playing = false
		if active_tween and active_tween.is_valid():
			active_tween.pause()
		if _pitch_ramp_tween and _pitch_ramp_tween.is_valid():
			_pitch_ramp_tween.pause()
	else:
		if player_a:
			player_a.stream_paused = false
		if player_b:
			player_b.stream_paused = false
		is_playing = true
		if active_tween and active_tween.is_valid():
			active_tween.play()
		if _pitch_ramp_tween and _pitch_ramp_tween.is_valid():
			_pitch_ramp_tween.play()
	playback_toggled.emit(is_playing)

func play_next() -> void:
	_check_and_log_skip()
	if current_playlist.is_empty():
		return
	if _debounce_timer and not _debounce_timer.is_stopped():
		_debounce_timer.stop()
	
	if history_pointer < playback_history.size() - 1:
		history_pointer += 1
		var next_href = playback_history[history_pointer]
		if current_playlist.has(next_href):
			current_track_index = current_playlist.find(next_href)
			_navigating_history = true
			_start_debounce()
			return
			
	var next_href = get_next_track_href()
	if not next_href.is_empty() and current_playlist.has(next_href):
		current_track_index = current_playlist.find(next_href)
		upcoming_track_override = ""
	else:
		current_track_index = (current_track_index + 1) % current_playlist.size()
	_start_debounce()

func play_previous() -> void:
	if current_playlist.is_empty():
		return
	if _debounce_timer and not _debounce_timer.is_stopped():
		_debounce_timer.stop()
		
	upcoming_track_override = ""
	if history_pointer > 0:
		history_pointer -= 1
		var prev_href = playback_history[history_pointer]
		if current_playlist.has(prev_href):
			current_track_index = current_playlist.find(prev_href)
			_navigating_history = true
			_start_debounce()
			return
			
	# Fallback if no history or track not in playlist
	current_track_index = (current_track_index - 1 + current_playlist.size()) % current_playlist.size()
	_start_debounce()

func update_preferred_type() -> void:
	var step = smart_mixing_step_index % 4
	match step:
		0, 2:
			preferred_type_for_current_track = "perfect"
			energy_threshold = 0.15
		1:
			preferred_type_for_current_track = "interesting"
			energy_threshold = 0.35
		3:
			preferred_type_for_current_track = "creative" if randf() < 0.5 else "interesting"
			if preferred_type_for_current_track == "creative":
				energy_threshold = 0.60
			else:
				energy_threshold = 0.35

func get_preferred_smart_match() -> String:
	if current_playlist.is_empty() or current_track_index == -1 or current_track_index >= current_playlist.size():
		return ""
		
	var current_href = current_playlist[current_track_index]
	var metadata_service = get_node_or_null("/root/MetadataService")
	
	var DJPathfinderClass = load("res://scripts/services/dj_pathfinder.gd")
	var matches = DJPathfinderClass.calculate_smart_matches(current_href, current_playlist, metadata_service)
	if matches.is_empty():
		return ""
		
	var preferred_type = preferred_type_for_current_track
	var chosen_href = ""
	if matches.has(preferred_type) and matches[preferred_type] is Dictionary:
		chosen_href = matches[preferred_type].get("href", "")
		
	if chosen_href.is_empty():
		for type in ["perfect", "interesting", "creative"]:
			if matches.has(type) and matches[type] is Dictionary and not matches[type].get("href", "").is_empty():
				chosen_href = matches[type].get("href", "")
				break
				
	return chosen_href

## Triggers the DJ transition crossfade to the next track
func start_transition(force_immediate: bool = false) -> void:
	if is_transitioning or current_playlist.is_empty():
		return
		
	is_transitioning = true
	
	# Select upcoming track (support manual override or default to best runner-up)
	var next_track_href = get_next_track_href()
	if next_track_href.is_empty():
		is_transitioning = false
		return
		
	if upcoming_track_override == next_track_href:
		upcoming_track_override = ""
		
	if not smart_mixing_enabled:
		is_transitioning = false
		current_track_index = current_playlist.find(next_track_href)
		if current_track_index == -1:
			current_playlist.append(next_track_href)
			current_track_index = current_playlist.size() - 1
		_record_history(next_track_href)
		_navigating_history = false
		_load_and_stream_remote_file(next_track_href, active_player, is_playing)
		return
	var prev_index = current_track_index
	current_track_index = current_playlist.find(next_track_href)
	if current_track_index == -1:
		current_playlist.append(next_track_href)
		current_track_index = current_playlist.size() - 1
		
	if smart_mixing_enabled:
		smart_mixing_step_index = (smart_mixing_step_index + 1) % 4
		
	var transition_type = upcoming_transition_type
		
	var incoming_player = player_b if active_player == player_a else player_a
	
	# Mute and reset incoming deck immediately to prevent any accidental sound during loading/outro wait
	var in_bus_idx = AudioServer.get_bus_index(incoming_player.bus)
	_reset_bus_effects(in_bus_idx)
	AudioServer.set_bus_volume_db(in_bus_idx, -60.0)
	
	transition_started.emit(next_track_href, transition_type)
	
	# Load next stream on incoming player, but do not play immediately
	var load_success = await _load_and_stream_remote_file(next_track_href, incoming_player, false)
	if not load_success:
		is_transitioning = false
		current_track_index = prev_index
		if smart_mixing_enabled:
			smart_mixing_step_index = (smart_mixing_step_index - 1 + 4) % 4
		var active_href = ""
		if current_track_index >= 0 and current_track_index < current_playlist.size():
			active_href = current_playlist[current_track_index]
		transition_completed.emit(active_href)
		return
	
	# Calculate target trigger time based on outro segment and 8-bar loop boundaries
	var t_trigger = -1.0
	var has_beat_metadata = false
	var metadata_service = get_node_or_null("/root/MetadataService")
	var current_href = current_playlist[prev_index] if prev_index >= 0 and prev_index < current_playlist.size() else ""
	var cue_in = _get_track_cue_in(next_track_href)
	
	if is_instance_valid(metadata_service) and not current_href.is_empty():
		var meta_out = metadata_service.get_cached_metadata(current_href)
		var meta_in = metadata_service.get_cached_metadata(next_track_href)
		if not meta_out.is_empty() and not meta_in.is_empty():
			var downbeats_out = meta_out.get("downbeats", [])
			var downbeats_in = meta_in.get("downbeats", [])
			var bpm_out = meta_out.get("bpm", 120.0)
			
			if not downbeats_out.is_empty() and not downbeats_in.is_empty():
				var segments = meta_out.get("segments", {})
				var outro = segments.get("outro", [])
				var outro_start = outro[0] if not outro.is_empty() else _get_track_cue_out(current_href, current_track_length) - 30.0
				
				var active_dsp = dsp_a if active_player == player_a else dsp_b
				var transients_out = meta_out.get("transients", [])
				var transients_in = meta_in.get("transients", [])
				if force_immediate:
					var pos = active_dsp.get_playback_position()
					var next_downbeat = -1.0
					for db in downbeats_out:
						if db > pos:
							next_downbeat = db
							break
					if next_downbeat == -1.0:
						var last_db = downbeats_out[-1]
						var bar_duration = 240.0 / (bpm_out if bpm_out > 0.0 else 120.0)
						var diff = pos - last_db
						var bars_passed = ceil(diff / bar_duration)
						if bars_passed <= 0:
							bars_passed = 1
						next_downbeat = last_db + (bars_passed * bar_duration)
					t_trigger = next_downbeat
					has_beat_metadata = true
				else:
					t_trigger = get_aligned_trigger_time(active_dsp.get_playback_position(), bpm_out, downbeats_out, cue_in, downbeats_in, outro_start, transients_out, transients_in)
					has_beat_metadata = true

	# Wait until outgoing track has duration or less remaining (skip if force_immediate is true, unless we have beat metadata to quantize)
	var duration = get_transition_duration(transition_type, current_href, next_track_href)
	if not force_immediate or (force_immediate and has_beat_metadata):
		var last_pos := -1.0
		var stall_accumulated := 0.0
		# Use is_playing (app-level intent) as the loop authority — not active_player.playing
		# (a Godot node property that goes false the moment the stream ends). During a
		# seamless DJ session the music is always conceptually playing; node state is
		# only checked inside the loop as a signal to break early.
		while is_playing and is_transitioning:
			if not active_player:
				break
			var active_dsp = dsp_a if active_player == player_a else dsp_b
			# Stream finished naturally — fire the transition now rather than waiting
			if not active_player.playing or active_dsp.is_finished():
				print("AudioManager: Outgoing stream ended during transition wait. Firing immediately.")
				break
			var pos = active_dsp.get_playback_position()
			
			# Stall detection: position genuinely frozen (e.g. WebDAV buffer stall)
			if not active_player.stream_paused:
				if pos == last_pos:
					stall_accumulated += get_process_delta_time()
					if stall_accumulated >= 1.5:
						print("AudioManager: Playback stall detected in transition loop. Breaking.")
						break
				else:
					stall_accumulated = 0.0
			else:
				stall_accumulated = 0.0
			last_pos = pos
			
			if has_beat_metadata:
				if pos >= t_trigger:
					break
			else:
				var remaining = current_track_length - pos
				if remaining <= duration:
					break
			await get_tree().process_frame
		
	if not is_transitioning:
		return
	
	# Start crossfading
	_run_deck_transition(active_player, incoming_player, transition_type, t_trigger, force_immediate)


func _run_deck_transition(outgoing: AudioStreamPlayer, incoming: AudioStreamPlayer, transition_type: String, target_trigger_time: float = -1.0, force_immediate: bool = false) -> void:
	if _pitch_ramp_tween and _pitch_ramp_tween.is_valid():
		_pitch_ramp_tween.kill()
		
	player = incoming
	var dsp = dsp_a if incoming == player_a else dsp_b
	current_track_length = dsp.get_duration()
	length_changed.emit(current_track_length)
	
	_outgoing_player = outgoing
	_incoming_player = incoming
	var next_href = current_playlist[current_track_index]
	var current_href = current_playlist[current_track_index - 1] if current_track_index > 0 else ""
	_outgoing_href = current_href
	_incoming_href = next_href
	
	_last_transition_info = {
		"track_a": current_href,
		"track_b": next_href,
		"transition_type": transition_type,
		"finished": false,
		"end_time_msec": -1,
		"logged": false
	}
	
	var duration = get_transition_duration(transition_type, current_href, next_href)
	_active_transition_duration = duration
	_transition_original_duration = duration
	_transition_elapsed = 0.0
	crossfader = -1.0
	crossfader = 0.0
	for b in range(6):
		_clipping_attenuation_db[b] = 0.0
		
	var out_bus = outgoing.bus
	var in_bus = incoming.bus
	var out_bus_idx = AudioServer.get_bus_index(out_bus)
	var in_bus_idx = AudioServer.get_bus_index(in_bus)
	
	_reset_bus_effects(out_bus_idx)
	_reset_bus_effects(in_bus_idx)
	
	AudioServer.set_bus_volume_db(out_bus_idx, 0.0)
	AudioServer.set_bus_volume_db(in_bus_idx, -60.0)
	
	var cue_in = _get_track_cue_in(next_href)
	
	var outgoing_dsp = dsp_a if outgoing == player_a else dsp_b
	var t_trigger = target_trigger_time
	var has_beat_metadata = target_trigger_time >= 0.0
	var metadata_service = get_node_or_null("/root/MetadataService")
	
	if not has_beat_metadata and is_instance_valid(metadata_service) and not current_href.is_empty():
		var meta_out = metadata_service.get_cached_metadata(current_href)
		var meta_in = metadata_service.get_cached_metadata(next_href)
		if not meta_out.is_empty() and not meta_in.is_empty():
			var downbeats_out = meta_out.get("downbeats", [])
			var downbeats_in = meta_in.get("downbeats", [])
			var bpm_out = meta_out.get("bpm", 120.0)
			
			if not downbeats_out.is_empty() and not downbeats_in.is_empty():
				var segments = meta_out.get("segments", {})
				var outro = segments.get("outro", [])
				var outro_start = outro[0] if not outro.is_empty() else _get_track_cue_out(current_href, current_track_length) - 30.0
				
				if force_immediate:
					var pos = outgoing_dsp.get_playback_position()
					var next_downbeat = -1.0
					for db in downbeats_out:
						if db > pos:
							next_downbeat = db
							break
					if next_downbeat == -1.0:
						var last_db = downbeats_out[-1]
						var bar_duration = 240.0 / (bpm_out if bpm_out > 0.0 else 120.0)
						var diff = pos - last_db
						var bars_passed = ceil(diff / bar_duration)
						if bars_passed <= 0:
							bars_passed = 1
						next_downbeat = last_db + (bars_passed * bar_duration)
					t_trigger = next_downbeat
					has_beat_metadata = true
				else:
					t_trigger = get_aligned_trigger_time(outgoing_dsp.get_playback_position(), bpm_out, downbeats_out, cue_in, downbeats_in, outro_start)
					has_beat_metadata = true
				
	if t_trigger < 0.0:
		t_trigger = outgoing_dsp.get_playback_position()
				
	if has_beat_metadata:
		# Initialize PLL State
		var meta_out = metadata_service.get_cached_metadata(current_href) if is_instance_valid(metadata_service) else {}
		var meta_in = metadata_service.get_cached_metadata(next_href) if is_instance_valid(metadata_service) else {}
		var downbeats_in = meta_in.get("downbeats", []) if not meta_in.is_empty() else []
		var bpm_out = meta_out.get("bpm", 120.0) if not meta_out.is_empty() else 120.0
		
		var first_downbeat_in = cue_in
		for db in downbeats_in:
			if db >= cue_in:
				first_downbeat_in = db
				break
				
		if force_immediate:
			cue_in = first_downbeat_in
				
		var target_db_out = t_trigger + (first_downbeat_in - cue_in)
		var beat_grid_out = meta_out.get("beat_grid", []) if not meta_out.is_empty() else []
		var beat_grid_in = meta_in.get("beat_grid", []) if not meta_in.is_empty() else []
		
		_pll_db_in_idx = _find_closest_beat_index(first_downbeat_in, beat_grid_in)
		_pll_db_out_idx = _find_closest_beat_index(target_db_out, beat_grid_out)
		_transition_elapsed_time = 0.0
		
		if transition_type == "Tempo Morph":
			_pll_delay = min(3.0, duration * 0.5)
			var bpm_in = meta_in.get("bpm", 120.0) if not meta_in.is_empty() else 120.0
			var target_bpm = (bpm_out + bpm_in) / 2.0
			_incoming_base_pitch = target_bpm / bpm_in
		else:
			_pll_delay = 0.0
			_incoming_base_pitch = 1.0
			
		current_pll_adjustment = 0.0
		current_pll_phase_error_ms = 0.0
		_pll_cc_timer = 0.0
		_last_cc_offset = 0.0


	incoming.play(cue_in)
	if not is_playing:
		incoming.stream_paused = true
	
	duration = get_transition_duration(transition_type, current_href, next_href)
	_active_transition_duration = duration
	_transition_original_duration = duration
	
	if duration <= 0.0:
		outgoing.stop()
		active_player = incoming
		player = active_player
		
		_reset_bus_effects(out_bus_idx)
		_reset_bus_effects(in_bus_idx)
		AudioServer.set_bus_volume_db(in_bus_idx, 0.0)
		
		is_transitioning = false
		if not _last_transition_info.is_empty():
			_last_transition_info["finished"] = true
			_last_transition_info["end_time_msec"] = Time.get_ticks_msec()
			_start_success_timer(_last_transition_info.duplicate())
			
		_outgoing_player = null
		_incoming_player = null
		_outgoing_href = ""
		_incoming_href = ""
		_pll_db_in_idx = -1
		_pll_db_out_idx = -1
		current_pll_adjustment = 0.0
		current_pll_phase_error_ms = 0.0
		_pll_cc_timer = 0.0
		_last_cc_offset = 0.0
		
		_active_transition_duration = 0.0
		_transition_elapsed = 0.0
		crossfader = 0.0
		for b in range(6):
			_clipping_attenuation_db[b] = 0.0
			
		var dsp_final = dsp_a if incoming == player_a else dsp_b
		current_track_length = dsp_final.get_duration()
		_outro_load_triggered = false
		transition_completed.emit(current_playlist[current_track_index])
		_update_prefetching()
		update_preferred_type()
		_update_upcoming_transition()
		_record_history(current_playlist[current_track_index])
		_navigating_history = false
		return
	
	if active_tween and active_tween.is_valid():
		active_tween.kill()

	var outgoing_has_vocals = false
	var incoming_has_vocals = false
	if is_instance_valid(metadata_service) and not current_href.is_empty():
		var meta_out = metadata_service.get_cached_metadata(current_href)
		var meta_in = metadata_service.get_cached_metadata(next_href)
		outgoing_has_vocals = meta_out.get("vocal_presence", false)
		incoming_has_vocals = meta_in.get("vocal_presence", false)
	var apply_mid_cut = outgoing_has_vocals and incoming_has_vocals

	# Transition-specific volume envelopes and dynamics
	if transition_type == "Bass Swap":
		var half_duration = duration / 2.0
		
		# EQ setup: cut incoming bass initially
		_transition_eq_gains[in_bus][0] = -40.0
		_transition_eq_gains[in_bus][1] = -40.0
		_apply_final_eq_gains()
		
		var out_eq = AudioServer.get_bus_effect(out_bus_idx, 0) as AudioEffectEQ
		
		active_tween = create_tween()
		if not is_playing:
			active_tween.pause()
			
		# --- FIRST HALF ---
		# Incoming volume fade in
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, half_duration).set_trans(Tween.TRANS_SINE)
		
		# --- SECOND HALF ---
		# Outgoing volume fade out starts after first half finishes
		var swap_duration = 0.5 if duration > 1.0 else duration * 0.06
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(out_bus_idx, val)
		, 0.0, -60.0, half_duration).set_trans(Tween.TRANS_SINE)
		
		# Outgoing bass cut (parallel to outgoing volume fade out)
		if out_eq:
			active_tween.parallel().tween_method(func(val: float):
				_transition_eq_gains[out_bus][0] = val
				_transition_eq_gains[out_bus][1] = val
				_apply_final_eq_gains()
			, 0.0, -40.0, swap_duration)
			
		# Incoming bass boost (parallel to outgoing volume fade out)
		var in_eq = AudioServer.get_bus_effect(in_bus_idx, 0) as AudioEffectEQ
		if in_eq:
			active_tween.parallel().tween_method(func(val: float):
				_transition_eq_gains[in_bus][0] = val
				_transition_eq_gains[in_bus][1] = val
				_apply_final_eq_gains()
			, -40.0, 0.0, swap_duration)
			
		if apply_mid_cut and out_eq:
			active_tween.parallel().tween_method(func(val: float):
				_transition_eq_gains[out_bus][2] = val
				_transition_eq_gains[out_bus][3] = val
				_apply_final_eq_gains()
			, 0.0, -24.0, half_duration)
		
	elif transition_type == "Filter Sweep":
		active_tween = create_tween().set_parallel(true)
		if not is_playing:
			active_tween.pause()
			
		var fade_duration = duration * 3.0 / 8.0
		var fade_delay = duration * 5.0 / 8.0
		# Outgoing: keep at 0.0 dB for fade_delay, then fade to -60.0 dB over fade_duration
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(out_bus_idx, val)
		, 0.0, -60.0, fade_duration).set_trans(Tween.TRANS_SINE).set_delay(fade_delay)
		
		# Incoming: fade in to 0.0 dB over fade_duration, then stay at 0.0 dB
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, fade_duration).set_trans(Tween.TRANS_SINE)
		
		# Filters setup
		var out_lp = AudioServer.get_bus_effect(out_bus_idx, 1) as AudioEffectLowPassFilter
		var in_hp = AudioServer.get_bus_effect(in_bus_idx, 2) as AudioEffectHighPassFilter
		
		# Outgoing sweep lowpass cutoff down from 20000Hz to 150Hz over the entire duration
		if out_lp:
			active_tween.tween_property(out_lp, "cutoff_hz", 150.0, duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
		
		# Incoming sweep highpass cutoff down from 2000Hz to 10Hz over the entire duration
		if in_hp:
			in_hp.cutoff_hz = 2000.0
			active_tween.tween_property(in_hp, "cutoff_hz", 10.0, duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
			
		if apply_mid_cut:
			var out_eq = AudioServer.get_bus_effect(out_bus_idx, 0) as AudioEffectEQ
			if out_eq:
				active_tween.tween_method(func(val: float):
					_transition_eq_gains[out_bus][2] = val
					_transition_eq_gains[out_bus][3] = val
					_apply_final_eq_gains()
				, 0.0, -24.0, duration * 0.5)
		
	elif transition_type == "Echo Out":
		var half_duration = duration / 2.0
		
		# Setup delay on outgoing bus
		var out_delay = AudioServer.get_bus_effect(out_bus_idx, 3) as AudioEffectDelay
		if out_delay:
			out_delay.dry = 1.0
			out_delay.feedback_delay_ms = 350.0
			out_delay.feedback_level_db = -10.0
			out_delay.tap1_level_db = -6.0
			out_delay.tap2_level_db = -12.0
			
		active_tween = create_tween()
		if not is_playing:
			active_tween.pause()
			
		# --- FIRST HALF ---
		# Incoming: bus volume fades from -60.0 dB to 0.0 dB over the first half
		active_tween.tween_method(func(val: float):
			print("ECHO OUT TWEENING VOLUME: ", val)
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, half_duration).set_trans(Tween.TRANS_SINE)
		
		# --- SECOND HALF ---
		# At midpoint, cut dry signal of outgoing deck
		var cut_duration = 0.1 if duration > 1.0 else duration * 0.02
		if out_delay:
			active_tween.tween_method(func(val: float):
				print("ECHO OUT TWEENING DRY: ", val)
				out_delay.dry = val
			, 1.0, 0.0, cut_duration)
			
		# --- TAIL DECAY ---
		var remaining = duration - half_duration - cut_duration
		if remaining > 0.0:
			active_tween.tween_interval(remaining)
			
		if apply_mid_cut:
			var out_eq = AudioServer.get_bus_effect(out_bus_idx, 0) as AudioEffectEQ
			if out_eq:
				active_tween.parallel().tween_method(func(val: float):
					_transition_eq_gains[out_bus][2] = val
					_transition_eq_gains[out_bus][3] = val
					_apply_final_eq_gains()
				, 0.0, -24.0, half_duration)
		
	elif transition_type == "Reverb Freeze":
		var half_duration = duration / 2.0
		
		# Outgoing Reverb setup
		var out_reverb = AudioServer.get_bus_effect(out_bus_idx, 4) as AudioEffectReverb
		if out_reverb:
			out_reverb.wet = 0.0
			out_reverb.dry = 1.0
			out_reverb.room_size = 0.95
			out_reverb.damping = 0.1
			out_reverb.predelay_feedback = 0.3
			
		active_tween = create_tween()
		if not is_playing:
			active_tween.pause()
			
		# --- FIRST HALF ---
		# Incoming: fade in to 0.0 dB over first half
		active_tween.tween_method(func(val: float):
			print("REVERB FREEZE TWEENING VOLUME: ", val)
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, half_duration).set_trans(Tween.TRANS_SINE)
		
		# Ramp up wet mix over first half (runs in parallel with incoming fade in)
		if out_reverb:
			active_tween.parallel().tween_method(func(val: float):
				print("REVERB FREEZE TWEENING WET: ", val)
				out_reverb.wet = val
			, 0.0, 1.0, half_duration)
			
		# Scale down outgoing bus volume during first half to prevent clipping / volume swell as wet ramps up
		active_tween.parallel().tween_method(func(val: float):
			print("REVERB FREEZE TWEENING OUTGOING VOLUME (FIRST HALF): ", val)
			AudioServer.set_bus_volume_db(out_bus_idx, val)
		, 0.0, -6.0, half_duration).set_trans(Tween.TRANS_SINE)
		
		# --- MIDPOINT CALLBACK ---
		# Stop outgoing player to freeze the audio input into the reverb tail.
		# If the outgoing player has already stopped naturally (stream ended before
		# the midpoint), abort the second-half gracefully — there is no signal to
		# freeze, so continuing the fade-out serves no purpose.
		active_tween.tween_callback(func():
			if is_instance_valid(outgoing) and outgoing.playing:
				print("REVERB FREEZE MIDPOINT: STOPPING OUTGOING")
				outgoing.stop()
			else:
				print("REVERB FREEZE MIDPOINT: Outgoing already silent — aborting second half.")
				if active_tween and active_tween.is_valid():
					active_tween.kill()
				AudioServer.set_bus_volume_db(out_bus_idx, -60.0)
				if out_reverb:
					out_reverb.wet = 0.0
					out_reverb.dry = 1.0
		)
		
		# --- SECOND HALF ---
		# Fade out outgoing bus volume to -60.0 dB over the second half
		# (starts at midpoint, runs for half_duration)
		active_tween.tween_method(func(val: float):
			print("REVERB FREEZE TWEENING OUTGOING VOLUME (SECOND HALF): ", val)
			AudioServer.set_bus_volume_db(out_bus_idx, val)
		, -6.0, -60.0, half_duration).set_trans(Tween.TRANS_SINE)
		
		# At midpoint, cut dry mix instantly to freeze the tail
		var cut_duration = 0.1 if duration > 1.0 else duration * 0.02
		if out_reverb:
			active_tween.parallel().tween_method(func(val: float):
				print("REVERB FREEZE TWEENING DRY: ", val)
				out_reverb.dry = val
			, 1.0, 0.0, cut_duration)
			
		if apply_mid_cut:
			var out_eq = AudioServer.get_bus_effect(out_bus_idx, 0) as AudioEffectEQ
			if out_eq:
				active_tween.parallel().tween_method(func(val: float):
					_transition_eq_gains[out_bus][2] = val
					_transition_eq_gains[out_bus][3] = val
					_apply_final_eq_gains()
				, 0.0, -24.0, half_duration)
			
	elif transition_type == "Tempo Morph":
		active_tween = create_tween().set_parallel(true)
		if not is_playing:
			active_tween.pause()
			
		var bpm_out = 120.0
		var bpm_in = 120.0
		
		if is_instance_valid(metadata_service):
			current_href = current_playlist[current_track_index - 1] if current_track_index > 0 else ""
			next_href = current_playlist[current_track_index]
			if not current_href.is_empty():
				var meta_out = metadata_service.get_cached_metadata(current_href)
				var meta_in = metadata_service.get_cached_metadata(next_href)
				if not meta_out.is_empty() and meta_out.get("bpm", 0.0) > 0.0:
					bpm_out = meta_out.get("bpm", 120.0)
				if not meta_in.is_empty() and meta_in.get("bpm", 0.0) > 0.0:
					bpm_in = meta_in.get("bpm", 120.0)
					
		var target_bpm = (bpm_out + bpm_in) / 2.0
		var pitch_out = target_bpm / bpm_out
		var pitch_in = target_bpm / bpm_in
		
		# Ramp speed scales over first 50% of duration (max 3.0s) for a smoother morph
		var ramp_time = min(3.0, duration * 0.5)
		var speed_scale_out_prop = "_speed_scale_a" if outgoing == player_a else "_speed_scale_b"
		var speed_scale_in_prop = "_speed_scale_a" if incoming == player_a else "_speed_scale_b"
		active_tween.tween_property(self, speed_scale_out_prop, pitch_out, ramp_time)
		active_tween.tween_property(self, speed_scale_in_prop, pitch_in, ramp_time)
		
		# Volume crossfades over entire duration
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(out_bus_idx, val)
		, 0.0, -60.0, duration).set_trans(Tween.TRANS_SINE)
		
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, duration).set_trans(Tween.TRANS_SINE)
		
		if apply_mid_cut:
			var out_eq = AudioServer.get_bus_effect(out_bus_idx, 0) as AudioEffectEQ
			if out_eq:
				active_tween.tween_method(func(val: float):
					_transition_eq_gains[out_bus][2] = val
					_transition_eq_gains[out_bus][3] = val
					_apply_final_eq_gains()
				, 0.0, -24.0, duration * 0.5)
		
	else:
		# Standard Crossfade: Standard full-duration volume crossfades
		active_tween = create_tween().set_parallel(true)
		if not is_playing:
			active_tween.pause()
			
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(out_bus_idx, val)
		, 0.0, -60.0, duration).set_trans(Tween.TRANS_SINE)
		
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, duration).set_trans(Tween.TRANS_SINE)
		
		if apply_mid_cut:
			var out_eq = AudioServer.get_bus_effect(out_bus_idx, 0) as AudioEffectEQ
			if out_eq:
				active_tween.tween_method(func(val: float):
					_transition_eq_gains[out_bus][2] = val
					_transition_eq_gains[out_bus][3] = val
					_apply_final_eq_gains()
				, 0.0, -24.0, duration * 0.5)
		
	await active_tween.finished
	
	# Switch roles
	outgoing.stop()
	active_player = incoming
	player = active_player
	
	# Capture morphed speed scale before reset overrides it to 1.0
	var incoming_speed_scale_prop = "_speed_scale_a" if incoming == player_a else "_speed_scale_b"
	var current_speed_scale_in = get(incoming_speed_scale_prop)
	
	_reset_bus_effects(out_bus_idx)
	_reset_bus_effects(in_bus_idx)
	
	if transition_type == "Tempo Morph":
		set(incoming_speed_scale_prop, current_speed_scale_in)
		var ramp_duration = 6.0 if duration > 1.0 else 0.1
		_pitch_ramp_tween = create_tween()
		_pitch_ramp_tween.tween_property(self, incoming_speed_scale_prop, 1.0, ramp_duration).set_trans(Tween.TRANS_SINE)
	
	is_transitioning = false
	
	if not _last_transition_info.is_empty():
		_last_transition_info["finished"] = true
		_last_transition_info["end_time_msec"] = Time.get_ticks_msec()
		_start_success_timer(_last_transition_info.duplicate())
	
	# Clear PLL state
	_outgoing_player = null
	_incoming_player = null
	_outgoing_href = ""
	_incoming_href = ""
	_pll_db_in_idx = -1
	_pll_db_out_idx = -1
	current_pll_adjustment = 0.0
	current_pll_phase_error_ms = 0.0
	_pll_cc_timer = 0.0
	_last_cc_offset = 0.0
	
	_active_transition_duration = 0.0
	_transition_elapsed = 0.0
	crossfader = 0.0
	for b in range(6):
		_clipping_attenuation_db[b] = 0.0
	
	var dsp_final = dsp_a if incoming == player_a else dsp_b
	current_track_length = dsp_final.get_duration()
	_outro_load_triggered = false
	transition_completed.emit(current_playlist[current_track_index])
	_update_prefetching()
	update_preferred_type()
	_update_upcoming_transition()
	_record_history(current_playlist[current_track_index])
	_navigating_history = false


func _linear_to_eq_db(linear_gain: float) -> float:
	if linear_gain <= 0.0001:
		return -40.0
	return clamp(linear_to_db(linear_gain), -40.0, 0.0)

func _update_subtractive_eq() -> void:
	if not is_transitioning or not _outgoing_player or not _incoming_player:
		return
		
	var out_bus_name = _outgoing_player.bus
	var in_bus_name = _incoming_player.bus
	
	# Determine crossfade width in fader progress X [0, 1]
	var width: float = 0.1
	if _active_transition_duration > 0.0:
		width = clamp(subtractive_eq_crossfade_duration / _active_transition_duration, 0.01, 1.0)
	
	var half_width: float = width / 2.0
	var low_bound: float = 0.5 - half_width
	var high_bound: float = 0.5 + half_width
	
	var bass_out_linear: float = 0.0
	var bass_in_linear: float = 0.0
	
	if crossfader <= low_bound:
		bass_out_linear = 1.0
		bass_in_linear = 0.0
	elif crossfader >= high_bound:
		bass_out_linear = 0.0
		bass_in_linear = 1.0
	else:
		# Smooth linear crossfade inside the window
		var t: float = (crossfader - low_bound) / width
		bass_out_linear = 1.0 - t
		bass_in_linear = t
		
	var bass_out_db = _linear_to_eq_db(bass_out_linear)
	var bass_in_db = _linear_to_eq_db(bass_in_linear)
	
	_transition_eq_gains[out_bus_name][0] = bass_out_db
	_transition_eq_gains[out_bus_name][1] = bass_out_db
	
	_transition_eq_gains[in_bus_name][0] = bass_in_db
	_transition_eq_gains[in_bus_name][1] = bass_in_db
	
	_apply_final_eq_gains()

func _apply_final_eq_gains() -> void:
	if not _outgoing_player or not _incoming_player:
		return
		
	var out_bus_idx = AudioServer.get_bus_index(_outgoing_player.bus)
	var in_bus_idx = AudioServer.get_bus_index(_incoming_player.bus)
	
	if out_bus_idx == -1 or in_bus_idx == -1:
		return
		
	var out_eq = AudioServer.get_bus_effect(out_bus_idx, 0) as AudioEffectEQ
	var in_eq = AudioServer.get_bus_effect(in_bus_idx, 0) as AudioEffectEQ
	
	var out_bus_name = _outgoing_player.bus
	var in_bus_name = _incoming_player.bus
	
	if out_eq:
		for b in range(6):
			var gain = _transition_eq_gains[out_bus_name][b]
			var atten_db = _clipping_attenuation_db[b]
			out_eq.set_band_gain_db(b, clamp(gain + atten_db, -40.0, 0.0))
			
	if in_eq:
		for b in range(6):
			var gain = _transition_eq_gains[in_bus_name][b]
			in_eq.set_band_gain_db(b, clamp(gain, -40.0, 0.0))

func _calculate_chunk_rms(chunk: PackedVector2Array, deck_name: String) -> Dictionary:
	var num_frames = chunk.size()
	if num_frames == 0:
		return { "low": 0.0, "mid": 0.0, "high": 0.0 }
		
	var state = _filter_states[deck_name]
	
	var sum_low_sq := 0.0
	var sum_mid_sq := 0.0
	var sum_high_sq := 0.0
	
	for i in range(num_frames):
		var frame = chunk[i]
		
		# Left channel
		var sample_l = frame.x
		state.low_l += 0.0344 * (sample_l - state.low_l)
		state.mid_low_l += 0.363 * (sample_l - state.mid_low_l)
		var low_l_val = state.low_l
		var mid_l_val = state.mid_low_l - state.low_l
		var high_l_val = sample_l - state.mid_low_l
		
		# Right channel
		var sample_r = frame.y
		state.low_r += 0.0344 * (sample_r - state.low_r)
		state.mid_low_r += 0.363 * (sample_r - state.mid_low_r)
		var low_r_val = state.low_r
		var mid_r_val = state.mid_low_r - state.low_r
		var high_r_val = sample_r - state.mid_low_r
		
		sum_low_sq += (low_l_val * low_l_val + low_r_val * low_r_val) * 0.5
		sum_mid_sq += (mid_l_val * mid_l_val + mid_r_val * mid_r_val) * 0.5
		sum_high_sq += (high_l_val * high_l_val + high_r_val * high_r_val) * 0.5
		
	var rms_low = sqrt(sum_low_sq / num_frames)
	var rms_mid = sqrt(sum_mid_sq / num_frames)
	var rms_high = sqrt(sum_high_sq / num_frames)
	
	return {
		"low": rms_low,
		"mid": rms_mid,
		"high": rms_high
	}

func _process_clipping_prevention() -> void:
	if not is_transitioning or not _outgoing_player or not _incoming_player:
		for b in range(6):
			_clipping_attenuation_db[b] = 0.0
		return
		
	var out_bus = _outgoing_player.bus
	var in_bus = _incoming_player.bus
	
	var raw_rms_out = _last_rms[out_bus]
	var raw_rms_in = _last_rms[in_bus]
	
	var vol_out_linear = db_to_linear(AudioServer.get_bus_volume_db(AudioServer.get_bus_index(out_bus)))
	var vol_in_linear = db_to_linear(AudioServer.get_bus_volume_db(AudioServer.get_bus_index(in_bus)))
	
	var threshold_linear = rms_reference * db_to_linear(2.0)
	
	for band_type in ["low", "mid", "high"]:
		var rms_out_band = raw_rms_out[band_type] * vol_out_linear
		var rms_in_band = raw_rms_in[band_type] * vol_in_linear
		
		var band_indices: Array[int] = []
		if band_type == "low":
			band_indices = [0, 1]
		elif band_type == "mid":
			band_indices = [2, 3]
		else:
			band_indices = [4, 5]
			
		var eq_gain_out_db = _transition_eq_gains[out_bus][band_indices[0]]
		var eq_gain_in_db = _transition_eq_gains[in_bus][band_indices[0]]
		
		rms_out_band *= db_to_linear(eq_gain_out_db)
		rms_in_band *= db_to_linear(eq_gain_in_db)
		
		var combined_rms = rms_out_band + rms_in_band
		
		var attenuation_db = 0.0
		if combined_rms > threshold_linear:
			var excess = combined_rms - threshold_linear
			var target_rms_out = clamp(rms_out_band - excess, 0.0, rms_out_band)
			var atten_linear = 1.0
			if rms_out_band > 0.0001:
				atten_linear = target_rms_out / rms_out_band
			attenuation_db = linear_to_db(atten_linear)
			attenuation_db = clamp(attenuation_db, -40.0, 0.0)
			
		for b in band_indices:
			_clipping_attenuation_db[b] = attenuation_db
			
	_apply_final_eq_gains()


func _reset_bus_effects(bus_idx: int) -> void:
	AudioServer.set_bus_volume_db(bus_idx, 0.0)
	var bus_name = "DeckA" if bus_idx == AudioServer.get_bus_index("DeckA") else "DeckB"
	if _transition_eq_gains.has(bus_name):
		for b in range(6):
			_transition_eq_gains[bus_name][b] = 0.0
	var eq = AudioServer.get_bus_effect(bus_idx, 0) as AudioEffectEQ
	if eq:
		for b in range(6):
			eq.set_band_gain_db(b, 0.0)
	var lp = AudioServer.get_bus_effect(bus_idx, 1) as AudioEffectLowPassFilter
	if lp:
		lp.cutoff_hz = 20000.0
	var hp = AudioServer.get_bus_effect(bus_idx, 2) as AudioEffectHighPassFilter
	if hp:
		hp.cutoff_hz = 10.0
		
	var delay = AudioServer.get_bus_effect(bus_idx, 3) as AudioEffectDelay
	if delay:
		delay.dry = 1.0
		delay.feedback_delay_ms = 300.0
		delay.feedback_level_db = -60.0
		delay.tap1_level_db = -60.0
		delay.tap2_level_db = -60.0
		
	var reverb = AudioServer.get_bus_effect(bus_idx, 4) as AudioEffectReverb
	if reverb:
		reverb.wet = 0.0
		reverb.dry = 1.0
		reverb.room_size = 0.5
		reverb.damping = 0.5
		reverb.predelay_feedback = 0.0
		
	if player_a and player_a.bus == AudioServer.get_bus_name(bus_idx):
		player_a.pitch_scale = 1.0
		_speed_scale_a = 1.0
	elif player_b and player_b.bus == AudioServer.get_bus_name(bus_idx):
		player_b.pitch_scale = 1.0
		_speed_scale_b = 1.0

## Generates a play queue sorted by harmonic and energy compatibility and starts playback
func play_harmonic_shuffle(start_track_href: String = "") -> void:
	if _pitch_ramp_tween and _pitch_ramp_tween.is_valid():
		_pitch_ramp_tween.kill()
		
	if current_playlist.is_empty():
		return
		
	var analyzed_tracks_meta := {}
	var unanalyzed_tracks := []
	var metadata_service = get_node_or_null("/root/MetadataService")
	
	for href in current_playlist:
		var meta := {}
		if is_instance_valid(metadata_service):
			meta = metadata_service.get_cached_metadata(href)
			
		if not meta.is_empty() and meta.get("bpm", 0.0) > 0.0:
			analyzed_tracks_meta[href] = meta
		else:
			unanalyzed_tracks.append(href)
		
	# Determine starting track
	var start_href = start_track_href
	if start_href.is_empty() and current_track_index >= 0 and current_track_index < current_playlist.size():
		start_href = current_playlist[current_track_index]
	elif start_href.is_empty() and not current_playlist.is_empty():
		if not analyzed_tracks_meta.is_empty():
			start_href = analyzed_tracks_meta.keys()[randi() % analyzed_tracks_meta.size()]
		else:
			start_href = current_playlist[randi() % current_playlist.size()]
		
	var path: Array[String] = []
	if not analyzed_tracks_meta.is_empty():
		var DJPathfinderClass = load("res://scripts/services/dj_pathfinder.gd")
		var analyzed_path = DJPathfinderClass.generate_mood_path(analyzed_tracks_meta, start_href)
		path = analyzed_path
		# Append unanalyzed tracks to the end
		for href in unanalyzed_tracks:
			if not path.has(href):
				path.append(href)
	else:
		# Fallback to simple shuffle if nothing is analyzed yet
		path = current_playlist.duplicate()
		path.shuffle()
		
	if not path.is_empty():
		current_playlist = path
		current_track_index = 0
		if start_href != "" and current_playlist.has(start_href):
			current_track_index = current_playlist.find(start_href)
		is_transitioning = false
		_record_history(current_playlist[current_track_index])
		_navigating_history = false
		_load_and_stream_remote_file(current_playlist[current_track_index], active_player)

func _start_debounce() -> void:
	var track_href = current_playlist[current_track_index]
	track_changed.emit(track_href.get_file().uri_decode())
	_debounce_timer.start(3.0)

func _on_debounce_timeout() -> void:
	if current_playlist.is_empty() or current_track_index == -1:
		return
		
	var target_href = current_playlist[current_track_index]
	if smart_mixing_enabled and is_playing and active_player.playing and not is_transitioning:
		upcoming_track_override = target_href
		start_transition(true)
	else:
		is_transitioning = false
		_record_history(target_href)
		_navigating_history = false
		_load_and_stream_remote_file(target_href, active_player, is_playing)

func scroll_track(value) -> void:
	var dsp = dsp_a if active_player == player_a else dsp_b
	dsp.seek_pos(value)
	var playback = active_player.get_stream_playback()
	if playback:
		if playback.has_method("clear_buffer"):
			playback.clear_buffer()

func _on_deck_finished(finished_player: AudioStreamPlayer) -> void:
	if _block_finished_signal:
		return
	if finished_player != active_player:
		# Ignore finished signals from the outgoing player during a transition
		return
		
	if is_transitioning:
		return
		
	# Loop playlist sequence
	if smart_mixing_enabled:
		start_transition()
	else:
		var next_href = get_next_track_href()
		if not next_href.is_empty():
			current_track_index = current_playlist.find(next_href)
			_record_history(next_href)
			_navigating_history = false
			_load_and_stream_remote_file(next_href, active_player, is_playing)

func _get_track_cue_in(href_path: String) -> float:
	var metadata_service = get_node_or_null("/root/MetadataService")
	if is_instance_valid(metadata_service):
		var meta = metadata_service.get_cached_metadata(href_path)
		if not meta.is_empty():
			return meta.get("cue_in", 0.0)
	return 0.0

func _get_track_cue_out(href_path: String, fallback: float) -> float:
	var metadata_service = get_node_or_null("/root/MetadataService")
	if is_instance_valid(metadata_service):
		var meta = metadata_service.get_cached_metadata(href_path)
		if not meta.is_empty():
			return meta.get("cue_out", fallback)
	return fallback

func _get_track_lufs(href_path: String) -> float:
	var metadata_service = get_node_or_null("/root/MetadataService")
	if is_instance_valid(metadata_service):
		var meta = metadata_service.get_cached_metadata(href_path)
		if not meta.is_empty():
			return meta.get("lufs", -14.0)
	return -14.0

func _find_closest_beat_index(pos: float, beat_grid: Array) -> int:
	if beat_grid.is_empty():
		return -1
	# Binary search for the closest beat
	var low = 0
	var high = beat_grid.size() - 1
	while low < high:
		var mid = (low + high) / 2
		if beat_grid[mid] < pos:
			low = mid + 1
		else:
			high = mid
			
	var closest = low
	if low > 0 and abs(beat_grid[low - 1] - pos) < abs(beat_grid[low] - pos):
		closest = low - 1
	return closest

func get_bars_and_beats(pos: float, beat_grid: Array) -> Dictionary:
	var idx = _find_closest_beat_index(pos, beat_grid)
	if idx == -1:
		return {"bar": 1, "beat": 1, "beat_index": -1}
	var bar = int(idx / 4) + 1
	var beat = (idx % 4) + 1
	return {"bar": bar, "beat": beat, "beat_index": idx}

func get_aligned_trigger_time(pos_out: float, bpm_out: float, downbeats_out: Array, cue_in_in: float, downbeats_in: Array, outro_start: float = -1.0, transients_out: Array = [], transients_in: Array = []) -> float:
	var first_downbeat_in = cue_in_in
	for db in downbeats_in:
		if db >= cue_in_in:
			first_downbeat_in = db
			break
			
	var time_to_beat = first_downbeat_in - cue_in_in
	var target_db_out = -1.0
	
	if outro_start >= 0.0:
		# Trigger transitions strictly on the first downbeat of the outro segment that lands on a standard 8-bar loop boundary.
		# Note: downbeats_out contains downbeat timestamps (one per bar).
		# An 8-bar loop boundary downbeat is downbeats_out[k] where k % 8 == 0.
		for k in range(downbeats_out.size()):
			if k % 8 == 0 and downbeats_out[k] - time_to_beat >= pos_out and downbeats_out[k] >= outro_start:
				target_db_out = downbeats_out[k]
				break
				
		# Fallback if no such downbeat exists in the array
		if target_db_out == -1.0 and not downbeats_out.is_empty():
			for k in range(downbeats_out.size()):
				if k % 8 == 0 and downbeats_out[k] - time_to_beat >= pos_out:
					target_db_out = downbeats_out[k]
					break
	else:
		# Standard downbeat alignment (original behavior)
		for db in downbeats_out:
			if db - time_to_beat >= pos_out:
				target_db_out = db
				break
			
	if target_db_out == -1.0:
		if not downbeats_out.is_empty():
			var last_db = downbeats_out[-1]
			var bar_duration = 240.0 / (bpm_out if bpm_out > 0.0 else 120.0)
			
			if outro_start >= 0.0:
				var last_idx = downbeats_out.size() - 1
				var next_8_bar_idx = last_idx
				if next_8_bar_idx % 8 != 0:
					next_8_bar_idx = last_idx + (8 - (last_idx % 8))
				var temp_db = last_db + (next_8_bar_idx - last_idx) * bar_duration
				while temp_db - time_to_beat < pos_out:
					temp_db += 8.0 * bar_duration
				target_db_out = temp_db
			else:
				var temp_db = last_db
				while temp_db - time_to_beat < pos_out:
					temp_db += bar_duration
				target_db_out = temp_db
		else:
			var bar_duration = 240.0 / (bpm_out if bpm_out > 0.0 else 120.0)
			var temp_db = 0.0
			if outro_start >= 0.0:
				while temp_db - time_to_beat < pos_out or temp_db < outro_start:
					temp_db += 8.0 * bar_duration
			else:
				while temp_db - time_to_beat < pos_out:
					temp_db += bar_duration
			target_db_out = temp_db
			
	var t_trigger = target_db_out - time_to_beat
	
	# Apply Transient Avoidance
	var safety_margin := 0.5
	var beat_duration = 60.0 / (bpm_out if bpm_out > 0.0 else 120.0)
	var bar_duration = 4.0 * beat_duration
	
	var original_t_trigger = t_trigger
	var shift_direction = -1 # Try shifting earlier first if possible
	if t_trigger - bar_duration < pos_out:
		shift_direction = 1
		
	var later_shift_count = 0
	var max_attempts = 4
	
	for attempt in range(max_attempts):
		var collision := false
		
		# Check outgoing track transients
		for t in transients_out:
			if absf(t - t_trigger) < safety_margin:
				collision = true
				break
				
		# Check incoming track transients (relative to cue_in_in)
		if not collision:
			for t in transients_in:
				var t_in_global = t_trigger + (t - cue_in_in)
				if absf(t_in_global - t_trigger) < safety_margin:
					collision = true
					break
					
		if not collision:
			break # Found a trigger time that is free of collisions!
			
		if shift_direction == -1:
			var next_t = t_trigger - bar_duration
			if next_t >= pos_out:
				t_trigger = next_t
			else:
				# Cannot go earlier anymore. Switch direction to later.
				shift_direction = 1
				later_shift_count = 1
				t_trigger = original_t_trigger + later_shift_count * bar_duration
		else:
			later_shift_count += 1
			t_trigger = original_t_trigger + later_shift_count * bar_duration
			
	return t_trigger

func calculate_pll_adjustment(pos_out: float, pos_in: float, beat_grid_out: Array, beat_grid_in: Array, base_pitch: float, cc_offset: float = 0.0) -> Dictionary:
	if _pll_db_in_idx == -1 or _pll_db_out_idx == -1:
		return {"adjustment": 0.0, "phase_error_ms": 0.0}
		
	var idx_out = _find_closest_beat_index(pos_out, beat_grid_out)
	if idx_out == -1:
		return {"adjustment": 0.0, "phase_error_ms": 0.0}
		
	var idx_in_expected = _pll_db_in_idx + (idx_out - _pll_db_out_idx)
	if idx_in_expected < 0 or idx_in_expected >= beat_grid_in.size():
		return {"adjustment": 0.0, "phase_error_ms": 0.0}
		
	var dt_out = beat_grid_out[idx_out] - pos_out
	var dt_in = beat_grid_in[idx_in_expected] - pos_in
	
	var phase_error = dt_in - dt_out - cc_offset
	var kp = 0.15
	var adjustment = phase_error * kp
	adjustment = clamp(adjustment, -0.005, 0.005)
	
	return {
		"adjustment": adjustment,
		"phase_error_ms": phase_error * 1000.0
	}

func _apply_pll_sync(delta: float) -> void:
	if not _outgoing_player or not _incoming_player:
		return
		
	var metadata_service = get_node_or_null("/root/MetadataService")
	if not is_instance_valid(metadata_service):
		return
		
	var meta_out = metadata_service.get_cached_metadata(_outgoing_href)
	var meta_in = metadata_service.get_cached_metadata(_incoming_href)
	
	var beat_grid_out = meta_out.get("beat_grid", [])
	var beat_grid_in = meta_in.get("beat_grid", [])
	
	if beat_grid_out.is_empty() or beat_grid_in.is_empty():
		return
		
	var dsp_out = dsp_a if _outgoing_player == player_a else dsp_b
	var pos_out = dsp_out.get_playback_position()
	
	var dsp_in = dsp_a if _incoming_player == player_a else dsp_b
	var pos_in = dsp_in.get_playback_position()
	
	_pll_cc_timer += delta
	if _pll_cc_timer >= 0.1:
		_pll_cc_timer = 0.0
		if is_instance_valid(dsp_out) and is_instance_valid(dsp_in):
			_last_cc_offset = dsp_out.get_cross_correlation_offset(dsp_in, pos_out, pos_in, 0.5)
	
	var pll_data = calculate_pll_adjustment(pos_out, pos_in, beat_grid_out, beat_grid_in, _incoming_base_pitch, _last_cc_offset)
	
	current_pll_adjustment = pll_data["adjustment"]
	current_pll_phase_error_ms = pll_data["phase_error_ms"]


func _feed_deck(p: AudioStreamPlayer, dsp: AudioDSP, base_speed_scale: float, is_incoming: bool) -> void:
	if not p or not p.playing or p.stream_paused:
		return
	var playback: AudioStreamGeneratorPlayback = p.get_stream_playback()
	if playback:
		var frames_available = playback.get_frames_available()
		if frames_available > 0:
			var speed_scale = base_speed_scale
			if is_incoming:
				speed_scale = base_speed_scale * (1.0 + current_pll_adjustment)
			var ratio = 1.0 / speed_scale
			var chunk = dsp.get_next_chunk(frames_available, ratio)
			if chunk.size() > 0:
				playback.push_buffer(chunk)
				var rms_levels = _calculate_chunk_rms(chunk, p.bus)
				_last_rms[p.bus] = rms_levels


func get_playback_position() -> float:
	var dsp = dsp_a if active_player == player_a else dsp_b
	if dsp:
		return dsp.get_playback_position()
	return 0.0

func _load_transition_history() -> Array:
	if not FileAccess.file_exists(_transition_history_file):
		return []
	var file = FileAccess.open(_transition_history_file, FileAccess.READ)
	if not file:
		return []
	var content = file.get_as_text()
	file.close()
	var json = JSON.new()
	if json.parse(content) == OK:
		if json.data is Array:
			return json.data
	return []

func _save_transition_history(history: Array) -> void:
	var file = FileAccess.open(_transition_history_file, FileAccess.WRITE)
	if file:
		file.store_string(JSON.stringify(history, "\t"))
		file.close()

func log_transition_event(track_a: String, track_b: String, transition_type: String, outcome: String) -> void:
	if track_a.is_empty() or track_b.is_empty():
		return
	var history = _load_transition_history()
	var entry = {
		"track_a": track_a,
		"track_b": track_b,
		"transition_type": transition_type,
		"outcome": outcome,
		"timestamp": Time.get_unix_time_from_system()
	}
	history.append(entry)
	_save_transition_history(history)

func _start_success_timer(info: Dictionary) -> void:
	await get_tree().create_timer(10.0).timeout
	if not _last_transition_info.is_empty() and _last_transition_info["track_a"] == info["track_a"] and _last_transition_info["track_b"] == info["track_b"] and not _last_transition_info.get("logged", false):
		_last_transition_info["logged"] = true
		log_transition_event(info["track_a"], info["track_b"], info["transition_type"], "completed")

func _check_and_log_skip() -> void:
	if _last_transition_info.is_empty() or _last_transition_info.get("logged", false):
		return
		
	var during_transition = is_transitioning
	var time_since_end = -1.0
	if _last_transition_info["finished"]:
		time_since_end = (Time.get_ticks_msec() - _last_transition_info["end_time_msec"]) / 1000.0
		
	if during_transition or (time_since_end >= 0.0 and time_since_end <= 10.0):
		_last_transition_info["logged"] = true
		log_transition_event(
			_last_transition_info["track_a"],
			_last_transition_info["track_b"],
			_last_transition_info["transition_type"],
			"skipped"
		)

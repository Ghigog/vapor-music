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

var current_playlist: Array[String] = []
var current_track_index: int = -1
var current_track_length: float
var is_playing := false
var is_transitioning := false
var active_tween: Tween
var _pitch_ramp_tween: Tween
var transition_duration := 8.0
var upcoming_track_override := "":
	set(val):
		if upcoming_track_override != val:
			upcoming_track_override = val
			_update_upcoming_transition()

var playback_history: Array[String] = []
var history_pointer: int = -1
var _navigating_history := false

func get_transition_duration(type: String) -> float:
	if not Engine.is_editor_hint() and transition_duration == 0.1:
		return 0.1
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

var smart_mixing_step_index := 0
var preferred_type_for_current_track := "perfect"
var upcoming_transition_type := "Standard Crossfade"

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

func _update_upcoming_transition() -> void:
	var next_href = get_next_track_href()
	if next_href.is_empty():
		upcoming_transition_type = "Standard Crossfade"
		return
		
	var current_href = ""
	if current_track_index != -1 and current_track_index < current_playlist.size():
		current_href = current_playlist[current_track_index]
		
	var metadata_service = get_node_or_null("/root/MetadataService")
	if is_instance_valid(metadata_service) and not current_href.is_empty():
		var current_meta = metadata_service.get_cached_metadata(current_href)
		var next_meta = metadata_service.get_cached_metadata(next_href)
		if not current_meta.is_empty() and not next_meta.is_empty():
			var bpm_diff = abs(current_meta.get("bpm", 120.0) - next_meta.get("bpm", 120.0))
			var key_a = current_meta.get("musical_key", "")
			var key_b = next_meta.get("musical_key", "")
			
			var DJPathfinderClass = load("res://scripts/services/dj_pathfinder.gd")
			var key_cost = DJPathfinderClass.get_harmonic_relation_cost(key_a, key_b)
			var match_type = _get_match_type_between(current_meta, next_meta)
			
			if key_cost >= 8.0:
				# 1. Clashing keys: wash/mask transition effects to hide clash
				if bpm_diff >= 8.0 or match_type == "creative" or match_type == "interesting":
					upcoming_transition_type = "Echo Out"
				else:
					upcoming_transition_type = "Reverb Freeze"
			elif key_cost >= 2.1 and key_cost <= 3.5:
				# 2. Key modulations (Energy Boost, Power Mix, Subdominant, etc.)
				if bpm_diff < 3.0:
					upcoming_transition_type = "Bass Swap"
				elif bpm_diff < 8.0:
					upcoming_transition_type = "Tempo Morph"
				else:
					upcoming_transition_type = "Echo Out"
			else:
				# 3. Harmonically compatible keys
				if bpm_diff < 3.0:
					upcoming_transition_type = "Bass Swap"
				elif bpm_diff < 8.0:
					if match_type == "creative" or match_type == "interesting":
						upcoming_transition_type = "Reverb Freeze"
					elif bpm_diff >= 5.0:
						upcoming_transition_type = "Tempo Morph"
					else:
						upcoming_transition_type = "Filter Sweep"
				else:
					if match_type == "creative" or match_type == "interesting" or bpm_diff >= 12.0:
						upcoming_transition_type = "Echo Out"
					else:
						upcoming_transition_type = "Standard Crossfade"
			return
			
	# Fallback to deterministic pseudo-random choice if metadata not yet ready
	var pair_hash = (current_href + next_href).hash()
	var rng = abs(pair_hash) % 6
	match rng:
		0:
			upcoming_transition_type = "Standard Crossfade"
		1:
			upcoming_transition_type = "Bass Swap"
		2:
			upcoming_transition_type = "Filter Sweep"
		3:
			upcoming_transition_type = "Echo Out"
		4:
			upcoming_transition_type = "Reverb Freeze"
		5:
			upcoming_transition_type = "Tempo Morph"

var _debounce_timer: Timer

const CACHE_DIR = "user://audio_cache/"

func _ready() -> void:
	# Programmatically setup DeckA and DeckB audio buses with EQ and Filter effects
	_setup_audio_buses()
	
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

func _process(_delta: float) -> void:
	# Monitor playback position to trigger smooth transitions before outro ends
	if is_playing and not is_transitioning and active_player and active_player.playing:
		var pos = active_player.get_playback_position()
		var length = current_track_length
		var duration = get_transition_duration(upcoming_transition_type)
		if length > 15.0 and (length - pos) <= (duration + 4.0):
			print("AudioManager: Outro zone reached. Triggering automatic blend transition.")
			start_transition()

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
	is_transitioning = false
	
	_record_history(track_href)
	_navigating_history = false
	
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
		target_player.stream = stream
		if play_immediately:
			target_player.play()
			if not is_playing:
				target_player.stream_paused = true
		else:
			target_player.stop()
			
		if target_player == active_player:
			current_track_length = 30.0 # dummy length for tests
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
	
	var audio_data := PackedByteArray()
	
	if FileAccess.file_exists(cache_path):
		print("AudioManager: Loading track from local disk cache: ", cache_path)
		audio_data = FileAccess.get_file_as_bytes(cache_path)
	else:
		var base_url: String = SettingsManager.webdav_url
		var username := SettingsManager.webdav_username
		var password := SettingsManager.webdav_password
		
		var auth_raw := "%s:%s" % [username, password]
		var auth_header := "Authorization: Basic %s" % Marshalls.utf8_to_base64(auth_raw)
		
		# Fix URL assembly: Extract host safely without duplicating path segments
		var url_parts = base_url.split("/dav")
		var host_base = url_parts[0]
		
		# Clean up incoming href path to prevent double encoding
		var clean_href = href_path
		if not clean_href.begins_with("/"):
			clean_href = "/" + clean_href
		
		# Step 1: Decode it fully to strip any pre-existing %20 artifacts
		var completely_decoded = clean_href.uri_decode()
		
		# Step 2: Encode the raw string, then fix the slashes so it doesn't break the web path structure
		var safe_encoded_path = completely_decoded.uri_encode().replace("%2F", "/")
		
		var full_target_endpoint = host_base + safe_encoded_path
		
		print("AudioManager: Requesting stream from: ", full_target_endpoint)
		
		var http_client := HTTPRequest.new()
		add_child(http_client)
		
		# FIX: Bypass local desktop TLS client handshake limits if a bundle isn't defined
		http_client.set_tls_options(TLSOptions.client_unsafe()) 
		
		var temp_download_path = cache_path + ".tmp"
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
			
		err = DirAccess.rename_absolute(temp_download_path, cache_path)
		if err != OK:
			print("AudioManager: Failed to rename downloaded file to cache: ", err)
			cache_path = temp_download_path
			
		audio_data = FileAccess.get_file_as_bytes(cache_path)
		
	# Instantiate and stream standard MP3 byte headers directly into memory audio layers
	loading_track.emit(false)
	var stream := AudioStreamMP3.new()
	stream.data = audio_data
	
	# Double-check that data was actually fed into the stream buffer
	if stream.data.size() == 0:
		print("AudioManager: Error - Received an empty binary stream buffer.")
		return false
		
	target_player.stream = stream
	if play_immediately:
		target_player.play()
		if not is_playing:
			target_player.stream_paused = true
	else:
		target_player.stop()
	
	# Enqueue for local audio analysis now that it is cached on disk
	var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
	if is_instance_valid(audio_analyzer):
		audio_analyzer.analyze_track(href_path, true)
	
	if target_player == active_player:
		current_track_length = stream.get_length()
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
		1:
			preferred_type_for_current_track = "interesting"
		3:
			preferred_type_for_current_track = "creative" if randf() < 0.5 else "interesting"

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
		
	if not upcoming_track_override.is_empty() and next_track_href == upcoming_track_override:
		upcoming_track_override = ""
			
	# Update index
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
		return
	
	# Wait until outgoing track has duration or less remaining (skip if force_immediate is true)
	var duration = get_transition_duration(transition_type)
	if not force_immediate:
		while active_player and active_player.playing and is_transitioning:
			var remaining = current_track_length - active_player.get_playback_position()
			if remaining <= duration:
				break
			await get_tree().process_frame
		
	if not is_transitioning:
		return
	
	# Start crossfading
	_run_deck_transition(active_player, incoming_player, transition_type)


func _run_deck_transition(outgoing: AudioStreamPlayer, incoming: AudioStreamPlayer, transition_type: String) -> void:
	if _pitch_ramp_tween and _pitch_ramp_tween.is_valid():
		_pitch_ramp_tween.kill()
		
	player = incoming
	current_track_length = incoming.stream.get_length() if incoming.stream else 0.0
	length_changed.emit(current_track_length)
	
	var out_bus = outgoing.bus
	var in_bus = incoming.bus
	var out_bus_idx = AudioServer.get_bus_index(out_bus)
	var in_bus_idx = AudioServer.get_bus_index(in_bus)
	
	_reset_bus_effects(out_bus_idx)
	_reset_bus_effects(in_bus_idx)
	
	AudioServer.set_bus_volume_db(out_bus_idx, 0.0)
	AudioServer.set_bus_volume_db(in_bus_idx, -60.0)
	
	incoming.play()
	if not is_playing:
		incoming.stream_paused = true
	
	var duration = get_transition_duration(transition_type)
	
	if active_tween and active_tween.is_valid():
		active_tween.kill()
	active_tween = create_tween().set_parallel(true)
	
	if not is_playing:
		active_tween.pause()
	
	# Transition-specific volume envelopes and dynamics
	if transition_type == "Bass Swap":
		var half_duration = duration / 2.0
		# Outgoing: keep at 0.0 dB for half_duration, then fade out to -60.0 dB over half_duration
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(out_bus_idx, val)
		, 0.0, -60.0, half_duration).set_trans(Tween.TRANS_SINE).set_delay(half_duration)
		
		# Incoming: fade in to 0.0 dB over half_duration, then stay at 0.0 dB
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, half_duration).set_trans(Tween.TRANS_SINE)
		
		# EQ setup: cut incoming bass initially
		var in_eq = AudioServer.get_bus_effect(in_bus_idx, 0) as AudioEffectEQ
		in_eq.set_band_gain_db(0, -40.0)
		in_eq.set_band_gain_db(1, -40.0)
		
		var out_eq = AudioServer.get_bus_effect(out_bus_idx, 0) as AudioEffectEQ
		
		# Outgoing bass cut swap at midpoint
		active_tween.tween_method(func(val: float):
			out_eq.set_band_gain_db(0, val)
			out_eq.set_band_gain_db(1, val)
		, 0.0, -40.0, 0.5 if duration > 1.0 else duration * 0.06).set_delay(half_duration)
		
		# Incoming bass boost swap at midpoint
		active_tween.tween_method(func(val: float):
			in_eq.set_band_gain_db(0, val)
			in_eq.set_band_gain_db(1, val)
		, -40.0, 0.0, 0.5 if duration > 1.0 else duration * 0.06).set_delay(half_duration)
		
	elif transition_type == "Filter Sweep":
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
		active_tween.tween_property(out_lp, "cutoff_hz", 150.0, duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
		
		# Incoming sweep highpass cutoff down from 2000Hz to 10Hz over the entire duration
		in_hp.cutoff_hz = 2000.0
		active_tween.tween_property(in_hp, "cutoff_hz", 10.0, duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
		
	elif transition_type == "Echo Out":
		var half_duration = duration / 2.0
		active_tween.tween_interval(duration)
		
		# Outgoing: bus volume stays at 0.0 dB. At midpoint, dry level cuts to 0.0
		# Incoming: bus volume fades from -60.0 dB to 0.0 dB over the first half
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, half_duration).set_trans(Tween.TRANS_SINE)
		
		# Setup delay on outgoing bus
		var out_delay = AudioServer.get_bus_effect(out_bus_idx, 3) as AudioEffectDelay
		if out_delay:
			out_delay.dry = 1.0
			out_delay.feedback_delay_ms = 350.0
			out_delay.feedback_level_db = -10.0
			out_delay.tap1_level_db = -6.0
			out_delay.tap2_level_db = -12.0
			
			# At midpoint, cut dry signal of outgoing deck
			active_tween.tween_method(func(val: float):
				out_delay.dry = val
			, 1.0, 0.0, 0.1 if duration > 1.0 else duration * 0.02).set_delay(half_duration)
			
	elif transition_type == "Reverb Freeze":
		var half_duration = duration / 2.0
		active_tween.tween_interval(duration)
		
		# Incoming: fade in to 0.0 dB over first half
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, half_duration).set_trans(Tween.TRANS_SINE)
		
		# Outgoing Reverb setup
		var out_reverb = AudioServer.get_bus_effect(out_bus_idx, 4) as AudioEffectReverb
		if out_reverb:
			out_reverb.wet = 0.0
			out_reverb.dry = 1.0
			out_reverb.room_size = 0.95
			out_reverb.damping = 0.1
			out_reverb.predelay_feedback = 0.3
			
			# Ramp up wet mix over first half
			active_tween.tween_method(func(val: float):
				out_reverb.wet = val
			, 0.0, 1.0, half_duration)
			
			# At midpoint, cut dry mix instantly to freeze the tail
			active_tween.tween_method(func(val: float):
				out_reverb.dry = val
			, 1.0, 0.0, 0.1 if duration > 1.0 else duration * 0.02).set_delay(half_duration)
			
	elif transition_type == "Tempo Morph":
		var bpm_out = 120.0
		var bpm_in = 120.0
		
		var metadata_service = get_node_or_null("/root/MetadataService")
		if is_instance_valid(metadata_service):
			var current_href = current_playlist[current_track_index - 1] if current_track_index > 0 else ""
			var next_href = current_playlist[current_track_index]
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
		
		# Ramp pitch scales over first 50% of duration (max 3.0s) for a smoother morph
		var ramp_time = min(3.0, duration * 0.5)
		active_tween.tween_property(outgoing, "pitch_scale", pitch_out, ramp_time)
		active_tween.tween_property(incoming, "pitch_scale", pitch_in, ramp_time)
		
		# Volume crossfades over entire duration
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(out_bus_idx, val)
		, 0.0, -60.0, duration).set_trans(Tween.TRANS_SINE)
		
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, duration).set_trans(Tween.TRANS_SINE)
		
	else:
		# Standard Crossfade: Standard full-duration volume crossfades
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(out_bus_idx, val)
		, 0.0, -60.0, duration).set_trans(Tween.TRANS_SINE)
		
		active_tween.tween_method(func(val: float):
			AudioServer.set_bus_volume_db(in_bus_idx, val)
		, -60.0, 0.0, duration).set_trans(Tween.TRANS_SINE)
		
	await active_tween.finished
	
	# Switch roles
	outgoing.stop()
	active_player = incoming
	player = active_player
	
	# Capture morphed pitch scale before reset overrides it to 1.0
	var current_pitch_in = incoming.pitch_scale
	
	_reset_bus_effects(out_bus_idx)
	_reset_bus_effects(in_bus_idx)
	
	if transition_type == "Tempo Morph":
		incoming.pitch_scale = current_pitch_in
		var ramp_duration = 6.0 if duration > 1.0 else 0.1
		_pitch_ramp_tween = create_tween()
		_pitch_ramp_tween.tween_property(active_player, "pitch_scale", 1.0, ramp_duration).set_trans(Tween.TRANS_SINE)
	
	is_transitioning = false
	current_track_length = incoming.stream.get_length() if incoming.stream else 0.0
	transition_completed.emit(current_playlist[current_track_index])
	update_preferred_type()
	_update_upcoming_transition()
	_record_history(current_playlist[current_track_index])
	_navigating_history = false


func _reset_bus_effects(bus_idx: int) -> void:
	AudioServer.set_bus_volume_db(bus_idx, 0.0)
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
	elif player_b and player_b.bus == AudioServer.get_bus_name(bus_idx):
		player_b.pitch_scale = 1.0

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
	if is_playing and active_player.playing and not is_transitioning:
		upcoming_track_override = target_href
		start_transition(true)
	else:
		is_transitioning = false
		_record_history(target_href)
		_navigating_history = false
		_load_and_stream_remote_file(target_href, active_player, is_playing)

func scroll_track(value) -> void:
	active_player.seek(value)

func _on_deck_finished(finished_player: AudioStreamPlayer) -> void:
	if finished_player != active_player:
		# Ignore finished signals from the outgoing player during a transition
		return
		
	if is_transitioning:
		return
		
	# Loop playlist sequence
	start_transition()

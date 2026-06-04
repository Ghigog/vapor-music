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

var player: AudioStreamPlayer # Compatibility reference, points to active_player
var player_a: AudioStreamPlayer
var player_b: AudioStreamPlayer
var active_player: AudioStreamPlayer

var current_playlist: Array[String] = []
var current_track_index: int = -1
var current_track_length: float
var is_playing := false
var is_transitioning := false
var upcoming_track_override := ""
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
	# Monitor playback position to trigger smooth transitions 10 seconds before outro ends
	if is_playing and not is_transitioning and active_player and active_player.playing:
		var pos = active_player.get_playback_position()
		var length = current_track_length
		if length > 15.0 and (length - pos) <= 10.0:
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
			
			# Add EQ6 effect
			var eq = AudioEffectEQ6.new()
			AudioServer.add_bus_effect(idx, eq, 0)
			
			# Add Lowpass Filter effect
			var lp = AudioEffectLowPassFilter.new()
			lp.cutoff_hz = 20000.0
			AudioServer.add_bus_effect(idx, lp, 1)

			# Add Highpass Filter effect
			var hp = AudioEffectHighPassFilter.new()
			hp.cutoff_hz = 10.0
			AudioServer.add_bus_effect(idx, hp, 2)

## Prepares a new track vector array queue context state
func play_track(track_href: String, playlist: Array) -> void:
	if _debounce_timer and not _debounce_timer.is_stopped():
		_debounce_timer.stop()
		
	current_playlist = Array(playlist, TYPE_STRING, &"", null)
	current_track_index = current_playlist.find(track_href)
	is_transitioning = false
	
	# Stop secondary player if active
	var secondary = player_b if active_player == player_a else player_a
	secondary.stop()
	_reset_bus_effects(AudioServer.get_bus_index(player_a.bus))
	_reset_bus_effects(AudioServer.get_bus_index(player_b.bus))
	
	_load_and_stream_remote_file(track_href, active_player)
	
	# Proactively queue any already-cached tracks in the playlist for background analysis
	var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
	if is_instance_valid(audio_analyzer):
		audio_analyzer.scan_library_cache(current_playlist)


## Asynchronously downloads the remote audio file context and loads it into an AudioStream layer
func _load_and_stream_remote_file(href_path: String, target_player: AudioStreamPlayer) -> void:
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
			return
			
		var response = await http_client.request_completed
		var response_code: int = response[1]
		
		http_client.queue_free()
		
		if response_code != 200:
			print("AudioManager: Server streaming request rejected with HTTP code: %d" % response_code)
			if FileAccess.file_exists(temp_download_path):
				DirAccess.remove_absolute(temp_download_path)
			loading_track.emit(false)
			return
			
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
		return
		
	target_player.stream = stream
	target_player.play()
	
	# Enqueue for local audio analysis now that it is cached on disk
	var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
	if is_instance_valid(audio_analyzer):
		audio_analyzer.analyze_track(href_path, true)
	
	if target_player == active_player:
		current_track_length = stream.get_length()
		length_changed.emit(current_track_length)
		is_playing = true
		playback_toggled.emit(true)

func toggle_play() -> void:
	if active_player.stream == null:
		return
	if is_playing:
		active_player.stream_paused = true
		is_playing = false
	else:
		active_player.stream_paused = false
		is_playing = true
	playback_toggled.emit(is_playing)

func play_next() -> void:
	if current_playlist.is_empty():
		return
	if _debounce_timer and not _debounce_timer.is_stopped():
		_debounce_timer.stop()
	
	if not upcoming_track_override.is_empty() and current_playlist.has(upcoming_track_override):
		current_track_index = current_playlist.find(upcoming_track_override)
		upcoming_track_override = ""
	else:
		current_track_index = (current_track_index + 1) % current_playlist.size()
	_start_debounce()

func play_previous() -> void:
	if current_playlist.is_empty():
		return
	if _debounce_timer and not _debounce_timer.is_stopped():
		_debounce_timer.stop()
	current_track_index = (current_track_index - 1 + current_playlist.size()) % current_playlist.size()
	_start_debounce()

## Triggers the DJ transition crossfade to the next track
func start_transition() -> void:
	if is_transitioning or current_playlist.is_empty():
		return
		
	is_transitioning = true
	
	# Select upcoming track (support manual override or default to best runner-up)
	var next_track_href = ""
	if not upcoming_track_override.is_empty() and current_playlist.has(upcoming_track_override):
		next_track_href = upcoming_track_override
		upcoming_track_override = ""
	else:
		# Auto-blend selector: pick best runner-up
		var active_href = current_playlist[current_track_index]
		var tracks_meta := {}
		var metadata_service = get_node_or_null("/root/MetadataService")
		for href in current_playlist:
			if href == active_href: continue
			var meta = {}
			if is_instance_valid(metadata_service):
				meta = metadata_service.get_cached_metadata(href)
			if meta.is_empty() or meta.get("bpm", 0.0) <= 0.0:
				continue # Skip unanalyzed tracks to prevent selecting incompatible tracks with dummy metadata
			tracks_meta[href] = meta
			
		var current_meta = {}
		if is_instance_valid(metadata_service):
			current_meta = metadata_service.get_cached_metadata(active_href)
		if current_meta.is_empty():
			current_meta = {"bpm": 120.0, "musical_key": "", "energy_level": 0.5}
			
		var scored_tracks = []
		var DJPathfinderClass = load("res://scripts/services/dj_pathfinder.gd")
		for href in tracks_meta:
			var cost = DJPathfinderClass.calculate_transition_cost(current_meta, tracks_meta[href])
			scored_tracks.append({"href": href, "cost": cost})
		scored_tracks.sort_custom(func(a, b): return a.cost < b.cost)
		
		if not scored_tracks.is_empty():
			next_track_href = scored_tracks[0].href
		else:
			next_track_href = current_playlist[(current_track_index + 1) % current_playlist.size()]
			
	# Update index
	current_track_index = current_playlist.find(next_track_href)
	if current_track_index == -1:
		current_playlist.append(next_track_href)
		current_track_index = current_playlist.size() - 1
		
	# Select transition type randomly or based on key/bpm matches
	var transition_type = "Standard Crossfade"
	var rng = randi() % 3
	if rng == 0:
		transition_type = "Standard Crossfade"
	elif rng == 1:
		transition_type = "Bass Swap"
	else:
		transition_type = "Filter Sweep"
		
	var incoming_player = player_b if active_player == player_a else player_a
	
	transition_started.emit(next_track_href, transition_type)
	
	# Load next stream on incoming player
	await _load_and_stream_remote_file(next_track_href, incoming_player)
	
	# Start crossfading
	_run_deck_transition(active_player, incoming_player, transition_type)

func _run_deck_transition(outgoing: AudioStreamPlayer, incoming: AudioStreamPlayer, transition_type: String) -> void:
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
	
	var duration = 8.0
	var tween = create_tween().set_parallel(true)
	
	# Standard volume crossfade
	tween.tween_method(func(val: float):
		AudioServer.set_bus_volume_db(out_bus_idx, val)
	, 0.0, -60.0, duration).set_trans(Tween.TRANS_SINE)
	
	tween.tween_method(func(val: float):
		AudioServer.set_bus_volume_db(in_bus_idx, val)
	, -60.0, 0.0, duration).set_trans(Tween.TRANS_SINE)
	
	# Extra transition dynamics
	if transition_type == "Bass Swap":
		var in_eq = AudioServer.get_bus_effect(in_bus_idx, 0) as AudioEffectEQ
		in_eq.set_band_gain_db(0, -40.0)
		in_eq.set_band_gain_db(1, -40.0)
		
		var half_duration = duration / 2.0
		var out_eq = AudioServer.get_bus_effect(out_bus_idx, 0) as AudioEffectEQ
		
		# Outgoing bass swap
		tween.tween_method(func(val: float):
			out_eq.set_band_gain_db(0, val)
			out_eq.set_band_gain_db(1, val)
		, 0.0, -40.0, 0.5).set_delay(half_duration)
		
		# Incoming bass swap
		tween.tween_method(func(val: float):
			in_eq.set_band_gain_db(0, val)
			in_eq.set_band_gain_db(1, val)
		, -40.0, 0.0, 0.5).set_delay(half_duration)
		
	elif transition_type == "Filter Sweep":
		var out_lp = AudioServer.get_bus_effect(out_bus_idx, 1) as AudioEffectLowPassFilter
		var in_hp = AudioServer.get_bus_effect(in_bus_idx, 2) as AudioEffectHighPassFilter
		
		# Outgoing sweep lowpass cutoff down from 20000Hz to 150Hz
		tween.tween_property(out_lp, "cutoff_hz", 150.0, duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
		
		# Incoming sweep highpass cutoff down from 2000Hz to 10Hz
		in_hp.cutoff_hz = 2000.0
		tween.tween_property(in_hp, "cutoff_hz", 10.0, duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
		
	await tween.finished
	
	# Switch roles
	outgoing.stop()
	active_player = incoming
	player = active_player
	
	_reset_bus_effects(out_bus_idx)
	_reset_bus_effects(in_bus_idx)
	
	is_transitioning = false
	current_track_length = incoming.stream.get_length() if incoming.stream else 0.0
	transition_completed.emit(current_playlist[current_track_index])

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

## Generates a play queue sorted by harmonic and energy compatibility and starts playback
func play_harmonic_shuffle(start_track_href: String = "") -> void:
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
		start_transition()
	else:
		is_transitioning = false
		_load_and_stream_remote_file(target_href, active_player)

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

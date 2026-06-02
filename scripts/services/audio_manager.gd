## audio_manager.gd
## Global engine player that streams audio buffers dynamically via authenticated HTTP requests.
extends Node

signal track_changed(track_name: String)
signal playback_toggled(is_playing: bool)
signal loading_track(is_loading: bool)

var player: AudioStreamPlayer
var current_playlist: Array[String] = []
var current_track_index: int = -1
var current_track_length: float
var is_playing := false
var _debounce_timer: Timer

func _ready() -> void:
	# Build out runtime child container component dynamically
	player = AudioStreamPlayer.new()
	add_child(player)
	player.finished.connect(_on_track_finished)
	
	_debounce_timer = Timer.new()
	_debounce_timer.one_shot = true
	_debounce_timer.timeout.connect(_on_debounce_timeout)
	add_child(_debounce_timer)

## Prepares a new track vector array queue context state
func play_track(track_href: String, playlist: Array) -> void:
	if _debounce_timer and not _debounce_timer.is_stopped():
		_debounce_timer.stop()
	current_playlist = Array(playlist, TYPE_STRING, &"", null)
	current_track_index = current_playlist.find(track_href)
	
	_load_and_stream_remote_file(track_href)

## Asynchronously downloads the remote audio file context and loads it into an AudioStream layer
func _load_and_stream_remote_file(href_path: String) -> void:
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
	loading_track.emit(true)
	track_changed.emit(href_path.get_file().uri_decode())
	
	var http_client := HTTPRequest.new()
	add_child(http_client)
	
	# FIX: Bypass local desktop TLS client handshake limits if a bundle isn't defined
	http_client.set_tls_options(TLSOptions.client_unsafe()) 
	
	var err := http_client.request(full_target_endpoint, [auth_header], HTTPClient.METHOD_GET)
	if err != OK:
		print("AudioManager: Network request instantiation failure code: ", err)
		http_client.queue_free()
		return
		
	var response = await http_client.request_completed
	var response_code: int = response[1]
	var response_body: PackedByteArray = response[3]
	
	http_client.queue_free()
	
	if response_code != 200:
		print("AudioManager: Server streaming request rejected with HTTP code: %d" % response_code)
		return
		
	# Instantiate and stream standard MP3 byte headers directly into memory audio layers
	loading_track.emit(false)
	var stream := AudioStreamMP3.new()
	stream.data = response_body
	current_track_length = stream.get_length()
	print(current_track_length)
	
	# Double-check that data was actually fed into the stream buffer
	if stream.data.size() == 0:
		print("AudioManager: Error - Received an empty binary stream buffer.")
		return
		
	player.stream = stream
	player.play()
	is_playing = true
	playback_toggled.emit(true)

func toggle_play() -> void:
	if player.stream == null:
		return
	if is_playing:
		player.stream_paused = true
		is_playing = false
	else:
		player.stream_paused = false
		is_playing = true
	playback_toggled.emit(is_playing)

func play_next() -> void:
	if current_playlist.is_empty():
		return
	current_track_index = (current_track_index + 1) % current_playlist.size()
	_start_debounce()
	
func play_previous() -> void:
	if current_playlist.is_empty():
		return
	# Subtract 1, add size() to ensure the result is positive before the modulo
	current_track_index = (current_track_index - 1 + current_playlist.size()) % current_playlist.size()
	_start_debounce()

func _start_debounce() -> void:
	var track_href = current_playlist[current_track_index]
	track_changed.emit(track_href.get_file().uri_decode())
	_debounce_timer.start(3.0)

func _on_debounce_timeout() -> void:
	if current_playlist.is_empty() or current_track_index == -1:
		return
	_load_and_stream_remote_file(current_playlist[current_track_index])

func scroll_track(value) -> void:
	player.seek(value)

func _on_track_finished() -> void:
	if current_playlist.is_empty() or current_track_index == -1:
		return
	# Loop playlist sequence
	current_track_index = (current_track_index + 1) % current_playlist.size()
	_load_and_stream_remote_file(current_playlist[current_track_index])

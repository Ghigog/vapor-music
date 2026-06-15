extends Node
## WebDAVService
##
## Handles direct HTTP communication with WebDAV servers using raw TCP/TLS sockets,
## because Godot 4 does not support custom HTTP methods like PROPFIND natively.
## This allows us to send a real PROPFIND request (required by WebDAV spec).

signal library_scanned(files: Array)
signal connection_tested(success: bool, error_message: String)

var scanned_files: Array = []
var had_scan_errors: bool = false
var is_scanning: bool = false

const LIBRARY_CACHE_FILE = "user://library_cache.json"

func _ready() -> void:
	load_cached_library()

func load_cached_library() -> Array:
	var start_time := Time.get_ticks_usec()
	if FileAccess.file_exists(LIBRARY_CACHE_FILE):
		var file := FileAccess.open(LIBRARY_CACHE_FILE, FileAccess.READ)
		if file:
			var content := file.get_as_text()
			file.close()
			var parsed = JSON.parse_string(content)
			if parsed is Array:
				scanned_files = parsed
				var duration := (Time.get_ticks_usec() - start_time) / 1000000.0
				print("WebDAVService: Loaded %d tracks from cache in %.3fs" % [scanned_files.size(), duration])
				return scanned_files
	var duration := (Time.get_ticks_usec() - start_time) / 1000000.0
	print("WebDAVService: No cache found. Load completed in %.3fs" % duration)
	return []

func save_cached_library() -> void:
	var file := FileAccess.open(LIBRARY_CACHE_FILE, FileAccess.WRITE)
	if file:
		file.store_string(JSON.stringify(scanned_files))
		file.close()
		print("WebDAVService: Library cache saved to disk.")

const TCP_TIMEOUT_MS := 5000
const TLS_TIMEOUT_MS := 5000
const READ_TIMEOUT_MS := 10000

var _active_tcp: StreamPeerTCP = null
var _active_tls: StreamPeerTLS = null
var _current_connected_host: String = ""
var _current_connected_port: int = -1

## Parses a WebDAV URL into its component parts.
func _parse_url(url: String) -> Dictionary:
	var result := {"protocol": "https", "host": "", "port": 443, "path": "/"}

	var parts := url.split("://", true, 1)
	if parts.size() == 2:
		result.protocol = parts[0].to_lower()
		url = parts[1]

	result.port = 443 if result.protocol == "https" else 80

	var first_slash := url.find("/")
	var host_part: String
	if first_slash == -1:
		host_part = url
		result.path = "/"
	else:
		host_part = url.substr(0, first_slash)
		result.path = url.substr(first_slash)

	if host_part.contains(":"):
		var host_port := host_part.split(":", true, 1)
		result.host = host_port[0]
		result.port = host_port[1].to_int()
	else:
		result.host = host_part

	return result

## Generates the Base64-encoded Basic auth header value.
func _get_auth_header(username: String, password: String) -> String:
	return "Authorization: Basic " + Marshalls.utf8_to_base64(username + ":" + password)

## Builds a raw HTTP/1.1 PROPFIND request string.
func _build_propfind_request(host: String, path: String, auth_header: String, depth: int = 1) -> String:
	var body := "<?xml version=\"1.0\" encoding=\"utf-8\" ?>\n"
	body += "<d:propfind xmlns:d=\"DAV:\">\n"
	body += "  <d:prop><d:displayname/><d:getcontentlength/><d:resourcetype/></d:prop>\n"
	body += "</d:propfind>"
	
	var body_bytes := body.to_utf8_buffer()
	
	var req := "PROPFIND %s HTTP/1.1\r\n" % path
	req += "Host: %s\r\n" % host
	req += "User-Agent: VaporMusicPlayer/1.0 (Godot Engine)\r\n"
	req += auth_header + "\r\n"
	req += "Depth: %d\r\n" % depth
	req += "Content-Type: text/xml; charset=\"utf-8\"\r\n"
	req += "Content-Length: %d\r\n" % body_bytes.size()
	req += "Connection: keep-alive\r\n" # Changed to keep-alive to protect the pipe
	req += "\r\n"
	req += body
	return req

func _is_connection_alive() -> bool:
	if not _active_tcp or not _active_tls:
		return false
	_active_tcp.poll()
	_active_tls.poll()
	return _active_tcp.get_status() == StreamPeerTCP.STATUS_CONNECTED and _active_tls.get_status() == StreamPeerTLS.STATUS_CONNECTED

func _ensure_connection(host: String, port: int) -> int:
	if _is_connection_alive() and _current_connected_host == host and _current_connected_port == port:
		return OK
		
	disconnect_active_connection()
	
	var tcp := StreamPeerTCP.new()
	var err := tcp.connect_to_host(host, port)
	if err != OK:
		return err

	var start := Time.get_ticks_msec()
	while tcp.get_status() == StreamPeerTCP.STATUS_CONNECTING:
		tcp.poll()
		await Engine.get_main_loop().process_frame
		if Time.get_ticks_msec() - start > TCP_TIMEOUT_MS:
			return ERR_CONNECTION_ERROR

	if tcp.get_status() != StreamPeerTCP.STATUS_CONNECTED:
		return ERR_CONNECTION_ERROR

	var tls := StreamPeerTLS.new()
	err = tls.connect_to_stream(tcp, host, TLSOptions.client())
	if err != OK:
		return err

	start = Time.get_ticks_msec()
	while true:
		tls.poll()
		var tls_status := tls.get_status()
		if tls_status == StreamPeerTLS.STATUS_CONNECTED:
			break
		elif tls_status == StreamPeerTLS.STATUS_ERROR or tls_status == StreamPeerTLS.STATUS_DISCONNECTED:
			return ERR_CONNECTION_ERROR
		await Engine.get_main_loop().process_frame
		if Time.get_ticks_msec() - start > TLS_TIMEOUT_MS:
			return ERR_CONNECTION_ERROR

	_active_tcp = tcp
	_active_tls = tls
	_current_connected_host = host
	_current_connected_port = port
	return OK

func disconnect_active_connection() -> void:
	if _active_tcp:
		_active_tcp.disconnect_from_host()
	_active_tcp = null
	_active_tls = null
	_current_connected_host = ""
	_current_connected_port = -1

## Sends a raw PROPFIND request over TLS and returns [response_code, header_string, body_string].
func _send_propfind(host: String, port: int, path: String, auth_header: String, depth: int = 1) -> Array:
	var err = await _ensure_connection(host, port)
	if err != OK:
		return [-1, "", "Connection establishment failed: %d" % err]

	# --- Send PROPFIND ---
	var request_str := _build_propfind_request(host, path, auth_header, depth)
	err = _active_tls.put_data(request_str.to_utf8_buffer())
	if err != OK:
		print("WebDAVService: Send failed, retrying connection...")
		disconnect_active_connection()
		err = await _ensure_connection(host, port)
		if err != OK:
			return [-1, "", "Reconnection failed: %d" % err]
		err = _active_tls.put_data(request_str.to_utf8_buffer())
		if err != OK:
			return [-1, "", "Failed to send request after retry: %d" % err]

	# --- Read response ---
	var response_bytes := PackedByteArray()
	var start = Time.get_ticks_msec() 
	
	var headers_parsed := false
	var is_chunked := false
	var content_length := -1
	var header_end_idx := -1

	while true:
		_active_tls.poll()
		var status := _active_tls.get_status()
		var available := _active_tls.get_available_bytes()
		
		# 1. ALWAYS consume bytes if they are waiting in the TLS buffer
		if available > 0:
			var chunk := _active_tls.get_data(available)
			if chunk[0] == OK:
				response_bytes.append_array(chunk[1])
				start = Time.get_ticks_msec() # Reset timeout window on fresh data
				
				if not headers_parsed:
					header_end_idx = _find_header_delimiter(response_bytes)
					if header_end_idx != -1:
						headers_parsed = true
						var loop_header := _safe_get_string(response_bytes.slice(0, header_end_idx)).to_lower()
						is_chunked = "transfer-encoding: chunked" in loop_header
						if "content-length:" in loop_header:
							var cl_line := loop_header.split("content-length:")[1].split("\r\n")[0].strip_edges()
							content_length = cl_line.to_int()
				
				if headers_parsed:
					var body_bytes := response_bytes.slice(header_end_idx)
					if is_chunked:
						if _has_terminal_chunk_marker(body_bytes):
							break
					elif content_length != -1:
						if body_bytes.size() >= content_length:
							break
		else:
			# 2. Only exit due to a closed stream if our buffer is completely dry
			if status != StreamPeerTLS.STATUS_CONNECTED:
				break
			await Engine.get_main_loop().process_frame
			
		if Time.get_ticks_msec() - start > READ_TIMEOUT_MS:
			print("WebDAVService: Reading window closed due to channel inactivity timeout.")
			break

	if response_bytes.is_empty():
		print("WebDAVService: Empty response, retrying with fresh connection...")
		disconnect_active_connection()
		err = await _ensure_connection(host, port)
		if err == OK:
			err = _active_tls.put_data(request_str.to_utf8_buffer())
			if err == OK:
				response_bytes.clear()
				start = Time.get_ticks_msec()
				headers_parsed = false
				is_chunked = false
				content_length = -1
				header_end_idx = -1
				while true:
					_active_tls.poll()
					var status := _active_tls.get_status()
					var available := _active_tls.get_available_bytes()
					if available > 0:
						var chunk := _active_tls.get_data(available)
						if chunk[0] == OK:
							response_bytes.append_array(chunk[1])
							start = Time.get_ticks_msec()
							if not headers_parsed:
								header_end_idx = _find_header_delimiter(response_bytes)
								if header_end_idx != -1:
									headers_parsed = true
									var loop_header := _safe_get_string(response_bytes.slice(0, header_end_idx)).to_lower()
									is_chunked = "transfer-encoding: chunked" in loop_header
									if "content-length:" in loop_header:
										var cl_line := loop_header.split("content-length:")[1].split("\r\n")[0].strip_edges()
										content_length = cl_line.to_int()
							if headers_parsed:
								var body_bytes := response_bytes.slice(header_end_idx)
								if is_chunked:
									if _has_terminal_chunk_marker(body_bytes):
										break
								elif content_length != -1:
									if body_bytes.size() >= content_length:
										break
					else:
						if status != StreamPeerTLS.STATUS_CONNECTED:
							break
						await Engine.get_main_loop().process_frame
					if Time.get_ticks_msec() - start > READ_TIMEOUT_MS:
						break

	if response_bytes.is_empty():
		return [-1, "", "Empty response from server after retry"]

	# --- Parse HTTP response ---
	var header_end := _find_header_delimiter(response_bytes)
	if header_end == -1:
		return [-1, "", "Malformed HTTP response (no header delimiter)"]

	var header_str := _safe_get_string(response_bytes.slice(0, header_end))
	var raw_body_bytes := response_bytes.slice(header_end)
	
	if header_str.to_lower().contains("transfer-encoding: chunked"):
		raw_body_bytes = _decode_chunked_body(raw_body_bytes)

	var body_str := _safe_get_string(raw_body_bytes)

	var status_line := header_str.split("\r\n")[0]
	var status_parts := status_line.split(" ", true, 2)
	var response_code := status_parts[1].to_int() if status_parts.size() >= 2 else -1

	return [response_code, header_str, body_str]

func _find_header_delimiter(bytes: PackedByteArray) -> int:
	for i in range(bytes.size() - 3):
		if bytes[i] == 13 and bytes[i+1] == 10 and bytes[i+2] == 13 and bytes[i+3] == 10:
			return i + 4
	return -1

func _has_terminal_chunk_marker(body_bytes: PackedByteArray) -> bool:
	var sz := body_bytes.size()
	# Standard HTTP chunked streams terminate cleanly with "0\r\n\r\n"
	if sz >= 5:
		if body_bytes[sz-5] == 48 and body_bytes[sz-4] == 13 and body_bytes[sz-3] == 10 and body_bytes[sz-2] == 13 and body_bytes[sz-1] == 10:
			return true
	return false

## Low-level HTTP payload utility to reconstruct fragmented chunk boundaries safely.
func _decode_chunked_body(chunked_bytes: PackedByteArray) -> PackedByteArray:
	var decoded := PackedByteArray()
	var idx := 0
	var total_size := chunked_bytes.size()
	
	while idx < total_size:
		var line_end := -1
		for i in range(idx, min(idx + 16, total_size - 1)):
			if chunked_bytes[i] == 13 and chunked_bytes[i+1] == 10:
				line_end = i
				break
				
		if line_end == -1:
			break
			
		var hex_str := _safe_get_string(chunked_bytes.slice(idx, line_end)).strip_edges()
		var chunk_size := hex_str.hex_to_int()
		
		if chunk_size == 0:
			break
			
		idx = line_end + 2
		
		if idx + chunk_size > total_size:
			decoded.append_array(chunked_bytes.slice(idx, total_size))
			break
			
		decoded.append_array(chunked_bytes.slice(idx, idx + chunk_size))
		idx += chunk_size
		
		if idx < total_size and chunked_bytes[idx] == 13:
			idx += 1
		if idx < total_size and chunked_bytes[idx] == 10:
			idx += 1
			
	return decoded

## Aggressive baseline path normalizer to eliminate matching false-negatives
func _normalize_path(path: String) -> String:
	var clean := path.strip_edges().uri_decode()
	
	if "://" in clean:
		clean = clean.split("://", true, 1)[1]
		if "/" in clean:
			clean = clean.substr(clean.find("/"))
			
	if not clean.begins_with("/"):
		clean = "/" + clean
	if not clean.ends_with("/"):
		clean += "/"
	return clean

# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

func test_connection(url: String, username: String, app_password: String) -> void:
	var parts := _parse_url(url)
	var auth := _get_auth_header(username, app_password)
	
	var test_path: String = parts.path
	if not test_path.ends_with("/"):
		test_path += "/"
		
	var result := await _send_propfind(parts.host, parts.port, test_path, auth, 0)
	disconnect_active_connection()
	if result[0] == 207:
		connection_tested.emit(true, "")
	else:
		connection_tested.emit(false, "Server responded with code: %d" % result[0])

func scan_music_directory(target_folder: String = "Music") -> void:
	if not SettingsManager.has_credentials():
		return

	if is_scanning:
		print("WebDAVService: Scan already in progress. Ignoring request.")
		return
	is_scanning = true

	had_scan_errors = false
	var scan_start_time := Time.get_ticks_usec()

	var base_url: String = SettingsManager.webdav_url
	var parts := _parse_url(base_url)
	var auth := _get_auth_header(SettingsManager.webdav_username, SettingsManager.webdav_password)
	
	var base_path: String = parts.path
	if not base_path.ends_with("/"):
		base_path += "/"
		
	var initial_path: String = base_path
	if not target_folder.is_empty() and target_folder != "/":
		initial_path = base_path + target_folder.strip_edges().uri_encode()
		if not initial_path.ends_with("/"):
			initial_path += "/"
			
	var folder_queue: Array[String] = [_normalize_path(initial_path)]
	var all_discovered_tracks := []
	var scanned_paths: Array[String] = []

	print("WebDAVService: Starting clear deep traversal at: %s" % _normalize_path(initial_path))

	while not folder_queue.is_empty():
		var current_scan_path: String = folder_queue.pop_front()
		
		if scanned_paths.has(current_scan_path):
			continue
		scanned_paths.append(current_scan_path)
		
		var request_path = current_scan_path.uri_encode().replace("%2F", "/")
		var result := await _send_propfind(parts.host, parts.port, request_path, auth, 1)
		
		if result[0] != 207:
			had_scan_errors = true
			# ADDED: Essential debugging print statement
			print("WebDAVService Error: PROPFIND failed for path '%s' with status code: %d. Server message: %s" % [request_path, result[0], result[2]])
			continue
			
		var body_str: String = result[2]
		
		# 1. Parse tracks
		var tracks_at_level := _parse_webdav_xml(body_str)
		all_discovered_tracks.append_array(tracks_at_level)
		
		# 2. Parse subfolders safely
		var subfolders_at_level := _discover_folders_from_xml(body_str)
		for sub_folder: String in subfolders_at_level:
			var norm_sub := _normalize_path(sub_folder)
			var norm_current := _normalize_path(current_scan_path)
			
			if norm_sub != norm_current and not scanned_paths.has(norm_sub) and not folder_queue.has(norm_sub):
				if norm_sub.begins_with(norm_current):
					folder_queue.append(norm_sub)
				
	disconnect_active_connection()
	var duration := (Time.get_ticks_usec() - scan_start_time) / 1000000.0
	print("WebDAVService: Deep traversal finished in %.3fs. Found %d total tracks." % [duration, all_discovered_tracks.size()])

	is_scanning = false

	if had_scan_errors:
		print("WebDAVService: Scan encountered errors. Keeping current cached library list to avoid data loss.")
		library_scanned.emit(scanned_files)
		return

	# Compare with cached version
	var cache_changed := false
	if all_discovered_tracks.size() != scanned_files.size():
		cache_changed = true
	else:
		var sorted_new := all_discovered_tracks.duplicate()
		sorted_new.sort()
		var sorted_old := scanned_files.duplicate()
		sorted_old.sort()
		for i in range(sorted_new.size()):
			if sorted_new[i] != sorted_old[i]:
				cache_changed = true
				break

	if cache_changed:
		print("WebDAVService: Discrepancy detected between cache and server sync. Updating cache.")
		scanned_files = all_discovered_tracks
		save_cached_library()
		library_scanned.emit(all_discovered_tracks)
	else:
		print("WebDAVService: Sync finished. Server matches cache perfectly. (No UI rebuild required)")
		# Still emit to dismiss any explicit refresh/loading indicator
		library_scanned.emit(scanned_files)

## Internal XML structural extraction engines
func _discover_folders_from_xml(xml_content: String) -> Array:
	var folders := []
	var parser := XMLParser.new()
	parser.open_buffer(xml_content.to_utf8_buffer())
	
	var current_href := ""
	var is_directory := false
	
	while parser.read() == OK:
		if parser.get_node_type() == XMLParser.NODE_ELEMENT:
			var node_name := parser.get_node_name().to_lower()
			if node_name == "d:href" or node_name == "href":
				if parser.read() == OK:
					current_href = parser.get_node_data().strip_edges()
					is_directory = false
			if node_name == "d:collection" or node_name == "collection":
				is_directory = true
				
		elif parser.get_node_type() == XMLParser.NODE_ELEMENT_END:
			var node_name := parser.get_node_name().to_lower()
			if node_name == "d:response" or node_name == "response":
				if is_directory and not current_href.is_empty():
					folders.append(_normalize_path(current_href))
				current_href = ""
				is_directory = false
	return folders

func _parse_webdav_xml(xml_content: String) -> Array:
	var tracked_files := []
	var parser := XMLParser.new()
	parser.open_buffer(xml_content.to_utf8_buffer())

	while parser.read() == OK:
		if parser.get_node_type() == XMLParser.NODE_ELEMENT:
			var node_name := parser.get_node_name().to_lower()
			if node_name == "d:href" or node_name == "href":
				if parser.read() == OK and parser.get_node_type() == XMLParser.NODE_TEXT:
					var raw_href := parser.get_node_data().strip_edges()
					var lower_href := raw_href.to_lower()
					if lower_href.ends_with(".mp3") or lower_href.ends_with(".flac") \
							or lower_href.ends_with(".ogg") or lower_href.ends_with(".wav"):
						var clean_path = raw_href
						if "://" in clean_path:
							var path_parts = clean_path.split("://", true, 1)[1]
							if "/" in path_parts:
								clean_path = path_parts.substr(path_parts.find("/"))
						tracked_files.append(clean_path)
	return tracked_files

func _safe_get_string(bytes: PackedByteArray) -> String:
	var clean := PackedByteArray()
	for b in bytes:
		if b != 0:
			clean.append(b)
	return clean.get_string_from_utf8()

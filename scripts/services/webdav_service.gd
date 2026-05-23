extends Node
## WebDAVService
##
## Handles direct HTTP communication with WebDAV servers using raw TCP/TLS sockets,
## because Godot 4 does not support custom HTTP methods like PROPFIND natively.
## This allows us to send a real PROPFIND request (required by WebDAV spec).

signal library_scanned(files: Array)
signal connection_tested(success: bool, error_message: String)

const TCP_TIMEOUT_MS := 5000
const TLS_TIMEOUT_MS := 5000
const READ_TIMEOUT_MS := 10000

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
	req += "Connection: close\r\n"
	req += "\r\n"
	req += body
	return req

## Sends a raw PROPFIND request over TLS and returns [response_code, header_string, body_string].
## Returns [-1, "", ""] on network failure.
func _send_propfind(host: String, port: int, path: String, auth_header: String, depth: int = 1) -> Array:
	# --- TCP connect ---
	var tcp := StreamPeerTCP.new()
	var err := tcp.connect_to_host(host, port)
	if err != OK:
		return [-1, "", "TCP connect failed: %d" % err]

	var start := Time.get_ticks_msec()
	while tcp.get_status() == StreamPeerTCP.STATUS_CONNECTING:
		tcp.poll()
		await Engine.get_main_loop().process_frame
		if Time.get_ticks_msec() - start > TCP_TIMEOUT_MS:
			return [-1, "", "TCP connection timed out"]

	if tcp.get_status() != StreamPeerTCP.STATUS_CONNECTED:
		return [-1, "", "TCP connect status error: %d" % tcp.get_status()]

	# --- TLS handshake ---
	var tls := StreamPeerTLS.new()
	err = tls.connect_to_stream(tcp, host, TLSOptions.client())
	if err != OK:
		return [-1, "", "TLS setup failed: %d" % err]

	start = Time.get_ticks_msec()
	while true:
		tls.poll()
		var tls_status := tls.get_status()
		if tls_status == StreamPeerTLS.STATUS_CONNECTED:
			break
		elif tls_status == StreamPeerTLS.STATUS_ERROR or tls_status == StreamPeerTLS.STATUS_DISCONNECTED:
			return [-1, "", "TLS handshake failed: status %d" % tls_status]
		await Engine.get_main_loop().process_frame
		if Time.get_ticks_msec() - start > TLS_TIMEOUT_MS:
			return [-1, "", "TLS handshake timed out"]

	# --- Send PROPFIND ---
	var request_str := _build_propfind_request(host, path, auth_header, depth)
	err = tls.put_data(request_str.to_utf8_buffer())
	if err != OK:
		return [-1, "", "Failed to send request: %d" % err]

	# --- Read response ---
	var response_bytes := PackedByteArray()
	var done := false
	start = Time.get_ticks_msec()

	while not done:
		tls.poll()
		var tls_status := tls.get_status()
		if tls_status == StreamPeerTLS.STATUS_CONNECTED:
			var available := tls.get_available_bytes()
			if available > 0:
				var chunk := tls.get_data(available)
				if chunk[0] == OK:
					response_bytes.append_array(chunk[1])
					start = Time.get_ticks_msec()
		else:
			done = true

		if Time.get_ticks_msec() - start > READ_TIMEOUT_MS:
			break

		if not done:
			await Engine.get_main_loop().process_frame

	tcp.disconnect_from_host()

	if response_bytes.is_empty():
		return [-1, "", "Empty response from server"]

	# --- Parse HTTP response ---
	var header_end := -1
	for i in range(response_bytes.size() - 3):
		if response_bytes[i] == 13 and response_bytes[i+1] == 10 \
				and response_bytes[i+2] == 13 and response_bytes[i+3] == 10:
			header_end = i + 4
			break

	if header_end == -1:
		return [-1, "", "Malformed HTTP response (no header delimiter)"]

	var header_str := response_bytes.slice(0, header_end).get_string_from_utf8()
	var body_str := response_bytes.slice(header_end).get_string_from_utf8()

	# Parse status code from first line, e.g. "HTTP/1.1 207 Multi-Status"
	var status_line := header_str.split("\r\n")[0]
	var status_parts := status_line.split(" ", true, 2)
	var response_code := status_parts[1].to_int() if status_parts.size() >= 2 else -1

	return [response_code, header_str, body_str]

## Helper to filter absolute URLs down to clean relative paths for socket delivery.
func _sanitize_href_path(href: String, host_domain: String) -> String:
	if href.begins_with("http://") or href.begins_with("https://"):
		var split_parts := href.split("://" + host_domain, true, 1)
		if split_parts.size() == 2:
			return split_parts[1]
		# Fallback split configuration if port bindings differ
		var secondary_split := href.split("://", true, 1)
		var slash_index := secondary_split[1].find("/")
		if slash_index != -1:
			return secondary_split[1].substr(slash_index)
	return href

# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

## Tests if the provided credentials can connect to the server via PROPFIND.
## Emits connection_tested(success, error_message) when done.
func test_connection(url: String, username: String, app_password: String) -> void:
	var parts := _parse_url(url)
	var auth := _get_auth_header(username, app_password)
	
	var test_path: String = parts.path
	if not test_path.ends_with("/"):
		test_path += "/"
		
	var result := await _send_propfind(parts.host, parts.port, test_path, auth, 0)
	var response_code: int = result[0]
	var error_body: String = result[2]

	match response_code:
		207:
			connection_tested.emit(true, "")
		401, 403:
			connection_tested.emit(false, "Authentication failed. Check your username and app password.")
		404:
			connection_tested.emit(false, "URL not found on server. Check the WebDAV path.")
		-1:
			connection_tested.emit(false, "Connection failed: " + error_body)
		_:
			connection_tested.emit(false, "Server returned unexpected code: %d" % response_code)

## Scans the targeted music folder and safely indexes all nested subfolders sequentially.
## Pass the folder name directly from your UI text settings into 'target_folder'.
func scan_music_directory(target_folder: String = "Music") -> void:
	if not SettingsManager.has_credentials():
		return

	var base_url: String = SettingsManager.webdav_url
	var parts := _parse_url(base_url)
	var auth := _get_auth_header(SettingsManager.webdav_username, SettingsManager.webdav_password)
	
	# Clean up the initial base path structure
	var base_path: String = parts.path
	if not base_path.ends_with("/"):
		base_path += "/"
		
	# If target_folder is blank or "/", scan the root directory directly.
	# Otherwise, append the user's custom folder name cleanly.
	var initial_path: String = base_path
	if not target_folder.is_empty() and target_folder != "/":
		initial_path = base_path + target_folder.strip_edges().uri_encode()
		if not initial_path.ends_with("/"):
			initial_path += "/"
			
	var folder_queue: Array[String] = [initial_path]
	var all_discovered_tracks := []
	var scanned_paths: Array[String] = []

	print("WebDAVService: Starting deep library traversal at target path: %s" % initial_path)

	while not folder_queue.is_empty():
		var current_scan_path: String = folder_queue.pop_front()
		
		# Keep track of what we already scanned to completely prevent circular reference infinite loops
		if scanned_paths.has(current_scan_path):
			continue
		scanned_paths.append(current_scan_path)
		
		var result := await _send_propfind(parts.host, parts.port, current_scan_path, auth, 1)
		if result[0] != 207:
			print("WebDAVService: Skipping unreachable directory level: %s (Code %d)" % [current_scan_path, result[0]])
			continue
			
		var body_str: String = result[2]
		
		# 1. Extract music files from this folder layer
		var tracks_at_level := _parse_webdav_xml(body_str, parts.host)
		all_discovered_tracks.append_array(tracks_at_level)
		
		# 2. Extract nested subfolders at this layer (like "Vanilla - Origin")
		var subfolders_at_level := _discover_folders_from_xml(body_str, parts.host)
		for sub_folder: String in subfolders_at_level:
			# FIX: Standardize comparisons using standard string formats 
			# so spaces ("%20") don't confuse the duplicate manager.
			var clean_sub := sub_folder.strip_edges()
			var clean_current := current_scan_path.strip_edges()
			
			if clean_sub != clean_current and not scanned_paths.has(clean_sub) and not folder_queue.has(clean_sub):
				# Make sure the sub_folder is actually a child directory of our current path
				if clean_sub.begins_with(clean_current) or clean_sub.uri_decode().begins_with(clean_current.uri_decode()):
					folder_queue.append(clean_sub)
				
	print("WebDAVService: Deep traversal finished. Found %d total tracks." % all_discovered_tracks.size())
	library_scanned.emit(all_discovered_tracks)


## Internal helper to extract subdirectory href targets from a parent directory XML payload.
func _discover_folders_from_xml(xml_content: String, host_domain: String) -> Array:
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
					var raw_href := parser.get_node_data().strip_edges()
					current_href = _sanitize_href_path(raw_href, host_domain)
					is_directory = false
					
			if node_name == "d:collection" or node_name == "collection":
				is_directory = true
				
		elif parser.get_node_type() == XMLParser.NODE_ELEMENT_END:
			var node_name := parser.get_node_name().to_lower()
			if node_name == "d:response" or node_name == "response":
				if is_directory and not current_href.is_empty():
					if not current_href.ends_with("/"):
						current_href += "/"
					folders.append(current_href)
				current_href = ""
				is_directory = false
					
	return folders

## Parses audio file hrefs out of a WebDAV Multi-Status XML response.
func _parse_webdav_xml(xml_content: String, host_domain: String = "") -> Array:
	var tracked_files := []
	var parser := XMLParser.new()
	parser.open_buffer(xml_content.to_utf8_buffer())

	var current_href := ""

	while parser.read() == OK:
		if parser.get_node_type() == XMLParser.NODE_ELEMENT:
			var node_name := parser.get_node_name().to_lower()
			if node_name == "d:href" or node_name == "href":
				if parser.read() == OK:
					var raw_href := parser.get_node_data().strip_edges()
					current_href = _sanitize_href_path(raw_href, host_domain)
					var lower_href := current_href.to_lower()
					if lower_href.ends_with(".mp3") or lower_href.ends_with(".flac") \
							or lower_href.ends_with(".ogg") or lower_href.ends_with(".wav"):
						tracked_files.append(current_href)

	return tracked_files

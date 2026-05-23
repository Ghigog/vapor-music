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
	var body := '<?xml version="1.0" encoding="utf-8" ?><d:propfind xmlns:d="DAV:"><d:prop><d:displayname/><d:getcontentlength/><d:resourcetype/></d:prop></d:propfind>'
	var body_bytes := body.to_utf8_buffer()
	var req := "PROPFIND %s HTTP/1.1\r\n" % path
	req += "Host: %s\r\n" % host
	req += auth_header + "\r\n"
	req += "Depth: %d\r\n" % depth
	req += "Content-Type: application/xml; charset=utf-8\r\n"
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

# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

## Tests if the provided credentials can connect to the server via PROPFIND.
## Emits connection_tested(success, error_message) when done.
func test_connection(url: String, username: String, app_password: String) -> void:
	var parts := _parse_url(url)
	var auth := _get_auth_header(username, app_password)
	var result := await _send_propfind(parts.host, parts.port, parts.path, auth, 0)
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

## Scans the target music folder on the server using stored credentials.
## Emits library_scanned(files) on success.
func scan_music_directory(target_folder: String = "Vapor Music") -> void:
	if not SettingsManager.has_credentials():
		push_error("WebDAVService: No credentials stored.")
		return

	var base_url: String = SettingsManager.webdav_url
	var parts := _parse_url(base_url)
	var scan_path: String = parts.path.rstrip("/") + "/" + target_folder
	var auth := _get_auth_header(SettingsManager.webdav_username, SettingsManager.webdav_password)

	var result := await _send_propfind(parts.host, parts.port, scan_path, auth, 1)
	var response_code: int = result[0]
	var body_str: String = result[2]

	if response_code != 207:
		push_error("WebDAVService: PROPFIND returned %d" % response_code)
		return

	var audio_files := _parse_webdav_xml(body_str)
	library_scanned.emit(audio_files)

## Parses audio file hrefs out of a WebDAV Multi-Status XML response.
func _parse_webdav_xml(xml_content: String) -> Array:
	var tracked_files := []
	var parser := XMLParser.new()
	parser.open_buffer(xml_content.to_utf8_buffer())

	var current_href := ""

	while parser.read() == OK:
		if parser.get_node_type() == XMLParser.NODE_ELEMENT:
			var node_name := parser.get_node_name().to_lower()
			# Strip namespace prefix (d:href, D:href → href)
			if node_name == "d:href" or node_name == "href":
				if parser.read() == OK:
					current_href = parser.get_node_data().strip_edges()
					var lower_href := current_href.to_lower()
					if lower_href.ends_with(".mp3") or lower_href.ends_with(".flac") \
							or lower_href.ends_with(".ogg") or lower_href.ends_with(".wav"):
						tracked_files.append(current_href)

	return tracked_files

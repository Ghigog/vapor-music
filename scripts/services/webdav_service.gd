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

## Network I/O runs on these worker threads, not the main thread.
##
## The previous implementation interleaved socket polling with
## `await process_frame`, which kept the UI alive but capped network progress at
## one poll per rendered frame — a full library scan took ~20s wall-clock largely
## by waiting for vsync. Workers poll on a millisecond cadence instead and hand
## results back to the main thread with call_deferred.
##
## Thread-safety rules this file follows:
##   - A worker owns its sockets exclusively (created and destroyed inside the
##     worker function; never stored on the Node).
##   - Workers read no mutable member state; every input is captured into a job
##     Dictionary on the main thread before the thread starts.
##   - All member writes and signal emissions happen on the main thread, in the
##     _on_*_complete handlers.
var _scan_thread: Thread = null
var _test_thread: Thread = null

## Sleep between socket polls on a worker, in ms. 2ms keeps a poll loop at
## ~500Hz — responsive without burning a core.
const POLL_INTERVAL_MS := 2

const FileUtil = preload("res://scripts/services/file_util.gd")
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
	if FileUtil.write_string_atomic(LIBRARY_CACHE_FILE, JSON.stringify(scanned_files)) == OK:
		print("WebDAVService: Library cache saved to disk.")

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
	req += "Connection: keep-alive\r\n" # Changed to keep-alive to protect the pipe
	req += "\r\n"
	req += body
	return req

# ---------------------------------------------------------------------------
# Worker-thread network layer
#
# Every function below prefixed _t_ runs ONLY on a worker thread. A connection
# context is a plain Dictionary {tcp, tls, host, port} owned by one worker —
# sockets never touch the Node's members, so no locking is needed.
# ---------------------------------------------------------------------------

func _t_connect(ctx: Dictionary) -> int:
	_t_disconnect(ctx)

	var tcp := StreamPeerTCP.new()
	var err := tcp.connect_to_host(ctx.host, ctx.port)
	if err != OK:
		return err

	var start := Time.get_ticks_msec()
	while tcp.get_status() == StreamPeerTCP.STATUS_CONNECTING:
		tcp.poll()
		OS.delay_msec(POLL_INTERVAL_MS)
		if Time.get_ticks_msec() - start > TCP_TIMEOUT_MS:
			return ERR_CONNECTION_ERROR

	if tcp.get_status() != StreamPeerTCP.STATUS_CONNECTED:
		return ERR_CONNECTION_ERROR

	var tls := StreamPeerTLS.new()
	err = tls.connect_to_stream(tcp, ctx.host, TLSOptions.client())
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
		OS.delay_msec(POLL_INTERVAL_MS)
		if Time.get_ticks_msec() - start > TLS_TIMEOUT_MS:
			return ERR_CONNECTION_ERROR

	ctx.tcp = tcp
	ctx.tls = tls
	return OK


func _t_is_alive(ctx: Dictionary) -> bool:
	if not ctx.get("tcp") or not ctx.get("tls"):
		return false
	ctx.tcp.poll()
	ctx.tls.poll()
	return ctx.tcp.get_status() == StreamPeerTCP.STATUS_CONNECTED \
		and ctx.tls.get_status() == StreamPeerTLS.STATUS_CONNECTED


func _t_disconnect(ctx: Dictionary) -> void:
	if ctx.get("tcp"):
		ctx.tcp.disconnect_from_host()
	ctx.tcp = null
	ctx.tls = null


## Reads one HTTP response off the context's TLS stream. Returns raw bytes
## (empty on timeout/closure). Completion is detected via Content-Length or the
## chunked terminal marker, mirroring the previous implementation exactly —
## only the wait between polls changed (frame await → millisecond sleep).
func _t_read_response(ctx: Dictionary) -> PackedByteArray:
	var response_bytes := PackedByteArray()
	var start := Time.get_ticks_msec()

	var headers_parsed := false
	var is_chunked := false
	var content_length := -1
	var header_end_idx := -1

	while true:
		var tls: StreamPeerTLS = ctx.get("tls")
		if not tls:
			break
		tls.poll()
		var status := tls.get_status()
		var available := tls.get_available_bytes()

		# ALWAYS consume bytes if they are waiting in the TLS buffer
		if available > 0:
			var chunk := tls.get_data(available)
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
			# Only exit due to a closed stream if our buffer is completely dry
			if status != StreamPeerTLS.STATUS_CONNECTED:
				break
			OS.delay_msec(POLL_INTERVAL_MS)

		if Time.get_ticks_msec() - start > READ_TIMEOUT_MS:
			print("WebDAVService: Reading window closed due to channel inactivity timeout.")
			break

	return response_bytes


## Sends a PROPFIND on the worker's connection and returns
## [response_code, header_string, body_string]. Reuses the context's live
## connection (keep-alive); reconnects and retries once on send failure or an
## empty response, matching the previous behaviour.
func _t_send_propfind(ctx: Dictionary, path: String, auth_header: String, depth: int = 1) -> Array:
	if not _t_is_alive(ctx):
		var err := _t_connect(ctx)
		if err != OK:
			return [-1, "", "Connection establishment failed: %d" % err]

	var request_str := _build_propfind_request(ctx.host, path, auth_header, depth)
	var response_bytes := PackedByteArray()

	# Attempt 1 on the (possibly reused) connection; attempt 2 on a fresh one.
	for attempt in range(2):
		var tls: StreamPeerTLS = ctx.get("tls")
		var send_err: int = tls.put_data(request_str.to_utf8_buffer()) if tls else FAILED
		if send_err == OK:
			response_bytes = _t_read_response(ctx)
			if not response_bytes.is_empty():
				break

		if attempt == 0:
			print("WebDAVService: Empty response/send failure, retrying with fresh connection...")
			var err := _t_connect(ctx)
			if err != OK:
				return [-1, "", "Reconnection failed: %d" % err]

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
	if _test_thread and _test_thread.is_alive():
		print("WebDAVService: Connection test already in progress. Ignoring request.")
		return
	if _test_thread:
		_test_thread.wait_to_finish()

	var parts := _parse_url(url)
	var test_path: String = parts.path
	if not test_path.ends_with("/"):
		test_path += "/"

	var job := {
		"host": parts.host,
		"port": parts.port,
		"path": test_path,
		"auth": _get_auth_header(username, app_password),
	}
	_test_thread = Thread.new()
	_test_thread.start(_test_worker.bind(job))


## Worker thread: single depth-0 PROPFIND against the server root.
func _test_worker(job: Dictionary) -> void:
	var ctx := {"tcp": null, "tls": null, "host": job.host, "port": job.port}
	var result := _t_send_propfind(ctx, job.path, job.auth, 0)
	_t_disconnect(ctx)
	call_deferred("_on_test_complete", result[0])


func _on_test_complete(response_code: int) -> void:
	if _test_thread:
		_test_thread.wait_to_finish()
		_test_thread = null
	if response_code == 207:
		connection_tested.emit(true, "")
	else:
		connection_tested.emit(false, "Server responded with code: %d" % response_code)


func scan_music_directory(target_folder: String = "") -> void:
	if not SettingsManager.has_credentials():
		return

	if is_scanning:
		print("WebDAVService: Scan already in progress. Ignoring request.")
		return
	is_scanning = true
	if _scan_thread:
		_scan_thread.wait_to_finish()

	# Capture every input on the main thread — the worker must not touch
	# SettingsManager or any other autoload state.
	var parts := _parse_url(SettingsManager.webdav_url)
	var auth := _get_auth_header(SettingsManager.webdav_username, SettingsManager.webdav_password)

	var base_path: String = parts.path
	if not base_path.ends_with("/"):
		base_path += "/"

	var actual_folder := target_folder
	if actual_folder.is_empty():
		actual_folder = SettingsManager.webdav_folder

	var initial_path: String = base_path
	if not actual_folder.is_empty() and actual_folder != "/":
		initial_path = base_path + actual_folder.strip_edges().uri_encode()
		if not initial_path.ends_with("/"):
			initial_path += "/"

	var job := {
		"host": parts.host,
		"port": parts.port,
		"auth": auth,
		"initial_path": _normalize_path(initial_path),
	}
	_scan_thread = Thread.new()
	_scan_thread.start(_scan_worker.bind(job))


## How many parallel connections drain the folder queue during a scan.
##
## The traversal is latency-bound, not bandwidth-bound: each folder costs one
## PROPFIND round-trip (~300ms to a remote WebDAV host), and folders can only
## be discovered after their parent's response arrives. Sequential traversal of
## ~65 folders therefore takes ~20s regardless of thread or main-loop hosting —
## measured 19.77s cooperative vs 19.34s single-threaded. Parallel drains
## overlap those round-trips.
const SCAN_PARALLELISM := 4

## Scan coordinator (runs on _scan_thread): spawns the drain workers, joins
## them, and reports. Traversal semantics are unchanged from the sequential
## version — each folder is PROPFINDed exactly once, children only enqueued
## under their parent's subtree — just overlapped across connections.
func _scan_worker(job: Dictionary) -> void:
	var scan_start_time := Time.get_ticks_usec()

	# Shared traversal state, guarded by `mutex`. `in_flight` counts folders
	# handed to a worker whose children may not be enqueued yet — the queue
	# being empty is NOT the end condition while any request is outstanding.
	var shared := {
		"mutex": Mutex.new(),
		"queue": [job.initial_path],
		"scanned": [],
		"tracks": [],
		"errors": false,
		"in_flight": 0,
	}

	print("WebDAVService: Starting clear deep traversal at: %s" % job.initial_path)

	var drains: Array[Thread] = []
	for i in range(SCAN_PARALLELISM):
		var t := Thread.new()
		t.start(_scan_drain.bind(shared, job))
		drains.append(t)
	for t in drains:
		t.wait_to_finish()

	var duration := (Time.get_ticks_usec() - scan_start_time) / 1000000.0
	print("WebDAVService: Deep traversal finished in %.3fs. Found %d total tracks." % [duration, shared.tracks.size()])

	call_deferred("_on_scan_complete", shared.tracks, shared.errors)


## Drain worker: pulls folders off the shared queue, PROPFINDs them on its own
## connection, merges results back under the mutex. Exits when the queue is
## empty and no other worker has a request in flight.
func _scan_drain(shared: Dictionary, job: Dictionary) -> void:
	var ctx := {"tcp": null, "tls": null, "host": job.host, "port": job.port}
	var mutex: Mutex = shared.mutex

	while true:
		mutex.lock()
		if shared.queue.is_empty():
			if shared.in_flight == 0:
				mutex.unlock()
				break
			mutex.unlock()
			# Another worker may still discover subfolders — wait for it.
			OS.delay_msec(10)
			continue

		var current_scan_path: String = shared.queue.pop_front()
		if shared.scanned.has(current_scan_path):
			mutex.unlock()
			continue
		shared.scanned.append(current_scan_path)
		shared.in_flight += 1
		mutex.unlock()

		var request_path: String = current_scan_path.uri_encode().replace("%2F", "/")
		var result := _t_send_propfind(ctx, request_path, job.auth, 1)

		if result[0] != 207:
			print("WebDAVService Error: PROPFIND failed for path '%s' with status code: %d. Server message: %s" % [request_path, result[0], result[2]])
			mutex.lock()
			shared.errors = true
			shared.in_flight -= 1
			mutex.unlock()
			continue

		# Parse outside the lock — only the merge needs exclusivity.
		var body_str: String = result[2]
		var tracks_at_level := _parse_webdav_xml(body_str)
		var subfolders_at_level := _discover_folders_from_xml(body_str)
		var norm_current := _normalize_path(current_scan_path)

		mutex.lock()
		shared.tracks.append_array(tracks_at_level)
		for sub_folder: String in subfolders_at_level:
			var norm_sub := _normalize_path(sub_folder)
			if norm_sub != norm_current and not shared.scanned.has(norm_sub) and not shared.queue.has(norm_sub):
				if norm_sub.begins_with(norm_current):
					shared.queue.append(norm_sub)
		shared.in_flight -= 1
		mutex.unlock()

	_t_disconnect(ctx)


## Main thread: reconciles scan results with the cache and notifies the UI.
func _on_scan_complete(all_discovered_tracks: Array, errors: bool) -> void:
	if _scan_thread:
		_scan_thread.wait_to_finish()
		_scan_thread = null

	is_scanning = false
	had_scan_errors = errors

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


func _exit_tree() -> void:
	# Joining is mandatory — leaking a running Thread at shutdown aborts in
	# debug builds. The workers hold no references to the tree, so a blocking
	# join here is safe (worst case: one read-timeout window).
	if _scan_thread:
		_scan_thread.wait_to_finish()
		_scan_thread = null
	if _test_thread:
		_test_thread.wait_to_finish()
		_test_thread = null

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
							or lower_href.ends_with(".ogg") or lower_href.ends_with(".wav") \
							or lower_href.ends_with(".m4a"):
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

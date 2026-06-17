extends Node
## AudioAnalyzer
##
## Analyzes audio files locally on a background thread to extract BPM, Key, Energy, and Dynamics.
## Integrates with MetadataService to cache the results.

signal analysis_completed(href: String, results: Dictionary)
signal prefetch_started(total: int)
signal prefetch_progress(downloaded: int, total: int)
signal prefetch_completed()
signal prefetch_stopped()

var _queue: Array[String] = []
var _thread: Thread
var _mutex: Mutex
var _semaphore: Semaphore
var _exit_thread := false
var _metadata_service: Node = null
var dsp: Node = null

var cache_dir := "user://audio_cache/"
var background_caching_active := false
var current_download_href := ""
var download_http_request: HTTPRequest = null
var _prefetch_queue: Array[String] = []
var _prefetch_total := 0
var _prefetch_downloaded := 0
var is_transitioning_active := false

func _ready() -> void:
	_mutex = Mutex.new()
	_semaphore = Semaphore.new()
	_metadata_service = get_node_or_null("/root/MetadataService")
	_thread = Thread.new()
	_thread.start(_thread_worker)
	
	if ClassDB.class_exists("AudioDSP"):
		dsp = ClassDB.instantiate("AudioDSP")
		add_child(dsp)
	else:
		print("AudioAnalyzer: AudioDSP class not registered in ClassDB yet.")
	
	var webdav = get_node_or_null("/root/WebDAVService")
	if is_instance_valid(webdav) and webdav.has_signal("library_scanned"):
		webdav.library_scanned.connect(_on_library_scanned)

func _exit_tree() -> void:
	_mutex.lock()
	_exit_thread = true
	_mutex.unlock()
	_semaphore.post()
	_thread.wait_to_finish()

## Enqueues a track for local analysis
func analyze_track(href: String, priority: bool = false) -> void:
	_mutex.lock()
	if _queue.has(href):
		_queue.erase(href)
	if priority:
		_queue.insert(0, href)
		print("AudioAnalyzer: Priority enqueued ", href)
	else:
		_queue.append(href)
		print("AudioAnalyzer: Enqueued ", href)
	_mutex.unlock()
	_semaphore.post()

## Returns how many tracks are currently waiting to be analyzed
func get_queue_size() -> int:
	_mutex.lock()
	var size := _queue.size()
	_mutex.unlock()
	return size

## Returns true if the track is both cached locally as an audio file and has its analysis data ready (bpm > 0).
func is_track_ready(href: String) -> bool:
	var metadata_service = _metadata_service
	if not is_instance_valid(metadata_service):
		metadata_service = get_node_or_null("/root/MetadataService")
	if not is_instance_valid(metadata_service):
		return false

	var meta: Dictionary = metadata_service.get_cached_metadata(href)
	if meta.is_empty() or meta.get("bpm", 0.0) <= 0.0:
		return false

	var ext: String = href.get_extension()
	if ext.is_empty():
		ext = "mp3"
	var cache_path: String = cache_dir + href.md5_text() + "." + ext
	return FileAccess.file_exists(cache_path)

## Returns the number of ready tracks in the provided array.
func get_ready_tracks_count(hrefs: Array) -> int:
	var count := 0
	for href in hrefs:
		if is_track_ready(href):
			count += 1
	return count

## Scans a list of hrefs and queues any that (a) have a local disk cache but (b) have no analysis
## data yet (bpm == 0). Runs on the main thread, safe to call any time.
func _on_library_scanned(hrefs: Array) -> void:
	# Automatically prune orphaned cache files every time the library is scanned/loaded
	prune_orphaned_cache_files(hrefs)
	scan_library_cache(hrefs)

func scan_library_cache(hrefs: Array) -> void:
	var metadata_service = _metadata_service
	if not is_instance_valid(metadata_service):
		return

	var queued := 0
	var already_done := 0
	var no_cache := 0

	for href in hrefs:
		# Skip if already analyzed
		var meta: Dictionary = metadata_service.get_cached_metadata(href)
		if not meta.is_empty() and meta.get("bpm", 0.0) > 0.0:
			already_done += 1
			continue

		# Check if local audio cache file exists
		var ext: String = href.get_extension()
		if ext.is_empty():
			ext = "mp3"
		var cache_path: String = cache_dir + href.md5_text() + "." + ext
		if not FileAccess.file_exists(cache_path):
			no_cache += 1
			continue

		# Queue it
		_mutex.lock()
		if not _queue.has(href):
			_queue.append(href)
			queued += 1
		_mutex.unlock()
		_semaphore.post()

	print("AudioAnalyzer: Library scan — queued=%d, already_analyzed=%d, not_cached=%d" % [queued, already_done, no_cache])

func prune_orphaned_cache_files(hrefs: Array) -> void:
	var valid_hashes := {}
	for href in hrefs:
		var ext: String = href.get_extension()
		if ext.is_empty():
			ext = "mp3"
		var filename = href.md5_text() + "." + ext
		valid_hashes[filename] = true

	var dir = DirAccess.open(cache_dir)
	if dir:
		dir.list_dir_begin()
		var file_name = dir.get_next()
		while file_name != "":
			if not dir.current_is_dir():
				if file_name.ends_with(".tmp"):
					var base_name = file_name
					if file_name.ends_with(".analyzer.tmp"):
						base_name = file_name.trim_suffix(".analyzer.tmp")
					elif file_name.ends_with(".manager.tmp"):
						base_name = file_name.trim_suffix(".manager.tmp")
					else:
						base_name = file_name.trim_suffix(".tmp")
					if not valid_hashes.has(base_name):
						dir.remove(file_name)
						print("AudioAnalyzer: Pruned orphaned temp file: ", file_name)
				else:
					if not valid_hashes.has(file_name):
						dir.remove(file_name)
						print("AudioAnalyzer: Pruned orphaned cache file: ", file_name)
			file_name = dir.get_next()
		dir.list_dir_end()

func start_prefetching(hrefs: Array) -> void:
	# Prioritize upcoming track override download if uncached
	var next_track = ""
	var audio_manager = get_node_or_null("/root/AudioManager")
	if is_instance_valid(audio_manager):
		next_track = audio_manager.get_next_track_href()
		
	var next_track_uncached = false
	if not next_track.is_empty():
		var ext: String = next_track.get_extension()
		if ext.is_empty():
			ext = "mp3"
		var cache_path: String = cache_dir + next_track.md5_text() + "." + ext
		if not FileAccess.file_exists(cache_path):
			next_track_uncached = true
			
	if next_track_uncached and current_download_href != next_track:
		if not current_download_href.is_empty():
			if is_instance_valid(download_http_request):
				download_http_request.cancel_request()
				download_http_request.queue_free()
				download_http_request = null
			
			var ext: String = current_download_href.get_extension()
			if ext.is_empty():
				ext = "mp3"
			var temp_path = cache_dir + current_download_href.md5_text() + "." + ext + ".analyzer.tmp"
			if FileAccess.file_exists(temp_path):
				DirAccess.remove_absolute(temp_path)
				
			print("AudioAnalyzer: Cancelled download of ", current_download_href, " to prioritize next track: ", next_track)
			current_download_href = ""

	# Ensure all cached files are scanned and queued for background analysis
	scan_library_cache(hrefs)
	
	# Find uncached files
	var new_queue: Array[String] = []
	for href in hrefs:
		var ext: String = href.get_extension()
		if ext.is_empty():
			ext = "mp3"
		var cache_path: String = cache_dir + href.md5_text() + "." + ext
		if not FileAccess.file_exists(cache_path):
			new_queue.append(href)
			
	# Move next_track to the front of the queue to prioritize it
	if not next_track.is_empty() and new_queue.has(next_track):
		new_queue.erase(next_track)
		new_queue.push_front(next_track)
			
	var ready_count = get_ready_tracks_count(hrefs)
			
	if background_caching_active:
		# If a download is active, check if it is still in the new look-ahead window
		if not current_download_href.is_empty():
			if not hrefs.has(current_download_href):
				# Cancel current download request
				if is_instance_valid(download_http_request):
					download_http_request.cancel_request()
					download_http_request.queue_free()
					download_http_request = null
				
				# Remove temp file
				var ext: String = current_download_href.get_extension()
				if ext.is_empty():
					ext = "mp3"
				var temp_path = cache_dir + current_download_href.md5_text() + "." + ext + ".analyzer.tmp"
				if FileAccess.file_exists(temp_path):
					DirAccess.remove_absolute(temp_path)
					
				print("AudioAnalyzer: Cancelled download of ", current_download_href, " (no longer in look-ahead)")
				current_download_href = ""
			else:
				# Keep current download, but erase from new queue
				new_queue.erase(current_download_href)
				
		_prefetch_queue = new_queue
		_prefetch_total = hrefs.size()
		_prefetch_downloaded = ready_count
		
		prefetch_progress.emit(_prefetch_downloaded, _prefetch_total)
		
		if current_download_href.is_empty():
			if not _prefetch_queue.is_empty():
				_download_next_prefetch()
			else:
				if _prefetch_downloaded >= _prefetch_total:
					background_caching_active = false
					prefetch_completed.emit()
	else:
		background_caching_active = true
		_prefetch_queue = new_queue
		_prefetch_total = hrefs.size()
		_prefetch_downloaded = ready_count
		
		prefetch_started.emit(_prefetch_total)
		prefetch_progress.emit(_prefetch_downloaded, _prefetch_total)
		
		if _prefetch_queue.is_empty():
			if _prefetch_downloaded >= _prefetch_total:
				background_caching_active = false
				prefetch_completed.emit()
			return
			
		_download_next_prefetch()

func stop_prefetching() -> void:
	if not background_caching_active:
		return
	background_caching_active = false
	_prefetch_queue.clear()
	if is_instance_valid(download_http_request):
		download_http_request.cancel_request()
		download_http_request.queue_free()
		download_http_request = null
	prefetch_stopped.emit()
	print("AudioAnalyzer: Pre-fetching stopped.")

func _download_next_prefetch() -> void:
	if not background_caching_active:
		return
		
	if _prefetch_queue.is_empty():
		current_download_href = ""
		
		# Check if background analysis is also finished
		_mutex.lock()
		var queue_empty = _queue.is_empty()
		_mutex.unlock()
		
		var webdav = get_node_or_null("/root/WebDAVService")
		var ready_count = 0
		if is_instance_valid(webdav):
			ready_count = get_ready_tracks_count(webdav.scanned_files)
			
		if ready_count >= _prefetch_total or queue_empty:
			background_caching_active = false
			prefetch_completed.emit()
		return
		
	var href: String = _prefetch_queue.pop_front()
	current_download_href = href
	
	var ext: String = href.get_extension()
	if ext.is_empty():
		ext = "mp3"
	var cache_path: String = cache_dir + href.md5_text() + "." + ext
	var temp_download_path := cache_path + ".analyzer.tmp"
	
	if FileAccess.file_exists(cache_path):
		call_deferred("_download_next_prefetch")
		return
		
	if not DirAccess.dir_exists_absolute(cache_dir):
		DirAccess.make_dir_recursive_absolute(cache_dir)
		
	var base_url: String = SettingsManager.webdav_url
	var username := SettingsManager.webdav_username
	var password := SettingsManager.webdav_password
	
	var auth_raw := "%s:%s" % [username, password]
	var auth_header := "Authorization: Basic %s" % Marshalls.utf8_to_base64(auth_raw)
	
	var url_parts = base_url.split("/dav")
	var host_base = url_parts[0]
	
	var clean_href = href
	if not clean_href.begins_with("/"):
		clean_href = "/" + clean_href
	
	var completely_decoded = clean_href.uri_decode()
	var safe_encoded_path = completely_decoded.uri_encode().replace("%2F", "/")
	var full_target_endpoint = host_base + safe_encoded_path
	
	download_http_request = HTTPRequest.new()
	add_child(download_http_request)
	download_http_request.timeout = 15.0
	download_http_request.set_tls_options(TLSOptions.client_unsafe())
	download_http_request.download_file = temp_download_path
	
	download_http_request.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		_on_prefetch_download_completed(href, cache_path, temp_download_path, response_code)
	)
	
	var err := download_http_request.request(full_target_endpoint, [auth_header], HTTPClient.METHOD_GET)
	if err != OK:
		print("AudioAnalyzer prefetch error: request failed for ", href)
		if FileAccess.file_exists(temp_download_path):
			DirAccess.remove_absolute(temp_download_path)
		download_http_request.queue_free()
		download_http_request = null
		call_deferred("_download_next_prefetch")

func _on_prefetch_download_completed(href: String, cache_path: String, temp_path: String, response_code: int) -> void:
	if not is_instance_valid(download_http_request):
		return
	download_http_request.queue_free()
	download_http_request = null
	
	if response_code != 200:
		print("AudioAnalyzer prefetch: download failed code %d for %s" % [response_code, href])
		if FileAccess.file_exists(temp_path):
			DirAccess.remove_absolute(temp_path)
	else:
		if FileAccess.file_exists(cache_path):
			print("AudioAnalyzer prefetch: cache already exists for ", href)
			if FileAccess.file_exists(temp_path):
				DirAccess.remove_absolute(temp_path)
			analyze_track(href, true)
		else:
			var err = DirAccess.rename_absolute(temp_path, cache_path)
			if err != OK:
				print("AudioAnalyzer prefetch: rename error: ", err)
			else:
				print("AudioAnalyzer prefetch: downloaded ", href)
				analyze_track(href, true)
			
	_download_next_prefetch()

func _thread_worker() -> void:
	while true:
		_semaphore.wait()
		
		_mutex.lock()
		if _exit_thread:
			_mutex.unlock()
			break
			
		if _queue.is_empty():
			_mutex.unlock()
			continue
			
		var href: String = _queue.pop_front()
		_mutex.unlock()
		
		# Suspend popping/analyzing during active transitions
		while is_transitioning_active:
			OS.delay_msec(100)
			_mutex.lock()
			var should_exit = _exit_thread
			_mutex.unlock()
			if should_exit:
				break
				
		if _exit_thread:
			break
		
		var results := _perform_analysis(href)
		call_deferred("_on_analysis_completed", href, results)

func _on_analysis_completed(href: String, results: Dictionary) -> void:
	if results.is_empty():
		print("AudioAnalyzer: Analysis failed for ", href)
		return
		
	print("AudioAnalyzer: Analysis complete for ", href, ": BPM=", results.bpm, ", Key=", results.musical_key)
	
	# Update MetadataService cache — always persist, even if the entry is new
	var metadata_service = _metadata_service
	if is_instance_valid(metadata_service):
		var existing: Dictionary = metadata_service.get_cached_metadata(href)
		for key in results.keys():
			existing[key] = results[key]
		metadata_service.cache[href] = existing
		metadata_service.save_cache()
		metadata_service.metadata_updated.emit(href, existing)
		
		# Trigger a full metadata lookup (async) so genre, artist image, etc. are populated.
		# Only needed if genre is still unknown, to avoid redundant API calls.
		var genre: String = existing.get("genre", "Unknown")
		if genre == "Unknown":
			var info = metadata_service.parse_track_info(href)
			metadata_service.lookup_metadata(href, info.artist, info.album, info.track)
		
	analysis_completed.emit(href, results)
	
	if background_caching_active:
		var webdav = get_node_or_null("/root/WebDAVService")
		if is_instance_valid(webdav):
			var scanned = webdav.scanned_files
			var ready_count = get_ready_tracks_count(scanned)
			_prefetch_downloaded = ready_count
			prefetch_progress.emit(ready_count, _prefetch_total)
			
			_mutex.lock()
			var queue_empty = _queue.is_empty()
			_mutex.unlock()
			
			if ready_count >= _prefetch_total or (_prefetch_queue.is_empty() and current_download_href.is_empty() and queue_empty):
				background_caching_active = false
				prefetch_completed.emit()

func _perform_analysis(href: String) -> Dictionary:
	var ext := href.get_extension().to_lower()
	if ext.is_empty():
		ext = "mp3"
		
	var cache_path := cache_dir + href.md5_text() + "." + ext
	if not FileAccess.file_exists(cache_path):
		print("AudioAnalyzer: Cache file does not exist: ", cache_path)
		return {}
		
	var abs_path = ProjectSettings.globalize_path(cache_path)
	
	if not is_instance_valid(dsp):
		print("AudioAnalyzer: AudioDSP not available, cannot analyze file.")
		return {}
			
	print("AudioAnalyzer: Analyzing via C++ AudioDSP: ", abs_path)
	var results = dsp.analyze_file(abs_path)
	if not results.is_empty():
		results["vocal_presence"] = results.get("energy_level", 0.5) > 0.35
	return results

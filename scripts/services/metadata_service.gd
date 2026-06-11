extends Node
## MetadataService
##
## Fetches and caches artist image, album art, and lyrics from public APIs (Deezer and LRCLIB).
## Cache is saved to user://metadata_cache.json and entries are pruned when files are removed.

signal artist_focused(artist: String, image_path: String)
signal album_focused(artist: String, album: String, image_path: String)
signal track_focused(artist: String, album: String, title: String, lyrics: Dictionary, image_path: String)
signal lyrics_fetched(href: String, lyrics: Dictionary)
signal metadata_updated(href: String, metadata: Dictionary)

var cache_file_path = "user://metadata_cache.json"

var cache: Dictionary = {}
var _save_timer: Timer
var _pending_lookups: Dictionary = {}

func _ready() -> void:
	load_cache()
	if WebDAVService.has_signal("library_scanned"):
		WebDAVService.library_scanned.connect(prune_cache)

	# Set up save debounce timer
	_save_timer = Timer.new()
	_save_timer.one_shot = true
	_save_timer.wait_time = 2.0
	_save_timer.timeout.connect(_perform_save_cache)
	add_child(_save_timer)

func _exit_tree() -> void:
	if _save_timer and not _save_timer.is_stopped():
		_save_timer.stop()
		_perform_save_cache()

## Loads the metadata cache from user://metadata_cache.json
func load_cache() -> void:
	if FileAccess.file_exists(cache_file_path):
		var file := FileAccess.open(cache_file_path, FileAccess.READ)
		if file:
			var content := file.get_as_text()
			file.close()
			var parsed = JSON.parse_string(content)
			if parsed is Dictionary:
				cache = parsed
				return
	cache = {}

func _perform_save_cache() -> void:
	var file := FileAccess.open(cache_file_path, FileAccess.WRITE)
	if file:
		file.store_string(JSON.stringify(cache, "\t"))
		file.close()
		print("MetadataService: Cache written to disk.")

## Saves the current metadata cache to user://metadata_cache.json
func save_cache(force: bool = false) -> void:
	if force:
		if _save_timer:
			_save_timer.stop()
		_perform_save_cache()
	else:
		if _save_timer:
			_save_timer.start()

## Prunes any cache entries for track hrefs that are no longer present in the library scan.
func prune_cache(active_files: Array) -> void:
	# Protect against transient scan errors causing mass cache erasure.
	# If the WebDAV scan encountered errors, skip pruning.
	if is_instance_valid(WebDAVService) and WebDAVService.get("had_scan_errors"):
		print("MetadataService: Skipping cache prune because the library scan encountered network errors (avoiding data loss).")
		return
		
	var active_set := {}
	for file in active_files:
		active_set[file] = true

	var keys_to_remove := []
	for cached_href in cache.keys():
		if not active_set.has(cached_href):
			keys_to_remove.append(cached_href)

	if not keys_to_remove.is_empty():
		for key in keys_to_remove:
			cache.erase(key)
		save_cache(true)
		print("MetadataService: Pruned %d orphaned cache entries." % keys_to_remove.size())

## Returns cached metadata dictionary for a track href, or empty dictionary if not found.
func get_cached_metadata(href: String) -> Dictionary:
	if cache.has(href):
		return cache[href]
	return {}

## Helper to make a standard HTTP GET request and return the body as string or null.
func _make_http_request(url: String) -> String:
	var http := HTTPRequest.new()
	add_child(http)
	http.set_tls_options(TLSOptions.client())
	
	var err := http.request(url, ["User-Agent: VaporMusicPlayer/1.0 (Godot)"], HTTPClient.METHOD_GET)
	if err != OK:
		print("MetadataService HTTP Error: Request to %s failed to initialize: %d" % [url, err])
		http.queue_free()
		return ""
		
	var response = await http.request_completed
	var response_code: int = response[1]
	var response_body: PackedByteArray = response[3]
	http.queue_free()
	
	print("MetadataService HTTP: Request to %s completed with status code %d" % [url, response_code])
	if response_code == 200:
		return _safe_get_string(response_body)
	return ""

## Downloads an image to user://metadata_images/ and returns the local path. Returns empty string if failed.
func _download_image(url: String) -> String:
	if url.is_empty() or not url.begins_with("http"):
		return url
		
	var ext: String = url.get_extension()
	if ext.is_empty() or ext.length() > 4:
		ext = "jpg" # Default fallback
		
	if "?" in ext:
		ext = ext.split("?")[0]
		
	var local_dir: String = "user://metadata_images/"
	if not DirAccess.dir_exists_absolute(local_dir):
		DirAccess.make_dir_recursive_absolute(local_dir)
		
	var local_filename: String = url.md5_text() + "." + ext
	var local_path: String = local_dir + local_filename
	
	if FileAccess.file_exists(local_path):
		return local_path
		
	var http := HTTPRequest.new()
	add_child(http)
	http.set_tls_options(TLSOptions.client())
	
	var err := http.request(url, ["User-Agent: VaporMusicPlayer/1.0 (Godot)"], HTTPClient.METHOD_GET)
	if err != OK:
		http.queue_free()
		return ""
		
	var response = await http.request_completed
	var response_code: int = response[1]
	var response_body: PackedByteArray = response[3]
	http.queue_free()
	
	if response_code == 200 and response_body.size() > 0:
		var file := FileAccess.open(local_path, FileAccess.WRITE)
		if file:
			file.store_buffer(response_body)
			file.close()
			return local_path
			
	return ""

## Fetches the artist image URL from Deezer API.
func fetch_artist_image(artist: String) -> String:
	if artist.strip_edges().is_empty() or artist == "Unknown Artist":
		return ""
		
	var url: String = "https://api.deezer.com/search/artist?q=%s" % artist.uri_encode()
	var response_str: String = await _make_http_request(url)
	if response_str.is_empty():
		return ""
		
	var data = JSON.parse_string(response_str)
	if data is Dictionary and data.has("data") and data["data"] is Array and not data["data"].is_empty():
		var artist_data = data["data"][0]
		if artist_data is Dictionary:
			for key in ["picture_xl", "picture_big", "picture_medium", "picture_small"]:
				if artist_data.has(key) and not artist_data[key].is_empty():
					var val: String = artist_data[key]
					return val
	return ""

## Fetches the album art URL from Deezer API.
func fetch_album_art(artist: String, album: String) -> String:
	if album.strip_edges().is_empty() or album == "Unknown Album":
		return ""
		
	var query := album
	if not artist.is_empty() and artist != "Unknown Artist":
		query = "%s %s" % [artist, album]
		
	var url := "https://api.deezer.com/search/album?q=%s" % query.uri_encode()
	var response_str: String = await _make_http_request(url)
	if response_str.is_empty():
		# Fall back to simpler query if network failed
		url = "https://api.deezer.com/search/album?q=%s" % album.uri_encode()
		response_str = await _make_http_request(url)
		if response_str.is_empty():
			return ""
			
	var data = JSON.parse_string(response_str)
	if not (data is Dictionary and data.has("data") and data["data"] is Array and not data["data"].is_empty()):
		# Fall back to simpler query if empty data list returned
		url = "https://api.deezer.com/search/album?q=%s" % album.uri_encode()
		response_str = await _make_http_request(url)
		if not response_str.is_empty():
			data = JSON.parse_string(response_str)
			
	if data is Dictionary and data.has("data") and data["data"] is Array and not data["data"].is_empty():
		var album_data = data["data"][0]
		if album_data is Dictionary:
			for key in ["cover_xl", "cover_big", "cover_medium", "cover_small"]:
				if album_data.has(key) and not album_data[key].is_empty():
					var val: String = album_data[key]
					return val
	return ""

## Fetches the lyrics from LRCLIB API. Returns a Dictionary (synced or plain).
func fetch_lyrics(artist: String, title: String) -> Dictionary:
	if artist.strip_edges().is_empty() or artist == "Unknown Artist" or title.strip_edges().is_empty():
		return {}
		
	var url: String = "https://lrclib.net/api/get?artist_name=%s&track_name=%s" % [
		artist.uri_encode(),
		title.uri_encode()
	]
	var response_str: String = await _make_http_request(url)
	if response_str.is_empty():
		return {}
		
	var data = JSON.parse_string(response_str)
	if data is Dictionary:
		var synced_text = data.get("syncedLyrics", "")
		if synced_text is String and not synced_text.is_empty():
			var parsed = parse_lrc(synced_text)
			return {
				"synced": true,
				"lines": parsed
			}
		else:
			var plain_text = data.get("plainLyrics", "")
			if plain_text is String and not plain_text.is_empty():
				return {
					"synced": false,
					"plain": plain_text
				}
	return {}

## Parses LRC string format: [MM:SS.CC] Lyrics into structured Array
func parse_lrc(lrc_text: String) -> Array:
	var lines: Array = []
	var regex = RegEx.new()
	regex.compile("\\[(\\d+):(\\d+)\\.(\\d+)\\](.*)")
	
	var raw_lines = lrc_text.split("\n")
	for line in raw_lines:
		line = line.strip_edges()
		var match_obj = regex.search(line)
		if match_obj:
			var minutes = match_obj.get_string(1).to_float()
			var seconds = match_obj.get_string(2).to_float()
			var centiseconds = match_obj.get_string(3).to_float()
			var text = match_obj.get_string(4).strip_edges()
			
			var total_seconds = (minutes * 60.0) + seconds + (centiseconds / 100.0)
			lines.append({
				"time": total_seconds,
				"text": text
			})
	return lines

## Queries Deezer search to resolve artist, album, and correct track title using track_title
func resolve_metadata_via_search(track_title: String) -> Dictionary:
	if track_title.strip_edges().is_empty() or track_title == "Unknown Track":
		return {}
		
	var url: String = "https://api.deezer.com/search?q=%s" % track_title.uri_encode()
	var response_str: String = await _make_http_request(url)
	if response_str.is_empty():
		return {}
		
	var data = JSON.parse_string(response_str)
	if data is Dictionary and data.has("data") and data["data"] is Array and not data["data"].is_empty():
		var track_data = data["data"][0]
		if track_data is Dictionary:
			var artist_name := ""
			var album_title := ""
			var real_title := track_title
			var genre_name := "Unknown"
			
			if track_data.has("artist") and track_data["artist"] is Dictionary:
				artist_name = track_data["artist"].get("name", "")
			if track_data.has("album") and track_data["album"] is Dictionary:
				album_title = track_data["album"].get("title", "")
				var album_id = track_data["album"].get("id", 0)
				if album_id > 0:
					genre_name = await fetch_album_genre(album_id)
			if track_data.has("title"):
				real_title = track_data.get("title", "")
				
			if not artist_name.is_empty() and not album_title.is_empty():
				return {
					"artist": artist_name,
					"album": album_title,
					"title": real_title,
					"genre": genre_name
				}
	return {}

## Fetches the genre from the Deezer API for a given album ID.
func fetch_album_genre(album_id: int) -> String:
	if album_id <= 0:
		return ""
	var url := "https://api.deezer.com/album/%d" % album_id
	print("MetadataService: Requesting album details for ID %d: %s" % [album_id, url])
	var response_str := await _make_http_request(url)
	if response_str.is_empty():
		print("MetadataService: Failed to retrieve album details for ID %d" % album_id)
		return ""
	var data = JSON.parse_string(response_str)
	if data is Dictionary:
		if data.has("genres") and data["genres"] is Dictionary:
			var genres_data = data["genres"]
			if genres_data.has("data") and genres_data["data"] is Array and not genres_data["data"].is_empty():
				var genre_obj = genres_data["data"][0]
				if genre_obj is Dictionary and genre_obj.has("name"):
					return genre_obj.get("name", "")
	print("MetadataService: No genres found in response for album ID %d" % album_id)
	return ""

## Helper to sanitize/clean album queries by stripping trailing whitespace or common delimiters
func _clean_album_query(album_str: String) -> String:
	# Strip trailing whitespace and double spaces
	return album_str.strip_edges()

## Fetches the genre by searching for the album.
func fetch_genre_by_search(artist: String, album: String) -> String:
	var clean_album = _clean_album_query(album)
	if clean_album.is_empty() or clean_album == "Unknown Album":
		print("MetadataService: fetch_genre_by_search - album name is empty/unknown, skipping")
		return ""
	var query: String = clean_album
	if not artist.is_empty() and artist != "Unknown Artist":
		query = "%s %s" % [artist, clean_album]
	var url := "https://api.deezer.com/search/album?q=%s" % query.uri_encode()
	print("MetadataService: Searching Deezer album info for query='%s': %s" % [query, url])
	var response_str := await _make_http_request(url)
	if response_str.is_empty():
		print("MetadataService: Search request failed or empty, retrying with album name only...")
		url = "https://api.deezer.com/search/album?q=%s" % clean_album.uri_encode()
		response_str = await _make_http_request(url)
		if response_str.is_empty():
			print("MetadataService: Search request with album name only failed.")
			return ""
	
	var data = JSON.parse_string(response_str)
	if not (data is Dictionary and data.has("data") and data["data"] is Array and not data["data"].is_empty()):
		print("MetadataService: No search results returned for query '%s', retrying with album name only..." % query)
		url = "https://api.deezer.com/search/album?q=%s" % clean_album.uri_encode()
		response_str = await _make_http_request(url)
		if not response_str.is_empty():
			data = JSON.parse_string(response_str)
			
	if data is Dictionary and data.has("data") and data["data"] is Array and not data["data"].is_empty():
		var album_data = data["data"][0]
		if album_data is Dictionary and album_data.has("id"):
			var album_id = album_data["id"]
			var resolved = await fetch_album_genre(album_id)
			print("MetadataService: Resolved genre from album ID %d: '%s'" % [album_id, resolved])
			return resolved
	print("MetadataService: Could not resolve album genre from search results for query: '%s'" % query)
	return ""

## Parses basic ID3v2 tags (BPM, Key, Genre) from the beginning of an MP3 file.
func _parse_id3v2_tags(file_path: String) -> Dictionary:
	var tags: Dictionary = {
		"bpm": 0.0,
		"musical_key": "",
		"genre": "Unknown"
	}
	
	if file_path.is_empty() or not FileAccess.file_exists(file_path):
		return tags
		
	var file := FileAccess.open(file_path, FileAccess.READ)
	if not file:
		return tags
		
	# Check for "ID3" identifier
	var header_bytes: PackedByteArray = file.get_buffer(10)
	if header_bytes.size() < 10:
		file.close()
		return tags
		
	if header_bytes[0] != 0x49 or header_bytes[1] != 0x44 or header_bytes[2] != 0x33: # "ID3"
		file.close()
		return tags
		
	var version_major: int = header_bytes[3]
	if version_major < 2 or version_major > 4:
		file.close()
		return tags
		
	# Parse synchsafe tag size
	var tag_size: int = (
		((header_bytes[6] & 0x7F) << 21) |
		((header_bytes[7] & 0x7F) << 14) |
		((header_bytes[8] & 0x7F) << 7) |
		(header_bytes[9] & 0x7F)
	)
	
	# Read the tag payload, capping it to a reasonable size (e.g., 128KB) to avoid reading massive embedded images
	var max_read: int = min(tag_size, 128 * 1024)
	var tag_data: PackedByteArray = file.get_buffer(max_read)
	file.close()
	
	var offset: int = 0
	var tag_data_size: int = tag_data.size()
	
	while offset + 10 < tag_data_size:
		# Read frame ID (4 bytes for ID3v2.3/v2.4, 3 bytes for ID3v2.2)
		var frame_id: String = ""
		var frame_size: int = 0
		var header_len: int = 10
		
		var id_len := 3 if version_major == 2 else 4
		if offset + id_len > tag_data_size:
			break
			
		var id_bytes := tag_data.slice(offset, offset + id_len)
		var is_id_valid := true
		for b in id_bytes:
			if not ((b >= 48 and b <= 57) or (b >= 65 and b <= 90)): # '0'-'9' or 'A'-'Z'
				is_id_valid = false
				break
		if not is_id_valid:
			break # Hit padding / end of valid frames
			
		if version_major == 2:
			header_len = 6
			if offset + 6 > tag_data_size:
				break
			frame_id = _safe_get_string(id_bytes)
			frame_size = (tag_data[offset + 3] << 16) | (tag_data[offset + 4] << 8) | tag_data[offset + 5]
		else:
			# ID3v2.3 / ID3v2.4
			frame_id = _safe_get_string(id_bytes)
			if version_major == 4:
				# ID3v2.4 frame size is synchsafe
				frame_size = (
					((tag_data[offset + 4] & 0x7F) << 21) |
					((tag_data[offset + 5] & 0x7F) << 14) |
					((tag_data[offset + 6] & 0x7F) << 7) |
					(tag_data[offset + 7] & 0x7F)
				)
			else:
				# ID3v2.3 frame size is normal 32-bit int
				frame_size = (tag_data[offset + 4] << 24) | (tag_data[offset + 5] << 16) | (tag_data[offset + 6] << 8) | tag_data[offset + 7]
				
		if frame_size <= 0 or offset + header_len + frame_size > tag_data_size:
			break
			
		var frame_content: PackedByteArray = tag_data.slice(offset + header_len, offset + header_len + frame_size)
		offset += header_len + frame_size
		
		# We only care about text frames (starting with 'T')
		if frame_id.begins_with("T") and frame_content.size() > 1:
			var encoding: int = frame_content[0]
			var text_bytes: PackedByteArray = frame_content.slice(1)
			var decoded_text: String = ""
			
			if encoding == 0: # ISO-8859-1 (Latin-1)
				var chars := []
				for b in text_bytes:
					if b == 0:
						break
					chars.append(char(b))
				decoded_text = "".join(chars)
			elif encoding == 3: # UTF-8
				var clean_bytes := PackedByteArray()
				for b in text_bytes:
					if b == 0:
						break
					clean_bytes.append(b)
				decoded_text = _safe_get_string(clean_bytes)
			elif encoding == 1 or encoding == 2: # UTF-16 or UTF-16BE
				# Handle basic UTF-16 decoding by stripping BOM and nulls
				var clean_bytes := PackedByteArray()
				var start_byte: int = 0
				if text_bytes.size() >= 2:
					# Check for BOM (FE FF or FF FE)
					if (text_bytes[0] == 0xFE and text_bytes[1] == 0xFF) or (text_bytes[0] == 0xFF and text_bytes[1] == 0xFE):
						start_byte = 2
				for i in range(start_byte, text_bytes.size()):
					if text_bytes[i] != 0:
						clean_bytes.append(text_bytes[i])
				decoded_text = _safe_get_string(clean_bytes)
				
			decoded_text = decoded_text.strip_edges().replace("\u0000", "")
			
			# Map frames
			if frame_id == "TBP" or frame_id == "TBPM":
				tags["bpm"] = decoded_text.to_float()
			elif frame_id == "TKEY":
				tags["musical_key"] = decoded_text
			elif frame_id == "TCON":
				var parsed_genre = decoded_text
				if _is_unknown_genre(parsed_genre):
					tags["genre"] = "Unknown"
				else:
					tags["genre"] = parsed_genre
				
	return tags

## Adds a play timestamp to the track's listening history
func add_play_timestamp(href: String, timestamp: int) -> void:
	var existing := get_cached_metadata(href)
	if existing.is_empty():
		var info := parse_track_info(href)
		existing = await lookup_metadata(href, info.artist, info.album, info.track)
		
	var history: Array = existing.get("listening_history", [])
	history.append(timestamp)
	existing["listening_history"] = history
	cache[href] = existing
	save_cache()

## Main lookup function. Fetches missing metadata fields for a track and caches it.
func lookup_metadata(href: String, artist: String, album: String, title: String) -> Dictionary:
	if _pending_lookups.has(href):
		while _pending_lookups.has(href):
			await get_tree().process_frame
		return get_cached_metadata(href)
		
	_pending_lookups[href] = true
	var existing: Dictionary = get_cached_metadata(href)
	var updated: bool = false
	
	# If we have fully cached resolved details, return them directly
	if not existing.is_empty():
		var cached_artist: String = existing.get("artist_name", "")
		var cached_album: String = existing.get("album_name", "")
		if not cached_artist.is_empty() and cached_artist != "Unknown Artist" and not cached_album.is_empty() and cached_album != "Unknown Album":
			artist = cached_artist
			album = cached_album
			if existing.has("track_title"):
				title = existing["track_title"]
	
	# Try to resolve unknown artist/album using Deezer search API
	var search_resolved_genre := ""
	if artist == "Unknown Artist" or album == "Unknown Album":
		var resolved = await resolve_metadata_via_search(title)
		if not resolved.is_empty():
			artist = resolved.artist
			album = resolved.album
			title = resolved.title
			search_resolved_genre = resolved.get("genre", "")
			updated = true
	
	var artist_image: String = existing.get("artist_image_url", "")
	var artist_image_local: String = existing.get("artist_image_local", "")
	var album_art: String = existing.get("album_art_url", "")
	var album_art_local: String = existing.get("album_art_local", "")
	var lyrics: Dictionary = existing.get("lyrics", {})
	
	# Retrieve or default extended schema fields
	var bpm: float = existing.get("bpm", 0.0)
	var musical_key: String = existing.get("musical_key", "")
	var genre: String = existing.get("genre", "Unknown")
	if _is_unknown_genre(genre):
		genre = "Unknown"
	if genre == "Unknown" and not _is_unknown_genre(search_resolved_genre):
		genre = search_resolved_genre
		updated = true
	var energy_level: float = existing.get("energy_level", 0.0)
	var energy_graph: Array = existing.get("energy_graph", [])
	var dynamic_range: float = existing.get("dynamic_range", 0.0)
	var harmony_map: Dictionary = existing.get("harmony_map", {})
	var listening_history: Array = existing.get("listening_history", [])
	
	# Try to parse local ID3v2 tags if the file is cached locally
	if bpm == 0.0 or musical_key.is_empty() or genre == "Unknown":
		var ext := href.get_extension()
		if ext.is_empty():
			ext = "mp3"
		var local_cache_path := "user://audio_cache/" + href.md5_text() + "." + ext
		if FileAccess.file_exists(local_cache_path):
			var id3_tags := _parse_id3v2_tags(local_cache_path)
			if bpm == 0.0 and id3_tags["bpm"] > 0.0:
				bpm = id3_tags["bpm"]
				updated = true
			if musical_key.is_empty() and not id3_tags["musical_key"].is_empty():
				musical_key = id3_tags["musical_key"]
				updated = true
			var parsed_id3_genre = id3_tags["genre"]
			if genre == "Unknown" and not _is_unknown_genre(parsed_id3_genre):
				genre = parsed_id3_genre
				updated = true
				
	# Fetch genre from search API if still Unknown
	if genre == "Unknown" and artist != "Unknown Artist" and album != "Unknown Album":
		print("MetadataService: Genre is still Unknown for track='%s' (artist='%s', album='%s'). Attempting search..." % [title, artist, album])
		var resolved_genre = await fetch_genre_by_search(artist, album)
		if not _is_unknown_genre(resolved_genre):
			genre = resolved_genre
			updated = true
		else:
			print("MetadataService: Failed to resolve genre via search for album='%s'" % album)
	
	if artist_image.is_empty() or artist == "Unknown Artist":
		var new_artist_image = await fetch_artist_image(artist)
		if new_artist_image != artist_image:
			artist_image = new_artist_image
			updated = true
	if not artist_image.is_empty() and artist_image_local.is_empty():
		artist_image_local = await _download_image(artist_image)
		updated = true
		
	if album_art.is_empty() or album == "Unknown Album":
		var new_album_art = await fetch_album_art(artist, album)
		if new_album_art != album_art:
			album_art = new_album_art
			updated = true
	if not album_art.is_empty() and album_art_local.is_empty():
		album_art_local = await _download_image(album_art)
		updated = true
		
	if lyrics.is_empty():
		lyrics = await fetch_lyrics(artist, title)
		if not lyrics.is_empty():
			lyrics_fetched.emit(href, lyrics)
		updated = true
		
	var result: Dictionary = {
		"artist_name": artist,
		"album_name": album,
		"track_title": title,
		"artist_image_url": artist_image,
		"artist_image_local": artist_image_local,
		"album_art_url": album_art,
		"album_art_local": album_art_local,
		"lyrics": lyrics,
		"bpm": bpm,
		"musical_key": musical_key,
		"genre": genre,
		"energy_level": energy_level,
		"energy_graph": energy_graph,
		"dynamic_range": dynamic_range,
		"harmony_map": harmony_map,
		"listening_history": listening_history
	}
	
	if updated:
		var current = cache.get(href, {})
		for key in result.keys():
			var new_val = result[key]
			var cur_val = current.get(key)
			
			# Merge strategy: Only update if the new value is non-empty/non-default, or if the current value is missing
			if new_val != null and not (new_val is String and new_val.is_empty()) and not (new_val is Dictionary and new_val.is_empty()) and not (new_val is Array and new_val.is_empty()) and not (new_val is float and new_val == 0.0):
				current[key] = new_val
			elif cur_val == null:
				current[key] = new_val
				
		cache[href] = current
		save_cache()
		metadata_updated.emit(href, current)
		
	_pending_lookups.erase(href)
	return cache.get(href, result)

func focus_artist(artist: String) -> void:
	var img_path: String = ""
	for href in cache:
		var item = cache[href]
		if item is Dictionary and item.get("artist_name", "") == artist:
			if item.has("artist_image_local") and not item["artist_image_local"].is_empty():
				img_path = item["artist_image_local"]
				break
	
	if img_path.is_empty():
		var remote_url: String = await fetch_artist_image(artist)
		if not remote_url.is_empty():
			img_path = await _download_image(remote_url)
			
	artist_focused.emit(artist, img_path)

func focus_album(artist: String, album: String) -> void:
	var img_path: String = ""
	for href in cache:
		var item = cache[href]
		if item is Dictionary and item.get("album_name", "") == album:
			if item.has("album_art_local") and not item["album_art_local"].is_empty():
				img_path = item["album_art_local"]
				break
				
	if img_path.is_empty():
		var remote_url: String = await fetch_album_art(artist, album)
		if not remote_url.is_empty():
			img_path = await _download_image(remote_url)
			
	album_focused.emit(artist, album, img_path)

func focus_track(href: String, artist: String, album: String, title: String) -> void:
	var meta: Dictionary = await lookup_metadata(href, artist, album, title)
	var img_path: String = meta.get("album_art_local", "")
	if img_path.is_empty():
		img_path = meta.get("artist_image_local", "")
	var lyrics: Dictionary = meta.get("lyrics", {})
	track_focused.emit(artist, album, title, lyrics, img_path)

func focus_track_by_href(href: String) -> void:
	var info := parse_track_info(href)
	await focus_track(href, info.artist, info.album, info.track)

## Helper to identify numeric/alphanumeric track number prefixes (e.g., "01", "1", "A1", "1-01")
func _is_track_number_prefix(s: String) -> bool:
	var clean := s.strip_edges()
	if clean.is_valid_int():
		return true
	var regex := RegEx.new()
	regex.compile("^[A-Za-z]?\\d+[-a-zA-Z]?$")
	var match_obj := regex.search(clean)
	return match_obj != null

## Smart metadata parser that handles structured filenames and path/directory fallbacks
func parse_track_info(href: String) -> Dictionary:
	var cached := get_cached_metadata(href)
	if not cached.is_empty():
		var cached_artist: String = cached.get("artist_name", "")
		var cached_album: String = cached.get("album_name", "")
		var cached_track: String = cached.get("track_title", "")
		if not cached_artist.is_empty() and cached_artist != "Unknown Artist" \
				and not cached_album.is_empty() and cached_album != "Unknown Album":
			return {
				"artist": cached_artist,
				"album": cached_album,
				"track": cached_track if not cached_track.is_empty() else href.get_file().uri_decode().get_basename()
			}

	var raw_filename := href.get_file().uri_decode()
	var display_name := raw_filename.get_basename()
	
	var file_artist := ""
	var file_album := ""
	var file_track := display_name
	
	# Parse path segments for directory structure fallback
	var decoded_path := href.uri_decode()
	var path_segments := []
	for segment in decoded_path.split("/"):
		if not segment.is_empty():
			path_segments.append(segment)
			
	var base_folder := "Music"
	if SettingsManager.has_credentials() and "webdav_folder" in SettingsManager:
		base_folder = SettingsManager.webdav_folder
		
	var relative_start := -1
	for i in range(path_segments.size()):
		if path_segments[i].to_lower() == base_folder.to_lower():
			relative_start = i + 1
			break
			
	var relative_segments := []
	if relative_start != -1 and relative_start < path_segments.size():
		relative_segments = path_segments.slice(relative_start, path_segments.size() - 1)
	else:
		if path_segments.size() >= 2:
			relative_segments = path_segments.slice(0, path_segments.size() - 1)
			
	var folder_artist := ""
	var folder_album := ""
	
	if relative_segments.size() >= 2:
		folder_artist = relative_segments[0].strip_edges()
		folder_album = relative_segments[1].strip_edges()
	elif relative_segments.size() == 1:
		var seg: String = relative_segments[0].strip_edges()
		var clean_seg := seg.replace("–", "-").replace("—", "-")
		if " - " in clean_seg:
			var parts: PackedStringArray = clean_seg.split(" - ")
			if parts.size() == 2:
				folder_artist = parts[0].strip_edges()
				folder_album = parts[1].strip_edges()
			else:
				folder_album = seg
		else:
			folder_album = seg
		
	# Parse structured filename "Artist - Album - Track" or "Artist - Track"
	var clean_display := display_name.replace("–", "-").replace("—", "-")
	if " - " in clean_display:
		var raw_parts := clean_display.split(" - ")
		if raw_parts.size() > 1 and _is_track_number_prefix(raw_parts[0]):
			raw_parts.remove_at(0)
			
		if raw_parts.size() >= 3:
			file_artist = raw_parts[0].strip_edges()
			file_album = raw_parts[1].strip_edges()
			var track_parts := []
			for i in range(2, raw_parts.size()):
				track_parts.append(raw_parts[i])
			file_track = " - ".join(track_parts).strip_edges()
		elif raw_parts.size() == 2:
			file_artist = raw_parts[0].strip_edges()
			file_track = raw_parts[1].strip_edges()
		elif raw_parts.size() == 1:
			file_track = raw_parts[0].strip_edges()

	var artist := "Unknown Artist"
	if not file_artist.is_empty():
		artist = file_artist
	elif not folder_artist.is_empty():
		artist = folder_artist
		
	var album := "Unknown Album"
	if not file_album.is_empty():
		album = file_album
	elif not folder_album.is_empty():
		album = folder_album
		
	return {
		"artist": artist,
		"album": album,
		"track": file_track.strip_edges()
	}

func _safe_get_string(bytes: PackedByteArray) -> String:
	var clean := PackedByteArray()
	for b in bytes:
		if b != 0:
			clean.append(b)
	return clean.get_string_from_utf8()

func _is_unknown_genre(genre_str: String) -> bool:
	var g = genre_str.strip_edges().to_lower()
	return g.is_empty() or g == "unknown" or g == "unknown genre" or g == "--" or g == "unknown_genre"

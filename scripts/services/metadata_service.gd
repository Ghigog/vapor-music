extends Node
## MetadataService
##
## Fetches and caches artist image, album art, and lyrics from public APIs (Deezer and LRCLIB).
## Cache is saved to user://metadata_cache.json and entries are pruned when files are removed.

signal artist_focused(artist: String, image_path: String)
signal album_focused(artist: String, album: String, image_path: String)
signal track_focused(artist: String, album: String, title: String, lyrics: Dictionary, image_path: String)

const CACHE_FILE_PATH = "user://metadata_cache.json"

var cache: Dictionary = {}

func _ready() -> void:
	load_cache()
	if WebDAVService.has_signal("library_scanned"):
		WebDAVService.library_scanned.connect(prune_cache)

## Loads the metadata cache from user://metadata_cache.json
func load_cache() -> void:
	if FileAccess.file_exists(CACHE_FILE_PATH):
		var file := FileAccess.open(CACHE_FILE_PATH, FileAccess.READ)
		if file:
			var content := file.get_as_text()
			file.close()
			var parsed = JSON.parse_string(content)
			if parsed is Dictionary:
				cache = parsed
				return
	cache = {}

## Saves the current metadata cache to user://metadata_cache.json
func save_cache() -> void:
	var file := FileAccess.open(CACHE_FILE_PATH, FileAccess.WRITE)
	if file:
		file.store_string(JSON.stringify(cache, "\t"))
		file.close()

## Prunes any cache entries for track hrefs that are no longer present in the library scan.
func prune_cache(active_files: Array) -> void:
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
		save_cache()
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
		http.queue_free()
		return ""
		
	var response = await http.request_completed
	var response_code: int = response[1]
	var response_body: PackedByteArray = response[3]
	http.queue_free()
	
	if response_code == 200:
		return response_body.get_string_from_utf8()
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
		
	var query: String = "album:\"%s\"" % album
	if not artist.is_empty() and artist != "Unknown Artist":
		query = "artist:\"%s\" album:\"%s\"" % [artist, album]
		
	var url: String = "https://api.deezer.com/search/album?q=%s" % query.uri_encode()
	var response_str: String = await _make_http_request(url)
	if response_str.is_empty():
		# Fall back to simpler query if specific fails
		url = "https://api.deezer.com/search/album?q=%s" % album.uri_encode()
		response_str = await _make_http_request(url)
		if response_str.is_empty():
			return ""
			
	var data = JSON.parse_string(response_str)
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
			
			if track_data.has("artist") and track_data["artist"] is Dictionary:
				artist_name = track_data["artist"].get("name", "")
			if track_data.has("album") and track_data["album"] is Dictionary:
				album_title = track_data["album"].get("title", "")
			if track_data.has("title"):
				real_title = track_data.get("title", "")
				
			if not artist_name.is_empty() and not album_title.is_empty():
				return {
					"artist": artist_name,
					"album": album_title,
					"title": real_title
				}
	return {}

## Main lookup function. Fetches missing metadata fields for a track and caches it.
func lookup_metadata(href: String, artist: String, album: String, title: String) -> Dictionary:
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
	if artist == "Unknown Artist" or album == "Unknown Album":
		var resolved = await resolve_metadata_via_search(title)
		if not resolved.is_empty():
			artist = resolved.artist
			album = resolved.album
			title = resolved.title
			updated = true
	
	var artist_image: String = existing.get("artist_image_url", "")
	var artist_image_local: String = existing.get("artist_image_local", "")
	var album_art: String = existing.get("album_art_url", "")
	var album_art_local: String = existing.get("album_art_local", "")
	var lyrics: Dictionary = existing.get("lyrics", {})
	
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
		updated = true
		
	var result: Dictionary = {
		"artist_name": artist,
		"album_name": album,
		"track_title": title,
		"artist_image_url": artist_image,
		"artist_image_local": artist_image_local,
		"album_art_url": album_art,
		"album_art_local": album_art_local,
		"lyrics": lyrics
	}
	
	if updated:
		cache[href] = result
		save_cache()
		
	return result

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


extends GutTest

const TEST_CACHE_PATH = "user://test_metadata_cache.json"

# We subclass MetadataService to mock the network calls
class MockMetadataService extends "res://scripts/services/metadata_service.gd":
	var mock_response := ""
	
	func _ready() -> void:
		# Override cache path to not pollute the real cache during testing
		pass

	func _make_http_request(_url: String) -> String:
		return mock_response

var service: MockMetadataService

func before_each() -> void:
	service = MockMetadataService.new()
	service.cache = {}
	# Set a separate cache file for unit tests
	service.cache_file_path = TEST_CACHE_PATH
	add_child_autofree(service)

func after_each() -> void:
	# Clean up test file if it exists
	if FileAccess.file_exists(TEST_CACHE_PATH):
		DirAccess.remove_absolute(TEST_CACHE_PATH)

func test_load_save_cache() -> void:
	service.cache = {
		"test_href": {
			"artist_image_url": "http://example.com/artist.jpg",
			"album_art_url": "http://example.com/album.jpg",
			"lyrics": "Test Lyrics"
		}
	}
	service.save_cache(true)
	
	# Clear in-memory cache and reload
	service.cache = {}
	service.load_cache()
	
	assert_true(service.cache.has("test_href"), "Cache should contain test_href after reload")
	assert_eq(service.cache["test_href"]["lyrics"], "Test Lyrics", "Lyrics should match")

func test_prune_cache() -> void:
	service.cache = {
		"file1.mp3": {"lyrics": "Song 1"},
		"file2.mp3": {"lyrics": "Song 2"},
		"file3.mp3": {"lyrics": "Song 3"}
	}
	
	service.prune_cache(["file1.mp3", "file3.mp3"])
	
	assert_true(service.cache.has("file1.mp3"), "Should keep file1.mp3")
	assert_false(service.cache.has("file2.mp3"), "Should prune file2.mp3")
	assert_true(service.cache.has("file3.mp3"), "Should keep file3.mp3")

func test_fetch_artist_image() -> void:
	# Mock Deezer artist search response
	service.mock_response = JSON.stringify({
		"data": [
			{
				"name": "Daft Punk",
				"picture_xl": "https://e-cdns-images.dzcdn.net/images/artist/xl.jpg",
				"picture_big": "https://e-cdns-images.dzcdn.net/images/artist/big.jpg"
			}
		]
	})
	
	var img = await service.fetch_artist_image("Daft Punk")
	assert_eq(img, "https://e-cdns-images.dzcdn.net/images/artist/xl.jpg", "Should parse picture_xl first")

func test_fetch_album_art() -> void:
	# Mock Deezer album search response
	service.mock_response = JSON.stringify({
		"data": [
			{
				"title": "Discovery",
				"cover_xl": "https://e-cdns-images.dzcdn.net/images/cover/xl.jpg"
			}
		]
	})
	
	var art = await service.fetch_album_art("Daft Punk", "Discovery")
	assert_eq(art, "https://e-cdns-images.dzcdn.net/images/cover/xl.jpg", "Should parse cover_xl first")

func test_fetch_lyrics() -> void:
	# Mock LRCLIB response
	service.mock_response = JSON.stringify({
		"syncedLyrics": "[00:00.00] Work it\n[00:02.00] Make it",
		"plainLyrics": "Work it\nMake it"
	})
	
	var lyrics = await service.fetch_lyrics("Daft Punk", "Harder Better Faster Stronger")
	assert_true(lyrics.get("synced", false), "Should parse as synced")
	assert_eq(lyrics["lines"].size(), 2, "Should parse 2 lines")
	assert_eq(lyrics["lines"][0]["text"], "Work it", "Line text should match")

func test_lookup_metadata_uses_cache() -> void:
	service.cache = {
		"file1.mp3": {
			"artist_image_url": "cached_artist",
			"album_art_url": "cached_album",
			"lyrics": {"synced": false, "plain": "cached_lyrics"},
			"bpm": 120.0,
			"musical_key": "4A",
			"genre": "Electronic"
		}
	}
	
	service.mock_response = "SHOULD NOT BE CALLED"
	
	var result = await service.lookup_metadata("file1.mp3", "Daft Punk", "Discovery", "One More Time")
	assert_eq(result["artist_image_url"], "cached_artist", "Should return cached artist image")
	assert_eq(result["album_art_url"], "cached_album", "Should return cached album art")
	assert_eq(result["lyrics"]["plain"], "cached_lyrics", "Should return cached lyrics")

func test_focus_artist_emits_signal() -> void:
	watch_signals(service)
	service.cache = {
		"file1.mp3": {
			"artist_name": "Daft Punk",
			"artist_image_local": "user://metadata_images/cached_artist.jpg"
		}
	}
	
	await service.focus_artist("Daft Punk")
	assert_signal_emitted_with_parameters(service, "artist_focused", ["Daft Punk", "user://metadata_images/cached_artist.jpg"], 0)

func test_focus_album_emits_signal() -> void:
	watch_signals(service)
	service.cache = {
		"file1.mp3": {
			"album_name": "Discovery",
			"album_art_local": "user://metadata_images/cached_album.jpg"
		}
	}
	
	await service.focus_album("Daft Punk", "Discovery")
	assert_signal_emitted_with_parameters(service, "album_focused", ["Daft Punk", "Discovery", "user://metadata_images/cached_album.jpg"], 0)

func test_focus_track_emits_signal() -> void:
	watch_signals(service)
	service.cache = {
		"file1.mp3": {
			"artist_name": "Daft Punk",
			"album_name": "Discovery",
			"track_title": "One More Time",
			"album_art_local": "user://metadata_images/cached_album.jpg",
			"lyrics": {"synced": false, "plain": "One more time..."}
		}
	}
	
	await service.focus_track("file1.mp3", "Daft Punk", "Discovery", "One More Time")
	assert_signal_emitted_with_parameters(service, "track_focused", ["Daft Punk", "Discovery", "One More Time", {"synced": false, "plain": "One more time..."}, "user://metadata_images/cached_album.jpg"], 0)

func test_resolve_metadata_via_search() -> void:
	service.mock_response = JSON.stringify({
		"data": [
			{
				"title": "Feel Good Inc.",
				"artist": {"name": "Gorillaz"},
				"album": {"title": "Demon Days"}
			}
		]
	})
	
	var resolved = await service.resolve_metadata_via_search("Feel Good Inc")
	assert_eq(resolved.artist, "Gorillaz", "Artist should be resolved")
	assert_eq(resolved.album, "Demon Days", "Album should be resolved")
	assert_eq(resolved.title, "Feel Good Inc.", "Track title should be resolved")

func test_lookup_metadata_resolves_unknown() -> void:
	service.mock_response = JSON.stringify({
		"data": [
			{
				"title": "Feel Good Inc.",
				"artist": {"name": "Gorillaz"},
				"album": {"title": "Demon Days"}
			}
		]
	})
	
	var result = await service.lookup_metadata("file_feel_good.mp3", "Unknown Artist", "Unknown Album", "Feel Good Inc")
	assert_eq(result["artist_name"], "Gorillaz", "Should resolve and return Gorillaz")
	assert_eq(result["album_name"], "Demon Days", "Should resolve and return Demon Days")
	assert_eq(result["track_title"], "Feel Good Inc.", "Should resolve and return track title")

func test_extended_schema_defaults() -> void:
	service.mock_response = "{}"
	var result = await service.lookup_metadata("nonexistent.mp3", "Artist", "Album", "Track")
	assert_eq(result["bpm"], 0.0, "BPM should default to 0.0")
	assert_eq(result["musical_key"], "", "Key should default to empty string")
	assert_eq(result["genre"], "Unknown", "Genre should default to Unknown")
	assert_eq(result["energy_level"], 0.0, "Energy should default to 0.0")
	assert_eq(result["energy_graph"], [], "Energy graph should default to empty array")
	assert_eq(result["dynamic_range"], 0.0, "Dynamic range should default to 0.0")
	assert_eq(result["harmony_map"], {}, "Harmony map should default to empty dictionary")
	assert_eq(result["listening_history"], [], "Listening history should default to empty array")

func test_listening_history() -> void:
	service.cache = {
		"file1.mp3": {
			"artist_name": "Artist",
			"album_name": "Album",
			"track_title": "Track",
			"listening_history": []
		}
	}
	await service.add_play_timestamp("file1.mp3", 12345678)
	var meta = service.get_cached_metadata("file1.mp3")
	assert_eq(meta["listening_history"], [12345678], "History should contain the play timestamp")

func test_id3v2_tag_parsing() -> void:
	var temp_path = "user://test_id3_parse.mp3"
	var file = FileAccess.open(temp_path, FileAccess.WRITE)
	if not file:
		fail_test("Could not open temp file for writing ID3 test tags")
		return
		
	# ID3v2.3 Header (10 bytes)
	var bytes = PackedByteArray([
		0x49, 0x44, 0x33, # "ID3"
		0x03, 0x00,       # v2.3.0
		0x00,             # Flags
		0x00, 0x00, 0x00, 0x2A # Size: 42 bytes (synchsafe: 42)
	])
	
	# TBPM frame (13 bytes total: ID=4, Size=4, Flags=2, Encoding=1, Value=5)
	# "TBPM", Size=6, Flags=0, Encoding=0, Value="120"
	bytes.append_array(PackedByteArray([
		0x54, 0x42, 0x50, 0x4D, # "TBPM"
		0x00, 0x00, 0x00, 0x04, # Size = 4 (encoding + "120")
		0x00, 0x00,             # Flags
		0x00,                   # Encoding: ISO-8859-1
		0x31, 0x32, 0x30        # "120"
	]))
	
	# TKEY frame
	# "TKEY", Size=3, Flags=0, Encoding=0, Value="8A"
	bytes.append_array(PackedByteArray([
		0x54, 0x4B, 0x45, 0x59, # "TKEY"
		0x00, 0x00, 0x00, 0x03, # Size = 3 (encoding + "8A")
		0x00, 0x00,             # Flags
		0x00,                   # Encoding: ISO-8859-1
		0x38, 0x41              # "8A"
	]))
	
	# TCON frame
	# "TCON", Size=5, Flags=0, Encoding=0, Value="Rock"
	bytes.append_array(PackedByteArray([
		0x54, 0x43, 0x4F, 0x4E, # "TCON"
		0x00, 0x00, 0x00, 0x05, # Size = 5 (encoding + "Rock")
		0x00, 0x00,             # Flags
		0x00,                   # Encoding: ISO-8859-1
		0x52, 0x6F, 0x63, 0x6B  # "Rock"
	]))
	
	file.store_buffer(bytes)
	file.close()
	
	var tags = service._parse_id3v2_tags(temp_path)
	DirAccess.remove_absolute(temp_path)
	
	assert_eq(tags["bpm"], 120.0, "BPM should be parsed as 120")
	assert_eq(tags["musical_key"], "8A", "Key should be parsed as 8A")
	assert_eq(tags["genre"], "Rock", "Genre should be parsed as Rock")

func test_get_entity_image_path_prefers_custom_override() -> void:
	service.cache = {
		"file1.mp3": {
			"artist_name": "Daft Punk",
			"artist_image_local": "user://metadata_images/cached_artist.jpg"
		}
	}
	# Populate in-memory only — CUSTOM_IMAGES_FILE is a shared user:// path,
	# not test-isolated, so tests must never round-trip it through disk.
	service._custom_images_loaded = true
	service.custom_images = {"artist|Daft Punk": "user://custom_images/override.jpg"}

	assert_eq(service.get_entity_image_path("artist", "Daft Punk"), "user://custom_images/override.jpg")

func test_get_entity_image_path_falls_back_to_cache_scan() -> void:
	service.cache = {
		"file1.mp3": {
			"album_name": "Discovery",
			"album_art_local": "user://metadata_images/cached_album.jpg"
		}
	}
	service._custom_images_loaded = true
	service.custom_images = {}

	assert_eq(service.get_entity_image_path("album", "Discovery"), "user://metadata_images/cached_album.jpg")

func test_get_entity_image_path_empty_when_unresolved() -> void:
	service.cache = {}
	service._custom_images_loaded = true
	service.custom_images = {}

	assert_eq(service.get_entity_image_path("artist", "Nobody"), "")

func test_needs_enrichment_true_for_unknown_href() -> void:
	service.cache = {}
	assert_true(service._needs_enrichment("never_seen.mp3"))

func test_needs_enrichment_true_when_genre_unknown() -> void:
	service.cache = {
		"file1.mp3": {
			"genre": "Unknown",
			"artist_name": "",
			"album_name": ""
		}
	}
	assert_true(service._needs_enrichment("file1.mp3"))

func test_needs_enrichment_true_when_artist_known_but_image_missing() -> void:
	service.cache = {
		"file1.mp3": {
			"genre": "Electronic",
			"artist_name": "Daft Punk",
			"artist_image_local": "",
			"album_name": "Unknown Album"
		}
	}
	assert_true(service._needs_enrichment("file1.mp3"))

func test_needs_enrichment_false_when_fully_enriched() -> void:
	service.cache = {
		"file1.mp3": {
			"genre": "Electronic",
			"artist_name": "Daft Punk",
			"artist_image_local": "user://metadata_images/a.jpg",
			"album_name": "Discovery",
			"album_art_local": "user://metadata_images/b.jpg"
		}
	}
	assert_false(service._needs_enrichment("file1.mp3"))

func test_start_background_enrichment_noop_when_nothing_needed() -> void:
	service.cache = {
		"file1.mp3": {
			"genre": "Electronic",
			"artist_name": "Daft Punk",
			"artist_image_local": "user://metadata_images/a.jpg",
			"album_name": "Discovery",
			"album_art_local": "user://metadata_images/b.jpg"
		}
	}
	watch_signals(service)
	await service.start_background_enrichment(["file1.mp3"])

	assert_signal_not_emitted(service, "enrichment_progress", "Nothing needed enrichment, so the sweep should never start")
	assert_signal_not_emitted(service, "enrichment_completed")

func test_set_manual_metadata_writes_fields() -> void:
	service.cache = {
		"file1.mp3": {"artist_name": "Wrong Artist", "album_name": "Wrong Album"}
	}
	watch_signals(service)
	# Artist/album deliberately left unchanged here — this test is only about
	# the plain field write, not the identity-change side effects covered below.
	service.set_manual_metadata("file1.mp3", {
		"artist_name": "Wrong Artist",
		"album_name": "Wrong Album",
		"genre": "Techno",
		"bpm": 128.0,
		"musical_key": "8A",
	})

	var entry: Dictionary = service.cache["file1.mp3"]
	assert_eq(entry.genre, "Techno")
	assert_eq(entry.bpm, 128.0)
	assert_eq(entry.musical_key, "8A")
	assert_signal_emitted(service, "metadata_updated")

func test_set_manual_metadata_clears_stale_art_on_artist_change() -> void:
	service.cache = {
		"file1.mp3": {
			"artist_name": "Wrong Artist",
			"album_name": "Right Album",
			"artist_image_local": "user://metadata_images/wrong_artist.jpg",
			"album_art_local": "user://metadata_images/album.jpg",
		}
	}
	service.set_manual_metadata("file1.mp3", {
		"artist_name": "Right Artist",
		"album_name": "Right Album",
		"genre": "House",
		"bpm": 0.0,
		"musical_key": "",
	})

	var entry: Dictionary = service.cache["file1.mp3"]
	assert_eq(entry.artist_name, "Right Artist")
	assert_eq(entry.artist_image_local, "", "Artist image resolved for the wrong artist must be cleared")
	assert_eq(entry.album_art_local, "", "Album art is also cleared since the whole identity changed")

func test_set_manual_metadata_resets_genre_on_album_change_when_not_retyped() -> void:
	service.cache = {
		"file1.mp3": {
			"artist_name": "Some Artist",
			"album_name": "Wrong Album",
			"genre": "Jazz",
		}
	}
	service.set_manual_metadata("file1.mp3", {
		"artist_name": "Some Artist",
		"album_name": "Right Album",
		"genre": "Jazz", # left exactly as prefilled, i.e. not actually retyped
		"bpm": 0.0,
		"musical_key": "",
	})

	assert_eq(service.cache["file1.mp3"].genre, "Unknown",
		"Genre resolved against the old album shouldn't survive an album fix the user didn't also retype the genre for")

func test_set_manual_metadata_keeps_explicit_genre_on_album_change() -> void:
	service.cache = {
		"file1.mp3": {
			"artist_name": "Some Artist",
			"album_name": "Wrong Album",
			"genre": "Jazz",
		}
	}
	service.set_manual_metadata("file1.mp3", {
		"artist_name": "Some Artist",
		"album_name": "Right Album",
		"genre": "Techno", # explicitly retyped alongside the album fix
		"bpm": 0.0,
		"musical_key": "",
	})

	assert_eq(service.cache["file1.mp3"].genre, "Techno")

func test_set_manual_metadata_clears_lyrics_on_title_change() -> void:
	service.cache = {
		"file1.mp3": {
			"artist_name": "Some Artist",
			"album_name": "Some Album",
			"track_title": "Wrong Title",
			"lyrics": {"synced": false, "plain": "la la la"},
		}
	}
	service.set_manual_metadata("file1.mp3", {
		"artist_name": "Some Artist",
		"album_name": "Some Album",
		"track_title": "Right Title",
		"genre": "Unknown",
		"bpm": 0.0,
		"musical_key": "",
	})

	assert_eq(service.cache["file1.mp3"].lyrics, {}, "Lyrics resolved for the wrong title must be cleared")

func test_set_manual_metadata_leaves_art_untouched_for_bpm_only_edit() -> void:
	service.cache = {
		"file1.mp3": {
			"artist_name": "Some Artist",
			"album_name": "Some Album",
			"artist_image_local": "user://metadata_images/a.jpg",
			"album_art_local": "user://metadata_images/b.jpg",
			"genre": "House",
			"bpm": 0.0,
		}
	}
	service.set_manual_metadata("file1.mp3", {
		"artist_name": "Some Artist",
		"album_name": "Some Album",
		"genre": "House",
		"bpm": 140.0,
		"musical_key": "",
	})

	var entry: Dictionary = service.cache["file1.mp3"]
	assert_eq(entry.bpm, 140.0)
	assert_eq(entry.artist_image_local, "user://metadata_images/a.jpg",
		"Neither artist nor album changed, so cached art must survive an unrelated BPM edit")
	assert_eq(entry.album_art_local, "user://metadata_images/b.jpg")



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
	service.set("CACHE_FILE_PATH", TEST_CACHE_PATH)
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
	service.save_cache()
	
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
			"lyrics": {"synced": false, "plain": "cached_lyrics"}
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


## test_playlist_service.gd
## GUT unit tests for PlaylistService CRUD operations, track additions/removals, reordering, and cover art.
extends GutTest

const TEST_PLAYLISTS_PATH = "user://test_playlists.json"
const TEST_IMAGE_PATH = "user://test_input_image.png"

func before_all() -> void:
	PlaylistService.playlist_file_path = TEST_PLAYLISTS_PATH
	
	# Create a dummy image file for custom cover tests
	var file = FileAccess.open(TEST_IMAGE_PATH, FileAccess.WRITE)
	if file:
		file.store_string("dummy image data")
		file.close()

func before_each() -> void:
	PlaylistService.playlists = {}
	PlaylistService.save_playlists()

func after_each() -> void:
	if FileAccess.file_exists(TEST_PLAYLISTS_PATH):
		DirAccess.remove_absolute(TEST_PLAYLISTS_PATH)

func after_all() -> void:
	PlaylistService.playlist_file_path = "user://playlists.json"
	PlaylistService.load_playlists()
	if FileAccess.file_exists(TEST_IMAGE_PATH):
		DirAccess.remove_absolute(TEST_IMAGE_PATH)
		
	# Clean up test custom covers
	var dir = DirAccess.open("user://playlist_images/")
	if dir:
		dir.list_dir_begin()
		var file_name = dir.get_next()
		while file_name != "":
			if not dir.current_is_dir():
				dir.remove(file_name)
			file_name = dir.get_next()
		dir.list_dir_end()

func test_create_playlist() -> void:
	var watcher = watch_signals(PlaylistService)
	var playlist = PlaylistService.create_playlist("Vapor Lounge")
	
	assert_signal_emitted(PlaylistService, "playlist_created", "playlist_created signal should be emitted")
	assert_eq(playlist.name, "Vapor Lounge")
	assert_true(playlist.id != "")
	assert_eq(playlist.custom_cover_path, "")
	assert_eq(playlist.tracks.size(), 0)
	
	# Verify persistence
	PlaylistService.load_playlists()
	var loaded = PlaylistService.get_playlist(playlist.id)
	assert_eq(loaded.name, "Vapor Lounge")

func test_delete_playlist() -> void:
	var watcher = watch_signals(PlaylistService)
	var playlist = PlaylistService.create_playlist("Vibe Check")
	
	PlaylistService.delete_playlist(playlist.id)
	assert_signal_emitted(PlaylistService, "playlist_deleted")
	
	var deleted = PlaylistService.get_playlist(playlist.id)
	assert_true(deleted.is_empty(), "Playlist should be deleted and empty")

func test_rename_playlist() -> void:
	var watcher = watch_signals(PlaylistService)
	var playlist = PlaylistService.create_playlist("Chill Synth")
	
	PlaylistService.rename_playlist(playlist.id, "Lo-Fi Beats")
	assert_signal_emitted(PlaylistService, "playlist_renamed")
	
	var updated = PlaylistService.get_playlist(playlist.id)
	assert_eq(updated.name, "Lo-Fi Beats")

func test_add_and_remove_track() -> void:
	var watcher = watch_signals(PlaylistService)
	var playlist = PlaylistService.create_playlist("Late Night FM")
	
	PlaylistService.add_track_to_playlist(playlist.id, "res://music/track1.mp3")
	assert_signal_emitted(PlaylistService, "playlist_tracks_updated")
	
	var updated = PlaylistService.get_playlist(playlist.id)
	assert_eq(updated.tracks.size(), 1)
	assert_eq(updated.tracks[0], "res://music/track1.mp3")
	
	PlaylistService.remove_track_from_playlist(playlist.id, 0)
	updated = PlaylistService.get_playlist(playlist.id)
	assert_eq(updated.tracks.size(), 0)

func test_reorder_tracks() -> void:
	var playlist = PlaylistService.create_playlist("Reorder FM")
	PlaylistService.add_track_to_playlist(playlist.id, "track1")
	PlaylistService.add_track_to_playlist(playlist.id, "track2")
	PlaylistService.add_track_to_playlist(playlist.id, "track3")
	
	# Move track1 (index 0) to the end (index 2)
	PlaylistService.reorder_track_in_playlist(playlist.id, 0, 2)
	var updated = PlaylistService.get_playlist(playlist.id)
	assert_eq(updated.tracks[0], "track2")
	assert_eq(updated.tracks[1], "track3")
	assert_eq(updated.tracks[2], "track1")

func test_custom_cover_art() -> void:
	var watcher = watch_signals(PlaylistService)
	var playlist = PlaylistService.create_playlist("Visuals")
	
	PlaylistService.set_playlist_custom_cover(playlist.id, TEST_IMAGE_PATH)
	assert_signal_emitted(PlaylistService, "playlist_cover_updated")
	
	var updated = PlaylistService.get_playlist(playlist.id)
	var custom_path = updated.custom_cover_path
	assert_true(custom_path.begins_with("user://playlist_images/"), "Cover should be copied to user://playlist_images/")
	assert_true(FileAccess.file_exists(custom_path), "Copied cover file should exist on disk")
	
	# Clean up custom cover by setting empty path
	PlaylistService.set_playlist_custom_cover(playlist.id, "")
	updated = PlaylistService.get_playlist(playlist.id)
	assert_eq(updated.custom_cover_path, "")
	assert_false(FileAccess.file_exists(custom_path), "Custom cover file should be deleted from disk when cleared")

func test_first_track_album_art_fallback() -> void:
	var playlist = PlaylistService.create_playlist("Fallback Art")
	PlaylistService.add_track_to_playlist(playlist.id, "track_href_1")
	
	# Inject mock metadata for track_href_1
	if is_instance_valid(MetadataService):
		MetadataService.cache["track_href_1"] = {
			"album_art_local": "user://metadata_images/mock_art.jpg"
		}
	
	var cover_path = PlaylistService.get_playlist_cover_path(playlist.id)
	assert_eq(cover_path, "user://metadata_images/mock_art.jpg", "Should fall back to album_art_local of the first track")

func test_reorder_playlists() -> void:
	var watcher = watch_signals(PlaylistService)
	var p1 = PlaylistService.create_playlist("Playlist 1")
	var p2 = PlaylistService.create_playlist("Playlist 2")
	var p3 = PlaylistService.create_playlist("Playlist 3")
	
	# Initial order: [Playlist 3, Playlist 2, Playlist 1] (due to prepend on creation)
	var lists = PlaylistService.get_playlists()
	assert_eq(lists[0].name, "Playlist 3")
	assert_eq(lists[1].name, "Playlist 2")
	assert_eq(lists[2].name, "Playlist 1")
	
	# Reorder index 0 (Playlist 3) to index 2 (at the end)
	PlaylistService.reorder_playlists(0, 2)
	assert_signal_emitted(PlaylistService, "playlists_loaded")
	
	lists = PlaylistService.get_playlists()
	assert_eq(lists[0].name, "Playlist 2")
	assert_eq(lists[1].name, "Playlist 1")
	assert_eq(lists[2].name, "Playlist 3")

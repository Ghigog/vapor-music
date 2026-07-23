## test_playlist_folder_service.gd
## GUT unit tests for PlaylistFolderService CRUD, and for the folder-related
## additions to PlaylistService (set_playlist_folder, reorder_playlist_relative,
## and delete_folder un-filing rather than deleting playlists).
extends GutTest

const TEST_FOLDERS_PATH = "user://test_playlist_folders.json"
const TEST_PLAYLISTS_PATH = "user://test_playlists_for_folders.json"

func before_all() -> void:
	PlaylistFolderService.folders_file_path = TEST_FOLDERS_PATH
	PlaylistService.playlist_file_path = TEST_PLAYLISTS_PATH

func before_each() -> void:
	PlaylistFolderService.folders = {}
	PlaylistFolderService.save_folders()
	PlaylistService.playlists = {}
	PlaylistService.save_playlists()

func after_each() -> void:
	if FileAccess.file_exists(TEST_FOLDERS_PATH):
		DirAccess.remove_absolute(TEST_FOLDERS_PATH)
	if FileAccess.file_exists(TEST_PLAYLISTS_PATH):
		DirAccess.remove_absolute(TEST_PLAYLISTS_PATH)

func after_all() -> void:
	PlaylistFolderService.folders_file_path = "user://playlist_folders.json"
	PlaylistFolderService.load_folders()
	PlaylistService.playlist_file_path = "user://playlists.json"
	PlaylistService.load_playlists()

func test_create_folder() -> void:
	var watcher = watch_signals(PlaylistFolderService)
	var folder = PlaylistFolderService.create_folder("Deep House")

	assert_signal_emitted(PlaylistFolderService, "folder_created")
	assert_eq(folder.name, "Deep House")
	assert_true(folder.id != "")
	assert_eq(folder.parent_id, "")

	PlaylistFolderService.load_folders()
	var loaded = PlaylistFolderService.get_folder(folder.id)
	assert_eq(loaded.name, "Deep House")

func test_rename_folder() -> void:
	var watcher = watch_signals(PlaylistFolderService)
	var folder = PlaylistFolderService.create_folder("Working Sets")

	PlaylistFolderService.rename_folder(folder.id, "Live Sets")
	assert_signal_emitted(PlaylistFolderService, "folder_renamed")

	var updated = PlaylistFolderService.get_folder(folder.id)
	assert_eq(updated.name, "Live Sets")

func test_set_playlist_folder() -> void:
	var watcher = watch_signals(PlaylistService)
	var folder = PlaylistFolderService.create_folder("Gigs")
	var playlist = PlaylistService.create_playlist("Set 1")
	assert_eq(playlist.folder_id, "", "New playlists default to the root, no folder")

	PlaylistService.set_playlist_folder(playlist.id, folder.id)
	assert_signal_emitted(PlaylistService, "playlist_folder_changed")

	var updated = PlaylistService.get_playlist(playlist.id)
	assert_eq(updated.folder_id, folder.id)

func test_delete_folder_unfiles_playlists_without_deleting_them() -> void:
	var watcher = watch_signals(PlaylistFolderService)
	var folder = PlaylistFolderService.create_folder("Temp Folder")
	var playlist = PlaylistService.create_playlist("Keep Me", folder.id)
	assert_eq(playlist.folder_id, folder.id)

	PlaylistFolderService.delete_folder(folder.id)
	assert_signal_emitted(PlaylistFolderService, "folder_deleted")

	assert_true(PlaylistFolderService.get_folder(folder.id).is_empty(), "Folder itself should be gone")
	var surviving = PlaylistService.get_playlist(playlist.id)
	assert_false(surviving.is_empty(), "Playlist must survive its folder's deletion")
	assert_eq(surviving.folder_id, "", "Playlist should be un-filed back to the root")

func test_reorder_playlist_relative() -> void:
	var p1 = PlaylistService.create_playlist("A")
	var p2 = PlaylistService.create_playlist("B")
	var p3 = PlaylistService.create_playlist("C")

	# Creation order (each prepended): [C, B, A]
	var order = PlaylistService.get_playlists()
	assert_eq(order[0].name, "C")
	assert_eq(order[1].name, "B")
	assert_eq(order[2].name, "A")

	# Move A immediately before C: [A, C, B]
	PlaylistService.reorder_playlist_relative(p1.id, p3.id, true)
	order = PlaylistService.get_playlists()
	assert_eq(order[0].name, "A")
	assert_eq(order[1].name, "C")
	assert_eq(order[2].name, "B")

func test_reorder_playlist_relative_stays_correct_regardless_of_folder_membership() -> void:
	var folder = PlaylistFolderService.create_folder("Mixed")
	var p1 = PlaylistService.create_playlist("Root 1")
	var p2 = PlaylistService.create_playlist("Foldered", folder.id)
	var p3 = PlaylistService.create_playlist("Root 2")

	# Reordering p3 relative to p1 must not disturb p2's folder membership —
	# an id-based reorder only ever touches the two ids it's given.
	PlaylistService.reorder_playlist_relative(p3.id, p1.id, true)

	var updated_p2 = PlaylistService.get_playlist(p2.id)
	assert_eq(updated_p2.folder_id, folder.id, "Unrelated playlist's folder must be untouched")

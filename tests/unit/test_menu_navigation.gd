## test_menu_navigation.gd
## GUT unit tests to verify sidebar navigation buttons and playlist selection routing.
extends GutTest

var main_scene = preload("res://scenes/main.tscn")
var main_node: Control
var sidebar: Control
var playlists_container: VBoxContainer

func before_each() -> void:
	# Clean up any test configuration for PlaylistService
	PlaylistService.playlist_file_path = "user://test_menu_playlists.json"
	PlaylistService.playlists = {}
	PlaylistService.save_playlists()
	
	main_node = main_scene.instantiate()
	add_child_autofree(main_node)
	sidebar = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/Sidebar")
	if sidebar:
		playlists_container = sidebar.get_node_or_null("VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/PlaylistsContainer")

func after_each() -> void:
	if FileAccess.file_exists("user://test_menu_playlists.json"):
		DirAccess.remove_absolute("user://test_menu_playlists.json")
	PlaylistService.playlist_file_path = "user://playlists.json"
	PlaylistService.load_playlists()

func test_navigation_buttons_accessible_and_functional() -> void:
	assert_not_null(sidebar, "Sidebar node must exist in main scene layout")
	
	var nav_library = sidebar.get_node_or_null("VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/NavLibrary") as Button
	var nav_vibe = sidebar.get_node_or_null("VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/NavVibe") as Button
	var nav_settings = sidebar.get_node_or_null("VBox/Scroll/ScrollMargin/ScrollVBox/NavItems/NavSettings") as Button
	
	assert_not_null(nav_library, "NavLibrary button must exist in Sidebar")
	assert_not_null(nav_vibe, "NavVibe button must exist in Sidebar")
	assert_not_null(nav_settings, "NavSettings button must exist in Sidebar")
	
	# Test Library navigation click
	NavManager.current_screen = "settings"
	nav_library.emit_signal("pressed")
	assert_eq(NavManager.current_screen, "library", "Pressing Library button should navigate to library screen")
	
	# Test Vibe navigation click
	nav_vibe.emit_signal("pressed")
	assert_eq(NavManager.current_screen, "vibe", "Pressing Vibe button should navigate to vibe screen")
	
	# Test Settings navigation click
	nav_settings.emit_signal("pressed")
	assert_eq(NavManager.current_screen, "settings", "Pressing Settings button should navigate to settings screen")

func test_playlist_creation_and_navigation() -> void:
	assert_not_null(playlists_container, "PlaylistsContainer must exist in Sidebar")
	
	# Create a dummy playlist via PlaylistService
	var playlist = PlaylistService.create_playlist("Synthwave Mix")
	
	# Wait for deferred call of _rebuild_playlists_list in the sidebar
	await wait_frames(3)
	
	# Find the item button inside the container
	var playlist_btn: Button = null
	for child in playlists_container.get_children():
		if child is Button and child.get("playlist_id") == playlist.id:
			playlist_btn = child
			break
			
	assert_not_null(playlist_btn, "A button should be instantiated for the newly created playlist in the sidebar")
	
	# Navigate away to library first
	NavManager.navigate_to("library")
	assert_eq(NavManager.current_screen, "library")
	
	# Simulate pressing the playlist button
	playlist_btn.emit_signal("pressed")
	
	# Assert navigation transitioned to the playlist screen
	assert_eq(NavManager.current_screen, "playlist", "Pressing a playlist item button should navigate to the playlist view")
	assert_eq(PlaylistService.active_playlist_id, playlist.id, "PlaylistService active_playlist_id should be set to the clicked playlist ID")

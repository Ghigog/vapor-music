## test_window_playback_ui.gd
## GUT unit tests for new Window Resizing, Dragging and vertical progress layout.
extends GutTest

var main_scene = preload("res://scenes/main.tscn")
var main_node: Control

func before_each() -> void:
	main_node = main_scene.instantiate()
	add_child_autofree(main_node)

func test_vertical_progress_node_exists() -> void:
	var vp = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/VerticalProgress")
	assert_not_null(vp, "VerticalProgress node must exist under LayoutRoot")
	assert_true(vp.has_signal("drag_started"), "VerticalProgress must have drag_started signal")
	assert_true(vp.has_signal("drag_ended"), "VerticalProgress must have drag_ended signal")
	assert_true(vp.has_signal("value_changed"), "VerticalProgress must have value_changed signal")

func test_sidebar_has_playback_control_buttons() -> void:
	var sidebar_node = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/Sidebar")
	assert_not_null(sidebar_node, "Sidebar must exist under LayoutRoot")
	
	var play_pause = sidebar_node.get_node_or_null("VBox/PlayerTiles/PlayPauseBtn")
	var backward = sidebar_node.get_node_or_null("VBox/PlayerTiles/ControlsHBox/BackwardBtn")
	var forward = sidebar_node.get_node_or_null("VBox/PlayerTiles/ControlsHBox/ForwardBtn")
	
	assert_not_null(play_pause, "Sidebar must contain PlayPauseBtn under VBox/PlayerTiles")
	assert_not_null(backward, "Sidebar must contain BackwardBtn under ControlsHBox")
	assert_not_null(forward, "Sidebar must contain ForwardBtn under ControlsHBox")

func test_minimum_window_size_limits() -> void:
	assert_eq(main_node.MIN_WINDOW_SIZE, Vector2i(380, 400), "MIN_WINDOW_SIZE must be exactly 380x400")

func test_mini_player_responsive_squeeze() -> void:
	var mini_player = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/MiniPlayer")
	assert_not_null(mini_player, "MiniPlayer must exist")
	
	# Clear opposite anchors to prevent Godot layout warning when manually sizing
	mini_player.anchor_left = 0.0
	mini_player.anchor_right = 0.0
	
	# Simulate wide layout
	mini_player.size.x = 600
	mini_player._on_resized()
	assert_eq(mini_player.nav_library.text, "♪ Library", "Should show full text at large width")
	
	# Simulate narrow layout
	mini_player.size.x = 400
	mini_player._on_resized()
	assert_eq(mini_player.nav_library.text, "♪", "Should show icon-only at narrow width")

func test_sidebar_preview_square_visibility() -> void:
	var sidebar_node = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/Sidebar")
	assert_not_null(sidebar_node, "Sidebar must exist under LayoutRoot")
	
	# Verify initial state
	assert_false(sidebar_node.preview_square.visible, "PreviewSquare should be hidden initially")
	
	# Simulating track focus
	sidebar_node._on_track_focused("Artist", "Album", "Title", {}, "")
	assert_true(sidebar_node.preview_square.visible, "PreviewSquare should be visible when track is focused")
	
	# Simulating artist focus with no image path (empty)
	sidebar_node._on_artist_focused("Artist", "")
	assert_false(sidebar_node.preview_square.visible, "PreviewSquare should be hidden when artist focused has no image path")
	
	# Simulating artist focus with an image path
	sidebar_node._on_artist_focused("Artist", "user://dummy_image.png")
	assert_true(sidebar_node.preview_square.visible, "PreviewSquare should be visible when artist focused has an image path")
	
	# Simulating album focus with no image path (empty)
	sidebar_node._on_album_focused("Artist", "Album", "")
	assert_false(sidebar_node.preview_square.visible, "PreviewSquare should be hidden when album focused has no image path")
	
	# Simulating album focus with an image path
	sidebar_node._on_album_focused("Artist", "Album", "user://dummy_image.png")
	assert_true(sidebar_node.preview_square.visible, "PreviewSquare should be visible when album focused has an image path")




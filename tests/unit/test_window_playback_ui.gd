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
	assert_eq(main_node.MIN_WINDOW_SIZE, Vector2i(480, 400), "MIN_WINDOW_SIZE must be exactly 480x400")

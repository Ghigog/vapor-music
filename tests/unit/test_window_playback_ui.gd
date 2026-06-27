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
	
	var play_pause = sidebar_node.get_node_or_null("VBox/PlayerTilesMargin/PlayerTiles/PlayPauseBtn")
	var backward = sidebar_node.get_node_or_null("VBox/PlayerTilesMargin/PlayerTiles/BackwardBtn")
	var forward = sidebar_node.get_node_or_null("VBox/PlayerTilesMargin/PlayerTiles/ForwardBtn")
	
	assert_not_null(play_pause, "Sidebar must contain PlayPauseBtn under VBox/PlayerTilesMargin/PlayerTiles")
	assert_not_null(backward, "Sidebar must contain BackwardBtn under PlayerTiles")
	assert_not_null(forward, "Sidebar must contain ForwardBtn under PlayerTiles")

func test_minimum_window_size_limits() -> void:
	assert_eq(main_node.MIN_WINDOW_SIZE, Vector2i(340, 380), "MIN_WINDOW_SIZE must be exactly 340x380")

func test_mini_player_responsive_squeeze() -> void:
	var mini_player = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/MiniPlayer")
	assert_not_null(mini_player, "MiniPlayer must exist")
	assert_not_null(mini_player.nav_playlists, "Playlists button must exist on MiniPlayer")
	
	# Clear opposite anchors to prevent Godot layout warning when manually sizing
	mini_player.anchor_left = 0.0
	mini_player.anchor_right = 0.0
	
	# Simulate wide layout
	mini_player.size.x = 600
	mini_player._on_resized()
	assert_eq(mini_player.nav_library.text, "♪ Library", "Should show full text at large width")
	assert_eq(mini_player.nav_playlists.text, "▤ Playlists", "Should show full text at large width")
	
	# Simulate narrow layout
	mini_player.size.x = 400
	mini_player._on_resized()
	assert_eq(mini_player.nav_library.text, "♪", "Should show icon-only at narrow width")
	assert_eq(mini_player.nav_playlists.text, "▤", "Should show icon-only at narrow width")

func test_mini_player_playlists_popup() -> void:
	var mini_player = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/MiniPlayer")
	assert_not_null(mini_player, "MiniPlayer must exist")
	
	assert_null(mini_player.playlist_popup, "Popup should be null initially")
	
	# Trigger the playlists pressed handler
	mini_player._on_nav_playlists_pressed()
	
	assert_not_null(mini_player.playlist_popup, "Popup should be instantiated after press")
	assert_true(mini_player.playlist_popup.visible, "Popup should be visible after first press")
	
	# Toggle it again
	mini_player._on_nav_playlists_pressed()
	assert_false(mini_player.playlist_popup.visible, "Popup should be hidden after second press")

func test_sidebar_preview_square_visibility() -> void:
	var sidebar_node = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/Sidebar")
	assert_not_null(sidebar_node, "Sidebar must exist under LayoutRoot")
	
	# Verify initial state
	assert_false(sidebar_node.preview_margin.visible, "PreviewSquare should be hidden initially")
	
	# Simulating track focus
	sidebar_node._on_track_focused("Artist", "Album", "Title", {}, "")
	assert_true(sidebar_node.preview_margin.visible, "PreviewSquare should be visible when track is focused")
	
	# Simulating artist focus with no image path (empty)
	sidebar_node._on_artist_focused("Artist", "")
	assert_false(sidebar_node.preview_margin.visible, "PreviewSquare should be hidden when artist focused has no image path")
	
	# Simulating artist focus with an image path
	sidebar_node._on_artist_focused("Artist", "user://dummy_image.png")
	assert_true(sidebar_node.preview_margin.visible, "PreviewSquare should be visible when artist focused has an image path")
	
	# Simulating album focus with no image path (empty)
	sidebar_node._on_album_focused("Artist", "Album", "")
	assert_false(sidebar_node.preview_margin.visible, "PreviewSquare should be hidden when album focused has no image path")
	
	# Simulating album focus with an image path
	sidebar_node._on_album_focused("Artist", "Album", "user://dummy_image.png")
	assert_true(sidebar_node.preview_margin.visible, "PreviewSquare should be visible when album focused has an image path")

func test_transition_timeline_waveforms() -> void:
	var vibe_screen = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/ContentFrame/ScreenContainer/VibeScreen")
	assert_not_null(vibe_screen, "VibeScreen must exist")
	var timeline = vibe_screen.get_node_or_null("VBox/TransitionCard/VBox/TransitionTimeline")
	assert_not_null(timeline, "TransitionTimeline must exist")
	
	# Verify that the new properties exist and are initialized to defaults
	assert_true("outgoing_peaks" in timeline, "timeline must have outgoing_peaks")
	assert_true("incoming_peaks" in timeline, "timeline must have incoming_peaks")
	assert_true("outgoing_duration" in timeline, "timeline must have outgoing_duration")
	assert_true("incoming_duration" in timeline, "timeline must have incoming_duration")
	assert_true("incoming_cue_in" in timeline, "timeline must have incoming_cue_in")
	
	assert_eq(timeline.outgoing_peaks.size(), 0, "outgoing_peaks should be empty initially")
	assert_eq(timeline.incoming_peaks.size(), 0, "incoming_peaks should be empty initially")
	assert_eq(timeline.outgoing_duration, 0.0, "outgoing_duration should be 0.0 initially")
	assert_eq(timeline.incoming_duration, 0.0, "incoming_duration should be 0.0 initially")
	assert_eq(timeline.incoming_cue_in, 0.0, "incoming_cue_in should be 0.0 initially")

func test_vertical_progress_waveform_properties() -> void:
	var vp = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/VerticalProgress")
	assert_not_null(vp, "VerticalProgress node must exist under LayoutRoot")
	
	# Verify new properties exist and are initialized
	assert_true("peaks" in vp, "VerticalProgress must have peaks property")
	assert_true("max_amplitude" in vp, "VerticalProgress must have max_amplitude property")
	assert_eq(vp.peaks.size(), 0, "peaks should be empty initially")
	assert_eq(vp.max_amplitude, 20.0, "max_amplitude should default to 20.0")
	
	# Verify that _smooth_peaks functions correctly
	var raw = PackedFloat32Array([0.1, 0.5, 0.9, 0.3, 0.2, 0.8, 0.4])
	var smoothed = vp._smooth_peaks(raw)
	assert_eq(smoothed.size(), raw.size(), "smoothed array must retain size")
	
	# Verify it's smoothed (value shouldn't equal raw spikes)
	assert_ne(smoothed[2], 0.9, "smoothed value should reduce high peak spike")

func test_vibe_screen_responsive_cards() -> void:
	var vibe_screen = main_node.get_node_or_null("AppWindowFrame/LayoutRoot/ContentFrame/ScreenContainer/VibeScreen")
	assert_not_null(vibe_screen, "VibeScreen must exist")
	
	var list = vibe_screen.get_node_or_null("VBox/RunnerUpsScroll/RunnerUpsList")
	assert_not_null(list, "RunnerUpsList node must exist under ScrollContainer")
	assert_true(list is BoxContainer, "RunnerUpsList must be a BoxContainer")
	
	# Clear existing children to have a predictable test setup
	for child in list.get_children():
		child.queue_free()
	
	# Add a couple of dummy cards
	var card1 = Button.new()
	var card2 = Button.new()
	list.add_child(card1)
	list.add_child(card2)
	
	var orig_bp = PlatformManager.current_breakpoint
	
	# Test Desktop layout
	PlatformManager.current_breakpoint = PlatformManager.BREAKPOINT_LG
	list.get_parent_control().size = Vector2(800, 400)
	list.size = Vector2(800, 400)
	vibe_screen._update_cards_layout()
	
	assert_false(list.vertical, "List should not be vertical on desktop layout")
	var expected_width = clamp((list.get_parent_control().size.x - 16.0) / 2.0, vibe_screen.CARD_MIN_WIDTH, vibe_screen.CARD_MAX_WIDTH)
	var expected_height = expected_width + vibe_screen.CARD_HEIGHT_OFFSET
	assert_eq(card1.custom_minimum_size.y, expected_height, "Card height should be proportional to its width")
	assert_true((card1.size_flags_horizontal & Control.SIZE_SHRINK_CENTER) != 0, "Card size flags should be set for centering/filling when width is large")
	
	# Test Mobile layout
	PlatformManager.current_breakpoint = PlatformManager.BREAKPOINT_XS
	list.get_parent_control().size = Vector2(300, 400)
	list.size = Vector2(300, 400)
	vibe_screen._update_cards_layout()
	
	assert_true(list.vertical, "List should be vertical on mobile layout")
	var expected_mobile_width = clamp(list.get_parent_control().size.x, vibe_screen.CARD_MIN_WIDTH, vibe_screen.CARD_MAX_WIDTH)
	var expected_mobile_height = expected_mobile_width + vibe_screen.CARD_HEIGHT_OFFSET
	assert_eq(card1.custom_minimum_size.y, expected_mobile_height, "Card height should be proportional to width on mobile stack")
	
	# Clean up and reset
	PlatformManager.current_breakpoint = orig_bp
	card1.free()
	card2.free()







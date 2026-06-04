## test_nav_manager.gd
## GUT unit tests for NavManager screen state navigation, stack manipulation, and history logic.
extends GutTest

func before_each() -> void:
	NavManager.clear_history()
	NavManager.current_screen = "library"

func after_each() -> void:
	NavManager.clear_history()

func test_default_screen() -> void:
	assert_eq(NavManager.current_screen, "library", "Default screen should be library")
	assert_false(NavManager.can_go_back(), "History should be empty initially")

func test_navigation_requested() -> void:
	var signal_watcher = watch_signals(NavManager)
	
	NavManager.navigate_to("settings")
	
	assert_eq(NavManager.current_screen, "settings", "Active screen should update to settings")
	assert_true(NavManager.can_go_back(), "History stack should contain previous screen")
	assert_signal_emitted_with_parameters(NavManager, "navigation_requested", ["settings"], 0)

func test_navigation_redundancy() -> void:
	var signal_watcher = watch_signals(NavManager)
	
	NavManager.navigate_to("library") # Navigate to already active screen
	
	assert_signal_not_emitted(NavManager, "navigation_requested")
	assert_false(NavManager.can_go_back(), "History stack should remain empty")

func test_go_back() -> void:
	var signal_watcher = watch_signals(NavManager)
	
	NavManager.navigate_to("settings")
	NavManager.navigate_to("search")
	
	assert_eq(NavManager.current_screen, "search")
	
	NavManager.go_back()
	
	assert_eq(NavManager.current_screen, "settings", "Should pop back to settings screen")
	assert_signal_emitted(NavManager, "back_requested")
	assert_signal_emitted_with_parameters(NavManager, "navigation_requested", ["settings"], 2)

func test_clear_history() -> void:
	NavManager.navigate_to("settings")
	NavManager.navigate_to("search")
	
	assert_true(NavManager.can_go_back())
	
	NavManager.clear_history()
	
	assert_false(NavManager.can_go_back(), "Stack should be empty after clear_history")

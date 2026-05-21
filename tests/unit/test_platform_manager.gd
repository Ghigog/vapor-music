## test_platform_manager.gd
## GUT unit tests for PlatformManager platform detection and breakpoints.
extends GutTest


func test_platform_name_not_empty() -> void:
	assert_ne(
		PlatformManager.platform_name,
		"",
		"platform_name must be populated on _ready"
	)


func test_breakpoint_populated_after_ready() -> void:
	assert_ne(
		PlatformManager.current_breakpoint,
		"",
		"current_breakpoint must be set after _ready"
	)


func test_breakpoint_is_valid_value() -> void:
	var valid := ["xs", "sm", "md", "lg", "xl"]
	assert_true(
		PlatformManager.current_breakpoint in valid,
		"current_breakpoint must be one of: xs sm md lg xl"
	)


func test_desktop_and_mobile_are_mutually_exclusive() -> void:
	if PlatformManager.is_mobile():
		assert_false(PlatformManager.is_desktop(), "Cannot be both mobile and desktop")
	else:
		assert_false(PlatformManager.is_mobile(), "Cannot be both desktop and mobile")


func test_viewport_size_is_positive() -> void:
	var size := PlatformManager.get_viewport_size()
	assert_gt(size.x, 0.0, "Viewport width must be positive")
	assert_gt(size.y, 0.0, "Viewport height must be positive")


func test_sidebar_hidden_on_mobile() -> void:
	if PlatformManager.is_mobile():
		assert_false(
			PlatformManager.should_show_sidebar(),
			"Sidebar must not show on mobile hardware"
		)


func test_is_mobile_layout_reflects_breakpoint() -> void:
	var bp := PlatformManager.current_breakpoint
	if bp in ["xs", "sm"]:
		assert_true(PlatformManager.is_mobile_layout())
	else:
		assert_false(PlatformManager.is_mobile_layout())

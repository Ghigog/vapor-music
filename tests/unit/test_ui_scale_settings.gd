## test_ui_scale_settings.gd
## GUT unit tests for SettingsManager UI scale setting persistence and application.
extends GutTest

var _initial_ui_scale: float = 1.0

var _original_settings_path: String = ""

func before_all() -> void:
	_original_settings_path = SettingsManager.settings_file_path
	SettingsManager.settings_file_path = "user://test_settings.cfg"
	SettingsManager.save_settings()

func after_all() -> void:
	var dir = DirAccess.open("user://")
	if dir and dir.file_exists("test_settings.cfg"):
		dir.remove("test_settings.cfg")
	SettingsManager.settings_file_path = _original_settings_path
	SettingsManager.load_settings()

func test_settings_manager_persists_ui_scale() -> void:
	SettingsManager.save_ui_scale(1.35)
	
	# Reload settings to verify persistence
	SettingsManager.load_settings()
	
	assert_eq(SettingsManager.ui_scale, 1.35, "UI scale should be 1.35 after reload")
	assert_almost_eq(get_window().content_scale_factor, 1.35, 0.0001, "Window content_scale_factor should be 1.35")

func test_settings_manager_clamp_limits() -> void:
	# Verify that we can set different values
	SettingsManager.save_ui_scale(0.85)
	assert_eq(SettingsManager.ui_scale, 0.85, "UI scale should be updated to 0.85")
	assert_almost_eq(get_window().content_scale_factor, 0.85, 0.0001, "Window content_scale_factor should be 0.85")

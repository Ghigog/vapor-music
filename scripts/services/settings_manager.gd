extends Node
## SettingsManager
##
## Manages user preferences and persistent settings using ConfigFile.
## Stores WebDAV connection details.

const SETTINGS_FILE_PATH = "user://settings.cfg"
const SECTION_WEBDAV = "webdav"
const SECTION_SYSTEM = "system"

var config := ConfigFile.new()

# WebDAV settings
var webdav_url: String = ""
var webdav_username: String = ""
var webdav_password: String = ""
var webdav_folder: String = "Music"

# UI Settings
var base_font_size: int = 16

# Headphone Calibration Settings
var headphone_profile: String = ""
var headphone_calibration_enabled: bool = false

signal credentials_loaded()

func _ready() -> void:
	load_settings()

func load_settings() -> void:
	var err = config.load(SETTINGS_FILE_PATH)
	if err == OK:
		webdav_url = config.get_value(SECTION_WEBDAV, "url", "")
		webdav_username = config.get_value(SECTION_WEBDAV, "username", "")
		webdav_password = config.get_value(SECTION_WEBDAV, "password", "")
		webdav_folder = config.get_value(SECTION_WEBDAV, "folder", "Music")
		base_font_size = config.get_value(SECTION_SYSTEM, "base_font_size", 16)
		headphone_profile = config.get_value(SECTION_SYSTEM, "headphone_profile", "")
		headphone_calibration_enabled = config.get_value(SECTION_SYSTEM, "headphone_calibration_enabled", false)
		credentials_loaded.emit()
		
		# Set initial base font size in ThemeManager
		ThemeManager.set_base_font_size(base_font_size)

func save_settings() -> void:
	config.set_value(SECTION_WEBDAV, "url", webdav_url)
	config.set_value(SECTION_WEBDAV, "username", webdav_username)
	config.set_value(SECTION_WEBDAV, "password", webdav_password)
	config.set_value(SECTION_WEBDAV, "folder", webdav_folder)
	config.set_value(SECTION_SYSTEM, "base_font_size", base_font_size)
	config.set_value(SECTION_SYSTEM, "headphone_profile", headphone_profile)
	config.set_value(SECTION_SYSTEM, "headphone_calibration_enabled", headphone_calibration_enabled)
	config.save(SETTINGS_FILE_PATH)

func has_credentials() -> bool:
	return webdav_url != "" and webdav_username != "" and webdav_password != ""

func save_credentials(url: String, user: String, passw: String) -> void:
	webdav_url = url
	webdav_username = user
	webdav_password = passw
	save_settings()

func save_target_folder(folder: String) -> void:
	webdav_folder = folder
	save_settings()

func save_base_font_size(size: int) -> void:
	base_font_size = size
	save_settings()
	ThemeManager.set_base_font_size(base_font_size)

func save_headphone_profile(profile: String) -> void:
	headphone_profile = profile
	save_settings()

func save_headphone_calibration_enabled(enabled: bool) -> void:
	headphone_calibration_enabled = enabled
	save_settings()

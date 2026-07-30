extends Node
## SettingsManager
##
## Manages user preferences and persistent settings using ConfigFile.
## Stores WebDAV connection details.

var settings_file_path = "user://settings.cfg"
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
var ui_scale: float = 1.2

# Theme Settings
## "preset" (a .tres in assets/themes/) or "custom" (generated from the two colors below).
var theme_mode: String = "preset"
var active_theme_path: String = "res://assets/themes/default_dark.tres"
var custom_base_color: Color = Color(0.157, 0.157, 0.157, 1.0)
var custom_accent_color: Color = Color(0.0392, 0.5176, 1.0, 1.0)

# Headphone Calibration Settings
var headphone_profile: String = ""
var headphone_calibration_enabled: bool = false

signal credentials_loaded()

func _ready() -> void:
	load_settings()

func _get_encryption_password() -> String:
	var device_id := OS.get_unique_id()
	if device_id.is_empty():
		device_id = "VaporMusicPlayerSalt"
	return (device_id + "_vapor_secure").md5_text()

func load_settings() -> void:
	if not FileAccess.file_exists(settings_file_path):
		# If missing, save a default configuration immediately
		save_settings()
		return
	
	# Verify if the first byte is plain text to avoid C++ console error from load_encrypted_pass
	var is_plain_text := false
	var file := FileAccess.open(settings_file_path, FileAccess.READ)
	if file:
		if file.get_length() == 0:
			is_plain_text = true
		else:
			var first_byte := file.get_8()
			# Plain text files typically start with '[', ';', whitespace, or newlines
			if first_byte == 91 or first_byte == 59 or first_byte == 10 or first_byte == 13 or first_byte == 32 or first_byte == 9:
				is_plain_text = true
		file.close()
	
	var passw := _get_encryption_password()
	var err: Error = OK
	if is_plain_text:
		err = config.load(settings_file_path)
		if err == OK:
			# Upgrade to encrypted file immediately
			save_settings()
	else:
		err = config.load_encrypted_pass(settings_file_path, passw)
		
	if err == OK:
		webdav_url = config.get_value(SECTION_WEBDAV, "url", "")
		webdav_username = config.get_value(SECTION_WEBDAV, "username", "")
		webdav_password = config.get_value(SECTION_WEBDAV, "password", "")
		webdav_folder = config.get_value(SECTION_WEBDAV, "folder", "Music")
		base_font_size = config.get_value(SECTION_SYSTEM, "base_font_size", 16)
		ui_scale = config.get_value(SECTION_SYSTEM, "ui_scale", 1.2)
		headphone_profile = config.get_value(SECTION_SYSTEM, "headphone_profile", "")
		headphone_calibration_enabled = config.get_value(SECTION_SYSTEM, "headphone_calibration_enabled", false)
		theme_mode = config.get_value(SECTION_SYSTEM, "theme_mode", "preset")
		active_theme_path = config.get_value(SECTION_SYSTEM, "active_theme_path", "res://assets/themes/default_dark.tres")
		custom_base_color = config.get_value(SECTION_SYSTEM, "custom_base_color", custom_base_color)
		custom_accent_color = config.get_value(SECTION_SYSTEM, "custom_accent_color", custom_accent_color)
		credentials_loaded.emit()

		# Apply the persisted theme before the font-size pass below so its
		# tokens are in place first.
		if theme_mode == "custom":
			ThemeManager.apply_custom_colors(custom_base_color, custom_accent_color)
		elif active_theme_path != "":
			ThemeManager.load_theme(active_theme_path)

		# Set initial base font size in ThemeManager
		ThemeManager.set_base_font_size(base_font_size)
		# Set initial UI scale factor on the window
		get_window().content_scale_factor = ui_scale

func save_settings() -> void:
	config.set_value(SECTION_WEBDAV, "url", webdav_url)
	config.set_value(SECTION_WEBDAV, "username", webdav_username)
	config.set_value(SECTION_WEBDAV, "password", webdav_password)
	config.set_value(SECTION_WEBDAV, "folder", webdav_folder)
	config.set_value(SECTION_SYSTEM, "base_font_size", base_font_size)
	config.set_value(SECTION_SYSTEM, "ui_scale", ui_scale)
	config.set_value(SECTION_SYSTEM, "headphone_profile", headphone_profile)
	config.set_value(SECTION_SYSTEM, "headphone_calibration_enabled", headphone_calibration_enabled)
	config.set_value(SECTION_SYSTEM, "theme_mode", theme_mode)
	config.set_value(SECTION_SYSTEM, "active_theme_path", active_theme_path)
	config.set_value(SECTION_SYSTEM, "custom_base_color", custom_base_color)
	config.set_value(SECTION_SYSTEM, "custom_accent_color", custom_accent_color)

	var passw := _get_encryption_password()
	config.save_encrypted_pass(settings_file_path, passw)

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

func save_ui_scale(scale: float) -> void:
	ui_scale = scale
	save_settings()
	var window = get_window()
	if window:
		window.content_scale_factor = ui_scale

## Switches to a bundled preset theme (e.g. Vapor Dark) and persists the choice.
func save_active_theme(path: String) -> void:
	theme_mode = "preset"
	active_theme_path = path
	save_settings()
	ThemeManager.load_theme(path)

## Switches to a fully custom theme generated from the two given colors and
## persists the choice. Applying is left to the caller (Settings screen
## applies live as the user drags the color wheel; this just saves).
func save_custom_theme_colors(base_color: Color, accent_color: Color) -> void:
	theme_mode = "custom"
	custom_base_color = base_color
	custom_accent_color = accent_color
	save_settings()

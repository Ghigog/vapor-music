extends Node
## SettingsManager
##
## Manages user preferences and persistent settings using ConfigFile.
## Stores WebDAV connection details.

const SETTINGS_FILE_PATH = "user://settings.cfg"
const SECTION_WEBDAV = "webdav"

var config := ConfigFile.new()

# WebDAV settings
var webdav_url: String = ""
var webdav_username: String = ""
var webdav_password: String = ""

signal credentials_loaded()

func _ready() -> void:
	load_settings()

func load_settings() -> void:
	var err = config.load(SETTINGS_FILE_PATH)
	if err == OK:
		webdav_url = config.get_value(SECTION_WEBDAV, "url", "")
		webdav_username = config.get_value(SECTION_WEBDAV, "username", "")
		webdav_password = config.get_value(SECTION_WEBDAV, "password", "")
		credentials_loaded.emit()

func save_settings() -> void:
	config.set_value(SECTION_WEBDAV, "url", webdav_url)
	config.set_value(SECTION_WEBDAV, "username", webdav_username)
	config.set_value(SECTION_WEBDAV, "password", webdav_password)
	config.save(SETTINGS_FILE_PATH)

func has_credentials() -> bool:
	return webdav_url != "" and webdav_username != "" and webdav_password != ""

func save_credentials(url: String, user: String, passw: String) -> void:
	webdav_url = url
	webdav_username = user
	webdav_password = passw
	save_settings()

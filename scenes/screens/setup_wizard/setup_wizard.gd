extends Control

signal wizard_completed

@onready var url_input: LineEdit = %UrlInput
@onready var username_input: LineEdit = %UsernameInput
@onready var password_input: LineEdit = %PasswordInput
@onready var test_button: Button = %TestButton
@onready var save_button: Button = %SaveButton
@onready var status_label: Label = %StatusLabel

var connection_successful := false

func _ready() -> void:
	WebDAVService.connection_tested.connect(_on_connection_tested)
	test_button.pressed.connect(_on_test_button_pressed)
	save_button.pressed.connect(_on_save_button_pressed)
	
	# Reset state
	save_button.disabled = true
	status_label.text = ""
	
	# Pre-fill with Koofr URL as default
	url_input.text = "https://app.koofr.net/dav/Koofr"
	
	# Every time text changes, require re-testing
	url_input.text_changed.connect(_on_text_changed)
	username_input.text_changed.connect(_on_text_changed)
	password_input.text_changed.connect(_on_text_changed)

func _on_text_changed(_new_text: String) -> void:
	save_button.disabled = true
	connection_successful = false
	status_label.text = ""

func _on_test_button_pressed() -> void:
	var url = url_input.text.strip_edges()
	var username = username_input.text.strip_edges()
	var password = password_input.text.strip_edges()
	
	if url == "" or username == "" or password == "":
		status_label.text = "Please fill in all fields."
		status_label.add_theme_color_override("font_color", Color.RED)
		return
		
	status_label.text = "Testing connection..."
	status_label.add_theme_color_override("font_color", Color.YELLOW)
	test_button.disabled = true
	
	WebDAVService.test_connection(url, username, password)

func _on_connection_tested(success: bool, error_message: String) -> void:
	test_button.disabled = false
	connection_successful = success
	
	if success:
		status_label.text = "Connection successful!"
		status_label.add_theme_color_override("font_color", Color.GREEN)
		save_button.disabled = false
	else:
		status_label.text = error_message
		status_label.add_theme_color_override("font_color", Color.RED)
		save_button.disabled = true

func _on_save_button_pressed() -> void:
	if not connection_successful:
		return
		
	var url = url_input.text.strip_edges()
	var username = username_input.text.strip_edges()
	var password = password_input.text.strip_edges()
	
	SettingsManager.save_credentials(url, username, password)
	wizard_completed.emit()

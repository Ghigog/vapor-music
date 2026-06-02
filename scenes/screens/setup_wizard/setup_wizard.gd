extends Control

signal wizard_completed
signal wizard_cancelled

@onready var url_input: LineEdit = %UrlInput
@onready var username_input: LineEdit = %UsernameInput
@onready var password_input: LineEdit = %PasswordInput
@onready var folder_input: LineEdit = %FolderInput
@onready var cancel_button: Button = %CancelButton
@onready var test_button: Button = %TestButton
@onready var save_button: Button = %SaveButton
@onready var status_label: Label = %StatusLabel

var connection_successful := false

func _ready() -> void:
	WebDAVService.connection_tested.connect(_on_connection_tested)
	test_button.pressed.connect(_on_test_button_pressed)
	save_button.pressed.connect(_on_save_button_pressed)
	cancel_button.pressed.connect(_on_cancel_button_pressed)
	
	# Connect to theme changes
	_apply_styles()
	ThemeManager.theme_changed.connect(_apply_styles)
	
	# Every time text changes, require re-testing
	url_input.text_changed.connect(_on_text_changed)
	username_input.text_changed.connect(_on_text_changed)
	password_input.text_changed.connect(_on_text_changed)
	folder_input.text_changed.connect(_on_text_changed)

	refresh_fields_and_visibility()

func refresh_fields_and_visibility() -> void:
	# Reset state
	save_button.disabled = true
	connection_successful = false
	status_label.text = ""
	
	# Pre-fill fields with smart defaults or existing saved values
	if SettingsManager.has_credentials():
		url_input.text = SettingsManager.webdav_url
		username_input.text = SettingsManager.webdav_username
		password_input.text = SettingsManager.webdav_password
		folder_input.text = SettingsManager.webdav_folder
	else:
		url_input.text = "https://app.koofr.net/dav/Koofr"
		folder_input.text = "Music" # Matches your cloud setup out of the box!
		
	# Only allow canceling if credentials already exist
	cancel_button.visible = SettingsManager.has_credentials()

func _apply_styles() -> void:
	var theme = ThemeManager.current_theme
	
	# Modal panel style
	var modal_panel: PanelContainer = $CenterContainer/ModalPanel
	if modal_panel:
		modal_panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD, 0.95))
		
	# Margins
	var margin_container: MarginContainer = $CenterContainer/ModalPanel/MarginContainer
	if margin_container:
		margin_container.add_theme_constant_override("margin_left", theme.SPACE_5)
		margin_container.add_theme_constant_override("margin_right", theme.SPACE_5)
		margin_container.add_theme_constant_override("margin_top", theme.SPACE_5)
		margin_container.add_theme_constant_override("margin_bottom", theme.SPACE_5)
		
	# Dim Background Color
	var dim_bg: Panel = $DimBackground
	if dim_bg:
		var bg_style := StyleBoxFlat.new()
		bg_style.bg_color = Color(0.0, 0.0, 0.0, 0.6)
		dim_bg.add_theme_stylebox_override("panel", bg_style)
		
	# Titles & Descriptions
	var title_lbl: Label = $CenterContainer/ModalPanel/MarginContainer/VBoxContainer/TitleLabel
	title_lbl.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	title_lbl.add_theme_font_override("font", theme.font_display)
	title_lbl.add_theme_font_size_override("font_size", theme.TYPE_LG)
	
	var desc_lbl: Label = $CenterContainer/ModalPanel/MarginContainer/VBoxContainer/DescLabel
	desc_lbl.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	desc_lbl.add_theme_font_override("font", theme.font_ui)
	desc_lbl.add_theme_font_size_override("font_size", theme.TYPE_SM)
	
	# Input Labels
	for label_path in [
		"CenterContainer/ModalPanel/MarginContainer/VBoxContainer/UrlContainer/Label",
		"CenterContainer/ModalPanel/MarginContainer/VBoxContainer/UserContainer/Label",
		"CenterContainer/ModalPanel/MarginContainer/VBoxContainer/PassContainer/Label",
		"CenterContainer/ModalPanel/MarginContainer/VBoxContainer/FolderContainer/Label"
	]:
		var lbl = get_node(label_path) as Label
		if lbl:
			lbl.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
			lbl.add_theme_font_override("font", theme.font_ui)
			lbl.add_theme_font_size_override("font_size", theme.TYPE_XS)
			
	# Inputs (LineEdits)
	for input in [url_input, username_input, password_input, folder_input]:
		if input:
			input.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
			input.add_theme_color_override("placeholder_color", theme.TEXT_DISABLED)
			input.add_theme_font_override("font", theme.font_ui)
			input.add_theme_font_size_override("font_size", theme.TYPE_SM)
			var input_style = StyleBoxFlat.new()
			input_style.bg_color = theme.BG_ELEVATED
			input_style.border_color = theme.GLASS_BORDER
			input_style.border_width_left = 1
			input_style.border_width_right = 1
			input_style.border_width_top = 1
			input_style.border_width_bottom = 1
			input_style.set_corner_radius_all(theme.RADIUS_XS)
			input_style.content_margin_left = 10
			input_style.content_margin_right = 10
			input_style.content_margin_top = 6
			input_style.content_margin_bottom = 6
			input.add_theme_stylebox_override("normal", input_style)
			input.add_theme_stylebox_override("focus", input_style)
			
	# Buttons
	for btn in [cancel_button, test_button, save_button]:
		if btn:
			btn.add_theme_font_override("font", theme.font_ui)
			btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
			btn.add_theme_color_override("font_hover_color", theme.TEXT_INVERSE)
			btn.add_theme_color_override("font_pressed_color", theme.TEXT_INVERSE)
			btn.add_theme_color_override("font_focus_color", theme.TEXT_INVERSE)
			btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
			
	# Distinct style for cancel button
	cancel_button.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	var cancel_normal = ThemeManager.make_cta_button(false)
	cancel_normal.bg_color = Color(0, 0, 0, 0)
	cancel_normal.border_color = theme.GLASS_BORDER
	cancel_button.add_theme_stylebox_override("normal", cancel_normal)
	cancel_button.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	cancel_button.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	
	# Test button normal/hover
	test_button.add_theme_color_override("font_color", theme.ACCENT_BRIGHT)
	test_button.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
	test_button.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	test_button.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	
	# Save & Start button normal/hover/disabled
	save_button.add_theme_color_override("font_color", theme.AQUA_CORE)
	save_button.add_theme_color_override("font_disabled_color", theme.TEXT_DISABLED)
	save_button.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
	save_button.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	save_button.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	var save_disabled = ThemeManager.make_cta_button(false)
	save_disabled.bg_color = theme.BG_ELEVATED
	save_disabled.border_color = theme.GLASS_BORDER_SUBTLE
	save_button.add_theme_stylebox_override("disabled", save_disabled)

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
		status_label.add_theme_color_override("font_color", ThemeManager.current_theme.SEMANTIC_ERROR)
		return
		
	status_label.text = "Testing connection..."
	status_label.add_theme_color_override("font_color", ThemeManager.current_theme.SEMANTIC_WARNING)
	test_button.disabled = true
	
	WebDAVService.test_connection(url, username, password)

func _on_connection_tested(success: bool, error_message: String) -> void:
	test_button.disabled = false
	connection_successful = success
	
	if success:
		status_label.text = "Connection successful!"
		status_label.add_theme_color_override("font_color", ThemeManager.current_theme.SEMANTIC_SUCCESS)
		save_button.disabled = false
	else:
		status_label.text = error_message
		status_label.add_theme_color_override("font_color", ThemeManager.current_theme.SEMANTIC_ERROR)
		save_button.disabled = true

func _on_save_button_pressed() -> void:
	if not connection_successful:
		return
		
	var url = url_input.text.strip_edges()
	var username = username_input.text.strip_edges()
	var password = password_input.text.strip_edges()
	var target_folder = folder_input.text.strip_edges()
	
	SettingsManager.save_credentials(url, username, password)
	SettingsManager.save_target_folder(target_folder)
	
	# Fire the background scan immediately using manual path target
	WebDAVService.scan_music_directory(target_folder)
	
	wizard_completed.emit()

func _on_cancel_button_pressed() -> void:
	wizard_cancelled.emit()

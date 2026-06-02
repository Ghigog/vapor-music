## settings_screen.gd
## App settings screen.
## MVP: placeholder. Full settings UI (EQ, themes, cloud) is phase-2.
extends Control

@onready var icon_label: Label = $Center/Placeholder/IconLabel
@onready var heading:    Label = $Center/Placeholder/HeadingLabel
@onready var body:       Label = $Center/Placeholder/BodyLabel
@onready var select_a_theme: Label = $"Center/Placeholder/Select a theme"
@onready var theme_selector: OptionButton = $Center/Placeholder/ThemeSelector

@onready var font_size_label: Label = %FontSizeLabel
@onready var font_size_spinbox: SpinBox = %FontSizeSpinBox
@onready var connect_library_button: Button = %ConnectLibraryButton

const THEME_MAP = {
	"Vapor Dark" : "res://assets/themes/default_dark.tres",
	"Vapor Light" : "res://assets/themes/default_light.tres"
}

func _ready() -> void:
	_apply_styles()
	ThemeManager.theme_changed.connect(_apply_styles)
	
	# Populate the OptionButton
	theme_selector.clear()
	for theme_name in THEME_MAP.keys():
		theme_selector.add_item(theme_name)
		
	# Select current active theme
	var active_path = ThemeManager.current_theme.resource_path
	for theme_name in THEME_MAP.keys():
		if THEME_MAP[theme_name] == active_path:
			theme_selector.text = theme_name
		
	# Connect the selection signal
	theme_selector.item_selected.connect(_on_theme_selected)

	# Initialize font size spinbox
	font_size_spinbox.value = SettingsManager.base_font_size
	font_size_spinbox.value_changed.connect(_on_font_size_changed)
	
	# Connect library connection trigger button
	connect_library_button.pressed.connect(_on_connect_library_pressed)

func _apply_styles() -> void:
	icon_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
	icon_label.add_theme_font_size_override("font_size", 80)

	heading.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	heading.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	heading.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_LG)

	select_a_theme.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	select_a_theme.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	select_a_theme.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_LG)

	theme_selector.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	theme_selector.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	theme_selector.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	
	var opt_style = StyleBoxFlat.new()
	opt_style.bg_color = ThemeManager.current_theme.BG_ELEVATED
	opt_style.border_color = ThemeManager.current_theme.GLASS_BORDER
	opt_style.set_border_width_all(1)
	opt_style.set_corner_radius_all(ThemeManager.current_theme.RADIUS_XS)
	opt_style.content_margin_left = 10
	opt_style.content_margin_right = 10
	opt_style.content_margin_top = 6
	opt_style.content_margin_bottom = 6
	theme_selector.add_theme_stylebox_override("normal", opt_style)
	theme_selector.add_theme_stylebox_override("hover", opt_style)
	theme_selector.add_theme_stylebox_override("pressed", opt_style)
	theme_selector.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

	body.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	body.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	body.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

	# Font size label style
	font_size_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	font_size_label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	font_size_label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	
	# Set custom SpinBox line edit font and color
	var spinbox_line_edit = font_size_spinbox.get_line_edit()
	if spinbox_line_edit:
		spinbox_line_edit.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
		spinbox_line_edit.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		spinbox_line_edit.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
		var spinbox_style = StyleBoxFlat.new()
		spinbox_style.bg_color = ThemeManager.current_theme.BG_ELEVATED
		spinbox_style.border_color = ThemeManager.current_theme.GLASS_BORDER
		spinbox_style.set_border_width_all(1)
		spinbox_style.set_corner_radius_all(ThemeManager.current_theme.RADIUS_XS)
		spinbox_style.content_margin_left = 8
		spinbox_style.content_margin_right = 8
		spinbox_line_edit.add_theme_stylebox_override("normal", spinbox_style)
		spinbox_line_edit.add_theme_stylebox_override("focus", spinbox_style)
		
	# Connect Library Button styles
	connect_library_button.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	connect_library_button.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	connect_library_button.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	connect_library_button.add_theme_color_override("font_hover_color", ThemeManager.current_theme.TEXT_INVERSE)
	connect_library_button.add_theme_color_override("font_pressed_color", ThemeManager.current_theme.TEXT_INVERSE)
	connect_library_button.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
	connect_library_button.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	connect_library_button.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	connect_library_button.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

func _on_theme_selected(index: int) -> void:
	# Get the name (key) based on the index
	var theme_name = theme_selector.get_item_text(index)
	var theme_path = THEME_MAP[theme_name]
	
	# Update the global theme manager
	ThemeManager.load_theme(theme_path)

func _on_font_size_changed(value: float) -> void:
	SettingsManager.save_base_font_size(int(value))

func _on_connect_library_pressed() -> void:
	NavManager.show_setup_wizard()

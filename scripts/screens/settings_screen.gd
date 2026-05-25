## settings_screen.gd
## App settings screen.
## MVP: placeholder. Full settings UI (EQ, themes, cloud) is phase-2.
extends Control

@onready var icon_label: Label = $Center/Placeholder/IconLabel
@onready var heading:    Label = $Center/Placeholder/HeadingLabel
@onready var body:       Label = $Center/Placeholder/BodyLabel
@onready var select_a_theme: Label = $"Center/Placeholder/Select a theme"
@onready var theme_selector: OptionButton = $Center/Placeholder/ThemeSelector
const THEME_MAP = {
	"Default Dark" : "res://assets/themes/default_dark.tres",
	"Light Mode" : "res://assets/themes/default_light.tres"
}

func _ready() -> void:
	_apply_styles()
	ThemeManager.theme_changed.connect(_apply_styles)
	# Populate the OptionButton
	for theme_name in THEME_MAP.keys():
		theme_selector.add_item(theme_name)
		
	# Connect the selection signal
	theme_selector.item_selected.connect(_on_theme_selected)

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

	body.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	body.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	body.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

func _on_theme_selected(index: int) -> void:
	# Get the name (key) based on the index
	var theme_name = theme_selector.get_item_text(index)
	var theme_path = THEME_MAP[theme_name]
	
	# Update the global theme manager
	ThemeManager.load_theme(theme_path)
	

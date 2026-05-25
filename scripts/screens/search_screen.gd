## search_screen.gd
## In-library search screen.
## MVP: placeholder. Full search UI is a phase-2 feature.
extends Control

@onready var icon_label: Label = $Center/Placeholder/IconLabel
@onready var heading:    Label = $Center/Placeholder/HeadingLabel
@onready var body:       Label = $Center/Placeholder/BodyLabel


func _ready() -> void:
	_apply_styles()
	ThemeManager.theme_changed.connect(_apply_styles)


func _apply_styles() -> void:
	icon_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
	icon_label.add_theme_font_size_override("font_size", 80)

	heading.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	heading.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	heading.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_LG)

	body.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	body.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	body.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

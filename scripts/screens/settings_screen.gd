## settings_screen.gd
## App settings screen.
## MVP: placeholder. Full settings UI (EQ, themes, cloud) is phase-2.
extends Control

@onready var icon_label: Label = $Center/Placeholder/IconLabel
@onready var heading:    Label = $Center/Placeholder/HeadingLabel
@onready var body:       Label = $Center/Placeholder/BodyLabel


func _ready() -> void:
	_apply_styles()


func _apply_styles() -> void:
	icon_label.add_theme_color_override("font_color", ThemeManager.TEXT_TERTIARY)
	icon_label.add_theme_font_size_override("font_size", 80)

	heading.add_theme_color_override("font_color", ThemeManager.TEXT_PRIMARY)
	heading.add_theme_font_override("font", ThemeManager.font_display)
	heading.add_theme_font_size_override("font_size", ThemeManager.TYPE_LG)

	body.add_theme_color_override("font_color", ThemeManager.TEXT_SECONDARY)
	body.add_theme_font_override("font", ThemeManager.font_ui)
	body.add_theme_font_size_override("font_size", ThemeManager.TYPE_SM)

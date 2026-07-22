## glass_modal.gd
## Reusable in-app confirmation modal (DESIGN_LANGUAGE.md §14.4): dim inside
## the app frame with the frame's corner radius, centered glass panel, quiet
## Cancel and a CTA-styled confirm. Never a native ConfirmationDialog.
extends RefCounted


static func confirm(context: Node, title_text: String, body_text: String, confirm_label: String, on_confirm: Callable) -> void:
	var theme = ThemeManager.current_theme
	var overlay := Panel.new()
	var dim := StyleBoxFlat.new()
	dim.bg_color = Color(0, 0, 0, 0.45)
	dim.set_corner_radius_all(theme.RADIUS_LG)
	overlay.add_theme_stylebox_override("panel", dim)
	var host: Control = context.get_tree().current_scene.get_node_or_null("AppWindowFrame")
	if host == null:
		host = context.get_tree().current_scene
	host.add_child(overlay)
	overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var center := CenterContainer.new()
	overlay.add_child(center)
	center.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD, 0.92))
	center.add_child(panel)

	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", theme.SPACE_6)
	margin.add_theme_constant_override("margin_right", theme.SPACE_6)
	margin.add_theme_constant_override("margin_top", theme.SPACE_5)
	margin.add_theme_constant_override("margin_bottom", theme.SPACE_5)
	panel.add_child(margin)

	var vbox := VBoxContainer.new()
	vbox.add_theme_constant_override("separation", theme.SPACE_4)
	margin.add_child(vbox)

	var title := Label.new()
	title.text = title_text
	title.add_theme_font_override("font", theme.font_display)
	title.add_theme_font_size_override("font_size", theme.TYPE_MD)
	title.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	vbox.add_child(title)

	var body := Label.new()
	body.text = body_text
	body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	body.custom_minimum_size.x = 220
	body.add_theme_font_override("font", theme.font_ui)
	body.add_theme_font_size_override("font_size", theme.TYPE_SM)
	body.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	vbox.add_child(body)

	var buttons := HBoxContainer.new()
	buttons.alignment = BoxContainer.ALIGNMENT_END
	buttons.add_theme_constant_override("separation", theme.SPACE_2)
	vbox.add_child(buttons)

	var cancel := Button.new()
	cancel.text = "  Cancel  "
	cancel.custom_minimum_size.y = ThemeManager.min_touch_height(32)
	cancel.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	cancel.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	cancel.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	cancel.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	cancel.add_theme_font_override("font", theme.font_ui)
	cancel.add_theme_font_size_override("font_size", theme.TYPE_SM)
	cancel.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	buttons.add_child(cancel)

	var confirm_btn := Button.new()
	confirm_btn.text = "  %s  " % confirm_label
	confirm_btn.custom_minimum_size.y = ThemeManager.min_touch_height(32)
	confirm_btn.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
	confirm_btn.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	confirm_btn.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	confirm_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	confirm_btn.add_theme_font_override("font", theme.font_ui)
	confirm_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	confirm_btn.add_theme_color_override("font_color", theme.ACCENT_BRIGHT)
	confirm_btn.add_theme_color_override("font_hover_color", theme.TEXT_INVERSE)
	buttons.add_child(confirm_btn)

	cancel.pressed.connect(overlay.queue_free)
	confirm_btn.pressed.connect(func() -> void:
		on_confirm.call()
		overlay.queue_free()
	)
	# Clicking the dim backdrop cancels — the panel consumes its own clicks.
	overlay.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			overlay.queue_free()
	)


## Name prompt for saving a selection as a playlist.
## on_confirm.call(name: String) — called with a non-empty name.
static func prompt_name(context: Node, track_count: int, on_confirm: Callable) -> void:
	var theme = ThemeManager.current_theme
	var overlay := Panel.new()
	var dim := StyleBoxFlat.new()
	dim.bg_color = Color(0, 0, 0, 0.45)
	dim.set_corner_radius_all(theme.RADIUS_LG)
	overlay.add_theme_stylebox_override("panel", dim)
	var host: Control = context.get_tree().current_scene.get_node_or_null("AppWindowFrame")
	if host == null:
		host = context.get_tree().current_scene
	host.add_child(overlay)
	overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var center := CenterContainer.new()
	overlay.add_child(center)
	center.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD, 0.92))
	center.add_child(panel)

	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", theme.SPACE_6)
	margin.add_theme_constant_override("margin_right", theme.SPACE_6)
	margin.add_theme_constant_override("margin_top", theme.SPACE_5)
	margin.add_theme_constant_override("margin_bottom", theme.SPACE_5)
	panel.add_child(margin)

	var vbox := VBoxContainer.new()
	vbox.add_theme_constant_override("separation", theme.SPACE_4)
	margin.add_child(vbox)

	var title := Label.new()
	title.text = "Save Playlist"
	title.add_theme_font_override("font", theme.font_display)
	title.add_theme_font_size_override("font_size", theme.TYPE_MD)
	title.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	vbox.add_child(title)

	var body := Label.new()
	body.text = "%d tracks selected" % track_count
	body.add_theme_font_override("font", theme.font_ui)
	body.add_theme_font_size_override("font_size", theme.TYPE_SM)
	body.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	vbox.add_child(body)

	var name_edit := LineEdit.new()
	name_edit.placeholder_text = "Playlist name"
	name_edit.custom_minimum_size.x = 260
	name_edit.custom_minimum_size.y = ThemeManager.min_touch_height(32)
	name_edit.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.3))
	name_edit.add_theme_stylebox_override("focus", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
	name_edit.add_theme_font_override("font", theme.font_ui)
	name_edit.add_theme_font_size_override("font_size", theme.TYPE_SM)
	name_edit.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	name_edit.add_theme_color_override("font_placeholder_color", theme.TEXT_TERTIARY)
	vbox.add_child(name_edit)

	var buttons := HBoxContainer.new()
	buttons.alignment = BoxContainer.ALIGNMENT_END
	buttons.add_theme_constant_override("separation", theme.SPACE_2)
	vbox.add_child(buttons)

	var cancel := Button.new()
	cancel.text = "  Cancel  "
	cancel.custom_minimum_size.y = ThemeManager.min_touch_height(32)
	cancel.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	cancel.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	cancel.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	cancel.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	cancel.add_theme_font_override("font", theme.font_ui)
	cancel.add_theme_font_size_override("font_size", theme.TYPE_SM)
	cancel.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	buttons.add_child(cancel)

	var save_btn := Button.new()
	save_btn.text = "  Save  "
	save_btn.custom_minimum_size.y = ThemeManager.min_touch_height(32)
	save_btn.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
	save_btn.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	save_btn.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	save_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	save_btn.add_theme_font_override("font", theme.font_ui)
	save_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	save_btn.add_theme_color_override("font_color", theme.ACCENT_BRIGHT)
	save_btn.add_theme_color_override("font_hover_color", theme.TEXT_INVERSE)
	buttons.add_child(save_btn)

	var finish := func() -> void:
		var clean: String = name_edit.text.strip_edges()
		if clean.is_empty():
			clean = "New Playlist"
		on_confirm.call(clean)
		overlay.queue_free()

	cancel.pressed.connect(overlay.queue_free)
	save_btn.pressed.connect(finish)
	name_edit.text_submitted.connect(func(_t: String) -> void: finish.call())
	overlay.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			overlay.queue_free()
	)
	name_edit.grab_focus.call_deferred()

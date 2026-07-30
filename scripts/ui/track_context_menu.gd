## track_context_menu.gd
## Right-click / long-press entry point for a track row: a short glass action
## list positioned at the trigger point, same non-dimming contextual style as
## add_to_picker.gd (DESIGN_LANGUAGE §14.6a) — it is not itself a decision,
## just a menu that opens one. Replaces jumping straight to AddToPicker so
## "Edit Metadata" has a place to live without a second gesture.
extends RefCounted

const TrackIndex = preload("res://scripts/services/track_index.gd")
const AddToPicker = preload("res://scripts/ui/add_to_picker.gd")
const EditMetadataModal = preload("res://scripts/ui/edit_metadata_modal.gd")


static func show_for_track(context: Node, href: String, at_position: Vector2) -> void:
	var theme = ThemeManager.current_theme

	var backdrop := Control.new()
	context.get_tree().current_scene.add_child(backdrop)
	backdrop.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	backdrop.mouse_filter = Control.MOUSE_FILTER_STOP

	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD, 0.95))
	panel.custom_minimum_size = Vector2(190, 0)
	backdrop.add_child(panel)

	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", theme.SPACE_2)
	margin.add_theme_constant_override("margin_right", theme.SPACE_2)
	margin.add_theme_constant_override("margin_top", theme.SPACE_2)
	margin.add_theme_constant_override("margin_bottom", theme.SPACE_2)
	panel.add_child(margin)

	var vbox := VBoxContainer.new()
	margin.add_child(vbox)

	var add_btn := _make_action_row(theme, "Add to Playlist...")
	vbox.add_child(add_btn)
	var edit_btn := _make_action_row(theme, "Edit Metadata...")
	vbox.add_child(edit_btn)

	add_btn.pressed.connect(func() -> void:
		backdrop.queue_free()
		AddToPicker.show_for_track(context, href, at_position)
	)
	edit_btn.pressed.connect(func() -> void:
		backdrop.queue_free()
		# Re-derive from the cache rather than trusting a row snapshot from
		# whenever this track's row was last rendered — background
		# enrichment can have filled in fields since then, and prefilling
		# stale blanks would make a correct value look wrong to fix.
		EditMetadataModal.show_for_track(context, href, TrackIndex.new().build_row(href))
	)

	backdrop.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			if not panel.get_global_rect().has_point(event.global_position):
				backdrop.queue_free()
	)

	# Position near the trigger point, clamped inside the viewport — needs a
	# layout pass first so panel.size is real, not the pre-layout zero-rect
	# (same reasoning as add_to_picker.gd's _show).
	await context.get_tree().process_frame
	var vp_size := context.get_viewport().get_visible_rect().size
	var pos := at_position
	pos.x = clamp(pos.x, 8.0, maxf(8.0, vp_size.x - panel.size.x - 8.0))
	pos.y = clamp(pos.y, 8.0, maxf(8.0, vp_size.y - panel.size.y - 8.0))
	panel.global_position = pos


static func _make_action_row(theme, label_text: String) -> Button:
	var row := Button.new()
	row.text = "  " + label_text
	row.alignment = HORIZONTAL_ALIGNMENT_LEFT
	row.custom_minimum_size.y = ThemeManager.min_touch_height(28)
	row.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	row.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	row.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	row.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	row.add_theme_font_override("font", theme.font_ui)
	row.add_theme_font_size_override("font_size", theme.TYPE_SM)
	row.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	return row

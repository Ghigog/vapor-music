## edit_metadata_modal.gd
## "Edit Metadata" modal reached from a track's right-click context menu
## (track_context_menu.gd). A real GlassModal-style decision (§14.4 — dims
## inside the frame, centered), not a contextual picker: it needs a form and
## an explicit Save, unlike add_to_picker.gd's instant-toggle rows.
extends RefCounted

const MANUAL_ARTIST_LABEL := "Unknown Artist"
const MANUAL_ALBUM_LABEL := "Unknown Album"


## `row` is a TrackIndex row (see track_index.gd) — used only to prefill the
## form; the save path writes straight to MetadataService's cache.
static func show_for_track(context: Node, href: String, row: Dictionary) -> void:
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
	vbox.add_theme_constant_override("separation", theme.SPACE_3)
	margin.add_child(vbox)

	var title := Label.new()
	title.text = "Edit Metadata"
	title.add_theme_font_override("font", theme.font_display)
	title.add_theme_font_size_override("font_size", theme.TYPE_MD)
	title.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	vbox.add_child(title)

	var filename := Label.new()
	filename.text = href.get_file().uri_decode()
	filename.clip_text = true
	filename.custom_minimum_size.x = 280
	filename.add_theme_font_override("font", theme.font_ui)
	filename.add_theme_font_size_override("font_size", theme.TYPE_SM)
	filename.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	vbox.add_child(filename)

	# Sentinel values ("Unknown Artist"/"Unknown Album") never render as
	# literal text (§14.7) — a still-unresolved field starts blank so the
	# user types a real value rather than deleting a fake one.
	var artist_prefill: String = row.get("artist", "") if row.get("artist_source", "unknown") != "unknown" else ""
	var album_prefill: String = row.get("album", "") if row.get("album_source", "unknown") != "unknown" else ""
	var bpm_val: float = row.get("bpm", 0.0)
	var bpm_prefill: String = ("%.1f" % bpm_val) if bpm_val > 0.0 else ""

	var title_edit := _make_field(vbox, theme, "Title", row.get("title", ""))
	var artist_edit := _make_field(vbox, theme, "Artist", artist_prefill)
	var album_edit := _make_field(vbox, theme, "Album", album_prefill)
	var genre_edit := _make_field(vbox, theme, "Genre", row.get("genre", ""))
	var bpm_edit := _make_field(vbox, theme, "BPM", bpm_prefill)
	var key_edit := _make_field(vbox, theme, "Key", row.get("key", ""))

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
		# A cleared Artist/Album field means "I don't know" — write the same
		# sentinel the rest of the app already treats as unknown, not an
		# empty string (which get_cached_metadata/parse_track_info wouldn't
		# recognize as an explicit reset).
		var artist_text := artist_edit.text.strip_edges()
		var album_text := album_edit.text.strip_edges()
		var genre_text := genre_edit.text.strip_edges()
		var fields := {
			"artist_name": artist_text if not artist_text.is_empty() else MANUAL_ARTIST_LABEL,
			"album_name": album_text if not album_text.is_empty() else MANUAL_ALBUM_LABEL,
			"genre": genre_text if not genre_text.is_empty() else "Unknown",
			"bpm": bpm_edit.text.strip_edges().to_float(),
			"musical_key": key_edit.text.strip_edges(),
		}
		var title_text := title_edit.text.strip_edges()
		if not title_text.is_empty():
			fields["track_title"] = title_text
		MetadataService.set_manual_metadata(href, fields)
		overlay.queue_free()

	cancel.pressed.connect(overlay.queue_free)
	save_btn.pressed.connect(finish)
	title_edit.text_submitted.connect(func(_t: String) -> void: finish.call())
	overlay.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			if not panel.get_global_rect().has_point(event.global_position):
				overlay.queue_free()
	)
	title_edit.grab_focus.call_deferred()


## Builds one "Label caption + LineEdit" field and appends it to `vbox`.
static func _make_field(vbox: VBoxContainer, theme, label_text: String, value: String) -> LineEdit:
	var caption := Label.new()
	caption.text = label_text
	caption.add_theme_font_override("font", theme.font_ui)
	caption.add_theme_font_size_override("font_size", theme.TYPE_SM)
	caption.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	vbox.add_child(caption)

	var edit := LineEdit.new()
	edit.text = value
	edit.custom_minimum_size.x = 280
	edit.custom_minimum_size.y = ThemeManager.min_touch_height(32)
	edit.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.3))
	edit.add_theme_stylebox_override("focus", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
	edit.add_theme_font_override("font", theme.font_ui)
	edit.add_theme_font_size_override("font_size", theme.TYPE_SM)
	edit.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	edit.add_theme_color_override("font_placeholder_color", theme.TEXT_TERTIARY)
	vbox.add_child(edit)
	return edit

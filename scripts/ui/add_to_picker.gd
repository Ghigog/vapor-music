## add_to_picker.gd
## Right-click (desktop) / long-press (mobile & touch) equivalent of dragging a
## track onto a playlist or a library group header onto a Dynamic Group.
##
## A CONTEXTUAL picker, not a modal decision (contrast GlassModal, §14.4): no
## screen dim, dismisses on any outside click, positions itself near the
## trigger point rather than centering. Rows toggle membership instantly on
## tap — same "no confirm needed" pattern as the entity browser and selection
## checkboxes — and a "+ New…" row creates a fresh target and adds to it in
## one step.
extends RefCounted


## A rebuild-on-interaction list needs a row's own click handler to trigger
## another rebuild — a real bound method on an object, not a Callable that
## closes over itself. A lambda that calls itself via a captured variable
## reads that variable's value AT THE LAMBDA'S OWN CREATION TIME, which for a
## self-reference is captured before the assignment completes — it silently
## resolves to an empty Callable rather than erroring, so a row's toggle
## commits but the follow-up rebuild does nothing. Nested lambdas one level
## deeper than that made no difference. A method call has no such moment —
## `self` is always fully valid — so this controller object owns the rebuild
## instead of a captured closure.
class _Picker:
	extends RefCounted
	var list_vbox: VBoxContainer
	var theme
	var list_items: Callable
	var is_checked: Callable
	var toggle_item: Callable

	func rebuild() -> void:
		for child in list_vbox.get_children():
			child.queue_free()
		for item: Dictionary in list_items.call():
			var row := Button.new()
			row.alignment = HORIZONTAL_ALIGNMENT_LEFT
			row.custom_minimum_size.y = ThemeManager.min_touch_height(28)
			row.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
			row.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
			row.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
			row.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
			row.add_theme_font_override("font", theme.font_ui)
			row.add_theme_font_size_override("font_size", theme.TYPE_SM)
			row.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
			# ASCII "+"/"✓" only — a fancier glyph tried here earlier fell back
			# to a font without it and rendered as a tofu box (§14.2).
			row.text = "  %s  %s" % ["✓" if is_checked.call(item) else "+", item.name]
			row.pressed.connect(_on_row_pressed.bind(item))
			list_vbox.add_child(row)

	func _on_row_pressed(item: Dictionary) -> void:
		toggle_item.call(item)
		rebuild()


static func show_for_track(context: Node, href: String, at_position: Vector2) -> void:
	var list_items := func() -> Array: return PlaylistService.get_playlists()
	var is_checked := func(item: Dictionary) -> bool:
		return (PlaylistService.get_playlist(item.id).get("tracks", []) as Array).has(href)
	var toggle_item := func(item: Dictionary) -> void:
		var tracks: Array = PlaylistService.get_playlist(item.id).get("tracks", [])
		if tracks.has(href):
			PlaylistService.remove_track_from_playlist(item.id, tracks.find(href))
		else:
			PlaylistService.add_track_to_playlist(item.id, href)
	var create_new := func(name: String) -> void:
		var pl: Dictionary = PlaylistService.create_playlist(name)
		PlaylistService.add_track_to_playlist(pl.id, href)
	_show(context, at_position, "Add to Playlist", "New Playlist...",
		list_items, is_checked, toggle_item, create_new)


## Bulk variant for the table's multi-select "Add to..." action — one or more
## tracks at once, into an EXISTING playlist. Deliberately not a checkbox
## toggle like show_for_track: "is every one of these N tracks already in
## this playlist" is a 3-state condition, not boolean, so is_checked always
## reports false (every row always offers "+", never a misleading "✓") and
## toggle_item only ever adds (add_tracks_to_playlist itself dedups, so
## tapping the same playlist twice is a harmless no-op, not a duplicate).
## The picker stays open after each tap (same as show_for_track) so the
## selection can be added to more than one playlist in one sitting.
static func show_for_tracks(context: Node, hrefs: Array, at_position: Vector2) -> void:
	if hrefs.is_empty():
		return
	var list_items := func() -> Array: return PlaylistService.get_playlists()
	var is_checked := func(_item: Dictionary) -> bool: return false
	var toggle_item := func(item: Dictionary) -> void:
		PlaylistService.add_tracks_to_playlist(item.id, hrefs)
	var create_new := func(name: String) -> void:
		var pl: Dictionary = PlaylistService.create_playlist(name)
		PlaylistService.add_tracks_to_playlist(pl.id, hrefs)
	_show(context, at_position, "Add %d Tracks to Playlist" % hrefs.size(), "New Playlist...",
		list_items, is_checked, toggle_item, create_new)


static func show_for_entity(context: Node, entity_type: String, value: String, at_position: Vector2) -> void:
	var list_items := func() -> Array: return DynamicGroupService.get_dynamic_groups()
	var is_checked := func(item: Dictionary) -> bool:
		return DynamicGroupService.has_entity(item.id, entity_type, value)
	var toggle_item := func(item: Dictionary) -> void:
		if DynamicGroupService.has_entity(item.id, entity_type, value):
			DynamicGroupService.remove_entity(item.id, entity_type, value)
		else:
			DynamicGroupService.add_entity(item.id, entity_type, value)
	var create_new := func(name: String) -> void:
		var g: Dictionary = DynamicGroupService.create_group(name)
		DynamicGroupService.add_entity(g.id, entity_type, value)
	_show(context, at_position, "Add to Group", "New Group...",
		list_items, is_checked, toggle_item, create_new)


## Generic picker builder shared by both entry points above — the structure
## (title, checkable list, create-new row) is identical; only the data source
## and the toggle/create actions differ.
static func _show(context: Node, at_position: Vector2, title_text: String, create_label: String,
		list_items: Callable, is_checked: Callable, toggle_item: Callable, create_new: Callable) -> void:
	var theme = ThemeManager.current_theme

	# Plain, undecorated click-catcher — mirrors playlist_popup.tscn's own
	# Backdrop. A context menu doesn't dim the app; a modal decision does.
	var backdrop := Control.new()
	context.get_tree().current_scene.add_child(backdrop)
	backdrop.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	backdrop.mouse_filter = Control.MOUSE_FILTER_STOP

	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD, 0.95))
	panel.custom_minimum_size = Vector2(220, 0)
	backdrop.add_child(panel)

	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", theme.SPACE_3)
	margin.add_theme_constant_override("margin_right", theme.SPACE_3)
	margin.add_theme_constant_override("margin_top", theme.SPACE_3)
	margin.add_theme_constant_override("margin_bottom", theme.SPACE_3)
	panel.add_child(margin)

	var vbox := VBoxContainer.new()
	vbox.add_theme_constant_override("separation", theme.SPACE_2)
	margin.add_child(vbox)

	var title := Label.new()
	title.text = title_text
	title.add_theme_font_override("font", theme.font_display)
	title.add_theme_font_size_override("font_size", theme.TYPE_SM)
	title.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	vbox.add_child(title)

	var list_scroll := ScrollContainer.new()
	list_scroll.custom_minimum_size.y = 0
	vbox.add_child(list_scroll)
	var list_vbox := VBoxContainer.new()
	list_vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	list_scroll.add_child(list_vbox)

	var picker := _Picker.new()
	picker.list_vbox = list_vbox
	picker.theme = theme
	picker.list_items = list_items
	picker.is_checked = is_checked
	picker.toggle_item = toggle_item
	# Keep the controller alive for the picker's lifetime — row buttons hold
	# bound-method Callables into it, but nothing else references the
	# RefCounted itself, so it would otherwise free the moment this static
	# function returns.
	backdrop.set_meta("picker_controller", picker)
	picker.rebuild()

	var create_btn := Button.new()
	create_btn.text = "  +  " + create_label
	create_btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	create_btn.custom_minimum_size.y = ThemeManager.min_touch_height(28)
	create_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	create_btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	create_btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	create_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	create_btn.add_theme_font_override("font", theme.font_ui)
	create_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	create_btn.add_theme_color_override("font_color", theme.ACCENT_CORE)
	vbox.add_child(create_btn)

	var name_edit := LineEdit.new()
	name_edit.placeholder_text = create_label
	name_edit.visible = false
	name_edit.custom_minimum_size.y = ThemeManager.min_touch_height(28)
	name_edit.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.3))
	name_edit.add_theme_stylebox_override("focus", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
	name_edit.add_theme_font_override("font", theme.font_ui)
	name_edit.add_theme_font_size_override("font_size", theme.TYPE_SM)
	name_edit.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	vbox.add_child(name_edit)

	create_btn.pressed.connect(func() -> void:
		create_btn.visible = false
		name_edit.visible = true
		name_edit.grab_focus.call_deferred()
	)
	var commit_create := func(text: String) -> void:
		var clean := text.strip_edges()
		if not clean.is_empty():
			create_new.call(clean)
		backdrop.queue_free()
	name_edit.text_submitted.connect(commit_create)

	backdrop.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			if not panel.get_global_rect().has_point(event.global_position):
				backdrop.queue_free()
	)

	# Position near the trigger point, clamped inside the viewport — needs a
	# layout pass first so panel.size is real, not the pre-layout zero-rect.
	await context.get_tree().process_frame
	var vp_size := context.get_viewport().get_visible_rect().size
	var pos := at_position
	pos.x = clamp(pos.x, 8.0, maxf(8.0, vp_size.x - panel.size.x - 8.0))
	pos.y = clamp(pos.y, 8.0, maxf(8.0, vp_size.y - panel.size.y - 8.0))
	panel.global_position = pos

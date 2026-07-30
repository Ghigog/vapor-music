## dynamic_group_screen.gd
## A Dynamic Group is a collection of ENTITIES (artists/albums/genres), never
## tracks — opening one shows the entities as cards, never a track list
## directly. Clicking a card drills into a TrackTable of whatever currently
## matches that entity, grouped by the entity's own column. Membership is
## always resolved live against the full library; nothing here is frozen.
extends Control

const TrackIndex = preload("res://scripts/services/track_index.gd")
const TrackTable = preload("res://scripts/ui/track_table.gd")
const GlassModal = preload("res://scripts/ui/glass_modal.gd")

## Entity types this screen understands, in the order they appear in the
## "+ Add" browser's tabs. Matches TrackIndex.GROUP_* so a card's type can be
## handed straight to the group_btn on drill-in.
const ENTITY_TYPES := [TrackIndex.GROUP_ARTIST, TrackIndex.GROUP_ALBUM, TrackIndex.GROUP_GENRE]
const ENTITY_LABELS := {"artist": "Artist", "album": "Album", "genre": "Genre"}
const CARD_ART_SIZE := 36

## A rebuild-on-interaction list needs a row's own click handler to trigger
## another rebuild — a real bound method on an object, not a Callable that
## closes over itself (see add_to_picker.gd's _Picker for why that silently
## breaks: a self-referencing lambda captures its own variable's value at the
## lambda's OWN creation time, before the assignment completes). A method call
## has no such moment — `self` is always fully valid.
class _EntityBrowser:
	extends RefCounted
	var list_vbox: VBoxContainer
	var search_edit: LineEdit
	var index_rows: Array
	var active_type: String = ""

	func rebuild() -> void:
		for child in list_vbox.get_children():
			child.queue_free()
		var query := search_edit.text.strip_edges().to_lower()
		for entry: Dictionary in TrackIndex.distinct_values(index_rows, active_type):
			var value: String = entry.value
			if not query.is_empty() and not query in value.to_lower():
				continue
			var row_btn := Button.new()
			row_btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
			var in_group := DynamicGroupService.has_entity(DynamicGroupService.active_group_id, active_type, value)
			# Plain ASCII "+" — the fullwidth "＋" isn't in the app font and
			# rendered as a fallback tofu glyph (§14.2, yet again).
			row_btn.text = "%s  %s  (%d)" % ["✓" if in_group else "+", value, entry.count]
			row_btn.custom_minimum_size.y = ThemeManager.min_touch_height(32)
			row_btn.pressed.connect(_on_row_pressed.bind(value))
			list_vbox.add_child(row_btn)

	func _on_row_pressed(value: String) -> void:
		var group_id := DynamicGroupService.active_group_id
		if DynamicGroupService.has_entity(group_id, active_type, value):
			DynamicGroupService.remove_entity(group_id, active_type, value)
		else:
			DynamicGroupService.add_entity(group_id, active_type, value)
		rebuild()

@onready var layout: VBoxContainer = %Layout

## Library-wide index, rebuilt on scan — a group's cards need live counts and
## its drill-in view needs live matches against the WHOLE library, not just
## this group's own (nonexistent) track list.
var _index := TrackIndex.new()

var _compact_header: HBoxContainer
var _compact_title: LineEdit
var _toolbar: HBoxContainer
var _cards_scroll: ScrollContainer
var _cards_flow: HFlowContainer
var _empty_state: VBoxContainer
var _back_btn: Button
var _table: VBoxContainer

var _in_drill := false


func _ready() -> void:
	visible = (NavManager.current_screen == "dynamic_group")
	NavManager.navigation_requested.connect(_on_navigation_requested)
	ThemeManager.theme_changed.connect(_apply_styles)

	if not WebDAVService.library_scanned.is_connected(_on_library_scanned):
		WebDAVService.library_scanned.connect(_on_library_scanned)
	if not WebDAVService.scanned_files.is_empty():
		_index.build(WebDAVService.scanned_files)

	_build_ui()
	PlatformManager.layout_changed.connect(func(_bp: String) -> void: _update_compact_header())

	if DynamicGroupService:
		DynamicGroupService.group_entities_updated.connect(func(id: String) -> void:
			if visible and id == DynamicGroupService.active_group_id:
				# An entity vanishing mid-drill would leave the table pointed at
				# nothing meaningful — bounce back to the honest state: the
				# cards plus the combined tracklist, both freshly rebuilt either
				# way (adding/removing an entity changes the union too).
				_show_cards()
		)
		DynamicGroupService.group_renamed.connect(func(id: String, new_name: String) -> void:
			if id == DynamicGroupService.active_group_id and _compact_title:
				_compact_title.text = new_name
		)
		DynamicGroupService.active_group_changed.connect(func(_id): _refresh())

	_apply_styles()
	_refresh()


func _on_library_scanned(files: Array) -> void:
	_index.build(files)
	if visible:
		_refresh()


func _on_navigation_requested(screen_name: String) -> void:
	visible = (screen_name == "dynamic_group")
	if visible:
		_refresh()


# ---------------------------------------------------------------------------
# UI construction
# ---------------------------------------------------------------------------

func _build_ui() -> void:
	var theme = ThemeManager.current_theme
	var header_margin := MarginContainer.new()
	header_margin.add_theme_constant_override("margin_left", theme.SPACE_3)
	header_margin.add_theme_constant_override("margin_right", theme.SPACE_3)
	header_margin.add_theme_constant_override("margin_top", theme.SPACE_3)
	layout.add_child(header_margin)
	_compact_header = HBoxContainer.new()
	header_margin.add_child(_compact_header)

	_compact_title = LineEdit.new()
	_compact_title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_compact_title.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	_compact_title.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	_compact_header.add_child(_compact_title)
	var commit_title := func() -> void:
		var clean: String = _compact_title.text.strip_edges()
		if not clean.is_empty() and not DynamicGroupService.active_group_id.is_empty():
			DynamicGroupService.rename_group(DynamicGroupService.active_group_id, clean)
	_compact_title.text_submitted.connect(func(_t: String) -> void: commit_title.call())
	_compact_title.focus_exited.connect(commit_title)

	var del_btn := Button.new()
	del_btn.text = "✕"
	del_btn.flat = true
	del_btn.tooltip_text = "Delete group"
	del_btn.custom_minimum_size = ThemeManager.min_touch_size(Vector2(32, 32))
	_compact_header.add_child(del_btn)
	del_btn.pressed.connect(func() -> void:
		var g: Dictionary = DynamicGroupService.get_group(DynamicGroupService.active_group_id)
		if g.is_empty():
			return
		GlassModal.confirm(self, "Delete Dynamic Group",
			"Delete group \"%s\"? This can't be undone." % g.name,
			"Delete",
			func() -> void:
				NavManager.navigate_to("library")
				DynamicGroupService.delete_group(g.id)
		)
	)
	del_btn.set_meta("is_delete_btn", true)

	var toolbar_margin := MarginContainer.new()
	toolbar_margin.add_theme_constant_override("margin_left", theme.SPACE_3)
	toolbar_margin.add_theme_constant_override("margin_right", theme.SPACE_3)
	toolbar_margin.add_theme_constant_override("margin_top", theme.SPACE_2)
	toolbar_margin.add_theme_constant_override("margin_bottom", theme.SPACE_1)
	layout.add_child(toolbar_margin)
	_toolbar = HBoxContainer.new()
	_toolbar.add_theme_constant_override("separation", theme.SPACE_2)
	toolbar_margin.add_child(_toolbar)

	_back_btn = Button.new()
	_back_btn.text = "‹  Cards"
	_back_btn.visible = false
	_back_btn.pressed.connect(_show_cards)
	_toolbar.add_child(_back_btn)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_toolbar.add_child(spacer)

	var add_btn := Button.new()
	add_btn.text = "+  Add"
	add_btn.tooltip_text = "Add artists, albums, or genres to this group"
	add_btn.pressed.connect(_show_entity_browser)
	_toolbar.add_child(add_btn)

	_cards_scroll = ScrollContainer.new()
	# Cards are a compact strip, not the main content — the table below is
	# the actual point of the screen (the group's live combined playlist), so
	# cards get a capped height (scrolling internally past a couple of wrapped
	# rows) and the table gets the space that's left, not the other way round.
	_cards_scroll.custom_minimum_size.y = 120
	_cards_scroll.size_flags_vertical = Control.SIZE_FILL
	_cards_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	layout.add_child(_cards_scroll)

	var cards_margin := MarginContainer.new()
	cards_margin.add_theme_constant_override("margin_left", theme.SPACE_3)
	cards_margin.add_theme_constant_override("margin_right", theme.SPACE_3)
	cards_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_cards_scroll.add_child(cards_margin)

	_cards_flow = HFlowContainer.new()
	_cards_flow.add_theme_constant_override("h_separation", theme.SPACE_3)
	_cards_flow.add_theme_constant_override("v_separation", theme.SPACE_3)
	cards_margin.add_child(_cards_flow)

	_empty_state = VBoxContainer.new()
	_empty_state.alignment = BoxContainer.ALIGNMENT_CENTER
	_empty_state.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_empty_state.visible = false
	layout.add_child(_empty_state)

	var empty_label := Label.new()
	empty_label.text = "No entities yet"
	empty_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	empty_label.name = "EmptyHeading"
	_empty_state.add_child(empty_label)

	var empty_body := Label.new()
	empty_body.text = "Drag an artist, album, or genre from the library here — or use \"+ Add\"."
	empty_body.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	empty_body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	empty_body.custom_minimum_size.x = 260
	empty_body.name = "EmptyBody"
	_empty_state.add_child(empty_body)

	_table = TrackTable.new()
	_table.manual_mode = false
	_table.visible = false
	layout.add_child(_table)
	# No separate "scope" call needed: AudioManager.play_track(href, queue) sets
	# current_playlist = queue, and EVERY vibe/smart-mix feature (auto-transition
	# matching, the Vibe screen's runner-up cards, harmonic shuffle) reads
	# current_playlist directly with no fallback to the wider library — so
	# passing this table's own view (one entity's current matches) as the queue
	# is the entire mechanism. Verified in DESIGN_LANGUAGE.md / memory notes.
	_table.play_requested.connect(func(row: Dictionary, queue: Array) -> void:
		AudioManager.play_track(row.href, queue)
		if is_instance_valid(MetadataService):
			MetadataService.focus_track(row.href, row.artist, row.album, row.title)
	)

	# Screen-level drop target: dragging a library group header anywhere on
	# this screen while viewing a group's cards adds it — a slightly larger
	# target than the sidebar row, useful once you're already here.
	set_process_unhandled_input(false)

	_update_compact_header()


func _update_compact_header() -> void:
	_compact_header.visible = not PlatformManager.should_show_sidebar()


# ---------------------------------------------------------------------------
# Drag-and-drop: accept a library entity dropped on the open group
# ---------------------------------------------------------------------------

func _can_drop_data(_at_position: Vector2, data: Variant) -> bool:
	return not _in_drill and data is Dictionary and data.get("type") == "entity"

func _drop_data(_at_position: Vector2, data: Variant) -> void:
	var entity_type = data.get("entity_type", "")
	var value = data.get("value", "")
	if not entity_type.is_empty() and not value.is_empty() and not DynamicGroupService.active_group_id.is_empty():
		DynamicGroupService.add_entity(DynamicGroupService.active_group_id, entity_type, value)


# ---------------------------------------------------------------------------
# Cards view
# ---------------------------------------------------------------------------

func _refresh() -> void:
	if not visible:
		return
	var id := DynamicGroupService.active_group_id
	if id.is_empty():
		return
	var group: Dictionary = DynamicGroupService.get_group(id)
	if group.is_empty():
		return
	_compact_title.text = group.name
	_show_cards()


func _refresh_cards() -> void:
	var group: Dictionary = DynamicGroupService.get_group(DynamicGroupService.active_group_id)
	var entities: Array = group.get("entities", [])

	for child in _cards_flow.get_children():
		child.queue_free()

	_empty_state.visible = entities.is_empty()
	_cards_scroll.visible = not entities.is_empty()

	for e: Dictionary in entities:
		_cards_flow.add_child(_make_card(e.get("type", ""), e.get("value", "")))


## The default view: cards for managing entities, plus — the actual point of
## a dynamic group — the live combined tracklist of everything they currently
## match. This is what makes it a playable "dynamic playlist of multiple
## genres/albums/artists" rather than just a bookmark list of entities.
func _show_cards() -> void:
	_in_drill = false
	_back_btn.visible = false
	_refresh_cards()
	_refresh_union_table()


## Rebuilds the table to the UNION of every entity's current matches — the
## same live-resolved-against-the-whole-library approach as a single card's
## drill-in (TrackIndex.matches_any_entity), just not narrowed to one entity.
func _refresh_union_table() -> void:
	var group: Dictionary = DynamicGroupService.get_group(DynamicGroupService.active_group_id)
	var entities: Array = group.get("entities", [])
	if entities.is_empty():
		_table.visible = false
		return

	var rows: Array = []
	for row in _index.rows:
		if TrackIndex.matches_any_entity(row, entities):
			rows.append(row)

	_table.visible = true
	_table.set_rows(rows)


func _count_matches(entity_type: String, value: String) -> int:
	var n := 0
	for row in _index.rows:
		if TrackIndex.matches_entity(row, entity_type, value):
			n += 1
	return n


func _make_card(entity_type: String, value: String) -> Control:
	var theme = ThemeManager.current_theme
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(148, 96)
	panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD, 0.35))

	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", theme.SPACE_3)
	margin.add_theme_constant_override("margin_right", theme.SPACE_3)
	margin.add_theme_constant_override("margin_top", theme.SPACE_2)
	margin.add_theme_constant_override("margin_bottom", theme.SPACE_2)
	panel.add_child(margin)

	var vbox := VBoxContainer.new()
	vbox.add_theme_constant_override("separation", theme.SPACE_1)

	# Artist/album cards get a circle/rounded-square art slot beside the
	# labels; genre has no natural image, so it stays text-only.
	if entity_type == "artist" or entity_type == "album":
		var art_row := HBoxContainer.new()
		art_row.add_theme_constant_override("separation", theme.SPACE_2)
		margin.add_child(art_row)

		var art := TextureRect.new()
		art.custom_minimum_size = Vector2(CARD_ART_SIZE, CARD_ART_SIZE)
		art.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		art.stretch_mode = TextureRect.STRETCH_SCALE
		art.mouse_filter = Control.MOUSE_FILTER_IGNORE
		if entity_type == "artist":
			ThumbnailService.apply_circle_mask(art)
		else:
			ThumbnailService.apply_rounded_mask(art, theme.RADIUS_XS)
		art_row.add_child(art)

		var art_path := MetadataService.get_entity_image_path(entity_type, value)
		if not art_path.is_empty():
			ThumbnailService.request(art_path, func(tex: Texture2D) -> void:
				if is_instance_valid(art):
					art.texture = tex
			, 96)

		art_row.add_child(vbox)
	else:
		margin.add_child(vbox)

	# Type badge.
	var badge := Label.new()
	badge.text = ENTITY_LABELS.get(entity_type, entity_type)
	badge.add_theme_font_override("font", theme.font_ui)
	badge.add_theme_font_size_override("font_size", theme.TYPE_XS)
	badge.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	vbox.add_child(badge)

	var name_lbl := Label.new()
	name_lbl.text = value
	name_lbl.clip_text = true
	name_lbl.add_theme_font_override("font", theme.font_display)
	name_lbl.add_theme_font_size_override("font_size", theme.TYPE_MD)
	name_lbl.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	vbox.add_child(name_lbl)

	var count_lbl := Label.new()
	var n := _count_matches(entity_type, value)
	count_lbl.text = "%d track%s" % [n, "" if n == 1 else "s"]
	count_lbl.add_theme_font_override("font", theme.font_ui)
	count_lbl.add_theme_font_size_override("font_size", theme.TYPE_SM)
	count_lbl.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	vbox.add_child(count_lbl)

	var hit := Button.new()
	hit.flat = true
	hit.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	hit.add_theme_stylebox_override("hover", ThemeManager.make_transparent())
	hit.add_theme_stylebox_override("pressed", ThemeManager.make_transparent())
	hit.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	panel.add_child(hit)
	hit.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	hit.pressed.connect(func() -> void: _drill_into(entity_type, value))

	# PanelContainer (like any Container) re-fits every DIRECT child to its
	# own full content rect on each layout pass — it ignores a child's own
	# anchors/offsets entirely. remove_btn's corner positioning only held
	# while it started invisible (Containers skip hidden children when
	# fitting); the instant hover set it visible, the next sort pass
	# stretched it to nearly the whole card, which is why the ✕ never
	# appeared where intended and the click/hover target ended up covering
	# almost the entire card instead of a small corner.
	#
	# Fix: give remove_btn a plain Control parent instead — Control (unlike
	# Container) never resizes its children, so anchors/offsets set on
	# remove_btn are respected. Same pattern as sidebar.gd's preview-slot
	# overlay (_setup_preview_slot) for a hover chip over an AspectRatioContainer.
	var overlay := Control.new()
	overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	panel.add_child(overlay)
	overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var remove_btn := Button.new()
	remove_btn.text = "✕"
	remove_btn.flat = true
	remove_btn.tooltip_text = "Remove from this group"
	remove_btn.visible = PlatformManager.is_touch_primary()
	remove_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	remove_btn.add_theme_stylebox_override("hover", ThemeManager.make_transparent())
	remove_btn.add_theme_stylebox_override("pressed", ThemeManager.make_transparent())
	remove_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	remove_btn.add_theme_font_override("font", theme.font_ui)
	remove_btn.add_theme_font_size_override("font_size", theme.TYPE_XS)
	remove_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	remove_btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
	overlay.add_child(remove_btn)
	remove_btn.custom_minimum_size = ThemeManager.min_touch_size(Vector2(24, 24))
	remove_btn.anchor_left = 1.0
	remove_btn.anchor_right = 1.0
	remove_btn.anchor_top = 0.0
	remove_btn.anchor_bottom = 0.0
	remove_btn.offset_left = -28.0
	remove_btn.offset_right = -4.0
	remove_btn.offset_top = 4.0
	remove_btn.offset_bottom = 28.0
	remove_btn.pressed.connect(func() -> void:
		DynamicGroupService.remove_entity(DynamicGroupService.active_group_id, entity_type, value)
	)

	if not PlatformManager.is_touch_primary():
		# `hit` is a full-rect Button covering the entire card, so it — not
		# `panel` — is what actually receives hover: panel.mouse_entered/exited
		# only fired in the razor-thin sliver where hit's geometry didn't
		# perfectly coincide with panel's. Listening on hit instead covers
		# the whole card. The exit guard mirrors sidebar.gd's del-button
		# pattern: moving onto remove_btn itself (drawn on top of hit in the
		# corner) also fires hit's mouse_exited, so only hide once the
		# pointer has actually left panel's bounds, not just hit's.
		hit.mouse_entered.connect(func() -> void: remove_btn.visible = true)
		hit.mouse_exited.connect(func() -> void:
			if not Rect2(Vector2.ZERO, panel.size).has_point(panel.get_local_mouse_position()):
				remove_btn.visible = false
		)

	return panel


# ---------------------------------------------------------------------------
# Drill-in: a TrackTable of one entity's current matches
# ---------------------------------------------------------------------------

func _drill_into(entity_type: String, value: String) -> void:
	_in_drill = true
	_back_btn.visible = true
	_cards_scroll.visible = false
	_empty_state.visible = false

	var rows: Array = []
	for row in _index.rows:
		if TrackIndex.matches_entity(row, entity_type, value):
			rows.append(row)

	_table.visible = true
	_table.set_rows(rows)
	# "Grouped by their given column": an artist card groups by album, an
	# album or genre card groups by artist — whichever the card ISN'T already
	# telling you, same rule the library table uses for its second column.
	if entity_type == TrackIndex.GROUP_ARTIST:
		_table._group_btn.select(1) # Albums
		_table._group_btn.item_selected.emit(1)
	else:
		_table._group_btn.select(0) # Artists
		_table._group_btn.item_selected.emit(0)


# ---------------------------------------------------------------------------
# Entity browser ("+ Add") — search + type tabs, instant toggle add/remove
# ---------------------------------------------------------------------------

func _show_entity_browser() -> void:
	var theme = ThemeManager.current_theme
	var overlay := Panel.new()
	var dim := StyleBoxFlat.new()
	dim.bg_color = Color(0, 0, 0, 0.45)
	dim.set_corner_radius_all(theme.RADIUS_LG)
	overlay.add_theme_stylebox_override("panel", dim)
	var host: Control = get_tree().current_scene.get_node_or_null("AppWindowFrame")
	if host == null:
		host = get_tree().current_scene
	host.add_child(overlay)
	overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var center := CenterContainer.new()
	overlay.add_child(center)
	center.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD, 0.94))
	panel.custom_minimum_size = Vector2(340, 420)
	center.add_child(panel)

	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", theme.SPACE_5)
	margin.add_theme_constant_override("margin_right", theme.SPACE_5)
	margin.add_theme_constant_override("margin_top", theme.SPACE_5)
	margin.add_theme_constant_override("margin_bottom", theme.SPACE_5)
	panel.add_child(margin)

	var vbox := VBoxContainer.new()
	vbox.add_theme_constant_override("separation", theme.SPACE_3)
	margin.add_child(vbox)

	var header := HBoxContainer.new()
	vbox.add_child(header)
	var title := Label.new()
	title.text = "Add to Group"
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title.add_theme_font_override("font", theme.font_display)
	title.add_theme_font_size_override("font_size", theme.TYPE_MD)
	title.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	header.add_child(title)
	var close_btn := Button.new()
	close_btn.text = "  Done  "
	header.add_child(close_btn)
	close_btn.pressed.connect(overlay.queue_free)

	var tabs := HBoxContainer.new()
	vbox.add_child(tabs)
	var search_edit := LineEdit.new()
	search_edit.placeholder_text = "Search"
	vbox.add_child(search_edit)

	var list_scroll := ScrollContainer.new()
	list_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	list_scroll.custom_minimum_size.y = 260
	vbox.add_child(list_scroll)
	var list_vbox := VBoxContainer.new()
	list_vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	list_scroll.add_child(list_vbox)

	var tab_buttons: Dictionary = {}
	var browser := _EntityBrowser.new()
	browser.list_vbox = list_vbox
	browser.search_edit = search_edit
	browser.index_rows = _index.rows
	browser.active_type = ENTITY_TYPES[0]
	# Keep the controller alive for the overlay's lifetime — row/tab buttons
	# hold bound-method Callables into it, but nothing else references this
	# RefCounted, so it would otherwise free the moment this function returns.
	overlay.set_meta("entity_browser_controller", browser)

	for t: String in ENTITY_TYPES:
		var tab_btn := Button.new()
		tab_btn.text = ENTITY_LABELS.get(t, t)
		tab_btn.toggle_mode = true
		tab_btn.button_pressed = (t == browser.active_type)
		tab_buttons[t] = tab_btn
		tabs.add_child(tab_btn)
		tab_btn.pressed.connect(func() -> void:
			browser.active_type = t
			for other_t in tab_buttons:
				tab_buttons[other_t].button_pressed = (other_t == browser.active_type)
			browser.rebuild()
		)

	search_edit.text_changed.connect(func(_t: String) -> void: browser.rebuild())

	overlay.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			overlay.queue_free()
	)

	browser.rebuild()


# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_styles() -> void:
	if not is_inside_tree():
		return
	var theme = ThemeManager.current_theme

	_compact_title.add_theme_font_override("font", theme.font_display)
	_compact_title.add_theme_font_size_override("font_size", theme.TYPE_MD)
	_compact_title.add_theme_color_override("font_color", theme.TEXT_PRIMARY)

	for child in _compact_header.get_children():
		if child.has_meta("is_delete_btn"):
			child.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
			child.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
			child.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
			child.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
			child.add_theme_font_override("font", theme.font_ui)
			child.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
			child.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)

	for btn: Button in [_back_btn]:
		btn.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.3))
		btn.add_theme_stylebox_override("hover", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
		btn.add_theme_stylebox_override("pressed", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
		btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		btn.add_theme_font_override("font", theme.font_ui)
		btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
		btn.add_theme_color_override("font_color", theme.TEXT_SECONDARY)

	for child in _toolbar.get_children():
		if child is Button and child != _back_btn:
			child.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.3))
			child.add_theme_stylebox_override("hover", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
			child.add_theme_stylebox_override("pressed", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
			child.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
			child.add_theme_font_override("font", theme.font_ui)
			child.add_theme_font_size_override("font_size", theme.TYPE_SM)
			child.add_theme_color_override("font_color", theme.TEXT_SECONDARY)

	for label in _empty_state.get_children():
		if label.name == "EmptyHeading":
			label.add_theme_font_override("font", theme.font_display)
			label.add_theme_font_size_override("font_size", theme.TYPE_MD)
			label.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
		elif label.name == "EmptyBody":
			label.add_theme_font_override("font", theme.font_ui)
			label.add_theme_font_size_override("font_size", theme.TYPE_SM)
			label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)

## track_table.gd
## The one track table: toolbar (search / group / sort / columns), column
## header, and the rendered row list. Both the library and the playlist screen
## host this control — a playlist IS this table in manual mode, which is how
## "playlists are views over the library" stays true in code.
##
## Pure view: hosts feed it row dictionaries (TrackIndex schema, plus
## "manual_pos" in manual mode) and react to its signals. It never talks to
## PlaylistService or AudioManager itself.
##
## Honesty rules for cells: verified metadata renders in the normal secondary
## color, values guessed from filenames/folders render dimmer, and genuinely
## unknown fields render as "—" — never as literal "Unknown Artist/Album".
extends VBoxContainer

const TrackIndex = preload("res://scripts/services/track_index.gd")
const TRACK_DRAG_BUTTON = preload("res://scripts/screens/track_drag_button.gd")
const GROUP_HEADER_DRAG_BUTTON = preload("res://scripts/ui/group_header_drag_button.gd")
const AddToPicker = preload("res://scripts/ui/add_to_picker.gd")

## Emitted with the full row and the current view order as the play queue.
signal play_requested(row: Dictionary, queue: Array)
## Manual mode only — indices are positions in the playlist's track list.
signal remove_requested(manual_index: int)
signal reorder_requested(from_index: int, to_index: int)
signal insert_requested(href: String, at_index: int)
## Emitted when an artist/album group is expanded, for sidebar imagery.
signal artist_focused(artist_name: String)
signal album_focused(album_name: String)
## Emitted when the user confirms a selection (✓) — the checked hrefs in the
## current sort order. The host names and creates the playlist.
signal save_selection_requested(hrefs: Array)

## Flat and search modes build real nodes per row, so they are capped until
## the list is virtualized. Grouped modes stay lazy per group.
const FLAT_CAP := 500

## Set before adding to the tree. Manual mode: no grouping, "Order" sort
## first, rows removable and reorderable, drops insert at position.
var manual_mode := false

## Set before adding to the tree to show the "Save view" toolbar button
## (library only — a playlist is already a saved view).
var show_save_view := false

## Set before adding to the tree to persist sort/collapse/group across runs
## under this key in user://view_state.json. Empty = session-only state.
var state_key := ""

const VIEW_STATE_PATH := "user://view_state.json"

var _rows: Array = []
var _view_hrefs: Array = []
var _flat_shown := FLAT_CAP
var _group_keys: Array = []
var _search_timer: Timer

## Header-driven view state. Sorting: click the arrow beside a column title;
## click again to flip direction (manual mode adds a third step back to
## "order"). Collapsing: click the column title itself; a thin stub remains
## and clicking it expands the column again.
var _sort_key := "title"
var _sort_asc := true
var _collapsed: Dictionary = {}

## Selection mode ("Save playlist"): every row gets a checkbox, pre-ticked
## for what was on screen when it started — expansion is the selection
## gesture. Row clicks toggle instead of play; ✓ confirms, ✕ abandons.
var _selecting := false
var _checked: Dictionary = {}
var _row_checks: Dictionary = {}
const CHECK_W := 18

## Which group headers are expanded, keyed by header text — survives _render()
## rebuilds (entering/exiting selection mode re-renders the whole list; without
## this every group snapped shut the instant "Save playlist" was pressed,
## hiding the very selection the user just made).
var _expanded_groups: Dictionary = {}

## Width of a collapsed column's stub in the header and its spacer in rows.
const COLLAPSED_W := 18

## All sortable fields, for the narrow-layout sort control. Wide layouts sort
## from the column header; narrow layouts hide the header, so they get a
## compact toolbar chip instead (§14.7 — every affordance needs a narrow/touch
## equivalent).
const SORT_FIELDS := ["title", "artist", "album", "genre", "year", "bpm", "key"]

## Every styled label built so far, as {label, role} — theme changes restyle
## in place instead of rebuilding (which would discard expansion state).
var _styled_cells: Array[Dictionary] = []

## href → {cells: {col_id: Label}} for visible rows, so metadata enrichment
## during playback updates cells in place without a re-render.
var _live_cells: Dictionary = {}

var _toolbar_margin: MarginContainer
var _search_edit: LineEdit
var _group_btn: OptionButton
var _save_btn: Button
var _confirm_btn: Button
var _cancel_btn: Button
var _mobile_sort_btn: OptionButton
var _mobile_dir_btn: Button
var _col_header_margin: MarginContainer
var _col_header: HBoxContainer
var _scroll: ScrollContainer
var _list: VBoxContainer
var _drop_indicator: ColorRect


func _ready() -> void:
	size_flags_vertical = Control.SIZE_EXPAND_FILL
	add_theme_constant_override("separation", 4)
	if manual_mode:
		_sort_key = "order"
	_build_ui()
	_load_view_state()
	_apply_styles()
	_update_layout_mode()
	ThemeManager.theme_changed.connect(_apply_styles)
	# Layout is WIDTH-driven (a narrowed desktop window gets the stacked rows,
	# a landscape phone gets columns) — re-render on breakpoint crossings.
	PlatformManager.layout_changed.connect(func(_bp: String) -> void:
		_update_layout_mode()
		_render()
	)


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

func set_rows(rows: Array) -> void:
	_rows = rows
	_flat_shown = FLAT_CAP
	_render()


## Updates one row's data and any visible cells in place. Preserves
## manual_pos, and never re-renders — a re-render mid-browse would destroy
## the user's group-expansion state.
func refresh_row(fresh: Dictionary) -> void:
	for i in _rows.size():
		if _rows[i].href == fresh.href:
			if _rows[i].has("manual_pos") and not fresh.has("manual_pos"):
				fresh.manual_pos = _rows[i].manual_pos
			_rows[i] = fresh
			break
	if not _live_cells.has(fresh.href):
		return
	var cells: Dictionary = _live_cells[fresh.href].cells
	for col_id: String in cells:
		var lbl: Label = cells[col_id]
		if not is_instance_valid(lbl):
			continue
		if col_id == "sub":
			lbl.text = _mobile_sub_text(fresh)
		else:
			lbl.text = _cell_text(fresh, col_id)
			var role := _cell_role(fresh, col_id)
			lbl.set_meta("role", role)
			lbl.add_theme_color_override("font_color", _style_for_role(role).color)


# ---------------------------------------------------------------------------
# UI construction
# ---------------------------------------------------------------------------

func _build_ui() -> void:
	_toolbar_margin = MarginContainer.new()
	add_child(_toolbar_margin)
	var toolbar := HFlowContainer.new()
	_toolbar_margin.add_child(toolbar)

	_search_edit = LineEdit.new()
	_search_edit.placeholder_text = "Search"
	_search_edit.clear_button_enabled = true
	_search_edit.custom_minimum_size.x = 180
	_search_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	toolbar.add_child(_search_edit)

	_group_btn = OptionButton.new()
	if manual_mode:
		_group_btn.visible = false
		_group_keys = [TrackIndex.GROUP_NONE]
	else:
		_group_btn.add_item("Artists")
		_group_btn.add_item("Albums")
		_group_btn.add_item("Genres")
		_group_btn.add_item("All tracks")
		_group_keys = [TrackIndex.GROUP_ARTIST, TrackIndex.GROUP_ALBUM, TrackIndex.GROUP_GENRE, TrackIndex.GROUP_NONE]
		_group_btn.item_selected.connect(func(_i: int) -> void:
			_persist_view_state()
			_reset_and_render()
		)
	toolbar.add_child(_group_btn)

	if show_save_view:
		_save_btn = Button.new()
		_save_btn.text = "Save playlist"
		_save_btn.tooltip_text = "Pick tracks for a new playlist — what's on screen starts ticked"
		_save_btn.pressed.connect(_enter_selection)
		toolbar.add_child(_save_btn)

		_confirm_btn = Button.new()
		_confirm_btn.text = "  ✓  "
		_confirm_btn.tooltip_text = "Save the ticked tracks as a playlist"
		_confirm_btn.visible = false
		_confirm_btn.pressed.connect(_confirm_selection)
		toolbar.add_child(_confirm_btn)

		_cancel_btn = Button.new()
		_cancel_btn.text = "  ✕  "
		_cancel_btn.tooltip_text = "Cancel the selection"
		_cancel_btn.visible = false
		_cancel_btn.pressed.connect(_exit_selection)
		toolbar.add_child(_cancel_btn)

	# Sorting and column visibility live in the column header itself — the
	# arrow beside a title sorts, the title click collapses. No menus on wide
	# layouts. Narrow layouts hide the header, so they get this compact sort
	# chip + direction toggle instead (visibility managed in
	# _update_layout_mode).
	_mobile_sort_btn = OptionButton.new()
	for field: String in _sort_field_list():
		_mobile_sort_btn.add_item(field.capitalize())
	_mobile_sort_btn.item_selected.connect(func(i: int) -> void:
		_sort_key = _sort_field_list()[i]
		_persist_view_state()
		_reset_and_render()
	)
	toolbar.add_child(_mobile_sort_btn)

	_mobile_dir_btn = Button.new()
	_mobile_dir_btn.text = "▲"
	_mobile_dir_btn.tooltip_text = "Toggle sort direction"
	_mobile_dir_btn.pressed.connect(func() -> void:
		_sort_asc = not _sort_asc
		_mobile_dir_btn.text = "▲" if _sort_asc else "▼"
		_persist_view_state()
		_reset_and_render()
	)
	toolbar.add_child(_mobile_dir_btn)

	# Debounce so a fast typist re-filters once, not per keystroke.
	_search_timer = Timer.new()
	_search_timer.wait_time = 0.25
	_search_timer.one_shot = true
	add_child(_search_timer)
	_search_timer.timeout.connect(_reset_and_render)
	_search_edit.text_changed.connect(func(_t: String) -> void: _search_timer.start())

	_col_header_margin = MarginContainer.new()
	add_child(_col_header_margin)
	_col_header = HBoxContainer.new()
	_col_header.add_theme_constant_override("separation", 10)
	_col_header_margin.add_child(_col_header)

	_scroll = ScrollContainer.new()
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	add_child(_scroll)
	_list = VBoxContainer.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_list.add_theme_constant_override("separation", 2)
	_scroll.add_child(_list)

	_drop_indicator = ColorRect.new()
	_drop_indicator.custom_minimum_size.y = 3
	_drop_indicator.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_drop_indicator.visible = false
	_list.add_child(_drop_indicator)


func _reset_and_render() -> void:
	_flat_shown = FLAT_CAP
	_render()


func _sort_field_list() -> Array:
	return (["order"] if manual_mode else []) + SORT_FIELDS


# ---------------------------------------------------------------------------
# Selection mode (Save playlist)
# ---------------------------------------------------------------------------

func _enter_selection() -> void:
	_selecting = true
	_seed_checked()
	_update_selection_buttons()
	_render()


func _exit_selection() -> void:
	_selecting = false
	_checked.clear()
	_update_selection_buttons()
	_render()


func _confirm_selection() -> void:
	# Checked hrefs in the current sort order — the selection can span
	# collapsed groups and searches, so order comes from the full index.
	var rows: Array[Dictionary] = []
	rows.assign(_rows)
	TrackIndex.sort_rows(rows, _sort_key, _sort_asc)
	var out: Array = []
	for row in rows:
		if _checked.has(row.href):
			out.append(row.href)
	if not out.is_empty():
		save_selection_requested.emit(out)
	_exit_selection()


## Pre-ticks what the user can currently see: in flat/search mode every match,
## in grouped mode only the tracks of groups they have expanded — expansion is
## the selection gesture. Reads _expanded_groups (not the live tree) so it
## gives the right answer regardless of what has actually been built.
func _seed_checked() -> void:
	_checked.clear()
	var query := _search_edit.text.strip_edges()
	var group_key: String = _group_keys[_group_btn.selected] if not manual_mode else TrackIndex.GROUP_NONE
	if not query.is_empty():
		group_key = TrackIndex.GROUP_NONE

	if group_key == TrackIndex.GROUP_NONE:
		for href in _view_hrefs:
			_checked[href] = true
		return

	var rows: Array[Dictionary] = []
	rows.assign(_filtered(query))
	TrackIndex.sort_rows(rows, _sort_key, _sort_asc)
	for group: Dictionary in TrackIndex.grouped(rows, group_key):
		if _expanded_groups.get(group.header, false):
			for row: Dictionary in group.rows:
				_checked[row.href] = true


func _update_selection_buttons() -> void:
	if _save_btn == null:
		return
	_save_btn.visible = not _selecting
	_confirm_btn.visible = _selecting
	_cancel_btn.visible = _selecting


func _toggle_checked(href: String) -> void:
	if _checked.has(href):
		_checked.erase(href)
	else:
		_checked[href] = true
	var chk = _row_checks.get(href)
	if chk and is_instance_valid(chk):
		_apply_check_visual(chk, _checked.has(href))


## Font-independent checkbox: an accent-filled rounded square when ticked, a
## thin outline when not — no glyphs, no baseline to drift (§14.2).
func _make_check_box(row_href: String) -> Button:
	var chk := Button.new()
	chk.custom_minimum_size = Vector2(CHECK_W, CHECK_W)
	chk.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	chk.focus_mode = Control.FOCUS_NONE
	_apply_check_visual(chk, _checked.has(row_href))
	chk.pressed.connect(func() -> void: _toggle_checked(row_href))
	_row_checks[row_href] = chk
	return chk


func _apply_check_visual(chk: Button, on: bool) -> void:
	var theme = ThemeManager.current_theme
	var sb := StyleBoxFlat.new()
	sb.set_corner_radius_all(theme.RADIUS_XS)
	sb.set_border_width_all(1)
	if on:
		sb.bg_color = theme.ACCENT_CORE
		sb.border_color = theme.ACCENT_BRIGHT
	else:
		sb.bg_color = Color(1, 1, 1, 0.06)
		sb.border_color = theme.TEXT_TERTIARY
	for state: String in ["normal", "hover", "pressed"]:
		chk.add_theme_stylebox_override(state, sb)
	chk.add_theme_stylebox_override("focus", ThemeManager.make_transparent())


# ---------------------------------------------------------------------------
# View-state persistence (sort / direction / collapsed columns / grouping)
# ---------------------------------------------------------------------------

func _load_view_state() -> void:
	if state_key.is_empty() or not FileAccess.file_exists(VIEW_STATE_PATH):
		return
	var file := FileAccess.open(VIEW_STATE_PATH, FileAccess.READ)
	if file == null:
		return
	var parsed = JSON.parse_string(file.get_as_text())
	file.close()
	if not (parsed is Dictionary and parsed.has(state_key)):
		return
	var s: Dictionary = parsed[state_key]
	var key: String = s.get("sort_key", _sort_key)
	if key in _sort_field_list():
		_sort_key = key
	_sort_asc = s.get("sort_asc", _sort_asc)
	_collapsed.clear()
	for col_id in s.get("collapsed", []):
		_collapsed[col_id] = true
	if not manual_mode:
		var group_idx: int = s.get("group", _group_btn.selected)
		if group_idx >= 0 and group_idx < _group_btn.item_count:
			_group_btn.select(group_idx)


func _persist_view_state() -> void:
	if state_key.is_empty():
		return
	var all: Dictionary = {}
	if FileAccess.file_exists(VIEW_STATE_PATH):
		var rf := FileAccess.open(VIEW_STATE_PATH, FileAccess.READ)
		if rf:
			var parsed = JSON.parse_string(rf.get_as_text())
			rf.close()
			if parsed is Dictionary:
				all = parsed
	all[state_key] = {
		"sort_key": _sort_key,
		"sort_asc": _sort_asc,
		"collapsed": _collapsed.keys(),
		"group": _group_btn.selected if not manual_mode else 0,
	}
	var wf := FileAccess.open(VIEW_STATE_PATH, FileAccess.WRITE)
	if wf:
		wf.store_string(JSON.stringify(all, "\t"))
		wf.close()


## Shows/hides the narrow-layout sort controls and syncs them to the current
## sort state (header clicks and the chip drive the same _sort_key/_sort_asc).
func _update_layout_mode() -> void:
	var narrow := PlatformManager.is_mobile_layout()
	_mobile_sort_btn.visible = narrow
	_mobile_dir_btn.visible = narrow
	if narrow:
		var idx: int = _sort_field_list().find(_sort_key)
		if idx >= 0:
			_mobile_sort_btn.select(idx)
		_mobile_dir_btn.text = "▲" if _sort_asc else "▼"


# ---------------------------------------------------------------------------
# Styling
# ---------------------------------------------------------------------------

func _apply_styles() -> void:
	var theme = ThemeManager.current_theme

	_toolbar_margin.add_theme_constant_override("margin_left", theme.SPACE_3)
	_toolbar_margin.add_theme_constant_override("margin_right", theme.SPACE_3)
	_toolbar_margin.add_theme_constant_override("margin_top", theme.SPACE_3)
	_toolbar_margin.add_theme_constant_override("margin_bottom", theme.SPACE_1)
	_col_header_margin.add_theme_constant_override("margin_left", theme.SPACE_3)
	_col_header_margin.add_theme_constant_override("margin_right", theme.SPACE_3)

	var control_height := ThemeManager.min_touch_height(int(theme.TOUCH_TARGET_MIN * 0.7))

	_search_edit.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.3))
	_search_edit.add_theme_stylebox_override("focus", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
	_search_edit.add_theme_font_override("font", theme.font_ui)
	_search_edit.add_theme_font_size_override("font_size", theme.TYPE_SM)
	_search_edit.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	_search_edit.add_theme_color_override("font_placeholder_color", theme.TEXT_TERTIARY)
	_search_edit.custom_minimum_size.y = control_height

	var toolbar_buttons: Array = [_group_btn, _mobile_sort_btn, _mobile_dir_btn]
	if _save_btn:
		toolbar_buttons.append(_save_btn)
		toolbar_buttons.append(_cancel_btn)
	if _confirm_btn:
		_confirm_btn.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
		_confirm_btn.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
		_confirm_btn.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
		_confirm_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		_confirm_btn.add_theme_font_override("font", theme.font_ui)
		_confirm_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
		_confirm_btn.add_theme_color_override("font_color", theme.ACCENT_BRIGHT)
		_confirm_btn.add_theme_color_override("font_hover_color", theme.TEXT_INVERSE)
		_confirm_btn.custom_minimum_size.y = control_height
	for btn: Button in toolbar_buttons:
		btn.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.3))
		btn.add_theme_stylebox_override("hover", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
		btn.add_theme_stylebox_override("pressed", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.45))
		btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		btn.add_theme_font_override("font", theme.font_ui)
		btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
		btn.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
		btn.add_theme_color_override("font_hover_color", theme.TEXT_PRIMARY)
		btn.custom_minimum_size.y = control_height

	_restyle_cells()


func _style_for_role(role: String) -> Dictionary:
	var theme = ThemeManager.current_theme
	match role:
		"header":
			return {"font": theme.font_display, "size": theme.TYPE_MD, "color": theme.TEXT_PRIMARY}
		"title":
			return {"font": theme.font_ui, "size": theme.TYPE_SM, "color": theme.TEXT_PRIMARY}
		"meta":
			return {"font": theme.font_ui, "size": theme.TYPE_SM, "color": theme.TEXT_SECONDARY}
		"guess", "unknown":
			return {"font": theme.font_ui, "size": theme.TYPE_SM, "color": theme.TEXT_TERTIARY}
		_:
			return {"font": theme.font_ui, "size": theme.TYPE_XS, "color": theme.TEXT_TERTIARY}


func _register_cell(lbl: Label, role: String) -> void:
	lbl.set_meta("role", role)
	var style := _style_for_role(role)
	lbl.add_theme_font_override("font", style.font)
	lbl.add_theme_font_size_override("font_size", style.size)
	lbl.add_theme_color_override("font_color", style.color)
	_styled_cells.append({"label": lbl, "role": role})


func _restyle_cells() -> void:
	var live: Array[Dictionary] = []
	for entry: Dictionary in _styled_cells:
		var lbl: Label = entry.label
		if not is_instance_valid(lbl):
			continue
		var style := _style_for_role(lbl.get_meta("role", entry.role))
		lbl.add_theme_font_override("font", style.font)
		lbl.add_theme_font_size_override("font_size", style.size)
		lbl.add_theme_color_override("font_color", style.color)
		live.append(entry)
	_styled_cells = live


# ---------------------------------------------------------------------------
# Pipeline render
# ---------------------------------------------------------------------------

func _render() -> void:
	for child in _list.get_children():
		if child != _drop_indicator:
			child.queue_free()
	_styled_cells.clear()
	_live_cells.clear()
	_row_checks.clear()

	var query := _search_edit.text.strip_edges()
	var group_key: String = _group_keys[_group_btn.selected] if not manual_mode else TrackIndex.GROUP_NONE
	# An active search flattens to matches — searching into collapsed groups
	# would hide the results it just found.
	if not query.is_empty():
		group_key = TrackIndex.GROUP_NONE

	var rows: Array[Dictionary] = []
	rows.assign(_filtered(query))
	TrackIndex.sort_rows(rows, _sort_key, _sort_asc)

	_view_hrefs = []
	for row in rows:
		_view_hrefs.append(row.href)

	_build_col_header(group_key, rows.is_empty())

	if rows.is_empty():
		var margin := MarginContainer.new()
		margin.add_theme_constant_override("margin_left", ThemeManager.current_theme.SPACE_3)
		margin.add_theme_constant_override("margin_top", ThemeManager.current_theme.SPACE_2)
		var lbl := Label.new()
		lbl.text = "No tracks match \"%s\"." % query if not query.is_empty() else "No tracks."
		_register_cell(lbl, "meta")
		margin.add_child(lbl)
		_list.add_child(margin)
		return

	if group_key == TrackIndex.GROUP_NONE:
		var shown: int = mini(rows.size(), _flat_shown)
		for i in range(shown):
			_list.add_child(_make_track_row(rows[i], group_key))
		if rows.size() > shown:
			_list.add_child(_make_show_more_row(rows.size() - shown))
	else:
		for group: Dictionary in TrackIndex.grouped(rows, group_key):
			_add_group(group.header, group.rows, group_key)


func _filtered(query: String) -> Array:
	var q := query.to_lower()
	if q.is_empty():
		return _rows.duplicate()
	var out: Array = []
	for row in _rows:
		if q in (row.title as String).to_lower() \
				or q in (row.artist as String).to_lower() \
				or q in (row.album as String).to_lower() \
				or q in (row.genre as String).to_lower():
			out.append(row)
	return out


## Column spec shared by the header row and every track row so they stay
## aligned. The second column shows whichever of artist/album the grouping
## doesn't already display; the genre column drops out when grouping by genre.
## Collapsed columns stay in the list flagged, so header stubs and row spacers
## keep the remaining columns aligned.
func _columns(group_key: String) -> Array[Dictionary]:
	var context_field := "album" if group_key == TrackIndex.GROUP_ARTIST else "artist"
	var cols: Array[Dictionary] = [
		{"id": "title", "label": "Title", "ratio": 3.0},
		{"id": context_field, "label": "Album" if context_field == "album" else "Artist", "ratio": 2.0},
	]
	if group_key != TrackIndex.GROUP_GENRE:
		cols.append({"id": "genre", "label": "Genre", "ratio": 1.2})
	cols.append({"id": "year", "label": "Year", "width": 48})
	cols.append({"id": "bpm", "label": "BPM", "width": 56})
	cols.append({"id": "key", "label": "Key", "width": 44})

	for col in cols:
		col["collapsed"] = _collapsed.has(col.id)
	return cols


func _apply_col_sizing(lbl: Label, col: Dictionary) -> void:
	lbl.clip_text = true
	if col.has("width"):
		lbl.custom_minimum_size.x = col.width
		lbl.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	else:
		lbl.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		lbl.size_flags_stretch_ratio = col.ratio


func _build_col_header(group_key: String, empty: bool) -> void:
	for child in _col_header.get_children():
		child.queue_free()
	_col_header_margin.visible = not empty and not PlatformManager.is_mobile_layout()
	if not _col_header_margin.visible:
		return
	if _selecting:
		var gutter := Control.new()
		gutter.custom_minimum_size.x = CHECK_W
		gutter.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_col_header.add_child(gutter)
	for col: Dictionary in _columns(group_key):
		_col_header.add_child(_make_header_cell(col))
	if manual_mode:
		_col_header.add_child(_make_row_gutter())


func _style_header_button(btn: Button, active: bool = false) -> void:
	var theme = ThemeManager.current_theme
	btn.flat = true
	btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("hover", ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("pressed", ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	btn.add_theme_font_override("font", theme.font_ui)
	btn.add_theme_font_size_override("font_size", theme.TYPE_XS)
	btn.add_theme_color_override("font_color", theme.ACCENT_CORE if active else theme.TEXT_TERTIARY)
	btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT if active else theme.TEXT_SECONDARY)


## One interactive header cell: the title collapses the column, the arrow
## beside it sorts. A collapsed column is a thin stub that expands on click.
func _make_header_cell(col: Dictionary) -> Control:
	var cell := HBoxContainer.new()
	cell.add_theme_constant_override("separation", 0)

	if col.get("collapsed", false):
		var stub := Button.new()
		stub.text = (col.label as String).left(1)
		stub.tooltip_text = "Show %s" % col.label
		_style_header_button(stub)
		cell.custom_minimum_size.x = COLLAPSED_W
		stub.pressed.connect(func() -> void:
			_collapsed.erase(col.id)
			_persist_view_state()
			_render()
		)
		cell.add_child(stub)
		return cell

	if col.has("width"):
		cell.custom_minimum_size.x = col.width
		cell.alignment = BoxContainer.ALIGNMENT_END
	else:
		cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		cell.size_flags_stretch_ratio = col.ratio

	var title_btn := Button.new()
	title_btn.text = col.label
	# Natural text width — no clip_text (a clipped Button reports ZERO minimum
	# width, §14.2 button edition). The arrow sits right beside the title; a
	# trailing spacer absorbs the rest of ratio columns.
	_style_header_button(title_btn)
	if col.id == "title":
		# Title is never collapsible — plain label behavior, no hover.
		title_btn.mouse_filter = Control.MOUSE_FILTER_IGNORE
	else:
		title_btn.tooltip_text = "Hide %s" % col.label
		title_btn.pressed.connect(func() -> void:
			_collapsed[col.id] = true
			_persist_view_state()
			_render()
		)
	cell.add_child(title_btn)

	var active: bool = _sort_key == col.id
	var arrow := Button.new()
	arrow.tooltip_text = "Sort by %s" % col.label
	_style_header_button(arrow, active)
	if active:
		arrow.text = "▲" if _sort_asc else "▼"
	else:
		# In-font stacked mini-triangles. The previous "↕" glyph is not in the
		# app font — the OS fallback renders it on a higher baseline, visibly
		# misaligned beside its title (DESIGN_LANGUAGE §14.2: glyphs come from
		# the app font or get composed from characters that do).
		arrow.custom_minimum_size.x = 16.0
		var stack := VBoxContainer.new()
		stack.mouse_filter = Control.MOUSE_FILTER_IGNORE
		stack.alignment = BoxContainer.ALIGNMENT_CENTER
		stack.add_theme_constant_override("separation", -1)
		arrow.add_child(stack)
		stack.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		for glyph: String in ["▲", "▼"]:
			var mini := Label.new()
			mini.text = glyph
			mini.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
			mini.mouse_filter = Control.MOUSE_FILTER_IGNORE
			mini.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
			mini.add_theme_font_size_override("font_size", 7)
			mini.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
			stack.add_child(mini)
	arrow.pressed.connect(func() -> void:
		_on_sort_arrow_pressed(col.id)
	)
	cell.add_child(arrow)

	if not col.has("width"):
		var spacer := Control.new()
		spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
		cell.add_child(spacer)
	return cell


## Sort cycle: inactive → ascending → descending → (manual mode only) back to
## the playlist's own Order; library flips between ascending and descending.
func _on_sort_arrow_pressed(field: String) -> void:
	if _sort_key == field:
		if _sort_asc:
			_sort_asc = false
		elif manual_mode:
			_sort_key = "order"
			_sort_asc = true
		else:
			_sort_asc = true
	else:
		_sort_key = field
		_sort_asc = true
	_persist_view_state()
	_reset_and_render()


func _add_group(header: String, rows: Array, group_key: String) -> void:
	var section := VBoxContainer.new()
	section.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_list.add_child(section)

	var header_row := _make_header_row(header, rows.size(), group_key)
	section.add_child(header_row.container)

	var contents := VBoxContainer.new()
	contents.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# Expansion state lives in _expanded_groups (survives rebuilds), not on
	# this node — a fresh _render() recreates every section from scratch.
	contents.set_meta("built", false)
	section.add_child(contents)

	var start_expanded: bool = _expanded_groups.get(header, false)
	if start_expanded:
		contents.set_meta("built", true)
		for row: Dictionary in rows:
			contents.add_child(_make_track_row(row, group_key))
		header_row.label.text = "▼  " + header
	contents.visible = start_expanded

	header_row.button.pressed.connect(func() -> void:
		# A long press already opened the "Add to Group" picker — the release
		# ending it must not ALSO expand/collapse the section.
		if header_row.button.suppress_next_click:
			header_row.button.suppress_next_click = false
			return
		var expanded: bool = not contents.visible
		if expanded and not contents.get_meta("built"):
			contents.set_meta("built", true)
			for row: Dictionary in rows:
				contents.add_child(_make_track_row(row, group_key))
		contents.visible = expanded
		header_row.label.text = ("▼  " if expanded else "▶  ") + header
		if expanded:
			_expanded_groups[header] = true
		else:
			_expanded_groups.erase(header)
		if header != TrackIndex.UNKNOWN_HEADER:
			if group_key == TrackIndex.GROUP_ARTIST:
				artist_focused.emit(header)
			elif group_key == TrackIndex.GROUP_ALBUM:
				album_focused.emit(header)
	)


# ---------------------------------------------------------------------------
# Row builders
# ---------------------------------------------------------------------------

## Base row: MarginContainer > Stack > transparent full-rect Button (hitbox)
## with visible content overlaid on top, mouse-transparent.
func _row_shell(min_height: int, left_margin: int) -> Dictionary:
	var margin := MarginContainer.new()
	margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	margin.add_theme_constant_override("margin_left", left_margin)
	margin.add_theme_constant_override("margin_right", ThemeManager.current_theme.SPACE_2)
	margin.add_theme_constant_override("margin_top", 2)
	margin.add_theme_constant_override("margin_bottom", 2)

	var stack := Control.new()
	stack.custom_minimum_size.y = ThemeManager.min_touch_height(min_height)
	stack.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	margin.add_child(stack)

	var btn := Button.new()
	btn.flat = true
	btn.add_theme_stylebox_override("normal",   ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("hover",    ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("pressed",  ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("focus",    ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("disabled", ThemeManager.make_transparent())
	stack.add_child(btn)
	btn.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	return {"container": margin, "stack": stack, "button": btn}


func _make_header_row(header: String, count: int, group_key: String = TrackIndex.GROUP_NONE) -> Dictionary:
	var theme = ThemeManager.current_theme
	var shell := _row_shell(28, theme.SPACE_3)

	# Every header row gets the script — it no-ops (empty entity_type) for the
	# unknown bucket and GROUP_NONE's single anonymous section. Where it's a
	# real group, the whole group can be dragged onto a sidebar Dynamic Group
	# to add it whole — e.g. drag "Björk" to add every Björk track as a
	# live-resolving entity — or long-pressed/right-clicked for the same
	# "Add to Group" picker the drag produces.
	shell.button.set_script(GROUP_HEADER_DRAG_BUTTON)
	shell.button._setup_gestures()
	if group_key != TrackIndex.GROUP_NONE and header != TrackIndex.UNKNOWN_HEADER:
		shell.button.entity_type = group_key
		shell.button.entity_value = header
		shell.button.long_pressed.connect(func(pos: Vector2) -> void:
			AddToPicker.show_for_entity(self, group_key, header, pos)
		)
		shell.button.right_clicked.connect(func(pos: Vector2) -> void:
			AddToPicker.show_for_entity(self, group_key, header, pos)
		)

	var hbox := HBoxContainer.new()
	hbox.mouse_filter = Control.MOUSE_FILTER_IGNORE
	hbox.add_theme_constant_override("separation", 10)
	shell.stack.add_child(hbox)
	hbox.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var lbl := Label.new()
	lbl.text = "▶  " + header
	# No clip_text: a clipped label without expand flags reports zero minimum
	# width and vanishes. Headers take their natural width so the count sits
	# beside them; the trailing spacer absorbs the rest.
	lbl.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_register_cell(lbl, "header")
	hbox.add_child(lbl)

	var count_lbl := Label.new()
	count_lbl.text = "%d" % count
	count_lbl.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	count_lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_register_cell(count_lbl, "small")
	hbox.add_child(count_lbl)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	hbox.add_child(spacer)

	return {"container": shell.container, "button": shell.button, "label": lbl}


func _cell_text(row: Dictionary, col_id: String) -> String:
	match col_id:
		"title":
			return row.title
		"artist":
			return "—" if row.artist_source == "unknown" else row.artist
		"album":
			return "—" if row.album_source == "unknown" else row.album
		"genre":
			return row.genre if not (row.genre as String).is_empty() else "—"
		"year":
			return "—" if (row.get("year", 0) as int) <= 0 else str(row.year)
		"bpm":
			return "—" if row.bpm <= 0.0 else str(roundi(row.bpm))
		"key":
			return row.key if not (row.key as String).is_empty() else "—"
	return ""


func _cell_role(row: Dictionary, col_id: String) -> String:
	match col_id:
		"title":
			return "title"
		"artist":
			if row.artist_source == "unknown":
				return "unknown"
			return "meta" if row.artist_source == "cache" else "guess"
		"album":
			if row.album_source == "unknown":
				return "unknown"
			return "meta" if row.album_source == "cache" else "guess"
		"genre":
			return "meta" if not (row.genre as String).is_empty() else "unknown"
		"year":
			# Years come from folder/file names — always a guess.
			return "guess" if (row.get("year", 0) as int) > 0 else "unknown"
		"bpm":
			return "meta" if row.bpm > 0.0 else "unknown"
		"key":
			return "meta" if not (row.key as String).is_empty() else "unknown"
	return "meta"


## Second line of a narrow-mode row: known facts joined with dots, unknowns
## simply omitted — omission is honest, placeholders are noise.
func _mobile_sub_text(row: Dictionary) -> String:
	var parts: Array[String] = []
	if row.artist_source != "unknown":
		parts.append(row.artist)
	if row.bpm > 0.0:
		parts.append("%d BPM" % roundi(row.bpm))
	if not (row.key as String).is_empty():
		parts.append(row.key)
	if parts.is_empty() and row.album_source != "unknown":
		parts.append(row.album)
	return " · ".join(parts)


## Fixed-width spacer matching the remove-button gutter so manual-mode rows
## and the column header stay aligned.
func _make_row_gutter() -> Control:
	var gutter := Control.new()
	gutter.custom_minimum_size.x = 28
	gutter.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return gutter


func _make_track_row(row: Dictionary, group_key: String) -> Control:
	var theme = ThemeManager.current_theme
	# Row shape follows layout WIDTH, not the device badge — a narrowed
	# desktop window stacks, a landscape phone gets columns.
	var narrow := PlatformManager.is_mobile_layout()
	var indent: int = theme.SPACE_6 if group_key != TrackIndex.GROUP_NONE else theme.SPACE_3
	var shell := _row_shell(44 if narrow else 28, indent)

	var btn: Button = shell.button
	btn.set_script(TRACK_DRAG_BUTTON)
	btn._setup_gestures()
	btn.href = row.href
	btn.track_title = row.title
	btn.long_pressed.connect(func(pos: Vector2) -> void:
		AddToPicker.show_for_track(self, row.href, pos)
	)
	btn.right_clicked.connect(func(pos: Vector2) -> void:
		AddToPicker.show_for_track(self, row.href, pos)
	)
	if manual_mode:
		btn.set_meta("drag_extras", {"manual_index": row.get("manual_pos", -1), "table_iid": get_instance_id()})
		btn.set_meta("drop_table", self)
		btn.set_meta("row_container", shell.container)

	shell.container.set_meta("href", row.href)

	var cells: Dictionary = {}
	var title_lbl: Label

	if narrow:
		var outer := HBoxContainer.new()
		outer.mouse_filter = Control.MOUSE_FILTER_IGNORE
		outer.add_theme_constant_override("separation", 8)
		shell.stack.add_child(outer)
		outer.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		if _selecting:
			outer.add_child(_make_check_box(row.href))

		var vbox := VBoxContainer.new()
		vbox.mouse_filter = Control.MOUSE_FILTER_IGNORE
		vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		vbox.add_theme_constant_override("separation", 0)
		vbox.alignment = BoxContainer.ALIGNMENT_CENTER
		outer.add_child(vbox)

		title_lbl = Label.new()
		title_lbl.text = row.title
		title_lbl.clip_text = true
		title_lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_register_cell(title_lbl, "title")
		vbox.add_child(title_lbl)

		var sub := Label.new()
		sub.text = _mobile_sub_text(row)
		sub.clip_text = true
		sub.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_register_cell(sub, "small")
		vbox.add_child(sub)

		cells = {"title": title_lbl, "sub": sub}
	else:
		var hbox := HBoxContainer.new()
		hbox.mouse_filter = Control.MOUSE_FILTER_IGNORE
		hbox.add_theme_constant_override("separation", 10)
		shell.stack.add_child(hbox)
		hbox.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		if _selecting:
			hbox.add_child(_make_check_box(row.href))

		for col: Dictionary in _columns(group_key):
			if col.get("collapsed", false):
				var spacer := Control.new()
				spacer.custom_minimum_size.x = COLLAPSED_W
				spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
				hbox.add_child(spacer)
				continue
			var lbl := Label.new()
			lbl.text = _cell_text(row, col.id)
			lbl.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
			lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
			_apply_col_sizing(lbl, col)
			_register_cell(lbl, _cell_role(row, col.id))
			hbox.add_child(lbl)
			cells[col.id] = lbl
		if manual_mode:
			hbox.add_child(_make_row_gutter())
		title_lbl = cells["title"]

	_live_cells[row.href] = {"cells": cells}

	var remove_btn: Button = null
	if manual_mode:
		remove_btn = Button.new()
		remove_btn.text = "✕"
		remove_btn.flat = true
		# Touch has no hover — the remove button stays visible on touch
		# hardware and is hover-revealed only where a pointer exists (§14.7).
		remove_btn.visible = PlatformManager.is_touch_primary()
		remove_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
		remove_btn.add_theme_stylebox_override("hover", ThemeManager.make_transparent())
		remove_btn.add_theme_stylebox_override("pressed", ThemeManager.make_transparent())
		remove_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		remove_btn.add_theme_font_override("font", theme.font_ui)
		remove_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
		remove_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
		remove_btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
		shell.stack.add_child(remove_btn)
		remove_btn.set_anchors_and_offsets_preset(Control.PRESET_CENTER_RIGHT)
		remove_btn.custom_minimum_size = ThemeManager.min_touch_size(Vector2(24, 24))
		remove_btn.pressed.connect(func() -> void:
			remove_requested.emit(row.get("manual_pos", -1))
		)

	btn.pressed.connect(func() -> void:
		# A long press already opened the "Add to Playlist" picker — the
		# release ending it must not ALSO start playback.
		if btn.suppress_next_click:
			btn.suppress_next_click = false
			return
		if _selecting:
			# In selection mode the whole row is the checkbox's hit target.
			_toggle_checked(row.href)
			return
		play_requested.emit(row, _view_hrefs)
	)
	# Hover reads ThemeManager at call time, so it stays correct across theme
	# changes without reconnecting.
	btn.mouse_entered.connect(func() -> void:
		title_lbl.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
		if remove_btn:
			remove_btn.visible = true
	)
	btn.mouse_exited.connect(func() -> void:
		title_lbl.add_theme_color_override("font_color", _style_for_role("title").color)
		_hide_drop_indicator()
		if remove_btn and not PlatformManager.is_touch_primary():
			# Keep visible while the pointer is over the remove button itself.
			var stack: Control = shell.stack
			if not Rect2(Vector2.ZERO, stack.size).has_point(stack.get_local_mouse_position()):
				remove_btn.visible = false
	)

	return shell.container


func _make_show_more_row(remaining: int) -> Control:
	var theme = ThemeManager.current_theme
	var btn := Button.new()
	btn.text = "Show %d more" % mini(remaining, FLAT_CAP)
	btn.custom_minimum_size.y = ThemeManager.min_touch_height(32)
	btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	btn.add_theme_font_override("font", theme.font_ui)
	btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	btn.add_theme_color_override("font_color", theme.ACCENT_CORE)
	btn.add_theme_color_override("font_hover_color", theme.ACCENT_BRIGHT)
	btn.pressed.connect(func() -> void:
		_flat_shown += FLAT_CAP
		_render()
	)
	return btn


# ---------------------------------------------------------------------------
# Manual-mode drag & drop (called by track_drag_button via "drop_table" meta)
# ---------------------------------------------------------------------------

## Reordering and positional inserts only make sense against the real list:
## manual sort, no search filter.
func _reorder_allowed() -> bool:
	return manual_mode \
		and _sort_key == "order" \
		and _search_edit.text.strip_edges().is_empty()


func can_row_accept_drop(data: Variant) -> bool:
	return _reorder_allowed() and data is Dictionary and data.get("type") == "track" and not (data.get("href", "") as String).is_empty()


func preview_row_drop(row_btn: Button, at_position: Vector2, _data: Variant) -> void:
	var container: Control = row_btn.get_meta("row_container", null)
	if container == null or not is_instance_valid(container):
		return
	_drop_indicator.color = ThemeManager.current_theme.ACCENT_CORE
	_drop_indicator.visible = true
	var above := at_position.y < row_btn.size.y * 0.5
	var idx := container.get_index() + (0 if above else 1)
	_list.move_child(_drop_indicator, clampi(idx, 0, _list.get_child_count() - 1))


func handle_row_drop(row_btn: Button, at_position: Vector2, data: Dictionary) -> void:
	_hide_drop_indicator()
	var extras: Dictionary = row_btn.get_meta("drag_extras", {})
	var target: int = extras.get("manual_index", -1)
	if target < 0:
		return
	var above := at_position.y < row_btn.size.y * 0.5
	var insert_at := target + (0 if above else 1)

	if data.get("table_iid", 0) == get_instance_id() and data.get("manual_index", -1) >= 0:
		var from: int = data.manual_index
		var to := insert_at
		if from < to:
			to -= 1
		if from != to and from >= 0:
			reorder_requested.emit(from, to)
	else:
		insert_requested.emit(data.href, insert_at)


func _hide_drop_indicator() -> void:
	if _drop_indicator:
		_drop_indicator.visible = false
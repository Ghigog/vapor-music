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
const FileUtil = preload("res://scripts/services/file_util.gd")
const TRACK_DRAG_BUTTON = preload("res://scripts/screens/track_drag_button.gd")
const GROUP_HEADER_DRAG_BUTTON = preload("res://scripts/ui/group_header_drag_button.gd")
const AddToPicker = preload("res://scripts/ui/add_to_picker.gd")
const TrackContextMenu = preload("res://scripts/ui/track_context_menu.gd")

const ICON_PLAY := preload("res://assets/icon/play-cropped.png")
const ARROW_ICON_SIZE := 12

## The group-header disclosure arrow reuses the play icon: right-pointing as
## exported for collapsed, rotated 90° clockwise (pointing down) for expanded.
## Cached lazily since every header row in a big grouped library needs it.
static var _icon_arrow_expanded_cache: ImageTexture = null

static func _icon_arrow_expanded() -> ImageTexture:
	if _icon_arrow_expanded_cache == null:
		var img := ICON_PLAY.get_image().duplicate()
		img.rotate_90(0) # 0 = clockwise. This build doesn't expose Image.ClockDirection to GDScript, so it's a bare int.
		_icon_arrow_expanded_cache = ImageTexture.create_from_image(img)
	return _icon_arrow_expanded_cache

## Sort-direction indicators reuse the same play icon: rotated 90°
## counter-clockwise (pointing up) for ascending, the same "expanded" rotation
## above (pointing down) for descending.
static var _icon_arrow_ascending_cache: ImageTexture = null

static func _icon_arrow_ascending() -> ImageTexture:
	if _icon_arrow_ascending_cache == null:
		var img := ICON_PLAY.get_image().duplicate()
		img.rotate_90(1) # 1 = counter-clockwise. See note above re: the missing enum.
		_icon_arrow_ascending_cache = ImageTexture.create_from_image(img)
	return _icon_arrow_ascending_cache

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
## Manual mode only — the ticked rows' playlist positions, so the host can
## remove them in one batch. Indices, not hrefs: a playlist can hold the same
## href more than once, so only position identifies which entry to drop.
signal remove_selection_requested(manual_indices: Array)

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

## Selection mode ("Select"): every row gets a checkbox, pre-ticked for what
## was on screen when it started — expansion is the selection gesture. Row
## clicks toggle instead of play; ✓ confirms, ✕ abandons.
var _selecting := false
var _checked: Dictionary = {}
var _row_checks: Dictionary = {}
const CHECK_W := 18

## Which group headers are expanded, keyed by header text — survives _render()
## rebuilds (entering/exiting selection mode re-renders the whole list; without
## this every group snapped shut the instant "Select" was pressed,
## hiding the very selection the user just made).
var _expanded_groups: Dictionary = {}

## header text -> Array of hrefs in that group, refreshed each _render_virtual()
## regardless of expand state. Lets a header click select the whole group while
## selecting, even collapsed groups whose track rows were never built.
var _group_hrefs: Dictionary = {}

## Width of a collapsed column's stub in the header and its spacer in rows.
const COLLAPSED_W := 18

## User-adjustable column widths, keyed by column id — dragged from the
## resize handle beside each fixed-width header cell. Starting widths for a
## column that hasn't been resized yet; year/bpm/key match their old
## hardcoded widths exactly (no visual change until the user drags).
const DEFAULT_COL_WIDTHS := {
	"title": 200,
	"artist": 140,
	"album": 140,
	"genre": 90,
	"year": 48,
	"bpm": 56,
	"key": 44,
}
const MIN_COL_WIDTH := 36
const MAX_COL_WIDTH := 480
## Width of the draggable strip between resizable header cells. Wider than
## the visible divider line itself (drawn centered within it, see
## _make_resize_handle) so it's forgiving to grab with a real mouse.
const RESIZE_HANDLE_W := 10

var _col_widths: Dictionary = {}
## col_id → live header cell, so a drag can resize the header alongside
## already-built rows without a full _render().
var _header_cells: Dictionary = {}
var _resizing_col := ""
var _resize_start_x := 0.0
var _resize_start_width := 0.0

## Fixed sizes for the artist/album art slot — see _make_art_rect. Rounded to
## the row height each context uses (28/44/28 for wide/narrow/header rows).
const ROW_ART_SIZE_WIDE := 20
const ROW_ART_SIZE_NARROW := 32
const HEADER_ART_SIZE := 20

## All sortable fields, for the narrow-layout sort control. Wide layouts sort
## from the column header; narrow layouts hide the header, so they get a
## compact toolbar chip instead (§14.7 — every affordance needs a narrow/touch
## equivalent).
const SORT_FIELDS := ["title", "artist", "album", "genre", "year", "bpm", "key"]

## Every styled label built so far, as {label, role} — theme changes restyle
## in place instead of rebuilding (which would discard expansion state).
var _styled_cells: Array[Dictionary] = []

## href → {cells: {col_id: Label}} for visible rows, so metadata enrichment
## during playback updates cells in place without a re-render. Manual mode
## only (playlists) — every row is a real, permanent Control there, same as
## before virtualization.
var _live_cells: Dictionary = {}

## Virtualized (non-manual) mode only — see "Virtual row list" section below.
## href → pooled track-row entry, but ONLY for hrefs currently scrolled into
## view; refresh_row() no-ops for anything not in here, same contract as
## _live_cells for the (rare) case of an off-screen row.
var _bound_cells: Dictionary = {}
## Flat list of {kind:"header"|"track", height, ...} slots computed from
## TrackIndex.grouped() + _expanded_groups — the whole scrollable content
## described as pure data, before any row Control exists. Rebuilt whenever
## the underlying rows/filter/sort/group/expand state changes.
var _slot_plan: Array[Dictionary] = []
## Cumulative height prefix sum, one longer than _slot_plan (offsets[i] is
## the y where slot i starts; offsets[-1] is the total scrollable height).
var _slot_offsets: PackedInt64Array = PackedInt64Array()
## Recycled Control pools, one entry per currently-instantiated pooled row —
## sized to roughly what the viewport can show, not to the full row count.
## Each entry: {shell, bound_index: int, ...kind-specific fields}.
var _header_pool: Array[Dictionary] = []
var _track_pool: Array[Dictionary] = []
## Identifies whether the CURRENT pool Controls match what layout/grouping/
## column-visibility calls for — narrow-vs-wide, group_key, and which columns
## are collapsed all change a row's Control SHAPE (not just its content), so
## a pool built for one shape is wrong for another. Compared once per render;
## a mismatch frees and rebuilds both pools from scratch.
var _pool_shape_sig := ""
## Permanent "No tracks." label for the virtualized path (manual mode builds
## its own each render, same as before — see _render_manual()).
var _empty_label: Label

var _toolbar_margin: MarginContainer
var _search_edit: LineEdit
var _group_btn: OptionButton
var _save_btn: Button
var _confirm_btn: Button
var _add_to_btn: Button
## Manual mode only — bulk-removes the ticked rows from this playlist.
var _remove_btn: Button
var _cancel_btn: Button
var _mobile_sort_btn: OptionButton
var _mobile_dir_btn: Button
var _h_scroll: ScrollContainer
var _col_header_margin: MarginContainer
var _col_header: HBoxContainer
var _scroll: ScrollContainer
## VBoxContainer in manual mode (auto-laid-out, unchanged from before
## virtualization); a bare Control in the virtualized path, whose pooled
## children are positioned/sized manually — see _rebind_visible().
var _list: Control
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


## Updates one row's data and any visible cells in place — EXCEPT when the
## edit moves the row to a different group bucket (see below), it never
## re-renders, so a re-render mid-browse doesn't destroy the user's
## group-expansion state. Preserves manual_pos.
func refresh_row(fresh: Dictionary) -> void:
	var old_row: Dictionary = {}
	for i in _rows.size():
		if _rows[i].href == fresh.href:
			old_row = _rows[i]
			if _rows[i].has("manual_pos") and not fresh.has("manual_pos"):
				fresh.manual_pos = _rows[i].manual_pos
			_rows[i] = fresh
			break

	# The active grouping field (artist/album/genre) can change under an
	# in-place edit — e.g. "Edit Metadata" moving a track to a different
	# album. This row's position in the grouped tree was fixed at the last
	# _render(), so an in-place cell-text update alone would leave it
	# rendered under the OLD group. _expanded_groups is keyed by header TEXT,
	# not tree position, so it survives a re-render here — other groups stay
	# exactly as expanded/collapsed as they were.
	var group_key: String = _group_keys[_group_btn.selected] if not manual_mode and not _group_keys.is_empty() else TrackIndex.GROUP_NONE
	if not _search_edit.text.strip_edges().is_empty():
		group_key = TrackIndex.GROUP_NONE
	var grouping_stale: bool = group_key != TrackIndex.GROUP_NONE and not old_row.is_empty() and old_row.get(group_key) != fresh.get(group_key)

	# Same problem, one level down: even ungrouped/flat views are SORTED by
	# one of these fields, so an edited field the current sort runs on leaves
	# the row's list position stale too ("order" is manual playlist position,
	# untouched by a metadata edit, so it's excluded).
	var sort_stale: bool = _sort_key != "order" and not old_row.is_empty() and old_row.get(_sort_key) != fresh.get(_sort_key)

	if grouping_stale or sort_stale:
		_render()
		return

	# Manual mode's rows are permanent Controls tracked in _live_cells; the
	# virtualized path's are pooled and only tracked in _bound_cells while
	# actually scrolled into view — refresh_row() no-ops for anything not
	# currently bound there, same contract manual mode always had for a row
	# that was never built in the first place.
	var cell_map: Dictionary = _live_cells if manual_mode else _bound_cells
	if not cell_map.has(fresh.href):
		return
	var live: Dictionary = cell_map[fresh.href]
	var cells: Dictionary = live.cells
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

	# Enrichment can fill in artwork after the row was already built (it only
	# runs reactively otherwise) — pick up a newly-available path without a
	# full re-render, which would collapse group expansion.
	var art_rect: TextureRect = live.get("art_rect")
	if is_instance_valid(art_rect):
		var fresh_art_path: String = fresh.get("album_art_local", "")
		if fresh_art_path.is_empty():
			fresh_art_path = fresh.get("artist_image_local", "")
		if fresh_art_path.is_empty():
			# This row's own cache entry never got an artist image (enrichment
			# hasn't reached it yet, or its lookup short-circuited) — but another
			# track by the same artist may already have one cached.
			fresh_art_path = MetadataService.get_entity_image_path(TrackIndex.GROUP_ARTIST, fresh.get("artist", ""))
		if fresh_art_path != live.get("art_path", ""):
			live.art_path = fresh_art_path
			_request_art(art_rect, fresh_art_path, 128, live if not manual_mode else {})


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
		# Default to the flat list, not grouped-by-artist — a saved per-screen
		# choice in view_state.json (below) still wins over this once one
		# exists; this only sets what a screen with no saved preference yet
		# opens to.
		_group_btn.select(_group_keys.find(TrackIndex.GROUP_NONE))
		_group_btn.item_selected.connect(func(_i: int) -> void:
			_persist_view_state()
			_reset_and_render()
		)
	toolbar.add_child(_group_btn)

	if show_save_view:
		_save_btn = Button.new()
		_save_btn.text = "Select"
		_save_btn.tooltip_text = ("Select tracks — remove them, add them to a playlist, or save as a new one"
			if manual_mode else "Select tracks — save as a new playlist or add to an existing one")
		_save_btn.pressed.connect(_enter_selection)
		toolbar.add_child(_save_btn)

		_confirm_btn = Button.new()
		_confirm_btn.text = "  ✓  "
		_confirm_btn.tooltip_text = "Save the ticked tracks as a playlist"
		_confirm_btn.visible = false
		_confirm_btn.pressed.connect(_confirm_selection)
		toolbar.add_child(_confirm_btn)

		_add_to_btn = Button.new()
		_add_to_btn.text = "Add to..."
		_add_to_btn.tooltip_text = "Add the ticked tracks to an existing playlist"
		_add_to_btn.visible = false
		_add_to_btn.pressed.connect(_on_add_to_pressed)
		toolbar.add_child(_add_to_btn)

		if manual_mode:
			_remove_btn = Button.new()
			_remove_btn.text = "Remove"
			_remove_btn.tooltip_text = "Remove the ticked tracks from this playlist"
			_remove_btn.visible = false
			_remove_btn.pressed.connect(_on_remove_pressed)
			toolbar.add_child(_remove_btn)

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
	_mobile_dir_btn.icon = _icon_arrow_ascending()
	_mobile_dir_btn.add_theme_constant_override("icon_max_width", 14)
	_mobile_dir_btn.tooltip_text = "Toggle sort direction"
	_mobile_dir_btn.pressed.connect(func() -> void:
		_sort_asc = not _sort_asc
		_mobile_dir_btn.icon = _icon_arrow_ascending() if _sort_asc else _icon_arrow_expanded()
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

	# Columns are user-resizable up to MAX_COL_WIDTH regardless of how narrow
	# the window is — content that no longer fits scrolls horizontally
	# instead of being capped to the current window size or (worse) forcing
	# the table's own minimum width to grow, which would squeeze the sidebar
	# next to it. This wrapper is what actually scrolls; the header and row
	# list inside it (_h_content) are free to be wider than it and share one
	# horizontal scroll position, so columns stay aligned while scrolling.
	_h_scroll = ScrollContainer.new()
	_h_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_h_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	# The toolbar above is an HFlowContainer that wraps onto extra lines
	# whenever its controls don't all fit on one row — a window narrow enough
	# to wrap it several times could otherwise demand more minimum height than
	# this VBoxContainer has, squeezing the row list to zero and making an
	# apparently-populated table render as totally empty. This floor keeps the
	# list always showing at least a few rows; a badly wrapped toolbar clips
	# instead, which is a far less confusing failure.
	_h_scroll.custom_minimum_size.y = 160
	_h_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	_h_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	add_child(_h_scroll)
	var h_content := VBoxContainer.new()
	h_content.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	h_content.size_flags_vertical = Control.SIZE_EXPAND_FILL
	h_content.add_theme_constant_override("separation", 0)
	_h_scroll.add_child(h_content)

	_col_header_margin = MarginContainer.new()
	h_content.add_child(_col_header_margin)
	_col_header = HBoxContainer.new()
	_col_header.add_theme_constant_override("separation", 10)
	_col_header_margin.add_child(_col_header)

	_scroll = ScrollContainer.new()
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	h_content.add_child(_scroll)

	if manual_mode:
		# Manual mode (playlists) stays exactly as before virtualization: every
		# row is a real, permanent child of a normal auto-laying-out
		# VBoxContainer. Playlists are human-curated and naturally small, and
		# drag-reorder's drop-indicator positioning depends on real sibling
		# indices — not worth the risk of rewriting for a virtual scroll when
		# this path never had the performance problem virtualization solves.
		_list = VBoxContainer.new()
		_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_list.add_theme_constant_override("separation", 2)
		_scroll.add_child(_list)

		_drop_indicator = ColorRect.new()
		_drop_indicator.custom_minimum_size.y = 3
		_drop_indicator.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_drop_indicator.visible = false
		_list.add_child(_drop_indicator)
	else:
		# Virtualized path: _list is a bare Control, not a Container — nothing
		# auto-lays-out its children, so every pooled row's position/size is
		# set explicitly (see _rebind_visible). custom_minimum_size.y is set to
		# the FULL virtual content height each render, which is what drives
		# the ScrollContainer's scrollbar range even though only a handful of
		# rows are ever real Controls at once.
		_list = Control.new()
		_list.clip_contents = true
		_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_scroll.add_child(_list)

		_empty_label = Label.new()
		_empty_label.visible = false
		_empty_label.position = Vector2(ThemeManager.current_theme.SPACE_3, ThemeManager.current_theme.SPACE_2)
		_register_cell(_empty_label, "meta")
		_list.add_child(_empty_label)

		# Every scroll path (wheel, scrollbar drag, touch, keyboard focus-
		# scroll) ultimately calls Range.set_value() on the internal
		# VScrollBar, so this fires reliably; _scroll.resized covers a window
		# resize revealing more rows without any scrolling happening.
		_scroll.get_v_scroll_bar().value_changed.connect(func(_v: float) -> void: _rebind_visible())
		_scroll.resized.connect(_rebind_visible)


func _reset_and_render() -> void:
	_flat_shown = FLAT_CAP
	_render()


func _sort_field_list() -> Array:
	return (["order"] if manual_mode else []) + SORT_FIELDS


# ---------------------------------------------------------------------------
# Selection mode (Select)
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


## Checked hrefs in the current sort order — the selection can span
## collapsed groups and searches, so order comes from the full index.
func _checked_hrefs_in_order() -> Array:
	var rows: Array[Dictionary] = []
	rows.assign(_rows)
	TrackIndex.sort_rows(rows, _sort_key, _sort_asc)
	var out: Array = []
	for row in rows:
		if _checked.has(row.href):
			out.append(row.href)
	return out


func _confirm_selection() -> void:
	var out := _checked_hrefs_in_order()
	if not out.is_empty():
		save_selection_requested.emit(out)
	_exit_selection()


## "Add to..." — adds the ticked tracks to an EXISTING playlist, without
## leaving selection mode, so the same selection can be added to more than
## one playlist before the user dismisses with ✓ or ✕.
func _on_add_to_pressed() -> void:
	var out := _checked_hrefs_in_order()
	if out.is_empty():
		return
	AddToPicker.show_for_tracks(self, out, _add_to_btn.get_global_rect().position)


## Manual mode only. Positions, not hrefs — a playlist can hold the same href
## more than once, so only manual_pos identifies which entry to drop.
func _checked_manual_indices() -> Array:
	var out: Array = []
	for row: Dictionary in _rows:
		if _checked.has(row.href):
			out.append(row.get("manual_pos", -1))
	return out


## "Remove" — drops the ticked tracks from this playlist. Unlike "Add to...",
## this exits selection mode: the ticked rows are about to disappear, so
## staying in a selection built around them doesn't make sense.
func _on_remove_pressed() -> void:
	var indices := _checked_manual_indices()
	if not indices.is_empty():
		remove_selection_requested.emit(indices)
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
	_add_to_btn.visible = _selecting
	if _remove_btn:
		_remove_btn.visible = _selecting
	_cancel_btn.visible = _selecting


func _toggle_checked(href: String) -> void:
	if _checked.has(href):
		_checked.erase(href)
	else:
		_checked[href] = true
	var chk = _row_checks.get(href)
	if chk and is_instance_valid(chk):
		_apply_check_visual(chk, _checked.has(href))


## While selecting, a group header click ticks/unticks every track in that
## group in one gesture — works even collapsed, since _group_hrefs is
## rebuilt from the full row set regardless of expand state. Toggle (not
## always-on) so clicking an already-fully-checked group is a fast way to
## deselect it again.
func _toggle_checked_group(header: String) -> void:
	var hrefs: Array = _group_hrefs.get(header, [])
	if hrefs.is_empty():
		return
	var all_checked := true
	for href in hrefs:
		if not _checked.has(href):
			all_checked = false
			break
	for href in hrefs:
		if all_checked:
			_checked.erase(href)
		else:
			_checked[href] = true
	_render()


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
	_col_widths.clear()
	for col_id: String in s.get("col_widths", {}):
		_col_widths[col_id] = int(s.col_widths[col_id])
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
		"col_widths": _col_widths,
		"group": _group_btn.selected if not manual_mode else 0,
	}
	FileUtil.write_string_atomic(VIEW_STATE_PATH, JSON.stringify(all, "\t"))


## Shows/hides the narrow-layout sort controls and syncs them to the current
## sort state (header clicks and the chip drive the same _sort_key/_sort_asc).
func _update_layout_mode() -> void:
	var narrow := PlatformManager.is_mobile_layout()
	_mobile_sort_btn.visible = narrow
	_mobile_dir_btn.visible = narrow
	# _h_scroll's horizontal scroll exists so a wide desktop table with many
	# fixed-width columns can overflow sideways instead of squeezing the
	# sidebar. Narrow layout collapses every row to one stacked art+label
	# unit with nothing to overflow — left enabled there, the ScrollContainer
	# sizes h_content (and everything inside it: toolbar, row list, its own
	# scrollbar) to that unit's tiny natural width instead of the viewport,
	# collapsing the whole table into a sliver down the left edge.
	_h_scroll.horizontal_scroll_mode = (
		ScrollContainer.SCROLL_MODE_DISABLED if narrow else ScrollContainer.SCROLL_MODE_AUTO
	)
	if narrow:
		var idx: int = _sort_field_list().find(_sort_key)
		if idx >= 0:
			_mobile_sort_btn.select(idx)
		_mobile_dir_btn.icon = _icon_arrow_ascending() if _sort_asc else _icon_arrow_expanded()


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
		toolbar_buttons.append(_add_to_btn)
		if _remove_btn:
			toolbar_buttons.append(_remove_btn)
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
	if manual_mode:
		_render_manual()
	else:
		_render_virtual()


## Unchanged from before virtualization — every row is a real, permanent
## Control. Manual mode is always flat (GROUP_NONE), so there's no grouping
## branch here.
func _render_manual() -> void:
	for child in _list.get_children():
		if child != _drop_indicator:
			child.queue_free()
	_styled_cells.clear()
	_live_cells.clear()
	_row_checks.clear()

	var query := _search_edit.text.strip_edges()

	var rows: Array[Dictionary] = []
	rows.assign(_filtered(query))
	TrackIndex.sort_rows(rows, _sort_key, _sort_asc)

	_view_hrefs = []
	for row in rows:
		_view_hrefs.append(row.href)

	_build_col_header(TrackIndex.GROUP_NONE, rows.is_empty())

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

	var shown: int = mini(rows.size(), _flat_shown)
	for i in range(shown):
		_list.add_child(_make_track_row(rows[i], TrackIndex.GROUP_NONE))
	if rows.size() > shown:
		_list.add_child(_make_show_more_row(rows.size() - shown))


## Virtualized (non-manual) path: no Control is built per row/header here —
## just the flat data plan + total scrollable height. Actual Controls are
## handed out by _rebind_visible() from a small recycled pool, only for
## whatever's currently scrolled into view.
func _render_virtual() -> void:
	_row_checks.clear()

	var query := _search_edit.text.strip_edges()
	var group_key: String = _group_keys[_group_btn.selected]
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

	_group_hrefs.clear()
	if group_key != TrackIndex.GROUP_NONE:
		for group: Dictionary in TrackIndex.grouped(rows, group_key):
			var hrefs: Array = []
			for row: Dictionary in group.rows:
				hrefs.append(row.href)
			_group_hrefs[group.header] = hrefs

	_build_col_header(group_key, rows.is_empty())

	var shape_sig := _current_shape_sig(group_key)
	if shape_sig != _pool_shape_sig:
		_clear_pools()
		_pool_shape_sig = shape_sig

	if rows.is_empty():
		_empty_label.text = "No tracks match \"%s\"." % query if not query.is_empty() else "No tracks."
		_empty_label.visible = true
		_slot_plan = []
		_slot_offsets = PackedInt64Array([0])
		_list.custom_minimum_size.y = 0
		_pool_release_excess("header", 0)
		_pool_release_excess("track", 0)
		return

	_empty_label.visible = false
	_slot_plan = _build_slot_plan(rows, group_key)
	_slot_offsets = _build_slot_offsets(_slot_plan)
	_list.custom_minimum_size.y = _slot_offsets[_slot_offsets.size() - 1]
	_rebind_visible()


## Identifies the row Control SHAPE currently in effect — narrow-vs-wide
## layout, the active grouping (changes which columns show), and which
## columns are collapsed to a stub all change what a pooled row/header
## Control looks like, not just its content. Deliberately excludes _selecting
## (checkboxes are permanent children, just visibility-toggled) and
## _col_widths (a resize changes width, not shape — handled live).
func _current_shape_sig(group_key: String) -> String:
	return "%s|%s|%s" % [PlatformManager.is_mobile_layout(), group_key, str(_collapsed.keys())]


func _clear_pools() -> void:
	for entry: Dictionary in _header_pool:
		entry.shell.container.queue_free()
	for entry: Dictionary in _track_pool:
		entry.shell.container.queue_free()
	_header_pool.clear()
	_track_pool.clear()
	_bound_cells.clear()
	_row_checks.clear()
	_styled_cells.clear()


# ---------------------------------------------------------------------------
# Virtual slot plan — pure data, no Controls. See track_table_virtual tests.
# ---------------------------------------------------------------------------

## Flattens TrackIndex.grouped() + _expanded_groups into ordered slots. A
## collapsed group contributes ONE header slot and zero track slots — this
## alone is what keeps a fully-collapsed grouped library to a handful of
## Controls instead of one per group, before pooling even comes into it.
func _build_slot_plan(rows: Array[Dictionary], group_key: String) -> Array[Dictionary]:
	var narrow := PlatformManager.is_mobile_layout()
	var track_h: int = ThemeManager.min_touch_height(44 if narrow else 28)
	var header_h: int = ThemeManager.min_touch_height(28)
	var plan: Array[Dictionary] = []
	if group_key == TrackIndex.GROUP_NONE:
		for row: Dictionary in rows:
			plan.append({"kind": "track", "row": row, "group_key": group_key, "height": track_h})
		return plan
	for group: Dictionary in TrackIndex.grouped(rows, group_key):
		plan.append({"kind": "header", "header": group.header, "count": group.rows.size(), "group_key": group_key, "height": header_h})
		if _expanded_groups.get(group.header, false):
			for row: Dictionary in group.rows:
				plan.append({"kind": "track", "row": row, "group_key": group_key, "height": track_h})
	return plan


## offsets[i] = y-position where slot i starts; offsets[N] = total height.
func _build_slot_offsets(plan: Array[Dictionary]) -> PackedInt64Array:
	var offsets := PackedInt64Array()
	offsets.resize(plan.size() + 1)
	offsets[0] = 0
	for i in plan.size():
		offsets[i + 1] = offsets[i] + int(plan[i].height)
	return offsets


## Returns [start, end) slot indices overlapping the vertical window
## [scroll_y, scroll_y + viewport_h). Binary search over the offsets prefix
## sum — offsets is strictly increasing (every slot height > 0), so a plain
## bsearch is unambiguous.
func _visible_slot_range(offsets: PackedInt64Array, scroll_y: float, viewport_h: float) -> Vector2i:
	var n := offsets.size() - 1
	if n <= 0:
		return Vector2i.ZERO
	# Largest i such that offsets[i] <= scroll_y.
	var start_idx: int = offsets.bsearch(int(floor(scroll_y)), true)
	if start_idx >= offsets.size() or offsets[start_idx] > scroll_y:
		start_idx -= 1
	start_idx = clampi(start_idx, 0, n - 1)
	# Smallest i such that offsets[i] >= scroll_y + viewport_h.
	var end_idx: int = offsets.bsearch(int(ceil(scroll_y + viewport_h)), true)
	end_idx = clampi(end_idx, start_idx + 1, n)
	return Vector2i(start_idx, end_idx)


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
		{"id": "title", "label": "Title", "width": _col_width("title")},
		{"id": context_field, "label": "Album" if context_field == "album" else "Artist", "width": _col_width(context_field)},
	]
	if group_key != TrackIndex.GROUP_GENRE:
		cols.append({"id": "genre", "label": "Genre", "width": _col_width("genre")})
	cols.append({"id": "year", "label": "Year", "width": _col_width("year"), "align_right": true})
	cols.append({"id": "bpm", "label": "BPM", "width": _col_width("bpm"), "align_right": true})
	cols.append({"id": "key", "label": "Key", "width": _col_width("key"), "align_right": true})

	for col in cols:
		col["collapsed"] = _collapsed.has(col.id)
	return cols


func _col_width(col_id: String) -> int:
	return int(_col_widths.get(col_id, DEFAULT_COL_WIDTHS.get(col_id, 100)))


func _apply_col_sizing(lbl: Label, col: Dictionary) -> void:
	lbl.clip_text = true
	if col.has("width"):
		lbl.custom_minimum_size.x = col.width
		if col.get("align_right", false):
			lbl.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	else:
		lbl.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		lbl.size_flags_stretch_ratio = col.ratio


func _build_col_header(group_key: String, empty: bool) -> void:
	for child in _col_header.get_children():
		child.queue_free()
	_header_cells.clear()
	_col_header_margin.visible = not empty and not PlatformManager.is_mobile_layout()
	if not _col_header_margin.visible:
		return
	if _selecting:
		var gutter := Control.new()
		gutter.custom_minimum_size.x = CHECK_W
		gutter.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_col_header.add_child(gutter)
	for col: Dictionary in _columns(group_key):
		var cell := _make_header_cell(col)
		_header_cells[col.id] = cell
		_col_header.add_child(cell)
		# Every visible column is a fixed, user-resizable width — a drag handle
		# goes right after it. Growing one only pushes columns to ITS right
		# (never the flexible trailing spacer below shrinking something on the
		# left) — the old "Title absorbs all leftover space" design made every
		# drag look reversed, since growing anything shrank Title upstream and
		# pulled the whole rest of the row left to compensate.
		if col.has("width") and not col.get("collapsed", false):
			_col_header.add_child(_make_resize_handle(col.id))
	_col_header.add_child(_make_trailing_spacer())
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
		if col.get("align_right", false):
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
		arrow.icon = _icon_arrow_ascending() if _sort_asc else _icon_arrow_expanded()
		arrow.add_theme_constant_override("icon_max_width", 12)
		arrow.add_theme_color_override("icon_normal_color", ThemeManager.current_theme.ACCENT_CORE)
		arrow.add_theme_color_override("icon_hover_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	else:
		# Stacked up/down hint, tiny — same play icon, one rotated each way.
		arrow.custom_minimum_size.x = 16.0
		var stack := VBoxContainer.new()
		stack.mouse_filter = Control.MOUSE_FILTER_IGNORE
		stack.alignment = BoxContainer.ALIGNMENT_CENTER
		stack.add_theme_constant_override("separation", -1)
		arrow.add_child(stack)
		stack.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		for icon_tex: Texture2D in [_icon_arrow_ascending(), _icon_arrow_expanded()]:
			var mini := TextureRect.new()
			mini.texture = icon_tex
			mini.custom_minimum_size = Vector2(7, 7)
			mini.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
			mini.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
			mini.mouse_filter = Control.MOUSE_FILTER_IGNORE
			mini.self_modulate = ThemeManager.current_theme.TEXT_TERTIARY
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


# ---------------------------------------------------------------------------
# Virtual row list — pool management + rebind (non-manual mode only)
# ---------------------------------------------------------------------------

## Returns the pool entry for `index` within the currently-visible run,
## creating one (and adding it to _list) if the pool isn't that big yet.
func _pool_get(kind: String, index: int, group_key: String) -> Dictionary:
	var pool: Array = _header_pool if kind == "header" else _track_pool
	if index < pool.size():
		return pool[index]
	var entry: Dictionary = _create_header_shell(group_key) if kind == "header" else _create_track_shell(group_key)
	pool.append(entry)
	_list.add_child(entry.shell.container)
	return entry


## Hides (never frees) any pool entries beyond what this rebind needs.
func _pool_release_excess(kind: String, needed_count: int) -> void:
	var pool: Array = _header_pool if kind == "header" else _track_pool
	for i in range(needed_count, pool.size()):
		var entry: Dictionary = pool[i]
		entry.shell.container.visible = false
		entry.bound_index = -1


## Recomputes the visible slot range from current scroll position + viewport
## size, and binds/positions/sizes pool Controls to cover exactly that range.
## Called after every _render_virtual() and on every scroll/resize.
func _rebind_visible() -> void:
	if manual_mode or _slot_plan.is_empty():
		return
	var viewport_h: float = _scroll.size.y
	var total_h: float = float(_slot_offsets[_slot_offsets.size() - 1])
	var scroll_y: float = clampf(_scroll.scroll_vertical, 0.0, maxf(0.0, total_h - viewport_h))
	var visible: Vector2i = _visible_slot_range(_slot_offsets, scroll_y, viewport_h)

	var row_width: float = _scroll.size.x
	var header_cursor := 0
	var track_cursor := 0
	# Rebuilt fresh every call from whatever's actually on screen right now —
	# refresh_row() and _row_checks both key off this, so it must reflect the
	# CURRENT visible set, not just what changed since the last rebind.
	_bound_cells.clear()
	_row_checks.clear()
	for i in range(visible.x, visible.y):
		var slot: Dictionary = _slot_plan[i]
		var y: float = float(_slot_offsets[i])
		var h: float = float(slot.height)
		var kind: String = slot.kind
		var entry: Dictionary = _pool_get(kind, header_cursor if kind == "header" else track_cursor, slot.group_key)
		if kind == "header":
			header_cursor += 1
		else:
			track_cursor += 1
		if entry.bound_index != i:
			if kind == "header":
				_bind_header_shell(entry, slot)
			else:
				_bind_track_shell(entry, slot)
			entry.bound_index = i
		if kind == "track":
			# Re-applied every rebind (not just on a fresh bind) — _bound_cells
			# and _row_checks were just cleared above, so an already-bound row
			# that's merely staying on screen across a scroll-only rebind still
			# needs to be re-registered in both, or it'd silently drop out.
			_checkbox_bind(entry.checkbox, entry.href)
			_bound_cells[entry.href] = entry
		entry.shell.container.position = Vector2(0, y)
		entry.shell.container.size = Vector2(row_width, h)
		entry.shell.container.visible = true

	_pool_release_excess("header", header_cursor)
	_pool_release_excess("track", track_cursor)


## Wired once per pooled Control at creation — never re-attached per bind
## (that would duplicate the gesture Timer/connections). Every handler below
## reads state live off the button/entry instead of closing over the slot
## data that was true when this shell happened to be created.
func _create_header_shell(group_key: String) -> Dictionary:
	var theme = ThemeManager.current_theme
	var shell := _row_shell(28, theme.SPACE_3)

	shell.button.set_script(GROUP_HEADER_DRAG_BUTTON)
	shell.button._setup_gestures()
	shell.button.long_pressed.connect(func(pos: Vector2) -> void:
		if not shell.button.entity_type.is_empty():
			AddToPicker.show_for_entity(self, shell.button.entity_type, shell.button.entity_value, pos)
	)
	shell.button.right_clicked.connect(func(pos: Vector2) -> void:
		if not shell.button.entity_type.is_empty():
			AddToPicker.show_for_entity(self, shell.button.entity_type, shell.button.entity_value, pos)
	)

	var hbox := HBoxContainer.new()
	hbox.mouse_filter = Control.MOUSE_FILTER_IGNORE
	hbox.add_theme_constant_override("separation", 10)
	shell.stack.add_child(hbox)
	hbox.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	# Circle for an artist's photo, rounded-square for an album's art — shape
	# follows what's pictured (DESIGN_LANGUAGE §5.2). Presence/shape is fixed
	# by group_key at creation time, safe since group_key is part of the pool
	# shape signature (a group_key change frees and rebuilds both pools).
	var art_rect: TextureRect = null
	if group_key == TrackIndex.GROUP_ARTIST or group_key == TrackIndex.GROUP_ALBUM:
		art_rect = _make_art_rect(HEADER_ART_SIZE, group_key == TrackIndex.GROUP_ARTIST)
		hbox.add_child(art_rect)

	var arrow_rect := TextureRect.new()
	arrow_rect.texture = ICON_PLAY
	arrow_rect.custom_minimum_size = Vector2(ARROW_ICON_SIZE, ARROW_ICON_SIZE)
	arrow_rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	arrow_rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	arrow_rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	arrow_rect.self_modulate = ThemeManager.current_theme.TEXT_PRIMARY
	hbox.add_child(arrow_rect)

	var lbl := Label.new()
	lbl.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_register_cell(lbl, "header")
	hbox.add_child(lbl)

	var count_lbl := Label.new()
	count_lbl.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	count_lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_register_cell(count_lbl, "small")
	hbox.add_child(count_lbl)

	hbox.add_child(_make_trailing_spacer())

	var entry: Dictionary = {
		"shell": shell, "bound_index": -1, "label": lbl, "count_label": count_lbl,
		"art_rect": art_rect, "art_path": "", "arrow": arrow_rect,
	}

	shell.button.pressed.connect(func() -> void:
		# A long press already opened the "Add to Group" picker — the release
		# ending it must not ALSO expand/collapse the section.
		if shell.button.suppress_next_click:
			shell.button.suppress_next_click = false
			return
		if entry.bound_index < 0 or entry.bound_index >= _slot_plan.size():
			return
		var slot: Dictionary = _slot_plan[entry.bound_index]
		var header: String = slot.header
		if _selecting:
			_toggle_checked_group(header)
			return
		var expanded: bool = not _expanded_groups.get(header, false)
		if expanded:
			_expanded_groups[header] = true
		else:
			_expanded_groups.erase(header)
		if header != TrackIndex.UNKNOWN_HEADER:
			if slot.group_key == TrackIndex.GROUP_ARTIST:
				artist_focused.emit(header)
			elif slot.group_key == TrackIndex.GROUP_ALBUM:
				album_focused.emit(header)
		_render()
	)

	return entry


func _bind_header_shell(entry: Dictionary, slot: Dictionary) -> void:
	var header: String = slot.header
	var group_key: String = slot.group_key
	entry.label.text = header
	entry.count_label.text = "%d" % slot.count
	var expanded: bool = _expanded_groups.get(header, false)
	entry.arrow.texture = _icon_arrow_expanded() if expanded else ICON_PLAY

	var btn: Button = entry.shell.button
	if group_key != TrackIndex.GROUP_NONE and header != TrackIndex.UNKNOWN_HEADER:
		btn.entity_type = group_key
		btn.entity_value = header
	else:
		btn.entity_type = ""
		btn.entity_value = ""

	if entry.art_rect != null:
		if header != TrackIndex.UNKNOWN_HEADER:
			var art_path: String = MetadataService.get_entity_image_path(group_key, header)
			if art_path != entry.art_path:
				entry.art_path = art_path
				_request_art(entry.art_rect, art_path, 96, entry)
		elif entry.art_path != "":
			entry.art_path = ""
			entry.art_rect.texture = null


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


## Builds a fixed-size art slot, masked circle (artist) or rounded-square
## (album). Always reserves its layout space — texture starts null and is
## filled in later by _request_art, so a row/header never shifts when
## enrichment fills in an image that wasn't cached yet.
func _make_art_rect(size: int, circle: bool) -> TextureRect:
	var rect := TextureRect.new()
	rect.custom_minimum_size = Vector2(size, size)
	rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	rect.stretch_mode = TextureRect.STRETCH_SCALE
	rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	if circle:
		ThumbnailService.apply_circle_mask(rect)
	else:
		ThumbnailService.apply_rounded_mask(rect, ThemeManager.current_theme.RADIUS_XS)
	return rect


## Resolves `rect`'s texture from a cache-only local path. Empty path clears
## it (e.g. not enriched yet); a cache miss decodes in the background and
## fills in later, guarded against the row having been freed by a re-render.
## `entry`, when given, is a pooled row/header's own data dict — since it's
## rebound to different content across its lifetime (recycling), the decode
## callback re-checks entry.art_path against what was actually requested
## before applying the texture, so a slow decode for row A can't land on
## row B's Control after the pool reused it for a different track/header.
func _request_art(rect: TextureRect, path: String, max_size: int = 128, entry: Dictionary = {}) -> void:
	if path.is_empty():
		rect.texture = null
		return
	ThumbnailService.request(path, func(tex: Texture2D) -> void:
		if not is_instance_valid(rect):
			return
		if not entry.is_empty() and entry.get("art_path", "") != path:
			return
		rect.texture = tex
	, max_size)


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


## Inert spacer matching a header resize handle's width, so a row's fixed-
## width cells land at the same x as the header's (the handle itself only
## exists in the header — rows never receive mouse events for it).
func _make_col_gap() -> Control:
	var gap := Control.new()
	gap.custom_minimum_size.x = RESIZE_HANDLE_W
	gap.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return gap


## The one flexible element in the row — sits after every fixed-width column
## and absorbs whatever width they don't use. Since it's last, resizing any
## column only ever pushes what's to its right; this is the only thing that
## grows or shrinks in response, so nothing upstream of a resized column ever
## moves.
func _make_trailing_spacer() -> Control:
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return spacer


## Draggable strip after a fixed-width header cell. A faint vertical line
## marks it at rest (otherwise there's nothing on screen to find besides the
## cursor change) and brightens to the accent color on hover. Drag start/end
## are caught here (the mouse is guaranteed to be over this thin strip at
## press time); the drag itself is tracked in _input since a fast drag
## quickly leaves the strip's own rect.
func _make_resize_handle(col_id: String) -> Control:
	var handle := Control.new()
	handle.custom_minimum_size.x = RESIZE_HANDLE_W
	handle.mouse_filter = Control.MOUSE_FILTER_STOP
	handle.mouse_default_cursor_shape = Control.CURSOR_HSPLIT

	var line := ColorRect.new()
	line.color = ThemeManager.current_theme.GLASS_BORDER
	line.mouse_filter = Control.MOUSE_FILTER_IGNORE
	line.set_anchors_preset(Control.PRESET_FULL_RECT)
	line.offset_left = RESIZE_HANDLE_W / 2 - 1
	line.offset_right = -(RESIZE_HANDLE_W / 2 - 1)
	handle.add_child(line)
	handle.mouse_entered.connect(func() -> void:
		line.color = ThemeManager.current_theme.ACCENT_CORE
	)
	handle.mouse_exited.connect(func() -> void:
		line.color = ThemeManager.current_theme.GLASS_BORDER
	)

	handle.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
			if event.double_click:
				_col_widths.erase(col_id)
				_set_col_width(col_id, DEFAULT_COL_WIDTHS.get(col_id, 100))
				_persist_view_state()
				return
			_resizing_col = col_id
			_resize_start_x = _col_header.get_local_mouse_position().x
			_resize_start_width = _col_width(col_id)
			get_viewport().set_input_as_handled()
	)
	return handle


## Applies a resized column width to the live header cell and every currently
## built row's cell for that column — no re-render, so scroll position and
## group-expansion state survive a drag.
func _set_col_width(col_id: String, width: int) -> void:
	var clamped: int = clampi(width, MIN_COL_WIDTH, MAX_COL_WIDTH)
	_col_widths[col_id] = clamped
	var header_cell: Control = _header_cells.get(col_id)
	if header_cell and is_instance_valid(header_cell):
		header_cell.custom_minimum_size.x = clamped
	# Off-screen pooled rows pick up the new width from _col_widths the next
	# time they're bound (_bind_track_shell re-applies every width_node on
	# every bind) — this only needs to touch what's live right now.
	var cell_map: Dictionary = _live_cells if manual_mode else _bound_cells
	for href: String in cell_map:
		var width_nodes: Dictionary = cell_map[href].get("width_nodes", {})
		var node: Control = width_nodes.get(col_id)
		if node and is_instance_valid(node):
			node.custom_minimum_size.x = clamped


func _input(event: InputEvent) -> void:
	if _resizing_col.is_empty():
		return
	if event is InputEventMouseMotion:
		# No positional clamping needed here: MAX_COL_WIDTH in _set_col_width
		# is the only ceiling, and a column wider than the visible panel is
		# fine now — the table scrolls horizontally instead of the panel
		# (or the sidebar next to it) having to grow to fit it.
		var local_x: float = _col_header.get_local_mouse_position().x
		var delta: float = local_x - _resize_start_x
		_set_col_width(_resizing_col, int(_resize_start_width + delta))
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and not event.pressed:
		_resizing_col = ""
		_persist_view_state()
		get_viewport().set_input_as_handled()


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
		TrackContextMenu.show_for_track(self, row.href, pos)
	)
	btn.right_clicked.connect(func(pos: Vector2) -> void:
		TrackContextMenu.show_for_track(self, row.href, pos)
	)
	if manual_mode:
		btn.set_meta("drag_extras", {"manual_index": row.get("manual_pos", -1), "table_iid": get_instance_id()})
		btn.set_meta("drop_table", self)
		btn.set_meta("row_container", shell.container)

	shell.container.set_meta("href", row.href)

	var cells: Dictionary = {}
	## Node whose custom_minimum_size.x a live resize should touch — same as
	## cells[id] for plain-Label columns, but the wrapping hbox for "title"
	## (which also carries the art icon, so the Label itself must stay
	## expand-fill inside a fixed-width wrapper rather than being fixed-width
	## itself).
	var width_nodes: Dictionary = {}
	var title_lbl: Label
	var art_rect: TextureRect = null

	# Album art first, artist image as fallback — same order focus_track()
	# already uses when it picks a single image for a track.
	var art_path: String = row.get("album_art_local", "")
	if art_path.is_empty():
		art_path = row.get("artist_image_local", "")
	if art_path.is_empty():
		# This row's own cache entry never got an artist image (enrichment
		# hasn't reached it yet, or its lookup short-circuited) — but another
		# track by the same artist may already have one cached (the group
		# header art already relies on this same cross-track lookup).
		art_path = MetadataService.get_entity_image_path(TrackIndex.GROUP_ARTIST, row.get("artist", ""))

	if narrow:
		var outer := HBoxContainer.new()
		outer.mouse_filter = Control.MOUSE_FILTER_IGNORE
		outer.add_theme_constant_override("separation", 8)
		shell.stack.add_child(outer)
		outer.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		if _selecting:
			outer.add_child(_make_check_box(row.href))

		art_rect = _make_art_rect(ROW_ART_SIZE_NARROW, false)
		outer.add_child(art_rect)
		_request_art(art_rect, art_path)

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
			if col.id == "title":
				# Title carries the art slot, so it's wrapped in its own hbox
				# rather than going through _apply_col_sizing (Label-only) —
				# the wrapper takes the fixed column width; cells["title"]
				# still ends up bound to the actual Label (hover/live-cell
				# code needs the Label), so a live resize targets the wrapper
				# via width_nodes instead.
				var title_box := HBoxContainer.new()
				title_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
				title_box.add_theme_constant_override("separation", 6)
				title_box.custom_minimum_size.x = col.width
				hbox.add_child(title_box)
				width_nodes[col.id] = title_box

				art_rect = _make_art_rect(ROW_ART_SIZE_WIDE, false)
				title_box.add_child(art_rect)
				_request_art(art_rect, art_path)

				var title_cell := Label.new()
				title_cell.text = _cell_text(row, col.id)
				title_cell.clip_text = true
				title_cell.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
				title_cell.mouse_filter = Control.MOUSE_FILTER_IGNORE
				title_cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
				_register_cell(title_cell, _cell_role(row, col.id))
				title_box.add_child(title_cell)
				cells[col.id] = title_cell
				hbox.add_child(_make_col_gap())
				continue
			var lbl := Label.new()
			lbl.text = _cell_text(row, col.id)
			lbl.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
			lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
			_apply_col_sizing(lbl, col)
			_register_cell(lbl, _cell_role(row, col.id))
			hbox.add_child(lbl)
			cells[col.id] = lbl
			width_nodes[col.id] = lbl
			if col.has("width"):
				hbox.add_child(_make_col_gap())
		hbox.add_child(_make_trailing_spacer())
		if manual_mode:
			hbox.add_child(_make_row_gutter())
		title_lbl = cells["title"]

	_live_cells[row.href] = {"cells": cells, "width_nodes": width_nodes, "art_rect": art_rect, "art_path": art_path}

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


# ---------------------------------------------------------------------------
# Virtual row list — track-row shell (non-manual mode only; see _make_track_row
# above for manual mode's unchanged, fully-materialized equivalent)
# ---------------------------------------------------------------------------

## Font-independent checkbox, wired once — reads its currently-bound href off
## its own meta at click time rather than a captured value, since a pooled
## checkbox is rebound to different hrefs across its lifetime.
func _make_check_box_shell() -> Button:
	var chk := Button.new()
	chk.custom_minimum_size = Vector2(CHECK_W, CHECK_W)
	chk.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	chk.focus_mode = Control.FOCUS_NONE
	chk.set_meta("bound_href", "")
	chk.pressed.connect(func() -> void: _toggle_checked(chk.get_meta("bound_href", "")))
	return chk


func _checkbox_bind(chk: Button, href: String) -> void:
	chk.visible = _selecting
	chk.set_meta("bound_href", href)
	if _selecting:
		_apply_check_visual(chk, _checked.has(href))
		_row_checks[href] = chk


func _create_track_shell(group_key: String) -> Dictionary:
	var narrow := PlatformManager.is_mobile_layout()
	var indent: int = ThemeManager.current_theme.SPACE_6 if group_key != TrackIndex.GROUP_NONE else ThemeManager.current_theme.SPACE_3
	var shell := _row_shell(44 if narrow else 28, indent)

	var btn: Button = shell.button
	btn.set_script(TRACK_DRAG_BUTTON)
	btn._setup_gestures()
	btn.long_pressed.connect(func(pos: Vector2) -> void:
		TrackContextMenu.show_for_track(self, btn.href, pos)
	)
	btn.right_clicked.connect(func(pos: Vector2) -> void:
		TrackContextMenu.show_for_track(self, btn.href, pos)
	)

	var entry: Dictionary = {"shell": shell, "bound_index": -1, "href": "", "art_path": ""}
	var checkbox := _make_check_box_shell()
	entry["checkbox"] = checkbox

	var cells: Dictionary = {}
	var width_nodes: Dictionary = {}
	var title_lbl: Label
	var art_rect: TextureRect

	if narrow:
		var outer := HBoxContainer.new()
		outer.mouse_filter = Control.MOUSE_FILTER_IGNORE
		outer.add_theme_constant_override("separation", 8)
		shell.stack.add_child(outer)
		outer.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		outer.add_child(checkbox)

		art_rect = _make_art_rect(ROW_ART_SIZE_NARROW, false)
		outer.add_child(art_rect)

		var vbox := VBoxContainer.new()
		vbox.mouse_filter = Control.MOUSE_FILTER_IGNORE
		vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		vbox.add_theme_constant_override("separation", 0)
		vbox.alignment = BoxContainer.ALIGNMENT_CENTER
		outer.add_child(vbox)

		title_lbl = Label.new()
		title_lbl.clip_text = true
		title_lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_register_cell(title_lbl, "title")
		vbox.add_child(title_lbl)

		var sub := Label.new()
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
		hbox.add_child(checkbox)

		for col: Dictionary in _columns(group_key):
			if col.get("collapsed", false):
				var spacer := Control.new()
				spacer.custom_minimum_size.x = COLLAPSED_W
				spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
				hbox.add_child(spacer)
				continue
			if col.id == "title":
				var title_box := HBoxContainer.new()
				title_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
				title_box.add_theme_constant_override("separation", 6)
				title_box.custom_minimum_size.x = col.width
				hbox.add_child(title_box)
				width_nodes[col.id] = title_box

				art_rect = _make_art_rect(ROW_ART_SIZE_WIDE, false)
				title_box.add_child(art_rect)

				var title_cell := Label.new()
				title_cell.clip_text = true
				title_cell.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
				title_cell.mouse_filter = Control.MOUSE_FILTER_IGNORE
				title_cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
				_register_cell(title_cell, "title")
				title_box.add_child(title_cell)
				cells[col.id] = title_cell
				hbox.add_child(_make_col_gap())
				continue
			var lbl := Label.new()
			lbl.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
			lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
			_apply_col_sizing(lbl, col)
			_register_cell(lbl, "meta")
			hbox.add_child(lbl)
			cells[col.id] = lbl
			width_nodes[col.id] = lbl
			if col.has("width"):
				hbox.add_child(_make_col_gap())
		hbox.add_child(_make_trailing_spacer())
		title_lbl = cells["title"]

	entry["cells"] = cells
	entry["width_nodes"] = width_nodes
	entry["art_rect"] = art_rect
	entry["title_lbl"] = title_lbl

	btn.pressed.connect(func() -> void:
		if btn.suppress_next_click:
			btn.suppress_next_click = false
			return
		if _selecting:
			_toggle_checked(btn.href)
			return
		if entry.bound_index >= 0 and entry.bound_index < _slot_plan.size():
			var row: Dictionary = _slot_plan[entry.bound_index].row
			play_requested.emit(row, _view_hrefs)
	)
	# Hover reads ThemeManager at call time and title_lbl is the SAME Label
	# object across every rebind of this pooled shell, so this needs no
	# live-read fix — only the row DATA varies per bind, not this reference.
	btn.mouse_entered.connect(func() -> void:
		title_lbl.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	)
	btn.mouse_exited.connect(func() -> void:
		title_lbl.add_theme_color_override("font_color", _style_for_role("title").color)
	)

	return entry


func _bind_track_shell(entry: Dictionary, slot: Dictionary) -> void:
	var row: Dictionary = slot.row
	var btn: Button = entry.shell.button
	# Stop any long-press in flight against whatever this shell was PREVIOUSLY
	# bound to — its release must not fire against the newly-bound row.
	btn._press_timer.stop()
	btn.suppress_next_click = false
	btn.href = row.href
	btn.track_title = row.title
	entry.href = row.href

	for col_id: String in entry.cells:
		var lbl: Label = entry.cells[col_id]
		if col_id == "sub":
			lbl.text = _mobile_sub_text(row)
		else:
			lbl.text = _cell_text(row, col_id)
			var role := _cell_role(row, col_id)
			lbl.set_meta("role", role)
			lbl.add_theme_color_override("font_color", _style_for_role(role).color)

	for col_id: String in entry.width_nodes:
		var node: Control = entry.width_nodes[col_id]
		node.custom_minimum_size.x = _col_width(col_id)

	var art_path: String = row.get("album_art_local", "")
	if art_path.is_empty():
		art_path = row.get("artist_image_local", "")
	if art_path.is_empty():
		art_path = MetadataService.get_entity_image_path(TrackIndex.GROUP_ARTIST, row.get("artist", ""))
	if art_path != entry.art_path:
		entry.art_path = art_path
		_request_art(entry.art_rect, art_path, 128, entry)


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
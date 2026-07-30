## test_track_table_virtual.gd
## GUT unit tests for track_table.gd's virtual-scroll slot-plan logic
## (_build_slot_plan / _build_slot_offsets / _visible_slot_range) — pure data
## functions, no Control involved, that decide how many real rows a 500+
## track grouped library actually needs at once (only what's on screen).
extends GutTest

const TrackIndex = preload("res://scripts/services/track_index.gd")
const TrackTable = preload("res://scripts/ui/track_table.gd")

var table


func before_each() -> void:
	table = TrackTable.new()
	table.manual_mode = false
	add_child_autofree(table)


func _row(href: String, artist: String, album: String, genre: String = "") -> Dictionary:
	return {
		"href": href, "title": href, "artist": artist, "album": album,
		"artist_source": "cache", "album_source": "cache", "genre": genre,
		"bpm": 0.0, "key": "", "year": 0,
		"album_art_local": "", "artist_image_local": "",
	}


func test_flat_mode_is_one_track_slot_per_row() -> void:
	var rows: Array[Dictionary] = [_row("a", "Artist A", "Album A"), _row("b", "Artist B", "Album B")]
	var plan: Array[Dictionary] = table._build_slot_plan(rows, TrackIndex.GROUP_NONE)
	assert_eq(plan.size(), 2, "Flat mode should have exactly one slot per row")
	for slot: Dictionary in plan:
		assert_eq(slot.kind, "track")


func test_all_collapsed_groups_produce_only_header_slots() -> void:
	var rows: Array[Dictionary] = [
		_row("a", "Artist A", "Album A"), _row("b", "Artist A", "Album A"),
		_row("c", "Artist B", "Album B"), _row("d", "Artist C", "Album C"),
	]
	# _expanded_groups starts empty — nothing expanded.
	var plan: Array[Dictionary] = table._build_slot_plan(rows, TrackIndex.GROUP_ARTIST)
	assert_eq(plan.size(), 3, "3 distinct artists, all collapsed, should be exactly 3 header slots")
	for slot: Dictionary in plan:
		assert_eq(slot.kind, "header", "No group is expanded, so no track slots should appear")


func test_expanding_one_group_adds_only_its_own_track_slots() -> void:
	var rows: Array[Dictionary] = [
		_row("a", "Artist A", "Album A"), _row("b", "Artist A", "Album A"),
		_row("c", "Artist B", "Album B"),
	]
	table._expanded_groups["Artist A"] = true
	var plan: Array[Dictionary] = table._build_slot_plan(rows, TrackIndex.GROUP_ARTIST)
	# header(A) + 2 tracks + header(B) collapsed = 4 slots total.
	assert_eq(plan.size(), 4)
	assert_eq(plan[0].kind, "header")
	assert_eq(plan[0].header, "Artist A")
	assert_eq(plan[1].kind, "track")
	assert_eq(plan[2].kind, "track")
	assert_eq(plan[3].kind, "header")
	assert_eq(plan[3].header, "Artist B")


func test_slot_offsets_cumulative_sum() -> void:
	var plan: Array[Dictionary] = [
		{"kind": "header", "height": 28},
		{"kind": "track", "height": 28},
		{"kind": "track", "height": 28},
	]
	var offsets: PackedInt64Array = table._build_slot_offsets(plan)
	assert_eq(offsets.size(), 4)
	assert_eq(offsets[0], 0)
	assert_eq(offsets[1], 28)
	assert_eq(offsets[2], 56)
	assert_eq(offsets[3], 84)


func test_visible_slot_range_empty_plan() -> void:
	var empty_plan: Array[Dictionary] = []
	var offsets: PackedInt64Array = table._build_slot_offsets(empty_plan)
	var visible: Vector2i = table._visible_slot_range(offsets, 0.0, 100.0)
	assert_eq(visible, Vector2i.ZERO)


func test_visible_slot_range_covers_full_short_list() -> void:
	# 5 slots of height 20 = 100 total; a 200px viewport should show all of them.
	var plan: Array[Dictionary] = []
	for i in 5:
		plan.append({"kind": "track", "height": 20})
	var offsets: PackedInt64Array = table._build_slot_offsets(plan)
	var visible: Vector2i = table._visible_slot_range(offsets, 0.0, 200.0)
	assert_eq(visible, Vector2i(0, 5))


func test_visible_slot_range_scrolled_to_middle() -> void:
	# 10 slots of height 10 = 100 total. Scrolled to y=45 with a 20px viewport
	# should cover slots [4, 7) — slot 4 spans [40,50), slot 6 spans [60,70).
	var plan: Array[Dictionary] = []
	for i in 10:
		plan.append({"kind": "track", "height": 10})
	var offsets: PackedInt64Array = table._build_slot_offsets(plan)
	var visible: Vector2i = table._visible_slot_range(offsets, 45.0, 20.0)
	assert_eq(visible, Vector2i(4, 7), "Window [45,65) should cover slots 4..6 (slot 4 starts at 40, slot 6 ends at 70)")


func test_visible_slot_range_scroll_exactly_at_slot_boundary() -> void:
	# 4 slots of height 25 = 100 total. Scrolling to exactly y=50 (the exact
	# start of slot 2) must start AT slot 2, not slot 1.
	var plan: Array[Dictionary] = []
	for i in 4:
		plan.append({"kind": "track", "height": 25})
	var offsets: PackedInt64Array = table._build_slot_offsets(plan)
	var visible: Vector2i = table._visible_slot_range(offsets, 50.0, 10.0)
	assert_eq(visible.x, 2)


func test_visible_slot_range_scroll_past_end_clamps() -> void:
	var plan: Array[Dictionary] = []
	for i in 3:
		plan.append({"kind": "track", "height": 30})
	var offsets: PackedInt64Array = table._build_slot_offsets(plan)
	# Scroll position beyond total height (90) should clamp to the last slot,
	# never return an out-of-range or empty result.
	var visible: Vector2i = table._visible_slot_range(offsets, 500.0, 20.0)
	assert_eq(visible.x, 2)
	assert_eq(visible.y, 3)


func test_unknown_bucket_sinks_last_and_still_a_single_header_slot() -> void:
	var rows: Array[Dictionary] = [_row("a", "Artist A", "Album A")]
	rows[0].artist_source = "unknown"
	var plan: Array[Dictionary] = table._build_slot_plan(rows, TrackIndex.GROUP_ARTIST)
	assert_eq(plan.size(), 1)
	assert_eq(plan[0].kind, "header")
	assert_eq(plan[0].header, TrackIndex.UNKNOWN_HEADER)


# ---------------------------------------------------------------------------
# End-to-end pool/rebind behavior — the actual recycling mechanism, not just
# the pure slot-plan math above.
# ---------------------------------------------------------------------------

func _many_rows(count: int) -> Array[Dictionary]:
	var rows: Array[Dictionary] = []
	for i in count:
		rows.append(_row("href_%03d" % i, "Artist", "Album"))
	return rows


func test_pool_stays_small_regardless_of_flat_row_count() -> void:
	table._scroll.size = Vector2(300, 200)
	table.set_rows(_many_rows(500))
	# A 200px viewport at ~28-34px/row shows well under 20 rows — the pool
	# must stay proportional to that, not to the 500 rows in the data.
	assert_true(table._track_pool.size() < 20, "Pool should stay near viewport size (%d), not dataset size" % table._track_pool.size())
	assert_eq(table._track_pool.size(), table._bound_cells.size(), "Every pooled track entry should be a bound, visible row")


## ScrollContainer clamps scroll_vertical against its own scrollbar's
## max_value, which it only recomputes on a deferred NOTIFICATION_SORT_CHILDREN
## — a frame must pass after content-size/scroll changes before the new
## scroll_vertical actually sticks, or it silently re-clamps back to 0.
func _force_scroll(y: float) -> void:
	table._scroll.scroll_vertical = y
	await get_tree().process_frame
	await get_tree().process_frame
	table._rebind_visible()


func test_scrolling_rebinds_the_same_pool_control_to_different_content() -> void:
	table._scroll.size = Vector2(300, 200)
	table.set_rows(_many_rows(200))
	await get_tree().process_frame
	var first_entry: Dictionary = table._track_pool[0]
	var first_href: String = first_entry.href
	assert_eq(first_href, "href_000", "Top of an unscrolled flat list should bind the first row")

	await _force_scroll(table._slot_offsets[50])

	# Same pool Control object (identity), now bound to different content —
	# this is the actual recycling behavior, and the row's live properties
	# (href/title) must have moved with it, not lagged behind.
	assert_eq(table._track_pool[0], first_entry, "Should be the exact same pool entry (recycled), not a new one")
	assert_ne(first_entry.href, first_href, "Recycled entry should now be bound to different content")
	assert_eq(first_entry.shell.button.href, first_entry.href, "The drag/context-menu button's live href must match what's actually bound")


func test_bound_cells_reflects_only_the_currently_visible_window() -> void:
	table._scroll.size = Vector2(300, 200)
	table.set_rows(_many_rows(200))
	await get_tree().process_frame
	assert_true(table._bound_cells.has("href_000"), "Top row should be bound at scroll position 0")
	assert_false(table._bound_cells.has("href_100"), "A row 100 slots down should not be bound while scrolled to the top")

	await _force_scroll(table._slot_offsets[100])
	assert_false(table._bound_cells.has("href_000"), "Scrolling away should drop the old row from _bound_cells")
	assert_true(table._bound_cells.has("href_100"), "The newly-scrolled-to row should now be bound")


func test_group_hrefs_populated_per_header_regardless_of_expand_state() -> void:
	table._group_btn.select(table._group_keys.find(TrackIndex.GROUP_ARTIST))
	table.set_rows([
		_row("a", "Artist A", "Album A"), _row("b", "Artist A", "Album A"),
		_row("c", "Artist B", "Album B"),
	])
	# Neither group is expanded, but _group_hrefs must still know every
	# member — a collapsed group has no track Controls to derive this from.
	assert_eq(table._group_hrefs.get("Artist A", []), ["a", "b"])
	assert_eq(table._group_hrefs.get("Artist B", []), ["c"])


func test_toggle_checked_group_selects_and_deselects_all_members() -> void:
	table._group_btn.select(table._group_keys.find(TrackIndex.GROUP_ARTIST))
	table.set_rows([
		_row("a", "Artist A", "Album A"), _row("b", "Artist A", "Album A"),
		_row("c", "Artist B", "Album B"),
	])
	table._enter_selection()
	table._checked.clear()

	table._toggle_checked_group("Artist A")
	assert_true(table._checked.has("a"))
	assert_true(table._checked.has("b"))
	assert_false(table._checked.has("c"), "Selecting one group must not touch another group's tracks")

	# Pressing the same (fully-checked) group again should deselect it.
	table._toggle_checked_group("Artist A")
	assert_false(table._checked.has("a"))
	assert_false(table._checked.has("b"))


func test_checkbox_state_survives_scrolling_away_and_back() -> void:
	table._scroll.size = Vector2(300, 200)
	table.set_rows(_many_rows(200))
	await get_tree().process_frame
	table._enter_selection()
	table._checked.clear()
	table._checked["href_100"] = true

	# Scroll to bring href_100 into view — its checkbox must reflect _checked,
	# not whatever the recycled Control's PREVIOUS binding happened to show.
	await _force_scroll(table._slot_offsets[100])
	assert_true(table._bound_cells.has("href_100"), "href_100 should be bound after scrolling to it")
	var entry: Dictionary = table._bound_cells.get("href_100", {})
	assert_true(entry.get("checkbox", null) != null and entry.checkbox.visible, "Checkbox should be visible in selection mode")
	assert_true(table._row_checks.has("href_100"), "Currently-bound checked row should be tracked in _row_checks")

## playlist_popup.gd
## Script controller for the mobile playlists popover.
## Handles dynamic styling, list rendering, position calculations, and playlist creation.
extends Control

@onready var backdrop: Control = $Backdrop
@onready var popup_panel: PanelContainer = $PopupPanel
@onready var title_label: Label = $PopupPanel/Margin/VBox/Header/TitleLabel
@onready var add_btn: Button = $PopupPanel/Margin/VBox/Header/AddBtn
@onready var close_btn: Button = $PopupPanel/Margin/VBox/Header/CloseBtn
@onready var list_container: VBoxContainer = $PopupPanel/Margin/VBox/Scroll/ListContainer
@onready var new_playlist_input: LineEdit = $PopupPanel/Margin/VBox/NewPlaylistInput

var _anchor_button: Button = null
var _creation_committed := false
var add_folder_btn: Button = null

func _ready() -> void:
	visible = false

	backdrop.gui_input.connect(_on_backdrop_gui_input)
	add_btn.pressed.connect(_on_add_btn_pressed)
	close_btn.pressed.connect(_on_close_btn_pressed)

	# Mobile has no sidebar, so this popup is the only reachable place to
	# create a folder on that layout too (§14.6). Mirrors the sidebar's
	# add_folder_btn: same "▤+" glyph composed from two already-proven
	# characters, inserted next to the existing "+" rather than a new scene node.
	add_folder_btn = Button.new()
	add_folder_btn.text = "▤+"
	add_folder_btn.custom_minimum_size = Vector2(32, 32)
	add_folder_btn.tooltip_text = "New Folder"
	var header_hbox := add_btn.get_parent()
	header_hbox.add_child(add_folder_btn)
	header_hbox.move_child(add_folder_btn, add_btn.get_index() + 1)
	add_folder_btn.pressed.connect(_on_add_folder_btn_pressed)

	new_playlist_input.text_submitted.connect(_on_new_playlist_submitted)
	new_playlist_input.focus_exited.connect(_on_new_playlist_focus_exited)

	if PlaylistService:
		PlaylistService.playlist_created.connect(func(_p): call_deferred(&"_rebuild_list"))
		PlaylistService.playlist_deleted.connect(func(_id): call_deferred(&"_rebuild_list"))
		PlaylistService.playlists_loaded.connect(func(): call_deferred(&"_rebuild_list"))
		PlaylistService.playlist_folder_changed.connect(func(_id, _folder_id): call_deferred(&"_rebuild_list"))

	if PlaylistFolderService:
		PlaylistFolderService.folder_created.connect(func(_f): call_deferred(&"_rebuild_list"))
		PlaylistFolderService.folder_deleted.connect(func(_id): call_deferred(&"_rebuild_list"))
		PlaylistFolderService.folder_renamed.connect(func(_id, _name): call_deferred(&"_rebuild_list"))
		PlaylistFolderService.folders_loaded.connect(func(): call_deferred(&"_rebuild_list"))

	# Mobile has no sidebar, so this popup is the ONLY way to reach Dynamic
	# Groups on that layout — without this, groups built on desktop would be
	# permanently unreachable on a phone (§14.6: every affordance needs a
	# narrow/touch path, not just a nicer one).
	if DynamicGroupService:
		DynamicGroupService.group_created.connect(func(_g): call_deferred(&"_rebuild_list"))
		DynamicGroupService.group_deleted.connect(func(_id): call_deferred(&"_rebuild_list"))
		DynamicGroupService.groups_loaded.connect(func(): call_deferred(&"_rebuild_list"))

	ThemeManager.theme_changed.connect(_apply_styles)
	_apply_styles()

func _apply_styles() -> void:
	if not is_inside_tree():
		return
	var theme = ThemeManager.current_theme
	
	# Apply glass panel styling
	popup_panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD, 0.95))
	
	# Style title
	title_label.add_theme_font_override("font", theme.font_ui)
	title_label.add_theme_font_size_override("font_size", theme.TYPE_SM)
	title_label.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	
	# Style buttons
	for btn in [add_btn, add_folder_btn, close_btn]:
		btn.add_theme_font_override("font", theme.font_ui)
		btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
		btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
		btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
		btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
		btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
		btn.add_theme_color_override("font_hover_color", theme.TEXT_SECONDARY)
	
	# Style LineEdit
	new_playlist_input.add_theme_stylebox_override("normal", ThemeManager.make_glass_panel(theme.RADIUS_SM, 0.3))
	new_playlist_input.add_theme_font_override("font", theme.font_ui)
	new_playlist_input.add_theme_font_size_override("font_size", theme.TYPE_SM)
	new_playlist_input.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	
	_rebuild_list()

func _rebuild_list() -> void:
	if not is_inside_tree():
		return

	# Clear existing children
	for child in list_container.get_children():
		child.queue_free()

	if not PlaylistService:
		return

	var all_playlists = PlaylistService.get_playlists()
	var theme = ThemeManager.current_theme

	# Folders group their member playlists, always shown expanded — the
	# popup is a reachability path (§14.6), not a full editing surface, so
	# it skips collapse state and drag-to-file rather than duplicating the
	# sidebar's interaction model on a much smaller surface.
	if PlaylistFolderService:
		for folder in PlaylistFolderService.get_playlist_folders():
			_add_folder_label(folder, theme)
			for playlist in all_playlists:
				if playlist.get("folder_id", "") == folder.id:
					_add_playlist_row(playlist, theme, true)

	for playlist in all_playlists:
		if (playlist.get("folder_id", "") as String).is_empty():
			_add_playlist_row(playlist, theme, false)

	if DynamicGroupService:
		_rebuild_dynamic_groups_section(theme)

func _add_folder_label(folder: Dictionary, theme) -> void:
	var label = Label.new()
	label.text = "  ▤  " + folder.name
	label.custom_minimum_size.y = ThemeManager.min_touch_height(int(theme.TOUCH_TARGET_MIN * 0.5))
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.add_theme_font_override("font", theme.font_ui)
	label.add_theme_font_size_override("font_size", theme.TYPE_SM)
	label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	list_container.add_child(label)

func _add_playlist_row(playlist: Dictionary, theme, indented: bool) -> void:
	var btn = Button.new()
	btn.text = ("        ♪  " if indented else "    ♪  ") + playlist.name
	btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	btn.custom_minimum_size.y = ThemeManager.min_touch_height(
		int(theme.TOUCH_TARGET_MIN * 0.75))

	btn.add_theme_font_override("font", theme.font_ui)
	btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

	var is_active = (NavManager.current_screen == "playlist" and PlaylistService.active_playlist_id == playlist.id)
	if is_active:
		btn.add_theme_stylebox_override("normal", ThemeManager.make_nav_item_active())
		btn.add_theme_color_override("font_color", theme.ACCENT_CORE)
	else:
		btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
		btn.add_theme_color_override("font_hover_color", theme.TEXT_SECONDARY)

	btn.pressed.connect(func():
		PlaylistService.active_playlist_id = playlist.id
		NavManager.navigate_to("playlist")
		hide_popup()
	)

	list_container.add_child(btn)

func _rebuild_dynamic_groups_section(theme) -> void:
	var sep = HSeparator.new()
	list_container.add_child(sep)

	var section_header = HBoxContainer.new()
	list_container.add_child(section_header)

	var section_label = Label.new()
	section_label.text = "  Dynamic Groups"
	section_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	section_label.add_theme_font_override("font", theme.font_ui)
	section_label.add_theme_font_size_override("font_size", theme.TYPE_SM)
	section_label.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	section_header.add_child(section_label)

	var add_group_btn = Button.new()
	add_group_btn.text = " + "
	add_group_btn.custom_minimum_size = ThemeManager.min_touch_size(Vector2(32, 32))
	add_group_btn.add_theme_font_override("font", theme.font_ui)
	add_group_btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
	add_group_btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
	add_group_btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
	add_group_btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
	add_group_btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	add_group_btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	add_group_btn.add_theme_color_override("font_hover_color", theme.TEXT_SECONDARY)
	section_header.add_child(add_group_btn)
	add_group_btn.pressed.connect(_on_add_group_btn_pressed)

	for group in DynamicGroupService.get_dynamic_groups():
		var btn = Button.new()
		btn.text = "    ⚡  " + group.name
		btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
		btn.custom_minimum_size.y = ThemeManager.min_touch_height(
			int(theme.TOUCH_TARGET_MIN * 0.75))

		btn.add_theme_font_override("font", theme.font_ui)
		btn.add_theme_font_size_override("font_size", theme.TYPE_SM)
		btn.add_theme_stylebox_override("normal", ThemeManager.make_transparent())
		btn.add_theme_stylebox_override("hover", ThemeManager.make_nav_item_hover())
		btn.add_theme_stylebox_override("pressed", ThemeManager.make_nav_item_hover())
		btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

		var is_active = (NavManager.current_screen == "dynamic_group" and DynamicGroupService.active_group_id == group.id)
		if is_active:
			btn.add_theme_stylebox_override("normal", ThemeManager.make_nav_item_active())
			btn.add_theme_color_override("font_color", theme.ACCENT_CORE)
		else:
			btn.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
			btn.add_theme_color_override("font_hover_color", theme.TEXT_SECONDARY)

		btn.pressed.connect(func():
			DynamicGroupService.active_group_id = group.id
			NavManager.navigate_to("dynamic_group")
			hide_popup()
		)

		list_container.add_child(btn)

func toggle_popup(anchor_button: Button) -> void:
	if visible:
		hide_popup()
	else:
		show_popup(anchor_button)

func show_popup(anchor_button: Button) -> void:
	_anchor_button = anchor_button
	new_playlist_input.visible = false
	visible = true
	_rebuild_list()
	
	# Wait for size calculation
	await get_tree().process_frame
	_position_popup()

func hide_popup() -> void:
	visible = false

func _position_popup() -> void:
	if not _anchor_button or not is_inside_tree():
		return
		
	var btn_pos = _anchor_button.global_position
	var btn_size = _anchor_button.size
	var popup_w = popup_panel.size.x
	var popup_h = popup_panel.size.y
	
	var target_x = btn_pos.x + (btn_size.x - popup_w) / 2.0
	var viewport_w = get_viewport_rect().size.x
	target_x = clamp(target_x, 8.0, viewport_w - popup_w - 8.0)
	
	var target_y = btn_pos.y - popup_h - 8.0
	popup_panel.global_position = Vector2(target_x, target_y)

func _on_backdrop_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		var local_click = popup_panel.get_local_mouse_position()
		var rect = Rect2(Vector2.ZERO, popup_panel.size)
		if not rect.has_point(local_click):
			hide_popup()
			get_viewport().set_input_as_handled()

func _on_add_btn_pressed() -> void:
	new_playlist_input.placeholder_text = "New Playlist..."
	new_playlist_input.set_meta("creating", "playlist")
	new_playlist_input.text = ""
	new_playlist_input.visible = !new_playlist_input.visible
	if new_playlist_input.visible:
		new_playlist_input.grab_focus()

	await get_tree().process_frame
	_position_popup()

## Shares the same inline LineEdit as playlist creation — a "creating" meta
## flag on the input tells _on_new_playlist_submitted which service to call.
func _on_add_group_btn_pressed() -> void:
	new_playlist_input.placeholder_text = "New Group..."
	new_playlist_input.set_meta("creating", "group")
	new_playlist_input.text = ""
	new_playlist_input.visible = !new_playlist_input.visible
	if new_playlist_input.visible:
		new_playlist_input.grab_focus()

	await get_tree().process_frame
	_position_popup()

## Shares the same inline LineEdit via the "creating" meta flag too.
func _on_add_folder_btn_pressed() -> void:
	new_playlist_input.placeholder_text = "New Folder..."
	new_playlist_input.set_meta("creating", "folder")
	new_playlist_input.text = ""
	new_playlist_input.visible = !new_playlist_input.visible
	if new_playlist_input.visible:
		new_playlist_input.grab_focus()

	await get_tree().process_frame
	_position_popup()

func _on_close_btn_pressed() -> void:
	hide_popup()

func _on_new_playlist_submitted(new_text: String) -> void:
	if _creation_committed:
		return
	_creation_committed = true

	var clean = new_text.strip_edges()
	if not clean.is_empty():
		var creating: String = new_playlist_input.get_meta("creating", "playlist")
		if creating == "group":
			if DynamicGroupService:
				DynamicGroupService.create_group(clean)
		elif creating == "folder":
			if PlaylistFolderService:
				PlaylistFolderService.create_folder(clean)
		elif PlaylistService:
			PlaylistService.create_playlist(clean)

	new_playlist_input.visible = false
	_creation_committed = false

	await get_tree().process_frame
	_position_popup()

func _on_new_playlist_focus_exited() -> void:
	if new_playlist_input.visible:
		_on_new_playlist_submitted(new_playlist_input.text)

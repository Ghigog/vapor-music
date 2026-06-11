extends Control
## vibe_screen.gd
## DJ Vibe Workbench controller. Shows now-playing analyzer tags
## and lists compatible runner-up selections.

@onready var heading: Label = $VBox/Header/HBox/Heading
@onready var analysis_status: Label = $VBox/Header/HBox/AnalysisStatus
@onready var track_title: Label = $VBox/NowPlayingCard/VBox/TrackTitle
@onready var artist_label: Label = $VBox/NowPlayingCard/VBox/ArtistLabel

# Metadata labels/badges
@onready var badge_key: Label = $VBox/NowPlayingCard/VBox/BadgesHBox/KeyBadge
@onready var badge_bpm: Label = $VBox/NowPlayingCard/VBox/BadgesHBox/BpmBadge
@onready var badge_energy: Label = $VBox/NowPlayingCard/VBox/BadgesHBox/EnergyBadge
@onready var badge_genre: Label = $VBox/NowPlayingCard/VBox/BadgesHBox/GenreBadge

# Transition details
@onready var transition_label: Label = $VBox/TransitionCard/VBox/TransitionLabel

# Runner-up containers
@onready var runner_ups_list: VBoxContainer = $VBox/RunnerUpsSection/VBox/RunnerUpsList

# Help Modal Elements
@onready var help_button: Button = $VBox/Header/HBox/HelpButton
@onready var help_modal: Control = $HelpModal
@onready var help_modal_panel: PanelContainer = $HelpModal/Panel
@onready var help_modal_title: Label = $HelpModal/Panel/VBox/Header/Title
@onready var help_modal_close: Button = $HelpModal/Panel/VBox/Header/CloseIcon
@onready var help_modal_text: RichTextLabel = $HelpModal/Panel/VBox/Scroll/ContentText

var _runner_ups: Array = []

func _ready() -> void:
	_apply_styles()
	ThemeManager.theme_changed.connect(_apply_styles)
	
	# Connect to AudioManager signals
	AudioManager.track_changed.connect(_on_track_changed)
	AudioManager.playback_toggled.connect(_on_playback_toggled)
	
	if help_button:
		help_button.pressed.connect(_on_help_button_pressed)
	if help_modal_close:
		help_modal_close.pressed.connect(_on_help_close_pressed)
	
	if AudioManager.has_signal("transition_started"):
		AudioManager.connect("transition_started", _on_transition_started)
	if AudioManager.has_signal("transition_completed"):
		AudioManager.connect("transition_completed", _on_transition_completed)
		
	var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
	if is_instance_valid(audio_analyzer):
		audio_analyzer.analysis_completed.connect(_on_analysis_completed)
		
	if is_instance_valid(MetadataService):
		MetadataService.metadata_updated.connect(_on_metadata_updated)
		
	if is_instance_valid(WebDAVService) and WebDAVService.has_signal("library_scanned"):
		WebDAVService.library_scanned.connect(func(files): _refresh_display())
		
	if AudioManager.has_signal("smart_mixing_toggled"):
		AudioManager.smart_mixing_toggled.connect(func(enabled): _refresh_display())
		
	_refresh_display()

func _apply_styles() -> void:
	if not is_inside_tree():
		return
	var theme = ThemeManager.current_theme
	
	heading.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	heading.add_theme_font_override("font", theme.font_display)
	heading.add_theme_font_size_override("font_size", theme.TYPE_LG)
	
	analysis_status.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	analysis_status.add_theme_font_override("font", theme.font_ui)
	analysis_status.add_theme_font_size_override("font_size", theme.TYPE_SM)
	
	track_title.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	track_title.add_theme_font_override("font", theme.font_display)
	track_title.add_theme_font_size_override("font_size", theme.TYPE_MD)
	
	artist_label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	artist_label.add_theme_font_override("font", theme.font_ui)
	artist_label.add_theme_font_size_override("font_size", theme.TYPE_SM)
	
	# Style badges
	for badge in [badge_key, badge_bpm, badge_energy, badge_genre]:
		badge.add_theme_color_override("font_color", theme.ACCENT_CORE)
		badge.add_theme_font_override("font", theme.font_ui)
		badge.add_theme_font_size_override("font_size", theme.TYPE_XS)
		var sb := StyleBoxFlat.new()
		sb.bg_color = theme.GLASS_TINT
		sb.border_color = theme.GLASS_BORDER_SUBTLE
		sb.set_border_width_all(1)
		sb.set_corner_radius_all(theme.RADIUS_SM)
		sb.content_margin_left = 8
		sb.content_margin_right = 8
		sb.content_margin_top = 4
		sb.content_margin_bottom = 4
		badge.add_theme_stylebox_override("normal", sb)

	# Cards panel styles
	for card_path in ["VBox/NowPlayingCard", "VBox/TransitionCard", "VBox/RunnerUpsSection"]:
		var card = get_node_or_null(card_path)
		if card is PanelContainer:
			card.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD))

	# Style help button
	if help_button:
		help_button.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
		help_button.add_theme_font_override("font", theme.font_ui)
		help_button.add_theme_font_size_override("font_size", theme.TYPE_SM)
		
		var sb_btn_normal = StyleBoxFlat.new()
		sb_btn_normal.bg_color = theme.GLASS_TINT
		sb_btn_normal.border_color = theme.GLASS_BORDER_SUBTLE
		sb_btn_normal.set_border_width_all(1)
		sb_btn_normal.set_corner_radius_all(theme.RADIUS_SM)
		sb_btn_normal.content_margin_left = 12
		sb_btn_normal.content_margin_right = 12
		sb_btn_normal.content_margin_top = 4
		sb_btn_normal.content_margin_bottom = 4
		
		var sb_btn_hover = StyleBoxFlat.new()
		sb_btn_hover.bg_color = theme.ACCENT_SURFACE
		sb_btn_hover.border_color = theme.ACCENT_BRIGHT
		sb_btn_hover.set_border_width_all(1)
		sb_btn_hover.set_corner_radius_all(theme.RADIUS_SM)
		sb_btn_hover.content_margin_left = 12
		sb_btn_hover.content_margin_right = 12
		sb_btn_hover.content_margin_top = 4
		sb_btn_hover.content_margin_bottom = 4
		
		help_button.add_theme_stylebox_override("normal", sb_btn_normal)
		help_button.add_theme_stylebox_override("hover", sb_btn_hover)
		help_button.add_theme_stylebox_override("pressed", sb_btn_hover)
		help_button.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

	# Style help modal
	if help_modal_panel:
		help_modal_panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_LG, 0.9))
		
	if help_modal_title:
		help_modal_title.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
		help_modal_title.add_theme_font_override("font", theme.font_display)
		help_modal_title.add_theme_font_size_override("font_size", theme.TYPE_LG)
		
	if help_modal_close:
		help_modal_close.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
		help_modal_close.add_theme_font_override("font", theme.font_ui)
		help_modal_close.add_theme_font_size_override("font_size", theme.TYPE_SM)
		
		var sb_close_normal = StyleBoxFlat.new()
		sb_close_normal.bg_color = Color(0, 0, 0, 0)
		sb_close_normal.content_margin_left = 8
		sb_close_normal.content_margin_right = 8
		
		var sb_close_hover = StyleBoxFlat.new()
		sb_close_hover.bg_color = theme.GLASS_TINT
		sb_close_hover.border_color = theme.SEMANTIC_ERROR
		sb_close_hover.set_border_width_all(1)
		sb_close_hover.set_corner_radius_all(theme.RADIUS_SM)
		sb_close_hover.content_margin_left = 8
		sb_close_hover.content_margin_right = 8
		
		help_modal_close.add_theme_stylebox_override("normal", sb_close_normal)
		help_modal_close.add_theme_stylebox_override("hover", sb_close_hover)
		help_modal_close.add_theme_stylebox_override("pressed", sb_close_hover)
		help_modal_close.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		
	if help_modal_text:
		help_modal_text.add_theme_color_override("default_color", theme.TEXT_SECONDARY)
		help_modal_text.add_theme_font_override("normal_font", theme.font_ui)
		help_modal_text.add_theme_font_override("bold_font", theme.font_display)
		help_modal_text.add_theme_font_override("mono_font", theme.font_mono)
		help_modal_text.add_theme_font_size_override("normal_font_size", theme.TYPE_SM)
		help_modal_text.add_theme_font_size_override("bold_font_size", theme.TYPE_SM)
		help_modal_text.add_theme_font_size_override("mono_font_size", theme.TYPE_XS)

	_style_runner_ups()

func _on_track_changed(_track_name: String) -> void:
	_refresh_display()

func _on_playback_toggled(_is_playing: bool) -> void:
	_refresh_display()

func _on_transition_started(next_track: String, transition_type: String) -> void:
	transition_label.text = "Blending → %s  (%s)" % [
		next_track.get_file().uri_decode().get_basename(),
		transition_type
	]

func _on_transition_completed(_track: String) -> void:
	_refresh_display()

func _on_analysis_completed(href: String, _results: Dictionary) -> void:
	_refresh_display()

func _on_metadata_updated(href: String, _metadata: Dictionary) -> void:
	var active_index = AudioManager.current_track_index
	var playlist = AudioManager.current_playlist
	if active_index != -1 and not playlist.is_empty() and playlist[active_index] == href:
		_refresh_display()

func _update_analysis_status() -> void:
	if not is_instance_valid(WebDAVService) or not is_instance_valid(MetadataService):
		return
		
	var all_tracks = WebDAVService.get("scanned_files")
	if all_tracks == null or all_tracks.is_empty():
		all_tracks = AudioManager.current_playlist
		
	if all_tracks.is_empty():
		analysis_status.text = "No tracks loaded"
		return
		
	var total_count = all_tracks.size()
	var analyzed_count = 0
	var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
	if is_instance_valid(audio_analyzer):
		analyzed_count = audio_analyzer.get_ready_tracks_count(all_tracks)
	else:
		for href in all_tracks:
			var meta = MetadataService.get_cached_metadata(href)
			if not meta.is_empty() and meta.get("bpm", 0.0) > 0.0:
				analyzed_count += 1
			
	if analyzed_count == total_count:
		analysis_status.text = "✓ All %d tracks ready" % total_count
	else:
		analysis_status.text = "⏳ Analyzing library... %d / %d ready" % [analyzed_count, total_count]

func _refresh_display() -> void:
	if not is_inside_tree():
		return
		
	_update_analysis_status()
		
	var active_index = AudioManager.current_track_index
	var playlist = AudioManager.current_playlist
	
	if active_index == -1 or playlist.is_empty() or active_index >= playlist.size():
		track_title.text = "DJ Standby"
		artist_label.text = "Queue a track to begin the vibe session."
		badge_key.text = "--"
		badge_bpm.text = "-- BPM"
		badge_energy.text = "Energy: --%"
		badge_genre.text = "Genre: --"
		transition_label.text = "AI DJ: Standby"
		_clear_runner_ups()
		return
		
	var current_href = playlist[active_index]
	var meta = {}
	if is_instance_valid(MetadataService):
		meta = MetadataService.get_cached_metadata(current_href)
		
	var info = MetadataService.parse_track_info(current_href)
	track_title.text = info.track
	artist_label.text = info.artist
	
	var analyzed = not meta.is_empty() and meta.get("bpm", 0.0) > 0.0
	
	if analyzed:
		var key = meta.get("musical_key", "")
		badge_key.text = key if not key.is_empty() else "??"
		badge_bpm.text = "%d BPM" % int(meta.get("bpm", 0.0))
		badge_energy.text = "Energy: %d%%" % int(meta.get("energy_level", 0.0) * 100.0)
		var genre = meta.get("genre", "Unknown")
		badge_genre.text = genre if genre != "Unknown" else "--"
	else:
		badge_key.text = "..."
		badge_bpm.text = "... BPM"
		badge_energy.text = "Energy: ..."
		badge_genre.text = "--"
	
	# Recalculate runner-ups (will show analyzing state if current track not ready)
	_calculate_runner_ups(current_href)
	
	if not AudioManager.is_transitioning:
		var next_href = AudioManager.get_next_track_href()
		if not next_href.is_empty():
			var info_next = MetadataService.parse_track_info(next_href)
			var transition_type = AudioManager.upcoming_transition_type
			transition_label.text = "Intended: Blend to %s via %s" % [info_next.track, transition_type]
		else:
			transition_label.text = "AI DJ: Standby"

func _clear_runner_ups() -> void:
	for child in runner_ups_list.get_children():
		child.queue_free()
	_runner_ups = []

func _show_runner_up_message(msg: String) -> void:
	var label := Label.new()
	label.text = msg
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
	label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	runner_ups_list.add_child(label)

func _calculate_runner_ups(current_href: String) -> void:
	_clear_runner_ups()
	
	var playlist = AudioManager.current_playlist
	if playlist.size() <= 1:
		_show_runner_up_message("Not enough tracks in the active playlist.")
		return
		
	# Count analyzed library tracks (excluding current)
	var total_count := playlist.size() - 1
	var analyzed_count := 0
	for href in playlist:
		if href == current_href:
			continue
		var meta = {}
		if is_instance_valid(MetadataService):
			meta = MetadataService.get_cached_metadata(href)
		if not meta.is_empty() and meta.get("bpm", 0.0) > 0.0:
			analyzed_count += 1
			
	# Don't show runner-ups until the current track has been analyzed
	var current_meta = {}
	if is_instance_valid(MetadataService):
		current_meta = MetadataService.get_cached_metadata(current_href)
		
	var current_analyzed = not current_meta.is_empty() and current_meta.get("bpm", 0.0) > 0.0
	if not current_analyzed:
		_show_runner_up_message("Analyzing current track...")
		return
		
	# Use DJPathfinder to calculate smart matches
	var DJPathfinderClass = load("res://scripts/services/dj_pathfinder.gd")
	var smart_matches = DJPathfinderClass.calculate_smart_matches(current_href, playlist, MetadataService)
	
	if smart_matches.is_empty():
		_show_runner_up_message("No compatible matches found.")
		return
		
	_runner_ups = []
	var displayed_matches := []
	for type in ["perfect", "interesting", "creative"]:
		if smart_matches.has(type) and not smart_matches[type].is_empty():
			displayed_matches.append({"type": type, "data": smart_matches[type]})
			_runner_ups.append(smart_matches[type].href)
			
	# Determine the selected track (matches exactly what AudioManager intends to play next)
	var ai_preferred_type = AudioManager.preferred_type_for_current_track
	var target_selected_href = AudioManager.get_next_track_href()
			
	# Render the cards
	for idx in range(displayed_matches.size()):
		var item = displayed_matches[idx]
		var type = item.type
		var match_data = item.data
		var is_selected = (match_data.href == target_selected_href)
		var is_ai_preferred = (type == ai_preferred_type)
		_create_runner_up_card(match_data.href, match_data.meta, match_data.cost, type, is_selected, is_ai_preferred)

func _create_runner_up_card(href: String, meta: Dictionary, cost: float, type: String, is_selected: bool = false, is_ai_preferred: bool = false) -> void:
	var info = MetadataService.parse_track_info(href)
	
	var btn := Button.new()
	btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	btn.custom_minimum_size.y = 52
	btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	
	var container := MarginContainer.new()
	container.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	container.add_theme_constant_override("margin_left", 12)
	container.add_theme_constant_override("margin_right", 12)
	btn.add_child(container)
	
	var hbox := HBoxContainer.new()
	container.add_child(hbox)
	
	var vbox := VBoxContainer.new()
	vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	vbox.alignment = VBoxContainer.ALIGNMENT_CENTER
	hbox.add_child(vbox)
	
	var title_lbl := Label.new()
	title_lbl.text = info.track
	title_lbl.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	vbox.add_child(title_lbl)
	
	var artist_lbl := Label.new()
	artist_lbl.text = info.artist
	artist_lbl.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	vbox.add_child(artist_lbl)
	
	var details_hbox := HBoxContainer.new()
	details_hbox.alignment = HBoxContainer.ALIGNMENT_END
	hbox.add_child(details_hbox)
	
	var key = meta.get("musical_key", "8A")
	if key.is_empty(): key = "8A"
	
	var bpm = meta.get("bpm", 120.0)
	
	var key_bpm_lbl := Label.new()
	key_bpm_lbl.text = "%s  •  %d BPM" % [key, int(bpm)]
	details_hbox.add_child(key_bpm_lbl)
	
	# Determine match label and color based on type
	var type_lbl := Label.new()
	if type == "perfect":
		type_lbl.text = "  Match  "
		type_lbl.add_theme_color_override("font_color", ThemeManager.current_theme.SEMANTIC_SUCCESS)
	elif type == "interesting":
		type_lbl.text = "  Fresh  "
		type_lbl.add_theme_color_override("font_color", ThemeManager.current_theme.AQUA_CORE)
	elif type == "creative":
		type_lbl.text = "  Switch  "
		type_lbl.add_theme_color_override("font_color", ThemeManager.current_theme.SEMANTIC_WARNING)
	details_hbox.add_child(type_lbl)
	
	# If this is the AI preferred choice, add an AI Choice badge!
	if is_ai_preferred:
		var ai_lbl := Label.new()
		ai_lbl.text = "  🤖 AI Choice  "
		ai_lbl.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
		details_hbox.add_child(ai_lbl)
		
	# Apply visual styles to labels inside the card
	var theme = ThemeManager.current_theme
	title_lbl.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	title_lbl.add_theme_font_override("font", theme.font_ui)
	title_lbl.add_theme_font_size_override("font_size", theme.TYPE_SM)
	
	artist_lbl.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	artist_lbl.add_theme_font_override("font", theme.font_ui)
	artist_lbl.add_theme_font_size_override("font_size", theme.TYPE_XS)
	
	key_bpm_lbl.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	key_bpm_lbl.add_theme_font_override("font", theme.font_ui)
	key_bpm_lbl.add_theme_font_size_override("font_size", theme.TYPE_XS)
	
	type_lbl.add_theme_font_override("font", theme.font_ui)
	type_lbl.add_theme_font_size_override("font_size", theme.TYPE_XS)
	
	if is_ai_preferred:
		var ai_lbl = details_hbox.get_child(details_hbox.get_child_count() - 1) as Label
		ai_lbl.add_theme_font_override("font", theme.font_ui)
		ai_lbl.add_theme_font_size_override("font_size", theme.TYPE_XS)
	
	# Click handler: locks track as next override
	btn.pressed.connect(func():
		AudioManager.set("upcoming_track_override", href)
		_refresh_display()
		print("VibeScreen: Selected next track override: ", info.track)
	)
	
	if is_selected:
		var highlight = StyleBoxFlat.new()
		highlight.bg_color = theme.ACCENT_SURFACE
		highlight.border_color = theme.ACCENT_BRIGHT
		highlight.set_border_width_all(1)
		highlight.set_corner_radius_all(theme.RADIUS_SM)
		btn.add_theme_stylebox_override("normal", highlight)
		btn.add_theme_stylebox_override("hover", highlight)
	else:
		_apply_card_button_style(btn, theme)
		
	runner_ups_list.add_child(btn)
 
func _apply_card_button_style(btn: Button, theme) -> void:
	var sb_normal = StyleBoxFlat.new()
	sb_normal.bg_color = theme.BG_ELEVATED
	sb_normal.border_color = theme.GLASS_BORDER_SUBTLE
	sb_normal.set_border_width_all(1)
	sb_normal.set_corner_radius_all(theme.RADIUS_SM)
	
	var sb_hover = StyleBoxFlat.new()
	sb_hover.bg_color = theme.GLASS_TINT
	sb_hover.border_color = theme.ACCENT_CORE
	sb_hover.set_border_width_all(1)
	sb_hover.set_corner_radius_all(theme.RADIUS_SM)
	
	btn.add_theme_stylebox_override("normal", sb_normal)
	btn.add_theme_stylebox_override("hover", sb_hover)
	btn.add_theme_stylebox_override("pressed", sb_hover)
	btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
 
func _style_runner_ups() -> void:
	var theme = ThemeManager.current_theme
	var target_selected_href = AudioManager.get_next_track_href()
			
	for idx in range(runner_ups_list.get_child_count()):
		var child = runner_ups_list.get_child(idx)
		if child is Button:
			var is_selected = false
			if idx < _runner_ups.size() and _runner_ups[idx] == target_selected_href:
				is_selected = true
			if is_selected:
				var highlight = StyleBoxFlat.new()
				highlight.bg_color = theme.ACCENT_SURFACE
				highlight.border_color = theme.ACCENT_BRIGHT
				highlight.set_border_width_all(1)
				highlight.set_corner_radius_all(theme.RADIUS_SM)
				child.add_theme_stylebox_override("normal", highlight)
				child.add_theme_stylebox_override("hover", highlight)
			else:
				_apply_card_button_style(child, theme)

func _on_help_button_pressed() -> void:
	if not help_modal:
		return
	
	help_modal.visible = true
	
	# Load docs/ai_dj_workflow.md dynamically
	var workflow_file_path = "res://docs/ai_dj_workflow.md"
	if FileAccess.file_exists(workflow_file_path):
		var file = FileAccess.open(workflow_file_path, FileAccess.READ)
		if file:
			var content = file.get_as_text()
			file.close()
			if help_modal_text:
				help_modal_text.text = _parse_markdown_to_bbcode(content)
		else:
			if help_modal_text:
				help_modal_text.text = "[color=#F87171]Error opening help document.[/color]"
	else:
		if help_modal_text:
			help_modal_text.text = "[color=#F87171]Help document not found at: res://docs/ai_dj_workflow.md[/color]"

func _on_help_close_pressed() -> void:
	if help_modal:
		help_modal.visible = false

func _parse_markdown_to_bbcode(markdown_text: String) -> String:
	var lines = markdown_text.split("\n")
	var bbcode_lines: Array[String] = []
	
	var in_mermaid = false
	var in_table = false
	var table_rows: Array = []
	
	for i in range(lines.size()):
		var line = lines[i].strip_edges()
		
		# Handle Mermaid block
		if line.begins_with("```mermaid"):
			in_mermaid = true
			bbcode_lines.append("\n[color=#3DD6C8][b]AI Choice Path Cycle:[/b][/color]")
			continue
		elif line.begins_with("```") and in_mermaid:
			in_mermaid = false
			continue
			
		if in_mermaid:
			# Parse mood path visual details
			if line.contains("A[Step 0: Match]") or line.contains("Step 0"):
				bbcode_lines.append("  • Step 0: [b]Match[/b] (Perfect match)")
			elif line.contains("B[Step 1: Fresh]") or line.contains("Step 1"):
				bbcode_lines.append("  • Step 1: [b]Fresh[/b] (Interesting match)")
			elif line.contains("C[Step 2: Match]") or line.contains("Step 2"):
				bbcode_lines.append("  • Step 2: [b]Match[/b] (Perfect match)")
			elif line.contains("D[Step 3: Switch") or line.contains("Step 3"):
				bbcode_lines.append("  • Step 3: [b]Switch[/b] or [b]Fresh[/b] (50/50 split)")
			continue
			
		# Handle Table rows
		if line.begins_with("|") and line.ends_with("|"):
			# Check separator
			var is_separator = true
			for j in range(line.length()):
				var char = line[j]
				if char != '|' and char != '-' and char != ':' and char != ' ':
					is_separator = false
					break
			if is_separator:
				continue
				
			in_table = true
			var cells = line.split("|")
			var row_cells: Array[String] = []
			for j in range(1, cells.size() - 1):
				row_cells.append(cells[j].strip_edges())
			table_rows.append(row_cells)
			continue
		else:
			if in_table:
				if not table_rows.is_empty():
					var num_cols = table_rows[0].size()
					bbcode_lines.append("[table=%d]" % num_cols)
					for row_idx in range(table_rows.size()):
						var row = table_rows[row_idx]
						for cell in row:
							var formatted_cell = _format_inline_markdown(cell)
							if row_idx == 0:
								bbcode_lines.append("[cell][b][color=#9B8EFF]%s[/color][/b][/cell]" % formatted_cell)
							else:
								bbcode_lines.append("[cell]%s[/cell]" % formatted_cell)
					bbcode_lines.append("[/table]\n")
				table_rows.clear()
				in_table = false
		
		# Headers
		if line.begins_with("# "):
			bbcode_lines.append("[font_size=20][b][color=#9B8EFF]%s[/color][/b][/font_size]\n" % _format_inline_markdown(line.substr(2)))
		elif line.begins_with("## "):
			bbcode_lines.append("\n[font_size=16][b][color=#9B8EFF]%s[/color][/b][/font_size]\n" % _format_inline_markdown(line.substr(3)))
		elif line.begins_with("### "):
			bbcode_lines.append("\n[font_size=14][b]%s[/b][/font_size]\n" % _format_inline_markdown(line.substr(4)))
		elif line == "---":
			bbcode_lines.append("[center][color=#9494FF]⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯[/color][/center]\n")
		elif line.begins_with("- ") or line.begins_with("* "):
			bbcode_lines.append("• %s" % _format_inline_markdown(line.substr(2)))
		else:
			if not line.is_empty():
				bbcode_lines.append(_format_inline_markdown(line))
			else:
				bbcode_lines.append("")
				
	if in_table and not table_rows.is_empty():
		var num_cols = table_rows[0].size()
		bbcode_lines.append("[table=%d]" % num_cols)
		for row_idx in range(table_rows.size()):
			var row = table_rows[row_idx]
			for cell in row:
				var formatted_cell = _format_inline_markdown(cell)
				if row_idx == 0:
					bbcode_lines.append("[cell][b][color=#9B8EFF]%s[/color][/b][/cell]" % formatted_cell)
				else:
					bbcode_lines.append("[cell]%s[/cell]" % formatted_cell)
		bbcode_lines.append("[/table]\n")
		
	return "\n".join(bbcode_lines)

func _format_inline_markdown(text: String) -> String:
	var res = text
	
	# Replace `code` with styled mono font using Godot's [code] tag
	var code_regex = RegEx.new()
	code_regex.compile("`([^`]+)`")
	res = code_regex.sub(res, "[color=#3DD6C8][code]$1[/code][/color]", true)
	
	# Replace **bold** with [b]bold[/b]
	var bold_regex = RegEx.new()
	bold_regex.compile("\\*\\*([^\\*]+)\\*\\*")
	res = bold_regex.sub(res, "[b]$1[/b]", true)
	
	return res

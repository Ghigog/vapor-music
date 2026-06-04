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

var _runner_ups: Array = []

func _ready() -> void:
	_apply_styles()
	ThemeManager.theme_changed.connect(_apply_styles)
	
	# Connect to AudioManager signals
	AudioManager.track_changed.connect(_on_track_changed)
	AudioManager.playback_toggled.connect(_on_playback_toggled)
	
	if AudioManager.has_signal("transition_started"):
		AudioManager.connect("transition_started", _on_transition_started)
	if AudioManager.has_signal("transition_completed"):
		AudioManager.connect("transition_completed", _on_transition_completed)
		
	var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
	if is_instance_valid(audio_analyzer):
		audio_analyzer.analysis_completed.connect(_on_analysis_completed)
		
	if is_instance_valid(WebDAVService) and WebDAVService.has_signal("library_scanned"):
		WebDAVService.library_scanned.connect(func(files): _refresh_display())
		
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
	transition_label.text = "Ready — next blend selected by AI"

func _on_analysis_completed(href: String, _results: Dictionary) -> void:
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
	for href in all_tracks:
		var meta = MetadataService.get_cached_metadata(href)
		if not meta.is_empty() and meta.get("bpm", 0.0) > 0.0:
			analyzed_count += 1
			
	if analyzed_count == total_count:
		analysis_status.text = "✓ All %d tracks analyzed" % total_count
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
		_show_runner_up_message("⏳  Analyzing library... %d / %d tracks ready." % [analyzed_count, total_count])
		return
		
	# Only rank tracks that have been analyzed — skip unanalyzed ones
	var scored_tracks = []
	var DJPathfinderClass = load("res://scripts/services/dj_pathfinder.gd")
	for href in playlist:
		if href == current_href:
			continue
		var meta = {}
		if is_instance_valid(MetadataService):
			meta = MetadataService.get_cached_metadata(href)
		# Skip tracks with no analysis data — defaults would produce meaningless rankings
		if meta.is_empty() or meta.get("bpm", 0.0) <= 0.0:
			continue
		var cost = DJPathfinderClass.calculate_transition_cost(current_meta, meta)
		scored_tracks.append({"href": href, "cost": cost, "meta": meta})
		
	if scored_tracks.is_empty():
		_show_runner_up_message("⏳  Analyzing library... %d / %d tracks ready." % [analyzed_count, total_count])
		return
		
	scored_tracks.sort_custom(func(a, b): return a.cost < b.cost)
	
	# Take top 3
	var count = min(3, scored_tracks.size())
	for idx in range(count):
		var match_data = scored_tracks[idx]
		_runner_ups.append(match_data.href)

	# Auto-choose the best match if no override is set or if the current override is not in the list
	var current_override = AudioManager.upcoming_track_override
	var select_idx = 0
	if not current_override.is_empty() and _runner_ups.has(current_override):
		select_idx = _runner_ups.find(current_override)
	else:
		if not _runner_ups.is_empty():
			AudioManager.upcoming_track_override = _runner_ups[0]
			select_idx = 0

	for idx in range(count):
		var match_data = scored_tracks[idx]
		_create_runner_up_card(match_data.href, match_data.meta, match_data.cost, idx == select_idx)

func _create_runner_up_card(href: String, meta: Dictionary, cost: float, is_selected: bool = false) -> void:
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
	
	# Determine match grade
	var match_lbl := Label.new()
	if cost < 5.0:
		match_lbl.text = "  Perfect Match  "
		match_lbl.add_theme_color_override("font_color", ThemeManager.current_theme.SEMANTIC_SUCCESS)
	elif cost < 10.0:
		match_lbl.text = "  Good Blend  "
		match_lbl.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_CORE)
	else:
		match_lbl.text = "  Transition  "
		match_lbl.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
	details_hbox.add_child(match_lbl)
	
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
	
	match_lbl.add_theme_font_override("font", theme.font_ui)
	match_lbl.add_theme_font_size_override("font_size", theme.TYPE_XS)
	
	# Click handler: locks track as next override
	btn.pressed.connect(func():
		AudioManager.set("upcoming_track_override", href)
		# Stylize button as active/highlighted
		var highlight = StyleBoxFlat.new()
		highlight.bg_color = theme.ACCENT_SURFACE
		highlight.border_color = theme.ACCENT_BRIGHT
		highlight.set_border_width_all(1)
		highlight.set_corner_radius_all(theme.RADIUS_SM)
		btn.add_theme_stylebox_override("normal", highlight)
		btn.add_theme_stylebox_override("hover", highlight)
		
		# Reset other buttons to normal
		for other_btn in runner_ups_list.get_children():
			if other_btn != btn and other_btn is Button:
				_apply_card_button_style(other_btn, theme)
				
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
	var current_override = AudioManager.upcoming_track_override
	for idx in range(runner_ups_list.get_child_count()):
		var child = runner_ups_list.get_child(idx)
		if child is Button:
			var is_selected = false
			if idx < _runner_ups.size() and _runner_ups[idx] == current_override:
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

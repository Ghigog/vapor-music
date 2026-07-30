extends Control
## vibe_screen.gd
## DJ Vibe Workbench controller. Shows now-playing analyzer tags
## and lists compatible runner-up selections.

const ICON_VIBE := preload("res://assets/icon/vibe-cropped.png")

@onready var heading_icon: TextureRect = $VBox/Header/HBox/HeadingIcon
@onready var heading: Label = $VBox/Header/HBox/Heading
@onready var analysis_status: Label = $VBox/Header/HBox/AnalysisStatus

# Transition details
@onready var transition_timeline: Control = $VBox/TransitionCard/VBox/TransitionTimeline
@onready var transition_label: Label = $VBox/TransitionCard/VBox/TransitionLabel

# Runner-up containers
@onready var runner_ups_list: BoxContainer = $VBox/RunnerUpsScroll/RunnerUpsList

# Help Modal Elements
@onready var help_button: Button = $VBox/Header/HBox/HelpButton
@onready var help_modal: Control = $HelpModal
@onready var help_modal_panel: PanelContainer = $HelpModal/Panel
@onready var help_modal_title: Label = $HelpModal/Panel/VBox/Header/Title
@onready var help_modal_close: Button = $HelpModal/Panel/VBox/Header/CloseIcon
@onready var help_modal_text: RichTextLabel = $HelpModal/Panel/VBox/Scroll/ContentText

var CARD_MIN_WIDTH: float = 200.0
var CARD_MAX_WIDTH: float = 260.0
var CARD_HEIGHT_OFFSET: float = 110.0

# Reference to the runner-ups scroll container — used to switch scroll axes.
@onready var runner_ups_scroll: ScrollContainer = $VBox/RunnerUpsScroll
# Reference for adjusting stretch ratios per layout.
@onready var transition_card_panel: PanelContainer = $VBox/TransitionCard

const VIBE_CARD_SCENE = preload("res://scenes/screens/vibe/vibe_card.tscn")

var _runner_ups: Array = []
@onready var _disabled_overlay: PanelContainer = $DisabledOverlay
var _current_waveform_href: String = ""
var _triggered_incoming_load_href: String = ""

## Number of peak buckets requested from the DSP for the timeline waveform.
const WAVEFORM_PEAK_COUNT := 1000

## How often to re-check whether an async DSP decode has produced peaks.
const WAVEFORM_POLL_INTERVAL := 0.5

## The href each cached peak array currently corresponds to. Empty means "not
## yet resolved" — these are the guards that keep _refresh_waveforms() cheap.
var _peaks_out_href: String = ""
var _peaks_in_href: String = ""

## Transition trigger time, recomputed on track change rather than per frame.
var _cached_trigger_time: float = 0.0

var _waveform_poll: Timer

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

	# Waveform peaks arrive asynchronously with no completion signal, so a
	# low-frequency poll backstops the signal-driven refreshes above.
	_waveform_poll = Timer.new()
	_waveform_poll.wait_time = WAVEFORM_POLL_INTERVAL
	_waveform_poll.timeout.connect(_refresh_waveforms)
	add_child(_waveform_poll)
	_waveform_poll.start()

	visibility_changed.connect(_on_visibility_changed)

	var audio_analyzer = get_node_or_null("/root/AudioAnalyzer")
	if is_instance_valid(audio_analyzer):
		audio_analyzer.analysis_completed.connect(_on_analysis_completed)
		if audio_analyzer.has_signal("prefetch_progress"):
			audio_analyzer.prefetch_progress.connect(func(d, t): _refresh_display())
		if audio_analyzer.has_signal("prefetch_completed"):
			audio_analyzer.prefetch_completed.connect(func(): _refresh_display())
		
	if is_instance_valid(MetadataService):
		MetadataService.metadata_updated.connect(_on_metadata_updated)
		
	if is_instance_valid(WebDAVService) and WebDAVService.has_signal("library_scanned"):
		WebDAVService.library_scanned.connect(func(files): _refresh_display())
		
	if AudioManager.has_signal("smart_mixing_toggled"):
		AudioManager.smart_mixing_toggled.connect(func(enabled): _refresh_display())
		
	# Disabled overlay is static in the scene
	
	var trans_card = get_node_or_null("VBox/TransitionCard")
	if trans_card:
		trans_card.visible = false
		
	if runner_ups_list:
		runner_ups_list.alignment = BoxContainer.ALIGNMENT_CENTER
		runner_ups_list.resized.connect(_update_cards_layout)
	if runner_ups_scroll:
		runner_ups_scroll.resized.connect(_update_cards_layout)
	if is_instance_valid(PlatformManager):
		PlatformManager.layout_changed.connect(func(_bp): _update_cards_layout())
		
	_refresh_display()

func _apply_styles() -> void:
	if not is_inside_tree():
		return
	var theme = ThemeManager.current_theme
	
	if _disabled_overlay:
		var overlay_style = _disabled_overlay.get_theme_stylebox("panel") as StyleBoxFlat
		if overlay_style:
			overlay_style.bg_color = Color(theme.BG_VOID.r, theme.BG_VOID.g, theme.BG_VOID.b, 0.75)
			overlay_style.border_color = theme.GLASS_BORDER_SUBTLE
			overlay_style.set_border_width_all(1)
			overlay_style.set_corner_radius_all(theme.RADIUS_MD)
			
		var center = _disabled_overlay.get_child(0)
		if center:
			var vbox = center.get_child(0) as VBoxContainer
			if vbox:
				var icon_rect = vbox.get_child(0) as TextureRect
				var msg_lbl = vbox.get_child(1) as Label
				var sub_lbl = vbox.get_child(2) as Label

				icon_rect.texture = ICON_VIBE
				icon_rect.self_modulate = theme.ACCENT_CORE
				
				msg_lbl.add_theme_font_override("font", theme.font_display)
				msg_lbl.add_theme_font_size_override("font_size", theme.TYPE_LG)
				msg_lbl.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
				
				sub_lbl.add_theme_font_override("font", theme.font_ui)
				sub_lbl.add_theme_font_size_override("font_size", theme.TYPE_SM)
				sub_lbl.add_theme_color_override("font_color", theme.TEXT_SECONDARY)

	heading_icon.texture = ICON_VIBE
	heading_icon.self_modulate = theme.TEXT_PRIMARY

	heading.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	heading.add_theme_font_override("font", theme.font_display)
	heading.add_theme_font_size_override("font_size", theme.TYPE_LG)
	
	analysis_status.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	analysis_status.add_theme_font_override("font", theme.font_ui)
	analysis_status.add_theme_font_size_override("font_size", theme.TYPE_SM)
	
	# Cards panel styles
	for card_path in [
		"VBox/TransitionCard"
	]:
		var card = get_node_or_null(card_path)
		if card is PanelContainer:
			card.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD))

	# Style Section Titles
	for title_path in [
		"VBox/TransitionCard/VBox/SectionTitle"
	]:
		var lbl = get_node_or_null(title_path)
		if lbl is Label:
			lbl.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
			lbl.add_theme_font_override("font", theme.font_display)
			lbl.add_theme_font_size_override("font_size", theme.TYPE_XS)

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
		help_button.custom_minimum_size = ThemeManager.min_touch_size(help_button.custom_minimum_size)

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
		help_modal_close.custom_minimum_size = ThemeManager.min_touch_size(help_modal_close.custom_minimum_size)
		
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
	_invalidate_waveform_cache()
	_refresh_display()

func _on_playback_toggled(_is_playing: bool) -> void:
	_refresh_display()

func _on_transition_started(next_track: String, transition_type: String) -> void:
	transition_label.text = "Blending → %s  (%s)" % [
		next_track.get_file().uri_decode().get_basename(),
		transition_type
	]
	# Deck roles swap on transition — cached peaks now point at the wrong deck.
	_invalidate_waveform_cache()

func _on_transition_completed(_track: String) -> void:
	_invalidate_waveform_cache()
	_refresh_display()

func _on_analysis_completed(href: String, _results: Dictionary) -> void:
	_refresh_display()

func _on_metadata_updated(href: String, _metadata: Dictionary) -> void:
	var active_index = AudioManager.current_track_index
	var playlist = AudioManager.current_playlist
	if active_index != -1 and not playlist.is_empty() and playlist[active_index] == href:
		# Segment data drives the trigger time — recompute now that it exists.
		_refresh_trigger_time()
		_refresh_display()


## _refresh_waveforms() no-ops while hidden, so re-resolve on becoming visible
## to pick up anything that changed off-screen.
func _on_visibility_changed() -> void:
	if is_visible_in_tree():
		_invalidate_waveform_cache()

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
		
	if _disabled_overlay:
		_disabled_overlay.visible = not AudioManager.smart_mixing_enabled
		
	_update_analysis_status()
		
	var active_index = AudioManager.current_track_index
	var playlist = AudioManager.current_playlist
	
	if active_index == -1 or playlist.is_empty() or active_index >= playlist.size():
		transition_label.text = "AI DJ: Standby"
		if transition_timeline:
			transition_timeline.update_transition_info("No Track", "No Track", 0.0, 0.0, 0.0, "Standard Crossfade")
		_current_waveform_href = ""
		_peaks_out_href = ""
		_peaks_in_href = ""
		_cached_trigger_time = 0.0
		_clear_runner_ups()
		return
		
	var current_href = playlist[active_index]
	var meta = {}
	if is_instance_valid(MetadataService):
		meta = MetadataService.get_cached_metadata(current_href)
		
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

	# Update Transition Timeline custom control
	var next_href = AudioManager.get_next_track_href()
	var next_meta = {}
	var next_bpm = 0.0
	if not next_href.is_empty() and is_instance_valid(MetadataService):
		next_meta = MetadataService.get_cached_metadata(next_href)
		next_bpm = next_meta.get("bpm", 0.0)
		
	if transition_timeline:
		var outgoing_title = current_href.get_file()
		var incoming_title = next_href.get_file() if not next_href.is_empty() else "End of Playlist"
		var outgoing_bpm = meta.get("bpm", 0.0)
		var t_duration = AudioManager.get_transition_duration(AudioManager.upcoming_transition_type, current_href, next_href)
		transition_timeline.update_transition_info(
			outgoing_title,
			incoming_title,
			outgoing_bpm,
			next_bpm,
			t_duration,
			AudioManager.upcoming_transition_type
		)

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
		
	_update_cards_layout()

func _create_runner_up_card(href: String, meta: Dictionary, cost: float, type: String, is_selected: bool = false, is_ai_preferred: bool = false) -> void:
	var card_btn = VIBE_CARD_SCENE.instantiate()
	
	var current_bpm = 0.0
	var active_index = AudioManager.current_track_index
	if active_index != -1 and active_index < AudioManager.current_playlist.size() and is_instance_valid(MetadataService):
		var curr_meta = MetadataService.get_cached_metadata(AudioManager.current_playlist[active_index])
		if not curr_meta.is_empty():
			current_bpm = curr_meta.get("bpm", 0.0)
			
	var curr_href = ""
	if active_index != -1 and active_index < AudioManager.current_playlist.size():
		curr_href = AudioManager.current_playlist[active_index]
	var curr_meta = {}
	if not curr_href.is_empty():
		curr_meta = MetadataService.get_cached_metadata(curr_href)
	
	var predicted_transition = AudioManager.get_transition_type_between(curr_meta, meta, curr_href, href)
	
	runner_ups_list.add_child(card_btn)
	card_btn.setup(href, meta, cost, type, is_selected, is_ai_preferred, current_bpm, predicted_transition)
	
	# Click handler: locks track as next override
	var info = MetadataService.parse_track_info(href)
	card_btn.pressed.connect(func():
		AudioManager.set("upcoming_track_override", href)
		_refresh_display()
		print("VibeScreen: Selected next track override: ", info.track)
	)

 
func _style_runner_ups() -> void:
	pass

func _update_cards_layout() -> void:
	if not is_inside_tree() or not runner_ups_list:
		return

	var is_mobile = PlatformManager.is_mobile_layout()

	# Hide the heading label on narrow layouts to prevent overflow into the sidebar.
	if heading:
		heading.visible = not is_mobile

	# Switch BoxContainer orientation: vertical list on mobile, horizontal row on desktop.
	runner_ups_list.vertical = is_mobile

	# Adjust scroll container axes to match the layout direction.
	if runner_ups_scroll:
		if is_mobile:
			# Mobile: scroll up/down through the card list; no horizontal scroll.
			runner_ups_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
			runner_ups_scroll.vertical_scroll_mode   = ScrollContainer.SCROLL_MODE_AUTO
		else:
			# Desktop: horizontal scrolling row; no vertical scroll.
			runner_ups_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
			runner_ups_scroll.vertical_scroll_mode   = ScrollContainer.SCROLL_MODE_DISABLED

	# Adjust transition card stretch ratio: give it less space on mobile so cards get more room.
	if transition_card_panel:
		transition_card_panel.size_flags_stretch_ratio = 0.6 if is_mobile else 1.0
	if runner_ups_scroll:
		runner_ups_scroll.size_flags_stretch_ratio = 2.0 if is_mobile else 1.3

	var children: Array[Control] = []
	for child in runner_ups_list.get_children():
		if child is Button:
			children.append(child)

	var count = children.size()
	if count == 0:
		return

	var parent_width = runner_ups_list.get_parent_control().size.x

	if is_mobile:
		# Vertical list: cards fill the full available width minus a small gutter.
		var gutter := 16.0
		var target_w: float = max(CARD_MIN_WIDTH, parent_width - gutter)
		var target_h: float = min(target_w, CARD_MAX_WIDTH) + CARD_HEIGHT_OFFSET
		for card in children:
			card.custom_minimum_size = Vector2(target_w, target_h)
			card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			card.size_flags_vertical   = Control.SIZE_SHRINK_BEGIN
	else:
		# Horizontal row: divide available width evenly, clamped to card bounds.
		var separation := 16.0
		if runner_ups_list.has_theme_constant_override("separation"):
			separation = runner_ups_list.get_theme_constant("separation")
		elif runner_ups_list.has_theme_constant("separation"):
			separation = runner_ups_list.get_theme_constant("separation")

		var total_separation: float = separation * (count - 1)
		var available_w: float = parent_width - total_separation
		var w_per_card: float = available_w / count

		if w_per_card < CARD_MIN_WIDTH:
			runner_ups_list.alignment = BoxContainer.ALIGNMENT_BEGIN
		else:
			runner_ups_list.alignment = BoxContainer.ALIGNMENT_CENTER

		for card in children:
			var card_w: float = clamp(w_per_card, CARD_MIN_WIDTH, CARD_MAX_WIDTH)
			var card_h: float = card_w + CARD_HEIGHT_OFFSET
			card.custom_minimum_size = Vector2(card_w, card_h)

			if w_per_card > CARD_MAX_WIDTH:
				card.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
			else:
				card.size_flags_horizontal = Control.SIZE_EXPAND_FILL

			card.size_flags_vertical = Control.SIZE_SHRINK_CENTER


## Clamps the help modal panel to fit within the current viewport size.
func _size_help_modal_to_viewport() -> void:
	if not help_modal_panel:
		return
	var vp_size := get_viewport_rect().size
	# Leave at least 48 px margin on each side.
	var max_w: float = vp_size.x - 96.0
	var max_h: float = vp_size.y - 96.0
	help_modal_panel.custom_minimum_size = Vector2(
		clamp(max_w, 280.0, 650.0),
		clamp(max_h, 360.0, 600.0)
	)

func _on_help_button_pressed() -> void:
	if not help_modal:
		return

	# Resize the modal to fit the current viewport before showing it.
	_size_help_modal_to_viewport()
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

func _get_track_cache_path(href_path: String) -> String:
	if href_path.is_empty():
		return ""
	if href_path == "song1.mp3" or href_path == "song2.mp3" or href_path == "song3.mp3" or href_path.begins_with("test_"):
		return "res://tests/unit/test_track.wav"
	var ext := href_path.get_extension()
	if ext.is_empty():
		ext = "mp3"
	return "user://audio_cache/" + href_path.md5_text() + "." + ext

## Per-frame work is deliberately minimal: only the playhead moves every frame.
##
## Waveform peaks, durations, cue points and the trigger time all change at
## track boundaries, not per frame — they are resolved in _refresh_waveforms(),
## driven by track/transition signals plus a low-frequency poll. Previously this
## function called get_waveform_peaks(1000) twice per frame (~120k floats/sec
## across the GDExtension boundary) to recompute values that had not changed.
func _process(_delta: float) -> void:
	if not is_inside_tree() or not is_visible_in_tree() or not transition_timeline:
		return

	var active_player = AudioManager.active_player
	var pos = 0.0

	if is_instance_valid(active_player) and active_player.playing:
		var dsp = AudioManager.dsp_a if active_player == AudioManager.player_a else AudioManager.dsp_b
		if dsp:
			pos = dsp.get_playback_position()

	transition_timeline.update_playback_state(
		AudioManager.is_transitioning,
		AudioManager.crossfader,
		pos,
		AudioManager.current_track_length,
		_cached_trigger_time
	)


## Recomputes the transition trigger time from analyzer segment data.
## Falls back to a duration-derived estimate when no outro segment is known.
func _refresh_trigger_time() -> void:
	_cached_trigger_time = 0.0

	var active_index = AudioManager.current_track_index
	var playlist = AudioManager.current_playlist
	if active_index == -1 or active_index >= playlist.size():
		return
	if not is_instance_valid(MetadataService):
		return

	var meta = MetadataService.get_cached_metadata(playlist[active_index])
	if meta.is_empty():
		return

	var outro = meta.get("segments", {}).get("outro", [])
	if not outro.is_empty():
		_cached_trigger_time = outro[0]
	else:
		_cached_trigger_time = AudioManager.current_track_length \
			- AudioManager.get_transition_duration(AudioManager.upcoming_transition_type) \
			- 4.0


## Resolves waveform peaks, durations and cue points for the transition timeline.
##
## Peaks arrive asynchronously — a DSP reports get_cache_sample_count() == 0
## until its decode completes, and there is no signal for that. So this is
## driven both by track/transition signals (for immediacy) and by a 2 Hz poll
## (to catch the async completion). The per-href guards below mean the
## expensive get_waveform_peaks() call only runs when peaks are genuinely new,
## so the poll itself is close to free.
func _refresh_waveforms() -> void:
	if not is_inside_tree() or not is_visible_in_tree() or not transition_timeline:
		return

	var is_trans = AudioManager.is_transitioning

	var active_index = AudioManager.current_track_index
	var playlist = AudioManager.current_playlist
	var current_href = ""
	if active_index != -1 and active_index < playlist.size():
		current_href = playlist[active_index]
	_current_waveform_href = current_href

	# Determine which deck is outgoing and which is incoming.
	var outgoing_dsp = null
	var incoming_dsp = null
	var out_track_href = ""
	var in_track_href = ""

	if is_trans:
		out_track_href = AudioManager._outgoing_href
		in_track_href = AudioManager._incoming_href
		outgoing_dsp = AudioManager.dsp_a if AudioManager._outgoing_player == AudioManager.player_a else AudioManager.dsp_b
		incoming_dsp = AudioManager.dsp_a if AudioManager._incoming_player == AudioManager.player_a else AudioManager.dsp_b
	else:
		out_track_href = current_href
		in_track_href = AudioManager.get_next_track_href()
		outgoing_dsp = AudioManager.dsp_a if AudioManager.active_player == AudioManager.player_a else AudioManager.dsp_b
		incoming_dsp = AudioManager.dsp_b if AudioManager.active_player == AudioManager.player_a else AudioManager.dsp_a

	# --- Outgoing peaks -----------------------------------------------------
	if out_track_href.is_empty():
		if not _peaks_out_href.is_empty():
			_peaks_out_href = ""
			transition_timeline.outgoing_peaks = PackedFloat32Array()
			transition_timeline.outgoing_duration = 0.0
	elif _peaks_out_href != out_track_href and outgoing_dsp:
		if outgoing_dsp.get_cache_sample_count() > 0:
			_peaks_out_href = out_track_href
			transition_timeline.outgoing_peaks = outgoing_dsp.get_waveform_peaks(WAVEFORM_PEAK_COUNT)
			transition_timeline.outgoing_duration = outgoing_dsp.get_duration()

	# --- Incoming peaks -----------------------------------------------------
	if in_track_href.is_empty():
		_triggered_incoming_load_href = ""
		if not _peaks_in_href.is_empty():
			_peaks_in_href = ""
			transition_timeline.incoming_peaks = PackedFloat32Array()
			transition_timeline.incoming_duration = 0.0
			transition_timeline.incoming_cue_in = 0.0
		return

	# Kick off a background decode of the upcoming track so its waveform is
	# ready to draw before the transition starts.
	if not is_trans and transition_timeline.incoming_peaks.is_empty() and AudioManager.is_track_cached(in_track_href):
		if _triggered_incoming_load_href != in_track_href and incoming_dsp:
			_triggered_incoming_load_href = in_track_href
			var path = _get_track_cache_path(in_track_href)
			if not path.is_empty():
				var duration_hint = 0.0
				if is_instance_valid(MetadataService):
					duration_hint = MetadataService.get_cached_metadata(in_track_href).get("duration", 0.0)
				AudioManager.load_dsp_file(incoming_dsp, in_track_href, ProjectSettings.globalize_path(path), duration_hint)

	if _peaks_in_href != in_track_href and incoming_dsp:
		if incoming_dsp.get_cache_sample_count() > 0:
			_peaks_in_href = in_track_href
			transition_timeline.incoming_peaks = incoming_dsp.get_waveform_peaks(WAVEFORM_PEAK_COUNT)
			transition_timeline.incoming_duration = incoming_dsp.get_duration()

			var cue_in = 0.0
			if is_instance_valid(MetadataService):
				cue_in = MetadataService.get_cached_metadata(in_track_href).get("cue_in", 0.0)
			transition_timeline.incoming_cue_in = cue_in


## Invalidates cached peaks so the next _refresh_waveforms() re-resolves them.
## Call whenever the deck assignment or track selection changes underneath us.
func _invalidate_waveform_cache() -> void:
	_peaks_out_href = ""
	_peaks_in_href = ""
	_refresh_trigger_time()
	_refresh_waveforms()

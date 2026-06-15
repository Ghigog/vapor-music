## settings_screen.gd
## App settings screen.
## Includes theme, font size, library syncing, pre-caching, and headphone AutoEQ calibration.
extends Control

# App Window Containers
@onready var settings_panel: PanelContainer = %SettingsPanel

# Basic Settings Controls
@onready var icon_label: Label = $ScrollContainer/Center/SettingsPanel/MarginContainer/Content/IconLabel
@onready var heading:    Label = $ScrollContainer/Center/SettingsPanel/MarginContainer/Content/HeadingLabel
@onready var body:       Label = $ScrollContainer/Center/SettingsPanel/MarginContainer/Content/BodyLabel
@onready var select_a_theme: Label = $"ScrollContainer/Center/SettingsPanel/MarginContainer/Content/Select a theme"
@onready var theme_selector: OptionButton = $ScrollContainer/Center/SettingsPanel/MarginContainer/Content/ThemeSelector

@onready var font_size_label: Label = %FontSizeLabel
@onready var font_size_spinbox: SpinBox = %FontSizeSpinBox
@onready var connect_library_button: Button = %ConnectLibraryButton
@onready var cache_title: Label = %CacheTitle
@onready var prefetch_button: Button = %PrefetchButton
@onready var prefetch_progress: ProgressBar = %PrefetchProgress
@onready var prefetch_status: Label = %PrefetchStatus

# AutoEQ Controls
@onready var auto_eq_separator: HSeparator = %AutoEQSeparator
@onready var auto_eq_section: VBoxContainer = %AutoEQSection
@onready var auto_eq_title: Label = %AutoEQTitle
@onready var auto_eq_toggle_label: Label = %AutoEQToggleLabel
@onready var auto_eq_toggle: CheckButton = %AutoEQToggle
@onready var auto_eq_search_label: Label = %AutoEQSearchLabel
@onready var auto_eq_search: LineEdit = %AutoEQSearch
@onready var auto_eq_selector: OptionButton = %AutoEQSelector
@onready var calibration_graph: CalibrationGraph = %CalibrationGraph

const THEME_MAP = {
	"Vapor Dark" : "res://assets/themes/default_dark.tres",
	"Vapor Light" : "res://assets/themes/default_light.tres"
}

# AutoEQ Presets Storage
var all_presets: Dictionary = {}
var all_models: Array = []
var filtered_models: Array = []

func _ready() -> void:
	_load_headphone_presets()
	_apply_styles()
	ThemeManager.theme_changed.connect(_apply_styles)
	
	# Populate the Theme OptionButton
	theme_selector.clear()
	for theme_name in THEME_MAP.keys():
		theme_selector.add_item(theme_name)
		
	# Select current active theme
	var active_path = ThemeManager.current_theme.resource_path
	for theme_name in THEME_MAP.keys():
		if THEME_MAP[theme_name] == active_path:
			theme_selector.text = theme_name
		
	# Connect the selection signal
	theme_selector.item_selected.connect(_on_theme_selected)

	# Initialize font size spinbox
	font_size_spinbox.value = SettingsManager.base_font_size
	font_size_spinbox.value_changed.connect(_on_font_size_changed)
	
	# Connect library connection trigger button
	connect_library_button.pressed.connect(_on_connect_library_pressed)

	# Hook prefetcher events
	if is_instance_valid(AudioAnalyzer):
		AudioAnalyzer.prefetch_started.connect(_on_prefetch_started)
		AudioAnalyzer.prefetch_progress.connect(_on_prefetch_progress)
		AudioAnalyzer.prefetch_completed.connect(_on_prefetch_completed)
		AudioAnalyzer.prefetch_stopped.connect(_on_prefetch_stopped)
		
		# Update UI based on active prefetcher state
		if AudioAnalyzer.background_caching_active:
			_show_prefetch_progress(true)
			_update_prefetch_ui(AudioAnalyzer._prefetch_downloaded, AudioAnalyzer._prefetch_total)
		else:
			_show_prefetch_progress(false)
			
	prefetch_button.pressed.connect(_on_prefetch_pressed)

	# Initialize AutoEQ settings
	auto_eq_toggle.button_pressed = SettingsManager.headphone_calibration_enabled
	auto_eq_toggle.toggled.connect(_on_auto_eq_toggle_toggled)
	
	_populate_auto_eq_selector("")
	auto_eq_selector.item_selected.connect(_on_auto_eq_selector_selected)
	auto_eq_search.text_changed.connect(_on_auto_eq_search_text_changed)
	
	_update_calibration_audio_and_graph()

func _load_headphone_presets() -> void:
	var presets_path = "res://assets/settings/headphone_presets.json"
	if FileAccess.file_exists(presets_path):
		var file = FileAccess.open(presets_path, FileAccess.READ)
		if file:
			var json_text = file.get_as_text()
			file.close()
			var json = JSON.new()
			var error = json.parse(json_text)
			if error == OK and json.data is Dictionary:
				all_presets = json.data
				all_models = all_presets.keys()
				all_models.sort()
			else:
				push_error("SettingsScreen: Failed to parse headphone presets JSON")
		else:
			push_error("SettingsScreen: Failed to open headphone presets file")
	else:
		push_error("SettingsScreen: Headphone presets file not found")

func _populate_auto_eq_selector(filter_text: String = "") -> void:
	auto_eq_selector.clear()
	filtered_models.clear()
	
	var search_query = filter_text.strip_edges().to_lower()
	for model in all_models:
		if search_query.is_empty() or search_query in model.to_lower():
			filtered_models.append(model)
			
	for model in filtered_models:
		auto_eq_selector.add_item(model)
		
	# Reselect active profile if present in filtered list
	var active_profile = SettingsManager.headphone_profile
	var select_idx = filtered_models.find(active_profile)
	if select_idx != -1:
		auto_eq_selector.selected = select_idx
	elif not filtered_models.is_empty():
		# Fallback to first filtered option but don't save until confirmed
		auto_eq_selector.selected = 0

func _on_auto_eq_search_text_changed(new_text: String) -> void:
	_populate_auto_eq_selector(new_text)

func _on_auto_eq_selector_selected(index: int) -> void:
	if index >= 0 and index < filtered_models.size():
		var selected_model = filtered_models[index]
		SettingsManager.save_headphone_profile(selected_model)
		_update_calibration_audio_and_graph()

func _on_auto_eq_toggle_toggled(button_pressed: bool) -> void:
	SettingsManager.save_headphone_calibration_enabled(button_pressed)
	_update_calibration_audio_and_graph()

func _update_calibration_audio_and_graph() -> void:
	if is_instance_valid(AudioManager):
		AudioManager.update_calibration_state()
		
	var enabled = SettingsManager.headphone_calibration_enabled
	calibration_graph.set_active(enabled)
	
	var active_profile = SettingsManager.headphone_profile
	var raw_profile = all_presets.get(active_profile, "")
	if not raw_profile.is_empty():
		var parsed = AutoEQParser.parse_profile_text(raw_profile)
		calibration_graph.set_profile(parsed)
	else:
		calibration_graph.clear_profile()

func _apply_styles() -> void:
	# Skin the Settings frosted glass card container
	if is_instance_valid(settings_panel):
		settings_panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel())

	icon_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_TERTIARY)
	icon_label.add_theme_font_size_override("font_size", 80)

	heading.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	heading.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	heading.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_LG)

	select_a_theme.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	select_a_theme.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	select_a_theme.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_LG)

	var opt_style = StyleBoxFlat.new()
	opt_style.bg_color = ThemeManager.current_theme.BG_ELEVATED
	opt_style.border_color = ThemeManager.current_theme.GLASS_BORDER
	opt_style.set_border_width_all(1)
	opt_style.set_corner_radius_all(ThemeManager.current_theme.RADIUS_XS)
	opt_style.content_margin_left = 10
	opt_style.content_margin_right = 10
	opt_style.content_margin_top = 6
	opt_style.content_margin_bottom = 6
	
	theme_selector.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	theme_selector.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	theme_selector.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	theme_selector.add_theme_stylebox_override("normal", opt_style)
	theme_selector.add_theme_stylebox_override("hover", opt_style)
	theme_selector.add_theme_stylebox_override("pressed", opt_style)
	theme_selector.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

	body.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	body.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	body.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

	# Font size label style
	font_size_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	font_size_label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	font_size_label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	
	# Set custom SpinBox line edit font and color
	var spinbox_line_edit = font_size_spinbox.get_line_edit()
	if spinbox_line_edit:
		spinbox_line_edit.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
		spinbox_line_edit.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		spinbox_line_edit.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
		var spinbox_style = StyleBoxFlat.new()
		spinbox_style.bg_color = ThemeManager.current_theme.BG_ELEVATED
		spinbox_style.border_color = ThemeManager.current_theme.GLASS_BORDER
		spinbox_style.set_border_width_all(1)
		spinbox_style.set_corner_radius_all(ThemeManager.current_theme.RADIUS_XS)
		spinbox_style.content_margin_left = 8
		spinbox_style.content_margin_right = 8
		spinbox_line_edit.add_theme_stylebox_override("normal", spinbox_style)
		spinbox_line_edit.add_theme_stylebox_override("focus", spinbox_style)
		
	# Connect Library Button styles
	connect_library_button.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	connect_library_button.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	connect_library_button.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	connect_library_button.add_theme_color_override("font_hover_color", ThemeManager.current_theme.TEXT_INVERSE)
	connect_library_button.add_theme_color_override("font_pressed_color", ThemeManager.current_theme.TEXT_INVERSE)
	connect_library_button.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
	connect_library_button.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	connect_library_button.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	connect_library_button.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

	# Cache Section Title style
	cache_title.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	cache_title.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	cache_title.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_MD)
	
	# Prefetch Button styles
	prefetch_button.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	prefetch_button.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	prefetch_button.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
	prefetch_button.add_theme_color_override("font_hover_color", ThemeManager.current_theme.TEXT_INVERSE)
	prefetch_button.add_theme_color_override("font_pressed_color", ThemeManager.current_theme.TEXT_INVERSE)
	prefetch_button.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
	prefetch_button.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	prefetch_button.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
	prefetch_button.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
	
	# Prefetch Progress bar styles
	var progress_bg = StyleBoxFlat.new()
	progress_bg.bg_color = ThemeManager.current_theme.BG_ELEVATED
	progress_bg.set_corner_radius_all(ThemeManager.current_theme.RADIUS_XS)
	
	var progress_fill = StyleBoxFlat.new()
	progress_fill.bg_color = ThemeManager.current_theme.ACCENT_CORE
	progress_fill.set_corner_radius_all(ThemeManager.current_theme.RADIUS_XS)
	
	prefetch_progress.add_theme_stylebox_override("background", progress_bg)
	prefetch_progress.add_theme_stylebox_override("fill", progress_fill)
	prefetch_progress.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	prefetch_progress.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	prefetch_progress.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_XS)
	
	# Prefetch Status styles
	prefetch_status.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	prefetch_status.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	prefetch_status.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

	# Skin new AutoEQ elements
	auto_eq_title.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	auto_eq_title.add_theme_font_override("font", ThemeManager.current_theme.font_display)
	auto_eq_title.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_MD)

	auto_eq_toggle_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	auto_eq_toggle_label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	auto_eq_toggle_label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

	auto_eq_search_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	auto_eq_search_label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	auto_eq_search_label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

	# Search LineEdit style
	auto_eq_search.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	auto_eq_search.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	auto_eq_search.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	var search_style = StyleBoxFlat.new()
	search_style.bg_color = ThemeManager.current_theme.BG_ELEVATED
	search_style.border_color = ThemeManager.current_theme.GLASS_BORDER
	search_style.set_border_width_all(1)
	search_style.set_corner_radius_all(ThemeManager.current_theme.RADIUS_XS)
	search_style.content_margin_left = 8
	search_style.content_margin_right = 8
	auto_eq_search.add_theme_stylebox_override("normal", search_style)
	auto_eq_search.add_theme_stylebox_override("focus", search_style)

	# Selector OptionButton style
	auto_eq_selector.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
	auto_eq_selector.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	auto_eq_selector.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	auto_eq_selector.add_theme_stylebox_override("normal", opt_style)
	auto_eq_selector.add_theme_stylebox_override("hover", opt_style)
	auto_eq_selector.add_theme_stylebox_override("pressed", opt_style)
	auto_eq_selector.add_theme_stylebox_override("focus", ThemeManager.make_transparent())

func _on_theme_selected(index: int) -> void:
	# Get the name (key) based on the index
	var theme_name = theme_selector.get_item_text(index)
	var theme_path = THEME_MAP[theme_name]
	
	# Update the global theme manager
	ThemeManager.load_theme(theme_path)

func _on_font_size_changed(value: float) -> void:
	SettingsManager.save_base_font_size(int(value))

func _on_connect_library_pressed() -> void:
	NavManager.show_setup_wizard()

func _on_prefetch_pressed() -> void:
	if not is_instance_valid(AudioAnalyzer) or not is_instance_valid(WebDAVService):
		return
		
	if AudioAnalyzer.background_caching_active:
		AudioAnalyzer.stop_prefetching()
	else:
		var scanned = WebDAVService.scanned_files
		if scanned.is_empty():
			prefetch_status.visible = true
			prefetch_status.text = "No tracks in library to cache. Scan first!"
			return
		AudioAnalyzer.start_prefetching(scanned)

func _on_prefetch_started(total: int) -> void:
	_show_prefetch_progress(true)
	_update_prefetch_ui(AudioAnalyzer._prefetch_downloaded, total)

func _on_prefetch_progress(downloaded: int, total: int) -> void:
	_update_prefetch_ui(downloaded, total)

func _on_prefetch_completed() -> void:
	prefetch_button.text = "Pre-cache & Analyze Library"
	prefetch_status.text = "✓ Cache complete & analyzed"
	prefetch_progress.visible = false

func _on_prefetch_stopped() -> void:
	prefetch_button.text = "Pre-cache & Analyze Library"
	prefetch_status.text = "Analysis stopped"
	prefetch_progress.visible = false

func _show_prefetch_progress(show: bool) -> void:
	prefetch_progress.visible = show
	prefetch_status.visible = true
	if show:
		prefetch_button.text = "Stop Background Caching"
	else:
		prefetch_button.text = "Pre-cache & Analyze Library"
		prefetch_status.text = "Idle"

func _update_prefetch_ui(_downloaded: int, total: int) -> void:
	prefetch_button.text = "Stop Background Caching"
	if total > 0:
		var ready_count = 0
		if is_instance_valid(AudioAnalyzer) and is_instance_valid(WebDAVService):
			ready_count = AudioAnalyzer.get_ready_tracks_count(WebDAVService.scanned_files)
		else:
			ready_count = _downloaded
			
		var pct = float(ready_count) / float(total) * 100.0
		prefetch_progress.visible = true
		prefetch_progress.value = pct
		prefetch_status.text = "Caching... %d / %d (%d%%)" % [ready_count, total, int(pct)]
	else:
		prefetch_progress.value = 100.0
		prefetch_status.text = "✓ All tracks cached"
		prefetch_progress.visible = false

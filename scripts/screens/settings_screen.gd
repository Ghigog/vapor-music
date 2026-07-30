## settings_screen.gd
## App settings screen.
## Includes theme, font size, library syncing, pre-caching, and headphone AutoEQ calibration.
extends Control

const ICON_SETTINGS := preload("res://assets/icon/settings-cropped.png")

# App Window Containers
@onready var settings_panel: PanelContainer = %SettingsPanel

# Basic Settings Controls
@onready var icon_label: TextureRect = $ScrollContainer/Center/SettingsPanel/MarginContainer/Content/IconLabel
@onready var heading:    Label = $ScrollContainer/Center/SettingsPanel/MarginContainer/Content/HeadingLabel
@onready var body:       Label = $ScrollContainer/Center/SettingsPanel/MarginContainer/Content/BodyLabel
@onready var select_a_theme: Label = $"ScrollContainer/Center/SettingsPanel/MarginContainer/Content/Select a theme"
@onready var theme_selector: OptionButton = $ScrollContainer/Center/SettingsPanel/MarginContainer/Content/ThemeSelector
@onready var base_color_label: Label = %BaseColorLabel
@onready var base_color_picker: ColorPickerButton = %BaseColorPicker
@onready var accent_color_label: Label = %AccentColorLabel
@onready var accent_color_picker: ColorPickerButton = %AccentColorPicker

@onready var font_size_label: Label = %FontSizeLabel
@onready var font_size_spinbox: SpinBox = %FontSizeSpinBox
@onready var ui_scale_label: Label = %UIScaleLabel
@onready var ui_scale_spinbox: SpinBox = %UIScaleSpinBox
@onready var connect_library_button: Button = %ConnectLibraryButton
@onready var cache_title: Label = %CacheTitle
@onready var prefetch_button: Button = %PrefetchButton
@onready var prefetch_progress: ProgressBar = %PrefetchProgress
@onready var prefetch_status: Label = %PrefetchStatus

# Caching Overlay Controls
@onready var cache_overlay: PanelContainer = %CacheOverlay
@onready var overlay_spinner: TextureRect = %OverlaySpinner
@onready var overlay_title: Label = %OverlayTitle
@onready var overlay_progress: Label = %OverlayProgress
@onready var overlay_warning: Label = %OverlayWarning
@onready var minimize_cache_btn: Button = %MinimizeCacheBtn
@onready var stop_cache_btn: Button = %StopCacheBtn

var _user_minimized_overlay := false



# Performance Diagnostics Controls
@onready var performance_separator: HSeparator = %PerformanceSeparator
@onready var performance_section: VBoxContainer = %PerformanceSection
@onready var performance_title: Label = %PerformanceTitle
@onready var fps_label: Label = %FPSLabel
@onready var fps_value: Label = %FPSValue
@onready var ram_label: Label = %RAMLabel
@onready var ram_value: Label = %RAMValue
@onready var audio_latency_label: Label = %AudioLatencyLabel
@onready var audio_latency_value: Label = %AudioLatencyValue

## Refresh interval for the performance readout, in seconds.
const DIAGNOSTICS_INTERVAL := 0.25
var _diagnostics_timer: Timer

const THEME_MAP = {
	"Vapor Dark" : "res://assets/themes/default_dark.tres",
	"Vapor Light" : "res://assets/themes/default_light.tres"
}

func _ready() -> void:
	_apply_styles()
	ThemeManager.theme_changed.connect(_apply_styles)

	# Performance readout refresh cadence — fast enough to feel live, slow
	# enough that the string formatting cost is negligible.
	_diagnostics_timer = Timer.new()
	_diagnostics_timer.wait_time = DIAGNOSTICS_INTERVAL
	_diagnostics_timer.timeout.connect(_update_diagnostics)
	add_child(_diagnostics_timer)
	_diagnostics_timer.start()
	_update_diagnostics()

	# Populate the Theme OptionButton
	theme_selector.clear()
	for theme_name in THEME_MAP.keys():
		theme_selector.add_item(theme_name)
		
	# Select current active theme
	if SettingsManager.theme_mode == "custom":
		theme_selector.text = "Custom"
	else:
		var active_path = ThemeManager.current_theme.resource_path
		for theme_name in THEME_MAP.keys():
			if THEME_MAP[theme_name] == active_path:
				theme_selector.text = theme_name
		
	# Connect the selection signal
	theme_selector.item_selected.connect(_on_theme_selected)

	# Initialize the custom color swatches from whatever theme is active
	base_color_picker.color = ThemeManager.current_theme.BG_ELEVATED
	accent_color_picker.color = ThemeManager.current_theme.ACCENT_CORE
	base_color_picker.color_changed.connect(_on_custom_color_changed)
	accent_color_picker.color_changed.connect(_on_custom_color_changed)
	base_color_picker.popup_closed.connect(_on_custom_color_picker_closed)
	accent_color_picker.popup_closed.connect(_on_custom_color_picker_closed)

	# Initialize font size spinbox
	font_size_spinbox.value = SettingsManager.base_font_size
	font_size_spinbox.value_changed.connect(_on_font_size_changed)
	
	# Initialize UI Scale spinbox
	ui_scale_spinbox.value = SettingsManager.ui_scale
	ui_scale_spinbox.value_changed.connect(_on_ui_scale_changed)
	
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
			_update_overlay_progress(AudioAnalyzer._prefetch_downloaded, AudioAnalyzer._prefetch_total)
			_update_overlay_visibility()
		else:
			_show_prefetch_progress(false)
			_update_overlay_visibility()
			
	prefetch_button.pressed.connect(_on_prefetch_pressed)
	minimize_cache_btn.pressed.connect(_on_minimize_cache_pressed)
	stop_cache_btn.pressed.connect(_on_prefetch_pressed)
func _apply_styles() -> void:
	# Skin the Settings frosted glass card container
	if is_instance_valid(settings_panel):
		settings_panel.add_theme_stylebox_override("panel", ThemeManager.make_glass_panel())

	icon_label.texture = ICON_SETTINGS
	icon_label.self_modulate = ThemeManager.current_theme.TEXT_TERTIARY

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

	# Custom color swatch labels
	for lbl in [base_color_label, accent_color_label]:
		lbl.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
		lbl.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		lbl.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_XS)

	# Color swatch buttons — round the picker button to match the rest of the UI
	for picker in [base_color_picker, accent_color_picker]:
		picker.add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		var swatch_style = StyleBoxFlat.new()
		swatch_style.border_color = ThemeManager.current_theme.GLASS_BORDER
		swatch_style.set_border_width_all(1)
		swatch_style.set_corner_radius_all(ThemeManager.current_theme.RADIUS_SM)
		picker.add_theme_stylebox_override("normal", swatch_style)

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

	# UI scale label style
	ui_scale_label.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
	ui_scale_label.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
	ui_scale_label.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
	
	# Set custom SpinBox line edit font and color for UI scale
	var ui_spinbox_line_edit = ui_scale_spinbox.get_line_edit()
	if ui_spinbox_line_edit:
		ui_spinbox_line_edit.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
		ui_spinbox_line_edit.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		ui_spinbox_line_edit.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
		var spinbox_style = StyleBoxFlat.new()
		spinbox_style.bg_color = ThemeManager.current_theme.BG_ELEVATED
		spinbox_style.border_color = ThemeManager.current_theme.GLASS_BORDER
		spinbox_style.set_border_width_all(1)
		spinbox_style.set_corner_radius_all(ThemeManager.current_theme.RADIUS_XS)
		spinbox_style.content_margin_left = 8
		spinbox_style.content_margin_right = 8
		ui_spinbox_line_edit.add_theme_stylebox_override("normal", spinbox_style)
		ui_spinbox_line_edit.add_theme_stylebox_override("focus", spinbox_style)
		
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

	# Skin Performance diagnostics elements
	if is_instance_valid(performance_title):
		performance_title.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
		performance_title.add_theme_font_override("font", ThemeManager.current_theme.font_display)
		performance_title.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_MD)
	
	for lbl in [fps_label, ram_label, audio_latency_label]:
		if is_instance_valid(lbl):
			lbl.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
			lbl.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
			lbl.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
			
	for val in [fps_value, ram_value, audio_latency_value]:
		if is_instance_valid(val):
			val.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_SECONDARY)
			val.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
			val.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

	# Caching Overlay styling
	if is_instance_valid(cache_overlay):
		var overlay_glass = ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_MD, 0.96)
		cache_overlay.add_theme_stylebox_override("panel", overlay_glass)
		
	if is_instance_valid(overlay_spinner):
		overlay_spinner.texture = ICON_SETTINGS
		overlay_spinner.self_modulate = ThemeManager.current_theme.ACCENT_BRIGHT
		
	if is_instance_valid(overlay_title):
		overlay_title.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
		overlay_title.add_theme_font_override("font", ThemeManager.current_theme.font_display)
		overlay_title.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_MD)
		
	if is_instance_valid(overlay_progress):
		overlay_progress.add_theme_color_override("font_color", ThemeManager.current_theme.TEXT_PRIMARY)
		overlay_progress.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		overlay_progress.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
		
	if is_instance_valid(overlay_warning):
		overlay_warning.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
		overlay_warning.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
		overlay_warning.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)

	# Overlay Buttons styling
	for btn in [minimize_cache_btn, stop_cache_btn]:
		if is_instance_valid(btn):
			btn.add_theme_font_override("font", ThemeManager.current_theme.font_ui)
			btn.add_theme_font_size_override("font_size", ThemeManager.current_theme.TYPE_SM)
			btn.add_theme_color_override("font_color", ThemeManager.current_theme.ACCENT_BRIGHT)
			btn.add_theme_color_override("font_hover_color", ThemeManager.current_theme.TEXT_INVERSE)
			btn.add_theme_color_override("font_pressed_color", ThemeManager.current_theme.TEXT_INVERSE)
			btn.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
			btn.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
			btn.add_theme_stylebox_override("pressed", ThemeManager.make_cta_button(true))
			btn.add_theme_stylebox_override("focus", ThemeManager.make_transparent())


func _on_theme_selected(index: int) -> void:
	# Get the name (key) based on the index
	var theme_name = theme_selector.get_item_text(index)
	var theme_path = THEME_MAP[theme_name]

	# Update the global theme manager and persist the choice
	SettingsManager.save_active_theme(theme_path)

	# Sync the custom color swatches to reflect the preset that just loaded
	base_color_picker.color = ThemeManager.current_theme.BG_ELEVATED
	accent_color_picker.color = ThemeManager.current_theme.ACCENT_CORE

## Live preview as the user drags the color wheel or edits the hex field —
## regenerates the theme in memory but doesn't touch disk yet.
func _on_custom_color_changed(_color: Color) -> void:
	ThemeManager.apply_custom_colors(base_color_picker.color, accent_color_picker.color)
	theme_selector.text = "Custom"

## Persist the custom colors once the picker popup closes, so we're not
## writing settings.cfg on every drag frame.
func _on_custom_color_picker_closed() -> void:
	SettingsManager.save_custom_theme_colors(base_color_picker.color, accent_color_picker.color)

func _on_font_size_changed(value: float) -> void:
	SettingsManager.save_base_font_size(int(value))

func _on_ui_scale_changed(value: float) -> void:
	SettingsManager.save_ui_scale(value)

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
	_user_minimized_overlay = false
	_show_prefetch_progress(true)
	_update_prefetch_ui(AudioAnalyzer._prefetch_downloaded, total)
	_update_overlay_progress(AudioAnalyzer._prefetch_downloaded, total)
	_update_overlay_visibility()

func _on_prefetch_progress(downloaded: int, total: int) -> void:
	_update_prefetch_ui(downloaded, total)
	_update_overlay_progress(downloaded, total)
	_update_overlay_visibility()

func _on_prefetch_completed() -> void:
	_user_minimized_overlay = false
	_update_status_after_caching(false)
	_update_overlay_visibility()

func _on_prefetch_stopped() -> void:
	_user_minimized_overlay = false
	_update_status_after_caching(true)
	_update_overlay_visibility()

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

func _update_overlay_progress(_downloaded: int, total: int) -> void:
	if not is_instance_valid(overlay_progress):
		return
	if total > 0:
		var ready_count = 0
		if is_instance_valid(AudioAnalyzer) and is_instance_valid(WebDAVService):
			ready_count = AudioAnalyzer.get_ready_tracks_count(WebDAVService.scanned_files)
		else:
			ready_count = _downloaded
			
		var pct = float(ready_count) / float(total) * 100.0
		overlay_progress.text = "Caching... %d / %d (%d%%)" % [ready_count, total, int(pct)]
	else:
		overlay_progress.text = "✓ All tracks cached"

func _update_overlay_visibility() -> void:
	if not is_instance_valid(cache_overlay) or not is_instance_valid(AudioAnalyzer):
		return
		
	if AudioAnalyzer.background_caching_active and not _user_minimized_overlay:
		cache_overlay.visible = true
	else:
		cache_overlay.visible = false

func _on_minimize_cache_pressed() -> void:
	_user_minimized_overlay = true
	_update_overlay_visibility()

func _update_status_after_caching(was_stopped: bool) -> void:
	prefetch_button.text = "Pre-cache & Analyze Library"
	prefetch_progress.visible = false
	
	var total = 0
	var ready_count = 0
	if is_instance_valid(AudioAnalyzer) and is_instance_valid(WebDAVService):
		var scanned = WebDAVService.scanned_files
		total = scanned.size()
		ready_count = AudioAnalyzer.get_ready_tracks_count(scanned)
	
	var prefix = "Caching stopped" if was_stopped else "Cache incomplete"
	if ready_count >= total and total > 0:
		var msg = "✓ Cache complete & analyzed"
		prefetch_status.text = msg
		if is_instance_valid(overlay_progress):
			overlay_progress.text = msg
	else:
		var left = total - ready_count
		var msg = "%s: %d tracks remaining" % [prefix, left]
		prefetch_status.text = msg
		if is_instance_valid(overlay_progress):
			overlay_progress.text = msg

## Only the spinner needs frame cadence. Diagnostics are refreshed on a timer —
## see _update_diagnostics().
func _process(_delta: float) -> void:
	if not is_inside_tree() or not is_visible_in_tree():
		return

	# Spin the caching overlay spinner gear if active
	if is_instance_valid(cache_overlay) and cache_overlay.visible and is_instance_valid(overlay_spinner):
		overlay_spinner.pivot_offset = overlay_spinner.size / 2.0
		overlay_spinner.rotation += _delta * 3.0


## Refreshes the performance readout.
##
## Driven by _diagnostics_timer at DIAGNOSTICS_INTERVAL rather than from
## _process: each of these three lines allocates a String and forces a Label
## re-layout, and no one can read a number that updates 60 times a second.
func _update_diagnostics() -> void:
	if not is_inside_tree() or not is_visible_in_tree():
		return

	var fps = Performance.get_monitor(Performance.TIME_FPS)
	if is_instance_valid(fps_value):
		fps_value.text = "%d FPS" % int(fps)

	var ram_bytes = Performance.get_monitor(Performance.MEMORY_STATIC)
	var ram_mb = float(ram_bytes) / 1024.0 / 1024.0
	if is_instance_valid(ram_value):
		ram_value.text = "%.1f MB" % ram_mb

	var audio_lat = Performance.get_monitor(Performance.AUDIO_OUTPUT_LATENCY) * 1000.0
	if is_instance_valid(audio_latency_value):
		audio_latency_value.text = "%.1f ms" % audio_lat

## mini_player.gd
## Bottom UI controller component handling active state indicators and playback toggles.
extends Control

# Hardened onready node paths referencing your exact scene tree hierarchy
@onready var track_title_label: Label = $HBox/ProgressBar/TrackInfo/TrackTitle
@onready var play_pause_btn: Button = $HBox/PlayPauseBtn
@onready var progress_bar: HSlider = $HBox/ProgressBar
@onready var loading_bar: ProgressBar = $LoadingBar
var dragging = false


func _ready() -> void:
	# Connect to our global audio singleton signals
	AudioManager.track_changed.connect(_on_track_changed)
	AudioManager.playback_toggled.connect(_on_playback_toggled)
	AudioManager.loading_track.connect(_on_loading_track)
	
	if play_pause_btn:
		play_pause_btn.pressed.connect(_on_play_pause_pressed)

func _process(delta: float) -> void:
	if AudioManager.is_playing and !dragging:
		progress_bar.value = AudioManager.player.get_playback_position()

func _on_track_changed(track_name: String) -> void:
	if track_title_label:
		# If the filename contains a long prefix like "Vanilla - Origin - ", 
		# we clean it up so just the song title displays nicely in the small bar!
		var clean_name := track_name
		if " - " in track_name:
			var parts := track_name.split(" - ")
			clean_name = parts[parts.size() - 1] # Grabs just the song name at the end
			
		track_title_label.text = clean_name

func _on_playback_toggled(is_playing: bool) -> void:
	progress_bar.max_value = AudioManager.current_track_length
	if play_pause_btn:
		# Changes text icons depending on play state. 
		# (If you are using visual texture sprites instead of font text later, we can swap this)
		if is_playing:
			play_pause_btn.text = "⏸"
		else:
			play_pause_btn.text = "▶"

func _on_play_pause_pressed() -> void:
	AudioManager.toggle_play()


func _on_progress_bar_drag_started() -> void:
	dragging = true


func _on_progress_bar_drag_ended(value_changed: bool) -> void:
	dragging = false
	AudioManager.scroll_track(progress_bar.value)

func _on_loading_track(is_loading: bool) -> void:
	if loading_bar:
		loading_bar.visible = is_loading

func _on_backward_btn_pressed() -> void:
	AudioManager.play_previous()

func _on_forward_btn_pressed() -> void:
	AudioManager.play_next()

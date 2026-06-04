## NavManager.gd
## Autoload singleton — manages in-app navigation state and history.
##
## All navigation in Vapor goes through this singleton so that the sidebar,
## tab bar, and any other trigger point stay in sync automatically.
##
## Screens (MVP):
##   "library"     — Music library browser (default)
##   "vibe"        — DJ Vibe Workbench
##   "now_playing" — Full-screen Now Playing view
##   "queue"       — Playback queue
##   "settings"    — App settings
##
## Usage:
##   NavManager.navigate_to("settings")
##   NavManager.navigation_requested.connect(_on_nav)

extends Node


# ---------------------------------------------------------------------------
# Signals
# ---------------------------------------------------------------------------

## Emitted whenever the active screen changes. Connect in any UI that needs
## to respond to navigation (sidebar active state, screen visibility, etc.).
signal navigation_requested(screen_name: String)

## Emitted when back navigation is triggered (e.g. Android back button).
signal back_requested()

## Emitted when the setup wizard is requested to open.
signal setup_wizard_requested()

## Request to show the setup wizard
func show_setup_wizard() -> void:
	setup_wizard_requested.emit()



# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

## The name of the currently visible screen.
var current_screen: String = "library"

## Navigation history stack. Used for back navigation.
var _history: Array[String] = []


func _ready() -> void:
	# On Android/iOS, intercept the hardware back button via ui_cancel.
	set_process_unhandled_input(PlatformManager.is_mobile())


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		go_back()
		get_viewport().set_input_as_handled()


# ---------------------------------------------------------------------------
# Navigation API
# ---------------------------------------------------------------------------

## Navigate to [param screen_name]. Pushes the previous screen onto the
## history stack so [method go_back] can return to it.
## Does nothing if [param screen_name] is already the active screen.
func navigate_to(screen_name: String) -> void:
	if screen_name == current_screen:
		return
	_history.push_back(current_screen)
	current_screen = screen_name
	navigation_requested.emit(screen_name)


## Navigate back to the previous screen. Emits [signal back_requested].
## Does nothing if the history stack is empty.
func go_back() -> void:
	if _history.is_empty():
		return
	current_screen = _history.pop_back()
	navigation_requested.emit(current_screen)
	back_requested.emit()


## Returns true if there is at least one screen in the history to return to.
func can_go_back() -> bool:
	return not _history.is_empty()


## Clears the navigation history. Call when resetting application state.
func clear_history() -> void:
	_history.clear()

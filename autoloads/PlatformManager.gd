## PlatformManager.gd
## Autoload singleton — detects the running platform (desktop / mobile) and
## tracks the current responsive breakpoint based on the viewport width.
##
## Breakpoints (from DESIGN_LANGUAGE.md §12):
##   xs  — < 480px    single-column mobile portrait
##   sm  — 480–768px  mobile landscape / tablet portrait
##   md  — 768–1080px tablet landscape / small desktop
##   lg  — 1080–1440px standard desktop
##   xl  — > 1440px   wide desktop
##
## Usage:
##   PlatformManager.is_mobile()          → bool
##   PlatformManager.should_show_sidebar() → bool
##   PlatformManager.layout_changed       → Signal(bp_name: String)

extends Node


# ---------------------------------------------------------------------------
# Signals
# ---------------------------------------------------------------------------

## Fired when the viewport crosses a breakpoint boundary.
signal layout_changed(bp_name: String)

## Fired on every viewport resize, even within the same breakpoint.
signal viewport_resized(new_size: Vector2)


# ---------------------------------------------------------------------------
# Breakpoint identifiers
# ---------------------------------------------------------------------------
const BREAKPOINT_XS := "xs"
const BREAKPOINT_SM := "sm"
const BREAKPOINT_MD := "md"
const BREAKPOINT_LG := "lg"
const BREAKPOINT_XL := "xl"


# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

## The currently active breakpoint string ("xs" … "xl").
var current_breakpoint: String = ""

## The OS name string returned by OS.get_name() at startup.
var platform_name: String = ""


func _ready() -> void:
	platform_name = OS.get_name()
	get_tree().root.size_changed.connect(_on_viewport_resized)
	_update_breakpoint()


# ---------------------------------------------------------------------------
# Platform Queries
# ---------------------------------------------------------------------------

## Returns true when running on Android or iOS.
func is_mobile() -> bool:
	return platform_name in ["Android", "iOS"]


## Returns true when running on a desktop operating system.
func is_desktop() -> bool:
	return platform_name in ["Windows", "macOS", "Linux", "FreeBSD", "NetBSD", "OpenBSD", "BSD"]


## Returns true when running inside the Godot editor (useful for layout preview).
func is_editor() -> bool:
	return Engine.is_editor_hint()


# ---------------------------------------------------------------------------
# Responsive Layout
# ---------------------------------------------------------------------------

## Returns the current viewport dimensions in physical pixels.
func get_viewport_size() -> Vector2:
	return get_tree().root.size


## Returns the active breakpoint string: "xs" | "sm" | "md" | "lg" | "xl".
func get_breakpoint() -> String:
	return current_breakpoint


## Returns true if the active layout is mobile-sized (xs or sm).
func is_mobile_layout() -> bool:
	return current_breakpoint in [BREAKPOINT_XS, BREAKPOINT_SM]


## Returns true if the active layout is tablet-sized or larger (md, lg, xl).
func is_tablet_or_larger() -> bool:
	return current_breakpoint in [BREAKPOINT_MD, BREAKPOINT_LG, BREAKPOINT_XL]


## Returns true when the sidebar navigation model should be visible.
## The sidebar shows on tablet+ breakpoints on non-mobile platforms.
## On mobile hardware the tab bar is always used regardless of window width.
func should_show_sidebar() -> bool:
	if is_mobile():
		return false
	return is_tablet_or_larger()


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

func _update_breakpoint() -> void:
	var width := get_viewport_size().x
	var new_bp: String

	if width < 480.0:
		new_bp = BREAKPOINT_XS
	elif width < 768.0:
		new_bp = BREAKPOINT_SM
	elif width < 1080.0:
		new_bp = BREAKPOINT_MD
	elif width < 1440.0:
		new_bp = BREAKPOINT_LG
	else:
		new_bp = BREAKPOINT_XL

	if new_bp != current_breakpoint:
		current_breakpoint = new_bp
		layout_changed.emit(current_breakpoint)


func _on_viewport_resized() -> void:
	_update_breakpoint()
	viewport_resized.emit(get_viewport_size())

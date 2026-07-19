## PlatformManager.gd
## Autoload singleton — detects the running platform and tracks the current
## responsive breakpoint.
##
## TWO ORTHOGONAL CONCERNS — do not conflate them:
##
##   1. LAYOUT MODE (sidebar vs tab bar) is driven purely by the *logical*
##      viewport width. A desktop window narrowed to phone width gets the
##      mobile layout; a tablet or a phone in landscape gets the desktop
##      layout. Query via should_show_sidebar() / is_mobile_layout().
##
##   2. INPUT MODE (touch target sizing, hover affordances) is driven by the
##      hardware. Query via is_touch_primary().
##
## A tablet in landscape is therefore "desktop layout" + "touch input" —
## a combination the old hardware-gated logic could not express.
##
## Breakpoints (from DESIGN_LANGUAGE.md §12), in DENSITY-INDEPENDENT units:
##   xs  — < 480dp    single-column mobile portrait
##   sm  — 480–768dp  mobile landscape / tablet portrait
##   md  — 768–1080dp tablet landscape / small desktop
##   lg  — 1080–1440dp standard desktop
##   xl  — > 1440dp   wide desktop
##
## Usage:
##   PlatformManager.is_mobile()           → bool  (hardware)
##   PlatformManager.is_touch_primary()    → bool  (input model)
##   PlatformManager.should_show_sidebar() → bool  (layout)
##   PlatformManager.layout_changed        → Signal(bp_name: String)

extends Node


# ---------------------------------------------------------------------------
# Signals
# ---------------------------------------------------------------------------

## Fired when the viewport crosses a breakpoint boundary.
signal layout_changed(bp_name: String)

## Fired after the viewport settles following a resize. Debounced by
## RESIZE_DEBOUNCE_SEC so a window drag does not emit tens of times per second.
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

## Seconds of quiet before viewport_resized fires. Prevents a desktop window
## drag from emitting tens of events per second, each triggering a relayout.
const RESIZE_DEBOUNCE_SEC := 0.1

var _resize_debounce: Timer

## Cached density scale — DPI does not change at runtime, so resolve it once.
var _density_scale: float = -1.0


func _ready() -> void:
	platform_name = OS.get_name()

	_resize_debounce = Timer.new()
	_resize_debounce.one_shot = true
	_resize_debounce.wait_time = RESIZE_DEBOUNCE_SEC
	_resize_debounce.timeout.connect(_emit_viewport_resized)
	add_child(_resize_debounce)

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


## Returns true when touch is the primary input model — i.e. the UI should use
## enlarged hit targets and must not depend on hover states.
##
## This is a HARDWARE question, deliberately separate from the layout question.
## A tablet in landscape gets the desktop *layout* but still needs touch targets.
## Set VAPOR_FORCE_TOUCH=1 to force touch mode on a desktop run. Lets the touch
## layout be inspected without a device — pair it with a narrow --resolution to
## approximate a phone. Reads the env var each call rather than caching so it can
## be toggled between runs without a rebuild.
func is_touch_primary() -> bool:
	if OS.has_environment("VAPOR_FORCE_TOUCH"):
		return true
	return is_mobile() or DisplayServer.is_touchscreen_available()


# ---------------------------------------------------------------------------
# Responsive Layout
# ---------------------------------------------------------------------------

## Returns the current viewport dimensions in LAYOUT PIXELS.
##
## This is the coordinate space Control nodes position themselves in, so it is
## what callers doing pixel maths (e.g. tab_bar pill placement) want. It is NOT
## density-corrected — use get_logical_size() for breakpoint decisions.
func get_viewport_size() -> Vector2:
	var root = get_tree().root
	var scale = root.content_scale_factor if "content_scale_factor" in root else 1.0
	return Vector2(root.size) / scale


## Returns the pixels-per-density-independent-unit ratio.
##
## Breakpoints are meaningless in raw physical pixels on mobile: a 1080x2400
## phone reports 1080px of width, which naively lands at the "lg" breakpoint and
## would hand a portrait phone the desktop sidebar. Dividing by this scale puts
## that same phone at ~411dp → "xs", which is correct.
##
## Desktop is left at 1.0 — the OS already reports logical points there, and
## Godot's content_scale_factor covers the HiDPI case.
func get_density_scale() -> float:
	if _density_scale > 0.0:
		return _density_scale

	_density_scale = 1.0
	if is_mobile():
		var dpi := DisplayServer.screen_get_dpi()
		# Guard against platforms that report a bogus/placeholder DPI.
		if dpi > 0:
			_density_scale = maxf(1.0, float(dpi) / 160.0)
	return _density_scale


## Returns viewport dimensions in density-independent units (dp).
## This is the basis for all breakpoint decisions.
func get_logical_size() -> Vector2:
	return get_viewport_size() / get_density_scale()


## Returns the active breakpoint string: "xs" | "sm" | "md" | "lg" | "xl".
func get_breakpoint() -> String:
	return current_breakpoint


## Returns true when the viewport is taller than it is wide.
func is_portrait() -> bool:
	var vp := get_viewport_size()
	return vp.y > vp.x


## Returns true if the active layout is mobile-sized (xs or sm).
func is_mobile_layout() -> bool:
	return current_breakpoint in [BREAKPOINT_XS, BREAKPOINT_SM]


## Returns true if the active layout is tablet-sized or larger (md, lg, xl).
func is_tablet_or_larger() -> bool:
	return current_breakpoint in [BREAKPOINT_MD, BREAKPOINT_LG, BREAKPOINT_XL]


## Returns true when the sidebar navigation model should be used.
##
## Purely a function of available width — no hardware gate. A phone rotated to
## landscape, or a tablet, gets the sidebar; a desktop window dragged narrow
## gets the tab bar. Layout follows the space available, not the device badge.
func should_show_sidebar() -> bool:
	return is_tablet_or_larger()


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

func _update_breakpoint() -> void:
	var width := get_logical_size().x
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
	# Breakpoint crossings are emitted immediately — layout_changed is already
	# self-rate-limiting (it only fires on an actual boundary crossing), and
	# delaying a layout swap would be visible as a lurch.
	_update_breakpoint()
	# The fine-grained resize signal is debounced; restarting the timer coalesces
	# a continuous window drag into a single emission once the drag settles.
	_resize_debounce.start()


func _emit_viewport_resized() -> void:
	viewport_resized.emit(get_viewport_size())

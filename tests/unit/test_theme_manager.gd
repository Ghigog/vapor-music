## test_theme_manager.gd
## GUT unit tests for ThemeManager design tokens.
## Run via the GUT panel in the Godot editor or `gut -gdir=res://tests`.
extends GutTest


func test_background_colors_have_correct_alpha() -> void:
	assert_eq(ThemeManager.current_theme.BG_VOID.a,  1.0, "BG_VOID must be fully opaque")
	assert_eq(ThemeManager.current_theme.BG_BASE.a,  1.0, "BG_BASE must be fully opaque")
	assert_lt(ThemeManager.current_theme.BG_GLASS.a, 1.0, "BG_GLASS must be semi-transparent")


func test_accent_core_is_blue_violet() -> void:
	# #7B6EF6 — hue sits between blue and violet.
	var c := ThemeManager.current_theme.ACCENT_CORE
	assert_gt(c.b, 0.9, "Accent blue channel should dominate")
	assert_lt(c.g, c.b, "Accent green channel should be less than blue")


func test_text_primary_is_near_white() -> void:
	var c := ThemeManager.current_theme.TEXT_PRIMARY
	assert_gt(c.r, 0.9, "TEXT_PRIMARY red channel near 1")
	assert_gt(c.g, 0.9, "TEXT_PRIMARY green channel near 1")
	assert_gt(c.b, 0.9, "TEXT_PRIMARY blue channel near 1")


func test_text_hierarchy_alpha_descending() -> void:
	assert_gt(
		ThemeManager.current_theme.TEXT_PRIMARY.a,
		ThemeManager.current_theme.TEXT_SECONDARY.a,
		"Primary text must be more opaque than secondary"
	)
	assert_gt(
		ThemeManager.current_theme.TEXT_SECONDARY.a,
		ThemeManager.current_theme.TEXT_TERTIARY.a,
		"Secondary text must be more opaque than tertiary"
	)
	assert_gt(
		ThemeManager.current_theme.TEXT_TERTIARY.a,
		ThemeManager.current_theme.TEXT_DISABLED.a,
		"Tertiary text must be more opaque than disabled"
	)


func test_radius_scale_is_ascending() -> void:
	assert_lt(ThemeManager.current_theme.RADIUS_XS,  ThemeManager.current_theme.RADIUS_SM)
	assert_lt(ThemeManager.current_theme.RADIUS_SM,  ThemeManager.current_theme.RADIUS_MD)
	assert_lt(ThemeManager.current_theme.RADIUS_MD,  ThemeManager.current_theme.RADIUS_LG)
	assert_lt(ThemeManager.current_theme.RADIUS_LG,  ThemeManager.current_theme.RADIUS_XL)
	assert_lt(ThemeManager.current_theme.RADIUS_XL,  ThemeManager.current_theme.RADIUS_2XL)


func test_spacing_scale_is_ascending() -> void:
	assert_lt(ThemeManager.current_theme.SPACE_1, ThemeManager.current_theme.SPACE_2)
	assert_lt(ThemeManager.current_theme.SPACE_2, ThemeManager.current_theme.SPACE_4)
	assert_lt(ThemeManager.current_theme.SPACE_4, ThemeManager.current_theme.SPACE_8)
	assert_lt(ThemeManager.current_theme.SPACE_8, ThemeManager.current_theme.SPACE_16)


func test_type_scale_is_ascending() -> void:
	assert_lt(ThemeManager.current_theme.TYPE_2XS, ThemeManager.current_theme.TYPE_XS)
	assert_lt(ThemeManager.current_theme.TYPE_XS,  ThemeManager.current_theme.TYPE_SM)
	assert_lt(ThemeManager.current_theme.TYPE_SM,  ThemeManager.current_theme.TYPE_BASE)
	assert_lt(ThemeManager.current_theme.TYPE_BASE, ThemeManager.current_theme.TYPE_MD)
	assert_lt(ThemeManager.current_theme.TYPE_MD,  ThemeManager.current_theme.TYPE_LG)
	assert_lt(ThemeManager.current_theme.TYPE_LG,  ThemeManager.current_theme.TYPE_XL)
	assert_lt(ThemeManager.current_theme.TYPE_XL,  ThemeManager.current_theme.TYPE_DISPLAY)


func test_make_glass_panel_returns_stylebox_flat() -> void:
	var style := ThemeManager.make_glass_panel()
	assert_is(style, StyleBoxFlat, "make_glass_panel() must return a StyleBoxFlat")


func test_make_glass_panel_respects_radius_param() -> void:
	var style := ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_XL)
	assert_eq(
		style.corner_radius_top_left,
		ThemeManager.current_theme.RADIUS_XL,
		"Corner radius must match the radius param"
	)


func test_make_glass_panel_respects_alpha_param() -> void:
	var style_default := ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_MD, 0.55)
	var style_opaque  := ThemeManager.make_glass_panel(ThemeManager.current_theme.RADIUS_MD, 0.90)
	assert_lt(
		style_default.bg_color.a,
		style_opaque.bg_color.a,
		"Higher alpha param must produce more opaque background"
	)


func test_fonts_initialized() -> void:
	assert_not_null(ThemeManager.current_theme.font_ui,      "font_ui must be initialized")
	assert_not_null(ThemeManager.current_theme.font_display, "font_display must be initialized")
	assert_not_null(ThemeManager.current_theme.font_mono,    "font_mono must be initialized")


func test_touch_target_meets_minimum() -> void:
	assert_gte(
		ThemeManager.current_theme.TOUCH_TARGET_MIN,
		44,
		"Touch target must be at least 44px (Apple HIG / WCAG)"
	)

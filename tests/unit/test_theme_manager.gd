## test_theme_manager.gd
## GUT unit tests for ThemeManager design tokens.
## Run via the GUT panel in the Godot editor or `gut -gdir=res://tests`.
extends GutTest


func test_background_colors_have_correct_alpha() -> void:
	assert_eq(ThemeManager.current_theme.BG_VOID.a,  0.0, "BG_VOID must be fully transparent")
	assert_lt(ThemeManager.current_theme.BG_BASE.a,  1.0, "BG_BASE must be semi-transparent/translucent")
	assert_lt(ThemeManager.current_theme.BG_GLASS.a, 1.0, "BG_GLASS must be semi-transparent")


func test_accent_core_is_system_blue() -> void:
	# #0A84FF (dark) / #007AFF (light) — Apple System Blue.
	# Red channel is near-zero, distinguishing it from the old blue-violet (#7B6EF6).
	var c := ThemeManager.current_theme.ACCENT_CORE
	assert_gt(c.b, 0.9, "Accent blue channel must dominate")
	assert_lt(c.g, c.b, "Accent green channel must be less than blue")
	assert_lt(c.r, 0.1, "Accent red channel must be near-zero (pure blue, not blue-violet)")


func test_text_primary_is_near_white() -> void:
	# The default dark theme (Vapor Dark) must have near-white primary text.
	var c := ThemeManager.current_theme.TEXT_PRIMARY
	assert_gt(c.r, 0.9, "TEXT_PRIMARY red channel near 1")
	assert_gt(c.g, 0.9, "TEXT_PRIMARY green channel near 1")
	assert_gt(c.b, 0.9, "TEXT_PRIMARY blue channel near 1")


func test_light_theme_text_primary_is_near_black() -> void:
	# Vapor Light uses near-black (#262626) for primary text — opposite of dark mode.
	var light := load("res://assets/themes/default_light.tres") as ThemeData
	assert_not_null(light, "default_light.tres must load as ThemeData")
	var c := light.TEXT_PRIMARY
	assert_lt(c.r, 0.2, "Light TEXT_PRIMARY red must be near-zero (near-black)")
	assert_lt(c.g, 0.2, "Light TEXT_PRIMARY green must be near-zero (near-black)")
	assert_lt(c.b, 0.2, "Light TEXT_PRIMARY blue must be near-zero (near-black)")


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


func test_font_scaling() -> void:
	var original_base = ThemeManager.current_theme.TYPE_BASE
	ThemeManager.set_base_font_size(20)
	
	assert_eq(ThemeManager.current_theme.TYPE_BASE, 20, "Base font size should be updated to 20")
	assert_eq(ThemeManager.current_theme.TYPE_LG, 24, "Title (TYPE_LG) should be base size + 4 (24)")
	assert_lt(ThemeManager.current_theme.TYPE_XS, ThemeManager.current_theme.TYPE_BASE, "Subtitle (TYPE_XS) should be smaller than base")
	
	# Restore original base font size
	ThemeManager.set_base_font_size(original_base)


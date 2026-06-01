# Vapor Music — Theme System Documentation

This document explains the architecture, components, and integration patterns for the dynamic theme system in Vapor Music. Every visual element in the application adapts dynamically to the active theme using this system.

---

## 1. System Architecture

The theme system consists of three key architectural parts:

```
┌─────────────────┐       ┌─────────────────┐       ┌──────────────────┐
│  ThemeData.gd   │ ─────►│ ThemeManager.gd │ ─────►│   UI Component   │
│ (Resource class)│       │ (Autoload Node) │       │  (Listens to     │
└─────────────────┘       └─────────────────┘       │  theme_changed)  │
                                                    └──────────────────┘
```

1. **`ThemeData.gd` (`class_name ThemeData`)**: The blueprint resource defining design tokens, color layers, accents, sizing, spacing scale, typography, and motion.
2. **`ThemeManager.gd` (Autoload Singleton)**: The single source of truth managing the active `ThemeData` state, exposing StyleBox builder factories, and broadcasting updates via signals.
3. **Themes (`.tres` Resources)**: Core configuration files representing colorways (e.g. `default_dark.tres`, `default_light.tres`) pre-loaded at runtime.

---

## 2. Design Tokens (`ThemeData`)

All variables are declared in `res://assets/theme_data.gd` and categorized as subgroups. Developers must always query these properties via `ThemeManager.current_theme`.

### Base Categories

- **Background Layers**: `BG_VOID` (opaque background), `BG_BASE` (sidebars/nav bars), `BG_ELEVATED` (cards), `BG_FLOAT` (modals/popups), `BG_GLASS` (semi-transparent frosted material base).
- **Glass Surfaces**: `GLASS_TINT`, `GLASS_BORDER` (top-left highlighting), `GLASS_BORDER_SUBTLE`, `GLASS_SHIMMER` (gloss gradient), `GLASS_BLUR` (24px std blur).
- **Text Hierarchy**: `TEXT_PRIMARY` (headings), `TEXT_SECONDARY` (artists, subtitles), `TEXT_TERTIARY` (hints, timestamps), `TEXT_DISABLED` (inactive state), `TEXT_INVERSE` (dark text on light accents).
- **Primary Accent ("Aurora Blue-Violet")**: `ACCENT_CORE`, `ACCENT_BRIGHT`, `ACCENT_DIM`, `ACCENT_GLOW`, `ACCENT_SURFACE`.
- **Secondary Accent ("Vapor Aqua")**: `AQUA_CORE` (waveform visualizer, loading, cloud sync), `AQUA_DIM`, `AQUA_GLOW`.
- **Semantic States**: `SEMANTIC_SUCCESS`, `SEMANTIC_WARNING`, `SEMANTIC_ERROR`, `SEMANTIC_INFO`.
- **Radius Scale (px)**: `RADIUS_XS` (6px) through `RADIUS_PILL` (9999px).
- **Spacing Scale (4px grid)**: `SPACE_1` (4px) through `SPACE_16` (64px).
- **Typography Scale (px)**: `TYPE_2XS` (11px) through `TYPE_DISPLAY` (48px).
- **Motion Durations**: `DURATION_INSTANT` (0.08s) through `DURATION_ATMOSPHERIC` (1.00s).

---

## 3. The Theme Manager (`ThemeManager`)

The `ThemeManager` is registered as a global autoload node in `project.godot`.

### Key Functions

- `load_theme(path: String) -> void`: Dynamic loader that loads a new `.tres` `ThemeData` resource file, re-calibrates fallback system fonts, and emits the `theme_changed` signal.
- `theme_changed` (Signal): Broadcasted to notify all listening controls that visual tokens have updated.

### StyleBox Flat Factories

All StyleBoxes are generated dynamically per call to prevent shared-mutation side effects.

- `make_glass_panel(radius: int, alpha: float) -> StyleBoxFlat`: Produces the signature frosted glass container with highlighted top-left borders and a soft drop shadow.
- `make_nav_panel() -> StyleBoxFlat`: Solid background stylebox with a thin right hairline separator.
- `make_nav_item_active() -> StyleBoxFlat`: Active sidebar button container with a left accent colored border.
- `make_nav_item_hover() -> StyleBoxFlat`: Translucent hover overlay panel.
- `make_cta_button(filled: bool) -> StyleBoxFlat`: CTA primary buttons.
- `make_circle_placeholder() -> StyleBoxFlat`: Clean pill/circular backgrounds (used for album art placeholders).

---

## 4. UI Component Integration Pattern

Every screen and UI control that needs to adapt to themes must follow a standard reactive pattern:

1. Connect to the `ThemeManager.theme_changed` signal in `_ready()`.
2. Define a clean `_apply_styles()` function.
3. Call `_apply_styles()` immediately during `_ready()` setup.

### Integration Template

```gdscript
## custom_card.gd
extends Control

@onready var title_label: Label = $Title
@onready var subtitle_label: Label = $Subtitle
@onready var action_btn: Button = $ActionBtn

func _ready() -> void:
	# 1. Connect signal safely
	ThemeManager.theme_changed.connect(_apply_styles)
	
	# 2. Call initial styling pass
	_apply_styles()

func _apply_styles() -> void:
	if not is_inside_tree():
		return
		
	var theme := ThemeManager.current_theme
	
	# Container Background
	add_theme_stylebox_override("panel", ThemeManager.make_glass_panel(theme.RADIUS_MD))
	
	# Typography & Font Overrides
	title_label.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	title_label.add_theme_font_override("font", theme.font_display)
	title_label.add_theme_font_size_override("font_size", theme.TYPE_MD)
	
	subtitle_label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	subtitle_label.add_theme_font_override("font", theme.font_ui)
	subtitle_label.add_theme_font_size_override("font_size", theme.TYPE_XS)
	
	# Button States
	action_btn.add_theme_stylebox_override("normal", ThemeManager.make_cta_button(false))
	action_btn.add_theme_stylebox_override("hover", ThemeManager.make_cta_button(true))
	action_btn.add_theme_color_override("font_color", theme.ACCENT_CORE)
```

---

## 5. Adding a New Theme

To create and add a custom theme:

1. Open the Godot FileSystem panel.
2. Right-click and choose **Create New -> Resource**.
3. Select `ThemeData` as the base resource class.
4. Save the file in `res://assets/themes/` (e.g., `synthwave.tres`).
5. Double-click the resource to inspect and configure customized color channels, scales, and accents in the Inspector panel.
6. Register the theme or trigger it via:
   ```gdscript
   ThemeManager.load_theme("res://assets/themes/synthwave.tres")
   ```

---

## 6. Unit Testing & Quality Controls

Theme integrity scales are fully verified via GUT unit tests located at `res://tests/unit/test_theme_manager.gd`.

### Running Tests

Execute the headless runner to perform diagnostic checks on ascending scales, alpha standards, and resource allocations:

```bash
/Applications/Godot.app/Contents/MacOS/Godot --headless --path . -s addons/gut/gut_cmdln.gd -gdir=res://tests/unit
```

### Verified Assertions

- Background alphas: VOID and BASE must remain fully opaque (1.0), while GLASS remains semi-transparent (< 1.0).
- Ascent curves: Spacing scales, typography sizes, and corner radius measures must ascend strictly to preserve responsive scales.
- Primary text colors: Must stay within high-contrast ranges (near-white in dark mode).
- Typography fallbacks: SystemFont structures must be fully initialized.

# Vapor Music — Architecture

**Version:** 0.1 (MVP)  
**Status:** Living Document

> This document describes the technical architecture of the Vapor Music Godot project.
> It is updated as features are added. Read alongside `docs/DESIGN_LANGUAGE.md`.

---

## Project Structure

```
vapor-music/
├── autoloads/           # Singletons (registered in project.godot)
│   ├── ThemeManager.gd  # All design tokens, StyleBox factories, fonts
│   ├── PlatformManager.gd  # OS detection, responsive breakpoints
│   └── NavManager.gd    # Navigation state and history
├── scenes/
│   ├── main.tscn        # Root scene — layout + screen host
│   ├── ui/
│   │   ├── sidebar/     # Desktop navigation sidebar
│   │   ├── tab_bar/     # Mobile floating glass-pill tab bar
│   │   └── mini_player/ # Persistent now-playing strip
│   └── screens/
│       ├── library/     # Music library browser
│       ├── search/      # In-library search (phase 2)
│       └── settings/    # App settings (phase 2)
├── scripts/             # .gd files — one per scene
│   ├── main.gd
│   ├── ui/
│   └── screens/
├── shaders/
│   └── frosted_glass.gdshader  # Canvas-item frosted glass effect
├── tests/
│   └── unit/            # GUT unit tests
├── addons/gut/          # GUT testing framework
├── core/                # (phase 2) audio engine, library, analyzer
├── sync/                # (phase 2) cloud and P2P sync
├── ai/                  # (phase 2) AI DJ / harmonic path construction
└── docs/
	├── DESIGN_LANGUAGE.md
	└── ARCHITECTURE.md
```

---

## Autoloads (Singletons)

### ThemeManager
Single source of truth for every visual token: colours, radii, spacing, durations,
font references, and `StyleBoxFlat` factory methods. All UI scripts source their
style values from here — never hardcode a colour or pixel value in a scene script.

**Key API:**
- Constants: `BG_VOID`, `ACCENT_CORE`, `TEXT_PRIMARY`, `RADIUS_MD`, `SPACE_6`, etc.
- Factories: `make_glass_panel(radius, alpha)`, `make_nav_panel()`, `make_cta_button(filled)`
- Fonts: `font_ui`, `font_display`, `font_mono` (SystemFont with design-specified fallback chains)

### PlatformManager
Detects the running OS and tracks the active responsive breakpoint.

**Breakpoints** (from `DESIGN_LANGUAGE.md §12`):

| ID  | Width       | Layout                      |
|-----|-------------|-----------------------------|
| xs  | < 480 px    | Single-column mobile        |
| sm  | 480–768 px  | Mobile landscape            |
| md  | 768–1080 px | Tablet / small desktop      |
| lg  | 1080–1440px | Standard desktop (sidebar)  |
| xl  | > 1440 px   | Wide desktop                |

**Key API:**
- `is_mobile()` / `is_desktop()` — hardware platform
- `is_mobile_layout()` — breakpoint is xs or sm
- `should_show_sidebar()` — true on desktop hardware at md+ breakpoint
- Signal `layout_changed(breakpoint)` — fires on breakpoint crossing
- Signal `viewport_resized(size)` — fires on any resize

### NavManager
Owns all in-app navigation state. Every navigation surface (sidebar, tab bar,
back buttons) calls `NavManager.navigate_to()` rather than manipulating screen
visibility directly.

**Key API:**
- `navigate_to(screen_name)` — pushes history, emits `navigation_requested`
- `go_back()` — pops history, used for Android back button
- Signal `navigation_requested(screen_name)` — main.gd listens to swap screens

---

## Responsive Layout

The root `Main` scene detects the platform via `PlatformManager.should_show_sidebar()`
and calls one of two layout methods:

**Desktop (`_apply_desktop_layout`):**
```
┌────────────┬──────────────────────────────┐
│  Sidebar   │        ScreenContainer       │
│  240 px    │                              │
│            ├──────────────────────────────┤
│            │      MiniPlayer  72px        │
└────────────┴──────────────────────────────┘
```

**Mobile (`_apply_mobile_layout`):**
```
┌──────────────────────────────────────────┐
│             ScreenContainer              │
├──────────────────────────────────────────┤
│             MiniPlayer  72px             │
│      ╔══════════════════════╗            │
│      ║  floating tab pill   ║            │
│      ╚══════════════════════╝            │
└──────────────────────────────────────────┘
```

Layout is re-applied whenever `PlatformManager.layout_changed` fires.

---

## Rendering

### Glass Panels (MVP)
In the MVP, glass panels are implemented as `PanelContainer` nodes with a
`StyleBoxFlat` carrying the semi-transparent `BG_GLASS` colour and a `GLASS_BORDER`
rule. This gives the colour and border feel of frosted glass without the blur pass.

### Glass Blur (Phase 2)
Full frosted-glass blur is implemented via:
1. A `SubViewport` capturing the content layer.
2. A `ColorRect` over the glass area with `ShaderMaterial` using `frosted_glass.gdshader`.

Performance budget: **max 4 active blur passes simultaneously** (see `DESIGN_LANGUAGE.md §13`).

### Dynamic Palette (Phase 2)
When a track loads, dominant colours are extracted from album art (k-means on a
64×64 thumbnail). The result drives a `Tween` on a `CanvasModulate` or shader
uniform in the Now Playing panel. Extraction runs on a background `Thread`.

---

## Navigation Flow

```
User taps nav item
		│
		▼
NavManager.navigate_to("screen")
		│
		├─► pushes previous screen to _history
		├─► sets current_screen
		└─► emits navigation_requested("screen")
				│
				├─► main.gd._on_navigation_requested  → toggles screen visibility
				├─► sidebar.gd._on_navigation_requested → updates active highlight
				└─► tab_bar.gd._on_navigation_requested → updates active tab
```

---

## Testing

The project uses the **GUT** (Godot Unit Testing) framework (`addons/gut`).

### Running Tests
1. Open the GUT panel: **Editor → GUT**
2. Set test directory to `res://tests/`
3. Click **Run All**

Or from the command line:
```bash
godot --headless -s addons/gut/gut_cmdln.gd -gdir=res://tests -gexit
```

### Test Files

| File | Covers |
|------|--------|
| `tests/unit/test_theme_manager.gd` | Color tokens, StyleBox factories, font init, scale ordering |
| `tests/unit/test_platform_manager.gd` | Platform detection, breakpoint values, viewport size |

---

## Phase Roadmap

| Phase | Goal | Key Features |
|-------|------|--------------|
| MVP (current) | App opens | Layout, navigation, design tokens, empty state |
| Phase 2 | Library import | File picker, SQLite metadata, library grid/list |
| Phase 3 | Playback | AudioStreamPlayer, mini-player, Now Playing screen |
| Phase 4 | AI DJ | BPM/key analysis, harmonic path, crossfade |
| Phase 5 | Cloud sync | WebDAV driver, P2P local discovery |
| Phase 6 | Polish | Full glass blur, dynamic palette, waveform visualiser |

---

*Built with Godot 4.6 · Designed for people who remember owning things.*

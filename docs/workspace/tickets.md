# Tickets

## Ticket Structure

Unique Identifier (like 001) : Title (status)
User story;
As a
Id Like to
So that

Context: (why)
Description: (what)
Requirements: (how)

Acceptance criteria (serves as basis for unit tests)
Given
When
Then

---

## Active Tickets Below

---

### VIS-001 : Rewrite Default Dark Theme to Apple Glass Palette (done)
**User Story:**
As a user,
I'd like the dark mode to feel modern and clean, like a native macOS/visionOS app,
So that the interface recedes and my music takes centre stage.

**Context:** The current dark theme uses a blue-violet accent with a near-black, slightly blue-tinted background that reads as "early 2000s". The new direction is Apple-system charcoal glass — cool, neutral, and translucent.

**Description:** Replace all colour tokens in `default_dark.tres` with the Apple Dark Glass palette. Deep charcoal window background at 55% opacity with heavy blur; pure-white text hierarchy; single `#0A84FF` accent throughout.

**Requirements:**
- `BG_VOID` / `BG_BASE` fully opaque charcoal, no blue tint
- `BG_GLASS` at 55% opacity (charcoal base)
- `GLASS_BORDER` white at 15% — subtle top/left edge highlight
- `TEXT_PRIMARY` = pure white, full opacity
- `TEXT_SECONDARY` = white at 60%
- `ACCENT_CORE` = `#0A84FF` (Apple Dark System Blue)
- No teal/aqua colour values in the active accent tokens
- All existing unit tests pass (BG_VOID opaque, BG_GLASS semi-transparent, radius ascending, etc.)

**Acceptance Criteria:**
- Given the app loads with the dark theme
- When the UI renders any glass panel
- Then the panel background is charcoal-grey, not blue-violet-tinted

---

### VIS-002 : Rewrite Default Light Theme to Apple Glass Palette (done)
**User Story:**
As a user who prefers a light interface,
I'd like the light mode to feel like frosted glass — bright, airy, and system-native,
So that it fits naturally on any macOS desktop.

**Context:** The current light theme reuses a blue-grey colour family that looks muted and inconsistent. The new Apple Light Glass palette uses pure-white panels at 50% opacity with heavy blur and a single `#007AFF` accent.

**Description:** Replace all colour tokens in `default_light.tres`. Window background `#F5F5F7`, glass panels white at 50%; primary text near-black `#262626`; secondary text `#8E8E93`; single accent `#007AFF`.

**Requirements:**
- `BG_VOID` = `#F5F5F7` (Apple light grey), fully opaque
- `BG_GLASS` = white at 50% — never below 40% (avoids muddy look)
- `GLASS_BORDER` = white at 80% — crisp top/left edge
- `GLASS_BORDER_SUBTLE` = black at 6% — subtle lower edge for depth
- `TEXT_PRIMARY` = `#262626` (near-black, 85% opacity of pure black)
- `TEXT_SECONDARY` = `#8E8E93` (Apple cool medium grey), full opacity
- `ACCENT_CORE` = `#007AFF` (Apple Light System Blue)
- `TEXT_INVERSE` = pure white
- Semantic colors (`SEMANTIC_SUCCESS` etc.) remain unchanged

**Acceptance Criteria:**
- Given the user switches to Vapor Light in Settings
- When any glass panel renders
- Then the panel is white-frosted (not grey or blue-tinted)
- And the accent colour is `#007AFF` blue, not blue-violet

---

### VIS-003 : Update ThemeManager StyleBox Factories for Glass Palette (done)
**User Story:**
As a developer,
I'd like `ThemeManager`'s factory methods to read border and shadow values from theme tokens,
So that StyleBoxes automatically adapt to both the light and dark glass palettes without hard-coded values.

**Context:** Several factory methods in `ThemeManager.gd` hard-code colour values (e.g. `Color(1,1,1,0.04)` for nav hover, `Color(0.024,0.024,0.039,0.50)` for shadow) that look wrong in light mode.

**Description:** Update `make_nav_panel()`, `make_nav_item_hover()`, and `make_glass_panel()` to use theme tokens. Shadow colour in `make_glass_panel()` should be a neutral dark at low opacity that works on both light and dark surfaces.

**Requirements:**
- `make_nav_item_hover()` → background uses `current_theme.GLASS_TINT` instead of `Color(1,1,1,0.04)`
- `make_nav_panel()` border → uses `current_theme.GLASS_BORDER_SUBTLE` instead of hard-coded `Color(1,1,1,0.05)`
- `make_glass_panel()` shadow → neutral `Color(0, 0, 0, 0.18)` at all times (soft, works on light & dark)
- No hard-coded colours remain in factory methods

**Acceptance Criteria:**
- Given `ThemeManager.make_glass_panel()` is called in light mode
- When the StyleBox is applied to a panel
- Then the shadow is a soft dark haze, not a near-black void

---

### VIS-004 : Rename Theme Selector Labels to Vapor Dark / Vapor Light (done)
**User Story:**
As a user,
I'd like the theme selector in Settings to show clear, brand-aligned names,
So that I know which theme I'm choosing.

**Context:** The `THEME_MAP` in `settings_screen.gd` currently uses generic names "Default Dark" and "Light Mode".

**Description:** Update keys to `"Vapor Dark"` and `"Vapor Light"` to reflect the new design identity.

**Requirements:**
- `THEME_MAP` dictionary keys updated to `"Vapor Dark"` and `"Vapor Light"`
- No changes to the associated `.tres` file paths

**Acceptance Criteria:**
- Given the Settings screen opens
- When the theme dropdown is shown
- Then the options read "Vapor Dark" and "Vapor Light"

---

### VIS-005 : Update Unit Tests for New Apple Blue Accent (done)
**User Story:**
As a developer,
I'd like the unit test suite to verify the new `#007AFF` / `#0A84FF` accent instead of the old `#7B6EF6` blue-violet,
So that regressions are caught if the accent is accidentally changed back.

**Context:** `test_accent_core_is_blue_violet()` checks that the blue channel > 0.9 and the green channel < blue. The new accent still satisfies both (blue = 1.0 in `#007AFF`), but the *intent* is now different and the red channel should be near zero, not moderate.

**Description:** Rename the test to `test_accent_core_is_system_blue()`. Add an additional assertion that `ACCENT_CORE.r < 0.1` to distinguish Apple blue from the old blue-violet. Add a `test_light_theme_text_primary_is_near_black()` test.

**Requirements:**
- Rename `test_accent_core_is_blue_violet()` → `test_accent_core_is_system_blue()`
- Add assertion `c.r < 0.1` (pure blue has minimal red)
- Add `test_light_theme_text_primary_is_near_black()` loading `default_light.tres` and asserting `TEXT_PRIMARY.r < 0.2`
- All other existing tests continue to pass

**Acceptance Criteria:**
- Given the test suite is run headless
- When all tests execute
- Then 0 failures and 0 errors are reported

---

### VIS-006 : Update Design Language Documentation (done)
**User Story:**
As a developer or designer joining the project,
I'd like the design language doc to accurately describe the current glass palette,
So that I can build new components correctly from day one.

**Context:** `docs/design_language.md` still describes the original blue-violet palette, the dual accent (Aurora + Aqua), and the Frutiger Aero aesthetic as co-primary.

**Description:** Update §1 (philosophy), §2.2 (dark palette tables), §2.3 (light palette), §2.4 (dynamic palette note), and Appendix A quick-reference card to reflect the Apple Glass system.

**Requirements:**
- §1: Position Apple macOS/visionOS as the primary lineage; Frutiger Aero remains as historical inspiration but is explicitly secondary
- §2.2: Replace all hex/token values with new charcoal glass values
- §2.3: Full rewrite to white glass / `#F5F5F7` / `#007AFF` system
- §2.4: Note the dynamic palette accent stays within the blue family
- Appendix A: Update colour references
- Add entry to changelog table

**Acceptance Criteria:**
- Given a developer reads design_language.md
- When they look up the primary accent hex
- Then they see `#007AFF` (light) / `#0A84FF` (dark), not `#7B6EF6`

---

### WIN-001 : Window Corner Resizing
**User Story:**
As a user,
I'd like to click and drag the corners/edges of the application,
So that I can resize the borderless application window like a typical desktop app.

**Context:** The application runs in borderless, transparent mode. Standard OS borders and resize handles are disabled, requiring custom drag-to-resize behavior at the edges and corners of the window.

**Description:** Implement resize detection regions on the outer edges and corners of the root Control node. When the mouse hovers, change the cursor shape to indicate resize capability. When clicked and dragged, adjust the OS window size and position dynamically.

**Requirements:**
- Define interactive resize zones (corners and optionally borders) with a thickness of 12px.
- Dynamically update default cursor shapes: `CURSOR_FDIAGSIZE` (top-left, bottom-right), `CURSOR_BDIAGSIZE` (top-right, bottom-left), `CURSOR_HSIZE` (left, right), `CURSOR_VSIZE` (top, bottom).
- When dragged, update `get_window().size` and `get_window().position` smoothly.
- Prevent window size from going below minimum size (`800x600`).
- Use screen/global coordinates (`DisplayServer.mouse_get_position()`) to calculate drag deltas to avoid jitter.

**Acceptance Criteria:**
- Given a desktop layout
- When hovering near the corners or edges of the window
- Then the mouse cursor changes to the appropriate resize cursor
- And dragging resizes the application window smoothly without jitter.

---

### WIN-002 : Sidebar Window Dragging
**User Story:**
As a user,
I'd like to click and drag the leftmost navigation panel (Sidebar),
So that I can reposition the application window on my desktop.

**Context:** Being borderless, the application has no title bar. The sidebar acts as the primary handle for moving the window.

**Description:** Connect mouse drag events on the Sidebar to move the parent OS window. Ensure buttons and interactive controls on the sidebar still respond to clicks normally.

**Requirements:**
- Handle GUI input on the Sidebar panel.
- Only trigger window move if the click was not consumed by child controls (buttons).
- Dragging uses relative mouse motion to update `get_window().position`.

**Acceptance Criteria:**
- Given a desktop layout
- When dragging the empty space of the sidebar
- Then the window moves smoothly tracking the mouse cursor.
- When clicking sidebar nav items, they trigger navigation rather than dragging the window.

---

### UI-001 : Vertical Progress Bar & Sidebar Player Tiles
**User Story:**
As a user,
I'd like a vertical progress bar sandwiched between the sidebar and the music list, and square windows-style media control tiles in the sidebar,
So that the playback controls feel sleek, minimalist, and integrated into the primary layout.

**Context:** The current horizontal bottom mini-player is less aligned with the vertical orientation of the music list and sidebar.

**Description:** Reposition the playback controls. Remove the bottom mini-player on desktop. Add a vertical progress bar running exactly along the separator line between the sidebar and the content frame. Add square, tiled buttons for Play/Pause, Forward, and Backward in the sidebar footer. Keep the bottom mini-player for mobile layout where sidebar is hidden.

**Requirements:**
- Desktop layout hides the horizontal `MiniPlayer` and stretches `ContentFrame` to fill the bottom area.
- Add a vertical `VSlider` (or a custom control) positioned exactly on the separator line `x = sidebar_width`.
- Style the vertical progress bar: track is invisible (using the sidebar border as the visual line), grabber thumb is a small circle (similar to current style).
- In the desktop sidebar footer, implement player tiles:
  - Play/Pause button: square button spanning full sidebar width.
  - Backward and Forward buttons: two square buttons placed side-by-side below Play/Pause, each taking half the sidebar width.
  - Display the current track name and artist name in the sidebar (e.g. as text above the player tiles).
- Ensure mobile layout still renders the bottom `MiniPlayer` and functions correctly.

**Acceptance Criteria:**
- Given desktop layout
- When playing music
- Then a vertical progress bar is visible on the line separating the sidebar and screen content.
- And the sidebar displays a square play/pause button and side-by-side forward/back buttons.
- And the bottom horizontal mini-player is hidden.
- Given mobile layout
- Then the horizontal mini-player is visible at the bottom and functional.

---

### VIS-007 : Setup Wizard Redesign & Settings Integration (done)
**User Story:**
As a user,
I'd like the Setup Wizard to match the modern glassmorphism aesthetic of the app, and be accessible at any time from the Settings screen,
So that I can easily connect or change my music library provider and adjust app settings like font sizes in one place.

**Context:** The old Setup Wizard was a basic placeholder using hardcoded dark styling. Users could not re-open the wizard after initial setup, and the Settings screen was off-centered and lacked font size configuration.

**Description:** Redesign the Setup Wizard modal container, fields, and buttons using `ThemeManager` styling. Center the Settings screen UI. Add a "Connect Music Library" button to launch the wizard from Settings, and implement universal font scaling.

**Requirements:**
- Center the settings screen using full anchors (`anchors_preset = 15`).
- Add a base font size setting that dynamically scales all typography elements (titles remain 4 points larger than base size).
- Add a Connect Music Library button to settings that launches the Setup Wizard.
- Update Setup Wizard modal panel, inputs, and buttons to use dynamic `ThemeManager` styling.
- Add a Cancel button to the Setup Wizard so users can dismiss it when opened from Settings.

**Acceptance Criteria:**
- Given the Settings screen is visible
- When the user modifies the base font size setting
- Then the text sizes of all UI elements scale dynamically.
- When the user clicks "Connect Music Library"
- Then the redesigned Setup Wizard modal is shown, pre-filling saved credentials and allowing cancellation.

---

### VIS-008 : Premium Glassmorphism Shader (done)
**User Story:**
As a user,
I'd like the UI panels to look like high-fidelity frosted glass (matching macOS/visionOS materials) even over a transparent background or when in-app components move underneath them,
So that the interface feels tactile, premium, and visually stunning.

**Context:** The standard shader relied on low-frequency blocky grain and a wide linear gradient fade border that didn't align with the StyleBox's rounded corners or behave realistically with lighting.

**Description:** Create a new custom shader `premium_glass.gdshader` that renders procedural rounded corner outlines utilizing an SDF representation of the container's layout size, applying a dual-stroke highlight (top-left gleam, bottom-right shadow), a 1-pixel resolution-independent frosted noise grain, a backdrop saturation/vibrancy booster, and a fallback for fully transparent regions.

**Requirements:**
- Implement in a new shader file `assets/shaders/premium_glass.gdshader`.
- Use a signed distance field (SDF) of a rounded rectangle inside the shader.
- Compute surface normals mathematically to shade the borders dynamically based on a top-left light source.
- Add fine frosted pixel-level noise using the screen-space fragment coordinates.
- Perform a saturation boost to blurred colors to prevent muddy colors.
- Maintain a fallback so the panels default to their flat translucent StyleBox colors if nothing is drawn behind the window.
- Dynamically bind and update the `container_size` uniform in `main.gd` on window resizing.
- All existing unit tests pass without errors.

**Acceptance Criteria:**
- Given the application runs with transparent settings
- When UI panels render or resize
- Then the SDF borders remain crisp, rounded, and align with the panel layout boundaries.
- And the shader compiles successfully and all unit tests pass.

---

### UI-002 : Confine Loading Animation to Playback Scrubber Bar (done)
**User Story:**
As a user,
I'd like the track loading animation to be confined to the progress bar area on mobile and show as a loading indicator on the vertical separator line on desktop,
So that the loading state is subtle, beautiful, and I can still see and interact with player controls during track changes.

**Context:** The mobile player bar used a `LoadingBar` that spanned the entire `PanelContainer` when visible, obstructing all navigation and playback buttons. In addition, the desktop view lacked any loading animation on its custom vertical progress bar. Confining it to the progress track on mobile and animating the vertical scrubber on desktop creates a consistent, premium user experience.

**Description:** 
- Mobile: Move `LoadingBar` inside `VBox/ProgressContainer` (MarginContainer) to overlay it exactly on top of `ProgressBar`. Adjust `@onready` references in `mini_player.gd`, update the scene connection paths, and hide the `ProgressBar` slider when the loading bar is active.
- Desktop: Listen to `AudioManager.loading_track` in `vertical_progress.gd`. When loading, disable the standard progress fill and grabber rendering, and instead animate a themed `AQUA_CORE` circle moving smoothly up and down the track line using a sine wave.

**Requirements:**
- Nest `ProgressBar` and `LoadingBar` inside a `ProgressContainer` (`MarginContainer`) in `mini_player.tscn`.
- Update `@onready` node paths in `mini_player.gd`.
- Update signal connection paths in `mini_player.tscn`.
- Hide `progress_bar` when `loading_bar` is active (`is_loading = true`) in `_on_loading_track`.
- Add `is_loading` variable, `AudioManager.loading_track` connection, and `_process()` rendering animation logic in `vertical_progress.gd`.

**Acceptance Criteria:**
- Given mobile layout during track change loading
- When a track is loading (`is_loading` is true)
- Then the loading bar overlays the progress bar path exactly, and the main player controls remain visible and interactive.
- Given desktop layout during track change loading
- When a track is loading (`is_loading` is true)
- Then the vertical progress bar animates an aqua-colored dot traveling up and down the separator track line, while disabling standard progress drawing.

---

### MET-001 : Sidebar Preview Square & Metadata Populator (done)
**User Story:**
As a user,
I'd like to see a preview square in the left side panel that displays the artist image, album cover, or song lyrics depending on what I last touched,
So that I feel connected to the visual and textual elements of my music.

**Context:** The user wants a dynamic preview square in the sidebar, which is updated based on user interactions:
- Artist selection -> Artist image.
- Album selection -> Album cover art.
- Song selection -> Heavily blurred background image with lyrics overlaid.

**Description:** Implement a square visual container in the sidebar between the buttons and options above. Wire it to show artist, album, and song metadata based on user interactions.

**Requirements:**
- Add an AspectRatioContainer called `PreviewSquare` in the sidebar layout.
- Ensure the preview square occupies a perfect square space layout-wise.
- Display artist image when an artist is clicked/pressed.
- Display album cover art when an album is clicked/opened.
- When a song is clicked, apply a heavy blur over the current image and display the lyrics text.
- Connect the preview square to `MetadataService` to fetch images and lyrics.

**Acceptance Criteria:**
- Given the desktop sidebar is visible
- When an artist is clicked in the library screen
- Then the preview square displays the fetched artist image.
- When an album is clicked
- Then the preview square displays the fetched album cover.
- When a song is clicked
- Then the preview square applies a heavy blur to the image and displays the lyrics text.

---

### MET-002 : Robust Metadata & Track Parsing (done)
**User Story:**
As a user,
I'd like the app to correctly parse my track paths and filenames,
So that albums are not split into multiple artists and unknown albums due to numeric track prefixes or compilation tracks.

**Context:**
Filenames formatted like "01 - Track Title.mp3" or compilation layouts were matching a generic " - " split and assigning the track number as the artist and leaving the album as unknown.

**Description:**
Update the parser in `library_screen.gd` to strip track number/alphanumeric prefixes first, handle single-folder parent-only directory fallbacks, and combine folder and file segments in a prioritized hierarchy.

**Requirements:**
- Implement track number prefix stripping (handles `01`, `1`, `A1`, `1-01`).
- Parse single folder parent directory structure correctly as the album name (no "Unknown Album" for `/Music/Album Name/Track.mp3`).
- Set priority: file-parsed artist/album first, then directory-level fallback, then defaults ("Unknown Artist"/"Unknown Album").
- Write comprehensive unit tests for different path variations.

**Acceptance Criteria:**
- Given a file path `/Music/Discovery/01 - Intro.mp3`
- When parsed
- Then the album is resolved to "Discovery", the artist is "Unknown Artist", and the track is "Intro".

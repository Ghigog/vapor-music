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

**Context:** `docs/DESIGN_LANGUAGE.md` still describes the original blue-violet palette, the dual accent (Aurora + Aqua), and the Frutiger Aero aesthetic as co-primary.

**Description:** Update §1 (philosophy), §2.2 (dark palette tables), §2.3 (light palette), §2.4 (dynamic palette note), and Appendix A quick-reference card to reflect the Apple Glass system.

**Requirements:**
- §1: Position Apple macOS/visionOS as the primary lineage; Frutiger Aero remains as historical inspiration but is explicitly secondary
- §2.2: Replace all hex/token values with new charcoal glass values
- §2.3: Full rewrite to white glass / `#F5F5F7` / `#007AFF` system
- §2.4: Note the dynamic palette accent stays within the blue family
- Appendix A: Update colour references
- Add entry to changelog table

**Acceptance Criteria:**
- Given a developer reads DESIGN_LANGUAGE.md
- When they look up the primary accent hex
- Then they see `#007AFF` (light) / `#0A84FF` (dark), not `#7B6EF6`

---

### WIN-001 : Window Corner Resizing (done)
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

### WIN-002 : Sidebar Window Dragging (done)
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

### UI-001 : Vertical Progress Bar & Sidebar Player Tiles (done)
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

---

### MET-003 : Local Library Caching and Background Sync (done)
**User Story:**
As a user,
I'd like the application to load my music library instantly on startup using a local cache,
So that I don't have to wait 5 seconds for a full WebDAV server scan every time I launch the app.

**Context:**
Currently, the application initiates a full recursive WebDAV directory scan on startup. For large libraries, this takes 5+ seconds and blocks navigation/display of the track list. By caching the track paths locally and performing the network synchronization in the background, we can display the library in less than a second.

**Description:**
Implement a persistent local JSON cache at `user://library_cache.json` in `WebDAVService`. Load this cache immediately on startup, and trigger the WebDAV server scan as a background task. If the background scan discovers any discrepancies (added or deleted files), update the local cache file, and rebuild the library tree UI seamlessly. Log time elapsed for both cache loading and background sync.

**Requirements:**
- Add `load_cached_library()` and `save_cached_library()` in `WebDAVService` using JSON serialization under `user://library_cache.json`.
- Populate `scanned_files` from cache on startup and emit `library_scanned` immediately if cached files are found.
- Time the execution duration of cache loading and deep server scanning, logging results.
- In `scan_music_directory()`, compare server results with the cached `scanned_files`. Only update the cache and emit `library_scanned` if discrepancies exist (to avoid unnecessary UI rebuilding), while ensuring any loading states (e.g. manual refresh) are dismissed.

**Acceptance Criteria:**
- Given saved credentials exist
- When the app is launched
- Then the cached library is loaded and displayed in less than a second
- And the background WebDAV sync runs silently and updates the cached list only if differences are found.

---

### AI-001 : Local Track Waveform & Energy Analysis (done)
**User Story:**
As a listener,
I'd like the application to analyze my imported music files locally,
So that musical properties like BPM, key, and energy can be extracted for smooth mixing.

**Context:** The app's core USP is harmonic mixing. Before tracks can be blended seamlessly, we must extract and index their key characteristics locally without third-party servers.

**Description:** Implement the `AudioAnalyzer` service that runs on a background thread (`Thread` class) to parse cached audio files, decode audio samples (PCM bytes), and run fast frequency analysis to detect beats per minute (BPM), musical key signatures (mapped to the Camelot Wheel), and perceived energy level curves.

**Requirements:**
- Perform audio parsing sequentially in a background thread to prevent UI stutter/frame drops.
- Implement dynamic waveform peak/beat envelope detection to extract average BPM.
- Analyze frequency spectrum arrays to identify dominant pitches and assign a key signature (e.g., 8A, 11B).
- Classify perceived energy/mood levels and generate a compressed energy profile (array of values).

**Acceptance Criteria:**
- Given an audio track with a known BPM and key
- When processed by the AudioAnalyzer on a background thread
- Then the calculated BPM is within ±2% margin of error
- And the Camelot key corresponds correctly to the track's musical key signature
- And the main UI thread remains fully responsive (no frame rate drops) during analysis.

---

### AI-002 : Database Schema & Metadata Tag Storage (done)
**User Story:**
As a developer,
I'd like to extend the metadata cache schema to support BPM, musical key, energy ratings, energy graphs, and listening history,
So that the player can query these properties instantly during playback and queue generation.

**Context:** The existing metadata cache at `user://metadata_cache.json` only stores artist, album, track name, local paths, and lyrics. We need structured fields to record the analyzer results, listening history, and energy graphs as detailed in our system architecture.

**Description:** Update `MetadataService` and `SettingsManager` to support database schemas/fields for `bpm`, `musical_key`, `energy_level`, `energy_graph`, and `listening_history`. Ensure metadata reader/writer processes these new fields.

**Requirements:**
- Extend the `metadata_cache.json` structure to include `bpm` (float), `musical_key` (String), `energy_level` (float), `energy_graph` (Array of floats), and `listening_history` (Array of floats/ints representing play timestamps).
- Update `MetadataService.lookup_metadata()` to populate and save these fields.
- Implement tag fallback parsing (e.g., reading ID3 TBP tags for BPM and TKEY for musical key).

**Acceptance Criteria:**
- Given a newly analyzed track
- When metadata is saved to the local JSON cache
- Then the JSON cache contains valid entries for `bpm`, `musical_key`, `energy_level`, `energy_graph`, and `listening_history`
- And reloading the cache returns the exact properties.

---

### AI-003 : Harmonic Shuffle / DJ Mood-Pathing Algorithm (done)
**User Story:**
As a listener,
I'd like the shuffle button to generate a transition queue sorted by harmonic compatibility and energy level,
So that track transitions do not feel jarring or disjointed.

**Context:** Traditional shuffle selections feel erratic. Using the Camelot Wheel (adjacent keys like 8A and 9A or 8B) and matching energy levels allows us to transition tracks seamlessly.

**Description:** Implement a playlist generator algorithm that builds a path between compatible tracks, arranging songs in the queue based on closest BPM differences, Camelot Wheel compatibility, and energy curves.

**Requirements:**
- Define standard Camelot Wheel transition rules (adjacent numbers, same number opposite letter).
- Implement a search/sorting algorithm to arrange the queued tracks along a smooth energy curve.
- Ensure the track skipping flow respects this dynamically calculated queue.

**Acceptance Criteria:**
- Given a list of tracks with diverse keys and tempos
- When harmonic shuffle is triggered
- Then each successive track in the queue is adjacent to the prior track on the Camelot Wheel (or matches it)
- And the queue contains a smooth gradient of energy levels.

---

### AI-004 : Intelligent Cross-Fading Playback Engine (done — in the Tauri shell)
**User Story:**
As a listener,
I'd like the player to align beats and dynamically transition between tracks,
So that the exit of one track blends seamlessly into the intro of the next.

**Context:** Traditional crossfading only fades volume linear-style, which sounds amateurish. Aligning beats and transition boundaries using a dual-deck playback system provides a professional DJ experience.

**Description:** Extend `AudioManager` with a dual-deck playback system utilizing primary and secondary `AudioStreamPlayer` nodes. Implement beat-matching and intro/outro overlap detection to transition audio tracks dynamically.

**Requirements:**
- Implement dual `AudioStreamPlayer` nodes to support playing two tracks simultaneously during a transition.
- Calculate exit and entry alignment points (overlapping the outro of the old track with the intro of the next based on the cached `energy_graph` profile).
- Adjust the playback speed (tempo) of the incoming track by adjusting `pitch_scale` by ±2% to sync beat boundaries.
- Implement a volume crossfade transition driven by `Tween` or frame-based updates mapped to the crossfade audio bus.

**Acceptance Criteria:**
- Given the current track is approaching its outro boundary
- When a transition begins
- Then the incoming track starts playing on the secondary player, matches the exit beat timing, and crossfades volumes in sync with the beat grid.

**Status (2026-08-16):** Done in `vapor-app`, and the "active" here was stale —
it was the Godot ticket, never revisited after the port. The supervisor decodes
the next track ~30 s before the outgoing one's `cue_out` and schedules a
beat-matched mix (TD-25); all six transition types exist (TD-20/MIG-008); beat
grids never cross to the audio thread, since alignment is computed on the
control side and only a ratio and a cue position are sent. Dual
`AudioStreamPlayer` nodes became two decks in `vapor-engine`, and `pitch_scale`
became time-stretching in `vapor-engine` — Signalsmith Stretch natively and
WSOLA on wasm, which is what TD-22 settled.

---

### AI-005 : Playlist Visualizer & Camelot Wheel UI (completed)
**User Story:**
As a user,
I'd like a visual dashboard showing Camelot Wheel connections and the upcoming mix transitions,
So that I can see and adjust how my tracks are being blended together.

**Context:** Users should see the "vibe" represented visually to understand the AI DJ's pathing decisions and adjust them interactively.

**Description:** Design a dashboard widget showing the Camelot Wheel, highlighting active/upcoming keys, and displaying the transition timeline showingoutro/intro overlaps.

**Requirements:**
- Draw a custom vector visualizer of the Camelot Wheel showing currently queued keys.
- Show an interactive timeline overlay indicating transition points, BPM offsets, and volume crossfade curves.
- Add sliders to let users adjust crossover duration and energy thresholds.

**Acceptance Criteria:**
- Given the now playing screen is active
- When the mix visualizer is opened
- Then the visual Camelot Wheel highlights the current and next track keys
- And adjusting the crossover slider updates the playback transition timing instantly.

---

### EQ-001 : Godot AudioServer Multiband Parametric EQ Layout (completed)
**User Story:**
As a developer,
I'd like to implement a multi-band parametric equalizer layout in the audio server,
So that individual frequency bands can be adjusted dynamically at runtime.

**Context:** Godot supports adding audio effects to buses. We need to scaffold a dedicated master effects bus featuring a parametric equalizer with configurable bands.

**Description:** Setup a master EQ effect bus layout programmatically or via editor settings. Expose interfaces in `AudioManager` to modify frequency, Q-factor, and gain of individual bands.

**Requirements:**
- Add an `AudioEffectEQ` or multiple `AudioEffectFilter` bands to the master bus.
- Expose methods in `AudioManager` to configure bands (e.g., `set_eq_band(frequency, gain, q)`).
- Ensure audio routing does not introduce distortion or latency when filters are active.

**Acceptance Criteria:**
- Given a master output channel
- When a band gain is modified via the API
- Then the master audio signal reflects the frequency attenuation or boost.

---

### EQ-002 : AutoEQ Calibration Profile Parser (completed)
**User Story:**
As a developer,
I'd like to parse parametric filter parameters from standard AutoEQ text profiles,
So that headphone calibration settings can be imported directly into our EQ filter engine.

**Context:** The AutoEQ project outputs text files containing specific frequency bands, gains, and Q-values. We need to parse these values to apply them automatically.

**Description:** Implement a parser script `autoeq_parser.gd` to read and convert AutoEQ parametric filter files into structured Godot dictionary data.

**Requirements:**
- Parse text configurations representing parametric bands (typically 5 to 10 bands containing Center Frequency, Gain, and Q-factor).
- Map parsed variables directly to the Godot `AudioServer` parametric EQ filter properties.
- Validate values to prevent clipping (e.g. automatically adjust preamp/post-gain).

**Acceptance Criteria:**
- Given a raw AutoEQ calibration text file
- When loaded by the parser
- Then a structured array of bands is returned with correct frequency, gain, and Q-factor mappings.

---

### EQ-003 : Settings Headphone Calibration Selection UI (completed/removed)
**User Story:**
As a user,
I'd like to select my headphone model from a list in the Settings screen,
So that the correct corrective EQ profile is applied automatically.

**Context:** The Settings screen needs an elegant selector interface showing supported headphone models, with search capability, and a master calibration toggle. (Deprecated/Removed on 2026-06-27: This feature was removed from the settings screen because it was half-baked).

**Description:** Redesign settings options to include an AutoEQ section. Add a searchable OptionButton/dropdown for headphone models, a master bypass switch, and target visual graphs showing the correction curve. (Removed on 2026-06-27)

**Requirements:**
- Build a list of common headphone presets (packaged inside a local JSON or fetched from a repository).
- Bind the item selection to trigger `EQ-002` parser and update `EQ-001` audio bus filters.
- Draw a clean vector graph showing the corrective frequency curve applied.

**Acceptance Criteria:**
- Given settings screen is active
- When the user selects a headphone model and enables calibration
- Then the EQ parameters update in the audio server
- And a graphical representation of the target response curve is displayed.

---

### SYNC-001 : Local Subnet Peer Discovery (mDNS/UDP) (done)
**User Story:**
As a user with multiple devices,
I'd like the apps to discover each other automatically on my local Wi-Fi subnet,
So that I don't have to input IP addresses manually to start syncing.

**Context:** Local synchronization should feel seamless. Automatic discovery removes the technical friction of manual IP setup.

**Description:** Implement multicast DNS (mDNS) or UDP broadcast discovery inside a network synchronization service (`SyncService`). Broadcast active device properties and listen for peers on the subnet.

**Requirements:**
- Set up a UDP broadcaster sending device identities on a specific port.
- Implement a UDP listener socket to collect discovered peer IPs, device names, and client types.
- Maintain an active peers directory in `SyncService`.

**Acceptance Criteria:**
- Given two client instances running on the same local Wi-Fi network
- When subnet discovery is active
- Then both devices automatically populate their active peers lists with each other's details.

---

### SYNC-002 : Secure Direct Pairing & Session Handshake (done)
**User Story:**
As a user,
I'd like to authorize and pair devices using a numeric PIN code,
So that unauthorized local network devices cannot read or modify my music library.

**Context:** Security is vital on shared networks. A pairing handshake prevents unauthorized connections from querying files or configurations.

**Description:** Build a secure handshake handler that executes when a connection is initiated. Display a pairing modal showing a PIN code, verifying it on the peer to establish an encrypted session.

**Requirements:**
- Implement a secure handshake protocol using Diffie-Hellman or basic token validation.
- Display a temporary 6-digit PIN modal.
- Verify matching credentials and record trusted device IDs locally.

**Acceptance Criteria:**
- Given a pairing request from a client
- When a matching PIN is input on both devices
- Then the connection is authenticated, and the devices are stored as trusted peers.

---

### SYNC-003 : Database Reconciliation Protocol (done)
**User Story:**
As a developer,
I'd like to compare and reconcile track metadata between paired devices,
So that missing tracks and updated ratings are identified before transfers start.

**Context:** Reconciling databases requires calculating delta diffs representing missing, updated, or orphaned tracks without transferring redundant assets.

**Description:** Develop a reconciliation engine that serializes the local track database to light hash maps, exchanging them over the network connection to calculate transfer tasks.

**Requirements:**
- Generate SHA-256 track fingerprints representing database records.
- Compare fingerprints to identify missing tracks on either device.
- Compare track update timestamps to handle ratings or playlist updates.

**Acceptance Criteria:**
- Given two reconciled databases
- When synced
- Then a delta list of missing files and metadata updates is generated correctly.

---

### SYNC-004 : Direct Peer File Transfer Pipeline (done)
**User Story:**
As a user,
I'd like missing files to transfer rapidly over local sockets,
So that my music library is synchronized without cloud bandwidth limits.

**Context:** Syncing can involve gigabytes of audio files. Direct TCP/WebSocket streaming provides high-speed file transfers on local routers.

**Description:** Implement a parallel file-chunk sender and receiver socket architecture to transfer audio streams and album art directly.

**Requirements:**
- Set up a dedicated TCP file transfer port or WebSocket server.
- Stream files in binary chunks, verifying integrity via MD5 checksum checks.
- Support resume logic for interrupted file transfers.

**Acceptance Criteria:**
- Given a transfer task of 5 files
- When execution begins
- Then the files stream over the local socket at maximum network speed
- And completed transfers match the source file checksums.

---

### SYNC-005 : Peer-to-Peer Synchronization Dashboard UI (done)
**User Story:**
As a user,
I'd like a sync dashboard in Settings to view paired devices, see transfer progress, and initiate local syncs,
So that I have full visibility and control over my data transfers.

**Context:** Users need visual feedback during large sync processes to see file progress, speeds, and connection status.

**Description:** Create a dashboard panel in Settings showing active connections, pairing status, a "Sync Now" CTA button, and real-time progress bars for file transfers.

**Requirements:**
- Render a list of discovered and paired devices with active connection indicators.
- Display a sync progress bar showing percentage, speed (MB/s), and current file name.
- Provide toggles to filter what synced (e.g., playlists only, track files, meta only).

**Acceptance Criteria:**
- Given a synchronization is active
- When viewing the settings sync panel
- Then the interface updates progress bars in real-time, showing transfer speed and remaining counts.

---

### AI-006 : Vibe Workbench & Metadata Improvements (done)
**User Story:**
As a listener,
I'd like my "Next" skip actions to honor suggested tracks, unanalyzed songs to be prioritized upon play, and a persistent status bar to track background analysis progress and fix genre lookups,
So that the AI DJ transitions and metadata elements feel seamless, reliable, and premium.

**Context:** Skip actions bypassed prepared overrides, unanalyzed tracks were harmonically path-sorted using dummy values, and the Deezer search structure broke genre lookups.

**Description:** Adjust skip mechanics to prioritize `upcoming_track_override`. Group and append unanalyzed tracks in the pathfinder. Elevate played track analysis to index `0` of the background thread queue. Implement a status label in the header showing analysed-to-total track ratios. Rewrite Deezer search queries to the standard `Artist Album` format.

**Requirements:**
- Skip button uses `upcoming_track_override` if present.
- Played tracks enqueued at front of `AudioAnalyzer` background queue.
- `WebDAVService.scanned_files` records complete scanned file collection.
- `vibe_screen.tscn` contains an `AnalysisStatus` label in an HBox header layout.
- Status bar displays `⏳ Analyzing library... X / Y ready` and `✓ All Y tracks analyzed`.
- Deezer search syntax changed from `artist:"..." album:"..."` to `Artist Album` with fallback logic.

**Acceptance Criteria:**
- Given a suggested track is clicked as upcoming
- When the user clicks "Next"
- Then the app transitions directly to that track.
- Given the vibe screen is loaded
- Then the header displays the persistent background analysis status.

**Note/Fix:**
- Fixed an issue where locally cached tracks were not automatically analyzed on startup or folder scan, causing the status bar to show `1 / 230 ready` instead of including all cached tracks. Connected `WebDAVService.library_scanned` to `AudioAnalyzer.scan_library_cache` so that all cached tracks are automatically queued and analyzed in the background when the library is loaded or scanned.

---

### AI-007 : AI DJ Smart Mixing & Path Preference (done)
**User Story:**
As a listener,
I'd like to toggle "Smart Mixing" via a checkbox instead of a button shuffle, and see distinct Match, Fresh, and Switch options with AI preferred choices highlighted in the vibe menu,
So that the transition flows are predictable, diverse, and I can customize or override them at will.

**Context:** The old "Harmonic Shuffle" button permanently re-sorted the playlist, and suggestions looping back to the same 2-3 tracks reduced song variety.

**Description:**
Convert UI shuffle buttons to toggle CheckBoxes for "Smart Mixing". If enabled, transitions utilize smart matches (Match, Fresh, Switch) following a repeating 4-step AI choice sequence. If disabled, transitions fall back to playing tracks sequentially. Highlight the selected track (AI preference or user manual override) and badge the AI choice.

**Requirements:**
- Rename shuffle buttons to "Smart Mixing" and convert to `CheckBox` nodes.
- Sync checkbox toggle states globally via `AudioManager.smart_mixing_enabled`.
- Implement `calculate_smart_matches()` in `DJPathfinder` resolving Match, Fresh, and Switch candidates.
- Implement repeating 4-step AI choice sequence (Perfect -> Interesting -> Perfect -> Creative/Interesting 50-50).
- Dynamically render suggestion cards as "Match", "Fresh", and "Switch" in the Vibe Workbench.
- Highlight selected next track and display a `🤖 AI Choice` badge on the preferred choice card.

**Acceptance Criteria:**
- Given Smart Mixing is disabled
- When a track finishes playing
- Then the next track is chosen sequentially from the playlist.
- Given Smart Mixing is enabled
- When the vibe screen is loaded
- Then exactly three cards (Match, Fresh, Switch) are rendered, with the AI's current sequence choice marked with a `🤖 AI Choice` badge.

**Note/Fix:**
- Updated the "ACTIVE AI BLEND MODE" UI label to display the upcoming transition info throughout track playback instead of "AI DJ standby" or "Ready — next blend selected by AI". The label now dynamically updates to show `Intended: Blend to [Track Title] via [Transition Type]` based on active playlist context, smart mixing modes, or user manual overrides, and recalculates the transition type whenever overrides change. Modified `_on_transition_completed` to call `_refresh_display()` directly, eliminating any residual "Ready — next blend selected by AI" state messages.

---

### PLAY-001 : Playlist Management Feature (done)
**User Story:**
As a listener,
I'd like to create, rename, delete, and manage playlists, with drag-and-drop support for tracks and custom cover art,
So that I can organize my music library dynamically.

**Context:** The app needs a playlist feature with JSON persistence, artwork fallback logic, and drag-and-drop.

**Description:**
Implement PlaylistService managing playlists in user://playlists.json. Design dynamic sidebar playlist items with renaming LineEdits. Create a dedicated PlaylistScreen with edit-in-place title controls, cover art uploading (including drag-and-drop from local filesystem), track reordering drag handles, and a frosted glassmorphism empty state.

**Requirements:**
- PlaylistService manages playlists, user://playlists.json persistence, and custom image copy.
- Fall back to first track's album art when custom art is not set.
- Draggable tracks in LibraryScreen returning track type and href.
- Sidebar list supporting dynamic creation (+ button), renaming, deletions, and drop targets.
- PlaylistScreen displaying active playlist details (title, cover, track list).
- Playlist track list supporting reorder handles, remove buttons, and drag-and-drop track reordering.
- Test suite passing successfully with 0 errors.

**Acceptance Criteria:**
- Given a list of playlists
- When a new playlist is created
- Then it is persisted to JSON and displayed in the sidebar
- When tracks are dragged from library and dropped on a sidebar playlist or the playlist screen
- Then they are appended or inserted at the correct index.

---

### DJ-001 : Implement Echo Out (Delay Fade) Transition (done)
**User Story:**
As a listener,
I'd like the player to perform an Echo Out transition when transitioning between tracks with large BPM differences or different genres,
So that the exit of the outgoing song rings out cleanly and allows the incoming song to drop in with a clean energy change.

**Context:** Fading between completely different tempos or genres (e.g. Rock to Electro, or 90 BPM to 140 BPM) sounds jarring if the beats clash. Cutting the outgoing song and letting a feedback delay tail ring out creates a professional "outro" effect that naturally masks the tempo jump.

**Description:**
Add an `AudioEffectDelay` to the audio server decks. When "Echo Out" is triggered, feed the outgoing deck into the delay with a 1/2 beat or 3/4 beat time setting, cut the primary/clean stream at the transition midpoint (4.0s) while leaving the delay feedback to naturally decay, and play the incoming deck clean.

**Requirements:**
- Add `AudioEffectDelay` programmatically at index 3 of the `DeckA` and `DeckB` buses in `_setup_audio_buses()`.
- Define standard feedback and delay time parameters (e.g., 0.5s time, 0.4 feedback).
- When a transition is "Echo Out", keep both decks at full volume (0 dB) initially.
- At the transition midpoint (4.0s), mute the primary stream of the outgoing player by bypassing it or lowering its volume before the delay effect, or setting the delay's wet mix to 1.0 and dry to 0.0, or cutting its clean playback while keeping the delay tail audible.
- Fade in the incoming deck clean to 0 dB in the first half of the transition.
- Ensure the delay effect settings are properly reset in `_reset_bus_effects()`.

**Acceptance Criteria:**
- Given a transition is designated as Echo Out
- When the midpoint is reached
- Then the outgoing song's clean signal cuts immediately, and a delay echo effect decays over the remaining transition duration while the incoming song plays at full volume.

---

### DJ-002 : Implement Reverb Freeze Transition (done)
**User Story:**
As a listener,
I'd like the player to perform a Reverb Freeze transition when playing creative mood shifts,
So that the transition is characterized by a spacious ambient fade-out rather than a basic volume drop.

**Context:** Ambient washes are excellent for bridging tracks with huge stylistic discrepancies. Freezing the reverb of the outgoing track and letting the tail wash out is a standard DJ technique to create a smooth, ethereal transition.

**Description:**
Add an `AudioEffectReverb` to the audio server decks. When "Reverb Freeze" is triggered, increase the room size and decay of the outgoing deck's reverb to maximum (freezing it), cut the dry output of the outgoing deck at the midpoint, and let the spacious reverb tail decay as the incoming track drops in.

**Requirements:**
- Add `AudioEffectReverb` programmatically at index 4 of the `DeckA` and `DeckB` buses in `_setup_audio_buses()`.
- When transition type is "Reverb Freeze", set the outgoing deck's Reverb wet level to 0.0 initially, then ramp it up to 1.0 approaching the midpoint.
- At the midpoint (4.0s), set dry level to 0.0 (cutting the dry song) and set Reverb room size to 1.0 and damping to 0.0 to create a "frozen" tail that ring decays.
- Fade in the incoming deck clean in the first half.
- Reset the reverb effects to default (wet 0.0, dry 1.0) in `_reset_bus_effects()`.

**Acceptance Criteria:**
- Given a transition is designated as Reverb Freeze
- When the midpoint is reached
- Then the outgoing song's dry signal cuts, and its reverb tail rings out like an ambient cloud while the incoming song starts playing.

**Refinement (2026-06-17):**
- Prevented digital clipping and ripping artifacts by adding a parallel outgoing bus volume tween in the first half (ramping from 0.0 dB to -6.0 dB) to compensate for the combined dry (1.0) and wet (ramping to 1.0) signals.
- Stopped the outgoing player at the transition midpoint via a tween callback to freeze the audio input. This prevents any post-midpoint audio from feeding into and overloading the reverb processor.
- Faded out the outgoing bus volume from -6.0 dB to -60.0 dB over the second half of the transition to let the frozen reverb tail decay naturally and disappear smoothly without abrupt cutoffs.

---

### DJ-003 : Implement Tempo Morph (Tempo-Sync'd) Transition (done)
**User Story:**
As a listener,
I'd like the player to match the tempos of both tracks exactly during a transition and then return to the native tempo,
So that the rhythm and beats stay perfectly locked during the crossfade.

**Context:** For medium BPM differences (e.g. 3.0 to 8.0 BPM), playing them at their native tempos during a crossfade causes the beats to slide against each other (trainwrecking). Matching their tempos during the blend and morphing the tempo post-transition keeps the rhythm locked.

**Description:**
Dynamically calculate the transition BPM midpoint. Scale the `pitch_scale` of both players during the transition so they match the midpoint tempo exactly. Once the transition completes, slowly ramp the `pitch_scale` of the active player to its native tempo (1.0) over 5-10 seconds.

**Requirements:**
- During the crossfade, calculate target BPM as the average of the two tracks, or match the outgoing track's BPM.
- Adjust `pitch_scale` of both players based on the difference between their native BPMs and the target BPM.
- Create a post-transition tween that ramps the active player's `pitch_scale` back to 1.0 over a configurable ramp duration (e.g. 6.0 seconds).
- Ensure `pitch_scale` modifications do not introduce digital artifacts.

**Acceptance Criteria:**
- Given a transition is designated as Tempo Morph
- When the transition is running
- Then the playback speeds of both players are adjusted so their BPMs match exactly.
- And when the transition completes, the new track's speed slowly slides back to its native tempo.

---

### DJ-004 : Implement Real DJ Matching and Harmonic Modulations (done)
**User Story:**
As a listener,
I want the AI DJ to use real harmonic modulations and mask key clashes using transitions,
So that the music suggested by the AI DJ is more diverse and transitions sound like a professional DJ set.

**Context:** The previous implementation used heavy key penalties for all key differences, leading the AI DJ to only recommend tracks in the same key (10A -> 10A/9A). Transition effects were mapped without considering key compatibility.

**Description:**
Add `get_harmonic_relation_cost` in `dj_pathfinder.gd` classifying key relations (Exact, Mode Shift, Step, Diagonal, Energy Boost/Drop, Power fifth, Subdominant). Update `calculate_transition_cost` to use this cost. Refactor `calculate_smart_matches` to enforce harmonic key sets for `perfect` and modulated key sets for `interesting`, and ignore key clashes for `creative` match. Update transition choice logic in `_update_upcoming_transition()` in `audio_manager.gd` to select masking transitions (Echo Out/Reverb Freeze) for key clashes.

**Requirements:**
- Implement dynamic harmonic key modulations in `dj_pathfinder.gd` to reduce the cost penalty for Energy Boosts, Power Fifth Mixes, etc.
- Update `calculate_smart_matches` to restrict `perfect` to compatible keys, `interesting` to modulations in the same genre, and `creative` to different genre ignoring key distance.
- Update transition effect selection in `audio_manager.gd` to select wash/masking effects (Echo Out/Reverb Freeze) for clashing keys.

**Acceptance Criteria:**
- Given a key clash
- When the transition is selected
- Then the transition type is either Echo Out or Reverb Freeze.
- Given smart matches are calculated
- Given smart matches are calculated
- Then the perfect match uses harmonically compatible keys, and the interesting match uses key modulations.

---

### DJ-005 : Scaffold C++ DSP Layer (GDExtension) & Integrate Rubber Band & Essentia (done)
**User Story:**
As a developer,
I'd like to compile and bind Essentia and the Rubber Band Library to Godot via GDExtension,
So that we have high-performance C++ audio analysis and time-stretching capabilities.

**Context:** GDScript is too slow for full-file digital signal processing (DSP), and Godot's built-in `pitch_scale` couples speed and pitch, distorting vocals. We need native C++ libraries for advanced analysis and independent time-stretching.

**Description:** Scaffold a GDExtension C++ project in the repository. Configure the build environment to compile native libraries for macOS, and provide scripts/stubs for mobile (Android/iOS). Bind Essentia for local file sweeps and Rubber Band for real-time time-stretching, exposing them as an `AudioDSP` node class.

**Requirements:**
- Create a `src/` directory with a standard GDExtension boilerplate (`register_types.cpp`, `register_types.h`).
- Write an `AudioDSP` C++ class extending `Node` (`audio_dsp.h`, `audio_dsp.cpp`).
- Write a `SConstruct` file configured to compile the source code into a dynamic library under `bin/`.
- Download and link compiled static libraries for Essentia (and its dependencies like libsamplerate, fftw) and the Rubber Band Library for macOS.
- Register `AudioDSP` class methods within the Godot ClassDB.
- Expose basic verification methods (e.g., library version queries) to GDScript to test binding validity.

**Testing Requirements:**
- Create a new unit test suite `tests/unit/test_audio_dsp.gd` extending `GutTest`.
- Implement `test_audio_dsp_node_registration()`: Assert that `ClassDB.class_exists("AudioDSP")` is true.
- Implement `test_audio_dsp_node_instantiation()`: Assert that `AudioDSP.new()` returns a valid instance that does not crash the engine.
- Implement `test_audio_dsp_method_presence()`: Assert that the instance responds to `get_library_version()`, `analyze_pcm()`, and `stretch_buffer()`.

**Acceptance Criteria:**
- Given the Godot project is opened and run
- When the engine initializes
- Then the `AudioDSP` GDExtension is loaded successfully, and the class can be instantiated via `AudioDSP.new()`.

---

### AI-008 : High-Fidelity Audio Analysis & Local Metadata Caching (done)
**User Story:**
As a listener,
I'd like my music library to be analyzed using real DSP key, BPM, and segment detection,
So that track transitions and paths are calculated using accurate, non-faked data.

**Context:** The current MP3 key/BPM analysis is faked using a CRC32 byte-hash modulo, and WAV key detection is hardcoded to `8A` with a 6-second analysis cap. We need a real offline DSP scan of the entire audio file using the new `AudioDSP` GDExtension.

**Description:** Overhaul `audio_analyzer.gd` to stream audio data to `AudioDSP`. Perform onset detection for a list of beat timestamps (`beat_grid`), Chromagram analysis for Camelot keys, and spectral flux for structural segments (intro, outro, chorus), saving the results to `user://metadata_cache.json`.

**Requirements:**
- Remove the faked CRC32 modulo math and the 1MB file read ceiling inside `audio_analyzer.gd`.
- Stream audio file PCM data to the `AudioDSP` GDExtension module.
- Retrieve a complete `beat_grid` array of floats (beat timestamps in seconds).
- Retrieve a `downbeats` array of floats representing the "one" beat of each bar.
- Calculate the accurate average BPM across the entire track.
- Extract the musical key signature and format it into Camelot notation (e.g., `8A`).
- Detect structural segment boundaries (timestamps for intro, chorus, breakdown, outro).
- Serialize the results in `user://metadata_cache.json` matching the new expanded JSON schema.

**Testing Requirements:**
- Add unit tests to `tests/unit/test_audio_analyzer.gd`.
- Implement `test_essentia_analysis_integration()`: Mock `AudioDSP` and assert that `_perform_analysis()` calls it and retrieves the mapped keys, average BPM, and beat grids.
- Implement `test_caching_expanded_fields()`: Load an analyzed track metadata cache and assert that `beat_grid`, `downbeats`, and `segments` are successfully serialized to the JSON cache file and retrieved with their proper types.

**Acceptance Criteria:**
- Given an audio track with known musical properties (BPM, key, structure)
- When background analysis runs
- Then the cached metadata contains accurate keys, BPM, beat grid timestamps, and segment boundaries.

---

### MET-004 : Silence Trimming & EBU R128 Loudness Normalization (done)
**User Story:**
As a listener,
I'd like tracks to play at consistent volumes and start without silent delays,
So that transitions flow seamlessly and do not suffer from sudden volume spikes or dead air.

**Context:** Differences in loudness between highly-compressed modern tracks and dynamic recordings create an uneven listening experience, and silence at the start/end of audio files disrupts transition pacing.

**Description:** In `audio_analyzer.gd`, sweep the audio amplitude to find start and end cue points (trimming silence). Calculate EBU R128 integrated loudness (LUFS) during analysis. In `audio_manager.gd`, apply a pre-fade gain multiplier to match a target loudness of `-14 LUFS` on each deck.

**Requirements:**
- Scan PCM samples to find the first and last timestamps where the average amplitude rises above `-40dB` (representing `cue_in` and `cue_out`).
- Implement an EBU R128 loudness analysis algorithm (K-weighting filter + gated RMS) to calculate integrated LUFS.
- Save `cue_in`, `cue_out`, and `lufs` in `user://metadata_cache.json`.
- In `audio_manager.gd`, parse the cached `cue_in` and play the incoming track starting from that offset rather than `0.0`.
- Apply a pre-fade gain multiplier on `DeckA` and `DeckB` buses: `Gain = 10^((Target_LUFS - Track_LUFS) / 20)` (Target = `-14.0 LUFS`).

**Testing Requirements:**
- Create a new unit test suite `tests/unit/test_audio_normalization.gd` extending `GutTest`.
- Implement `test_silence_detection_threshold()`: Generate a mock audio buffer with 2 seconds of silence followed by active audio. Assert that `cue_in` is detected at exactly 2.0 seconds.
- Implement `test_lufs_loudness_calculation()`: Feed a reference sine wave at a known amplitude and assert that the calculated LUFS value matches standard reference targets within a 0.5dB tolerance.
- Implement `test_gain_multiplier_calculation()`: Verify that the gain multiplier applied to the audio bus correctly scales with tracks of various LUFS values.

**Acceptance Criteria:**
- Given a track with a 4-second silent intro and a high volume level
- When analyzed and played
- Then playback begins instantly at the `cue_in` timestamp (4.0s) and its volume is attenuated to match the target reference loudness.

---

### DJ-006 : Beat Grid Sync & Phase-Locked Loop (PLL) Deck Alignment (done)
**User Story:**
As a listener,
I'd like overlapping tracks to have their beats perfectly synchronized,
So that kick drums hit at the exact same millisecond and do not trainwreck.

**Context:** The current system plays the incoming track from 0.0 blindly without aligning beat phases, resulting in messy percussive clashes during transitions.

**Description:** Rebuild the playback trigger in `audio_manager.gd` using cached `beat_grid` and `downbeats` arrays. Match the phase of incoming beats to outgoing beats, and run a phase-locked loop (PLL) to dynamically correct timing drift between the decks.

**Requirements:**
- Retrieve the `beat_grid` and `downbeats` arrays for both outgoing and incoming tracks.
- Calculate the target start sample offset for the incoming track so its Beat 1 aligns with the next phrasing downbeat of the outgoing track.
- Implement a software PLL in the manager's `_process()` loop running at 60fps.
- Calculate the phase difference (in milliseconds) between the playing decks' beat timestamps.
- Apply micro-adjustments to the playback rate of the incoming deck (within ±0.5%) to keep the beat boundaries locked in phase.

**Testing Requirements:**
- Create a new unit test suite `tests/unit/test_beat_sync_pll.gd` extending `GutTest`.
- Implement `test_beat_alignment_offset_calculation()`: Given a mock outgoing track beat grid and an incoming track beat grid, verify the computed start offset aligns the downbeats.
- Implement `test_pll_drift_correction_loop()`: Inject timing drift values in a mocked playback loop and assert that the PLL's rate adjustments successfully drive the timing offset back to zero.

**Acceptance Criteria:**
- Given two tracks playing during a transition
- When the crossfade runs
- Then the beat grids of both tracks remain locked in phase with sub-millisecond drift.

---

### DJ-007 : True Time-Stretching & Pitch-Locked Tempo Morphing (done)
**User Story:**
As a listener,
I'd like the tempo of clashing BPM tracks to match during a transition without pitch distortions,
So that vocals and melodies maintain their natural keys.

**Context:** The current "Tempo Morph" transition modifies Godot's built-in `pitch_scale`, which couples tempo and pitch, creating chipmunk or demonic vocal warping. We need independent time-stretching.

**Description:** Integrate the Rubber Band Library via the `AudioDSP` GDExtension to process audio buffers. Dynamically calculate target midpoint BPMs, feeding the stretch ratios to the time-stretch engine while keeping the key pitch-locked.

**Requirements:**
- Calculate the midpoint target BPM: `(BPM_out + BPM_in) / 2`.
- Calculate the required time-stretch ratios for both the outgoing and incoming decks.
- Route audio buffers through Rubber Band's processing engine in `AudioDSP`.
- Keep the `pitch_scale` of the Godot players locked at `1.0` while stretching the time domain dynamically.
- Implement a post-transition glide that slowly ramps the active deck's time-stretch ratio back to `1.0` over 6 seconds.

**Testing Requirements:**
- Create a new unit test suite `tests/unit/test_tempo_stretching.gd` extending `GutTest`.
- Implement `test_morph_ratio_calculations()`: Assert that target BPM calculations and corresponding stretch ratios are mathematically correct.
- Implement `test_pitch_remains_locked()`: Mock a morph transition and verify that the `pitch_scale` property of the `AudioStreamPlayer` nodes remains constant at `1.0` throughout the blend.
- Implement `test_post_transition_speed_glide()`: Verify that the post-transition speed restoration glide runs over the exact configured duration (6.0s) and interpolates smoothly to 1.0.

**Acceptance Criteria:**
- Given a transition is running with a 10 BPM difference
- When the tempo morphs
- Then the tempo matches at the midpoint while both tracks maintain their native pitch without chipmunk effects.

---

### DJ-008 : Phrase-Aware Structural Transitions & Vocal Clashing Masking (completed)
**User Story:**
As a listener,
I'd like transitions to trigger at natural phrasing boundaries and avoid vocal-on-vocal clashes,
So that mixes flow musically and do not sound cluttered.

**Context:** Transitions currently trigger on absolute time countdowns, often cutting off drops, starting in the middle of choruses, or overlapping vocals.

**Description:** Update `audio_manager.gd` to trigger transitions at structural outro segment boundaries aligned to 8-bar loops. Detect vocal presence in the transition window and apply mid-range EQ carving or alternate transition styles.

**Requirements:**
- Translate the playback positions of both decks from seconds into bars and beats based on the beat grid.
- Trigger transitions strictly on the first downbeat of the outro segment that lands on a standard 8-bar loop boundary.
- Read vocal presence metadata (extracted via spectral density during analysis).
- If both outgoing and incoming tracks have vocal presence, apply a deep mid-frequency EQ cut (e.g. -24dB) on one deck or switch to a hard-cut transition (Echo Out/Reverb Freeze).

**Testing Requirements:**
- Add unit tests to `tests/unit/test_dj_transitions.gd`.
- Implement `test_transition_trigger_alignment()`: Mock playing a track with structural segments, and assert that the transition trigger is scheduled for the first downbeat of the outro segment on an 8-bar boundary.
- Implement `test_vocal_clash_detection_and_masking()`: Mock a blend where both tracks contain vocals in the overlap window, and assert that a mid-range EQ attenuation of at least -20dB is dynamically applied to one of the channels.

**Acceptance Criteria:**
- Given both tracks contain vocals in the overlap zone
- When the transition triggers at the 8-bar outro boundary
- Then the system automatically applies mid-frequency isolation to prevent vocal clashing.

---

### DJ-009 : Subtractive Dynamic EQ & Real-Time Frequency RMS Compression (completed)
**User Story:**
As a listener,
I'd like overlapping frequency bands to be mixed subtractively,
So that basslines do not muddy or cause digital clipping.

**Context:** Static tweens can cause loudness spikes and muddy bass overlaps.

**Description:** Implement a subtractive mixing model driven by a crossfader variable ($X \in [0.0, 1.0]$). Swap low EQ bands at the midpoint while keeping total bass energy at 0dB. Implement real-time frequency RMS monitoring and compression.

**Requirements:**
- Define a crossfader progress variable $X$ driven by a transition tween.
- Calculate low EQ gains: `Bass_out = clamp(1.0 - 2X, 0.0, 1.0)` and `Bass_in = clamp(2X - 1.0, 0.0, 1.0)`.
- Implement real-time RMS metering on Low, Mid, and High bands.
- If the sum of any frequency band energy exceeds `+2dB` relative to reference, compress or reduce gain on the outgoing deck to prevent digital clipping.

**Testing Requirements:**
- Create a new unit test suite `tests/unit/test_subtractive_eq.gd` extending `GutTest`.
- Implement `test_subtractive_bass_gain_formula()`: Assert that the combined bass energy formula resolves to exactly `1.0` (0dB) across various points of the crossfade ($X = 0.0$, $X = 0.5$, $X = 1.0$).
- Implement `test_rms_clipping_prevention()`: Feed high-amplitude audio signals into both channels during a simulated crossfade and assert that the output signal gain is dynamically attenuated/compressed to prevent clipping.

**Acceptance Criteria:**
- Given the crossfader is at 50% ($X = 0.5$)
- When a transition is running
- Then the total low-frequency energy remains constant at 0dB, avoiding muddy bass overload or digital clipping.

---

### AI-009 : A* Global Set Pathfinder & Genre Taxonomy Mapping (completed)
**User Story:**
As a listener,
I'd like the AI DJ to plan a cohesive set path with smooth tempo and energy progressions,
So that the playlist feels intentional.

**Context:** The current pathfinder only looks one track ahead, causing random BPM leaps and genre jumps.

**Description:** Rewrite `dj_pathfinder.gd` to perform global pathfinding using an A* algorithm over the queue. Integrate a static subgenre taxonomy tree. Implement multi-track BPM slope and energy curves (e.g. "Build Vibe", "Chill Down").

**Requirements:**
- Create a JSON genre taxonomy tree mapping subgenres.
- Replace the greedy search with an A* algorithm calculating paths over a 10-song sequence.
- Penalize sudden genre leaps and BPM/energy spikes.
- Factor in target energy curves to plan gradual transitions.
- Derive transition duration dynamically based on outro/intro segment lengths.

**Testing Requirements:**
- Add unit tests to `tests/unit/test_dj_pathfinder.gd`.
- Implement `test_a_star_pathfinding_optimization()`: Load a mock metadata library and verify that the A* algorithm generates a path that matches the overall target slope of a chosen energy curve (e.g., "Build Vibe").
- Implement `test_genre_taxonomy_cost()`: Assert that transition paths between related subgenres (e.g., Tech House -> Techno) are assigned lower cost penalties than unrelated ones (e.g., House -> Liquid DNB).
- Implement `test_dynamic_duration_selection()`: Assert that transition duration adapts properly to outro and intro lengths.

**Acceptance Criteria:**
- Given a library with wide BPMs and genres
- When a 10-track playlist path is generated
- Then the queue displays a gradual BPM gradient and harmonic/genre compatibility across the entire sequence.

---

### AI-010 : Listener Preference Learning & Adaptive Transition Weighting (completed)
**User Story:**
As a listener,
I'd like the AI DJ to learn my transition preferences based on my skip behavior,
So that future suggestions align with my tastes.

**Context:** Algorithms should adapt to user preferences over time.

**Description:** Implement skip-metric tracking. Log successful blends and apply dynamic weight penalties to pathfinding nodes that have triggered user skips during transitions.

**Requirements:**
- Create a transaction log at `user://transition_history.json`.
- Monitor if a user skips a track during or within 10 seconds of a transition.
- Record the transition details (Track A, Track B, effect used, and outcome).
- Integrate a feedback weight in `dj_pathfinder.gd`'s cost function to penalize previously skipped paths.

**Testing Requirements:**
- Create a new unit test suite `tests/unit/test_preference_learning.gd` extending `GutTest`.
- Implement `test_skip_history_logging()`: Trigger a simulated track skip event within the transition window and verify that the event is logged in `user://transition_history.json`.
- Implement `test_preference_cost_penalty()`: Add a history entry showing a specific transition was skipped, and verify that the pathfinder assigns a higher cost to that edge in its A* evaluation.

**Acceptance Criteria:**
- Given the user skips Track B multiple times when transitioned from Track A via Echo Out
- When the pathfinder runs
- Then the cost for this specific transition rises, preventing the AI from selecting it again.

---

### DJ-010 : Segmented Key Modulation & Waveform-Aware Transient Avoidance (completed)
**User Story:**
As a listener,
I want the AI DJ to account for key changes within tracks and avoid clashing with major drops/hits,
So that transitions sound clean.

**Context:** Tracks that modulate keys are misclassified, and transitions that step on drops disrupt the energy.

**Description:** Support per-segment key tracking to match the incoming track to the outro key. Perform transient peak detection to micro-shift transition start times.

**Requirements:**
- Analyze per-segment key signatures during high-fidelity analysis.
- Match incoming tracks using the key of the outro segment.
- Detect major transients (drums, drops, hits) in the transition window.
- Micro-shift transition start times (by a few beats) to avoid cutting off drops or colliding peaks.

**Testing Requirements:**
- Create a new unit test suite `tests/unit/test_segmented_key_transients.gd` extending `GutTest`.
- Implement `test_outro_key_matching()`: Mock a track modulating keys in its outro segment and assert that `dj_pathfinder.gd` matches candidates based on the outro key.
- Implement `test_transient_peak_avoidance()`: Mock a high-amplitude transient (a drop) at the planned transition downbeat, and assert that the transition trigger is micro-shifted by a calculated beat offset.

**Acceptance Criteria:**
- Given a track that modulates to 9A in the outro
- When matched by the pathfinder
- Then the pathfinder selects an incoming track compatible with 9A (rather than the track's initial key of 8A).
- And the transition start is slightly shifted to avoid overlapping a major transient peak.

---

### DJ-011 : Threaded Real-Time Time-Stretching & Buffer Feeding (completed)
**User Story:**
As a listener,
I'd like the audio time-stretching to run in a background thread,
So that the main UI thread never stutters and the audio stream remains completely free of pops or click artifacts during transitions.

**Context:** Time-stretching two streams concurrently in the main `_process` thread via Rubber Band is extremely CPU-intensive and can cause frame drops and buffer underruns if the frame rate dips.

**Description:** Implement a thread-safe ring buffer and background thread execution in `AudioDSP` or `AudioManager` to handle Rubber Band chunk processing, allowing the main thread's `_feed_deck` to immediately fetch pre-rendered samples.

**Requirements:**
- Create a background thread (using Godot's `Thread` or C++ threads) dedicated to processing Rubber Band time-stretching.
- Implement a thread-safe circular buffer (ring buffer) in the GDExtension or GDScript.
- `_feed_deck` should pull chunks from the buffer in O(1) time on the main thread, rather than computing them.

**Testing Requirements:**
- Add unit tests to `tests/unit/test_tempo_stretching.gd` to verify threaded feed behavior.
- Verify that the main thread is never blocked during simultaneous time-stretching.

**Acceptance Criteria:**
- Given a transition running on two decks
- When time-stretching is active
- Then no frame drops or audio buffer underruns occur.

---

### DJ-012 : Smooth Subtractive Bass Crossfade (completed)
**User Story:**
As a listener,
I'd like the bass EQ frequencies to swap smoothly over a brief crossfade window around the midpoint,
So that the low-end transition does not feel like an abrupt binary cut.

**Context:** The current bass swap instantly cuts outgoing bass and jumps incoming bass at exactly 50% crossfader progress, creating a sudden energy drop/jump.

**Description:** Replace the binary midpoint swap in `_update_subtractive_eq()` with a smooth, short crossfade curve (e.g., 0.5s to 1.0s) centered at X = 0.5, while still maintaining the sum of low-frequency energy at or below 0dB to prevent clipping.

**Requirements:**
- Calculate smooth cross-fade gain levels for the low band using a power-complementary curve or linear fade over the interval X in [0.45, 0.55] (or a configurable duration).
- Maintain total bass sum energy at 0dB.

**Testing Requirements:**
- Add tests in `tests/unit/test_subtractive_eq.gd` to assert that bass gains cross smoothly and sum energy is validated.

**Acceptance Criteria:**
- Given a Bass Swap transition
- When the crossfader passes X = 0.5
- Then the bass frequencies blend smoothly rather than snapping instantly.

---

### DJ-013 : Multi-Pass Recursive Transient Peak Avoidance (completed)
**User Story:**
As a listener,
I'd like the transient avoidance system to guarantee that transition start times avoid all major peaks,
So that the mix never drops on top of consecutive drum hits or climax drops.

**Context:** The current system only shifts the trigger time once by 1 bar and doesn't verify if the new time contains a collision.

**Description:** Implement a recursive or looping transient check in `get_aligned_trigger_time()` to shift the trigger by successive bars until a trigger point is found that is completely free of transient peaks on both decks.

**Requirements:**
- Implement a loop/recursion in `get_aligned_trigger_time()` that checks for collisions at the candidate `t_trigger`.
- If a collision is detected within the safety margin, shift `t_trigger` by 1 bar (4 beats) and repeat the collision check (up to a maximum of 4 attempts to prevent infinite loops).

**Testing Requirements:**
- Add tests to `tests/unit/test_segmented_key_transients.gd` verifying multi-pass avoidance when consecutive transients are present.

**Acceptance Criteria:**
- Given a track with transient peaks at bar boundaries
- When the transition is scheduled
- Then the final scheduled trigger time is verified to be completely free of collisions.

---

### DJ-014 : Cross-Correlation Auto-Sync & Phase Refinement (completed)
**User Story:**
As a listener,
I'd like the playback engine to automatically correct any beat alignment errors by analyzing the audio waveforms directly,
So that the transition beats stay perfectly locked even if the initial beatgrid analysis was slightly off.

**Context:** Without manual user controls, any automated beatgrid detection error will cause a permanent, uncorrected phase clash (trainwreck) during the mix.

**Description:** Implement a waveform cross-correlation phase-refinement algorithm in the `AudioDSP` C++ layer. Periodically compute the cross-correlation of down-sampled PCM signals from both decks in the transition window, locate the peak shift, and feed it into the PLL to nudge the phase into perfect alignment.

**Requirements:**
- Down-sample the active playback channels in C++ to a low sample rate (e.g., 4000 Hz) to save CPU.
- Perform cross-correlation on a sliding 500ms window during transitions.
- Extract the phase offset corresponding to the highest correlation coefficient.
- Adjust the PLL's input phase error dynamically with this offset.

**Testing Requirements:**
- Create `tests/unit/test_beat_sync_pll.gd` test methods to mock drift and verify cross-correlation offset adjustments.

**Acceptance Criteria:**
- Given two decks playing out-of-sync beats due to a shifted beatgrid
- When cross-correlation runs during the transition
- Then the system automatically calculates the correct offset and locks the beats in phase.

---

### MET-005 : Low-Overhead Dynamic Streaming Audio Decoder (completed)
**User Story:**
As a listener,
I want the player to have a small memory footprint and stream tracks from disk,
So that the application never runs out of memory or crashes during long listening sessions.

**Context:** Currently, the C++ GDExtension loads the entire uncompressed track audio buffer into memory (100MB+ per track), which can cause out-of-memory crashes on mobile devices.

**Description:** Update `AudioDSP` to use a dynamic decoding and streaming buffer that reads blocks of frames from disk on demand, keeping only a small sliding cache window (e.g., 5 seconds of audio) in RAM.

**Requirements:**
- Modify `AudioDSP::load_file` to keep the file handle or a light decoder instance open, rather than decoding the whole file into a `std::vector<float>`.
- Implement dynamic file reading and decoding in `get_next_chunk()` as the playhead advances.
- Maintain a small cache of PCM frames to support smooth Rubber Band time-stretching.

**Testing Requirements:**
- Add tests in `tests/unit/test_audio_dsp.gd` verifying low memory consumption during streaming playback.

**Acceptance Criteria:**
- Given a very long audio track (e.g., a 60-minute mix)
- When loaded and played
- Then the RAM allocated for decoded samples remains constant and low (e.g., under 15MB).

---

### DJ-015 : Phrase-Adaptive Dynamic Transition Durations (completed)
**User Story:**
As a listener,
I'd like transitions to blend naturally based on the musical phrasing and intro/outro lengths of the tracks,
So that the blend doesn't feel abruptly cut off or rushed on tracks with long structural intros/outros.

**Context:** The current system uses hardcoded transition durations (e.g., 6.0s for Bass Swap, 4.0s for Filter Sweep) regardless of track structure. A track with a 30-second ambient outro should have a longer, smoother blend than a track with a quick 4-second outro.

**Description:** Implement dynamic calculation of transition durations in `AudioManager.get_transition_duration()` based on the actual duration of the outgoing track's outro segment and the incoming track's intro segment, caching and utilizing this dynamic duration throughout the transition execution.

**Requirements:**
- Extract `outro` segment duration from the outgoing track's cached metadata (`outro[1] - outro[0]`).
- Extract `intro` segment duration from the incoming track's cached metadata (`intro[1] - intro[0]`).
- Set the dynamic duration as the minimum of the two segment lengths, clamped between a minimum of 3.0s and a maximum of 16.0s.
- If segment data is missing or incomplete, fallback to the existing hardcoded transition type defaults.
- Bind the calculated duration to `_active_transition_duration` and use it to drive all transition tweens and progress calculations.

**Testing Requirements:**
- Add tests in `tests/unit/test_dj_transitions.gd` verifying dynamic duration assignment for tracks with varied intro/outro sizes.

**Acceptance Criteria:**
- Given a track A with a 12-second outro and track B with a 16-second intro
- When a Bass Swap transition begins between them
- Then the transition duration is dynamically set to 12.0 seconds.

---

### DJ-016 : Pre-Fader Gain Matching via EBU R128 Loudness (completed)
**User Story:**
As a listener,
I'd like the volume of all tracks to remain perceptually consistent when a new track plays or during a transition,
So that I don't experience sudden volume jumps or drops between tracks.

**Context:** Although tracks are loudness-normalized during analysis, a track's perceived loudness can vary significantly across sections. Standardizing playback levels via pre-fader gain matching using the EBU R128 LUFS value computed during analysis ensures consistent volume delivery.

**Description:** Use the EBU R128 LUFS value stored in track metadata to calculate a pre-fader volume adjustment. Apply this adjustment to the active deck's playback channel so that it matches a target loudness of -14 LUFS.

**Requirements:**
- In `AudioManager._load_and_stream_remote_file` or when preparing a player, retrieve the cached EBU R128 loudness value (`lufs`) of the track.
- If the track has no cached LUFS value, default to a fallback gain adjustment of 1.0 (0 dB).
- Calculate the target gain adjustment using the formula: Gain dB = -14.0 - Track LUFS.
- Clamp the gain adjustment between -12 dB and +6 dB to prevent extreme amplification or silencing.
- Apply this base gain offset to the deck's pre-fade volume channel so that fader/crossfader level changes operate on normalized loudness levels.

**Testing Requirements:**
- Add tests in `tests/unit/test_audio_normalization.gd` verifying pre-fade volume attenuation/amplification calculations based on varied track LUFS inputs.

**Acceptance Criteria:**
- Given a track with a cached loudness of -10 LUFS
- When it is loaded on Deck A for playback
- Then Deck A's pre-fade volume level is automatically attenuated by -4 dB to target -14 LUFS.

---

### MET-006 : Dual-Track Look-Ahead WebDAV Pre-Caching (completed)
**User Story:**
As a listener,
I'd like the next two planned tracks in my playlist to be cached on local disk in advance,
So that my music never pauses or buffers due to network latency, even if the plan shifts.

**Context:** The current caching system only pre-caches the single next sequential track. If a user manually overrides the next track, or if the A* pathfinder changes the next song, the player may attempt to stream a track live over WebDAV, causing buffer starvation and glitches.

**Description:** Enhance `AudioAnalyzer`'s prefetching and caching logic to maintain a sliding cache window of the next two tracks planned by the pathfinder or playlist sequence.

**Requirements:**
- Extend `AudioAnalyzer.start_prefetching()` to accept a list of planned track hrefs (up to 3 tracks: now playing + next 2 tracks).
- Prioritize downloading and analyzing these next 2 tracks on background threads.
- Automatically trigger pre-fetching of the next track in the queue as soon as a transition completes and the active track index advances.
- Cancel active downloads of tracks that are no longer in the immediate look-ahead window if the playlist is reordered or overridden.

**Testing Requirements:**
- Add tests in `tests/unit/test_audio_analyzer.gd` verifying look-ahead queue updates and prefetch cancellations.

**Acceptance Criteria:**
- Given a playlist plan: [Track A (Playing), Track B (Next), Track C (Follow-up)]
- When the playback of Track A starts
- Then the system automatically triggers background download/caching for both Track B and Track C.

---

### DJ-017 : Quantized Manual Playback Crossover Alignment (completed)
**User Story:**
As a listener,
I'd like manual track transitions (such as pressing next or double-clicking a track) to be quantized to the beat grid of the active track,
So that manual mix triggers do not create temporary phase clashes or beat overlap trainwrecks.

**Context:** Currently, manual track triggers (pressing next, double-clicking, etc.) bypass the transition timeline wait loop if `force_immediate` is true, immediately calling `_run_deck_transition` and playing the incoming track at `cue_in`. Because the player is started instantly on a random sub-beat, a phase clash occurs until the PLL adjusts.

**Description:** Modify `start_transition` in `audio_manager.gd`. If a manual transition is triggered with `force_immediate = true`, do not start immediately. Instead, calculate the sample offset/time remaining until the next downbeat or bar boundary (Beat 1 of the next bar) of the active playing track. Schedule `_run_deck_transition` to be called exactly at that next downbeat.

**Requirements:**
- In `start_transition(force_immediate: bool = false)`, if `force_immediate` is true, calculate the current playback position.
- Determine the next closest downbeat timestamp in the outgoing track's `downbeats` array.
- Set a timer or check in `_process` until the playhead crosses that downbeat.
- Call `_run_deck_transition` exactly on that quantized boundary.
- Ensure the incoming track's playback is triggered at the aligned beat phase of the incoming grid.

**Testing Requirements:**
- Create a test `test_quantized_manual_transition` in `tests/unit/test_dj_transitions.gd`. Verify that triggering a manual next action schedules the transition to start on the next downbeat rather than instantly.

**Acceptance Criteria:**
- Given the player is playing Track A at 120 BPM
- When the user presses "Next" at Bar 12, Beat 2.5
- Then the incoming track does not play instantly
- And the transition is scheduled to execute exactly at Bar 13, Beat 1.0 (the next downbeat).

---

### UI-003 : Real-Time Phase Lock Visual Sync Meter (resolved)
**User Story:**
As a listener,
I'd like a clear visual representation of the beat phase lock state during a transition,
So that I can visually verify that the two tracks are in perfect phase lock.

**Context:** `AudioManager._apply_pll_sync()` calculates `current_pll_phase_error_ms` at 100ms intervals, but this accuracy is hidden from the UI.

**Description:** Add a Phase Lock Sync Meter to the Vibe card in `vibe_screen.gd`. It will consist of a horizontal bar with a center marker. During a transition, an indicator will move left or right based on the phase error (in milliseconds). If the phase error is within $\pm5\text{ms}$, it centers and glows green.

**Requirements:**
- Add a progress/slider-like UI control `PhaseSyncMeter` inside `scenes/screens/vibe/vibe_screen.tscn`.
- In `vibe_screen.gd`, connect to the process loop or poll `AudioManager.current_pll_phase_error_ms`.
- Map the error range ($\pm50\text{ms}$) onto the sync meter's range.
- Apply styling: center range ($\pm5\text{ms}$) draws with `current_theme.SEMANTIC_SUCCESS` (green), wider range draws with `current_theme.SEMANTIC_WARNING` (amber) or `current_theme.SEMANTIC_ERROR` (red).

**Testing Requirements:**
- Add unit tests in `tests/unit/test_window_playback_ui.gd` to verify that `PhaseSyncMeter` exists and correctly maps positive/negative phase error inputs to the UI range.

**Acceptance Criteria:**
- Given a transition is running between Track A and Track B with a phase error of 2ms
- When the vibe screen processes the frame
- Then the Phase Sync Meter displays a centered indicator glowing green.

---

### DJ-018 : Self-Healing Segment Boundaries via Spectral Centroid (completed)
**User Story:**
As a listener,
I want the player to automatically detect the exact musical intro and outro boundaries of a track even if it has an ambient pad or silent tail,
So that transitions occur exactly where the rhythm starts and ends without manual adjustments.

**Context:** Currently, C++ `AudioDSP::analyze_file` calculates intro and outro boundaries strictly using an RMS amplitude threshold. Ambient hums, fading synth pads, or vinyl noise can exceed the threshold, leading to incorrect transitions.

**Description:** Refine the C++ analysis method `AudioDSP::analyze_file` under `src/audio_dsp.cpp` to incorporate **Spectral Centroid** and high-frequency energy checks. This will distinguish rhythmic sections (drums, transients) from non-rhythmic ambient sections, placing segment boundaries at the true start/end of the musical rhythm.

**Requirements:**
- Implement a simple Spectral Centroid calculator inside `AudioDSP::analyze_file`.
- Calculate high-frequency content (HFC) for onset/transient checks.
- Define `intro_end` as the first frame where both the RMS exceeds the threshold AND the spectral centroid/transient density indicates a rhythmic drum beat.
- Define `outro_start` as the frame where rhythmic transient density drops, ignoring fading ambient synth tails or hiss.

**Testing Requirements:**
- Add a unit test in `tests/unit/test_audio_dsp.gd` verifying segment boundary accuracy for tracks with quiet, noisy, or ambient intros.

**Acceptance Criteria:**
- Given an audio file with a 15-second quiet ambient synth pad followed by a heavy kick drum intro
- When the file analysis is run
- Then `intro_end` is detected at approximately 15.0 seconds (where the rhythmic transients begin), rather than 0.0 seconds.

---

### UI-004 : Audacity-Style Waveform Downsampler & Visualizer (completed)
**User Story:**
As a listener,
I'd like to see a detailed, full-track horizontal waveform (similar to Audacity) for the now-playing track,
So that I can visually inspect the track structure, transients, and playback position at a glance.

**Context:** The `vibe_screen.gd` and `mini_player.gd` display basic progress indicators but lack detailed waveform displays. Because the C++ GDExtension `AudioDSP` loads the entire track's PCM samples into memory (`m_samples`), we can generate high-fidelity waveform images/vectors without reading from disk.

**Description:** Implement a C++ downsampling function in `AudioDSP` that processes `m_samples` to generate a compressed array of max peak values (e.g., 500 or 1000 points representing the track). Expose this array to GDScript, and build a custom Control node `WaveformVisualizer` that draws these peaks as a beautiful, frosted vector waveform.

**Requirements:**
- Create `AudioDSP::get_waveform_peaks(int num_bins)` returning a `PackedFloat32Array` of averaged peak amplitudes.
- Build `WaveformVisualizer.gd` extending `Control` in `scripts/ui`.
- Implement vector drawing inside `WaveformVisualizer._draw()`, rendering vertical lines for peaks with a glass tint and highlight bar for the playhead.
- Integrate this visualizer into the deck details section of `vibe_screen.tscn`.

**Testing Requirements:**
- Add a test `test_get_waveform_peaks` in `tests/unit/test_audio_dsp.gd` verifying that downsampled array size matches the requested bin count and has values between 0.0 and 1.0.

**Acceptance Criteria:**
- Given a track is loaded in `AudioDSP`
- When `get_waveform_peaks(1000)` is called
- Then the system returns exactly 1000 peak points, and the UI control draws them as a balanced double-sided waveform.

---

### DJ-019 : Automated Phrase-Adaptive Mix Length & Selection (completed)
**User Story:**
As a listener,
I'd like the system to automatically determine the most musical transition type and duration for my tracks without manual selection,
So that I can enjoy a hands-off, professionally mixed listening session.

**Context:** The app currently features crossover and duration sliders. In a premium Apple-style player, these manual settings should be removed; the AI DJ should automatically match transition properties to the track structure.

**Description:** Remove manual transition duration and style controls from `vibe_screen.gd` and `settings_screen.gd`. Update the `AudioManager.get_transition_duration()` and selection logic to automatically match the phrasing of the track segments, clamping the crossover time between 4.0s and 16.0s depending on the outro/intro overlap.

**Requirements:**
- Remove `CrossoverSlider`, `CrossoverValue`, and related elements from `vibe_screen.tscn` and `settings_screen.tscn`.
- Update `AudioManager.get_transition_duration` to calculate the overlap of the actual segments: `transition_duration = min(outro_duration, intro_duration)`.
- Quantize the transition length to standard musical phrase boundaries (e.g., 4 bars, 8 bars, 16 bars) calculated based on track BPM, rather than arbitrary fractions of seconds.

**Testing Requirements:**
- Add tests in `tests/unit/test_dj_transitions.gd` to verify that transition length matches phrase intervals (e.g., 8 bars) for different BPM tracks.

**Acceptance Criteria:**
- Given a track playing at 120 BPM (1 bar = 2.0 seconds) with an outro segment of 18 seconds
- When the transition is prepared
- Then the transition length is automatically set to exactly 16.0 seconds (8 bars) rather than a raw, non-phrase aligned value.

---

### MET-007 : Dynamic Prefetch Queue Cancellation & Prioritization (completed)
**User Story:**
As a listener,
I'd like the player to immediately cancel pending cloud downloads if I skip tracks or reorder the playlist,
So that network bandwidth is immediately freed up to fetch the new upcoming tracks.

**Context:** If a user double-clicks several songs down the playlist, `AudioAnalyzer` may keep downloading files in the background that are no longer scheduled to play, causing latency and playback stuttering for the new selection.

**Description:** Refine the prefetch engine in `audio_analyzer.gd` to synchronize the prefetch queue with the active playback queue. Cancel running HTTP requests and delete temporary download files (`.analyzer.tmp`) for any tracks that exit the next-two-track look-ahead window.

**Requirements:**
- In `AudioAnalyzer.start_prefetching()`, check if `current_download_href` is still present in the updated look-ahead window.
- If not, cancel the active `download_http_request`, delete the corresponding `.tmp` download file, and reset `current_download_href`.
- Immediately queue the new upcoming track for download/analysis.

**Testing Requirements:**
- Add unit tests in `tests/unit/test_audio_analyzer.gd` simulating track skips and verifying that active downloads are canceled and replaced.

**Acceptance Criteria:**
- Given the prefetcher is downloading Track B
- When the user manually skips to Track F
- Then the download of Track B is immediately aborted, its temporary file is deleted, and the download of Track G (new follow-up) begins.

---

### SYNC-006 : WebDAV Metadata Sync & Remote Cache Merging (done)
**User Story:**
As a listener,
I'd like my playlists, headphone EQ selections, and analyzed track cues to sync between my devices automatically,
So that I can switch from listening on my desktop to my mobile device seamlessly.

**Context:** Playlists and track configurations are currently cached in the local user directory (`metadata_cache.json`, `playlists/`), which isolated devices cannot share.

**Description:** Implement a background synchronization task in `webdav_service.gd`. Periodically, write local metadata and playlist changes to a file named `vapor_metadata.json` on the remote WebDAV server. When the app scans the WebDAV directory, it will pull this file and merge it with the local database.

**Requirements:**
- Implement `WebDAVService.upload_metadata_cache(data: Dictionary)` using raw HTTP PUT requests.
- Save unified metadata, playlist names, and configuration choices to `user://remote_sync.json` and upload to WebDAV.
- On library scan, fetch `vapor_metadata.json` from the WebDAV server and merge it into `MetadataService` and `PlaylistService`.

**Testing Requirements:**
- Add tests in `tests/unit/test_webdav_service.gd` verifying cache merging and payload formatting.

**Acceptance Criteria:**
- Given a playlist created on the desktop client
- When the mobile client scans the WebDAV folder
- Then the playlist is downloaded, parsed, and rendered in the mobile UI.

---

### UI-005 : Transition Timeline Waveform Visualization (completed)
**User Story:**
As a DJ,
I'd like to see the waveforms of both the outgoing and incoming tracks inside the transition widget,
So that I can visually verify where the transition/mix trigger points and crossovers align.

**Context:**
The previous transition card displayed basic title labels and crossfader lines but did not show the physical waveform peaks of the tracks, leaving the DJ blind to structural beats and hits during transition.

**Description:**
Update the `TransitionTimeline` control to support drawing the outgoing and incoming tracks' waveform peaks. Map the horizontal time domain of both boxes to align precisely with the crossover range and playhead, and load the cached next track asynchronously on the idle deck to pre-populate its peaks.

**Requirements:**
- Store and manage peaks (`outgoing_peaks`, `incoming_peaks`) and durations inside `TransitionTimeline`.
- Draw balanced, double-sided vector waveforms using the theme's core colors inside both track boxes.
- Dynamically load the next track in the background onto the idle DSP for peak extraction.
- Coordinate playhead cursor positioning mathematically with the waveform timeline.

**Testing Requirements:**
- Added `test_transition_timeline_waveforms` in `tests/unit/test_window_playback_ui.gd` to check property configurations.

**Acceptance Criteria:**
- Given a cached upcoming track is scheduled in the playlist
- When the Vibe Screen updates
- Then the transition card displays both track waveforms, aligning the outro transition trigger and intro cue-in points with the central crossover zone.

---

### UI-006 : Vibe Workbench Visual Simplification & Refactoring (completed)
**User Story:**
As a listener,
I'd like a simplified, visually clean AI DJ Vibe Workbench screen that removes flavor-text panels, the Camelot wheel, manual sliders, and phase meters, and instead shows the full outgoing track's waveform and upcoming suggestion cards laid out horizontally,
So that I can relax, see transition details clearly, and enjoy a high-quality listening experience.

**Context:** The old vibe workbench had several redundant elements (such as the now playing card, Camelot wheel, phase sync meter, and manual vibe limit slider) that cluttered the workspace and distracted from the core listening interface.

**Description:**
Remove redundant columns and meters from the layout of `vibe_screen.tscn`. Convert the runner-up suggestion cards into horizontal layouts showing album art, key, BPM offset, compatibility badge, and transition type. Draw the full outgoing track duration in `TransitionTimeline` along with a blue horizontal pin/dot on the sidebar progress track at the automated transition trigger point.

**Requirements:**
- Remove Now Playing card, Camelot wheel, Phase Sync Meter, and Vibe Limit tuner slider.
- Render runner-ups as horizontal cards inside an `HBoxContainer`.
- Map the horizontal space of `TransitionTimeline` to the entire outgoing track's duration.
- Add a transition progress pin/dot to the vertical progress bar at the transition trigger point.
- Scale energy distance threshold dynamically based on the match cycle step (Match: 0.15, Fresh: 0.35, Switch: 0.60).

**Testing Requirements:**
- Remove outdated phase sync meter tests from `test_window_playback_ui.gd`.
- Update `test_transition_timeline_waveforms` in `test_window_playback_ui.gd` to point to the new full-width transition card layout.

**Acceptance Criteria:**
- Given the vibe screen is loaded
- When suggestions are populated
- Then they are displayed as clean horizontal cards.
- Given Smart Mixing is active
- When viewing the vertical progress bar
- Then a blue pin is drawn at the transition trigger point.

---

### UI-007 : Subtle Smoothed Vertical Seeker Waveform (completed)
**User Story:**
As a listener,
I'd like to see a subtle, low-opacity, smoothed waveform curve along the vertical progress seeker in the sidebar separator line,
So that I can visually preview the current track's energy layout in an unobtrusive and aesthetically pleasing way.

**Context:** The vertical progress bar displays the playhead and transition pin but does not show track waveform structure, leaving the user with a blank progress line.

**Description:**
Update the vertical progress bar (`vertical_progress.gd`) to dynamically query peak values from `AudioDSP` once a track is loaded. Apply a moving average low-pass filter to smooth the peak values into nice curves. Render them double-sided as a subtle background wash (22% fill, 30% outline) extending up to 20 pixels on each side, preserving macro peak and valley structures.

**Requirements:**
- Reset peaks and caching when tracks change or start loading.
- Fetch 150 peak points from the active deck's DSP using `get_waveform_peaks`.
- Apply a 5-sample moving average filter to smooth peaks.
- Draw a double-sided polygon at 22% opacity with `theme.AQUA_CORE` (extending up to 20px).
- Draw outlines at 30% opacity using `draw_polyline`.

**Testing Requirements:**
- Added `test_vertical_progress_waveform_properties` in `tests/unit/test_window_playback_ui.gd` to check properties and smoothing behavior.

**Acceptance Criteria:**
- Given a track is loaded and playing/paused
- When viewing the vertical progress bar in the sidebar separator
- Then a subtle, double-sided, smoothed blue waveform is drawn as the background of the progress bar.

---

### UI-008 : Refactor Procedural UI to Scene Files (completed)
**User Story:**
As a developer and maintainer,
I'd like to use Godot's native `.tscn` scene files instead of procedurally injecting UI nodes through GDScript,
So that UI layouts are easier to design visually, maintain, and constrain responsively.

**Context:** The codebase previously generated several UI sub-components (such as track cards, playlist rows, library rows, and the disabled overlay) dynamically via script. This made layout adjustments difficult and prone to sizing/bleeding errors.

**Description:**
Extract procedurally generated UI components into dedicated scene files (`.tscn`) and matching GDScript controllers. Set up proper container controls to support responsive layouts, text truncation (clip/ellipsis), and constraints to prevent elements from bleeding out.

**Requirements:**
- Created `vibe_card.tscn` and `vibe_card.gd` for Vibe DJ screen track suggestion cards.
- Integrated the `DisabledOverlay` node statically in `vibe_screen.tscn`.
- Created `playlist_track_row.tscn` for the playlist screen track list.
- Created `library_row.tscn` and `library_row.gd` for library screen list trees.
- Extracted dynamic drag button behavior to `track_drag_button.gd`.
- Conformed to workspace agent rules regarding `.tscn` preference.

**Testing Requirements:**
- Ran the suite of GUT unit tests to verify no regressions in screens or navigation functionality.

**Acceptance Criteria:**
- Given a list of suggestion cards or track rows
- When instantiating the components
- Then they are loaded from `.tscn` files and configured dynamically without programmatic node construction.
- When resizing the window to narrow/mobile boundaries
- Then the layouts resize responsively without layout bleeding.

---

### PERF-001 : Transition Performance Optimization & Diagnostics (completed)
**User Story:**
As a user,
I'd like track transitions to be smooth and the app to be highly efficient in CPU/GPU usage when switching apps or sitting idle,
So that I don't experience application window freezes or lags during playback.

**Context:** The C++ `AudioDSP` does heavy decoding in a background thread, but redundant load calls on transitions stopped and joined this thread on the main loop, causing severe GUI hangs. In addition, the engine constantly rendered at maximum refresh rate when idle.

**Description:**
Add a loaded files registry in `AudioManager` to avoid duplicate audio loading. Enable low processor usage mode programmatically for production builds. Add a diagnostics panel in settings showing real-time FPS, RAM usage, and audio latency.

**Requirements:**
- Implement `load_dsp_file` in `AudioManager` to track loaded paths and prevent duplicate loading.
- Enable `low_processor_usage_mode` programmatically in `main.gd` for non-headless production runs.
- Add `PerformanceSection` container and diagnostic metric labels to `settings_screen.tscn`.
- Style and update diagnostics labels in `settings_screen.gd` using the `Performance` singleton.

**Acceptance Criteria:**
- Given a song transition in Vibe DJ mode
- When the transition triggers
- Then the playback switches smoothly without main thread freezes.
- Given the Settings screen is open
- Then the System Diagnostics panel shows updating FPS, RAM, and audio latency metrics.

---

### UI-009 : Mobile Shuffle Checkbox Alignment (done)
**User Story:**
As a mobile user,
I'd like the Smart Mixing checkbox in the bottom mini player to be centered within its touch target in portrait view,
So that the navigation controls look balanced and aligned.

**Context:**
In mobile (portrait/narrow) view, the `ShuffleBtn` has its text cleared (`""`). By default, Godot's CheckBox node aligns the checkbox icon to the left, which made the control look off-center.

**Description:**
Add `alignment = 1` (horizontal alignment center) to the `ShuffleBtn` node in `mini_player.tscn` so that the checkbox icon centers itself when there is no text.

**Acceptance Criteria:**
- Given mobile portrait view
- When the bottom mini player is visible
- Then the Smart Mixing checkbox icon is centered within its button layout area.

---

### BUG-001 : Multi-Channel Audio Loader Exception Fix (completed)
**User Story:**
As a listener,
I want the player and analyzer to successfully load and analyze multi-channel audio files (such as multi-channel WAVs, FLACs, or Dolby Atmos .m4a files) without throwing exceptions,
So that I can import and play my entire music library regardless of channel configuration.

**Context:**
The GDExtension `AudioDSP` C++ layer uses Essentia's `MonoLoader` for analysis and caching. Essentia's `AudioLoader` (which is wrapped by `MonoLoader`) throws an exception `AudioLoader: could not load audio. Audio file has more than 2 channels` when loading files with more than 2 channels.

**Description:**
Add automatic multi-channel detection and downmixing support for all audio formats (such as WAV and Dolby Atmos M4A files) in the GDExtension `AudioDSP` C++ layer. If an audio file has more than 2 channels, downmix it to a temporary mono 16-bit PCM WAV file in the system `/tmp/` directory before loading/analyzing it (using `/tmp/` prevents the application scanner's cache pruning process from deleting the temporary downmixed file prematurely), and clean up the temporary file when finished.

**Requirements:**
- Implement `get_wav_channels()` to detect the channel count from WAV headers.
- Implement `get_channels_via_ffprobe()` using `ffprobe` to detect channel count for compressed/non-WAV formats.
- Implement `get_audio_channels()` to query channel counts utilizing the fast WAV detector or ffprobe.
- Implement `downmix_wav_to_mono()` to decode and average WAV channels.
- Implement `downmix_via_ffmpeg()` as a fallback using `ffmpeg` to downmix any audio format.
- Integrate downmixing checks in `analyze_file` and `load_file`.
- Clean up temporary files inside `clear_stream()` and `analyze_file`.

**Testing Requirements:**
- Added a unit test `test_multichannel_downmixing` in `tests/unit/test_audio_dsp.gd` that generates a 4-channel WAV file and asserts that analysis and streaming load succeed.
- Verified that background library scans successfully analyze Dolby Atmos `.m4a` files without throwing exceptions.

**Acceptance Criteria:**
- Given a WAV or M4A file with > 2 channels (e.g. 6-channel Dolby Atmos)
- When it is analyzed or loaded
- Then the C++ layer automatically downmixes it to mono, and the analysis/playback succeeds without throwing an exception.

---

### PERF-002 : UI Responsiveness and Native Chrome Optimizations (completed)
**User Story:**
As a user,
I want the UI to feel extremely snappy and fluid when dragging window panes, resizing, scrolling lists, and playing music,
So that the overall interaction feels premium and native.

**Context:**
The previous implementation locked `OS.low_processor_usage_mode = true` with a `6900` µs sleep time, causing scrolling and visual updates to lag. Additionally, window dragging and resizing were done manually via GDScript relative mouse tracking, leading to cursor separation and frame pacing stutter. Real-time visualizers also allocated `StyleBox` resources on every frame in their draw loops.

**Description:**
Optimize UI performance by dynamically toggling low processor mode, offloading window dragging and resizing to OS-native window manager APIs, and caching StyleBox objects in drawing hot paths.

**Requirements:**
- Implement focus-based `low_processor_usage_mode` toggling in `scripts/main.gd` using standard Godot `_notification()` handlers to prevent startup viewport query failures.
- Delegate custom window resize handles in `scripts/main.gd` to `get_window().start_resize()`.
- Refactor custom window dragging in `sidebar.gd` and `mini_player.gd` to use `get_window().start_drag()`.
- Cache `StyleBoxFlat` allocations in the `_draw()` loops of `waveform_visualizer.gd`, `phase_sync_meter.gd`, and `transition_timeline.gd`.
- Remove dynamic window `min_size` setting inside the layout reflow functions (`_apply_mobile_layout` / `_apply_desktop_layout`) to eliminate layout cycles and feedback loops during resizing.

**Acceptance Criteria:**
- Given the window is focused
- Then `OS.low_processor_usage_mode` is disabled, allowing smooth 60/120 FPS rendering.
- Given the window is unfocused
- Then `OS.low_processor_usage_mode` is enabled to reduce idle CPU/GPU consumption.
- Given custom title bars or resize borders are dragged
- Then the window moves and resizes natively without cursor lag.
- Given any window resizing operation
- Then the layout updates smoothly without freezing or rendering duplicate sidebar/mini-player elements.

---

### MIG-050 : Restore Match / Fresh / Switch and the AI Choice cycle (done)
**User Story:**
As a listener,
I'd like the Vibe screen to show me the three ways out of the track that is playing and which one the DJ would take,
So that I can see the choice being made and overrule it.

**Context:** `docs/FINDINGS.md` found that the React rewrite kept the planner (`generate_mood_path`) and dropped the chooser. The Daylight design specifies the three cards explicitly, tagged and colour-coded, and AI-007 specifies the behaviour. The engine inputs were all ported already — `harmonic_relation_cost`, `is_similar_genre`, `choose_transition`.

**Description:**
Port `_get_match_type_between` and the four-step cycle. Render one candidate per kind on the Vibe screen with the transition each would use, badge the DJ's choice, and let a person take another.

**Requirements:**
- Thresholds as the original: different genre → Switch; else ≥8 BPM or ≥0.2 energy → Fresh; else Match.
- One candidate per kind, each scored on what its kind is for rather than on one shared distance.
- Four-step cycle: Match, Fresh, Match, then a change. **Alternating rather than the original's 50/50 coin**, so a set can be reproduced and the screen can say in advance what it will do; the proportions over a full period are unchanged.
- `selected` separate from `aiChoice`, so an override moves the highlight and leaves the badge (AI-007 §4).
- Choosing by hand re-searches the tail along the same curve, so the arc is preserved.
- Cards transcribed from the design's `alternates`, tag colours included.

**Acceptance Criteria:**
- Given Smart Mixing is on and the library has one track of each kind
- When the Vibe screen loads
- Then exactly three cards render, one marked as the AI's choice
- When another is pressed
- Then it becomes the next track, the badge stays where it was, and the cycle advances.

---

### MIG-051 : Playlist folders (done)
**User Story:**
As a listener with a lot of playlists,
I'd like to group them into folders,
So that the sidebar is navigable.

**Context:** `playlist_folder_service.gd` was ported to `vapor-library` at migration time — `FolderStore`, and `folder_id` on a playlist, both with tests — and the shell exposed neither, so `folderId` reached the frontend as a field nothing could set.

**Requirements:**
- Commands for listing, creating, renaming and deleting folders, and for filing a playlist into one.
- One level rendered, as the original rendered; `parent_id` stays representable so nesting needs no later migration.
- **Deleting a folder does not delete the playlists in it** — they return to the top level, including a nested folder's, and the confirmation says so.
- Filing is a drag, with a "Not in a folder" target so a playlist filed once is not filed forever.

**Acceptance Criteria:**
- Given a folder holding a playlist
- When the folder is deleted
- Then the playlist is still there, at the top level.

---

### MIG-052 : Lyrics and artwork, with consent (done)
**User Story:**
As a listener,
I'd like to see the words to a track,
So that I can read along — and I'd like to know when the app is talking to someone else to get them.

**Context:** `metadata_service.gd`'s network half was never ported: LRCLIB for lyrics, Deezer for an artist portrait, album art and a genre. The original fetched unconditionally on every track and said nothing about it, which sits badly against an app whose whole claim is that it works everything out on the device.

**Requirements:**
- Port `fetch_lyrics`, `parse_lrc`, `fetch_artist_image`, `fetch_album_art` and the genre lookup.
- **Off by default**, per track rather than per library; off also forgets what was found, including the downloaded images.
- Parsing separate from the transport, so every response shape is tested from a canned string with no network.
- Images fetched by Rust into a file named by URL — one sleeve per album, not per track — and served as a `data:` URI, since the window's CSP allows `data:` and no remote host.
- Looked-up material drawn in its own panel that names where it came from; a looked-up sleeve is marked as such where it stands in for a file's own.
- Fix `parse_lrc`'s two defects: the fraction divided by 100 regardless of its precision, and only the first of several timestamps on a line.

**Acceptance Criteria:**
- Given lookups are off
- When Liner Notes opens
- Then no request is made and the screen says what one would send.
- Given lookups are on and the words exist
- When one is asked for
- Then the lines appear with their timings, credited to LRCLIB.

---

### MIG-053 : The Vibe Limit (Mix Tuner) (done)
**User Story:**
As a listener,
I'd like to say how far the DJ may jump in energy between two tracks,
So that a set can hold one intensity or swing between them as I choose.

**Context:** §6 of `ai_dj_workflow.md`. `transition_cost` has taken the energy threshold as a parameter since the port and all three callers passed `DEFAULT_ENERGY_THRESHOLD` — the control the engine was built for was the missing piece.

**Requirements:**
- A slider on the Vibe screen, under the curves it governs, from strict to loose.
- Band 0.1–1.0. Below 0.1 the penalty applies to nearly every pair and stops discriminating; 1.0 means no pair is ever over the limit.
- `sanitised` handles the non-finite case *before* `clamp`, which passes NaN through — the value comes off disk.
- Written on release, not on change.

**Acceptance Criteria:**
- Given the slider is moved and released
- Then the limit is saved, and the next set the DJ conducts uses it.

---

## Where the SYNC family landed (2026-08-16)

All six built in `vapor-app`, not in the Godot tree. Every decision lives in
`vapor-core/crates/vapor-library/src/sync.rs` and every socket in
`vapor-app/src-tauri/src/peers.rs`, so pairing and reconciliation — the two
things impossible to test with one machine — are tested as functions.

Four deliberate departures from the tickets as written, each because the ticket
named a mechanism where the requirement was an outcome:

| Ticket says | Built | Why |
|---|---|---|
| SYNC-003: SHA-256 fingerprints; SYNC-004: **MD5** checksums | SHA-256 throughout | The requirement is integrity. MD5 has been collision-broken since 2004, so a substituted file is exactly what it can no longer catch. One hash rather than two is also one fewer thing to get wrong. |
| SYNC-002: "Diffie-Hellman or basic token validation" | A PIN bound to **one** device, three attempts, two-minute window | The PIN is the security property, and it is only a property if guessing is bounded. Binding it to the device it was shown for is what stops a code on screen being an invitation to the whole subnet. Transport encryption is not here — see below. |
| SYNC-001: "mDNS or UDP broadcast" | UDP broadcast | The ticket's own requirements specify a broadcaster and a listener. mDNS would be a dependency for the same result on a subnet. |
| SYNC-004: "resume logic for interrupted transfers" | A partial file's length *is* the offset | No progress record to write, and therefore none that can disagree with the file on disk. |

**What is not there, and is not pretended to be:**

* **The connection is not encrypted.** Pairing authenticates the *device*; the
  bytes then move in clear over the LAN. On a home network with the library
  already sitting in plaintext on a WebDAV server, that is a defensible line —
  but it is a line, and a café network is the case where it is the wrong one.
  TLS with the paired key as a pre-shared secret is the fix.
* **Nothing has been run between two machines.** There is one here. See TD-55.
* **It is off by default.** A beacon every five seconds announces this machine
  to whatever network it happens to be joined to, and this app's position is
  that it does not do that without being asked — the same decision made for
  lyric lookups on the same day. Off means no advert, no listening socket, and
  therefore no firewall prompt. Turning it off also forgets every pairing: a
  device that can no longer be discovered should not still be trusted. What it
  does *not* yet do is stop the threads already running, which needs a relaunch
  — TD-58.

---

## Vibe DJ — open work (2026-08-17)

State after a day of fixes, written down so it survives a fresh session.

### What now works

The DJ conducts. `dj_mode` is a persisted setting the supervisor reads — it was
`useState(true)` in `App.tsx`, so the half of the app that decides what plays
next had never heard of it and nothing ever added to the queue. `extend_set`
appends the DJ's pick when the set has nowhere left to go, `Queue::set_next`
adds a track that is not already queued instead of refusing it, and
`Queue::has_more` answers "does this set have anywhere to go" rather than
`peek_next`, which under repeat-all wraps to the beginning and says yes forever.

Intensity replaced the energy measure for matching and curves. `energy_level` is
mean RMS over peak RMS — a consistency ratio that put ballads above drum & bass
on the real library. `intensity_from_lufs` separates the same groups by 0.256
instead of 0.031 and needs no re-analysis, since LUFS was already stored.

### VDJ-0 : the three exits are Stay, Follow, Switch (built 2026-08-17)

Supersedes the Match/Fresh/Switch cycle. Agreed with Dylan and written down
before building, because it removes machinery rather than adding it and the
reasoning is easy to lose.

The old model classified a candidate by *similarity* — Match, Fresh, Switch —
and picked one per step from a repeating cycle (`cycle_choice`, `mix_step`),
marking it AI CHOICE. That fights the planner: the cycle's pick and the set's
own next track are computed separately and can disagree.

The new model is three *intentions*, and the planner owns the set:

* **Stay** — hold at the current energy. Does not advance the curve.
* **Follow** — the planner's next track along the curve. **Always the default.**
* **Switch** — branch: genuinely different but still mixable, and it re-plans
  the tail toward the same energy goal.

Consequences:

* `cycle_choice` and `mix_step` go. There is no rotation to reason about; the
  default is structural.
* The AI CHOICE badge goes with them. Nothing is being *recommended* — Follow is
  simply what happens if you do nothing.
* Only the curve changes the destination. Stay and Switch change the route.
* Choosing Stay or Switch re-plans the tail, keeping the same energy goal.

Built. `Exit` replaced `MatchKind`; `cycle_choice`, `mix_step` and `ai_choice`
are gone; `mix_candidates_for` fills Follow from `queue.peek_next` and searches
only Stay and Switch. Taking an exit folds it into the set, so on the next
render the track pressed *is* the Follow card — `screens.test.tsx` pins that.

### VDJ-1 : Switch was unreachable (fixed 2026-08-17, thresholds measured)

`match_kind_between` returned Switch **only** when two genres differed, and
`same_genre` treats an unknown genre as similar — so with 46 genre tags across
534 tracks the branch was dead. Drum & bass into Sade is 25 BPM and 0.35 of
intensity apart, the 90th percentile of this library, and was called a Match.

Now decided by distance, with thresholds taken from the library's own
distribution over 4,000 random pairs rather than invented:

| | median | 75th | 90th |
|---|---|---|---|
| Δbpm | 24.8 | 42.9 | 59.4 |
| Δintensity | 0.14 | 0.25 | 0.35 |

Switch at Δintensity ≥ 0.30 or Δbpm ≥ 45; Match below 0.15 and 8. A known
genre difference still forces a Switch — good evidence when present, simply
never present here.

Renamed under VDJ-0, and one more cause found: even with the thresholds fixed,
Stay and Switch were each drawn *only* from the band `exit_between` put a
candidate in, so a library with nothing inside 8 BPM had no Stay card and said
nothing about why. Stay is now asked of every candidate — "how little does the
level move" — and Switch is taken from what Stay left, falling back to the
furthest remaining track when nothing clears the thresholds. Three cards
whenever there are three tracks to fill them, pinned by
`all_three_exits_are_offered_even_when_nothing_sits_in_the_band`.

### VDJ-2 : the planner has never run (fixed 2026-08-17)

Two mechanisms, and only the weaker one is reachable without a button press.
`extend_set` appends **one** track when the queue empties, which is why the
queue reads "2 to come". `pathfinder::plan` does A* with a beam width of 6 over
`transition_cost + curve_cost` and plans `MAX_PLANNED = 10` tracks along the
chosen curve — but only from "Conduct from here".

Fixed. `extend_set` now calls `generate_mood_path` and queues `PLAN_AHEAD = 10`,
falling back to `dj_pick` only when there is nothing to plan from. The playback
thread already called `extend_set` when the queue ran short, so the planner runs
without anyone pressing anything.

The curve is a saved setting (`Settings::curve`) rather than an argument, since
the playback thread has to know where the set is going with no screen open.
`set_curve` saves it, truncates everything after the track playing — that tail
was a route to the old destination — and re-plans. Selecting a curve is
therefore the whole action, and "Conduct from here" is gone.

The auto-advancing curve is *not* built, and should not be: it was specified as
advancing with the match/fresh/match/switch cycle, and that cycle no longer
exists. A curve that changes itself also contradicts VDJ-0, where the curve is
the one thing only the listener changes.

### VDJ-3 : the suggestion cards (fixed 2026-08-17)

Reduced to list rows while removing verbosity. The design is three cards
horizontally, artwork as the main element, the short info line beneath. Restored:
`.vibe__exits` is a grid of art-led cards with the exit word on the sleeve, and
the glass panel that used to sit behind them is gone.

Two layout bugs found by screenshotting it rather than by reading the assertions:
a `1fr` track made each sleeve 430px tall on a wide window, and `text-overflow:
ellipsis` does nothing to the children of a flex container, so the facts line ran
out through the rounded corner. The card cap is on the card, not the grid track —
`auto-fit` counts tracks at the *maximum* of a `minmax()`, so `minmax(120px,
190px)` in a 364px column makes one column, not two.

The curves are colour and shape now, not prose: each swatch draws its own
`Curve::target_energy` and is warm if it climbs, cool if it falls. The arithmetic
that used to sit under each label was accurate and was not what anyone is
choosing between.

### AND-1 : Android (runs on a Pixel 9; audio opens)

The status `docs/ANDROID.md` deliberately does not carry.

**Verified on hardware** — a Pixel 9 over wireless debugging, 2026-08-18:

* The app installs, starts, and renders. Onboarding lays out correctly at
  phone width with no changes made for it.
* An audio device opens: `AAudioStream_requestStart` returns 0 in our process.
* No panic and no `UnsatisfiedLinkError` in logcat on a cold start.

**Verified by building it:** the shell compiles for `aarch64-linux-android` here
and in CI; a debug APK builds; `libc++_shared.so` is bundled by the Gradle
plugin, settling the `oboe-shared-stdcxx` question a `cargo check` could not.

**Found by running it — see AND-4.** Nothing was publishing the Android runtime
handles, so audio was dead on the first launch. Fixed.

**Still unverified:**

* The credential store, end to end. It compiles and is type-checked on four
  platforms; no password has been saved and read back on a device. This is the
  one that most wants a real test — JNI signatures resolve at runtime, not at
  build time.
* Playback itself. A device is open; no audio has been decoded through it, and
  a real-time callback that must not allocate is a different question from a
  stream that starts.
* Peer discovery needs `INTERNET` (present) plus `CHANGE_WIFI_MULTICAST_STATE`
  and a held multicast lock (neither present). Deliberately not added: a
  permission for a feature that cannot work yet is a permission asked for
  nothing. Sync is off by default, so it should fail quietly.

**Decisions in `gen/android`, which is in the tree:** `allowBackup="false"`
(the Keystore key behind the credential store is never backed up, so a restored
backup would carry a record nothing can read); `minSdk = 24` (above the API 23
`KeyGenParameterSpec` needs); and a `.debug` application id suffix, so a test
build installs alongside the real app rather than demanding an uninstall that
would take someone's settings, playlists and password with it.

### AND-3 : `app/build.gradle.kts` is regenerated on every build (worked around)

The Tauri CLI rewrites `gen/android/app/build.gradle.kts` from its template on
every `tauri android build`, so an edit to that file survives exactly until the
next build — and silently, since the build reports success and simply produces
an APK without the change. Found by watching a `.debug` suffix disappear twice.

Comments in the file survive; property lines are replaced. Do not read that as a
merge worth relying on.

The customisation therefore lives in `gen/android/build.gradle.kts`, the root
file, which the CLI leaves alone, as a `subprojects { plugins.withId(...) }`
hook. `AndroidManifest.xml` is not regenerated and is a normal place to edit.

Worth revisiting if Tauri gains a supported way to set an application id suffix,
or if the root file starts being templated too.

### AND-2 : the Windows shell tests crashed the test binary (fixed 2026-08-17)

`vapor_app_lib-*.exe` exited `0xc0000005` — STATUS_ACCESS_VIOLATION — after
printing "running 165 tests" and before any test reported. Every test in the
shell's lib suite was unrun on Windows.

Not a regression from anything that day: the Windows job had never executed
before it. Every earlier run failed in three or four seconds, refused on billing
while the repo was private, and the summary line reads "failure" either way —
which is the whole reason it went unseen.

**Cause.** `AppState::load` called `audio::Player::start()`. Every test that
builds an `AppState` therefore opened a real audio device, and on a runner with
no audio endpoint the first one to try died inside `cpal`'s WASAPI backend. That
is below anything this crate can guard: `open_stream` already returns an error
for a missing device, a bad config and a stream that will not start, and none of
those paths is reached.

**Fix.** `load` reads this app's files and nothing else; `AppState::open_audio`
acquires the device and is called once from `run`. Pinned by
`loading_does_not_open_an_audio_device`, because "does this function touch
hardware" is invisible at every call site.

Worth keeping in view: the same crash is what a Windows laptop with every audio
device disabled would have done on launch. It was never only a CI problem.

**Found by** running the Windows step with `--test-threads=1 --nocapture`. A
parallel run names the binary and not the test; one thread at a time made the
last line printed the culprit.

### AND-4 : nothing published the Android runtime handles (fixed 2026-08-18)

`ndk_context` holds the `JavaVM` and app `Context` in a process-wide static so
that library code deep in a dependency tree can reach the Android runtime.
`cpal` reads it to open a stream through Oboe; `secrets::android` reads it to
reach the Keystore. Neither `tauri` nor `wry` depends on the crate at all — they
carry their own JNI plumbing — so nothing ever called
`initialize_android_context`.

First launch on a device: the app came up perfectly, rendered, and had no sound.
One line in logcat said why — the audio thread panicked with "android context
was not initialized", reported as "the audio thread stopped before reporting".
The credential store would have failed the same way, though it reports the
condition rather than panicking.

Fixed by `src/android.rs`, called from `MainActivity.onCreate` before `super`.
The application context is used rather than the activity, and its global
reference is leaked deliberately: one per process, and dropping it would leave
the static pointing at a collected object.

**It compiled for a day before this.** Four platforms, CI green, a debug APK
that installed. Nothing about the failure is visible from a build — which is the
argument for AND-1 keeping "compiled" and "ran" in separate columns.

### AND-5 : the release APK killed itself on launch (fixed 2026-08-27)

`v2.0.0-rc.3` was the first release APK this project ever built, and it closed
the moment it opened. So did rc.4, rc.5, rc.6 and rc.7. Five tags went to R8 —
keep rules, `isMinifyEnabled` from two different points in the Gradle
lifecycle, and finally `-dontshrink`/`-dontoptimize`/`-dontobfuscate`, which
did work: rc.7's APK is provably unobfuscated, and it crashed exactly the same.

R8 was never involved. From logcat, on the first device this had ever been
attached to:

```
FATAL EXCEPTION: main
android.app.RemoteServiceException$ForegroundServiceDidNotStartInTimeException:
  Context.startForegroundService() did not then call Service.startForeground():
  ServiceRecord{... com.dylangrowcoot.vapormusic/.PlaybackService ...}
    at PlaybackService$Companion.update(SourceFile:363)
    at PlaybackService$Companion.stop(SourceFile:399)
```

`startForegroundService` is a promise that `startForeground` follows within a
few seconds, and Android holds the app to it even if the service stops first.
`PlaybackService.stop()` was `update(context, "", "", false, 0, 0)` — a
delivery that starts the service — and `onStartCommand` answered it with
"nothing left to stay up for", `stopSelf()`, and no `startForeground`. The
framework posts `ForegroundServiceDidNotStartInTimeException` to the main
thread; it is not catchable, and the process dies.

On a cold start the Rust side publishes "nothing playing" before anything has
played, so `stop()` is the *first* thing the service ever hears. The crash was
therefore guaranteed on every launch of a fresh install.

Fixed in `PlaybackService.kt`, twice over: `onStartCommand` now calls
`startForeground` before it stops, and `send()` does not start the service at
all for a delivery that says nothing is happening when it is not already
running.

**Why the debug loop never showed it.** Not confirmed, and worth holding
loosely: the likeliest reading is that the debug installs were launched on a
phone that already had library state, so the first publish carried a title and
took the `publish()` path instead. A fresh install with nothing playing is the
crashing case, and a fresh install is what every release APK was.

**What it cost, and the lesson that is actually transferable.** Five tags and
an evening, spent on a hypothesis nobody could test, because "the first APK
where R8 ran is the first APK that crashed" is a very good correlation and a
completely wrong cause. Every one of those five attempts was reasoned from the
build; none was reasoned from the device. The first logcat took two minutes and
named the class, the method and the line. `docs/ANDROID.md` now says to get one
before touching anything.

### VDJ-4 : the identify pass has never been run (done 2026-08-23 — it does not need running)

`identify_library` asks Deezer for each track's tempo and album genre, and uses
their tempo only to choose which octave of the tempo measured here is the one a
listener counts — Delta Heavy's "Space Time" measures 87 and is 174, and both
this crate and Essentia agree on 87, so nothing on the device can settle it.
Unblocks VDJ-1 and the whole class of drum & bass matching chill hip hop.

Waiting on a button press: it sends artist and title for every track.

**Closed 2026-08-23 by removing the button press from the path**, at Dylan's
direction: the source does not matter as long as the data arrives lazily, per
track, when it is needed.

`look_up_in_background` already ran per track as one loads, fetching words and
a sleeve. It now asks for the facts too when they have never been asked for,
and applies the octave correction with every guard the library-wide pass
applies, in the same order. The switch in Settings goes back to being what it
reads as — *do you want the app to look things up online* — rather than a
command to fetch a library's worth of anything.

Three consequences worth naming. The pass spreads across the time somebody
spends listening rather than landing as one burst of hundreds of requests,
which is also the AUD-18 pressure. It only ever asks about records that get
played, so a library nobody listens to costs nothing. And it needs no
permission beyond the switch, which is why this row could sit blocked for days.

The correction itself is unchanged and now testable without a runtime:
`octave_correction` is the extracted decision, with tests on the guards that
matter — durations must agree that it is the same recording (a remix by the
same name is a different piece of music), the difference must be an octave
rather than a disagreement, Deezer's zero means "unknown" and not a tempo, and
a tempo set by hand is never overwritten.

The re-tracking that follows a correction is queued on `AppState::pending_retrack`
and drained by the playback supervisor, which holds the `AppHandle` that
`retrack_grids` needs and already runs on a timer.

**`identify_library` is untouched** and still there as the bulk option. Nothing
depends on it now.

### VDJ-5 : the hiccup (done 2026-08-20, confirmed 2026-08-23)

A small audible glitch in an otherwise much-improved transition, heard once and
not investigated. Reproduce offline with `bin/render` rather than by ear.
Candidates: the stretcher re-priming as the post-transition glide crosses unity,
a decoder stall at the seam, the limiter.

**Closed on the evidence, with one caveat.** Recorded 2026-08-17 naming three
candidates: the glide crossing unity, a decoder stall at the seam, and the
limiter. `89b0fb5` on 2026-08-20 — "Ramp the limiter, crossfade the glide" —
addressed two of the three, and `FINDINGS.md` records the field confirmation:
same listener, "completely clean", on a beat-matched Bass Swap that drove the
limiter deeper than anything measured before the fix.

The caveat, because closing it silently would misrepresent what was checked: the
third candidate, a decoder stall at the seam, was never separately ruled out, and
confirmation was by ear rather than offline through `bin/render` as this ticket
asked for. If the hiccup returns, that is the unexamined one.

---

## Tech debt

Recovered from the tech-debt document deleted on 2026-08-16 (commit cb3a979)
for stating what currently works. The *findings* in it moved to
`docs/FINDINGS.md`; these are the open items, and they belong here because
this is where status lives. Each says what it is waiting for, so the list
cannot be mistaken for fourteen things nobody has got round to.


### TD-11 : (blocked)

**Key detection is 60.6% exact / 82.8% harmonically compatible**, up from 56.1% / 80.9%. Feeding the chroma from **spectral peaks** rather than from every bin is what did it — a drum hit is broadband and was depositing energy into all twelve pitch classes at once. Still not good enough to be proud of. Segmented analysis is *not* the remaining lever, contrary to what this row used to say: TD-13 shipped it. **Tuning correction was tried and reverted** — it measures 58.1%, worse than doing nothing. See MIGRATION.

**Waiting for:** A reproducible fixture corpus (TD-43). Measurable here against the owner's library; not reproducible by anyone else.

**Where:** MIG-001


### TD-22 : (done 2026-08-16)

**Time-stretch is a placeholder.** WSOLA works and is transparent at the ±2% beat-matching uses, but was written to prove the approach rather than chosen on measured quality against Rubber Band or signalsmith-stretch.

**Closed by:** Signalsmith Stretch, now the default on every target that compiles C++. Rubber Band was rejected on its build system rather than its sound — it is the better-sounding library and reintroducing a C++ dependency tail is what MIG-012 existed to end.

The integration failed its first measurement at 118.4 ms off a 128 BPM grid, and three pre-roll corrections each moved that by exactly zero. `examples/impulse.rs` measured the wrapper directly and settled it: pre-roll cannot fix this, because pre-roll shifts input and output together. `seek` — the library's own pre-roll API — produces bit-identical output to doing nothing at all. The correction is one-sided, `Lₒ + Lᵢ/ratio` frames of output discarded, and the derivation is in `vapor_engine::signalsmith`.

Worst onset deviation across the transition, from `beat_alignment`:

| | Before | After |
|---|---|---|
| Signalsmith | 118.4 ms — failed | **0.18 ms** |
| WSOLA | 5.84 ms | 5.84 ms — unchanged, still what wasm uses |

**Where:** MIG-012


### TD-24 : (blocked)

**cpal is unvalidated on iOS and Android.** The least battle-tested part of the audio stack, and phase 4 is where it would be discovered late. TD-03 exercises it on macOS desktop only; that says nothing about either mobile target.

**Waiting for:** Real iOS and Android devices.

**Where:** MIG-011


### TD-55 : (blocked)

**The sync between two devices has never run between two devices.** SYNC-001 to SYNC-006 are built and tested — 35 tests on the decisions in `vapor_library::sync`, 11 on what the server will and will not answer — and every one of them runs in a single process. Nothing has broadcast to a real subnet, completed a real pairing, or moved a byte over a real socket. The decisions are the part that is hard to get right and they are covered; the part that is covered by nothing is whether two machines actually find each other. **The one bug class this shape cannot catch is a mismatch between the two halves of the wire format**, since both sides are compiled from the same enum.

**Waiting for:** A second machine running Vapor.

**Where:** SYNC-001..006


### TD-56 : (closed 2026-08-20 — decided)

**A sync moves bytes in clear over the LAN.** Pairing authenticates the *device* — a PIN bound to one peer, three attempts, a two-minute window — and after that the transfer is plain TCP. On a home network, with the library already sitting in plaintext on a WebDAV server, that is a defensible line. On a café network it is the wrong one, and nothing in the UI says which network you are on. The fix is TLS with the pairing establishing a pre-shared key; the reason it is not here is that inventing a handshake is how you get a broken one.

**Decided 2026-08-20: the home network is trusted, and plain TCP stays.** Not
an oversight and not a deferral — a deliberate scope line. The library already
sits in plaintext on a WebDAV server, so encrypting one hop between two of the
owner's own machines buys little against the cost of getting a handshake right.

**What that accepts, stated plainly:** on an untrusted network — a café, a
hotel, a shared office — anyone on the same segment can read library metadata
and audio off the wire while a sync runs, and nothing in the UI says which kind
of network you are on. Pairing still authenticates the *device*, so this is
eavesdropping, not impersonation.

**Reopen if** the app is used on networks the owner does not control, or is
given to anyone else. The fix does not change: TLS with the pairing PIN
establishing a pre-shared key, via `rustls`, not a hand-rolled handshake.

**Waiting for:** Nothing. Closed as a decision.

**Where:** SYNC-002


### TD-58 : (done 2026-08-16)

**Switching sync off does not stop the threads that are already running.** The setting gates whether they start, and turning it off stops adverts being *acted on*, forgets every pairing and refuses every command — but the beacon and the listener live until the process exits. So a machine that had sync on and then turned it off keeps broadcasting until it is relaunched, which is exactly the case the switch exists for.

**Closed by:** `peers::Session` — a stop flag both loops check, and the sockets made interruptible so they can reach the check. The UDP listener takes a 250 ms read timeout; the TCP listener goes non-blocking and polls, since a thread blocked in `accept` cannot notice a switch. `stop()` joins rather than detaching, so the ports are free by the time it returns: toggling off and straight back on is ordinary, and a detached stop would race the new session to the same two ports and lose, reported as "discovery is unavailable" on a machine where nothing is wrong.

The test is `peers::stopping_releases_the_ports_so_sync_can_start_again`, and it asserts the thread count rather than a flag — `start` succeeds if either half binds, so counting is what catches one port leaking while the other is fine. Verified to fail against the old detached behaviour before being kept.

Per-connection threads stay detached: they are bounded by `IO_TIMEOUT` and waiting for them would hold the switch for up to twenty seconds. An in-flight request is refused rather than served, because turning sync off clears the trust and `authorise` answers from that.

**Where:** SYNC-001


### TD-57 : (done 2026-08-16)

**A deletion does not travel through the shared document.** `merge_shared` is additive on purpose: nothing is deleted and nothing overwritten, so it cannot lose work and converges in one pass whichever order two devices sync in. The cost is that removing a playlist on the laptop lets the phone put it back.

**Closed by:** `sync::Tombstones` — playlist and folder ids with the time they were deleted, carried in the shared document, persisted locally, and republished every sync rather than only when something was just deleted. A device that has been off for a year still has the playlist and the document is the only place it will ever hear otherwise, so tombstones are kept indefinitely; the cost is an id and a `u64` each.

`SHARED_VERSION` went to 2, and the bump is the point rather than a formality. The field is `#[serde(default)]`, so a version-1 build would read the document, silently drop the tombstones, and write back one in which every deletion had been undone. Refusing to read it is the only safe thing an older build can do.

**The part of this ticket that was not built, and why.** The original note said the fix needed "a modification time on every mutation" so an edit newer than a tombstone could keep a playlist alive. It is not there. A tombstone applies unconditionally, so a deletion beats a concurrent edit it never saw, and those additions are lost — the tracks are untouched in the library, but the playlist is gone. That is asserted in `a_deletion_beats_a_concurrent_edit_and_that_is_deliberate` rather than left as a surprise.

The modification time was weighed and refused: the clock lives in the shell rather than in `vapor-library`, threading it through would touch twenty call sites plus every test, and `PlaylistStore::get_mut` handed out unguarded mutable access, so a mutation that forgets to stamp was a matter of when. Against a deletion that failed to travel every single time, blunt was the better of the two.

**Update, same day: `get_mut` is now private** (nothing outside `playlist.rs` was using it, so this cost nothing), and both stores were checked for other escapes — no public field, no method handing out a `&mut` to a record. A modification time is now stampable in one place with the compiler enforcing it.

**That is no longer the objection that matters, though.** Comparing "edit newer than tombstone" compares a timestamp taken on one device with one taken on another, and two machines whose clocks differ by minutes are ordinary — nothing in the sync path detects or corrects that, and the failure is silent in whichever direction the skew runs. The current rule never *compares* a timestamp; it records one and carries it, which is why a wrong clock cannot fool it. If this is revisited, the shape to reach for is a version vector or a Lamport clock, not a wall-clock time. Left as a decision rather than taken, since it changes sync semantics.

Deleting a folder rehomes its playlists to the top level on arrival, matching what the shell already does locally. A playlist left pointing at a folder id that no longer resolves vanishes from every view that files by folder, which is indistinguishable from losing it.

**Where:** SYNC-006


### TD-51 : (done 2026-08-16 — and it had found a real bug)

**The lookup is unexercised against a real service.** `metadata.rs` is tested exhaustively against canned response bodies and not once against LRCLIB or Deezer, because CI has no network and because turning the switch on sends a real query somewhere. So the parsing is trusted and the *shapes* are assumed: field names, the `data` array, the `genres.data[0].name` path and the size ladders all come from reading `metadata_service.gd`, which was itself written against the 2026 APIs. If a lookup returns nothing on a real machine, suspect the shape before the parser.

**Run against the live services 2026-08-16.** Three of four assumptions held; one did not, and it was silently broken.

| What | Assumed | Actual |
|---|---|---|
| LRCLIB `syncedLyrics` / `plainLyrics` | camelCase at top level | correct — 104 synced lines |
| Timestamp precision `[00:30.75]` | centiseconds | correct |
| Deezer artist `data[].picture_*` ladder | correct | correct — xl present |
| Deezer album `data[].cover_*` ladder | correct | correct — xl present |
| **Deezer genre `genres.data[0].name`** | **on the album search response** | **not there at all** |

`/search/album` carries no `genres` object at any level — only a numeric `genre_id`. So `genre_of` looked, found nothing, and returned an empty string **for every track ever looked up**, which is indistinguishable from an album that genuinely has no genre. The parser was right the whole time and was being handed the wrong document; the comment claiming the search response "already names the genre the original was going back for" was simply false, and the original made two requests because two are needed.

Fixed by taking `data[0].id` from the search hit and asking `/album/{id}`, which does carry `genres.data[0].name` — verified: `/album/302127` → `Electro`. The second request only happens once there is art, so a miss costs nothing extra.

**Scope, stated precisely:** the looked-up genre reaches the Liner Notes screen only. Library rows and the pathfinder read the genre from the file's own tags, so nothing about mixing or track selection changes — this restores a field on one screen that has been blank since the port.

The four response bodies are now in the tests, captured from the real services rather than written from reading the GDScript, which is how the mismatch survived a full suite of passing tests. `the_parsers_still_match_the_live_services` re-checks them against the network on demand and is `#[ignore]`d, so CI never runs it and nobody's listening is sent anywhere without asking:

```
cargo test --lib live_services -- --ignored --nocapture
```

**Where:** MIG-052


### TD-54 : (done 2026-08-17)

**Media controls: two bugs found and fixed on 2026-08-16, one condition that is not a bug.** (a) `publish` was called from the playback supervisor's thread; `MPNowPlayingInfoCenter` and `MPRemoteCommandCenter` want the main thread, and souvlaki does not marshal for you — it dispatches to a *global* queue, and only for artwork. Now hopped via `AppHandle::run_on_main_thread`. (b) `worth_sending` excluded position entirely, so after the title landed nothing was ever sent again and the Control Center scrubber froze where the track started; position now counts at 0.2 Hz. (c) **Not a bug:** macOS routes media keys to the Now Playing *application*, which means a `.app` bundle. `tauri dev` runs the bare binary, so keys will never arrive from `npm run app` however correct the code is — souvlaki's macOS backend returns `Ok` unconditionally, so nothing said so. `media::bundled()` detects it, logs it at startup, and `media_keys_available` reports it to the UI.

**Confirmed working 2026-08-17** by Dylan, in a bundled build, with a keyboard. All three parts of the ticket are now settled: (a) and (b) were the two real bugs and are fixed; (c) was never a bug, and the startup log explaining why `tauri dev` cannot receive media keys is what stopped it being rediscovered.

**Where:** MIG-023


### TD-35 : (blocked)

**The mark's shape is unfinished.** Known and expected; the attribute surface is stable so screens are safe to build against it. A design iteration rather than an engineering one — `design/vapor-mark.js` and the app's copy are byte-identical, so the app is not lagging the design.

**Waiting for:** A design iteration. `design/vapor-mark.js` and the app are byte-identical, so the app is not lagging the design.


### TD-41 : (closed 2026-08-21 — the tree is gone)

**The Godot CI job runs without the GDExtension**, so DSP-dependent tests are not covered. Deliberate — building Essentia from a HEAD-only tap on every run is worse — but the gap is real.

**Closed by AUD-4.** The Godot tree was deleted on 2026-08-21; this described a gap in CI coverage of a tree that no longer exists. `godot-final-v1.78` holds it if the reasoning is ever needed again.

**Where:** MIG-040


### TD-42 : (closed 2026-08-21 — the tree is gone)

~~**12 GUT tests fail**, never diagnosed.~~ **Diagnosed (2026-08-16); not fixed.** All twelve are **stale tests**, not product defects — every one chases an interface that moved, and nothing in the app is broken by them. Three causes: (a) **nine** in `test_library_screen_parsing.gd` call `library_screen.call("_parse_track_info", …)`, and that function is not on `library_screen.gd` — path parsing lives in `metadata_service.gd`; (b) **two** — `test_focus_track_emits_signal` and `test_sidebar_preview_square_visibility` — pass five arguments to `track_focused`, which gained a leading `href` and now takes six; (c) **one**, `test_mini_player_responsive_squeeze`, asserts nav labels of `"♪ Library"` and `"▤ Playlists"` that collapse to icons when narrow, and no such label exists in the source any more. A thirteenth, `test_sidebar_hidden_on_mobile`, is *risky* rather than failing: it asserts nothing at all. **Deliberately not fixed** — this is `vapor-library::naming` and the new shell's problem now, and repairing tests for a tree phase 5 archives spends effort on the thing being deleted. Recorded so nobody has to run it again to find out.

**Closed by AUD-4.** The Godot tree was deleted on 2026-08-21; this described a gap in CI coverage of a tree that no longer exists. `godot-final-v1.78` holds it if the reasoning is ever needed again.


### TD-45 : (closed 2026-08-21 — the tree is gone)

**The GUT baseline numbers in CI and in this document describe different runs.** CI pins `EXPECTED_PASS: 190 / EXPECTED_FAIL: 19` because it runs the *stub* path with no GDExtension (TD-41); the 211/12 quoted here and in `ci.yml`'s own comment is the local run with the dylib present. Both are correct and they are twelve lines apart in the same file, which is how a person ends up chasing a discrepancy that is not there. **Fixed in `ci.yml`** — both comments now say which run they describe. Kept as a record of why the numbers differ.

**Closed by AUD-4.** The Godot tree was deleted on 2026-08-21; this described a gap in CI coverage of a tree that no longer exists. `godot-final-v1.78` holds it if the reasoning is ever needed again.

**Where:** TD-41, TD-42


### TD-43 : (blocked)

**The fixture set is not reproducible by anyone else.** Validation runs against a personal library via `extract-fixtures.mjs`; there is no synthetic corpus, so no one else can verify the analysis numbers.

**Waiting for:** Real work, and the one item that would unblock another. Generating audio with known tempo and key that is representative of real recordings is a research problem.



### PERF-004 : (done)

**Opening Songs freezes the app for about a second, because a screenful of rows
pulls a screenful of full-resolution album covers.**

Measured 2026-08-20 on the 563-track library:

```
516 cover files, 149 MB total
median 281 KB   largest 954 KB   (base64 text, as stored)
```

The table is virtualised and artwork is already lazy — `lib/artwork.ts` fetches
per href with an LRU, and `library_view` deliberately carries no covers, both of
which were done for exactly this reason. The remaining cost is that each of the
~26 rows that mount (20 visible + `overscan: 6`) asks for a **full-resolution**
cover to draw at **48 px**. That is ~7 MB at the median and ~23 MB on a run of
large ones, read from disk, pushed through IPC, JSON-parsed in the webview and
decoded into 26 images, per open.

**Fix:** a thumbnail. 96 px covers a 48 px slot at 2× DPR and encodes to about
4 KB, so a screenful becomes ~130 KB rather than ~7 MB — roughly 55× less.
Generate on first request, cache beside the full cover in `covers.rs`, expose as
`track_thumb`, and point `artwork.ts`'s row path at it. Now Playing and Liner
Notes keep the full-resolution cover, which is the one place it is wanted.

Needs the `image` crate (jpeg + png features only); nothing in the workspace
decodes images today.

**Done 2026-08-20.** 128 px thumbnails, generated once and cached beside the
full cover. Measured against the real library: 301 KB average down to 8.1 KB,
37x smaller; all 516 generated and now 3.8 MB total against 149 MB. Two things
the first attempt got wrong and measurement caught: unoptimised generation was
330 ms a cover, so a first open would have been *worse* than the freeze (fixed
by `[profile.dev.package."*"] opt-level = 2`); and `track_thumb` held the state
mutex across that work, which would have moved the freeze onto the transport
rather than removing it.

**Waiting for:** Nothing.


### REL-001 : (closed 2026-08-23 — moved to the release pipeline)

**There is no release signing, on either platform, so every build is a
throwaway.** Wanted for later; recorded 2026-08-20 so it is not rediscovered.

**macOS.** `tauri build` produces an ad-hoc signed `.app` — `Identifier=
com.dylangrowcoot.vapormusic`, `Signature=adhoc`. Locally built it runs, but
the signature is a hash of the binary, so every release build is a new identity:
a keychain grant does not survive one, and the app cannot be given to anyone
else without Gatekeeper refusing it. The dev loop is already fixed — see
`src-tauri/.cargo/config.toml`, which signs with a stable self-signed identity
and pins the identifier — and the same identity can be set as `signingIdentity`
under `bundle.macOS` for release builds. Distribution to another machine needs a
paid Developer ID and notarisation; that is a separate decision.

**Android.** No keystore, so builds go out `--debug`, which means the package is
`com.dylangrowcoot.vapormusic.debug` (installs *beside* a release build rather
than replacing it) and the APK carries full native debug symbols: **591 MB**,
measured 2026-08-20, against a fraction of that for release. It installs to a
Pixel 9 over wireless ADB in 58 s, so the size is survivable rather than
blocking.

**What it needs:** a keystore (`keytool -genkeypair`, or Android Studio), a
`keystore.properties` kept out of git, and the `signingConfigs` block wired in
`gen/android/app/build.gradle.kts`. Half an hour, most of it deciding where the
key lives so it is not lost — a lost upload key cannot be replaced for an app
already on Play.

**Closed as a ticket 2026-08-23, at Dylan's direction**, and for the same
reason as AUD-21: it belongs to the release pipeline rather than to the board.
It is **step 2** in `docs/workspace/release-epic.md`, where the macOS and
Android halves are written out separately — the macOS one has a free part and a
paid part, and they should not be decided as though they were one thing.

The debug builds are not an accident to be corrected. They are what a build for
Dylan's own device is, and they stay that way until the release steps run.

---

## From the ten-desk audit (2026-08-21)

Recovered from the conversation that commissioned it. **The consolidated
artifact and the ten desk reports are both gone** — the artifact URL returns
"not found" and the reports were in a previous session's scratchpad, which has
been cleaned. What follows is what survives in the thread itself.

**What could not be recovered:** the ranked findings list had thirteen items and
only the *answers* to them survive, not the titles. Items 3, 5, 7, 12 and 13 are
unrecoverable — the replies were "lower priority", "good catch, let's do this",
"make a note to work on all of these", "sounds good to check this" and "ok, yes
lets do that", none of which names its subject. The "seven items in This week"
were never enumerated in the thread either. If the artifact is ever reachable
again, that is what to look for.

Three of the thirteen were verified first-hand and are all closed: CI red for 23
hours on `cargo fmt` (69f6a68), `licenses/README.md:3` granting the app under
AGPL-3.0 (a2cfd25), and the `dragDropEnabled` fix written twice by two sessions
hours apart (08c28de).

### AUD-1 : EU Cyber Resilience Act reporting, 11 September (not triggered)

Vulnerability-reporting duties attach on 11 September to anyone monetising into
the EU. **Decision 2 is a true donation with nothing unlocked**, which is not a
supply, so this does not attach. Recorded because it re-arms the moment money
buys anything at all — that is the same trigger that would bring EU VAT with no
threshold and withdrawal rights with it.

**Waiting for:** Nothing, unless decision 2 changes.

### AUD-2 : Google Play developer verification, 30 September (not triggered)

Attaches to Play distribution. **Decision 3 is direct download for v1**, so it
does not apply. Re-arms if Play is ever the channel; decision 5 already puts one
manual Android run on real hardware ahead of that.

**Waiting for:** Nothing, unless decision 3 changes.

### AUD-3 : what the supporter actually gets (built 2026-08-23 — waiting on a Ko-fi handle)

Four desks independently rejected a donation button that unlocks customisation:
money that unlocks features is consideration, which buys the whole legal and tax
profile of a paid app while capping revenue at donation levels, and calling a
sale a gift adds a misleading-practice exposure on top. Decision 2 took the
no-strings route.

Dylan still wants to give something back. **The rule that keeps it a gift:
nothing contingent on payment.** Candidates that survive it — a supporter credit
in About from an opt-in list, early access to a build everyone gets anyway
(time-shifted, not withheld), a separate cosmetic download on itch such as
wallpapers or a Camelot wheel print. What breaks it is any feature only a payer
can reach, whatever the button says.

The UX desk's locked-swatch treatment — 45% opacity, dissolving on unlock — was
designed for the rejected model and is worth reading before designing this one,
but it is gone with the reports.

**Decided 2026-08-23.** The thank-you is **a pin at the bottom of Settings, one
added per donation.** No feature behind it, nothing withheld, nothing gated —
just a visible record that somebody contributed. Dylan's framing: it gets people
to feel like they truly contributed to something.

**The work is the donation logic first**, and the pin after it. What the pin
looks like is a smaller question than whether money can arrive at all, and the
second one has no code behind it whatsoever today — a grep for donate, supporter,
ko-fi, patreon, paypal and itch across `src/` and `src-tauri/src/` returns
nothing.

**One thing to keep straight while building it.** A pin awarded *because* money
arrived is the same sentence as a purchase, and the rule from `DECISIONS.md` §2
is that nothing may be contingent on payment. What keeps this on the right side
of the line is that the pin does nothing and gates nothing — it is a receipt,
not a good. That distinction is load-bearing and should survive into whatever
the button and the copy end up saying.

**Channel: Ko-fi**, and the shape turned out to need no logic at all.

The problem looked like "how does the app learn that somebody donated". Ko-fi
has webhooks but no read API, so knowing it at runtime means running a service
to catch them — a thing to maintain forever and a fourth host in a privacy
document that makes a checkable claim about three. And it still would not give
a person *their* pin without an identity, which would mean the app verifying
proof of payment before showing something, which is the exact shape §2 rules
out.

**Dylan's answer, and it is better than either option that was on the table:
read the number off the Ko-fi dashboard and write it into the build.** No
server, no runtime call, no privacy entry. The desktop updater already runs on
every launch, so the number reaches people without any mechanism being invented
for it.

The failure mode is the good one. A hand-written count is stale between
releases, and stale can only mean *low* — the number is monotonic and the file
is only ever revised upward. The app can undercount the people who chipped in
and can never claim support it did not get.

**Built:** `src/lib/supporters.ts` holds `SUPPORTERS` and `KOFI_HANDLE`;
`SupporterPins` draws one disc per supporter at the bottom of Settings, with the
count in words above it because the discs are `aria-hidden`. Ten tests, on the
rules rather than the rendering: everybody sees the same wall (the component
takes a count and has no notion of a viewer, so there is no path by which it
could learn who paid), the copy states that nothing is bought or unlocked, the
wall caps its discs and says how many it did not draw, and a build with no
handle renders **nothing** rather than a link to `ko-fi.com/`.

The pins accumulate on the project, not on the person. You donate, the next
build has one more pin, and one of them is yours — which is the feeling that
was wanted, and it is the version that cannot become a purchase.

**Waiting for:** the Ko-fi handle. The card does not render until there is one.
The art is a placeholder — a flat enamel disc drawn from the theme's tokens —
and nothing outside `SupporterPins.tsx` knows what a pin looks like.

### AUD-4 : delete the Godot tree (done 2026-08-21)

The QA and Architecture desks disagreed. Architecture won on the numbers; QA's
point that the CI ratchet script is sound was also true, and "delete it" is
irreversible in a way "nightly dispatch" is not. Dylan decided delete, keep the
tag — see DECISIONS.md §6.

498 tracked files: `addons/` 248, `scripts/` 87, `tests/` 59, `scenes/` 16, plus
`src/`, `autoloads/`, `godot-cpp/`, `project.godot`, `SConstruct`. The archive
tag `godot-final-v1.78` exists and is on origin, so the tree survives deletion.
Closes TD-41, TD-42 and TD-45, which exist only because that tree does, and
drops the `godot (stub path)` job from `ci.yml`.

479 files removed, the `godot (stub path)` job dropped from `ci.yml`, and
`tools/` with it — both scripts in it were Godot's. TD-41, TD-42 and TD-45 close
with it. The `godot-cpp` submodule is deregistered and `.gitmodules` is gone.

### AUD-5 : the four named UI findings (3 of 4 done 2026-08-23)

Titles reconstructed from Dylan's answers; the desks' own wording is gone.

* **Onboarding.** "Without a WebDAV server you can't really use the app" — the
  first-run path assumes a server that a new person does not have. Improve the
  flow generally, not only that one screen.
* **The texts.** Copy throughout, approved without reservation ("Love it").
* **Contrast.** Believed already fixed once; to be re-checked rather than
  re-fixed. `docs/DESIGN_LANGUAGE.md` commits to WCAG 2.1 AA and names 4.5:1 for
  `--text-primary` on `--bg-glass`.
* **The library view.** Playlists, groups, albums and artists — and in no case a
  wall of rows. Horizontal scrolling on those shelves should pull more in
  lazily.

**Waiting for:** The `screens` and `components` lanes; all four land there and
another session has been in both.


**Three of the four are closed. The fourth is AUD-13 and stays there.**

* **Onboarding — done.** First run now offers two paths with the local folder
  first: "Choose a folder on this device", and "Connect a server instead"
  beneath it. The finding was that the flow assumed a server a new person does
  not have; there is now a path that needs no server, no address and no
  password, and it is the one offered first.
* **The texts — nothing to do.** Approved without reservation. Recorded so the
  next reader does not go looking for the work.
* **Contrast — done, via AUD-6.** It was to be re-checked rather than re-fixed,
  and the re-check found two real failures: `--ink-3` at 2.24:1 and `--ink-4` at
  2.76:1, the latter being what `.label` is set in. Both now clear 4.5:1 against
  the palest stop of the page gradient, guarded by a computed test. The ticket's
  own proposed value would not have cleared it — `#6b7079` measures 4.28:1.
* **The library view — open, and tracked as AUD-13.** Playlists, groups, albums
  and artists rather than a wall of rows, with shelves pulling in lazily. That is
  the same change AUD-13 needs for transport reasons, so it belongs in one place
  and that place is AUD-13, not here.

### AUD-6 : the Daylight secondary ink scale is under AA (done 2026-08-23)

`--accent-fill` was 4.02:1 under a white label and was fixed first — `#0062d6`,
5.65:1, guarded by a computed test in `src/lib/tokens.test.ts`. The rest of the
scale was measured at the same time and left alone, because darkening it changes
how every quiet label in the app looks and that was a design call rather than a
correctness one. Dylan made it on 2026-08-23: darken to AA.

Daylight, re-measured 2026-08-23 against both backgrounds a label can sit on —
the flat `--page` `#eceef1`, and `#dfe9f5`, the palest stop of `--page-gradient`
and so the real worst case at the top of the window:

| Token | Value | on `#eceef1` | on `#dfe9f5` | |
|---|---|---|---|---|
| `--ink` | `#14161a` | 15.58:1 | 14.76:1 | passes |
| `--ink-2` | `#676c75` | 4.54:1 | 4.30:1 | passes on the flat page, not at the top |
| `--sov-ink` | `#237a52` | 4.54:1 | 4.30:1 | same, to two decimal places |
| `--accent` as text | `#007aff` | 3.46:1 | 3.27:1 | large text only |
| `--ink-soft` | `#7b8189` | 3.38:1 | 3.20:1 | large text only |
| `--sov-quiet` | `#5c8f76` | 3.20:1 | 3.03:1 | large text only |
| `--ink-4` | `#8b9099` → `#646971` | 2.76 → **4.75:1** | 2.61 → **4.50:1** | fixed |
| `--ink-3` | `#9ba1aa` → `#626974` | 2.24 → **4.76:1** | 2.12 → **4.51:1** | fixed |

`--ink-4` was the one that mattered: it is what `.label` is set in, at 11px,
which is the most repeated text style in the app. Nine rules take it, fifty-two
take `--ink-3`, and eighteen more take `--ink-3-rgb`, which moved with it so the
two do not drift.

Three things the re-measurement changed:

* **The proposed `#6b7079` does not clear 4.5:1.** It is 4.28:1 on `--page` and
  4.06:1 on the gradient, so it would have closed the ticket without fixing it.
  Every other number in the original table verified exactly.
* **The bottom of the scale is now nearly flat**, and that is arithmetic rather
  than a choice. 4.5:1 on the palest stop is a single luminance ceiling; both
  tokens are the lightest hex on their own hue that sits under it, so what
  separates them is hue, not lightness. One channel lighter on either fails.
* **`--screen-gradient` starts paler still** at `#dcebfa`. Both new values clear
  it too — 4.55:1 and 4.56:1 — so nothing inside a device frame is worse.

Guarded in `src/lib/tokens.test.ts` beside `--accent-fill`: the palest stop is
read back out of `--page-gradient` rather than written down, so restyling the
horizon cannot quietly lower the bar.

`docs/DESIGN_LANGUAGE.md` commits the app to WCAG 2.1 AA, so this was a gap
against a stated position, not a suggestion.

**Waiting for:** Two decisions, neither blocking this.

* **`--ink-2` `#676c75` is 4.30:1 at the top of the gradient** and was left
  alone — it passes on most of the screen, and it is body-adjacent rather than a
  quiet label, so moving it is a louder change than this ticket was scoped for.
  The lightest value on its own hue that clears both is `#646971` at 4.75:1 and
  4.50:1 — which is exactly where `--ink-4` landed. Taking it would collapse
  `--ink-2`, `--ink-3` and `--ink-4` into a single colour, which is the design
  call the flatness above is already pressing on.
* **Lamplight's `--ink-4` `#8a7c6c` is 4.39:1** on its `--page` `#1c1712`.
  Found while measuring, not in the original table, which only covered Daylight.
  Not encoded in the test as acceptable, for the same reason the Daylight
  failures were not.

**Closed:** `--ink-4` is `#646971` and `--ink-3` is `#626974` in
`vapor-app/public/tokens.css`, both the lightest hex on their own hue that
clears 4.5:1 on `--page` and on the gradient's palest stop, with `--ink-3-rgb`
moved to match. Two computed assertions in `src/lib/tokens.test.ts` hold them
there; 301 frontend tests pass.

---

## What the audit left, written down (2026-08-22)

The ten desks produced more than the seven decisions in `docs/DECISIONS.md`.
Twelve of their findings were fixed on 2026-08-21/22 and are closed in the
commit log; what follows is everything still open, one ticket each, so the
backlog stops living in a conversation.

Ordered by what blocks distribution, not by which desk raised it.

### AUD-7 : LAN sync is readable and forgeable on the wire (done 2026-08-23)

`peers.rs` has no transport encryption — grep for tls, noise, cipher or
encrypt returns nothing. That much was already recorded as TD-56 and accepted
for a single trusted network.

The part TD-56 does not cover: the SHA-256 digest arrives in the same plaintext
reply as the bytes it covers (`lib.rs`, `Reply::Bytes`). An attacker on the path
rewrites both, so the integrity check confirms the content they substituted.
That is not eavesdropping, it is injection into the audio cache, and it reaches
the decoder.

Fix is at the pairing layer: derive a session key from the pairing secret and
store the peer's public key in `Trust`. The two unbounded allocations either
side of this were closed in `30ba440`.

**Waiting for:** Nothing. This is the one security item that should land before
the app runs on a network Dylan does not own.

**Closed:** Pairing runs an X25519 exchange and HKDF derives a 32-byte key per
peer, stored in `Trust` beside the device rather than on `TrustedDevice`, which
goes to the webview whole. Every `Reply::Bytes` is HMAC'd over a
length-prefixed transcript of href, offset and digest — the href and offset are
in there because without them a signed chunk is portable, and the peer's own
bytes for one track replayed as the answer for another would verify.
`accept_bytes` is the only way that variant is unpacked. 19 tests.

Two consequences worth knowing. `Trust::allows` now wants a key as well as a
listing, so **every existing pairing has to be made again**; `needs_repairing`
exists only to tell the owner why, and the wire refuses a stale pairing and an
unknown device identically so that nothing on the subnet can ask whether a
device id is known. And `os_random` was reading `/dev/urandom`, which does not
exist on Windows — every PIN on that target took the fallback, so the
unreachable "no OS randomness" branch was the normal path. It uses `getrandom`
now.

TD-56 is untouched and still accurate: there is no transport encryption, and
this does not add any. What it removes is the forgery, which was the half TD-56
did not cover.

### AUD-8 : the end-to-end suite never reaches Rust (done 2026-08-23)

`vite.e2e.config.ts` aliases `@tauri-apps/api/core` to `src/test/browser-ipc.ts`
— the same fake the component tests use. So the e2e suite drives the real UI
against a TypeScript reimplementation of the backend, and the seam between them
is exactly where all five defects that reached the first outside user lived.

`seam.rs` covers that seam properly but only for 12 of 102 commands.

**Waiting for:** A decision on how far to go. Extending `seam.rs` a few commands
at a time is cheap; making the e2e suite drive the real shell needs
`tauri-driver` and is a different size of job.

**Closed:** `seam.rs` now covers 30 commands, up from 12 — the library and entity reads, search and its facets, the queue view, settings round trips, downloads and the sync view, chosen by where a wrong wire shape strands a user rather than logs an error. Not `tauri-driver`; the ticket left the size open and this is the cheap half. Two of the new tests were wrong rather than the code, and both were checked against the implementation before either side was touched.

### AUD-9 : the monkey runs against a backend that cannot fail (done 2026-08-23)

`e2e/monkey.spec.ts` takes 250 random actions across three seeds. Every command
it hits succeeds, because the fake always answers — one `.fail()` exists in the
whole `e2e/` directory, in `journeys.spec.ts`.

The first defect that ever reached an outside user was an error bar that could
not be dismissed. A random walk is the right tool for finding that, and this one
structurally cannot.

**Waiting for:** Nothing. A `failureRate` on the fake plus a fourth seed is
about thirty lines.

**Closed** in `b50948a`, and the row stayed open afterwards — noticed
2026-08-23 while looking for work that was actually left. `monkey.spec.ts`
arms failures from the same seeded generator as the actions, so seed N fails
the same commands at the same points every run, and `SEEDS` is four:
`[1, 20_260_816, 99_991, 4_242_424]`. Nothing was added to `src/test/ipc.ts`
to do it — the fake already had `fail`/`clearFailure`, and that file is a seam
that holds answers rather than decisions.

### AUD-10 : four modules with no tests (3 of 4 done 2026-08-23)

`decoder.rs` (seek offsets, silence gating), `sync.rs`'s drift-thread lifecycle,
`android.rs` (359 lines of JNI), and `secrets/{mod,desktop}.rs` (137 lines).

`secrets/desktop.rs` is the one to do first: three of the five defects that
reached the first outside user were keyring behaviour, and it is the wrapper
around the keyring.

**Waiting for:** Nothing.

**Closed:** 26 tests across `secrets/`, `decoder.rs` and `sync.rs`. The keyring came first as the ticket asked, and needed two inline `match` blocks extracted into named functions before it could be tested at all — without touching the real keychain. `android.rs` is deliberately still untested: 359 lines of JNI that cannot run off-device, where a test mocking the JNI boundary would assert the mock.

### AUD-11 : dynamic groups do not sync at all (done 2026-08-23)

`sync::Shared` carries playlists, folders, bpm overrides and tombstones. It does
not mention groups — zero occurrences in `sync.rs`. Groups are a first-class
organising concept with their own rail and eight commands, and they exist on
exactly one device with nothing in the UI saying so.

They are entity sets, so additive merge works and needs no new tombstone kind.
The per-track removal tombstone added in `b121925` is the template if one is
ever needed.

**Waiting for:** Nothing. Either add `groups` to `Shared`, or say in the UI that
groups are per-device.

**Closed:** `groups` and `Tombstones::groups` added to `sync::Shared`, merged by the same `keep_earliest` as playlists and folders. `SHARED_VERSION` 3 → 4; a version-3 build refuses the document rather than writing back one in which every group on every device has been deleted. Five tests on the cases that lose data.

### AUD-12 : the cover cache has no ceiling (done 2026-08-23)

`covers.rs` has `get`, `put`, `thumb` and `size` — and no `max_bytes`, no
eviction, no `clear`. The audio cache beside it has a full LRU and three
commands to manage it.

At 50k tracks that is multiple GB with no lever short of "delete all data".
`data_breakdown` reports the number and offers nothing to do about it.

**Waiting for:** A choice — give `Covers` the same LRU as `Cache`, or stop
persisting full-size covers and keep thumbs plus a re-derive path.

**Closed:** Neither, quite. The second option is not available at the price it
sounds like: a cover comes out of the file's own tags during analysis and
nothing re-reads a tag on demand, so on a WebDAV library "re-derive" means
pulling the whole track back to recover one thumbnail. And the first would
treat covers like cached audio, which comes back on its own.

So: a 4 GiB ceiling, evicted **full covers first, thumbnails only if that was
not enough**, oldest first within each tier. A thumbnail is ~6 KB against a
~281 KB cover, so the first tier reclaims about 98% of the bytes while every
row, queue tile and shelf still draws — the full cover is wanted by one view,
the now-playing art, and it degrades to a thumbnail rather than to a blank.

The ceiling is a constant, not a setting, and that is the opposite of
`cache_max_bytes` on purpose: lowering the audio bound costs a download,
lowering this one costs artwork that only a full analysis pass brings back.
`clear_cover_art` is the deliberate lever, on Your Data beside "Empty cache",
and it says the different thing after it — "Artwork comes back when the library
is analysed again."

Seven Rust tests and two screen tests. The fake now models cover bytes with an
effect, so a screen claiming the artwork is gone can be contradicted.

Found while landing it: `argument_names_agree_on_both_sides` in
`tests/ipc_contract.rs` skipped any parameter whose type *contained* `Window`,
so AUD-13's `window: Option<RowWindow>` was dropped rather than checked. Fixed
in `e9a03d5`. It had been hidden because `cargo test` stops at the first failing
binary and `docs_state_claims` had been red since `6f13af3` — four of the nine
binaries were not running at all.

### AUD-13 : library_view returns the whole library, per keystroke (done 2026-08-23)

It clones every matching row, applies tags and analysis per row, sorts, groups
and serialises the lot — under the global mutex, on a debounced query effect. At
50k tracks that is roughly 15–25 MB of JSON per call, while the virtualiser
renders about forty rows.

The virtualiser solved rendering. Nothing solved transport. Dylan's own framing:
the library view should show playlists, groups, albums and artists, never a wall
of rows, and horizontal shelves should pull more in lazily — which is the same
fix from the product end.

**Waiting for:** The library redesign, which is where this naturally lands.

**Closed:** Ahead of the redesign, because the transport half stands on its own
and the redesign is not scheduled. `library_view` takes a `window` beside
`view` and answers with a `LibraryPage` — the window's sections, the total
before the window, and the offset it was clamped to. Measured at 50,000 rows:
17,649,902 bytes and 153 ms of `JSON.parse` per keystroke before, 106,032 bytes
and 0.27 ms after.

Clamped rather than refused, because a scroll position and a row count arrive
from two different round trips and a window past the end is what a library
shrinking under a search looks like. Ordering stays in Rust: a caller ordering
its own window would order each window separately and show a different order
per screenful. `libraryView` still returns everything for the three callers
that need every row — queueing a table, resolving a dragged entity, playing an
album — which are presses rather than keystrokes.

`seam.rs` covers the window through the real IPC, including that the shell
reads it under the key the frontend sends it under.

The product half of the ticket — playlists, groups, albums and artists instead
of a wall of rows — is untouched and still belongs to the redesign.

### AUD-14 : the DJ planner is outside the portable core (done 2026-08-23)

`choose_transition`, `candidate_cost`, `dj_pick`, `extend_set`, `plan_mix` and
the tuned weights live in `lib.rs` and take `&AppState`. The repo keeps a wasm CI
job over three crates specifically to hold the core platform-free, and the
feature the product is named for is in none of them.

The planners are already portable; what binds them is the command wrappers.
Behind a narrow `Catalogue` trait this is one to two days, and the tuned
constants get property tests that need no Tauri runtime.

**Waiting for:** Nothing, but it wants the `lib.rs` split further along first.

**Closed** as a fourth crate, `vapor-core/crates/vapor-dj`, in the wasm job
alongside the other three. It holds `Exit`, `exit_between`, `candidate_cost`,
`kind_distance`, `choose_transition` and the six tuned constants, over plain
values — nine tests, three of them proptests, none needing a Tauri runtime,
which was the whole ask.

**Why a fourth crate.** `vapor-library` cannot depend on `vapor-engine` — the
engine brings cpal and signalsmith on every non-wasm target and the catalogue
has no business carrying an audio device. `vapor-engine` could depend on
`vapor-library`, but then the audio engine carries regex, ts-rs and tag parsing
to answer a question it never asks. The planner is the one thing that needs
both.

**What stayed, deliberately.** `extend_set`, `plan_mix`, `track_meta_pool` and
`arm_mix` are still in `lib.rs`. They are `&mut` on a queue, a playhead and
three maps on `AppState`, and a `Catalogue` trait wide enough to abstract them
would be a larger fiction than the code it removed. The A* was never the
problem — `vapor_library::generate_mood_path` has been portable all along. This
ticket's own line was the right reading: the planners are already portable, and
what binds them is the command wrappers. The wrappers stay; the arithmetic does
not.

Two doc comments were repaired rather than moved as they stood:
`choose_transition` claimed three transition types could not be ported yet and
returns all three, and a paragraph about whether a mix can happen at all had
drifted onto `candidate_cost` and is back on `plan_mix`.

### AUD-15 : finish the lib.rs split — 39 commands left (done 2026-08-23)

62 of 101 moved on 2026-08-22 (`7aa97b5`, `f96f391`), 11,033 lines down to
9,663. Still in `lib.rs`: library, sync and peers, downloads, data, settings,
metadata lookup.

Until it is done, `CLAUDE.md`'s seam rule still binds — one session in the
backend at a time — which is the constraint the split exists to remove.

The extractor and its two hard-won fixes are worth reusing: a multi-line `fn`
signature closes a span on its first parameter unless brace counting waits for
the body to open, and a blind rewrite of a handler entry also matches
struct-field shorthand in `AppState { … }`.

**Waiting for:** Nothing. Mechanical, and each domain is independently green.

**Closed:** 34 commands moved into `commands/{library,lookup,sync,downloads,data,settings}.rs`,
and `lib.rs` went 10,385 → 9,170 lines with no `#[tauri::command]` left in it —
the five remaining matches are doc comments explaining why a command cannot be
integration-tested, which is also where this ticket's "39" came from.

The move surfaced a second `generate_handler!` in `seam.rs` naming `settings`
bare, which broke the moment the function left `lib.rs` and only under
`--all-targets`.

**`CLAUDE.md`'s seam rule is deliberately unchanged.** Its condition — "until it
is split into `commands/<domain>.rs`" — is met, but the reason underneath it is
not: `lib.rs` is still 9,170 lines and every command still goes through one
mutex on one `AppState`. Whether two sessions may now share the backend is
Dylan's call, not the line count's.

### AUD-16 : no privacy policy, EULA or support route (partly done 2026-08-23)

None of the three exists. Verified 2026-08-22: no policy or terms file anywhere,
no `mailto:` in the app or the README.

* **Privacy policy** — mandatory for both stores regardless of how little is
  collected, and artist/album/track names do leave the device when lookups are
  on. Needed even for direct download.
* **EULA** — `LICENSE` is a repository copyright notice. It grants a person who
  downloads a build no right to run it.
* **Support** — there is no telemetry and no crash reporting, both deliberate,
  which makes a person describing a fault the only channel there is. The app can
  now name its build (`b8eddb6`); nothing tells them where to send it.

**Done: the privacy policy.** `PRIVACY.md`, written from the code rather than
from a template. Every claim in it is checkable and the last section says how.
It found one thing the ticket did not have: **the desktop updater is a third
network destination**, it runs at every launch, and it is the only one with no
switch — `lib.rs`'s `#[cfg(desktop)]` updater block asks GitHub for
`latest.json` and installs silently. `RELEASE.md` §3's "the app talks to two
strangers, both off by default" is therefore short by one, and that one is not
off by default. Documented as such rather than smoothed over. Confirmed on the
way: `metadata_lookup_enabled` and `sync_enabled` are both `false` in
`Settings::default` (`vapor-library/src/settings.rs`), and lookups send exactly
three strings — artist, album, title — never a path, never an id.

**Done: the support route.** `SUPPORT.md`, a separate file rather than a section
of the privacy policy, because the two stores ask for them in two different
fields and a person hunting a bug should not have to read a data-protection
notice to find where to write. Names the five things a report needs, starting
with the build stamp the About lockup already prints.

**Done: the EULA groundwork, not the EULA.** `docs/EULA-NOTES.md` states what
`LICENSE` does and does not grant, the nine things a binary distribution needs
that it has none of, and the six questions actually worth a lawyer's time —
including the MPL-2.0 set inside a proprietary binary, and the silent
auto-update, which no template will know about.

**Waiting for:** Dylan, on two things and only two.

1. **A contact address.** `SUPPORT.md` carries
   `<CONTACT ADDRESS — Dylan to fill in>` and is otherwise finished. Not a
   session's to invent.
2. **The EULA itself**, which still wants a template and a lawyer before
   anything is sold. `docs/EULA-NOTES.md` is the brief to hand over, not a
   substitute for one.

`docs/EULA-NOTES.md` also carries `<JURISDICTION — Dylan to confirm>` and
`<LEGAL ENTITY AND ADDRESS — Dylan to fill in>`. Neither blocks anything today;
both are inputs the lawyer will ask for first.

Untouched deliberately: `docs/RELEASE.md` §3 and its §7 checkbox
("Privacy declaration covering the lookup services"). That checkbox is the
store's own data-safety form, which is Dylan's to fill in, and `RELEASE.md` is
the platform lane.

### AUD-17 : contributions are unsettled, on a public repository (done 2026-08-23)

`docs/LICENSING.md` names this as the precondition for going public: settle
contributions first, because one accepted outside pull request freezes the
licence choice and cannot be undone unilaterally.

The repository was already public when that was written, so it is not a step
before a switch — the switch is thrown.

**Waiting for:** Dylan. Disable pull requests, or add a CLA. Minutes either way.

**Closed 2026-08-23 — pull requests disabled.** GitHub has no switch that does
this: interaction limits come closest and expire after six months, which is a
reminder nobody gets. So `.github/workflows/no-outside-prs.yml` closes any pull
request whose author is not the repository owner, leaves a comment saying why,
and points the author at the issue tracker, which stays open.
`CONTRIBUTING.md` says the same thing to anybody who reads before writing.

It closes rather than blocks, deliberately: a blocked contribution is invisible
and looks like the project is broken, while a closed one carries its reason.

`pull_request_target` rather than `pull_request`, because a fork's
`pull_request` run gets a read-only token and cannot close anything. That event
is dangerous when a workflow checks out and runs the pull request's code; this
one checks out nothing and holds one permission.

**Not a position on outside help.** The blocker is that no contributor licence
agreement exists, and one accepted patch would freeze the licence choice in a
way that needs every contributor's agreement to undo. If a CLA lands, the
workflow and `CONTRIBUTING.md` are what change.

### AUD-18 : Deezer is called without registration or terms review (half done 2026-08-23)

`metadata.rs` calls `search/artist`, `search/track`, `track/{id}`, `search/album`
and `album/{id}`, and downloads artwork from `cdn-images.dzcdn.net` which is then
kept permanently — the file is the cache. No API key, no registered application,
no `User-Agent` naming a contact.

An analysis pass over a whole library is a lot of requests. `docs/RELEASE.md` §3
already lists reviewing the rate limits and terms as outstanding.

**Waiting for:** Deezer or MusicBrainz. That is a decision, not work, and it is
Dylan's — see `docs/workspace/release-epic.md`.

**Half done:** The calls are identified and paced, which is worth having
whichever way the decision goes. Every request carries a `User-Agent` naming
the app, its version and the project URL — LRCLIB requires that in as many
words and a URL satisfies it, so nothing here invents a contact address, and
the same header is what MusicBrainz asks for should the lookups move. One line
to change if a mailbox is wanted instead.

Three clocks: 200 ms Deezer, 300 ms LRCLIB (the middle of the band they name
for scanning a library), 200 ms for the artwork CDN. Separate so a slow answer
from one service cannot spend the other's budget, shared across threads so two
lookups queue rather than each keeping its own polite gap. Four attempts with a
doubling wait, at most 3.5 s.

The number that made this urgent: ~0.29 s per Deezer request measured on this
machine, which is about 7 requests a second sustained across a 563-track pass,
with nothing watching for a refusal. Deezer documents no quota at all — 50 per
5 seconds is what every client in the wild converges on, which is why the gap
is set at half of it.

`docs/RELEASE.md` §3 still lists the terms review as outstanding, correctly.

### AUD-19 : Windows CI was red for three days (done 2026-08-23)

`shell (windows-latest)` exited `0xc0000139` (`STATUS_ENTRYPOINT_NOT_FOUND`)
before any test ran, from 2026-08-20 to 2026-08-23. Both Windows jobs, since
`installers` reaches the same binary through `types:check`.

Not the runner image, which is what this ticket and `FINDINGS.md` both said:
the green and red sides of the boundary ran the same image, `20260810.198.2`.
The real cause was that `tauri-build` hands its application manifest to cargo
with `cargo:rustc-link-arg-bins=`, so the app binary carried the
Common-Controls 6.0.0.0 declaration and the test binary did not — harmless
until `tauri-plugin-dialog` brought in `rfd` and its `TaskDialogIndirect`
import, which only comctl32 version 6 exports.

`build.rs` now takes the manifest off tauri-build and declares the dependency
through the linker for every target. Two failures queued behind it were fixed
in the same pass: `seam` invoked from `tauri://localhost`, which is not the
local origin on Windows, and the throwaway signing key was written to `/tmp`,
which does not exist there.

Full account, including the two false positives that made the diagnostic step
lie twice, in `docs/FINDINGS.md`.

**Result:** 284 tests pass on `windows-latest`; `installers (windows-latest)`
produces an NSIS installer. All eight jobs green in run 32630969786.

### AUD-20 : CI actions float on tags, and nothing watches dependencies (done 2026-08-23)

`release.yml` pins all seven of its actions by commit SHA, because it is the one
workflow holding the signing key and `contents: write` together. `app.yml`
(0 of 12) and `ci.yml` (0 of 6) do not.

There is also no Dependabot or Renovate, and no `cargo-deny`. `docs/LICENSING.md`
warns that a new transitive dependency can reintroduce copyleft silently, and
then relies on a manual pass — which has already drifted once, 624 packages to
627.

**Waiting for:** Nothing. SHA-pinning is mechanical; `cargo-deny check licenses
advisories` turns the licence inventory from a document into a gate.

**Closed** in `f49b048`, with `f45a51a` behind it, and the row stayed open —
noticed 2026-08-23 alongside AUD-9's. Counted rather than taken from the
release epic: `app.yml` 17 of 17 pinned, `ci.yml` 9 of 9, `release.yml` 7 of 7,
`.github/dependabot.yml` present, `cargo-deny` gating in `ci.yml`.

`f45a51a` is the part worth remembering: two of `release.yml`'s five pinned
SHAs pointed at commits that do not exist upstream, so the pinning that made
the workflow safe would have failed it on the first tag push.

### AUD-21 : the updater keypair is mismatched, and deliberately unmanaged (closed 2026-08-23 — moved to the release pipeline)

On 2026-08-22 a session printed the updater private key into a transcript while
verifying it, and rotated it in response. `~/.tauri/` now holds a new keypair;
the old one is kept beside it suffixed `.COMPROMISED-2026-08-22`.

**`tauri.conf.json` still carries the old public key.** So the config trusts a
key whose private half is the compromised file, and a build signed with the new
key would produce a signature the app refuses. Nothing depends on this until a
release is signed.

Dylan's decision, same day: no private key gets saved or managed until
distribution is actually on the table, because every handling of it is a chance
to leak it. That is the right trade while nothing ships.

**Closed as a ticket 2026-08-23, at Dylan's direction.** It is not a defect
somebody forgot to fix, it is the last step of shipping, and a row that can only
be closed at the very end reads as neglect every time anyone scans the board.
It is now **step 1 of the release pipeline** in
`docs/workspace/release-epic.md`, with the key-location decision and the
mismatched public key recorded there in full.

Nothing about the state of the world changed with this edit: `tauri.conf.json`
still carries the old public key, the private half of it is still the
`.COMPROMISED-2026-08-22` file, and the first signed build is still where that
bites.

### AUD-22 : nothing has been released, and the pipeline is unexercised (open)

`release.yml` exists and its logic is verified as far as it can be locally — the
version check runs against the real files, the YAML parses, and a local
`tauri build` with the signing key produces exactly the artefacts it expects,
including the `.sig`. It has never run on a tag.

`docs/RELEASE.md` already records the intended first move: tag an `-rc` so a
mistake costs a draft rather than the release `releases/latest` resolves to,
which is the URL compiled into every binary.

**Waiting for:** Steps 1 and 2 of the release pipeline in
`docs/workspace/release-epic.md` — `verify` fails without the signing secret —
and a decision about what v1 contains.

That last one was asked on 2026-08-23 and turned out to have no answer on
record: **no document anywhere names a feature set for v1.** `DECISIONS.md` §3
picks a distribution channel for it, §2 defers a question to v1.1, and
`RELEASE.md` costs it. Donation and onboarding are the first two things anybody
has said are in it.

### AUD-23 : the app has no front door (redefined 2026-08-23 — it is onboarding, not marketing)

No landing page, no domain, no demo. The README was rewritten on 2026-08-22 to
stop describing a product that does not exist, and is now a developer's README
rather than a pitch — which is the right shape for it, and leaves nothing
pointing a stranger at a download.

The marketing desk's position, kept because it survived its own claims audit:
lead with the mix rather than the manifesto, because it is demonstrable in eight
seconds and is the only claim with measurement behind it. A recording of one
transition is probably the single highest-leverage artefact that does not exist.

**Redefined 2026-08-23.** Not a landing page, not a domain, no recording of a
transition — **no theatrics.** The front door is inside the app:

* A **small onboarding window that appears as a person moves around the app**,
  rather than one wall of explanation at the start.
* **No opening "connect your music" modal.** Send a new person to Settings and
  to the connection settings, with a help modal, instead.

The marketing position above is kept for whenever an outside-facing page is
actually wanted, and is explicitly not what this ticket is any more.

**Read this before starting.** First run is not bare today — AUD-5's onboarding
item landed a two-path chooser: "Choose a folder on this device" first, and
"Connect a server instead" beneath it, so that the flow no longer assumes a
WebDAV server a new person does not have. What is proposed here **replaces** that
opening modal rather than filling an empty space, and the reason that chooser
exists — a person with no server must have a way in — has to survive whatever
replaces it.

**Order:** after the donation work.

**Waiting for:** Nothing.

---

### Closed on 2026-08-22, for the record

Android ran on real hardware for the first time — a Pixel 9, over wireless ADB.
The debug APK installed and `MainActivity` launched with `libvapor_app_lib.so`
loading cleanly and no JNI error, which is the failure `docs/DECISIONS.md` §5
was written about. Launching is not the same as exercising the credential store,
so §5's precondition is partly met rather than met.

### AUD-24 : every electronic record is one genre (done 2026-08-23)

Nils Frahm, Delta Heavy, Jerry Paper, Eptic and xKore all sit under
"Electronic". They have almost nothing to do with each other, and no amount of
editing tracks by hand fixes the cause.

The cause is `metadata::genre_of`, which reads `genres.data[0].name` from a
Deezer **album** response. Two problems in one line:

* **`[0]`** — the first of however many, so a record tagged electronic and
  drum-and-bass keeps only the first.
* **Deezer's taxonomy is about twenty-five top-level genres.** There is no
  "drum and bass", no "riddim", no "neo-classical". Everything electronic maps
  to Electronic because that is the only shelf Deezer has.

`Row::genre` is also a single `String`, so even a richer source has nowhere to
put a second genre today.

**Stated preference: more granular and more specific, not less.**

There is **no last.fm or MusicBrainz work anywhere** — no branch, no worktree,
no file, no commit. Checked 2026-08-22. If a session was asked to do this, it
produced nothing that survives, so this starts from zero.

Worth weighing before starting:

* **MusicBrainz** — already the recommendation in AUD-18 for a different
  reason (Deezer is being called unregistered). Has genres *and* folksonomy
  tags, is built for this, and asks only for a `User-Agent` naming a contact.
* **last.fm** — user tags, far more granular than any editorial taxonomy and
  correspondingly noisy: "seen live" and "favourites" are tags too. Needs an
  API key. Best treated as a source to filter, not to trust.
* **The file's own tags** — already read by `lofty`, already the most specific
  thing available for a well-tagged library, and currently overwritten rather
  than preferred.

Shape of the work: `Row::genre` becomes a list; a source order is decided (the
file first, then a service); and the Genres tab groups on the list rather than
on one string. `Source` already distinguishes file, folder, service and
unknown, so provenance has somewhere to go.

**Waiting for:** A decision on the source, and on whether one track may appear
under several genres — which is the thing that makes granularity useful and is
a real change to how the tab reads.

**Closed:** Genre aliases landed in `vapor-library/src/genre.rs` with Last.fm alongside Deezer, so the Electronic bucket resolves to the specific label rather than the coarse one. Merged in `df747d2`.

### AUD-25 : drag and drop is broken, and the fix is sitting on a branch (done 2026-08-23)

Reported on desktop and on mobile, 2026-08-22.

The implementation exists and was never merged. `37369ec`, on
`worktree-drag-to-groups`, is **16 files and 802 insertions** — `drag.ts` +152,
a new `vapor-app/src/lib/entityDrag.ts` that main does not have at all, plus
`Home`, `Library`, `Playlist`, `Songs` and 100 lines of tests.

What did land in main is `08c28de`, "Land another session's drag fix, at the
owner's request": **two files, eighteen lines** — the `dragDropEnabled: false`
line in `tauri.conf.json` and a small `PlaylistRail` change. That config line
is necessary and not sufficient. It stops wry from swallowing the drag before
the webview sees it; it does not implement dragging anything.

So the branch holds the working half and main holds the config half, which
matches the symptom exactly.

Two hazards before merging:

* It is based on `278cb5d` and main has moved a long way since — including the
  `lib.rs` split. Expect conflicts in the screens it touches.
* `dragDropEnabled` was written twice by two sessions, byte-identical, hours
  apart. Whichever way this merges, check the file ends with one copy.

**Waiting for:** Nothing. Merge `37369ec`, resolve, and confirm on both
platforms — mobile is the one that has never been checked, and the Pixel 9 now
runs the app.

**Closed:** The branch was merged and confirmed on desktop and narrow. Merged in `6c394a2`.

### AUD-26 : drum and bass is detected at half tempo, and the fix does not fire (done 2026-08-23)

Three DnB tracks read 86/87/87 on the Vibe cards. Doubled, all three are
squarely in the genre's range. Investigated 2026-08-22; the chain is longer
than it looks and **AUD-24 is a precondition, not a cosmetic sibling.**

**The detector is not wrong.** All three tracks are in
`fixtures/essentia_ground_truth.json` and Essentia reports 85.90, 86.99 and
86.99 for them. `vapor-dsp` is reproducing the reference exactly. No clamp is
responsible — `MIN_BPM 60 / MAX_BPM 200` (`tempo.rs:21`) leaves 174 well
inside. The octave prior is not responsible either: prior(87) is 0.8429 against
prior(174) 0.7960, a 5.9% tilt the comb clears easily. The comb genuinely
prefers 87, because in an amen pattern the kick/snare alternation makes the
two-beat span the dominant periodicity. `FINDINGS.md` measured this already.

**The corpus is not short of fast music** — 54 of 563 entries are above 160 BPM
(verified). What it has is a **108-entry pile in the 84–90 band, 19% of the
whole corpus**, full of half-read drum and bass: Delta Heavy, Calibre, Break,
Lenzman, Alix Perez, High Contrast all at 85.8–87.0, while Keeno and Camo &
Krooked sit at 172.27. Essentia is internally inconsistent on this genre, and
the 81%-agreement figure **scores reproducing its half-tempo reading as
correct**. That is why this was accepted rather than solved: the metric could
not see it.

**The correction already exists and never runs.**
`vapor_library::octave_correct` (`genre.rs:165`) with a
`("drum & bass", 160.0, 185.0)` band maps 87 → 174. `tempo_band`
(`genre.rs:91`) matches on **exact string equality** after trim and lowercase,
so "Electronic" — which is all Deezer ever returns for this music, per AUD-24 —
misses every band. So does "DnB", "D&B", "Drum n Bass", "Breakbeat".

**And where it does fire, it only fixes the label.** `octave_correct` appears
exactly once in the shell, at `lib.rs:4869` in `track_meta_pool`, which feeds
the cards. `beat_grid` (`lib.rs:3738`) and the Tempo Morph target
(`lib.rs:4021`) read only `bpm_override`. So correcting the genre today would
make a card read 174 while the stretcher still meets a genuine 87 BPM record at
87 — **visibly right and audibly wrong, which is worse than now.**

Damage as it stands: `exit_between` (`lib.rs:5095`) compares raw BPM, so
87-DnB reads as adjacent to 87-hip-hop and the DJ will pick it. Energy is
unaffected — `energy_level` is loudness, not tempo.

**Order matters.** Aliases first (`dnb`, `d&b`, `drum n bass`, `drum'n'bass`,
`breakbeat`, punctuation stripped in `tempo_band`) — pure table work, no DSP
risk, and `octave_correct` still refuses when the tempo is already in band.
Then route `bpm_of` and `beat_grid` through the same correction, or the two
numbers stay inconsistent. **Do not touch `TEMPO_SIGMA`**; `FINDINGS.md`
records the prior as load-bearing, with the rival's comb score over twice the
truth's on at least one track.

Also connects to AUD-27: no beat grid means no ribbon pulse, and a BPM
corrected without retracking leaves the grid stale.

**Waiting for:** Nothing. The aliases are the first move and are self-contained.

**Closed:** `octave_correct`, `TEMPO_BANDS` and `tempo_band` landed in `vapor-library/src/genre.rs`, so a drum and bass track read at 87 is lifted to its own band. Merged in `df747d2`.

### AUD-27 : the Vibe ribbon reacts to brightness, not to tempo (done 2026-08-23)

Reported as "not behaving correctly" 2026-08-22. It is fully wired and mapped
to the wrong signals — no missing plumbing anywhere.

The chain is real: the audio thread publishes realtime peak level and
brightness (fraction of block energy above 1500 Hz, allocation-free,
`audio.rs:846-876`) into atomics; `PlaybackState` carries `level`,
`brightness`, `beatPeriod`, `nextBeat`; `Vibe.tsx:104` converts the beat into a
`performance.now()` timestamp; `vapor-ribbon.js` extrapolates it per frame.

The intended mapping is beat interval → speed, with volume and frequency on
other effects. What `vapor-ribbon.js:476` does:

```js
twistRate = TWIST_BASE + TWIST_BRIGHT * bright + TWIST_BEAT * this._beatPulse(time);
```

Brightness drives speed. Level drives *turn count*, not motion. And **beat
period never affects speed at all** — it scales only the decay window of a
kick, whose mean contribution works out to ~0.32 rad/s regardless of period, so
60 BPM and 180 BPM produce the same average ribbon speed. That is the reported
symptom exactly: it reacts to beats and never to tempo.

A comment at `vapor-ribbon.js:471` records the divergence as deliberate —
"Level moved OFF the rate and onto `turns`: loudness and brightness both
driving speed made them indistinguishable". The reasoning is sound and it
solved the collision by removing the wrong one: it took *level* off the rate
and left *brightness* on it, when the design says the rate belongs to tempo.
Tempo was never a candidate.

Two things mask it independently of any fix:

* `prefers-reduced-motion: reduce` freezes the whole element — read once at
  connect (`:327`), `_frozen` zeroes the pulse. Checked on Dylan's machine
  2026-08-22: **off**, so not the current cause.
* No beat grid, no pulse. `beat_window` returns `(0,0)` unless the track is
  analysed *and* the grid matches the current BPM, so a BPM corrected without
  retracking kills it — see AUD-26.

Only `Vibe.tsx` passes any of these. `NowPlaying.tsx:205` passes `energy`
alone; `App.tsx:658` and `Settings.tsx:672` pass nothing.

**Waiting for:** Dylan on the mapping. Proposed: tempo → rate, brightness →
hue, level → turns, which gives each signal its own channel — what the old
comment was reaching for. No shell changes needed; all three already arrive.

**Closed:** Remapped in `vapor-app/public/vapor-ribbon.js`: `TWIST_BASS` takes bass to rate, `TWIST_TEMPO` takes tempo to the resting rate, and `BRIGHT_HUE` moves brightness onto hue instead of speed. Level still drives the number of turns.

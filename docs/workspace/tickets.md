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

### AI-004 : Intelligent Cross-Fading Playback Engine (active)
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

### EQ-003 : Settings Headphone Calibration Selection UI (completed)
**User Story:**
As a user,
I'd like to select my headphone model from a list in the Settings screen,
So that the correct corrective EQ profile is applied automatically.

**Context:** The Settings screen needs an elegant selector interface showing supported headphone models, with search capability, and a master calibration toggle.

**Description:** Redesign settings options to include an AutoEQ section. Add a searchable OptionButton/dropdown for headphone models, a master bypass switch, and target visual graphs showing the correction curve.

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

### SYNC-001 : Local Subnet Peer Discovery (mDNS/UDP) (active)
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

### SYNC-002 : Secure Direct Pairing & Session Handshake (active)
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

### SYNC-003 : Database Reconciliation Protocol (active)
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

### SYNC-004 : Direct Peer File Transfer Pipeline (active)
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

### SYNC-005 : Peer-to-Peer Synchronization Dashboard UI (active)
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

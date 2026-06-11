# 🎵 Vapor Music

> *Own your music. Own your vibe.*

A local-first, AI-enhanced music player built in Godot for desktop and mobile. Vapor Music is for people who are done renting their music from corporate algorithms — and who refuse to sacrifice the premium, intelligent listening experience that comes with modern streaming.

---

## Philosophy

The era of streaming has trained people to accept two things as trade-offs: **convenience** and **ownership**. You could have one, but not both.

Vapor Music rejects this entirely.

We believe your music library is a personal artifact — one that should survive licensing collapses, algorithm shifts, internet outages, and corporate pivots. Your collection should feel *alive*, not like a read-only database you pay a monthly fee to access. It should know your mood. It should guide your listening. It should feel like the world's best DJ that happens to live entirely in your pocket or on your desk.

This is not a utility tool. It is an **active audio experience** built around the philosophy that the best music player is one that understands your music as deeply as you do.

---

## The Counter-Culture We Serve

There is a quiet but rapidly growing ownership renaissance among music lovers. They are exhausted by:

- Libraries that disappear overnight when licensing deals expire
- Jarring, mood-killing shuffle transitions between incompatible tracks
- Aggressive data harvesting that treats listening habits as a product to sell
- Dependency on internet connectivity just to hear songs they've loved for decades
- Corporate interfaces that prioritize discovery revenue over the listener's own collection

Vapor Music is built *for* these people.

---

## Core USPs

### 1. 🎛️ Harmonic AI Mixing — *The Local AI DJ*

This is the heart of the app's identity and the feature that separates Vapor from every other local music player.

Traditional players shuffle by picking `Random Song A` and crossfading it into `Random Song B`. This destroys the vibe. Vapor doesn't shuffle — it **conducts**.

**How It Works:**

- **Waveform & Energy Analysis** — At import time, Vapor analyzes every track locally on the user's device or home server, extracting:
  - BPM (Beats Per Minute)
  - Musical key (e.g., C Major → A Minor — using the Camelot Wheel for harmonic compatibility)
  - Spectral density and frequency profile
  - Perceived energy level and mood classification
- **Infinite Blend Shuffle** — When shuffle is engaged, the app dynamically constructs a listening path. A mellow acoustic track will never jump directly into heavy electronic music. Vapor bridges the gap through mid-tempo, harmonically compatible intermediary tracks, gently steering the wavelength of the session.
- **Intelligent Auto-Mixing** — Instead of a generic 5-second volume fade, Vapor identifies the optimal **exit beat** of the outgoing track and the optimal **entry intro** of the incoming track. It subtly pitch-shifts and BPM-adjusts the next track by ±1–2% to create a seamless, gapless transition that feels professionally mixed.

**Key Differentiator:** All analysis and mixing logic runs entirely **on-device**. No cloud AI, no subscriptions, no privacy compromise.

> [!NOTE]
> A critical analysis of the current transition limitations and a technical roadmap for professional-grade mixing can be found in the [AI DJ Refactor Plan](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/docs/ai_dj_refactor_plan.md).

---

### 2. ☁️ Zero-Config Cloud Sync — *Local-First, Frictionless Everywhere*

The primary reason everyday users stay on Spotify is friction. Setting up Navidrome or Plex requires Docker, port forwarding, and a networking degree. Vapor solves this without a server.

**The Architecture:**

```
[ Your Music Files ]  ──►  [ Encrypted Cloud Storage ]  ──►  [ Smart Client Apps ]
 (PC / NAS / Drive)         (Proton Drive / WebDAV /          (Desktop / Mobile)
							 Mega / Google Drive)                      │
													   Local AI analysis runs here
													   (BPM, Key, Energy Mapping)
```

- **Bring Your Own Cloud** — Users connect their existing private cloud storage directly. Supported backends target Proton Drive, Mega, Google Drive, Dropbox, and any WebDAV-compatible server. No Vapor-controlled infrastructure sits between the user and their files.
- **P2P Local Network Sync** — When a phone and PC are on the same Wi-Fi network, they fast-sync directly with each other using local discovery — no internet required, no bandwidth charges, no latency.
- **Encrypted at Rest** — Files are encrypted client-side before leaving the device. Vapor never sees the content of a user's library.

---

## High-Value Feature Set

### 📂 Playlist Management & Interactive Curation
Vapor Music features a local-first, drag-and-drop playlist curation system that natively integrates with our Vibe DJ transitions:
* **Collapsible Sidebar Hub**: The navigation sidebar houses a collapsible **Playlists** section. Create new playlists on-the-fly using the inline `+` button, and rename existing entries by double-clicking them to expose dynamic text inputs.
* **Fluid Drag-and-Drop Workflow**:
  * Drag tracks from the main **Library** browser and drop them onto sidebar playlist items to add them instantly.
  * Drop tracks anywhere in the active **Playlist Screen** to append them, or drop them directly onto a track row to insert them at that specific index.
* **Drag-to-Reorder Mechanics**: Arrange your vibe by grabbing track row drag handles (`☰`) in the playlist view and dragging them to reorder.
* **Living Custom Covers & Metadata**: Customize playlist cover art using the visual `Pencil` icon overlay. Select files using the built-in `FileDialog` or drag-and-drop cover art files directly from your operating system explorer. If no cover art is defined, the system automatically falls back to displaying the cached album art from the playlist's first track.
* **Vibe DJ Integration**: Playlists aren't static lists; they feed into the Vibe DJ's engine. Toggle **Smart Mixing** to dynamically generate transition effects (Standard Crossfade, Bass Swap, or Filter Sweep) based on BPM and musical key differences between adjacent tracks. Engage **Harmonic Shuffle** to calculate an optimized, smooth blend transition sequence through your entire playlist.

### 🎨 Digital Liner Notes & Living Metadata
Streaming services stripped away the beautiful context of music — the album art booklets, production credits, session notes, and lyrics. Vapor restores it.

- A dedicated **Liner Notes** screen acts as a premium digital vinyl sleeve
- Pull high-resolution artwork variants, historical context, and production notes into a gorgeous, clean UI
- Full in-app **metadata editor** — edit ID3/FLAC tags directly with an elegant interface, no third-party tools required

### 🎧 Acoustic Environment Profiling
For audiophiles playing back lossless FLAC or ALAC files, output calibration matters.

- Built-in **headphone profiles** sourced from open databases like [AutoEQ](https://github.com/jaakkopasanen/AutoEq)
- Users select their headphone model and Vapor applies a corrective EQ curve at the software layer, compensating for hardware frequency response deviations
- Ensures the mix that was intended in the studio is the mix that reaches the listener's ears

### 🖼️ Visual Aesthetic Customization
Vapor's UI is built for focused, distraction-free listening.

- **Apple-inspired glassmorphism** — frosted-glass panels, a minimal 3-colour palette, and seamless adaptation to any wallpaper or desktop. Two built-in themes: **Vapor Dark** (deep charcoal glass, `#0A84FF` accent) and **Vapor Light** (white frosted glass, `#007AFF` accent).
- **Low-Glare Ambient Mode** — a distraction-free, dimmed UI designed for late-night or ambient listening
- Dashboard widget mode — a minimal, unobtrusive now-playing overlay for multitaskers
- Fully themeable with community-shareable colour schemes (see the [Theme System Developer Guide](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/docs/theme_system.md) and [Design Language](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/docs/design_language.md) for details on creating custom visual presets)

---

## Competitive Positioning

| Feature | Corporate Streaming | Standard Local Players | **Vapor Music** |
|---|---|---|---|
| **Ownership** | ✗ Renting only | ✓ 100% Owned | ✓ **100% Owned** |
| **Library Sync** | ✓ Seamless | ✗ Manual file transfers | ✓ **Automated Cloud / P2P** |
| **The Vibe** | ✗ Jarring shuffles & ads | ✗ Basic crossfades | ✓ **Harmonic AI DJ Transitions** |
| **Privacy** | ✗ Aggressive data tracking | ✓ High privacy | ✓ **Private & Decentralized** |
| **Metadata & Art** | ✗ Minimal / controlled | ✗ Basic tag editors | ✓ **Living Liner Notes** |
| **Audio Fidelity** | ✗ Lossy compression | ✓ Lossless support | ✓ **Lossless + HW Calibration** |
| **Server Required** | N/A | ✗ Sometimes | ✓ **Never — zero config** |

---

## Ideal Architecture

### Technology Stack
- **Engine:** Godot 4.x (GDScript / C# where performance-critical)
- **Target Platforms:** Windows, macOS, Linux (Desktop) · Android, iOS (Mobile)
- **Audio Backend:** Godot's built-in AudioStreamPlayer with custom DSP nodes for BPM sync, pitch shifting, and EQ
- **Local Analysis:** Background-threaded analyzer running in Godot (GDScript/C#) to parse audio files, extract PCM data, and calculate BPM/key/energy values with zero external dependencies.
- **Cloud Sync Layer:** Abstract provider interface with per-backend drivers (WebDAV, rclone-compatible)
- **Database:** Serialized JSON database catalog (`metadata_cache.json`) for maximum portability and zero-config deployment on desktop and mobile platforms, storing track metadata, energy profiles, and listening history.

### Module Breakdown

```
vapor-music/
├── core/
│   ├── audio_engine/        # Playback, DSP, crossfade, BPM sync
│   ├── analyzer/            # Local track analysis (BPM, key, energy)
│   └── library/             # Track indexing, JSON cache database, metadata I/O
├── sync/
│   ├── cloud/               # Provider-agnostic cloud sync drivers
│   └── p2p/                 # Local network discovery & direct sync
├── ui/
│   ├── player/              # Now playing, queue, waveform visualizer
│   ├── library/             # Browse, search, filter views
│   ├── liner_notes/         # Album art, metadata editor, credits
│   └── settings/            # EQ profiles, themes, cloud config
└── ai/
	└── dj/                  # Harmonic path construction, mood graph
```

### Data Flow

```
 Import Track
	  │
	  ▼
 [Analyzer] ─── BPM, Key, Energy, Spectral Profile
	  │
	  ▼
 [JSON Library Cache] ─── Indexed, searchable, portable
	  │
	  ▼
 [AI DJ Module] ─── Constructs harmonic listening paths
	  │
	  ▼
 [Audio Engine] ─── Intelligent crossfade, pitch correction, EQ
	  │
	  ▼
 [Output] ─── Calibrated by headphone profile
```

---

## Design Principles

1. **Local-First** — Every feature must work with zero internet connectivity.
2. **Zero-Config** — No servers, no ports, no Docker. If a normal person can't set it up in 5 minutes, it's not ready.
3. **Privacy by Architecture** — Vapor cannot see user data by design, not just by policy.
4. **The Vibe is Sacred** — No jarring transitions. Ever. The listening experience is a first-class citizen.
5. **Own Your Data** — Library metadata, listening history, and analysis results are stored in open, portable formats the user can inspect, move, and back up themselves.

---

## Status

> 🚧 Early development — Godot project scaffolding in progress.
>
> **v1.26 (2026-06-10):** Smoothed the Tempo Morph DJ transition effect. Increased the pitch scale morph ramp duration to 50% of the transition duration (max 3.0s) for a gentler tempo match, and fixed a bug where the incoming player's pitch_scale was prematurely reset to 1.0 at the end of the transition, causing abrupt post-transition tempo jumps. All 105 tests passing.
>
> **v1.25 (2026-06-10):** Resolved critical audio manager playback and DJ transition states. Fixed issue where manual skips triggered outro wait loops causing overlapping audio tracks, and corrected deck/tween pause states to synchronize correctly across all active channels. Introduced AudioStreamGenerator mocks for fast, offline unit test runs. Added regression unit tests for transition skips, pause synchronization, and silent incoming load state. All 102 tests passing.
>
> **v1.24 (2026-06-10):** Expanded Vibe transition effects to include Echo Out, Reverb Freeze, and Tempo Morph. Programmatically added delay and reverb effects to audio buses and integrated a smart selection mapping using BPM differences and match categories (Perfect, Interesting, Creative) to select from 6 transition types. Simplified and polished transition documentation to be extremely punchy and legible in the Vibe Workbench Help Modal. All 100 tests passing.
>
> **v1.23 (2026-06-10):** Refined DJ transition timings by introducing transition-specific durations (Bass Swap: 6.0s, Filter Sweep: 4.0s, Standard Crossfade: 3.0s) and implementing a wait-for-outro trigger loop to execute transitions exactly at the outgoing track's end. Implemented a browser-style back/forward playback history stack for "Next" and "Previous" skip buttons, ensuring navigation follows the actual playback path. Added new unit tests for history navigation. All 97 tests passing.
>
> **v1.22 (2026-06-10):** Refined the DJ transition effects (Bass Swap and Filter Sweep) to support proper, professional-grade track overlapping at high volumes instead of simple full-duration crossfades. Made audio bus initialization robust against existing configurations and expanded transition tests. Optimized unit test execution speed by 10x (down to 1.5s) using dynamic transition scaling. All 96 tests passing.
>
> **v1.21 (2026-06-10):** Standardized the active AI DJ blend status to remain consistent across transition completions. Added a new Transition Effects section to the interactive Help Modal powered by `docs/ai_dj_workflow.md` to detail the Bass Swap, Filter Sweep, and Standard Crossfade rules, and updated developer instructions to maintain parity when modifying transition logic. All 92 tests passing.
>
> **v1.20 (2026-06-10):** Implemented the Playlist management feature. Created PlaylistService to persist playlists to `user://playlists.json`, support creating, deleting, renaming, and custom cover art copied to local disk (`user://playlist_images/[hash].[ext]`), with fallback to the album art of the first track. Added a collapsible Playlist section to the Sidebar with LineEdit-based renaming, and implemented drag-and-drop targets. Created PlaylistScreen with edit-in-place title controls, cover art uploading (supporting FileDialog and OS drag-and-drop), a frosted glassmorphic empty state, and a track list with reorder handles and remove buttons (drag-and-drop track reordering supported). Added comprehensive unit tests; all 90 tests passing.
>
> **v1.19 (2026-06-10):** Optimized library scan startup speed by loading and displaying cached tracks instantly (less than 1s) from a local `user://library_cache.json` database. Initiated deep recursive WebDAV server directory scanning in the background and implemented cache diffing to dynamically sync added/deleted tracks only when discrepancies are found. All 83 tests passing.
>
> **v1.18 (2026-06-10):** Added a "?" help button to the Vibe Workbench header that opens an overlay Help Modal. Implemented a dynamic Markdown-to-BBCode translation engine that parses `docs/ai_dj_workflow.md` directly at runtime to populate the help text. All 83 tests passing.
>
> **v1.17 (2026-06-10):** Enhanced the active AI DJ blend status to display the intended upcoming transition track and transition effect (e.g., Standard Crossfade, Bass Swap, or Filter Sweep) throughout track playback. The intended transition dynamically updates when the user chooses a next track override or when the playlist context changes, replacing the "AI DJ standby" display. All 79 tests passing.
>
> **v1.16 (2026-06-05):** Resolved duplicate track metadata lookups by tracking active/pending requests in `MetadataService` and yielding process frames when a lookup is already running for a track. This prevents parallel, redundant Deezer and LRCLIB HTTP requests and API rate-limiting during startup, library scanning, or track transitions. All 73 tests passing.
>
> **v1.15 (2026-06-04):** Optimized background analysis speed by using Godot's built-in fast hashing (speeding up analysis by 99%). Improved metadata service to query and resolve missing genres from Deezer API by sanitizing trailing whitespace in search queries and classifying "Unknown genre" ID3 tags. Added dynamic Vibe Screen behavior that selects the best compatible transition match by default and highlights it in the card list, allowing users to override the automatic DJ selection. All 72 tests passing.
>
> **v1.14 (2026-06-03):** Expanded testing suite footprint with two new dedicated test scripts targeting WebDAV core utility modules (`test_webdav_service.gd`) and screen transition stack managers (`test_nav_manager.gd`). All 58 unit tests passing.
>
> **v1.13 (2026-06-03):** Implemented debounced cache writing in `MetadataService`. Replaced instantaneous main-thread disk I/O on metadata updates with a 2-second debounce timer to eliminate redundant writes and frame-rate drops. Integrated a clean shutdown handler to flush pending saves to disk. All 48 tests passing.
>
> **v1.12 (2026-06-03):** Implemented local audio disk caching in `AudioManager` under `user://audio_cache/`. Streams are downloaded directly to local storage using `HTTPRequest.download_file` to reduce memory overhead and are cached for instant playback in subsequent loads. Cleaned up incomplete downloads dynamically on network interruptions. All 48 tests passing.
>
> **v1.11 (2026-06-03):** Implemented persistent connection reuse in `WebDAVService` for folder scanning. Bypassed TCP/TLS handshake overhead by reusing the active socket for subsequent directory requests and gracefully releasing it when scanning completes. Retried failed/stale connections automatically exactly once. All 48 tests passing.
>
> **v1.10 (2026-06-03):** Refined the sidebar lyrics overlay background blur shader (`blur.gdshader`) by reducing the default blur amount (from 5.0 to 2.0) and darkening mix factor (from 50% to 30%), providing a cleaner and more subtle legibility backdrop over album art. Ensured a minimum opacity of 50% to guarantee readability when transparent background options or empty buffer queries are present. Removed the unused round album art placeholder panel in the Now Playing sidebar section. Configured the sidebar to automatically update the preview square with the playing song's album cover and lyrics whenever the track changes (e.g., skip, play, or autoplay). De-duplicated the track path parsing code by centralizing it into `MetadataService` and delegating from `library_screen.gd`. All 48 tests passing.
>
> **v1.9 (2026-06-03):** Implemented a robust track metadata parsing algorithm in the library screen, resolving a major bug where track number prefixes or varied track artists were causing compilation albums to split into 15+ different artists under an "Unknown Album". Added support for parsing "Artist - Album" formatting in single-folder directories (e.g. `gorillaz - demon days`). Integrated a dynamic fallback system that uses the `MetadataService` cache and query capabilities (via Deezer search) to resolve missing artist, album, and track details for unknown tags. Added new unit tests confirming correct parsing of structured filenames and single-folder album structures; all 47 tests passing.
>
> **v1.8 (2026-06-02):** Implemented the Sidebar Preview Square (`PreviewSquare` AspectRatioContainer) that displays artist images, album covers, or lyrics based on active focus selection in the library. Added automatic local file downloading and caching to `user://metadata_images/` in `MetadataService`, along with signal dispatch for focus states. Integrated a screen-space background blur shader (`blur.gdshader`) for the lyric overlay. Added new unit tests verifying focus flow; all 37 tests passing.
>
> **v1.7 (2026-06-02):** Implemented a metadata lookup and caching service (`MetadataService`) querying public, keyless APIs (Deezer and lyrics.ovh) to fetch artist images, album covers, and song lyrics asynchronously. Implemented a persistent local cache stored at `user://metadata_cache.json` which automatically prunes entries when tracks are removed from the WebDAV library scans. Added unit test coverage with all 39 unit tests passing.
>
> **v1.6 (2026-06-02):** Redesigned track loading animation by confining the indeterminate loading progress bar to overlay exactly on top of the playback progress bar/scrubber track on mobile. Also implemented a custom vertical loading animation on the desktop vertical progress bar, where a themed loading circle dynamically travels up and down the separator track during track loading states.
>
> **v1.5 (2026-06-02):** Implemented a 3-second debounce timer on skip (next/previous) controls to avoid spamming network requests to the WebDAV server. The track metadata and UI now update instantly when skipping to provide a responsive interface, while the remote streaming requests are delayed until skipping stops. Direct track selection immediately cancels the timer and plays the song. Added full test coverage for the debounce logic in a new AudioManager test file. All 27 unit tests passing.
>
> **v1.4 (2026-06-02):** Implemented dynamic UI squeezing for the bottom player bar at narrow widths, switching button text to symbol-only mode and reducing spacing constraints dynamically. Lowered the minimum window size constraint to 380x400 to allow squeezing elements together to a minimum, and locked OS window resizing to prevent the player bar from coming off the panel. Removed side/bottom borders and aligned the bottom corner radius mathematically (`RADIUS_LG - margins = 12px`) to fit perfectly within the parent window frame. Added test coverage with all 25 unit tests passing.
>
> **v1.3 (2026-06-02):** Implemented a premium, Apple-quality glassmorphism shader (`assets/shaders/premium_glass.gdshader`) featuring dynamic SDF-based rounded borders, light source highlight/shadow shading, 1-pixel resolution-independent noise grain, and a backdrop saturation boost with a transparency fallback. Bound container dimensions and corner radius dynamically on window resize. All 24 unit tests passing.
>
> **v1.2 (2026-06-02):** Restructured playback controls. Replaced bottom horizontal mini-player on desktop with a top-to-bottom vertical progress bar running directly along the sidebar separator line, and square windows-style media control tiles in the sidebar footer. Repositioned mobile progress bar to sit horizontally at the top edge of the bottom panel. Implemented custom borderless window drag-to-move (via sidebar) and corner/edge drag-to-resize. All 23 unit tests passing.
>
> **v1.1 (2026-06-02):** Visual overhaul to Apple-inspired glassmorphism. New Vapor Dark and Vapor Light themes. Single-accent palette (`#007AFF` / `#0A84FF`). All 13 theme manager unit tests passing.

---

*Built with Godot 4 · Designed for music lovers who remember owning things.*

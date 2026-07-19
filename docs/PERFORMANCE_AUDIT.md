# Vapor Music — Performance & Responsiveness Audit

**Date:** 2026-07-18
**Method:** Static code review (GDScript, scenes, shaders). *Not* runtime-profiled.
**Scope:** `scripts/`, `scenes/`, `autoloads/`, `assets/shaders/`. The C++ in `src/` was not reviewed.

> Items marked **[verify]** are reasoned from configuration rather than measured, and
> should be confirmed on-device before acting on them.

---

## Status Legend

- [ ] Not started
- [x] Done
- [~] In progress / partial

---

## P1 — Per-frame work

### [x] 1.1 `vibe_screen._process` does per-frame waveform extraction
**File:** `scripts/screens/vibe_screen.gd:690`

Runs unconditionally whenever the node is in the tree — no `is_visible_in_tree()` guard
(unlike `settings_screen`, which has one). Per frame it:

- allocates two `PackedFloat32Array`s
- calls `get_waveform_peaks(1000)` **twice** — 120,000 floats/sec across the
  GDExtension boundary at 60fps
- performs 3 separate `MetadataService.get_cached_metadata()` lookups
- re-assigns `transition_timeline.outgoing_peaks` / `incoming_peaks` every frame

Waveform peaks only change when the track changes.

**Fix applied:** `_process` now does only the playhead — position read +
`update_playback_state()`, behind an `is_visible_in_tree()` guard. Everything else
moved to `_refresh_waveforms()` / `_refresh_trigger_time()`.

**Why it's still polled (deliberate):** peaks arrive asynchronously — a DSP reports
`get_cache_sample_count() == 0` until decode completes, and there is **no completion
signal**. So the refresh is driven by track/transition signals *plus* a 2 Hz backstop
poll (`WAVEFORM_POLL_INTERVAL`). Per-href guards (`_peaks_out_href` / `_peaks_in_href`)
mean `get_waveform_peaks()` only runs when peaks are genuinely new, so the poll is
near-free. Net: 60 Hz → 2 Hz, and the expensive call went from unconditional to
once-per-track.

**Cache invalidation points:** `track_changed`, `transition_started` (deck roles swap),
`transition_completed`, and `visibility_changed` (refresh no-ops while hidden, so it
must re-resolve on becoming visible).

**A real completion signal on the DSP would let the poll go away entirely** — worth
doing when `src/audio_dsp.cpp` is next touched.

---

### [x] 1.2 `settings_screen._process` formats diagnostic strings every frame
**File:** `scripts/screens/settings_screen.gd:419`

Three `Performance` monitor queries + three string allocations + three `Label`
re-layouts per frame, for a readout no human reads faster than ~4Hz.

**Fix applied:** Diagnostics moved to `_update_diagnostics()`, driven by a
`DIAGNOSTICS_INTERVAL` (0.25s) Timer. Only the spinner rotation remains in `_process`.

---

### [x] 1.3 `_calculate_chunk_rms` was a per-audio-sample GDScript loop
**Files:** `src/audio_dsp.cpp`, `src/audio_dsp.h`, `scripts/services/audio_manager.gd`,
`scripts/services/audio_dsp_stub.gd`

Called from `_feed_deck` every frame, per deck. Ran 4 one-pole filter updates per sample
per channel in GDScript.

**Fix applied:** Ported to `AudioDSP::calculate_chunk_rms()` in the GDExtension.
Coefficients and arithmetic order kept identical; accumulation in `double` because
GDScript floats are 64-bit. Uses `chunk.ptr()` for direct packed-array access rather
than per-element marshalling.

**Design improvement:** filter state now lives on the `AudioDSP` instance. Since each
deck already has its own DSP, the deck-keyed `_filter_states` dictionary the GDScript
version threaded around by name is gone entirely.

**Equivalence verified — bit-identical:** both implementations run on the same chunks
with independent filter state, each called exactly once per chunk so they advance in
lockstep:

```
chunks=524  frames=881,662  avg_frames=1682
max_abs_diff = 0.000000000000000
max_rel_diff = 0.000000000000000
```

**Performance — two different numbers, don't conflate them:**

| Measurement | GDScript | C++ | Note |
|---|---|---|---|
| Isolated per-call, 1682-frame chunks | 7208 µs | 26.6 µs | **270×** — true like-for-like |
| End-to-end avg frame time, playing | 18.39 ms | 16.68 ms | **≥1.7 ms/frame saved** |
| End-to-end FPS | 54.4 | 59.9 | C++ run is **vsync-capped**, so this is a lower bound |

> ⚠️ An earlier note in this audit claimed the GDScript version burned ~7.2 ms *per
> frame*. **That was wrong** — it extrapolated the per-call cost as if a 1682-frame
> chunk arrived every frame, which it does not in normal operation. The A/B frame-time
> measurement above is the number to trust.

**Stub updated:** `audio_dsp_stub.gd` gained a matching `calculate_chunk_rms()`. This
matters — the `.gdextension` only lists macOS libraries, so **mobile always runs the
stub**, and a missing method would have crashed the audio path. The stub returns zeros,
which is numerically correct there because its `get_next_chunk()` returns a zero-filled
buffer.

**Rebuild required:** `scons platform=macos target=template_debug`. Needs `essentia` and
`rubberband` from Homebrew. Only `bin/libaudio_dsp.macos.debug.dylib` was rebuilt — a
release dylib and any mobile builds still need producing separately.

---

### [x] 1.4 `print()` calls inside the audio `_process` path
**File:** `scripts/services/audio_manager.gd`

Transition-trigger and outro-preload logging. Guarded by one-shot flags so they didn't
spam, but `print` on Android is a synchronous logcat write.

**Fix applied:** both wrapped in `if OS.is_debug_build()`. Kept rather than deleted —
they're genuinely useful when debugging transition timing, and they now cost nothing in
an export build.

---

### [x] 1.5 Three independent per-frame playback-position polls
**Files:** `scripts/main.gd`, `scripts/ui/mini_player.gd`, `scripts/ui/sidebar.gd`

Each called `AudioManager.get_playback_position()` every frame. The sidebar additionally
ran a linear scan over the whole lyrics array per frame.

**Fix applied:** New `AudioManager.position_changed(position)` signal, emitted from the
existing `_process` at `POSITION_EMIT_HZ` (10 Hz) while playing, plus an immediate emit
from `scroll_track()` so seeks don't wait for the next tick. All three consumers lost
their `_process` entirely and now handle the signal.

At 10 Hz a full-width progress bar advances well under a pixel per update, so it still
reads as smooth.

**Verified with real playback** (all 466 tracks are cached locally, so a track could be
driven end-to-end):

```
emits=30 over 3.48s -> 8.6 Hz | playing=true
vertical_progress.value=5.32  (== last emitted position)
mini_player progress=5.00     (quantised by its own step=1.0 — pre-existing)
```

The 8.6 Hz vs 10 Hz gap is the accumulator resetting to `0.0` rather than subtracting
the interval, discarding the remainder each tick. Harmless for a UI refresh rate; change
to `_position_emit_accum -= 1.0 / POSITION_EMIT_HZ` if exact cadence ever matters.

**Not exercised:** the sidebar lyrics path needs a track with synced lyrics. Wiring is
identical to the two verified consumers.

**Left alone:** the lyrics linear scan. It is O(lines) per call at ~50 lines, now at
10 Hz instead of 60 — no longer on the hot path.

---

### [x] 1.6 `_draw()` had side effects and called `queue_redraw()` re-entrantly
**File:** `scripts/ui/vertical_progress.gd`

`_draw()` called `_check_and_fetch_peaks()`, which calls `queue_redraw()` internally.
The `_fetched_peaks_for_track` guard made it terminate, but any future edit clearing
that flag during draw would have produced a redraw storm.

**Fix applied:** Peak acquisition moved out of `_draw()` onto a `_peak_poll` Timer
(`PEAK_POLL_INTERVAL`, 0.5s). Same async-decode constraint as 1.1 — no completion
signal on the DSP — but the timer **stops itself** as soon as peaks are in hand or
there is no track, so it only ticks during the short window after a track loads.
`_invalidate_peaks()` restarts it on `loading_track` / `track_changed`.

**Also fixed:** `_smooth_peaks` rewritten with a prefix sum, O(n × window) → O(n).
Edge samples still average over a truncated window, matching the previous output
exactly.

---

## P2 — Rendering

### [x] 2.1 Sidebar's glass material had a stale hardcoded `container_size` — **bug**
**Files:** `scenes/main.tscn:59`, `scripts/ui/sidebar.gd`

> **Correction to the original audit.** I first described this as the sidebar *sharing*
> a material with the app frame. That was wrong, and the reality is worse.

`ShaderMaterial_ujy5h` — with `container_size` baked to `Vector2(1232, 672)` — is
assigned **only** to the Sidebar. `AppWindowFrame` has *no* material in the scene at
all; `_apply_background()` creates a **separate** `ShaderMaterial` at runtime, and that
is the one `main.gd`'s `resized` handler updates.

Net effect: **nothing ever updated the sidebar's material.** It rendered its rounded-box
SDF against 1232×672 forever while the node was actually 240px wide — a ~5× horizontal
error. Because `premium_glass` maps `p = UV * container_size`, the corner radius and
border thickness came out anisotropic (stretched into ellipses) rather than circular.

**Fix applied:** `_setup_glass_material()` in `sidebar.gd` duplicates the material to a
per-instance copy and drives `container_size` from the sidebar's own `resized` signal.
`duplicate()` also guards against the resource being shared if the scene is ever
instanced more than once.

**Verified** by rendering the app to PNG with and without the fix:

| | Result |
|---|---|
| Before | Oversized soft corner radius, mismatched with the app frame's corner; thick fuzzy border band |
| After | Crisp ~24px radius concentric with the frame corner; clean 1px border; top-right corner now rounds correctly |

Runtime parameter check confirmed the material tracks the real size (240×528) and is a
distinct instance from the frame's (1018×552).

---

### [ ] 2.2 **[verify]** `textureLod` blur may be a no-op under `gl_compatibility`
**File:** `assets/shaders/premium_glass.gdshader:73`

Blur is `textureLod(SCREEN_TEXTURE, SCREEN_UV, blur_amount)`. The project runs
`gl_compatibility` on both desktop and mobile. If the Compatibility backend doesn't
generate screen-texture mipmaps, `blur_amount` does nothing — you pay for the
screen-texture copy and get no blur.

**Action:** Confirm on-device first. If true, switch to a fixed-tap blur or accept a
flat glass tint on mobile.

**Attempted and inconclusive (2026-07-18):** rendering the app to PNG does *not* answer
this. The window is transparent (`viewport/transparent_background=true`), so nothing is
drawn behind the glass — `blur_sample.a` is 0 and the shader takes its documented
fallback path to the flat StyleBox tint. The blur branch is never exercised in a
captured frame. **Testing this needs the app over real desktop/app content**, i.e. a
screen capture of the running window rather than a viewport capture.

Useful technique for future visual checks: a temporary `_ready()` hook doing
`get_viewport().get_texture().get_image().save_png(...)` after ~90 frames gives a
reliable, scriptable render of the app's own UI — just not of anything behind it.

---

### [ ] 2.3 Two full-screen backbuffer copies per frame
`hint_screen_texture` forces a backbuffer copy per node using it. Two glass nodes =
two full-screen copies/frame. First thing to measure on a mid-range Android GPU.

---

### [x] 2.4 Dead shaders and stale files
**Deleted** (each verified unreferenced by *both* filename and UID before removal, and
each tracked in git, so all are recoverable via history):

- `shaders/frosted_glass.gdshader` — docstring described usage that no longer matched
  reality; actively misleading. This emptied `shaders/`.
- `assets/shaders/vapor_glass.gdshader` — superseded by `premium_glass.gdshader`
- `addons/shader/glass_panel.gdshader` — ditto
- `temp_check.gd` — a standalone `SceneTree` script for a one-off raw-TLS debug session

**Docs updated:** `ARCHITECTURE.md` still described `shaders/frosted_glass.gdshader` in
its structure tree and in a "Glass Blur (Phase 2)" section that no longer reflected the
implementation. Rewritten to describe `premium_glass.gdshader` as shipped, including the
per-node `container_size` requirement from 2.1 so that bug is harder to reintroduce.

**`addons/shader/` removed entirely** — `base_stylebox.tres` was also unreferenced
(only a self-reference to its own UID). `addons/gut` is untouched; it's the test
framework and is enabled in `project.godot`.

---

## P3 — Responsiveness & layout

### [x] 3.1 Layout mode was gated on *hardware*, not viewport
**File:** `autoloads/PlatformManager.gd`

`should_show_sidebar()` hard-returned `false` for mobile hardware, so a tablet or a
phone in landscape could never get the sidebar layout regardless of available width.

**Fix applied:** Decoupled two orthogonal concerns —
- **Layout mode** (sidebar vs tab bar) → driven purely by logical viewport width
- **Input mode** (touch targets, hover affordances) → driven by hardware via
  `is_touch_primary()`

A tablet in landscape is now "desktop layout + touch input", a combination the old
logic could not express.

**Critical detail — DPI:** breakpoints are now computed in *density-independent*
units via `get_density_scale()` / `get_logical_size()`. A 1080×2400 phone reports
1080 physical px of width; a naive width check puts a **portrait phone at the `lg`
breakpoint** and hands it the desktop sidebar. Dividing by DPI/160 puts it at ~411dp
→ `xs`, which is correct. Desktop stays at 1.0 (the OS already reports logical points,
and Godot's `content_scale_factor` covers HiDPI).

**Verified on desktop** (Retina Mac, `content_scale_factor` 1.5):

| Window | Logical | Breakpoint | Layout |
|--------|---------|------------|--------|
| 1600×900 | 1067dp | `md` | sidebar |
| 1000×800 | 667dp  | `sm` | tab bar |
| 700×900  | 467dp  | `xs` | tab bar |

**[verify] Not yet tested on-device.** Two things to confirm on real mobile hardware:
1. `DisplayServer.screen_get_dpi()` returns a sane value (the code guards against
   `<= 0` but not against a wrong-but-positive value).
2. The resulting dp width puts portrait phone at `xs`/`sm` and landscape at `md`+.

Note: headless mode reports a bogus dummy viewport size (226×253 regardless of
`--resolution`), so breakpoint behaviour **cannot** be tested headlessly. Use a real
windowed run.

---

### [x] 3.1b App was orientation-locked to portrait — blocked landscape entirely
**File:** `project.godot:45`

`window/handheld/orientation=1` is *Portrait, locked*. No amount of layout work would
have produced a landscape view on mobile, because the device could never rotate.

**Fix applied:** changed to `6` (Sensor) — free rotation, which is what makes the
3.1 landscape→sidebar behaviour reachable.

**[verify]** Confirm rotation feels right on-device; if free rotation is unwanted,
`4` (Sensor Landscape) or `5` (Sensor Portrait) constrain it to one axis.

---

### [x] 3.2 `viewport_resized` had no debounce
**File:** `autoloads/PlatformManager.gd`

Emitted on every `size_changed`. A desktop window drag fires tens of events/sec, each
triggering `tab_bar._position_pill()` and every `resized`-connected `queue_redraw`.

**Fix applied:** 100ms debounce (`RESIZE_DEBOUNCE_SEC`) via a restartable one-shot
Timer, so a continuous drag coalesces into a single emission once it settles.
`layout_changed` stays immediate — it's already self-rate-limiting (fires only on an
actual boundary crossing) and delaying a layout swap would read as a lurch.

---

### [ ] 3.3 No touch input handling anywhere
No `InputEventScreenTouch` / `InputEventScreenDrag` in the project. Everything relies
on Godot's mouse emulation. Taps work; drags are the risk.

**Test on-device specifically:**
- `vertical_progress` scrubbing
- playlist drag-to-reorder (`playlist_screen.gd:53`)

---

### [x] 3.4 No minimum touch-target sizing
**Files:** `autoloads/ThemeManager.gd`, `scripts/ui/sidebar.gd`,
`scripts/screens/library_screen.gd`, `scripts/screens/playlist_track_row.gd`,
`scripts/ui/playlist_popup.gd`

`TOUCH_TARGET_MIN := 44` already existed in `theme_data.gd` — but it was being used as a
base unit to scale *down* from. Sidebar nav buttons were `TOUCH_TARGET_MIN * 0.5` = **22
px**; popup rows `* 0.75` = 33 px. Sensible density for a mouse, unusable with a finger.

**Fix applied:** two helpers on `ThemeManager`, keyed off `is_touch_primary()` — i.e.
input hardware, not layout width, so a tablet in landscape gets the desktop sidebar
*and* finger-sized targets:

- `min_touch_height(compact)` — one axis, for full-width rows
- `min_touch_size(compact: Vector2)` — both axes, for icon buttons (a 44×24 target is
  still 24 px to a fingertip)

Applied at: sidebar nav buttons, sidebar playlist header (incl. the bare `+` glyph,
which was 17 px wide), sidebar transport tiles (32 px), library rows (28 px — the app's
densest tap surface), playlist track remove button (24×24, and destructive), playlist
popup rows.

**Verified** by walking the live scene tree and measuring every visible `Button`:

| Mode | Buttons checked | Under 44 px |
|---|---|---|
| Touch | 103 | **0** |
| Pointer | 103 | 103 *(unchanged — desktop density untouched)* |

Screenshots in forced-touch mode confirm no overflow or clipping in either the nav list
or the transport tiles.

**Deliberately left alone:** the mini-player's narrow mode packs 8 buttons at 38 px
wide. Height is already `TOUCH_TARGET_MIN`; forcing width to 44 would total 352 px and
overflow the narrowest phones. Width-only shortfall on a horizontal row is the right
trade. The tab bar was already compliant at `NAV_BAR_HEIGHT_MOBILE` (56 px).

**New debug affordance:** `VAPOR_FORCE_TOUCH=1` forces `is_touch_primary()` true on a
desktop run, so touch layout can be inspected without a device. Pair with a narrow
`--resolution`. Remove it if you'd rather not carry a debug env var.

---

## P4 — Library list

### [x] 4.1 `_rebuild_tree()` built the entire library eagerly
**File:** `scripts/screens/library_screen.gd`

For every artist → album → track it instantiated a scene, created containers, and wired
**three lambdas per track row** (`pressed`, `mouse_entered`, `mouse_exited`). Albums and
tracks start `visible = false`, so they never rendered — but they were fully
constructed, in-tree, and resident in memory.

**Fix applied:** Split into a pure data model (`_build_data_model()`) plus lazy node
builders. Only artist rows are built up front; `_build_albums()` runs on first artist
expand, `_build_tracks()` on first album expand.

**Measured on the real 466-track / 25-artist library:**

| | Nodes in tree | Rows built |
|---|---|---|
| Eager (old) | 6,691 | 1,518 |
| Lazy (new, at load) | 301 | 25 |

A **22× reduction** in nodes at load. Initial build measured at 15.3 ms. Scaling to the
5,000-track case in the original estimate, the old path would have built ~70,000 nodes
and ~15,000 connections in one frame.

**Implementation note:** expansion state is stored via `set_meta("built", …)` on the
container, *not* a captured local — GDScript lambdas capture locals **by value**, so a
captured `bool` would never observe the mutation and every expand would rebuild.

**Remaining:** individual albums are still built whole. If any single album is large
enough to hitch, virtualize the track list (not currently a problem at this library size).

---

### [x] 4.2 Theme change rebuilt the entire library
`_rebuild_tree()` was called from `_apply_styles`, so switching themes tore down and
rebuilt every node — and silently discarded whatever the user had expanded.

**Fix applied:** `_apply_styles()` now calls `_restyle_rows()`, which reapplies theme
tokens in place over a registry of `{label, role}` entries built by `_make_row_button`.
Roles (`ROLE_ARTIST` / `ROLE_ALBUM` / `ROLE_TRACK`) map to tokens via `_style_for_role()`,
which is also the single source the builders use — so there is one definition of what an
artist row looks like, not two that can drift.

The hover lambdas already read `ThemeManager.current_theme` at call time, so they remain
correct across theme changes with no reconnection.

---

## P5 — I/O & services

### [x] 5.1 `HTTPRequest` node churn, no request coalescing
**File:** `scripts/services/metadata_service.gd`

`_make_http_request` and `_download_image` each created and freed an `HTTPRequest` node
per call, with no in-flight dedup — two concurrent requests for the same artist image
issued two identical fetches.

**Fix applied:**

1. **Node pool** — `_acquire_http()` / `_release_http()`, capped at `HTTP_POOL_MAX` (4)
   so a burst doesn't leave a large pool resident. Pool is freed in `_exit_tree()`.
2. **In-flight dedup** — a `_Pending` RefCounted with a `done(result)` signal, keyed by
   URL in `_inflight_http` / `_inflight_images`. First caller does the work; later
   callers await the signal.
3. **Bonus: killed a busy-wait.** `lookup_metadata()` deduped at track level via
   `while _pending_lookups.has(href): await process_frame` — a spin waking ~60×/sec to
   do nothing. Now awaits the same `_Pending` signal.

**Two ordering hazards, both handled:**

- The in-flight entry is erased **before** `done` is emitted. Otherwise a waiter that
  immediately re-requests the same URL would await a `_Pending` that had already fired
  and would never fire again — a permanent hang.
- `download_file` is cleared on both acquire and release. A stale value would silently
  redirect the next caller's response body into a file.

**Verified.** Machinery exercised against a dead local port (no external traffic):

| Scenario | Requests started | Nodes created | Pool after |
|---|---|---|---|
| 6 concurrent, **same** URL | **1** | 1 | 1 |
| 5 sequential, distinct URLs | 5 | **0** (all reused) | 1 |
| 8 concurrent, distinct URLs | 8 | 7 | **4** (capped) |

Leftover `HTTPRequest` children: 4 (exactly the pool). In-flight entries: 0 — no leaks.

Success path confirmed separately with 3 concurrent real lookups against the Deezer
endpoint the app already calls: `started=1, got=3, all_identical=true, len=857` — a
single network request, with every coalesced waiter receiving the real body. The
dead-port cases all return `""`, so they could not have shown this.

---

### [ ] 5.2 `WebDAVService` scanning is main-thread cooperative
**File:** `scripts/services/webdav_service.gd:129, 149, 248, 301`

Yields with `await Engine.get_main_loop().process_frame` between polls. Doesn't freeze
the UI, but caps network progress at one poll per rendered frame — a slow scan is
bottlenecked by framerate, not the network.

**Fix:** Move to a real `Thread`. `AudioAnalyzer` already demonstrates the pattern
(`Thread` + `Mutex`).

---

### ✅ Positive note
`MetadataService`'s cache layer is the well-built one — debounced 2s save timer, flush
on `_exit_tree`, cache pruning. It's the pattern the other services should follow.

---

## Progress

**Done (2026-07-18):**
- 1.1 `vibe_screen._process` — peaks/metadata moved off the frame path
- 1.2 `settings_screen._process` — diagnostics on a 0.25s timer
- 3.1 Layout decoupled from hardware; density-independent breakpoints
- 3.1b Portrait orientation lock removed
- 3.2 `viewport_resized` debounced
- 4.1 Library tree lazy-built — 6,691 → 301 nodes at load
- 4.2 Theme change restyles in place instead of rebuilding
- 2.1 Sidebar glass material made per-instance and size-tracking
- 1.5 Position polls consolidated onto one 10 Hz signal
- 1.6 Peak fetching moved out of `_draw()`; `_smooth_peaks` now O(n)
- 2.4 Dead shaders and `temp_check.gd` deleted; `ARCHITECTURE.md` corrected

**`_process` loop count: 6 → 4.** The four that remain are all justified:

| File | Why it stays |
|---|---|
| `audio_manager.gd` | Buffer feeding genuinely must run per frame |
| `vibe_screen.gd` | Playhead only, behind `is_visible_in_tree()` |
| `settings_screen.gd` | Spinner rotation only, behind visibility guard |
| `vertical_progress.gd` | Loading animation only; `set_process(is_loading)` gates it |

- 1.3 `_calculate_chunk_rms` ported to C++ — bit-identical, ≥1.7 ms/frame saved
- 1.4 Audio-path `print()` calls gated behind `OS.is_debug_build()`
- 3.4 Touch targets enforced via `ThemeManager.min_touch_height/size()`
- 5.1 `HTTPRequest` pooling + URL dedup; `lookup_metadata` busy-wait removed

**Next, in payoff order:**
1. `WebDAVService` onto a real thread (5.2) — last structural item
2. Verify the `textureLod` blur (2.2) — needs a real desktop-composite capture

**Now the likely top bottleneck:** with RMS gone, the C++ path sits pinned at vsync
(16.68 ms) on an M1. The next thing worth profiling is rendering — specifically the two
`hint_screen_texture` backbuffer copies per frame (2.3), which is also where the
unresolved blur question (2.2) lives.

---

## Verification status

| Change | Verified how |
|--------|--------------|
| All edits parse | `--headless --editor` full project import, no script errors |
| App boots | Real windowed run, 466-track library loads clean |
| Breakpoint math | Real windowed run at 3 resolutions (see 3.1 table) |
| Library lazy build | ✅ Driven end-to-end: expand artist → albums appear; expand album → 12 tracks appear; collapse+re-expand does not duplicate; theme change leaves node count and Label instances untouched and preserves expansion |
| Sidebar glass fix | ✅ Runtime param check + before/after PNG render of the corner region |
| Position signal (1.5) | ✅ Real playback driven end-to-end: 8.6 Hz measured, both progress bars tracking |
| Dead file removal (2.4) | ✅ Name + UID reference search before deleting; clean editor import after |
| Peak poll rework (1.6) | ⚠️ Parses and boots; the stop-condition was not observed under a real decode |
| C++ RMS port (1.3) | ✅ Bit-identical over 881,662 frames; A/B frame-time measured under real playback |
| DSP stub parity (1.3) | ⚠️ Method added and correct by inspection; **not run on-device** — mobile always uses the stub |
| Touch targets (3.4) | ✅ Live scene-tree measurement of all 103 buttons in both modes + forced-touch screenshots |
| HTTP pool + dedup (5.1) | ✅ Dead-port matrix for the machinery, plus a real 3-way coalesced request for the success path |
| Mobile density path | ❌ **Not tested** — needs a device |
| Orientation change | ❌ **Not tested** — needs a device |
| Waveform refresh correctness | ⚠️ **Not exercised** — needs playback through a transition |

The vibe-screen rework is the one carrying real behavioural risk: the refactor was
verified to parse and boot, but the waveform/transition path was not driven end-to-end.
Watch the transition timeline during an actual track change and confirm both waveforms
populate and the trigger marker lands correctly.

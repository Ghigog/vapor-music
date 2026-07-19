# Editor Recovery Notes — 2026-07-19

**RESOLVED 2026-07-19 (afternoon). Both editor problems are fixed. The editor and the
game both work on Godot 4.7.1.** The history below is kept for reference; the section
immediately following is what matters.

---

## The two root causes and their fixes

### 1. Editor startup crash (SIGABRT) — Godot engine bug, macOS 26
`EditorHelp`'s background doc-regeneration thread can flush a queued native-menu
update off the main thread; AppKit on macOS 26 makes that fatal
(`NSMenu removeItemAtIndex` / `NSInternalInconsistencyException`). Present in 4.6.1
AND 4.7.1 (crash report `Godot-2026-07-19-121734.ips` confirms on 4.7.1).

**Fix (applied):** editor setting
`interface/editor/appearance/use_embedded_menu = true`
(in `~/Library/Application Support/Godot/editor_settings-4.7.tres`).
The editor draws its menu inside the window; the native macOS menu is never managed
by Godot, so the crash is structurally unreachable. Verified: repeated boots, zero
crashes since (previously ~80% crash rate).

**Trade-off:** no macOS global menu bar for the editor. Revert the setting if a Godot
release fixes the threading bug upstream.

### 2. Editor hang on splash / black window — zero-size shadow atlas
`project.godot` contained:
```
lights_and_shadows/directional_shadow/size=0        (+ .mobile)
lights_and_shadows/positional_shadow/size=0         (+ .mobile)
```
A shadow atlas of size 0 hangs the editor's GL 3D viewport on first render
(gl_compatibility, macOS 26, Apple M1) — the boot deadlocks at the project splash
with all threads parked. Found by a 10-round bisection of project settings; adding
only these lines to an otherwise-booting config reproduces the hang deterministically.
Further bisection on a blank project: `directional_shadow/size=0` alone triggers the
hang; `positional_shadow/size=0` alone does not.

The GAME was never affected: it is 2D-only, so no 3D viewport (and therefore no
shadow atlas) is ever created. The editor always creates one.

**Fix (applied):** the four lines are deleted from `project.godot`. Cost: none — the
app has no 3D content and no shadowed 2D lights. (If shadow-atlas memory savings were
the goal, do NOT reintroduce with 0; a small nonzero size or leaving defaults is safe.)

**Everything else was exonerated and restored unchanged:** window transparency /
borderless settings, boot splash, autoloads, GUT plugin, tests/, the AudioDSP
GDExtension, audio latency, display/stretch settings, physics and plugin prefs.

### Upstream reports worth filing (minimal repros available)
1. Menu-race crash: `.ips` file + "external script edits, then open project" repro.
2. Shadow-size-0 editor hang: blank project + the four lines above reproduces it.

### Residual state notes
- Script-editor open-tab session was cleared during recovery (tabs won't restore
  once; scenes/scripts themselves are untouched — reopen from the FileSystem dock).
- `.godot` caches were rebuilt several times; a pre-recovery copy of the old cache
  sits in the session scratchpad and can be deleted.
- The perf-audit working-tree changes remain uncommitted — commit them.

---

# Original investigation notes (historical)


**Status: the GAME is fully working and verified. The EDITOR is broken on Godot 4.6.1
by what is almost certainly an engine bug. Recommended action: update to 4.7.1-stable
(the update banner is showing in the Project Manager).**

---

## The three separate problems (they compounded confusingly)

### 1. A transient parse error (mine — fixed within minutes, but caused the first scare)
During perf-audit verification on Jul 18, a temporary test harness in `main.gd`
briefly had a parse error on disk (~15:11–15:12). A `main.gd` that fails to compile
boots the root scene with no controller — and because this app's window is borderless
and transparent by design, that renders as "splash, then black, flashing on mouse
move." Any launch in that window hit it. Fixed the same hour; the current tree parses
clean and the game runs.

### 2. Editor startup crash — engine bug in 4.6.1 (`SIGABRT`, exit 134)
```
NSInternalInconsistencyException: modification of a menu's items on a
non-main thread when the menu is part of the main menu
  NativeMenuMacOS::remove_item ← PopupMenu::clear
  ← EditorDockManager::update_docks_menu   (queued, flushed on wrong thread)
  ← CallQueue::flush ← ResourceLoader::_run_load_task
  ← EditorHelp::_reload_scripts_documentation  (background doc-regen thread)
```
The editor regenerates script documentation on a background thread whenever script
files changed since the last session. On macOS that thread can end up flushing a
queued native-menu update, which AppKit forbids off the main thread → abort.

- It is a **race**: some boots crash, some survive. Boot workload (reimports, doc
  regen backlog, background CPU load) shifts the odds.
- The Jul 18 perf-audit session edited ~12 scripts externally, arming a large doc
  regen — which is why it surfaced that afternoon.
- **Nothing project-side eliminates it.** Ruled out empirically: GUT plugin disabled,
  GDExtension unregistered, recovery mode (`--recovery-mode`), fresh `.godot`,
  restored `.godot`, doc cache rebuilt, Metal renderer (`--rendering-method
  forward_plus`), Rosetta. Rosetta and no-extension runs survived more often (timing),
  but the crash reproduced in almost every configuration eventually.
- The doc cache was rebuilt and saved (`.godot/editor/editor_script_doc_cache.res`,
  Jul 19) via a surviving extensionless session closed gracefully — this shrinks the
  regen backlog and improves (not fixes) boot odds on 4.6.1.

### 3. Editor renders black when it does survive — unresolved on 4.6.1
When a boot survives the race, the vapor-music editor window paints solid black
(alive: menus work, settings save). A minimal probe project renders fine on the same
machine under BOTH renderers — so it's an interaction between this project and the
4.6.1 editor, not a broken Godot install or GPU issue.

Ruled out: project `.godot` caches (full reset + reimport), window transparency /
borderless settings (flipped off — still black), open-scene restoration incl. the
screen-reading glass shader (no scenes open — still black), GUT plugin, the
GDExtension, editor settings file, saved window geometry, editor layout.

Not ruled out: the 4.6.1 editor itself. **Test on 4.7.1 before digging further.**

---

## Why "update to 4.7.1" is the recommendation
- The crash is in editor/engine threading code that only upstream can fix; 4.7.1 is
  two minor releases ahead.
- The black-window behaviour is editor-side and untested on 4.7.1.
- The Project Manager (left open) shows "Update available: 4.7.1-stable".
- If the black window persists on 4.7.1, the probe-project comparison in these notes
  is the starting point for a clean upstream bug report.

## If a boot crashes after updating (or before)
Just relaunch — it's a race. Odds improve when: no big reimport is pending, script
docs are settled (a previous session exited cleanly), and heavy background apps
(League was running throughout this session) are closed.

---

## Perturbation inventory (everything touched during recovery, and its end state)

| Thing | End state |
|---|---|
| `.godot/` (project cache) | Reset then fully regenerated; pre-reset copy preserved at the session scratchpad (`godot_cache_backup/`) |
| `.godot/editor/editor_script_doc_cache.res` | Rebuilt fresh (Jul 19) — the old one (from the broken 15:20 session) was detected corrupt and deleted by the editor |
| `.godot/editor/project_metadata.cfg` | `scenes=[...]` (8 reopened scenes) cleared to `[]` during testing — the editor will simply open with no scene; reopen from FileSystem dock |
| `bin/audio_dsp.gdextension` | Temporarily unregistered during testing — **restored** |
| `project.godot` window settings (`borderless`, `transparent`, `per_pixel_transparency`, `viewport/transparent_background`) | Temporarily flipped off during testing; a crashed editor session then pruned them from the file (they matched engine defaults) — **restored to original values** |
| GUT plugin | Temporarily disabled during testing — **restored** |
| Editor settings (`editor_settings-4.6.tres`) | Untouched |
| League / Riot processes | Untouched |
| Scratchpad probe project | Temporary, outside the repo |

**Verified after restoration:** the game launches and renders correctly — glass
transparency, full 563-track library, GDExtension loaded (screenshot-verified
Jul 19 ~12:14).

## Unrelated stderr noise (pre-existing, harmless, seen in every log)
- `Unicode parsing error ... NUL character` — the editor scanning binary files at the
  repo root (`.sconsign.dblite`, `.aider.tags.cache.v4/`). Consider `.gdignore`-ing
  or relocating them.
- `invalid UID: uid://0n0u5at313dh` in `playlist_screen.tscn` — stale script UID,
  falls back to path; re-save the scene in the editor to clear.
- `Could not find version of build tools that matches Target SDK, using 36.1.0` —
  Android export tooling, unrelated.

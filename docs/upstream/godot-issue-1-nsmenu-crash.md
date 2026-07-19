# Draft for github.com/godotengine/godot/issues/new — issue 1 of 2

> Post title suggestion:
> **macOS 26: Editor crashes at startup — NSMenu modified on non-main thread during script doc regeneration (`NativeMenuMacOS::remove_item`)**
>
> Before posting: attach the crash report
> `~/Library/Logs/DiagnosticReports/Godot-2026-07-19-121734.ips`
> (there are several from the same day; any of them shows the identical stack).

---

### Tested versions

- Reproducible in: v4.6.1.stable.official [14d19694e], v4.7.1.stable.official — both crash with the identical stack.

### System information

macOS 26.5.1 (25F80) — Apple M1 — Compatibility renderer (OpenGL API 4.1 Metal - 90.5)

### Issue description

Opening a project in the editor frequently aborts during startup with:

```
*** Terminating app due to uncaught exception 'NSInternalInconsistencyException',
reason: 'API misuse: modification of a menu's items on a non-main thread when the
menu is part of the main menu. Main menu contents may only be modified from the
main thread.'
```

Relevant frames (from the 4.6.1 symbolicated stack; 4.7.1 produces the same shape):

```
4  Godot  NativeMenuMacOS::remove_item(RID const&, int)
5  Godot  PopupMenu::clear(bool)
6  Godot  EditorDockManager::update_docks_menu()
7  Godot  CallQueue::_call_function(Callable const&, Variant const*, int, bool)
8  Godot  CallQueue::flush()
9  Godot  ResourceLoader::_run_load_task(void*)
10 Godot  ResourceLoader::_load_start(...)
11 Godot  ResourceLoader::load(...)
12 Godot  EditorHelp::_reload_scripts_documentation(EditorFileSystemDirectory*)
13 Godot  EditorHelp::_reload_scripts_documentation(EditorFileSystemDirectory*)
15 Godot  EditorHelp::_regen_script_doc_thread(void*)
```

A queued `EditorDockManager::update_docks_menu` deferred call is flushed by
`CallQueue::flush()` running on the ResourceLoader task thread (inside the
script-documentation regeneration worker), which reaches
`NativeMenuMacOS::remove_item` off the main thread. AppKit on macOS 26 appears to
treat this as a fatal assertion — the same project and editor versions reportedly
worked on this machine before the macOS 26 update, so a previously tolerated
threading pattern may have become fatal at the OS level.

### Frequency / conditions

It is a startup race, gated on the script-doc regeneration thread having real work:

- With a large stale set (many `.gd` files modified outside the editor since the
  last session, or a missing/corrupt `editor_script_doc_cache.res`): crashes on
  roughly 4 out of 5 launches (observed across ~25 launches).
- With a current doc cache: rarely/never crashes.
- Also reproduced in `--recovery-mode` (the doc-regen thread still runs there).
- Not reproduced under Rosetta (x86_64 slice) in several attempts — consistent
  with a timing race.

### Steps to reproduce

1. On macOS 26, take a project with several dozen GDScript files.
2. Modify a dozen `.gd` files *outside* the editor (or delete
   `.godot/editor/editor_script_doc_cache.res`) so script docs are stale.
3. Open the project in the editor. Repeat a few times if the first launch survives.

### Workaround

`EditorSettings → interface/editor/appearance/use_embedded_menu = true`
(4.7.x; the setting does not exist in 4.6.x). With the embedded menu the native
menu bar is never managed by Godot and the crash becomes unreachable — 100% of
launches survive with this set, on the same project state that crashed 80% of the
time with native menus.

### Minimal reproduction project

None attached — the trigger is "stale script docs in a project with many scripts,"
which any medium-sized GDScript project reproduces after step 2 above. Can attach
one on request.

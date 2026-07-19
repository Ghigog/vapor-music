> POSTED 2026-07-19: https://github.com/godotengine/godot/issues/115745#issuecomment-5017577491

# Comment to post on github.com/godotengine/godot/issues/115745
# (instead of filing a new issue — this is the same bug, closed as not planned)
# Attach ~/Downloads/godot-4.7.1-nsmenu-crash.ips.txt by dragging it into the
# comment box. Everything below the line is the comment body.

---

Requesting a reopen — I can add several data points to this, including a much
lower reproduction threshold, confirmation on current stable, and a workaround.

**Still present in v4.7.1.stable.official**, on macOS 26.5.1 (25F80), Apple M1,
Compatibility renderer. Identical stack to the OP (full `.ips` from 4.7.1
attached):

```
4  Godot  NativeMenuMacOS::remove_item(RID const&, int)
5  Godot  PopupMenu::clear(bool)
6  Godot  EditorDockManager::update_docks_menu()
7  Godot  CallQueue::_call_function(...)
8  Godot  CallQueue::flush()
9  Godot  ResourceLoader::_run_load_task(void*)
...
12 Godot  EditorHelp::_reload_scripts_documentation(EditorFileSystemDirectory*)
15 Godot  EditorHelp::_regen_script_doc_thread(void*)
```

**The 2000-file threshold isn't necessary.** Our project has ~40 GDScript files
and crashed on roughly 4 of 5 launches. What matters is the script-doc
regeneration thread having work at startup: modify a handful of `.gd` files
*outside* the editor (or delete `.godot/editor/editor_script_doc_cache.res`),
then open the project. With a current doc cache the crash goes quiet again —
which may be why it appears intermittent or machine-dependent.

**macOS 26 appears to make this fatal where it previously wasn't.** The same
project + editor combination worked on this machine for months and began
crashing on entry immediately after the macOS 26 update — consistent with
AppKit tightening the main-thread assertion from tolerated to terminating. If
so, every macOS 26 user with a stale doc cache is exposed, which is a stronger
severity than when this was triaged.

**Workaround that actually eliminates it** (the ones tried in this thread were
ineffective for us too): editor setting
`interface/editor/appearance/use_embedded_menu = true` (4.7.x). With the menu
embedded, `NativeMenuMacOS` never manages the editor menus and the crashing
path is unreachable — 0 crashes across dozens of launches on a project state
that crashed ~80% of the time with the native menu.

Also for triage: reproduced in `--recovery-mode` (the doc-regen thread still
runs there); *not* reproduced under Rosetta in several attempts, consistent
with a scheduling-dependent race.

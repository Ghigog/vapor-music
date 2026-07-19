> POSTED 2026-07-19: https://github.com/godotengine/godot/issues/121559

# Draft for github.com/godotengine/godot/issues/new — issue 2 of 2

> Post title suggestion:
> **macOS + Compatibility renderer: editor hangs forever at project splash when `rendering/lights_and_shadows/directional_shadow/size=0`**

---

### Tested versions

- Reproducible in: v4.6.1.stable.official [14d19694e], v4.7.1.stable.official

### System information

macOS 26.5.1 (25F80) — Apple M1 — Compatibility renderer (OpenGL API 4.1 Metal - 90.5)

### Issue description

If a project sets the directional shadow atlas size to `0`, opening that project in
the editor hangs **forever** on the boot splash. The process stays alive (the
window's traffic lights and the app menu respond; editor settings even save on
quit), but the editor UI never appears. There is no crash, no error, and nothing in
`--verbose` output after the normal startup lines — `--verbose` shows resource
loads completing normally and then simply stops.

Sampling the hung process shows the main thread idling in the run loop's frame
pacing (nanosleep) and every worker thread parked in condition-variable waits —
i.e. a deadlock/stall rather than a busy loop.

Notes:

- `positional_shadow/size=0` alone does **not** reproduce — the editor opens
  normally. The trigger is specifically `directional_shadow/size=0`.
- **Running a 2D game** with this setting is unaffected (no 3D viewport is ever
  created, so no directional shadow atlas is allocated). Only the editor — which
  always creates a 3D editor viewport — hangs. This makes the setting a trap: a 2D
  project can ship and run fine with it while the editor silently stops opening.
- Real-world impact: this took down the editor for our project on both 4.6.1 and
  4.7.1 and was only found by bisecting `project.godot` line by line, because the
  hang presents as a black/stuck splash with zero diagnostics.

Expected behaviour: either clamp/validate the atlas size (minimum 1 texel or
"disabled" semantics), or fail with an error — anything but an undiagnosable
infinite hang.

### Steps to reproduce (100% deterministic on the affected machine)

1. Create a new empty project.
2. Add to `project.godot`:

```ini
[rendering]

renderer/rendering_method="gl_compatibility"
renderer/rendering_method.mobile="gl_compatibility"
lights_and_shadows/directional_shadow/size=0
lights_and_shadows/directional_shadow/size.mobile=0
```

3. Open the project in the editor → it hangs at the splash screen indefinitely.
4. Remove the two `directional_shadow` lines → the editor opens normally.

### Minimal reproduction project

The four-line `project.godot` above is the entire reproduction; can attach a zip
on request.

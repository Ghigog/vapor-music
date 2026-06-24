## Implementation Plan for ticket DJ-019

Implement automated transition duration calculation in `AudioManager` matching track phrasing and segments. Clamps mix crossover times to a range of 4.0s to 16.0s depending on the outro/intro overlap. Removes manual transition duration controls from the vibe screen layout.

### Open questions
No open questions exist; requirements are fully specified.

### Proposed changes

- `[modify] scenes/screens/vibe/vibe_screen.tscn`
- `[modify] scripts/screens/vibe_screen.gd`
- `[modify] scripts/services/audio_manager.gd`
- `[modify] tests/unit/test_dj_transitions.gd`

### Verification Plan

#### Automated Tests
Run Godot headless test suite targeting transition scripts:
```bash
/Applications/Godot.app/Contents/MacOS/Godot --headless --path . -s addons/gut/gut_cmdln.gd -gdir=res://tests/unit -gselect=res://tests/unit/test_dj_transitions.gd
```

#### Manual Verification
- Run the application.
- Open Vibe Workbench screen.
- Verify that the Crossover Slider and its Label/Value elements are no longer visible under the MIX TUNER panel, leaving only the Vibe Limit slider.

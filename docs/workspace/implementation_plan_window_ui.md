## Implementation Plan for Window Management & Playback UI Redesign

### Open questions
None. The plan is aligned with the user feedback.

### Proposed changes
- [modify] `scripts/main.gd`
- [modify] `scenes/main.tscn`
- [modify] `scripts/ui/sidebar.gd`
- [modify] `scenes/ui/sidebar/sidebar.tscn`
- [modify] `scripts/ui/mini_player.gd`
- [modify] `scenes/ui/mini_player/mini_player.tscn`

### Verification Plan
- Run the unit tests suite with `/Applications/Godot.app/Contents/MacOS/Godot --headless --path . -s addons/gut/gut_cmdln.gd -gdir=res://tests/unit`
- Manually check window border resizing, sidebar dragging, and player control responsiveness on both desktop and mobile configurations.

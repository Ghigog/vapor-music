#ifndef MEDIA_CONTROLS_WINDOWS_H
#define MEDIA_CONTROLS_WINDOWS_H

#include <godot_cpp/classes/node.hpp>
#include <godot_cpp/variant/string.hpp>

namespace godot {

// Bridges Windows hardware media keys and the System Media Transport Controls
// (SMTC) — the Now Playing surface in the volume flyout / lock screen / media
// bar — to Godot, via the Win32 interop entry point for a classic desktop
// window (no UWP packaging required).
class MediaControls : public Node {
	GDCLASS(MediaControls, Node);

protected:
	static void _bind_methods();

public:
	MediaControls();
	~MediaControls();

	void register_controls();
	void unregister_controls();

	// artwork_path is accepted for API parity with the macOS bridge but is
	// currently unused — see media_controls_windows.cpp for why.
	void update_now_playing_info(String title, String artist, String album, double duration, String artwork_path);
	void update_playback_state(bool is_playing, double position, double duration);
	void clear_now_playing();

private:
	bool m_registered = false;
};

}

#endif // MEDIA_CONTROLS_WINDOWS_H

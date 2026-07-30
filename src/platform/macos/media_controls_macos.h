#ifndef MEDIA_CONTROLS_MACOS_H
#define MEDIA_CONTROLS_MACOS_H

#include <godot_cpp/classes/node.hpp>
#include <godot_cpp/variant/string.hpp>

namespace godot {

// Bridges macOS hardware media keys, Control Center's Now Playing widget, and
// Bluetooth/headphone remote buttons to Godot via MPRemoteCommandCenter and
// MPNowPlayingInfoCenter (MediaPlayer.framework). Available on macOS 10.12.2+.
class MediaControls : public Node {
	GDCLASS(MediaControls, Node);

protected:
	static void _bind_methods();

public:
	MediaControls();
	~MediaControls();

	void register_controls();
	void unregister_controls();

	// Called on track change — heavier (decodes artwork from disk).
	void update_now_playing_info(String title, String artist, String album, double duration, String artwork_path);

	// Called on play/pause/seek/position tick — cheap, no artwork decode.
	void update_playback_state(bool is_playing, double position, double duration);

	void clear_now_playing();

private:
	bool m_registered = false;
};

}

#endif // MEDIA_CONTROLS_MACOS_H

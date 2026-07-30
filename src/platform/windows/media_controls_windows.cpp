// NOTE: This file was written without a Windows toolchain available to
// compile or run it against (this repo is developed on macOS, and the
// project currently ships no Windows build of this GDExtension at all — see
// SConstruct). It follows Microsoft's documented pattern for wiring
// SystemMediaTransportControls to a classic Win32 desktop window
// ("GetForWindow" interop, MS Learn: "Integrate with the System Media
// Transport Controls"). Treat it as a first draft: build it on a Windows
// machine with a recent Windows SDK and expect to iron out include paths /
// lib names before it links.

#include "media_controls_windows.h"

#include <godot_cpp/classes/display_server.hpp>
#include <godot_cpp/core/class_db.hpp>

#include <windows.h>

#include <systemmediatransportcontrolsinterop.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Media.h>
#include <winrt/base.h>

using namespace godot;
using namespace winrt::Windows::Foundation;
using namespace winrt::Windows::Media;

namespace {

// SMTC is a per-window singleton on the Win32 interop path, and the button/
// seek callbacks arrive on a background WinRT dispatch thread — both are
// process-wide concerns, so they live at file scope rather than as instance
// members (mirrors how the callbacks have no `this` of their own to hang off).
SystemMediaTransportControls g_smtc{ nullptr };
winrt::event_token g_button_token;
winrt::event_token g_position_token;
MediaControls *g_owner = nullptr;

TimeSpan seconds_to_timespan(double p_seconds) {
	return std::chrono::duration_cast<TimeSpan>(std::chrono::duration<double>(p_seconds));
}

} // namespace

void MediaControls::_bind_methods() {
	ClassDB::bind_method(D_METHOD("register_controls"), &MediaControls::register_controls);
	ClassDB::bind_method(D_METHOD("unregister_controls"), &MediaControls::unregister_controls);
	ClassDB::bind_method(D_METHOD("update_now_playing_info", "title", "artist", "album", "duration", "artwork_path"), &MediaControls::update_now_playing_info);
	ClassDB::bind_method(D_METHOD("update_playback_state", "is_playing", "position", "duration"), &MediaControls::update_playback_state);
	ClassDB::bind_method(D_METHOD("clear_now_playing"), &MediaControls::clear_now_playing);

	ADD_SIGNAL(MethodInfo("play_requested"));
	ADD_SIGNAL(MethodInfo("pause_requested"));
	ADD_SIGNAL(MethodInfo("toggle_requested"));
	ADD_SIGNAL(MethodInfo("next_requested"));
	ADD_SIGNAL(MethodInfo("previous_requested"));
	ADD_SIGNAL(MethodInfo("seek_requested", PropertyInfo(Variant::FLOAT, "position")));
}

MediaControls::MediaControls() {
}

MediaControls::~MediaControls() {
	unregister_controls();
}

void MediaControls::register_controls() {
	if (m_registered) {
		return;
	}

	winrt::init_apartment(winrt::apartment_type::single_threaded);

	int64_t hwnd_handle = DisplayServer::get_singleton()->window_get_native_handle(DisplayServer::WINDOW_HANDLE, 0);
	HWND hwnd = reinterpret_cast<HWND>(hwnd_handle);
	if (hwnd == nullptr) {
		return;
	}

	auto interop = winrt::get_activation_factory<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>();
	winrt::com_ptr<ISystemMediaTransportControls> smtc_interface;
	if (FAILED(interop->GetForWindow(hwnd, winrt::guid_of<ISystemMediaTransportControls>(), smtc_interface.put_void()))) {
		return;
	}
	winrt::copy_from_abi(g_smtc, smtc_interface.get());

	g_owner = this;
	m_registered = true;

	g_smtc.IsPlayEnabled(true);
	g_smtc.IsPauseEnabled(true);
	g_smtc.IsNextEnabled(true);
	g_smtc.IsPreviousEnabled(true);
	g_smtc.IsEnabled(true);

	g_button_token = g_smtc.ButtonPressed([](SystemMediaTransportControls const &, SystemMediaTransportControlsButtonPressedEventArgs const &args) {
		if (g_owner == nullptr) {
			return;
		}
		switch (args.Button()) {
			case SystemMediaTransportControlsButton::Play:
				g_owner->call_deferred("emit_signal", "play_requested");
				break;
			case SystemMediaTransportControlsButton::Pause:
				g_owner->call_deferred("emit_signal", "pause_requested");
				break;
			case SystemMediaTransportControlsButton::Next:
				g_owner->call_deferred("emit_signal", "next_requested");
				break;
			case SystemMediaTransportControlsButton::Previous:
				g_owner->call_deferred("emit_signal", "previous_requested");
				break;
			default:
				break;
		}
	});

	g_position_token = g_smtc.PlaybackPositionChangeRequested([](SystemMediaTransportControls const &, PlaybackPositionChangeRequestedEventArgs const &args) {
		if (g_owner == nullptr) {
			return;
		}
		double seconds = std::chrono::duration<double>(args.RequestedPlaybackPosition()).count();
		g_owner->call_deferred("emit_signal", "seek_requested", seconds);
	});
}

void MediaControls::unregister_controls() {
	if (!m_registered) {
		return;
	}
	m_registered = false;
	g_owner = nullptr;

	if (g_smtc) {
		g_smtc.ButtonPressed(g_button_token);
		g_smtc.PlaybackPositionChangeRequested(g_position_token);
		g_smtc.IsEnabled(false);
		g_smtc.PlaybackStatus(MediaPlaybackStatus::Closed);
		g_smtc = nullptr;
	}
}

void MediaControls::update_now_playing_info(String title, String artist, String album, double duration, String artwork_path) {
	if (!g_smtc) {
		return;
	}

	// Artwork is intentionally skipped here: SMTC wants a
	// RandomAccessStreamReference (typically built from a file:// Uri or an
	// async StorageFile lookup), which needs proper URI escaping and/or a
	// coroutine round-trip that couldn't be verified without a Windows build
	// — left as a follow-up rather than shipped unverified.
	(void)artwork_path;

	auto updater = g_smtc.DisplayUpdater();
	updater.Type(MediaPlaybackType::Music);
	auto music_props = updater.MusicProperties();
	music_props.Title(winrt::to_hstring(title.utf8().get_data()));
	music_props.Artist(winrt::to_hstring(artist.utf8().get_data()));
	music_props.AlbumTitle(winrt::to_hstring(album.utf8().get_data()));
	updater.Update();

	SystemMediaTransportControlsTimelineProperties props;
	props.StartTime(seconds_to_timespan(0.0));
	props.MinSeekTime(seconds_to_timespan(0.0));
	props.MaxSeekTime(seconds_to_timespan(duration));
	props.EndTime(seconds_to_timespan(duration));
	g_smtc.UpdateTimelineProperties(props);
}

void MediaControls::update_playback_state(bool is_playing, double position, double duration) {
	if (!g_smtc) {
		return;
	}

	g_smtc.PlaybackStatus(is_playing ? MediaPlaybackStatus::Playing : MediaPlaybackStatus::Paused);

	SystemMediaTransportControlsTimelineProperties props;
	props.StartTime(seconds_to_timespan(0.0));
	props.MinSeekTime(seconds_to_timespan(0.0));
	props.Position(seconds_to_timespan(position));
	props.MaxSeekTime(seconds_to_timespan(duration));
	props.EndTime(seconds_to_timespan(duration));
	g_smtc.UpdateTimelineProperties(props);
}

void MediaControls::clear_now_playing() {
	if (!g_smtc) {
		return;
	}
	g_smtc.PlaybackStatus(MediaPlaybackStatus::Closed);
}

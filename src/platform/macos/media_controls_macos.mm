#include "media_controls_macos.h"

#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/project_settings.hpp>

#import <AppKit/AppKit.h>
#import <MediaPlayer/MediaPlayer.h>

using namespace godot;

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
	m_registered = true;

	MPRemoteCommandCenter *center = [MPRemoteCommandCenter sharedCommandCenter];

	// Raw pointer capture is safe here: these blocks live only as long as the
	// command targets registered below, and unregister_controls() (called from
	// the destructor) removes every target before this object is destroyed.
	MediaControls *self_ptr = this;

	center.playCommand.enabled = YES;
	[center.playCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
		self_ptr->call_deferred("emit_signal", "play_requested");
		return MPRemoteCommandHandlerStatusSuccess;
	}];

	center.pauseCommand.enabled = YES;
	[center.pauseCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
		self_ptr->call_deferred("emit_signal", "pause_requested");
		return MPRemoteCommandHandlerStatusSuccess;
	}];

	center.togglePlayPauseCommand.enabled = YES;
	[center.togglePlayPauseCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
		self_ptr->call_deferred("emit_signal", "toggle_requested");
		return MPRemoteCommandHandlerStatusSuccess;
	}];

	center.nextTrackCommand.enabled = YES;
	[center.nextTrackCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
		self_ptr->call_deferred("emit_signal", "next_requested");
		return MPRemoteCommandHandlerStatusSuccess;
	}];

	center.previousTrackCommand.enabled = YES;
	[center.previousTrackCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
		self_ptr->call_deferred("emit_signal", "previous_requested");
		return MPRemoteCommandHandlerStatusSuccess;
	}];

	center.changePlaybackPositionCommand.enabled = YES;
	[center.changePlaybackPositionCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
		if ([event isKindOfClass:[MPChangePlaybackPositionCommandEvent class]]) {
			double position = ((MPChangePlaybackPositionCommandEvent *)event).positionTime;
			self_ptr->call_deferred("emit_signal", "seek_requested", position);
		}
		return MPRemoteCommandHandlerStatusSuccess;
	}];

	// We don't support these — leave them disabled so Control Center doesn't
	// show affordances for actions that would silently no-op.
	center.stopCommand.enabled = NO;
	center.seekForwardCommand.enabled = NO;
	center.seekBackwardCommand.enabled = NO;
	center.skipForwardCommand.enabled = NO;
	center.skipBackwardCommand.enabled = NO;
	center.ratingCommand.enabled = NO;
	center.likeCommand.enabled = NO;
	center.dislikeCommand.enabled = NO;
}

void MediaControls::unregister_controls() {
	if (!m_registered) {
		return;
	}
	m_registered = false;

	MPRemoteCommandCenter *center = [MPRemoteCommandCenter sharedCommandCenter];
	[center.playCommand removeTarget:nil];
	[center.pauseCommand removeTarget:nil];
	[center.togglePlayPauseCommand removeTarget:nil];
	[center.nextTrackCommand removeTarget:nil];
	[center.previousTrackCommand removeTarget:nil];
	[center.changePlaybackPositionCommand removeTarget:nil];

	clear_now_playing();
}

void MediaControls::update_now_playing_info(String title, String artist, String album, double duration, String artwork_path) {
	MPNowPlayingInfoCenter *center = [MPNowPlayingInfoCenter defaultCenter];
	NSMutableDictionary *info = center.nowPlayingInfo ? [center.nowPlayingInfo mutableCopy] : [NSMutableDictionary dictionary];

	info[MPMediaItemPropertyTitle] = [NSString stringWithUTF8String:title.utf8().get_data()];
	info[MPMediaItemPropertyArtist] = [NSString stringWithUTF8String:artist.utf8().get_data()];
	info[MPMediaItemPropertyAlbumTitle] = [NSString stringWithUTF8String:album.utf8().get_data()];
	info[MPMediaItemPropertyPlaybackDuration] = @(duration);
	info[MPNowPlayingInfoPropertyDefaultPlaybackRate] = @(1.0);

	if (!artwork_path.is_empty()) {
		String abs_path = ProjectSettings::get_singleton()->globalize_path(artwork_path);
		NSString *ns_path = [NSString stringWithUTF8String:abs_path.utf8().get_data()];
		NSImage *image = [[NSImage alloc] initWithContentsOfFile:ns_path];
		if (image != nil) {
			MPMediaItemArtwork *artwork = [[MPMediaItemArtwork alloc] initWithBoundsSize:image.size
																		   requestHandler:^NSImage *_Nonnull(CGSize size) {
																			   return image;
																		   }];
			info[MPMediaItemPropertyArtwork] = artwork;
		}
	} else {
		[info removeObjectForKey:MPMediaItemPropertyArtwork];
	}

	center.nowPlayingInfo = info;
}

void MediaControls::update_playback_state(bool is_playing, double position, double duration) {
	MPNowPlayingInfoCenter *center = [MPNowPlayingInfoCenter defaultCenter];
	NSMutableDictionary *info = center.nowPlayingInfo ? [center.nowPlayingInfo mutableCopy] : [NSMutableDictionary dictionary];

	info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = @(position);
	info[MPMediaItemPropertyPlaybackDuration] = @(duration);
	info[MPNowPlayingInfoPropertyPlaybackRate] = @(is_playing ? 1.0 : 0.0);

	center.nowPlayingInfo = info;
	center.playbackState = is_playing ? MPNowPlayingPlaybackStatePlaying : MPNowPlayingPlaybackStatePaused;
}

void MediaControls::clear_now_playing() {
	MPNowPlayingInfoCenter *center = [MPNowPlayingInfoCenter defaultCenter];
	center.nowPlayingInfo = nil;
	center.playbackState = MPNowPlayingPlaybackStateStopped;
}

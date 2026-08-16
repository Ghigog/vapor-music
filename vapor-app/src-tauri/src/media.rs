//! System media controls — hardware keys, macOS Control Center, Windows SMTC,
//! Linux MPRIS.
//!
//! MIG-023. This is a parity regression rather than a gap: the Godot build
//! answers all of it through `MediaControlsManager.gd` plus a macOS `.mm` and a
//! Windows `.cpp`, and the Tauri shell answered none of it — **including on
//! macOS, where the old build works**.
//!
//! One crate covers all three desktop targets, which is why MIG-022 decided
//! *not* to port the 191 lines of C++/WinRT: three platform ports maintained
//! separately is what this replaces, not what it reimplements.
//!
//! ## What is deliberately thin
//!
//! This module owns no playback state. It is told what is happening and it
//! reports what a person pressed; the queue, the decks and the mixer are
//! elsewhere and stay there. Everything fallible is swallowed and logged —
//! a desktop session with no D-Bus, or a platform that refuses to register,
//! must cost the app nothing but its media keys.

use std::sync::{Arc, Mutex};

/// What a person asked for by pressing a key or a Control Center button.
///
/// The same six the GDScript emitted, minus `stop`: the shell's stop forgets
/// what was playing, and no hardware key means that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
}

/// What the system should be showing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub playing: bool,
    pub position: f64,
}

/// Translate a souvlaki event into a [`Press`], or `None` for one this app has
/// no answer for.
///
/// Split out and public so the mapping is testable: constructing a real
/// `MediaControls` needs a window handle and, on macOS, a run loop, so nothing
/// that goes through the platform can be reached from `cargo test`. The
/// mapping is the part with a decision in it.
pub fn press_of(event: &souvlaki::MediaControlEvent) -> Option<Press> {
    use souvlaki::MediaControlEvent as E;
    match event {
        E::Play => Some(Press::Play),
        E::Pause => Some(Press::Pause),
        E::Toggle => Some(Press::Toggle),
        E::Next => Some(Press::Next),
        E::Previous => Some(Press::Previous),
        // Stop is not Pause. The shell's stop forgets what was playing, and
        // answering a stop with a pause would leave the transport claiming a
        // track that the person has finished with.
        E::Stop => None,
        // Seek, SetPosition, OpenUri, Raise, Quit: no answer here yet. Seeking
        // from Control Center is worth having and needs a position the mixer
        // agrees with, which is more than a mapping.
        _ => None,
    }
}

/// Whether two states differ in a way worth telling the system about.
///
/// The supervisor ticks four times a second and the position moves every
/// tick, so sending on every change would be four D-Bus round trips a second
/// for a number nobody is reading. Position alone is not a reason to send;
/// anything else is.
pub fn worth_sending(previous: &NowPlaying, next: &NowPlaying) -> bool {
    previous.title != next.title
        || previous.artist != next.artist
        || previous.album != next.album
        || previous.playing != next.playing
        || (previous.duration - next.duration).abs() > 0.5
}

/// The system's media controls, if this platform gave us any.
pub struct Controls {
    inner: Mutex<Option<souvlaki::MediaControls>>,
    /// The last state sent, so an unchanged one is not sent again.
    last: Mutex<Option<NowPlaying>>,
}

impl Controls {
    /// Register with the platform, routing presses to `on_press`.
    ///
    /// Returns a `Controls` either way. A platform that will not register is
    /// not an error a person should see — it costs them their media keys and
    /// nothing else — so the failure is logged and this becomes a no-op that
    /// every caller can go on using unconditionally.
    pub fn attach<F>(window_handle: Option<*mut std::ffi::c_void>, on_press: F) -> Arc<Self>
    where
        F: Fn(Press) + Send + 'static,
    {
        let config = souvlaki::PlatformConfig {
            dbus_name: "vapor_music",
            display_name: "Vapor Music",
            // Windows needs a window to hang SMTC off; the other two ignore it.
            hwnd: window_handle,
        };

        let controls = match souvlaki::MediaControls::new(config) {
            Ok(mut controls) => {
                match controls.attach(move |event| {
                    if let Some(press) = press_of(&event) {
                        on_press(press);
                    }
                }) {
                    Ok(()) => Some(controls),
                    Err(e) => {
                        eprintln!("media controls: could not attach a handler ({e:?})");
                        None
                    }
                }
            }
            Err(e) => {
                eprintln!("media controls: this platform declined to register them ({e:?})");
                None
            }
        };

        Arc::new(Controls {
            inner: Mutex::new(controls),
            last: Mutex::new(None),
        })
    }

    /// Tell the system what is playing.
    ///
    /// Skipped when nothing meaningful has changed — see [`worth_sending`].
    pub fn publish(&self, now: &NowPlaying) {
        {
            let last = self.last.lock().ok();
            if let Some(last) = last {
                if last.as_ref().is_some_and(|p| !worth_sending(p, now)) {
                    return;
                }
            }
        }

        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        let Some(controls) = guard.as_mut() else {
            return;
        };

        let progress = Some(souvlaki::MediaPosition(std::time::Duration::from_secs_f64(
            now.position.max(0.0),
        )));
        let _ = controls.set_playback(if now.playing {
            souvlaki::MediaPlayback::Playing { progress }
        } else if now.title.is_empty() {
            souvlaki::MediaPlayback::Stopped
        } else {
            souvlaki::MediaPlayback::Paused { progress }
        });

        let _ = controls.set_metadata(souvlaki::MediaMetadata {
            title: Some(&now.title),
            artist: Some(&now.artist),
            album: Some(&now.album),
            duration: (now.duration > 0.0)
                .then(|| std::time::Duration::from_secs_f64(now.duration)),
            ..Default::default()
        });

        if let Ok(mut last) = self.last.lock() {
            *last = Some(now.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use souvlaki::MediaControlEvent as E;

    #[test]
    fn the_five_presses_this_app_answers_map_across() {
        assert_eq!(press_of(&E::Play), Some(Press::Play));
        assert_eq!(press_of(&E::Pause), Some(Press::Pause));
        assert_eq!(press_of(&E::Toggle), Some(Press::Toggle));
        assert_eq!(press_of(&E::Next), Some(Press::Next));
        assert_eq!(press_of(&E::Previous), Some(Press::Previous));
    }

    /// Stop is not Pause. The shell's stop forgets what was playing; answering
    /// a stop with a pause leaves the transport claiming a track the person
    /// has finished with.
    #[test]
    fn stop_is_not_answered_as_a_pause() {
        assert_eq!(press_of(&E::Stop), None);
    }

    #[test]
    fn an_event_with_no_answer_here_is_ignored_rather_than_guessed_at() {
        assert_eq!(press_of(&E::Raise), None);
        assert_eq!(press_of(&E::Quit), None);
        assert_eq!(
            press_of(&E::SetPosition(souvlaki::MediaPosition(
                std::time::Duration::from_secs(30)
            ))),
            None
        );
    }

    fn now(title: &str, playing: bool, position: f64) -> NowPlaying {
        NowPlaying {
            title: title.to_string(),
            artist: "An Artist".into(),
            album: "An Album".into(),
            duration: 240.0,
            playing,
            position,
        }
    }

    /// The supervisor ticks four times a second. Sending on every tick is four
    /// round trips a second for a number nobody is reading.
    #[test]
    fn a_moving_position_alone_is_not_worth_sending() {
        assert!(!worth_sending(
            &now("Windowlicker", true, 10.0),
            &now("Windowlicker", true, 10.25)
        ));
    }

    #[test]
    fn a_track_change_or_a_pause_is_worth_sending() {
        assert!(worth_sending(
            &now("Windowlicker", true, 10.0),
            &now("Xtal", true, 0.0)
        ));
        assert!(worth_sending(
            &now("Windowlicker", true, 10.0),
            &now("Windowlicker", false, 10.0)
        ));
    }

    /// A duration that arrives late — the header is read after playback starts
    /// — is a real change, and Control Center draws a scrubber from it.
    #[test]
    fn a_duration_arriving_late_is_worth_sending() {
        let mut later = now("Windowlicker", true, 1.0);
        later.duration = 0.0;

        assert!(worth_sending(&now("Windowlicker", true, 1.0), &later));
    }
}

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

impl Press {
    /// From the integer `PlaybackService.kt` sends.
    ///
    /// JNI has no enums, so the two sides agree on numbers. The constants are
    /// named in the Kotlin companion object and the order here is the contract
    /// — **inserting a variant above `Previous` silently remaps every button**.
    #[cfg(any(target_os = "android", feature = "android-check"))]
    pub fn from_i32(press: i32) -> Option<Self> {
        match press {
            0 => Some(Press::Play),
            1 => Some(Press::Pause),
            2 => Some(Press::Toggle),
            3 => Some(Press::Next),
            4 => Some(Press::Previous),
            _ => None,
        }
    }
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

/// How often the position is refreshed while a track plays.
///
/// Control Center draws a scrubber that advances on its own between updates,
/// so this is a correction rather than an animation — but without *any* update
/// the correction never happens, and a paused-then-resumed track leaves the
/// scrubber somewhere it is not.
pub const POSITION_REFRESH: f64 = 5.0;

/// Whether two states differ in a way worth telling the system about.
///
/// The supervisor ticks four times a second, so sending on every tick would be
/// four round trips a second for a number nobody is reading. But sending
/// *only* on a change leaves the scrubber frozen at wherever the track was
/// when it started, which is what the first version of this did: position was
/// excluded entirely, so after the title landed nothing was ever sent again.
///
/// So position counts, at a fifth of a Hz.
pub fn worth_sending(previous: &NowPlaying, next: &NowPlaying) -> bool {
    previous.title != next.title
        || previous.artist != next.artist
        || previous.album != next.album
        || previous.playing != next.playing
        || (previous.duration - next.duration).abs() > 0.5
        || (next.playing && (next.position - previous.position).abs() >= POSITION_REFRESH)
}

/// Whether this process is running from inside a `.app` bundle.
///
/// macOS routes hardware media keys to the *Now Playing* application, and an
/// application is something with a bundle identifier — which comes from being
/// inside `Foo.app/Contents/MacOS/`. A bare binary registers with
/// `MPRemoteCommandCenter` perfectly happily and then never hears from it.
///
/// Always true off macOS, where nothing about this applies.
pub fn bundled() -> bool {
    if !cfg!(target_os = "macos") {
        return true;
    }
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            let parent = path.parent()?.to_string_lossy().into_owned();
            Some(parent.ends_with(".app/Contents/MacOS"))
        })
        .unwrap_or(false)
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
        // `Sync` for Android's sake: the handler is kept in a process-wide
        // static there, because a JNI callback arrives on the JVM's thread with
        // nothing of ours to hang it off.
        F: Fn(Press) + Send + Sync + 'static,
    {
        /*
         * Android is not souvlaki's business.
         *
         * souvlaki covers the three desktops through D-Bus, SMTC and
         * MPNowPlayingInfoCenter; Android's equivalent is a `MediaSession`
         * owned by a foreground service, and the service is not optional
         * decoration — it is the only thing that stops the OS suspending the
         * process and starving the audio thread. `PlaybackService.kt` holds
         * that end, and this routes to it through the same seam every other
         * platform uses, so the supervisor calls one `publish` and does not
         * know which it got.
         */
        #[cfg(target_os = "android")]
        {
            crate::android::on_press(move |press| {
                if let Some(press) = Press::from_i32(press) {
                    on_press(press);
                }
            });
            return Arc::new(Controls {
                inner: Mutex::new(None),
                last: Mutex::new(None),
            });
        }

        #[cfg(not(target_os = "android"))]
        let config = souvlaki::PlatformConfig {
            dbus_name: "vapor_music",
            display_name: "Vapor Music",
            // Windows needs a window to hang SMTC off; the other two ignore it.
            hwnd: window_handle,
        };

        #[cfg(not(target_os = "android"))]
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

        #[cfg(not(target_os = "android"))]
        if !bundled() {
            // souvlaki's macOS backend returns `Ok` unconditionally — `new`
            // and `attach` cannot fail — so a successful registration says
            // nothing about whether the keys will actually arrive. This is the
            // condition that decides it, and without saying so out loud the
            // only symptom is a keyboard that does nothing.
            eprintln!(
                "media controls: this process is not inside a .app bundle, so macOS will not \
                 route media keys to it. `npm run app` (tauri dev) runs the bare binary; \
                 `npm run app:build` produces the bundle. Everything else works either way."
            );
        }

        #[cfg(not(target_os = "android"))]
        return Arc::new(Controls {
            inner: Mutex::new(controls),
            last: Mutex::new(None),
        });
    }

    /// Tell the system what is playing.
    ///
    /// Skipped when nothing meaningful has changed — see [`worth_sending`].
    ///
    /// **Called on the main thread**, via [`Controls::publish_from`]. The
    /// caller is the playback supervisor, which is a thread of its own, and
    /// `MPNowPlayingInfoCenter` and `MPRemoteCommandCenter` are AppKit-adjacent
    /// APIs — souvlaki does not marshal for you (it dispatches to a *global*
    /// queue, and only for artwork). Calling them off the main thread is
    /// undefined rather than merely discouraged, and the observable symptom is
    /// exactly what it was here: the controls register, report success, and
    /// then nothing on the keyboard reaches the app.
    pub fn publish(&self, now: &NowPlaying) {
        {
            let last = self.last.lock().ok();
            if let Some(last) = last {
                if last.as_ref().is_some_and(|p| !worth_sending(p, now)) {
                    return;
                }
            }
        }

        self.send(now);

        if let Ok(mut last) = self.last.lock() {
            *last = Some(now.clone());
        }
    }

    /// Hand the state to whichever platform is underneath.
    ///
    /// Split from `publish` so the "has this changed" question is asked once,
    /// in one place, rather than in each backend — and so the two bodies do not
    /// have to be threaded through each other with `cfg` on every other line.
    #[cfg(target_os = "android")]
    fn send(&self, now: &NowPlaying) {
        // Nothing playing and nothing paused: no reason to hold the process
        // alive, so the service and its wake lock go away.
        if now.title.is_empty() {
            crate::android::service_stop();
        } else {
            crate::android::service_update(
                &now.title,
                &now.artist,
                now.playing,
                now.position,
                now.duration,
            );
        }
    }

    #[cfg(not(target_os = "android"))]
    fn send(&self, now: &NowPlaying) {
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
    }

    /// Publish from any thread, by hopping to the main one.
    ///
    /// `run_on_main_thread` queues onto the platform's own event loop, which
    /// is where the media APIs expect to be called from. It returns
    /// immediately, so the supervisor's tick is not lengthened by a round trip
    /// through AppKit.
    pub fn publish_from(self: &Arc<Self>, handle: &tauri::AppHandle, now: NowPlaying) {
        // The cheap comparison happens here rather than on the main thread, so
        // an unchanged state costs a lock and not a dispatch.
        if let Ok(last) = self.last.lock() {
            if last.as_ref().is_some_and(|p| !worth_sending(p, &now)) {
                return;
            }
        }

        let controls = Arc::clone(self);
        let _ = handle.run_on_main_thread(move || controls.publish(&now));
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

    /// The frozen-scrubber bug. Position was excluded from `worth_sending`
    /// entirely, so once the title had landed nothing was ever sent again and
    /// Control Center showed a playhead stuck where the track started.
    #[test]
    fn a_playing_track_republishes_its_position_periodically() {
        let start = now("Windowlicker", true, 10.0);

        assert!(!worth_sending(&start, &now("Windowlicker", true, 12.0)));
        assert!(worth_sending(
            &start,
            &now("Windowlicker", true, 10.0 + POSITION_REFRESH)
        ));
    }

    /// A seek backwards is a jump the system has to be told about, and it is
    /// the case a one-directional comparison would miss.
    #[test]
    fn seeking_backwards_counts_as_a_move() {
        assert!(worth_sending(
            &now("Windowlicker", true, 90.0),
            &now("Windowlicker", true, 4.0)
        ));
    }

    /// Paused, the position is not advancing on its own, so there is nothing
    /// to correct and nothing to send.
    #[test]
    fn a_paused_track_does_not_republish_on_position_alone() {
        assert!(!worth_sending(
            &now("Windowlicker", false, 10.0),
            &now("Windowlicker", false, 40.0)
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

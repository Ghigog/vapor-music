//! Application settings.
//!
//! Port of the data model from `settings_manager.gd`. Reading and writing stay
//! in the shell, which matters more here than elsewhere: the GDScript stored
//! WebDAV credentials in a Godot `ConfigFile` encrypted with a key derived
//! in-process, which is obfuscation rather than security. The shell should put
//! them in the OS keychain instead — see MIG-031.
//!
//! Everything except the credentials is ordinary preference data and round
//! trips as JSON.

use serde::{Deserialize, Serialize};

fn default_folder() -> String {
    "Music".to_string()
}
fn default_font_size() -> u32 {
    16
}
fn default_ui_scale() -> f32 {
    1.2
}
fn default_theme() -> String {
    "default_dark".to_string()
}
fn default_cache_max_bytes() -> u64 {
    MAX_CACHE_BYTES_DEFAULT
}

/// Default local cache ceiling — generous on a desktop, finite anywhere.
pub const MAX_CACHE_BYTES_DEFAULT: u64 = 8 * 1024 * 1024 * 1024;

/// Smallest cache worth having.
///
/// A cache below roughly one album fetches a track, evicts it to make room for
/// the next, and fetches it again the moment it is wanted — strictly worse than
/// no cache, because it pays the download twice and reports itself as working.
pub const MIN_CACHE_BYTES: u64 = 256 * 1024 * 1024;

/// Where the library lives.
///
/// The password is deliberately **not** part of this struct. It belongs in the
/// platform keychain, and keeping it out means these settings can be logged,
/// serialised and diffed without leaking a credential.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default = "default_folder")]
    pub folder: String,
}

impl Default for RemoteConfig {
    /// Hand-written rather than derived: `#[serde(default)]` on the parent
    /// field calls `Default::default()`, not the per-field serde defaults, so a
    /// derived impl would silently give an empty folder instead of "Music".
    fn default() -> Self {
        RemoteConfig {
            url: String::new(),
            username: String::new(),
            folder: default_folder(),
        }
    }
}

impl RemoteConfig {
    /// Whether enough is configured to attempt a connection.
    ///
    /// The password is checked separately by the caller that holds it.
    pub fn is_configured(&self) -> bool {
        !self.url.trim().is_empty() && !self.username.trim().is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Preset,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    #[serde(default)]
    pub remote: RemoteConfig,

    #[serde(default = "default_font_size")]
    pub base_font_size: u32,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,

    #[serde(default)]
    pub theme_mode: ThemeMode,
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Custom theme colours as `#rrggbb`, used when `theme_mode` is `Custom`.
    #[serde(default)]
    pub custom_base_color: String,
    #[serde(default)]
    pub custom_accent_color: String,

    #[serde(default)]
    pub headphone_profile: String,
    #[serde(default)]
    pub headphone_calibration_enabled: bool,

    /// Manual BPM overrides, keyed by href.
    ///
    /// New in the port. Tempo detection agrees with Essentia on ~81% of a real
    /// library and the residual is metrical error, which was accepted rather
    /// than solved — so the app needs a way for a person to correct it. DJ
    /// software conventionally has one.
    #[serde(default)]
    pub bpm_overrides: std::collections::HashMap<String, f32>,

    /// Ceiling on the local audio cache, in bytes.
    ///
    /// The library lives in the user's cloud and local storage is only a cache,
    /// so this is the one number that decides how much of a device the app is
    /// entitled to. It belongs to the person, not to a constant.
    #[serde(default = "default_cache_max_bytes")]
    pub cache_max_bytes: u64,

    /// Whether the app may look up lyrics and artwork from public services.
    ///
    /// **Off by default, and this is the one setting whose default is a
    /// position rather than a preference.** Everything else the app knows
    /// about a track is worked out on the device from the audio itself; a
    /// lookup sends the artist and title of what someone is listening to to a
    /// third party, which is exactly the thing the rest of the design refuses
    /// to do. The Godot build fetched unconditionally. Asking first is the
    /// change, not the feature.
    #[serde(default)]
    pub metadata_lookup_enabled: bool,

    /// The Vibe Limit: the energy difference between consecutive tracks past
    /// which the pathfinder starts paying a steep penalty (§6 of
    /// `ai_dj_workflow.md`).
    ///
    /// Strict keeps a set at one intensity; loose permits drops and climbs.
    /// `transition_cost` has taken this as a parameter since the port and
    /// every caller passed the constant, so the Mix Tuner was a control the
    /// engine was already built for and nothing exposed.
    #[serde(default = "default_vibe_limit")]
    pub vibe_limit: f32,
}

fn default_vibe_limit() -> f32 {
    crate::track::DEFAULT_ENERGY_THRESHOLD
}

/// Strictest Vibe Limit worth offering.
///
/// Below this the penalty applies to almost every pair, so the cost model
/// stops discriminating and the "limit" becomes a flat tax — a slider whose
/// bottom end does nothing is worse than one that stops there.
pub const MIN_VIBE_LIMIT: f32 = 0.1;
/// Loosest. At 1.0 nothing is ever over the limit, which is the honest way to
/// spell "no limit" without a separate switch for it.
pub const MAX_VIBE_LIMIT: f32 = 1.0;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            remote: RemoteConfig::default(),
            base_font_size: default_font_size(),
            ui_scale: default_ui_scale(),
            theme_mode: ThemeMode::default(),
            theme: default_theme(),
            custom_base_color: String::new(),
            custom_accent_color: String::new(),
            headphone_profile: String::new(),
            headphone_calibration_enabled: false,
            bpm_overrides: std::collections::HashMap::new(),
            cache_max_bytes: default_cache_max_bytes(),
            metadata_lookup_enabled: false,
            vibe_limit: default_vibe_limit(),
        }
    }
}

/// Slowest tempo a manual correction may claim.
pub const MIN_MANUAL_BPM: f32 = 20.0;
/// Fastest tempo a manual correction may claim.
///
/// Comfortably past drum and bass, and past double-time corrections of it. The
/// band is not there to police taste — it is there because a value outside it
/// is a typo or a wrong unit, and because one such value is genuinely
/// dangerous: `f32::INFINITY` passes a `> 0.0` check, and `serde_json` writes
/// it as `null` and then fails to read it back. That loses the whole settings
/// file — every playlist preference and server setting — to a mistyped BPM.
pub const MAX_MANUAL_BPM: f32 = 300.0;

impl Settings {
    /// Corrected BPM for a track, if the user set one.
    pub fn bpm_override(&self, href: &str) -> Option<f32> {
        self.bpm_overrides
            .get(href)
            .copied()
            .filter(|b| b.is_finite() && *b > 0.0)
    }

    /// Set or clear a manual BPM.
    ///
    /// A non-positive value clears the override. A value that is not finite, or
    /// falls outside [`MIN_MANUAL_BPM`]–[`MAX_MANUAL_BPM`], is **refused** and
    /// returns `false` — leaving any existing correction alone. Refusing rather
    /// than clamping matters: someone who types 1280 meant 128, and silently
    /// storing 300 would be a wrong answer presented as an accepted one.
    pub fn set_bpm_override(&mut self, href: &str, bpm: f32) -> bool {
        if bpm <= 0.0 && bpm.is_finite() {
            self.bpm_overrides.remove(href);
            return true;
        }
        if !bpm.is_finite() || !(MIN_MANUAL_BPM..=MAX_MANUAL_BPM).contains(&bpm) {
            return false;
        }
        self.bpm_overrides.insert(href.to_string(), bpm);
        true
    }

    /// Clamp values that would break the UI if a config file were hand-edited.
    pub fn sanitised(mut self) -> Self {
        self.base_font_size = self.base_font_size.clamp(8, 48);
        self.ui_scale = self.ui_scale.clamp(0.5, 3.0);
        self.cache_max_bytes = self.cache_max_bytes.max(MIN_CACHE_BYTES);
        // `clamp` panics on a NaN bound and passes NaN through unchanged, and
        // this value comes off disk — so the non-finite case is handled before
        // it, not by it.
        self.vibe_limit = if self.vibe_limit.is_finite() {
            self.vibe_limit.clamp(MIN_VIBE_LIMIT, MAX_VIBE_LIMIT)
        } else {
            default_vibe_limit()
        };
        if self.remote.folder.trim().is_empty() {
            self.remote.folder = default_folder();
        }
        if self.theme.trim().is_empty() {
            self.theme = default_theme();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::DEFAULT_ENERGY_THRESHOLD;

    #[test]
    fn defaults_match_the_godot_values() {
        let s = Settings::default();
        assert_eq!(s.base_font_size, 16);
        assert_eq!(s.ui_scale, 1.2);
        assert_eq!(s.remote.folder, "Music");
        assert_eq!(s.theme_mode, ThemeMode::Preset);
    }

    /// Missing fields must not fail a load — a settings file from an older
    /// build should still open.
    #[test]
    fn partial_json_fills_in_defaults() {
        let s: Settings = serde_json::from_str(r#"{"base_font_size": 20}"#).expect("parse");
        assert_eq!(s.base_font_size, 20);
        assert_eq!(s.ui_scale, 1.2, "unspecified fields take defaults");
        assert_eq!(s.remote.folder, "Music");
    }

    #[test]
    fn round_trips_through_json() {
        let mut s = Settings::default();
        s.remote.url = "https://app.koofr.net/dav/Koofr".into();
        s.remote.username = "me".into();
        s.set_bpm_override("/a.mp3", 128.0);

        let json = serde_json::to_string(&s).expect("serialise");
        let back: Settings = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, s);
    }

    /// The credential must not be serialisable at all, so it cannot leak into
    /// a log or a settings file by accident.
    #[test]
    fn no_password_field_is_serialised() {
        let mut s = Settings::default();
        s.remote.username = "me".into();
        let json = serde_json::to_string(&s).expect("serialise");
        assert!(
            !json.to_lowercase().contains("password"),
            "settings must not carry a password: {json}"
        );
    }

    #[test]
    fn bpm_overrides_can_be_set_and_cleared() {
        let mut s = Settings::default();
        assert_eq!(s.bpm_override("/a.mp3"), None);
        assert!(s.set_bpm_override("/a.mp3", 128.0));
        assert_eq!(s.bpm_override("/a.mp3"), Some(128.0));
        assert!(s.set_bpm_override("/a.mp3", 0.0));
        assert_eq!(s.bpm_override("/a.mp3"), None, "non-positive clears");
    }

    /// A mistyped tempo must not silently become a plausible one. Someone who
    /// types 1280 meant 128, and clamping to 300 would present a wrong answer
    /// as an accepted one.
    #[test]
    fn an_implausible_bpm_is_refused_rather_than_clamped() {
        let mut s = Settings::default();
        s.set_bpm_override("/a.mp3", 128.0);

        for bad in [1280.0, 301.0, 19.0, 0.5] {
            assert!(!s.set_bpm_override("/a.mp3", bad), "accepted {bad}");
        }
        assert_eq!(
            s.bpm_override("/a.mp3"),
            Some(128.0),
            "a refused value disturbed the correction already there"
        );
    }

    /// The dangerous one. `INFINITY` passes a `> 0.0` check, and serde_json
    /// writes it as `null` and then cannot read it back — so accepting it would
    /// lose the entire settings file to one mistyped BPM.
    #[test]
    fn a_non_finite_bpm_cannot_reach_the_settings_file() {
        let mut s = Settings::default();

        for bad in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            assert!(!s.set_bpm_override("/a.mp3", bad), "accepted {bad}");
        }
        assert!(s.bpm_overrides.is_empty());

        // The failure this prevents, demonstrated rather than described.
        let json = serde_json::to_string(&s).expect("serialise");
        let back: std::result::Result<Settings, _> = serde_json::from_str(&json);
        assert!(
            back.is_ok(),
            "settings did not survive a round trip: {json}"
        );
    }

    /// The band's edges are inclusive, so a legitimate 300 BPM correction works.
    #[test]
    fn the_plausible_band_includes_its_own_edges() {
        let mut s = Settings::default();
        assert!(s.set_bpm_override("/a.mp3", MIN_MANUAL_BPM));
        assert!(s.set_bpm_override("/b.mp3", MAX_MANUAL_BPM));
        assert_eq!(s.bpm_override("/b.mp3"), Some(MAX_MANUAL_BPM));
    }

    #[test]
    fn hand_edited_nonsense_is_clamped() {
        let s = Settings {
            base_font_size: 900,
            ui_scale: 0.0,
            theme: "  ".into(),
            ..Default::default()
        }
        .sanitised();
        assert_eq!(s.base_font_size, 48);
        assert_eq!(s.ui_scale, 0.5);
        assert_eq!(s.theme, "default_dark");
    }

    /// A cache too small to hold a track fetches it, evicts it to make room for
    /// the next, and fetches it again — worse than no cache, and it reports
    /// itself as working.
    #[test]
    fn a_uselessly_small_cache_is_raised_to_a_usable_one() {
        let s = Settings {
            cache_max_bytes: 0,
            ..Default::default()
        }
        .sanitised();
        assert_eq!(s.cache_max_bytes, MIN_CACHE_BYTES);
    }

    /// The Vibe Limit comes off disk, so `sanitised` is the only thing
    /// between a corrupt file and a cost model that stops discriminating.
    #[test]
    fn a_vibe_limit_outside_the_band_is_pulled_back_into_it() {
        let strict = Settings {
            vibe_limit: 0.0,
            ..Default::default()
        }
        .sanitised();
        assert_eq!(strict.vibe_limit, MIN_VIBE_LIMIT);

        let loose = Settings {
            vibe_limit: 9.0,
            ..Default::default()
        }
        .sanitised();
        assert_eq!(loose.vibe_limit, MAX_VIBE_LIMIT);
    }

    /// `clamp` panics on a NaN bound and passes a NaN value straight through,
    /// so a NaN in the file would otherwise reach `transition_cost` and make
    /// every comparison in the search false.
    #[test]
    fn a_vibe_limit_that_is_not_a_number_falls_back_to_the_default() {
        let s = Settings {
            vibe_limit: f32::NAN,
            ..Default::default()
        }
        .sanitised();

        assert_eq!(s.vibe_limit, DEFAULT_ENERGY_THRESHOLD);
    }

    /// A settings file written before the Vibe Limit existed opens with the
    /// behaviour it had — the constant every call site used to pass.
    #[test]
    fn a_settings_file_without_a_vibe_limit_keeps_the_old_behaviour() {
        let s: Settings = serde_json::from_str(r#"{"base_font_size": 16}"#).expect("parse");

        assert_eq!(s.vibe_limit, DEFAULT_ENERGY_THRESHOLD);
    }

    /// A deliberately large cache is the person's call and must survive.
    #[test]
    fn a_generous_cache_bound_is_left_alone() {
        let huge = 200 * 1024 * 1024 * 1024;
        let s = Settings {
            cache_max_bytes: huge,
            ..Default::default()
        }
        .sanitised();
        assert_eq!(s.cache_max_bytes, huge);
    }

    #[test]
    fn remote_needs_url_and_username_to_be_configured() {
        let mut r = RemoteConfig::default();
        assert!(!r.is_configured());
        r.url = "https://x".into();
        assert!(!r.is_configured());
        r.username = "me".into();
        assert!(r.is_configured());
    }
}

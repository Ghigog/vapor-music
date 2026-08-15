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
}

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
        }
    }
}

impl Settings {
    /// Corrected BPM for a track, if the user set one.
    pub fn bpm_override(&self, href: &str) -> Option<f32> {
        self.bpm_overrides.get(href).copied().filter(|b| *b > 0.0)
    }

    /// Set or clear a manual BPM. A non-positive value clears it.
    pub fn set_bpm_override(&mut self, href: &str, bpm: f32) {
        if bpm > 0.0 {
            self.bpm_overrides.insert(href.to_string(), bpm);
        } else {
            self.bpm_overrides.remove(href);
        }
    }

    /// Clamp values that would break the UI if a config file were hand-edited.
    pub fn sanitised(mut self) -> Self {
        self.base_font_size = self.base_font_size.clamp(8, 48);
        self.ui_scale = self.ui_scale.clamp(0.5, 3.0);
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
        s.set_bpm_override("/a.mp3", 128.0);
        assert_eq!(s.bpm_override("/a.mp3"), Some(128.0));
        s.set_bpm_override("/a.mp3", 0.0);
        assert_eq!(s.bpm_override("/a.mp3"), None, "non-positive clears");
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

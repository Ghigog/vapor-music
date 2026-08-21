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
use ts_rs::TS;

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
    APPEARANCE_AUTO.to_string()
}

/// The three answers the appearance control can give.
///
/// Stored as a string rather than an enum because [`Settings::theme`] already
/// existed as one, holding a Godot theme-resource name (`default_dark`) that
/// nothing in the Tauri app ever read. Anything outside this set — including
/// that leftover — is repaired to `auto` by [`Settings::sanitised`], which is
/// also the migration: `auto` is the behaviour those installs already had,
/// since the placeholder dark mode only ever followed the OS.
pub const APPEARANCE_AUTO: &str = "auto";
/// The light theme: warm paper, sky at the horizon.
pub const APPEARANCE_DAYLIGHT: &str = "daylight";
/// The dark theme: warm umber ground under one lamp.
pub const APPEARANCE_LAMPLIGHT: &str = "lamplight";

/// Every value [`Settings::theme`] may hold.
pub const APPEARANCES: [&str; 3] = [APPEARANCE_AUTO, APPEARANCE_DAYLIGHT, APPEARANCE_LAMPLIGHT];
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ThemeMode {
    #[default]
    Preset,
    Custom,
}

/// Every other struct that crosses the IPC boundary renames to camelCase, and
/// this one did not. It therefore travelled as snake_case while `core.ts`
/// declared camelCase, so **every multi-word field read as `undefined` in the
/// frontend** — fourteen of them. Most survived on `??` fallbacks and were
/// silently ignored rather than visibly broken: the font size and UI scale did
/// nothing, the theme mode never applied, and `bpmOverrides` arrived empty so a
/// hand-corrected tempo was never marked in the table. The Vibe screen was the
/// one place that called a method on the value instead of defaulting it, and it
/// threw.
///
/// The aliases are what let a settings file written before this change still
/// load. Without them the rename would silently reset every one of those
/// fourteen fields to its default on first launch, which is the same data loss
/// by the opposite route.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Settings {
    #[serde(default)]
    pub remote: RemoteConfig,

    #[serde(default = "default_font_size")]
    #[serde(alias = "base_font_size")]
    pub base_font_size: u32,
    #[serde(default = "default_ui_scale")]
    #[serde(alias = "ui_scale")]
    pub ui_scale: f32,

    #[serde(default)]
    #[serde(alias = "theme_mode")]
    pub theme_mode: ThemeMode,
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Custom theme colours as `#rrggbb`, used when `theme_mode` is `Custom`.
    #[serde(default)]
    #[serde(alias = "custom_base_color")]
    pub custom_base_color: String,
    #[serde(default)]
    #[serde(alias = "custom_accent_color")]
    pub custom_accent_color: String,

    #[serde(default)]
    #[serde(alias = "headphone_profile")]
    pub headphone_profile: String,
    #[serde(default)]
    #[serde(alias = "headphone_calibration_enabled")]
    pub headphone_calibration_enabled: bool,

    /// Manual BPM overrides, keyed by href.
    ///
    /// New in the port. Tempo detection agrees with Essentia on ~81% of a real
    /// library and the residual is metrical error, which was accepted rather
    /// than solved — so the app needs a way for a person to correct it. DJ
    /// software conventionally has one.
    #[serde(default)]
    #[serde(alias = "bpm_overrides")]
    pub bpm_overrides: std::collections::HashMap<String, f32>,

    /// Album artwork chosen by hand, keyed by [`album_key`].
    ///
    /// The value is the URL the picture was found at; the bytes live in the
    /// image cache beside it, named by a hash of that URL. Storing the URL
    /// rather than the bytes keeps this file small — a settings document with a
    /// few hundred base64 covers in it is one the app has to read whole to
    /// answer any question about any setting.
    ///
    /// Exists because a file's embedded artwork can simply be wrong. It is not
    /// a guess the app can correct on its own: the only thing that knows the
    /// picture is wrong is the person looking at it.
    #[serde(default)]
    #[serde(alias = "album_art")]
    pub album_art: std::collections::HashMap<String, String>,

    /// Whether a looked-up cover outranks the file's own embedded artwork.
    ///
    /// Off, and it should stay the default. Embedded artwork is usually right,
    /// needs no network, and cannot be a wrong match — album search is fuzzy,
    /// and a library-wide preference for it would let one bad match replace
    /// good art on an album nobody was looking at. On, for a library whose
    /// tags are known to be poor.
    #[serde(default)]
    #[serde(alias = "prefer_looked_up_art")]
    pub prefer_looked_up_art: bool,

    /// Whether the library hides second and later copies of a recording.
    ///
    /// Off by default: these are the person's own files, and a library that
    /// quietly shows fewer tracks than are on disk is a library you cannot
    /// trust to be telling you what you have. Hiding them is a view, not a
    /// deletion — nothing is removed, and the duplicates are still there to be
    /// tidied up by hand.
    ///
    /// The Vibe DJ excludes them regardless of this, which is a different
    /// question: two copies of one track are identical in tempo, key and
    /// intensity, so a set that may use both will mix a record into itself.
    #[serde(default)]
    #[serde(alias = "hide_duplicates")]
    pub hide_duplicates: bool,

    /// Ceiling on the local audio cache, in bytes.
    ///
    /// The library lives in the user's cloud and local storage is only a cache,
    /// so this is the one number that decides how much of a device the app is
    /// entitled to. It belongs to the person, not to a constant.
    #[serde(default = "default_cache_max_bytes")]
    #[serde(alias = "cache_max_bytes")]
    // A JSON number over IPC, not a `bigint` — serde_json writes u64 as a plain
    // number. A cache ceiling is nowhere near 2^53.
    #[ts(type = "number")]
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
    #[serde(alias = "metadata_lookup_enabled")]
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
    #[serde(alias = "vibe_limit")]
    pub vibe_limit: f32,

    /// Whether this device announces itself on the local network (SYNC-001).
    ///
    /// **Off by default, for the same reason `metadata_lookup_enabled` is.**
    /// A beacon every five seconds tells everyone on the network that this
    /// machine is here and what its library folder is called, on whatever
    /// network it happens to be joined to. An unpaired peer can do nothing
    /// with that, but it is still a thing announced to a room, and this app's
    /// whole position is that it does not do that without being asked.
    ///
    /// Off also means no listening socket, so there is no firewall prompt and
    /// nothing accepting connections until sync is something the owner wants.
    #[serde(default)]
    #[serde(alias = "sync_enabled")]
    pub sync_enabled: bool,

    /// Whether the DJ conducts the set, or the queue simply plays in order.
    ///
    /// Lived in the frontend as `useState(true)` until 2026-08-17, which meant
    /// the backend — the half that actually decides what plays next — had never
    /// heard of it. The screen showed candidates and nothing drove playback, so
    /// the "DJ" could only re-order a queue someone else had built. A set of one
    /// track repeated forever.
    #[serde(default = "default_dj_mode", alias = "dj_mode")]
    pub dj_mode: bool,

    /// The energy curve the set is being conducted along.
    ///
    /// Persisted and owned by the backend for the same reason `dj_mode` is: the
    /// supervisor plans the set, so it has to know where the set is going.
    /// Choosing one *is* the action — there is no separate button — so this is
    /// the only thing that changes a set's destination.
    #[serde(default = "default_curve", alias = "curve")]
    pub curve: String,
}

/// Build, because a set that goes nowhere is the least interesting default and
/// the app exists to take you somewhere.
fn default_curve() -> String {
    "build".to_string()
}

/// On. It is the app's whole premise, and a person who does not want it has a
/// switch on the Vibe screen.
fn default_dj_mode() -> bool {
    true
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
            hide_duplicates: false,
            album_art: std::collections::HashMap::new(),
            prefer_looked_up_art: false,
            cache_max_bytes: default_cache_max_bytes(),
            metadata_lookup_enabled: false,
            vibe_limit: default_vibe_limit(),
            dj_mode: default_dj_mode(),
            curve: default_curve(),
            sync_enabled: false,
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

/// Separates the two halves of an [`album_key`].
///
/// A unit separator, because it cannot occur in a path or an album title and so
/// cannot make two different albums collide by being typed into one of them.
const KEY_SEP: char = '\u{1f}';

/// The identity of an album: its title, and the folder its tracks sit in.
///
/// Neither half is sufficient on its own, and both failures are real:
///
/// * **Title alone** merges two different records that share a name. Every
///   library eventually has two *Greatest Hits*, and they would share one tile,
///   one cover and one artwork override.
/// * **Folder alone** merges two different albums that share a directory,
///   which happens whenever a few loose tracks are dropped together. Measured
///   on the owner's library: 34 folders, and two of them hold two albums each.
///
/// Together they also get compilations right, which is the case that rules out
/// the obvious third option. Keying on artist and title would split a
/// various-artists album into one tile per artist — worse than what it fixes.
///
/// Derived from the href rather than stored, so it survives a rescan: an href
/// is what the server calls the file and does not change when tags do.
pub fn album_key(album: &str, href: &str) -> String {
    let folder = href.rsplit_once('/').map(|(f, _)| f).unwrap_or("");
    format!("{folder}{KEY_SEP}{}", album.trim())
}

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
    /// The artwork chosen by hand for an album, if any.
    pub fn album_art_for(&self, album: &str, href: &str) -> Option<&str> {
        self.album_art
            .get(&album_key(album, href))
            .map(String::as_str)
            .filter(|u| !u.trim().is_empty())
    }

    /// Choose artwork for an album by hand, or clear the choice with an empty
    /// URL. Returns whether anything changed.
    pub fn set_album_art(&mut self, album: &str, href: &str, url: &str) -> bool {
        let key = album_key(album, href);
        if url.trim().is_empty() {
            return self.album_art.remove(&key).is_some();
        }
        self.album_art.insert(key, url.trim().to_string()) != Some(url.trim().to_string())
    }

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
        // Not just the empty case: this field used to hold a Godot theme
        // resource name, so an install from before the appearance control has
        // `default_dark` in it and every install from after has one of three
        // known words. Anything else is a hand edit or a leftover, and the
        // safe reading of both is "follow the OS".
        let theme = self.theme.trim().to_ascii_lowercase();
        self.theme = if APPEARANCES.contains(&theme.as_str()) {
            theme
        } else {
            default_theme()
        };
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
        assert_eq!(s.theme, APPEARANCE_AUTO);
    }

    /// The Godot leftover is a value, not a blank, so the empty-string check
    /// this replaced would have let it through to the frontend — which knows
    /// three words and would have fallen back to one of them anyway, silently.
    #[test]
    fn an_unknown_appearance_falls_back_to_following_the_os() {
        for stored in ["default_dark", "Solarized", "", "  "] {
            let s = Settings {
                theme: stored.into(),
                ..Default::default()
            }
            .sanitised();
            assert_eq!(s.theme, APPEARANCE_AUTO, "stored {stored:?}");
        }
    }

    /// Casing and stray whitespace are a hand edit of a JSON file, not a
    /// different choice.
    #[test]
    fn a_known_appearance_survives_sanitising() {
        for stored in ["daylight", " Lamplight ", "AUTO"] {
            let s = Settings {
                theme: stored.into(),
                ..Default::default()
            }
            .sanitised();
            assert!(
                APPEARANCES.contains(&s.theme.as_str()),
                "stored {stored:?} became {:?}",
                s.theme
            );
            assert_eq!(s.theme, stored.trim().to_ascii_lowercase());
        }
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

    // -----------------------------------------------------------------------
    // Album identity and artwork overrides
    // -----------------------------------------------------------------------

    /// The failure keying on title alone would cause, and the reason this is
    /// not just the album name.
    #[test]
    fn two_albums_sharing_a_name_are_not_the_same_album() {
        let a = album_key("Greatest Hits", "/dav/Music/Queen/Greatest Hits/01.mp3");
        let b = album_key("Greatest Hits", "/dav/Music/Abba/Greatest Hits/01.mp3");
        assert_ne!(a, b);
    }

    /// And the failure keying on folder alone would cause. Both of these were
    /// measured in a real library, which is why the key is the pair.
    #[test]
    fn two_albums_sharing_a_folder_are_not_the_same_album() {
        let a = album_key("Gorillaz", "/dav/Music/Gorillaz/01.mp3");
        let b = album_key("Clint Eastwood", "/dav/Music/Gorillaz/02.mp3");
        assert_ne!(a, b);
    }

    /// A compilation is one album however many artists are on it — which is
    /// what rules out keying on artist and title.
    #[test]
    fn every_track_in_one_album_folder_shares_a_key() {
        let one = album_key("Now 42", "/dav/Music/Various/Now 42/01 Someone.mp3");
        let two = album_key("Now 42", "/dav/Music/Various/Now 42/02 Someone Else.mp3");
        assert_eq!(one, two);
    }

    #[test]
    fn a_key_is_stable_under_whitespace_and_a_missing_folder() {
        assert_eq!(
            album_key("Currents", "/dav/Music/Tame Impala/Currents/01.mp3"),
            album_key("  Currents  ", "/dav/Music/Tame Impala/Currents/01.mp3")
        );
        // A bare filename has no folder, and must not panic.
        assert!(!album_key("Loose", "track.mp3").is_empty());
    }

    #[test]
    fn artwork_chosen_by_hand_is_returned_and_can_be_cleared() {
        let mut s = Settings::default();
        let href = "/dav/Music/Tame Impala/Currents/01.mp3";
        assert_eq!(s.album_art_for("Currents", href), None);

        assert!(s.set_album_art("Currents", href, "https://cdn/cover.jpg"));
        assert_eq!(
            s.album_art_for("Currents", href),
            Some("https://cdn/cover.jpg")
        );

        // Every track on the album resolves to the same choice.
        let other = "/dav/Music/Tame Impala/Currents/07.mp3";
        assert_eq!(
            s.album_art_for("Currents", other),
            Some("https://cdn/cover.jpg")
        );
        // And an album that merely shares the title does not.
        assert_eq!(
            s.album_art_for("Currents", "/dav/Music/Eisley/Currents/01.mp3"),
            None
        );

        assert!(s.set_album_art("Currents", href, ""));
        assert_eq!(s.album_art_for("Currents", href), None);
        // Clearing something that was never set changes nothing.
        assert!(!s.set_album_art("Currents", href, "   "));
    }

    /// Survives the round trip through the settings file, and a file written
    /// before the field existed still opens.
    #[test]
    fn the_new_fields_round_trip_and_tolerate_an_older_file() {
        let mut s = Settings::default();
        s.set_album_art("Currents", "/m/a/Currents/01.mp3", "https://cdn/c.jpg");
        s.prefer_looked_up_art = true;

        let text = serde_json::to_string(&s).expect("write");
        let back: Settings = serde_json::from_str(&text).expect("read");
        assert_eq!(back.album_art, s.album_art);
        assert!(back.prefer_looked_up_art);

        let older: Settings = serde_json::from_str(r#"{"baseFontSize":16}"#).expect("read old");
        assert!(older.album_art.is_empty());
        assert!(!older.prefer_looked_up_art);
    }

    // -----------------------------------------------------------------------
    // The wire format, which was wrong for as long as this struct existed
    // -----------------------------------------------------------------------

    /// Every field crosses to the frontend in camelCase.
    ///
    /// This struct was the only one on the IPC boundary without the rename, so
    /// it travelled as snake_case while TypeScript read camelCase and got
    /// `undefined` for all fourteen multi-word fields.
    #[test]
    fn the_wire_format_is_camel_case() {
        let json = serde_json::to_value(Settings::default()).expect("serialise");
        let keys: Vec<&String> = json.as_object().expect("an object").keys().collect();

        let snake: Vec<&&String> = keys.iter().filter(|k| k.contains('_')).collect();
        assert!(
            snake.is_empty(),
            "these fields still cross as snake_case and will read as undefined \
             in the frontend: {snake:?}"
        );
        // And the specific one that threw.
        assert!(json.get("vibeLimit").is_some(), "{keys:?}");
        assert!(json.get("bpmOverrides").is_some(), "{keys:?}");
        assert!(json.get("baseFontSize").is_some(), "{keys:?}");
    }

    /// A settings file written before the rename still loads, with its values.
    ///
    /// Without the aliases the rename would reset all fourteen fields to their
    /// defaults on first launch — the same data loss as a corrupt file, by the
    /// opposite route, and silent.
    #[test]
    fn a_snake_case_settings_file_still_loads_with_its_values() {
        // `r##` rather than `r#`: the hex colours below contain `"#`, which ends
        // a single-hash raw string in the middle of the JSON.
        let old = r##"{
            "remote": {"url": "https://example.com", "username": "someone", "folder": "/dav/Music"},
            "base_font_size": 19,
            "ui_scale": 1.25,
            "theme_mode": "custom",
            "custom_base_color": "#101010",
            "custom_accent_color": "#ff8800",
            "headphone_profile": "hd650",
            "headphone_calibration_enabled": true,
            "bpm_overrides": {"/a.mp3": 128.0},
            "cache_max_bytes": 12345678900,
            "metadata_lookup_enabled": true,
            "vibe_limit": 0.75,
            "sync_enabled": true
        }"##;

        let s: Settings = serde_json::from_str(old).expect("an older file must still open");

        assert_eq!(s.base_font_size, 19);
        assert_eq!(s.ui_scale, 1.25);
        assert_eq!(s.theme_mode, ThemeMode::Custom);
        assert_eq!(s.custom_base_color, "#101010");
        assert_eq!(s.custom_accent_color, "#ff8800");
        assert_eq!(s.headphone_profile, "hd650");
        assert!(s.headphone_calibration_enabled);
        assert_eq!(s.bpm_overrides.get("/a.mp3"), Some(&128.0));
        assert_eq!(s.cache_max_bytes, 12_345_678_900);
        assert!(s.metadata_lookup_enabled);
        assert_eq!(s.vibe_limit, 0.75);
        assert!(s.sync_enabled);
        assert_eq!(s.remote.url, "https://example.com");
    }

    /// And a file in the new format loads too, so the round trip closes.
    #[test]
    fn a_camel_case_settings_file_loads() {
        let original = Settings {
            vibe_limit: 0.8,
            sync_enabled: true,
            base_font_size: 20,
            ..Default::default()
        };

        let text = serde_json::to_string(&original).expect("write");
        assert!(text.contains("vibeLimit"), "{text}");
        let back: Settings = serde_json::from_str(&text).expect("read");
        assert_eq!(back, original);
    }
}

//! No number crossing the IPC boundary may arrive as `null`.
//!
//! JSON has no way to write NaN or infinity, so `serde_json` writes `null`
//! instead. Every `f32` in a command's reply is typed `number` on the
//! TypeScript side, so a screen doing the obvious thing with it —
//! `blend.shiftPercent.toFixed(1)` — throws `TypeError` on `null`. A throw
//! during render unmounts the tree, and the window goes blank with nothing on
//! screen to say why.
//!
//! That is not a hypothetical shape of bug. It is the same class as the NaN gain
//! that reached the audio thread (`vapor_engine::deck`), arriving by a different
//! route: there it was multiplied into samples, here it is serialised into a
//! hole. Both come from a float that was never checked before it was used.
//!
//! The fix is at the boundary rather than at each use site. `finite` is applied
//! to every float a command returns, so `null` is not representable and the
//! screens can keep doing the obvious thing.

/// What `serde_json` actually does with a non-finite float.
///
/// Asserted rather than assumed, because the whole argument for `finite` rests
/// on it, and it is the sort of thing that would be quietly reasonable either
/// way — a serialiser could equally refuse to write the value at all.
#[test]
fn serde_writes_a_non_finite_float_as_null() {
    #[derive(serde::Serialize)]
    struct Probe {
        nan: f32,
        pos: f32,
        neg: f32,
        ok: f32,
    }

    let json = serde_json::to_string(&Probe {
        nan: f32::NAN,
        pos: f32::INFINITY,
        neg: f32::NEG_INFINITY,
        ok: 1.5,
    })
    .expect("serialise");

    assert_eq!(
        json, r#"{"nan":null,"pos":null,"neg":null,"ok":1.5}"#,
        "the premise of `finite` no longer holds — re-read it before trusting it"
    );
}

/// And what it does once the value has been through `finite`.
#[test]
fn a_float_through_finite_is_always_a_number() {
    #[derive(serde::Serialize)]
    struct Probe {
        #[serde(serialize_with = "vapor_app_lib::finite")]
        nan: f32,
        #[serde(serialize_with = "vapor_app_lib::finite")]
        pos: f32,
        #[serde(serialize_with = "vapor_app_lib::finite")]
        neg: f32,
        #[serde(serialize_with = "vapor_app_lib::finite")]
        ok: f32,
    }

    let json = serde_json::to_string(&Probe {
        nan: f32::NAN,
        pos: f32::INFINITY,
        neg: f32::NEG_INFINITY,
        ok: -3.25,
    })
    .expect("serialise");

    assert!(!json.contains("null"), "a null survived `finite`: {json}");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    for key in ["nan", "pos", "neg", "ok"] {
        assert!(
            parsed[key].is_number(),
            "{key} came out as {:?} rather than a number",
            parsed[key]
        );
    }
    // The real value survives; only the unrepresentable ones are substituted.
    assert_eq!(parsed["ok"], serde_json::json!(-3.25));
    // Zero rather than a clamp to a huge number: an infinite tempo shift is not
    // "a very large shift", it is an absence of an answer, and the screens
    // already render 0 sensibly.
    assert_eq!(parsed["nan"], serde_json::json!(0.0));
    assert_eq!(parsed["pos"], serde_json::json!(0.0));
}

/// The guard has to be on every float, not most of them — one unguarded field
/// is one blank window. This reads the source and gates it.
#[test]
fn every_float_a_command_returns_is_guarded() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
    )
    .expect("lib.rs");

    // Structs that are only ever serialised *to* the frontend. A field of type
    // f32 or f64 in one of these is a number TypeScript will call a method on.
    let mut unguarded: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    let mut guarded_next = false;

    for line in src.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("struct ") {
            let name = rest.split([' ', '{', '<', '(']).next().unwrap_or("");
            current = Some(name.to_string());
            continue;
        }
        if trimmed == "}" {
            current = None;
            continue;
        }
        let Some(name) = current.as_deref() else {
            continue;
        };
        // Only the reply types. `Settings` and the analysis cache are
        // `Deserialize` too and are covered by their own crates' tests.
        if !REPLY_TYPES.contains(&name) {
            continue;
        }

        if trimmed.starts_with("#[serde(serialize_with") {
            // Either width — `finite` for f32, `finite64` for f64.
            guarded_next = true;
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with("///") {
            continue;
        }
        if let Some((field, ty)) = trimmed.split_once(':') {
            let ty = ty.trim().trim_end_matches(',');
            if (ty == "f32" || ty == "f64") && !guarded_next {
                unguarded.push(format!("{name}.{}", field.trim()));
            }
            guarded_next = false;
        }
    }

    assert!(
        unguarded.is_empty(),
        "these floats cross to the frontend unguarded and can arrive as null, \
         which throws on `.toFixed`:\n  {}\n\nAdd \
         #[serde(serialize_with = \"finite\")].",
        unguarded.join("\n  ")
    );
}

/// The reply types this test polices. Listed rather than inferred so that a new
/// one has to be considered: adding a struct without adding it here is the one
/// way past this check, and it is a deliberate act rather than an oversight.
const REPLY_TYPES: &[&str] = &[
    "BlendPreview",
    "PlaybackState",
    "MixCandidate",
    "TrackMeta",
    "Row",
    "CacheStatus",
    "SyncView",
    "AlbumArt",
];

//! Musical key detection — the replacement for Essentia's `KeyExtractor`.
//!
//! The Godot build configures Essentia with `profileType: "temperley"`, so the
//! same profiles are used here; that makes the two implementations comparable
//! rather than merely both plausible.
//!
//! Chroma is folded from the magnitude spectrum, correlated against all 24
//! rotated major/minor profiles, and the winner mapped to the Camelot notation
//! the app actually uses (`dj_pathfinder` navigates the Camelot wheel).

use crate::spectrum::Spectrogram;

/// Temperley's (2001) revision of the Krumhansl–Kessler key profiles,
/// starting at the tonic.
const MAJOR_PROFILE: [f32; 12] = [
    5.0, 2.0, 3.5, 2.0, 4.5, 4.0, 2.0, 4.5, 2.0, 3.5, 1.5, 4.0,
];
const MINOR_PROFILE: [f32; 12] = [
    5.0, 2.0, 3.5, 4.5, 2.0, 4.0, 2.0, 4.5, 3.5, 2.0, 1.5, 4.0,
];

/// Camelot code per pitch class (index 0 = C), major then minor.
/// Major C = 8B and minor A = 8A anchor the wheel; each fifth adds one.
const CAMELOT_MAJOR: [&str; 12] = [
    "8B", "3B", "10B", "5B", "12B", "7B", "2B", "9B", "4B", "11B", "6B", "1B",
];
const CAMELOT_MINOR: [&str; 12] = [
    "5A", "12A", "7A", "2A", "9A", "4A", "11A", "6A", "1A", "8A", "3A", "10A",
];

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub struct KeyResult {
    pub camelot: String,
    /// e.g. "F# minor" — for display and debugging, not used by the pathfinder.
    pub name: String,
    /// Correlation of the winner; low values mean genuinely ambiguous tonality
    /// (drums-only, ambient) and callers may prefer to treat these as unknown.
    pub confidence: f32,
}

pub fn estimate(spec: &Spectrogram) -> Option<KeyResult> {
    if spec.frames.is_empty() {
        return None;
    }

    let chroma = chroma_vector(spec)?;
    let (pc, is_major, confidence) = best_key(&chroma);

    let camelot = if is_major {
        CAMELOT_MAJOR[pc]
    } else {
        CAMELOT_MINOR[pc]
    };
    let name = format!(
        "{} {}",
        NOTE_NAMES[pc],
        if is_major { "major" } else { "minor" }
    );

    Some(KeyResult {
        camelot: camelot.to_string(),
        name,
        confidence,
    })
}

/// Fold the spectrogram into 12 pitch classes, averaged over the whole track.
///
/// Expects a [`crate::spectrum::for_key`] spectrogram; the tempo window is far
/// too coarse in frequency to do this correctly (see the module docs there).
fn chroma_vector(spec: &Spectrogram) -> Option<[f32; 12]> {
    let mut chroma = [0.0f32; 12];
    let bins = spec.frames[0].len();

    // C3 to A6. Below C3 even the 8192-point window gives under two bins per
    // semitone; above A6 harmonics dominate and smear the chroma.
    let f_min = 130.0f32;
    let f_max = 1760.0f32;

    // Precompute each bin's pitch class and a leakage weight.
    //
    // Hard-assigning every bin to its nearest pitch class is what made the
    // first version report F minor for a C major triad: a windowed sinusoid
    // spreads over several bins, and the skirts land in adjacent semitones.
    // Weighting by distance from the semitone centre suppresses those skirts
    // without needing a full constant-Q transform.
    let mut bin_pc = vec![usize::MAX; bins];
    let mut bin_w = vec![0.0f32; bins];
    for k in 1..bins {
        let f = spec.bin_freq(k);
        if f < f_min || f > f_max {
            continue;
        }
        // MIDI note number, then fold to pitch class: A4=440 -> 69 -> class 9,
        // which puts C at 0.
        let midi = 69.0 + 12.0 * (f / 440.0).log2();
        let nearest = midi.round();
        let dev = (midi - nearest).abs(); // 0.0 at centre, 0.5 midway
        bin_pc[k] = (nearest as i32).rem_euclid(12) as usize;
        // Raised cosine: full weight on centre, zero halfway to the next
        // semitone.
        bin_w[k] = (std::f32::consts::PI * dev).cos().powi(2);
    }

    for frame in &spec.frames {
        for (k, &mag) in frame.iter().enumerate() {
            let pc = bin_pc[k];
            if pc != usize::MAX {
                // Energy, not amplitude — matches how the profiles were derived
                // and makes strong tonal content dominate.
                chroma[pc] += mag * mag * bin_w[k];
            }
        }
    }

    let total: f32 = chroma.iter().sum();
    if total <= 0.0 {
        return None;
    }
    for c in chroma.iter_mut() {
        *c /= total;
    }
    Some(chroma)
}

/// Pearson correlation of the chroma against all 24 rotated profiles.
fn best_key(chroma: &[f32; 12]) -> (usize, bool, f32) {
    let mut best = (0usize, true, f32::NEG_INFINITY);

    for pc in 0..12 {
        for &(profile, is_major) in &[(&MAJOR_PROFILE, true), (&MINOR_PROFILE, false)] {
            // Rotate the profile so its tonic sits at pitch class `pc`.
            let mut rotated = [0.0f32; 12];
            for i in 0..12 {
                rotated[(i + pc) % 12] = profile[i];
            }
            let r = pearson(chroma, &rotated);
            if r > best.2 {
                best = (pc, is_major, r);
            }
        }
    }
    best
}

fn pearson(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let ma = a.iter().sum::<f32>() / 12.0;
    let mb = b.iter().sum::<f32>() / 12.0;
    let mut num = 0.0;
    let mut da = 0.0;
    let mut db = 0.0;
    for i in 0..12 {
        let x = a[i] - ma;
        let y = b[i] - mb;
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if da <= 0.0 || db <= 0.0 {
        return 0.0;
    }
    num / (da.sqrt() * db.sqrt())
}

/// Camelot neighbours are harmonically compatible: same number (relative
/// major/minor), or +/-1 on the same letter. `dj_pathfinder` relies on this
/// relation, so it is asserted here as a property of the mapping itself.
pub fn camelot_distance(a: &str, b: &str) -> Option<u32> {
    let parse = |s: &str| -> Option<(u32, char)> {
        let letter = s.chars().last()?;
        let num: u32 = s[..s.len() - 1].parse().ok()?;
        if !(1..=12).contains(&num) {
            return None;
        }
        Some((num, letter))
    };
    let (na, la) = parse(a)?;
    let (nb, lb) = parse(b)?;

    let around = |x: u32, y: u32| -> u32 {
        let d = x.abs_diff(y);
        d.min(12 - d)
    };

    Some(if la == lb {
        around(na, nb)
    } else if na == nb {
        1
    } else {
        around(na, nb) + 1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camelot_tables_are_a_bijection() {
        let mut seen = std::collections::HashSet::new();
        for c in CAMELOT_MAJOR.iter().chain(CAMELOT_MINOR.iter()) {
            assert!(seen.insert(*c), "duplicate Camelot code {c}");
        }
        assert_eq!(seen.len(), 24);
    }

    #[test]
    fn relative_major_minor_share_a_number() {
        // A minor (pc 9) is relative to C major (pc 0): both are 8.
        assert_eq!(CAMELOT_MINOR[9], "8A");
        assert_eq!(CAMELOT_MAJOR[0], "8B");
        // E minor (pc 4) relative to G major (pc 7): both 9.
        assert_eq!(CAMELOT_MINOR[4], "9A");
        assert_eq!(CAMELOT_MAJOR[7], "9B");
    }

    #[test]
    fn fifths_are_adjacent_on_the_wheel() {
        // C major -> G major is one step up the wheel.
        assert_eq!(camelot_distance("8B", "9B"), Some(1));
        // And the wheel wraps.
        assert_eq!(camelot_distance("12B", "1B"), Some(1));
        // Relative major/minor is one step across.
        assert_eq!(camelot_distance("8A", "8B"), Some(1));
    }

    /// A synthesised C major triad must resolve to C major / 8B.
    #[test]
    fn detects_synthetic_c_major() {
        let rate = 44100u32;
        let n = rate as usize * 5;
        let mut s = vec![0.0f32; n];
        // C4, E4, G4
        for f in [261.63f32, 329.63, 392.0] {
            for (i, v) in s.iter_mut().enumerate() {
                let t = i as f32 / rate as f32;
                *v += (2.0 * std::f32::consts::PI * f * t).sin() / 3.0;
            }
        }
        let spec = crate::spectrum::for_key(&s, rate);
        let k = estimate(&spec).expect("key");
        assert_eq!(k.camelot, "8B", "got {} ({})", k.camelot, k.name);
    }
}

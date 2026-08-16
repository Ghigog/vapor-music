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
const MAJOR_PROFILE: [f32; 12] = [5.0, 2.0, 3.5, 2.0, 4.5, 4.0, 2.0, 4.5, 2.0, 3.5, 1.5, 4.0];
const MINOR_PROFILE: [f32; 12] = [5.0, 2.0, 3.5, 4.5, 2.0, 4.0, 2.0, 4.5, 3.5, 2.0, 1.5, 4.0];

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

    // Each bin contributes to the pitch classes of the fundamentals that could
    // have produced it, not just to its own — a harmonic pitch class profile.
    //
    // A note at f also radiates at 2f, 3f, 4f, 5f…, whose pitch classes are the
    // octave, the fifth, the octave again and the major third. Assigning every
    // bin to its own class therefore credits a plain C note with energy at G
    // and E, which inflates the fifth and third of every key and biases the
    // correlation toward major. Folding each bin back onto its candidate
    // fundamentals returns that energy where it belongs.
    //
    // Sub-weighting by harmonic index rather than treating them equally: a
    // fourth harmonic is much weaker evidence of a fundamental than the first,
    // and equal weighting simply moves the bias rather than removing it.
    const HARMONICS: usize = 4;
    let harmonic_weight = |h: usize| 0.6f32.powi(h as i32 - 1);

    // Precompute, per bin, the (pitch class, weight) pairs it contributes to.
    //
    // The leakage weight is retained from the earlier fix: a windowed sinusoid
    // spreads across several bins and the skirts land in adjacent semitones,
    // which is what made the first version read a C major triad as F minor.
    let mut contributions: Vec<Vec<(usize, f32)>> = vec![Vec::new(); bins];
    for (k, slot) in contributions.iter_mut().enumerate().take(bins).skip(1) {
        let f = spec.bin_freq(k);
        if f < f_min || f > f_max * HARMONICS as f32 {
            continue;
        }
        for h in 1..=HARMONICS {
            let fundamental = f / h as f32;
            if fundamental < f_min || fundamental > f_max {
                continue;
            }
            let midi = 69.0 + 12.0 * (fundamental / 440.0).log2();
            let nearest = midi.round();
            let dev = (midi - nearest).abs(); // 0.0 at centre, 0.5 midway
            let pc = (nearest as i32).rem_euclid(12) as usize;
            // Raised cosine: full weight on centre, zero halfway to the next
            // semitone.
            let leakage = (std::f32::consts::PI * dev).cos().powi(2);
            let w = leakage * harmonic_weight(h);
            if w > 1e-4 {
                slot.push((pc, w));
            }
        }
    }

    // Per-frame normalisation: every moment contributes equally to the profile.
    //
    // Without it the loudest section decides the key. A track whose chorus is
    // 12 dB above its verses is effectively analysed on the chorus alone, and a
    // modulation or a loud non-tonal drop drags the whole estimate with it.
    for frame in &spec.frames {
        let mut frame_chroma = [0.0f32; 12];
        for k in peaks(frame) {
            if contributions[k].is_empty() {
                continue;
            }
            // Energy, not amplitude — matches how the profiles were derived
            // and makes strong tonal content dominate within the frame.
            let energy = frame[k] * frame[k];
            for &(pc, w) in &contributions[k] {
                frame_chroma[pc] += energy * w;
            }
        }
        let sum: f32 = frame_chroma.iter().sum();
        if sum > 1e-12 {
            for (c, f) in chroma.iter_mut().zip(frame_chroma.iter()) {
                *c += f / sum;
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

/// Magnitude below which a bin is not worth calling a peak, relative to the
/// loudest bin in the same frame.
///
/// −40 dB. A conventional floor rather than a fitted one: the point is to
/// exclude the noise between partials, and anything 40 dB down is not a
/// partial anyone is playing.
const PEAK_FLOOR: f32 = 0.01;

/// Bins in `frame` that are local maxima worth treating as partials.
///
/// **Only peaks contribute to the chroma.** Every bin contributing is the
/// obvious implementation and it is what a drum kit exploits: a kick or a snare
/// is broadband, so it deposits energy into all twelve pitch classes at once and
/// flattens the profile toward uniform. On the electronic music this library is
/// mostly made of, that is a large share of the signal.
///
/// Essentia's `HPCP` — the thing these figures are compared against — is fed
/// from `SpectralPeaks` for exactly this reason.
fn peaks(frame: &[f32]) -> impl Iterator<Item = usize> + '_ {
    let floor = frame.iter().copied().fold(0.0f32, f32::max) * PEAK_FLOOR;
    (1..frame.len().saturating_sub(1))
        .filter(move |&k| frame[k] >= floor && frame[k] > frame[k - 1] && frame[k] >= frame[k + 1])
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

    /// A triad at a given tuning, for the tests below.
    fn triad(rate: u32, secs: f32, cents: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        let mut s = vec![0.0f32; n];
        let shift = 2f32.powf(cents / 1200.0);
        // C4, E4, G4
        for f in [261.63f32, 329.63, 392.0] {
            let f = f * shift;
            for (i, v) in s.iter_mut().enumerate() {
                let t = i as f32 / rate as f32;
                *v += (2.0 * std::f32::consts::PI * f * t).sin() / 3.0;
            }
        }
        s
    }

    /// A synthesised C major triad must resolve to C major / 8B.
    #[test]
    fn detects_synthetic_c_major() {
        let rate = 44100u32;
        let spec = crate::spectrum::for_key(&triad(rate, 5.0, 0.0), rate);
        let k = estimate(&spec).expect("key");
        assert_eq!(k.camelot, "8B", "got {} ({})", k.camelot, k.name);
    }
}

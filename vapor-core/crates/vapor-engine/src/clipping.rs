//! Three-band clipping prevention (MIG-006).
//!
//! Measured on a real transition, Bass Swap clipped: peak 1.000 with 152
//! clipped samples over the mix window, where Standard Crossfade peaked at
//! 0.869 with none. The cause is structural rather than a bug — Bass Swap holds
//! the outgoing deck at 0 dB while the incoming one fades all the way up, so
//! for the second half both decks are at full level and their sum exceeds
//! unity.
//!
//! The Godot build solves this with `calculate_chunk_rms` in the C++ extension
//! plus `_process_clipping_prevention` in `audio_manager.gd`: split each deck
//! into low/mid/high, and when the summed level in a band would clip, duck the
//! *outgoing* deck in that band only. Ducking per band rather than broadband is
//! the point — a bass collision should not pull the vocals down with it.
//!
//! This is a faithful port, including the one-block measurement lag: Godot also
//! acts on the previous frame's RMS.

/// Crossover coefficients from `audio_dsp.cpp`. One-pole smoothing factors, not
/// frequencies: at 44.1 kHz they put the splits near 240 Hz and 3.2 kHz.
const LOW_COEF: f64 = 0.0344;
const MID_COEF: f64 = 0.363;

/// Level at which ducking begins, as linear RMS. From `rms_reference` in
/// `audio_manager.gd`, with the same +2 dB of headroom applied.
pub const RMS_REFERENCE: f32 = 0.5;
const THRESHOLD_HEADROOM_DB: f32 = 2.0;

/// Deepest cut the guard may apply, matching the Godot clamp.
const MAX_ATTENUATION_DB: f32 = -40.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bands {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

impl Bands {
    pub fn scaled(self, k: f32) -> Bands {
        Bands {
            low: self.low * k,
            mid: self.mid * k,
            high: self.high * k,
        }
    }
}

/// Per-deck band splitter and RMS meter.
///
/// Filter state persists across blocks, so each deck owns one. In the Godot
/// build this state lived on the deck's `AudioDSP` instance for the same
/// reason — a shared meter would smear one deck's bass into the other's.
#[derive(Clone, Copy, Debug, Default)]
pub struct BandRms {
    low_l: f64,
    low_r: f64,
    mid_low_l: f64,
    mid_low_r: f64,
}

impl BandRms {
    pub fn reset(&mut self) {
        *self = BandRms::default();
    }

    /// RMS per band over one block, advancing the filter state.
    ///
    /// Accumulates in `f64` deliberately: the GDScript original accumulated in
    /// 64-bit floats, and matching that keeps this port comparable against it.
    pub fn measure(&mut self, block: &[[f32; 2]]) -> Bands {
        if block.is_empty() {
            return Bands::default();
        }

        let (mut sum_low, mut sum_mid, mut sum_high) = (0.0f64, 0.0f64, 0.0f64);

        for frame in block {
            let l = frame[0] as f64;
            self.low_l += LOW_COEF * (l - self.low_l);
            self.mid_low_l += MID_COEF * (l - self.mid_low_l);
            let low_l = self.low_l;
            let mid_l = self.mid_low_l - self.low_l;
            let high_l = l - self.mid_low_l;

            let r = frame[1] as f64;
            self.low_r += LOW_COEF * (r - self.low_r);
            self.mid_low_r += MID_COEF * (r - self.mid_low_r);
            let low_r = self.low_r;
            let mid_r = self.mid_low_r - self.low_r;
            let high_r = r - self.mid_low_r;

            sum_low += (low_l * low_l + low_r * low_r) * 0.5;
            sum_mid += (mid_l * mid_l + mid_r * mid_r) * 0.5;
            sum_high += (high_l * high_l + high_r * high_r) * 0.5;
        }

        let inv = 1.0 / block.len() as f64;
        Bands {
            low: (sum_low * inv).sqrt() as f32,
            mid: (sum_mid * inv).sqrt() as f32,
            high: (sum_high * inv).sqrt() as f32,
        }
    }
}

/// Attenuation to apply to the **outgoing** deck, per band, in dB.
///
/// Only the outgoing deck is ducked. The incoming track is the one the listener
/// is being moved toward, so pulling it down to make room would defeat the
/// transition; the Godot implementation makes the same choice.
///
/// `out` and `inc` are the decks' raw band RMS, before their gain and EQ; those
/// are supplied separately so the guard sees the level that will actually reach
/// the master.
pub fn attenuation_db(
    out: Bands,
    inc: Bands,
    out_gain: f32,
    inc_gain: f32,
    out_eq_db: Bands,
    inc_eq_db: Bands,
) -> Bands {
    let threshold = RMS_REFERENCE * db_to_linear(THRESHOLD_HEADROOM_DB);

    let band = |o: f32, i: f32, o_eq: f32, i_eq: f32| -> f32 {
        let o_level = o * out_gain * db_to_linear(o_eq);
        let i_level = i * inc_gain * db_to_linear(i_eq);
        let combined = o_level + i_level;

        if combined <= threshold {
            return 0.0;
        }
        // Take the whole excess out of the outgoing deck.
        let excess = combined - threshold;
        let target = (o_level - excess).clamp(0.0, o_level);
        if o_level <= 1e-4 {
            // Outgoing contributes nothing here; ducking it cannot help, and
            // dividing by it would produce a nonsense ratio.
            return 0.0;
        }
        linear_to_db(target / o_level).clamp(MAX_ATTENUATION_DB, 0.0)
    };

    Bands {
        low: band(out.low, inc.low, out_eq_db.low, inc_eq_db.low),
        mid: band(out.mid, inc.mid, out_eq_db.mid, inc_eq_db.mid),
        high: band(out.high, inc.high, out_eq_db.high, inc_eq_db.high),
    }
}

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn linear_to_db(g: f32) -> f32 {
    if g <= 1e-6 {
        MAX_ATTENUATION_DB
    } else {
        20.0 * g.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, amp: f32, n: usize, rate: f32) -> Vec<[f32; 2]> {
        (0..n)
            .map(|i| {
                let v = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate).sin();
                [v, v]
            })
            .collect()
    }

    #[test]
    fn splits_energy_into_the_right_band() {
        let rate = 44100.0;
        let n = 8192;

        let mut m = BandRms::default();
        let low = m.measure(&tone(60.0, 0.8, n, rate));
        assert!(
            low.low > low.mid && low.low > low.high,
            "60 Hz should land in the low band: {low:?}"
        );

        let mut m = BandRms::default();
        let high = m.measure(&tone(9000.0, 0.8, n, rate));
        assert!(
            high.high > high.low && high.high > high.mid,
            "9 kHz should land in the high band: {high:?}"
        );
    }

    #[test]
    fn quiet_material_is_not_ducked() {
        let quiet = Bands {
            low: 0.1,
            mid: 0.1,
            high: 0.1,
        };
        let a = attenuation_db(quiet, quiet, 1.0, 1.0, Bands::default(), Bands::default());
        assert_eq!(a, Bands::default());
    }

    /// The Bass Swap case: both decks loud in the low band at once.
    #[test]
    fn colliding_bass_ducks_the_outgoing_deck() {
        let loud = Bands {
            low: 0.6,
            mid: 0.05,
            high: 0.05,
        };
        let a = attenuation_db(loud, loud, 1.0, 1.0, Bands::default(), Bands::default());
        assert!(a.low < -1.0, "low band should be ducked, got {}", a.low);
        assert_eq!(a.mid, 0.0, "mid band was quiet and must be untouched");
        assert_eq!(a.high, 0.0, "high band was quiet and must be untouched");
    }

    /// Ducking is per band. A bass collision must not pull down the mids, or
    /// every transition would audibly duck the vocals.
    #[test]
    fn ducking_does_not_spill_into_other_bands() {
        let out = Bands {
            low: 0.7,
            mid: 0.3,
            high: 0.2,
        };
        let inc = out;
        let a = attenuation_db(out, inc, 1.0, 1.0, Bands::default(), Bands::default());
        assert!(a.low < a.mid, "low should be cut harder than mid");
        assert!(a.mid >= MAX_ATTENUATION_DB && a.mid <= 0.0);
    }

    #[test]
    fn attenuation_never_exceeds_the_clamp() {
        let huge = Bands {
            low: 50.0,
            mid: 50.0,
            high: 50.0,
        };
        let a = attenuation_db(huge, huge, 1.0, 1.0, Bands::default(), Bands::default());
        for v in [a.low, a.mid, a.high] {
            assert!((MAX_ATTENUATION_DB..=0.0).contains(&v), "out of range: {v}");
        }
    }

    /// A silent outgoing deck cannot be ducked into helping, and must not
    /// produce a division-by-zero ratio.
    #[test]
    fn silent_outgoing_deck_is_left_alone() {
        let silent = Bands::default();
        let loud = Bands {
            low: 0.9,
            mid: 0.9,
            high: 0.9,
        };
        let a = attenuation_db(silent, loud, 1.0, 1.0, Bands::default(), Bands::default());
        assert_eq!(a, Bands::default());
    }

    /// Filter state must persist across blocks: measuring one signal in two
    /// halves should closely match measuring it whole.
    #[test]
    fn filter_state_persists_across_blocks() {
        let rate = 44100.0;
        let sig = tone(200.0, 0.5, 8192, rate);

        let mut whole = BandRms::default();
        let a = whole.measure(&sig);

        let mut split = BandRms::default();
        split.measure(&sig[..4096]);
        let b = split.measure(&sig[4096..]);

        // Second half of a steady tone should read close to the whole-signal
        // value once the filters have settled.
        assert!(
            (a.low - b.low).abs() < 0.05 && (a.mid - b.mid).abs() < 0.05,
            "state did not carry: whole {a:?} vs second half {b:?}"
        );
    }
}

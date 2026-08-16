//! Transition envelopes — the replacement for the Godot tween automation in
//! `_run_deck_transition`.
//!
//! The Godot version builds a `Tween` per transition, chaining `tween_method`
//! calls that write `AudioServer.set_bus_volume_db` and EQ band gains from the
//! main thread at frame rate. That couples audio automation to rendering frame
//! rate and to main-thread scheduling — the structural reason PERF-001 exists.
//!
//! Here an envelope is a **pure function of elapsed time**. The mixer samples it
//! once per block from the audio clock. It cannot stutter when the UI is busy,
//! and testing it needs no engine and no `custom_step` on a paused tween.
//!
//! ## Fidelity
//!
//! These envelopes replicate `audio_manager.gd`'s easing curves rather than
//! "improving" them — the migration's verification strategy is comparison
//! against existing behaviour, so a silent change of sound would defeat the
//! point.
//!
//! **One deliberate exception.** Standard Crossfade is now equal-power, where
//! the original interpolated both gains linearly in dB and left a hole in the
//! middle of every mix (TD-23, MIG-015). Carried across the port unchanged so
//! the rest could be verified against it, then fixed once it was — a known
//! defect that survives a migration is a good argument against having migrated.
//!
//! Godot easing equivalents used below:
//!
//! * `set_trans(TRANS_SINE)` with no `set_ease` is `EASE_IN_OUT` →
//!   `(1 - cos(pi*t)) / 2`
//! * `TRANS_QUAD` + `EASE_IN`  → `t^2`
//! * `TRANS_QUAD` + `EASE_OUT` → `1 - (1-t)^2`

use crate::biquad::Sweep;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionType {
    StandardCrossfade,
    BassSwap,
    FilterSweep,
    EchoOut,
    ReverbFreeze,
    TempoMorph,
}

impl TransitionType {
    /// From `audio_manager.gd::get_transition_duration`.
    pub fn default_duration(&self) -> f32 {
        match self {
            TransitionType::StandardCrossfade => 3.0,
            TransitionType::BassSwap => 6.0,
            TransitionType::FilterSweep => 4.0,
            TransitionType::EchoOut => 5.0,
            TransitionType::ReverbFreeze => 5.0,
            TransitionType::TempoMorph => 6.0,
        }
    }

    /// Whether the incoming deck should meet the outgoing one halfway in tempo
    /// rather than matching it outright.
    ///
    /// Only Tempo Morph does: `audio_manager.gd` sets the incoming pitch to
    /// `((bpm_out + bpm_in) / 2) / bpm_in`, so both tracks bend toward a tempo
    /// between them instead of one being dragged to the other.
    pub fn morphs_tempo(&self) -> bool {
        matches!(self, TransitionType::TempoMorph)
    }
}

/// Everything the mixer applies to one deck at one instant.
#[derive(Clone, Copy, Debug)]
pub struct DeckAutomation {
    pub gain_db: f32,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub sweep: Option<Sweep>,
    /// Delay wet/dry, 0 = dry. Godot automates the *dry* level; this is its
    /// complement, because the effect takes a mix.
    pub delay_mix: f32,
    /// Reverb wet/dry, 0 = dry.
    pub reverb_mix: f32,
}

impl DeckAutomation {
    pub fn neutral() -> Self {
        DeckAutomation {
            gain_db: 0.0,
            eq_low_db: 0.0,
            eq_mid_db: 0.0,
            eq_high_db: 0.0,
            sweep: None,
            delay_mix: 0.0,
            reverb_mix: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Automation {
    pub outgoing: DeckAutomation,
    pub incoming: DeckAutomation,
}

pub struct Transition {
    pub kind: TransitionType,
    pub duration: f32,
    elapsed: f32,
}

/// The floor both implementations fade to.
pub const SILENCE_DB: f32 = -60.0;

/// Linear gain as decibels, floored at silence.
///
/// The deck takes dB because that is what every other envelope here speaks; an
/// equal-power law is naturally expressed as a linear gain, so it is converted
/// once here rather than the deck growing a second setter.
fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        return SILENCE_DB;
    }
    (20.0 * gain.log10()).max(SILENCE_DB)
}
/// Bass cut depth for a Bass Swap (`_transition_eq_gains` bands 0 and 1).
const BASS_CUT_DB: f32 = -40.0;
/// Mid and high duck applied by `apply_mid_cut` during an Echo Out.
const MID_CUT_DB: f32 = -24.0;
/// Level the outgoing deck is trimmed to as reverb wet ramps up, so the wet
/// signal adding to the dry one does not swell the mix.
const REVERB_TRIM_DB: f32 = -6.0;

impl Transition {
    pub fn new(kind: TransitionType, duration: f32) -> Self {
        Transition {
            kind,
            duration: duration.max(0.001),
            elapsed: 0.0,
        }
    }

    pub fn advance(&mut self, dt: f32) {
        self.elapsed = (self.elapsed + dt).min(self.duration);
    }

    pub fn is_complete(&self) -> bool {
        self.elapsed >= self.duration
    }

    pub fn progress(&self) -> f32 {
        self.elapsed / self.duration
    }

    pub fn automation(&self) -> Automation {
        match self.kind {
            TransitionType::StandardCrossfade => self.standard_crossfade(),
            TransitionType::BassSwap => self.bass_swap(),
            TransitionType::FilterSweep => self.filter_sweep(),
            TransitionType::EchoOut => self.echo_out(),
            TransitionType::ReverbFreeze => self.reverb_freeze(),
            TransitionType::TempoMorph => self.tempo_morph(),
        }
    }

    /// The incoming track arrives over the first half; at the midpoint the
    /// outgoing track's dry signal is cut and only its echo is left to decay.
    ///
    /// From `audio_manager.gd`: 350 ms delay, −10 dB feedback, dry cut over
    /// 0.1 s at the midpoint, tail decaying through the remainder. The effect
    /// is that the outgoing track does not fade — it stops, and its echo
    /// finishes the sentence.
    fn echo_out(&self) -> Automation {
        let half = self.duration * 0.5;
        let cut = if self.duration > 1.0 {
            0.1
        } else {
            self.duration * 0.02
        };

        let in_gain = lerp(SILENCE_DB, 0.0, sine_in_out(self.elapsed / half));
        // Dry goes 1 -> 0 over `cut` starting at the midpoint, so the mix goes
        // 0 -> 1 across the same span.
        let delay_mix = ((self.elapsed - half) / cut).clamp(0.0, 1.0);
        // The mid-cut from `apply_mid_cut`: the outgoing mids duck across the
        // first half so the incoming vocal has room.
        let mid_cut = lerp(0.0, MID_CUT_DB, (self.elapsed / half).clamp(0.0, 1.0));

        Automation {
            outgoing: DeckAutomation {
                eq_mid_db: mid_cut,
                eq_high_db: mid_cut,
                delay_mix,
                ..DeckAutomation::neutral()
            },
            incoming: DeckAutomation {
                gain_db: in_gain,
                ..DeckAutomation::neutral()
            },
        }
    }

    /// The outgoing track dissolves into its own reverb, which is then frozen
    /// and allowed to decay under the incoming track.
    ///
    /// From `audio_manager.gd`: wet ramps 0 → 1 across the first half while the
    /// outgoing bus drops to −6 dB — the level trim exists because a wet signal
    /// added to a dry one swells, and the original compensates rather than
    /// letting the mix jump. At the midpoint the outgoing deck stops, so what
    /// remains is tail with nothing feeding it.
    fn reverb_freeze(&self) -> Automation {
        let half = self.duration * 0.5;
        let first = (self.elapsed / half).clamp(0.0, 1.0);
        let past_midpoint = self.elapsed >= half;

        let in_gain = lerp(SILENCE_DB, 0.0, sine_in_out(first));
        let (out_gain, reverb_mix) = if past_midpoint {
            // Frozen: the deck is silenced and only the tail continues.
            let second = ((self.elapsed - half) / half).clamp(0.0, 1.0);
            (lerp(REVERB_TRIM_DB, SILENCE_DB, sine_in_out(second)), 1.0)
        } else {
            (lerp(0.0, REVERB_TRIM_DB, sine_in_out(first)), first)
        };

        Automation {
            outgoing: DeckAutomation {
                gain_db: out_gain,
                reverb_mix,
                ..DeckAutomation::neutral()
            },
            incoming: DeckAutomation {
                gain_db: in_gain,
                ..DeckAutomation::neutral()
            },
        }
    }

    /// A long equal-power blend, with both tracks bending toward a tempo
    /// between them.
    ///
    /// The envelope is the crossfade's; what makes it a Tempo Morph lives in
    /// the stretch ratio, which the mixer sets from
    /// [`TransitionType::morphs_tempo`]. Keeping it here would mean the
    /// envelope knew about tempo, which it otherwise does not.
    fn tempo_morph(&self) -> Automation {
        self.standard_crossfade()
    }

    /// Both decks cross with **constant power** (TD-23, MIG-015).
    ///
    /// The Godot original interpolated the two gains linearly *in decibels*,
    /// which sounds like the obvious thing and is not: at the midpoint both
    /// decks sit at −30 dB, so the summed power of two uncorrelated tracks is
    /// about 0.002 of full scale. That is a hole in the middle of every mix —
    /// roughly 3 dB by the usual description, far worse here because the fade
    /// spans 60 dB.
    ///
    /// The fix is the standard equal-power pair: gains of `cos` and `sin` over
    /// a quarter turn, whose squares sum to exactly 1 at every instant. The
    /// level a listener hears stays put across the whole transition.
    ///
    /// This was inherited deliberately during the port — fidelity over
    /// correctness while everything else moved — and is now corrected, because
    /// carrying a known defect across a migration is how the migration stops
    /// being worth doing.
    fn standard_crossfade(&self) -> Automation {
        // Eased rather than linear in time, so the *rate* of the crossfade
        // still matches the original's feel; only the level law changes.
        let p = sine_in_out(self.elapsed / self.duration);
        let angle = p * std::f32::consts::FRAC_PI_2;
        Automation {
            outgoing: DeckAutomation {
                gain_db: gain_to_db(angle.cos()),
                ..DeckAutomation::neutral()
            },
            incoming: DeckAutomation {
                gain_db: gain_to_db(angle.sin()),
                ..DeckAutomation::neutral()
            },
        }
    }

    /// Incoming fades in across the first half while its bass is held cut;
    /// at the midpoint the outgoing begins fading and the two low bands swap
    /// quickly, so two kick drums never share the low end.
    fn bass_swap(&self) -> Automation {
        let t = self.elapsed;
        let d = self.duration;
        let half = d * 0.5;
        // From audio_manager.gd — a fast swap, not a gradual one.
        let swap_duration = if d > 1.0 { 0.5 } else { d * 0.06 };

        // First half: incoming fades up, outgoing untouched.
        // Second half: outgoing fades out, incoming already at full level.
        let (in_gain, out_gain, swap) = if t < half {
            (lerp(SILENCE_DB, 0.0, sine_in_out(t / half)), 0.0, 0.0)
        } else {
            let second = (t - half) / half;
            (
                0.0,
                lerp(0.0, SILENCE_DB, sine_in_out(second)),
                ((t - half) / swap_duration).clamp(0.0, 1.0),
            )
        };

        Automation {
            outgoing: DeckAutomation {
                gain_db: out_gain,
                eq_low_db: lerp(0.0, BASS_CUT_DB, swap),
                ..DeckAutomation::neutral()
            },
            incoming: DeckAutomation {
                gain_db: in_gain,
                eq_low_db: lerp(BASS_CUT_DB, 0.0, swap),
                ..DeckAutomation::neutral()
            },
        }
    }

    /// Incoming fades in over the first 3/8; outgoing holds until 5/8 then
    /// fades over the remaining 3/8. Filters sweep across the full duration.
    fn filter_sweep(&self) -> Automation {
        let t = self.elapsed;
        let d = self.duration;
        let fade_duration = d * 3.0 / 8.0;
        let fade_delay = d * 5.0 / 8.0;
        let p = (t / d).clamp(0.0, 1.0);

        let in_gain = lerp(SILENCE_DB, 0.0, sine_in_out(t / fade_duration));
        let out_gain = if t < fade_delay {
            0.0
        } else {
            lerp(
                0.0,
                SILENCE_DB,
                sine_in_out((t - fade_delay) / fade_duration),
            )
        };

        // Linear in Hz with quad easing, matching the Godot `tween_property`
        // on `cutoff_hz`. A log sweep would be more perceptually even; that is
        // a tuning question for later, not a silent change here.
        let out_cutoff = lerp(20_000.0, 150.0, quad_in(p));
        let in_cutoff = lerp(2_000.0, 10.0, quad_out(p));

        Automation {
            outgoing: DeckAutomation {
                gain_db: out_gain,
                sweep: Some(Sweep::LowPass {
                    freq: out_cutoff,
                    q: 0.707,
                }),
                ..DeckAutomation::neutral()
            },
            incoming: DeckAutomation {
                gain_db: in_gain,
                sweep: Some(Sweep::HighPass {
                    freq: in_cutoff,
                    q: 0.707,
                }),
                ..DeckAutomation::neutral()
            },
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Godot `TRANS_SINE` with the default `EASE_IN_OUT`.
pub(crate) fn sine_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (1.0 - (std::f32::consts::PI * t).cos()) / 2.0
}

/// Godot `TRANS_QUAD` + `EASE_IN`.
fn quad_in(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

/// Godot `TRANS_QUAD` + `EASE_OUT`.
fn quad_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ports `test_bass_swap_volume_envelope`: for a 0.1 s transition, at
    /// 0.03 s the outgoing still holds 0 dB (midpoint is 0.05 s) while the
    /// incoming is already mid-fade.
    #[test]
    fn bass_swap_holds_outgoing_until_midpoint() {
        let mut t = Transition::new(TransitionType::BassSwap, 0.1);

        let a = t.automation();
        assert_eq!(a.outgoing.gain_db, 0.0, "outgoing starts at 0 dB");
        assert_eq!(a.incoming.gain_db, SILENCE_DB, "incoming starts at -60 dB");

        t.advance(0.03);
        let a = t.automation();
        assert_eq!(a.outgoing.gain_db, 0.0, "outgoing holds before midpoint");
        assert!(
            a.incoming.gain_db > SILENCE_DB && a.incoming.gain_db < 0.0,
            "incoming should be mid-fade, got {}",
            a.incoming.gain_db
        );
    }

    /// Ports `test_filter_sweep_volume_envelope`: for a 0.1 s transition the
    /// outgoing fade is delayed by 5/8 = 0.0625 s, so at 0.03 s it still
    /// holds 0 dB.
    #[test]
    fn filter_sweep_delays_outgoing_fade_by_five_eighths() {
        let mut t = Transition::new(TransitionType::FilterSweep, 0.1);

        let a = t.automation();
        assert_eq!(a.outgoing.gain_db, 0.0);
        assert_eq!(a.incoming.gain_db, SILENCE_DB);

        t.advance(0.03); // < 0.0625 s delay
        let a = t.automation();
        assert_eq!(a.outgoing.gain_db, 0.0, "outgoing holds before the delay");
        assert!(
            a.incoming.gain_db > SILENCE_DB && a.incoming.gain_db < 0.0,
            "incoming should be mid-fade, got {}",
            a.incoming.gain_db
        );

        // Just past the delay it must start moving.
        t.advance(0.04); // now 0.07 s
        let a = t.automation();
        assert!(
            a.outgoing.gain_db < 0.0,
            "outgoing should be falling after the delay, got {}",
            a.outgoing.gain_db
        );
    }

    #[test]
    fn bass_swap_swaps_low_bands_quickly_at_the_midpoint() {
        let mut t = Transition::new(TransitionType::BassSwap, 6.0);

        t.advance(2.9); // before midpoint
        let a = t.automation();
        assert_eq!(a.outgoing.eq_low_db, 0.0, "outgoing bass intact pre-swap");
        assert_eq!(
            a.incoming.eq_low_db, BASS_CUT_DB,
            "incoming bass held cut pre-swap"
        );

        t.advance(0.7); // 3.6 s: 0.6 s past the midpoint, swap is 0.5 s
        let a = t.automation();
        assert!(
            a.outgoing.eq_low_db <= BASS_CUT_DB + 0.01,
            "outgoing bass cut"
        );
        assert!(a.incoming.eq_low_db >= -0.01, "incoming bass restored");
    }

    #[test]
    fn filter_sweep_closes_and_opens_the_filters() {
        let mut t = Transition::new(TransitionType::FilterSweep, 4.0);
        let a = t.automation();
        let (start_lp, start_hp) = sweep_freqs(&a);

        t.advance(4.0);
        let a = t.automation();
        let (end_lp, end_hp) = sweep_freqs(&a);

        assert!(
            start_lp > 19_000.0 && end_lp < 200.0,
            "lowpass should close"
        );
        assert!(
            start_hp > 1_900.0 && end_hp < 20.0,
            "highpass should open down"
        );
    }

    fn sweep_freqs(a: &Automation) -> (f32, f32) {
        let lp = match a.outgoing.sweep {
            Some(Sweep::LowPass { freq, .. }) => freq,
            _ => panic!("expected a lowpass on the outgoing deck"),
        };
        let hp = match a.incoming.sweep {
            Some(Sweep::HighPass { freq, .. }) => freq,
            _ => panic!("expected a highpass on the incoming deck"),
        };
        (lp, hp)
    }

    #[test]
    fn every_transition_ends_fully_swapped() {
        for kind in [
            TransitionType::StandardCrossfade,
            TransitionType::BassSwap,
            TransitionType::FilterSweep,
        ] {
            let mut t = Transition::new(kind, 2.0);
            t.advance(2.0);
            assert!(t.is_complete(), "{kind:?} should be complete");
            let a = t.automation();
            assert!(
                a.outgoing.gain_db <= SILENCE_DB + 0.01,
                "{kind:?} outgoing should end silent, got {}",
                a.outgoing.gain_db
            );
            assert!(
                a.incoming.gain_db > -0.5,
                "{kind:?} incoming should end at full level, got {}",
                a.incoming.gain_db
            );
        }
    }

    /// Documents a property inherited from the Godot implementation rather
    /// than a bug introduced here: a dB-linear crossfade of two uncorrelated
    /// sources loses summed power at the midpoint. Recorded so that any future
    /// switch to equal-power is a deliberate, visible change.
    #[test]
    fn the_crossfade_holds_its_level_all_the_way_through() {
        // The defect this replaces: the Godot envelope interpolated both gains
        // linearly in dB, so at the midpoint both decks sat at -30 dB and the
        // summed power fell to about 0.002 — a hole in the middle of every mix.
        let duration = 4.0;
        let mut t = Transition::new(TransitionType::StandardCrossfade, duration);

        let mut worst: f32 = 0.0;
        let steps = 200;
        for _ in 0..=steps {
            let a = t.automation();
            let g_out = 10f32.powf(a.outgoing.gain_db / 20.0);
            let g_in = 10f32.powf(a.incoming.gain_db / 20.0);
            let power = g_out * g_out + g_in * g_in;
            worst = worst.max((power - 1.0).abs());
            t.advance(duration / steps as f32);
        }

        // Equal power means the squares sum to one at *every* instant, not just
        // at the midpoint — a law that only held in the middle would still dip
        // either side of it.
        assert!(
            worst < 0.02,
            "summed power strayed from unity by {worst:.4}; the crossfade is \
             not equal-power"
        );
    }

    /// The ends still have to be the ends: full one side, silent the other.
    #[test]
    fn the_crossfade_still_starts_and_finishes_where_it_should() {
        let mut t = Transition::new(TransitionType::StandardCrossfade, 2.0);

        let a = t.automation();
        assert!(a.outgoing.gain_db > -0.1, "outgoing did not start at full");
        assert_eq!(
            a.incoming.gain_db, SILENCE_DB,
            "incoming did not start silent"
        );

        t.advance(2.0);
        let a = t.automation();
        assert_eq!(
            a.outgoing.gain_db, SILENCE_DB,
            "outgoing did not finish silent"
        );
        assert!(a.incoming.gain_db > -0.1, "incoming did not finish at full");
    }

    /// Neither deck may get louder than it started — an equal-power law that
    /// achieved unity by boosting would clip a mix rather than fix it.
    #[test]
    fn the_crossfade_never_boosts_either_deck() {
        let duration = 3.0;
        let mut t = Transition::new(TransitionType::StandardCrossfade, duration);
        for _ in 0..=120 {
            let a = t.automation();
            assert!(
                a.outgoing.gain_db <= 0.001,
                "outgoing boosted to {}",
                a.outgoing.gain_db
            );
            assert!(
                a.incoming.gain_db <= 0.001,
                "incoming boosted to {}",
                a.incoming.gain_db
            );
            t.advance(duration / 120.0);
        }
    }
}

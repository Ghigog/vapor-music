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

/// Bounds a phrase-matched transition is held to, from
/// `clampf(overlap, 4.0, 16.0)`. Shorter than four seconds is a cut rather than
/// a mix; longer than sixteen outlasts most intros.
const MIN_PHRASE_SECS: f32 = 4.0;
const MAX_PHRASE_SECS: f32 = 16.0;

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

    /// How long a mix should run given the room the two tracks leave for it
    /// (TD-21).
    ///
    /// Ported from the `smart_mixing_enabled` branch of
    /// `get_transition_duration`. The overlap a mix has to work with is the
    /// shorter of the outgoing track's outro and the incoming track's intro;
    /// that is snapped **down** to a standard phrase — 16, 8 or 4 bars — so the
    /// transition ends on a musical boundary rather than wherever the clock ran
    /// out.
    ///
    /// `None` when the tracks give nothing to work with, and the caller falls
    /// back to [`default_duration`](Self::default_duration). Note the original
    /// derives the bar length from the **outgoing** tempo, which is the one
    /// both decks are playing at during the mix.
    pub fn phrase_duration(outro_len: f32, intro_len: f32, outgoing_bpm: f32) -> Option<f32> {
        if outro_len <= 0.0 || intro_len <= 0.0 {
            return None;
        }
        let bpm = if outgoing_bpm > 0.0 {
            outgoing_bpm
        } else {
            120.0
        };

        // Four beats to a bar, so a bar is 240/bpm seconds.
        let bar = 240.0 / bpm;
        let overlap = outro_len
            .min(intro_len)
            .clamp(MIN_PHRASE_SECS, MAX_PHRASE_SECS);

        let chosen = [16.0, 8.0, 4.0]
            .iter()
            .map(|bars| bars * bar)
            .find(|&candidate| candidate <= overlap)
            // Every phrase is longer than the room available, so take the
            // floor rather than overrunning the intro.
            .unwrap_or(MIN_PHRASE_SECS);

        Some(chosen.clamp(MIN_PHRASE_SECS, MAX_PHRASE_SECS))
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
    /// Duck the outgoing track's mids so the incoming vocal has room (TD-21).
    ///
    /// `audio_manager.gd` applies this to **every** transition type, and only
    /// when *both* tracks have vocals — two singers over each other is the
    /// clash it exists to prevent, and ducking a track that has none only makes
    /// the mix quieter.
    mid_cut: bool,
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
/// Depth the outgoing mids duck to when both tracks have vocals (TD-21).
///
/// `apply_mid_cut` writes `_transition_eq_gains[out_bus][2]` and `[3]`. Godot's
/// `AudioEffectEQ6` centres its six bands at 32, 100, 320, 1000, 3200 and
/// 10000 Hz, so bands 2 and 3 are 320 and 1000 — the **mid** band here, and
/// only that one. An earlier port applied this to the high band as well, which
/// takes 24 dB out of the outgoing track's entire top end.
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
            mid_cut: false,
        }
    }

    /// Duck the outgoing mids across the first half of the mix.
    ///
    /// Set when both tracks have vocals — see [`Transition::mid_cut`]. The
    /// caller decides, because whether a track has vocals is a property of the
    /// analysis and not of the envelope.
    pub fn with_mid_cut(mut self, apply: bool) -> Self {
        self.mid_cut = apply;
        self
    }

    /// The outgoing mid duck at this instant, in dB. Zero when not applied.
    ///
    /// `0 -> -24 dB` over the first half of the transition, from
    /// `apply_mid_cut`'s `tween_method(..., 0.0, -24.0, duration * 0.5)`.
    fn mid_cut_db(&self) -> f32 {
        if !self.mid_cut {
            return 0.0;
        }
        let half = self.duration * 0.5;
        lerp(0.0, MID_CUT_DB, (self.elapsed / half).clamp(0.0, 1.0))
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
        let mut a = match self.kind {
            TransitionType::StandardCrossfade => self.standard_crossfade(),
            TransitionType::BassSwap => self.bass_swap(),
            TransitionType::FilterSweep => self.filter_sweep(),
            TransitionType::EchoOut => self.echo_out(),
            TransitionType::ReverbFreeze => self.reverb_freeze(),
            TransitionType::TempoMorph => self.tempo_morph(),
        };

        // Applied here rather than inside each envelope, because in
        // `audio_manager.gd` it is a separate `if apply_mid_cut` block appended
        // to every one of the six — the same duck regardless of which
        // transition it decorates. Added to whatever the envelope already asked
        // for, so a Bass Swap's own EQ automation still stands.
        a.outgoing.eq_mid_db += self.mid_cut_db();
        a
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

        Automation {
            outgoing: DeckAutomation {
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

    /// The duck is off unless someone asks for it. `apply_mid_cut` is gated on
    /// *both* tracks having vocals, and an earlier port applied it to every
    /// Echo Out regardless — which quietens the outgoing track through the
    /// first half of a mix for no reason at all.
    #[test]
    fn the_mid_cut_is_off_by_default() {
        for kind in [
            TransitionType::StandardCrossfade,
            TransitionType::BassSwap,
            TransitionType::FilterSweep,
            TransitionType::EchoOut,
            TransitionType::ReverbFreeze,
            TransitionType::TempoMorph,
        ] {
            let mut t = Transition::new(kind, 4.0);
            t.advance(2.0);
            assert_eq!(
                t.automation().outgoing.eq_mid_db,
                0.0,
                "{kind:?} ducked the outgoing mids with no vocal clash to justify it"
            );
        }
    }

    /// And it applies to *every* transition type when it is asked for.
    /// `audio_manager.gd` appends the same `if apply_mid_cut` block to all six.
    #[test]
    fn the_mid_cut_applies_to_every_transition_type() {
        for kind in [
            TransitionType::StandardCrossfade,
            TransitionType::BassSwap,
            TransitionType::FilterSweep,
            TransitionType::EchoOut,
            TransitionType::ReverbFreeze,
            TransitionType::TempoMorph,
        ] {
            let duration = 4.0;
            let mut t = Transition::new(kind, duration).with_mid_cut(true);

            assert_eq!(
                t.automation().outgoing.eq_mid_db,
                0.0,
                "{kind:?} started the duck before the mix began"
            );

            // Fully applied at the midpoint, which is where the ramp ends.
            t.advance(duration * 0.5);
            let at_half = t.automation().outgoing.eq_mid_db;
            assert!(
                (at_half - MID_CUT_DB).abs() < 0.01,
                "{kind:?} reached {at_half} dB at the midpoint, expected {MID_CUT_DB}"
            );

            // And stays there rather than recovering.
            t.advance(duration * 0.5);
            let at_end = t.automation().outgoing.eq_mid_db;
            assert!(
                (at_end - MID_CUT_DB).abs() < 0.01,
                "{kind:?} let the duck recover to {at_end} dB before the mix ended"
            );
        }
    }

    /// The duck is on the **mid** band alone.
    ///
    /// `apply_mid_cut` writes `_transition_eq_gains[out_bus][2]` and `[3]`,
    /// which in Godot's six-band EQ are 320 Hz and 1 kHz. An earlier port also
    /// pulled the high band down, taking 24 dB out of the outgoing track's
    /// entire top end — audible as the mix going dull, not as room being made.
    #[test]
    fn the_mid_cut_leaves_the_high_band_alone() {
        let mut t = Transition::new(TransitionType::EchoOut, 4.0).with_mid_cut(true);
        t.advance(2.0);
        let a = t.automation();

        assert!(a.outgoing.eq_mid_db < -20.0, "the mid band was not ducked");
        assert_eq!(
            a.outgoing.eq_high_db, 0.0,
            "the duck reached the high band, which is two bands too far"
        );
    }

    /// It must never duck the *incoming* track — the whole point is to make
    /// room for the vocal that is arriving.
    #[test]
    fn the_mid_cut_never_touches_the_incoming_deck() {
        let mut t = Transition::new(TransitionType::StandardCrossfade, 4.0).with_mid_cut(true);
        for _ in 0..8 {
            t.advance(0.5);
            assert_eq!(
                t.automation().incoming.eq_mid_db,
                0.0,
                "the arriving track was ducked"
            );
        }
    }

    /// A Bass Swap already automates the outgoing low band. The duck has to add
    /// to that rather than replace it, or applying it would undo the swap.
    #[test]
    fn the_mid_cut_leaves_a_transitions_own_eq_intact() {
        let duration = 4.0;
        let mut plain = Transition::new(TransitionType::BassSwap, duration);
        let mut ducked = Transition::new(TransitionType::BassSwap, duration).with_mid_cut(true);

        plain.advance(duration * 0.75);
        ducked.advance(duration * 0.75);

        let (a, b) = (plain.automation(), ducked.automation());
        assert_eq!(
            a.outgoing.eq_low_db, b.outgoing.eq_low_db,
            "the duck disturbed the Bass Swap's own low-band automation"
        );
        assert!(
            b.outgoing.eq_mid_db < a.outgoing.eq_mid_db,
            "the duck was not applied on top"
        );
    }

    /// A mix lands on a phrase boundary rather than on a round number of
    /// seconds. At 128 BPM a bar is 1.875 s, so the three candidates are 30,
    /// 15 and 7.5 s — and which one fits is decided by the room the tracks
    /// leave.
    #[test]
    fn a_phrase_duration_is_a_whole_number_of_bars() {
        let bpm = 128.0;
        let bar = 240.0 / bpm;

        // Sixteen bars is 30 s, past the 16 s ceiling; eight is 15 s and fits.
        let d = TransitionType::phrase_duration(40.0, 40.0, bpm).expect("a duration");
        assert!(
            (d / bar - 8.0).abs() < 1e-4,
            "{d:.3}s is {:.2} bars, expected 8",
            d / bar
        );

        // A short intro forces the next phrase down.
        let d = TransitionType::phrase_duration(40.0, 9.0, bpm).expect("a duration");
        assert!(
            (d / bar - 4.0).abs() < 1e-4,
            "{d:.3}s is {:.2} bars, expected 4",
            d / bar
        );
    }

    /// The *shorter* of the two spans decides, because a mix cannot run past
    /// the end of the incoming track's intro or the outgoing track's outro.
    #[test]
    fn the_tighter_of_the_two_spans_decides() {
        let bpm = 120.0;
        let generous = TransitionType::phrase_duration(60.0, 60.0, bpm).expect("a duration");
        let tight = TransitionType::phrase_duration(60.0, 5.0, bpm).expect("a duration");
        assert!(
            tight <= generous,
            "a five-second intro produced a {tight:.2}s mix against {generous:.2}s for a long one"
        );
    }

    /// Never longer than the room available, which is the whole point — a mix
    /// that outruns the intro plays the incoming track's first verse under the
    /// outgoing one's.
    #[test]
    fn a_phrase_never_outruns_the_room_it_was_given() {
        for (outro, intro, bpm) in [
            (30.0f32, 6.0f32, 128.0f32),
            (8.0, 40.0, 100.0),
            (20.0, 20.0, 174.0),
            (5.0, 5.0, 90.0),
        ] {
            let d = TransitionType::phrase_duration(outro, intro, bpm).expect("a duration");
            let room = outro.min(intro).max(MIN_PHRASE_SECS);
            assert!(
                d <= room + 1e-4,
                "a {d:.2}s mix was chosen for {room:.2}s of room at {bpm} BPM"
            );
        }
    }

    /// Always inside the bounds the original clamps to.
    #[test]
    fn a_phrase_stays_within_four_and_sixteen_seconds() {
        for bpm in [60.0f32, 90.0, 128.0, 174.0, 200.0] {
            for span in [1.0f32, 4.0, 10.0, 30.0, 300.0] {
                let d = TransitionType::phrase_duration(span, span, bpm).expect("a duration");
                assert!(
                    (MIN_PHRASE_SECS..=MAX_PHRASE_SECS).contains(&d),
                    "{d:.2}s at {bpm} BPM with {span}s of room is outside 4..16"
                );
            }
        }
    }

    /// Tracks that give nothing to work with fall back rather than inventing a
    /// number — an unanalysed track has no segments, and a zero-length outro
    /// means the detector found no body at all.
    #[test]
    fn no_room_means_no_opinion() {
        assert_eq!(TransitionType::phrase_duration(0.0, 10.0, 128.0), None);
        assert_eq!(TransitionType::phrase_duration(10.0, 0.0, 128.0), None);
        assert_eq!(TransitionType::phrase_duration(-1.0, 10.0, 128.0), None);
    }

    /// A missing tempo must not divide by zero or produce a nonsense bar.
    #[test]
    fn a_missing_tempo_falls_back_to_a_sane_bar() {
        let d = TransitionType::phrase_duration(60.0, 60.0, 0.0).expect("a duration");
        assert!((MIN_PHRASE_SECS..=MAX_PHRASE_SECS).contains(&d));
    }
}

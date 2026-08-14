//! Two-deck mixer with beat-matched transitions.
//!
//! This is the component the migration actually risks, and the reason for this
//! crate. Everything else has an off-the-shelf answer; a sample-accurate DJ
//! mixer does not.
//!
//! ## How beat-matching works here
//!
//! Given the outgoing track's beat grid and BPM, and the incoming track's,
//! a transition needs three things to line up:
//!
//! 1. **Tempo.** The incoming deck's stretch ratio is set to
//!    `bpm_out / bpm_in`, so both decks run at the outgoing tempo. Refused
//!    beyond [`MAX_STRETCH`] — past roughly ±6% WSOLA artefacts become audible
//!    and, more importantly, the mix stops sounding like the record.
//! 2. **Phase.** The incoming deck is *seeked* so that one of its beats lands
//!    exactly on an outgoing beat at the transition start. This is the part the
//!    Godot build approximates with a PLL correcting drift after the fact; here
//!    it is solved up front and the PLL becomes a correction, not the mechanism.
//! 3. **Clock.** Both decks advance from the same audio clock — the number of
//!    frames rendered — so once aligned they cannot drift.
//!
//! The Godot version polls `get_playback_position()` from `_process` at frame
//! rate, which means alignment is only as good as the frame scheduler. That is
//! the structural reason transitions there need drift correction at all.

use crate::biquad::Sweep;
use crate::deck::Deck;
use crate::transition::{Transition, TransitionType};

/// Maximum tempo adjustment. The Godot build ramps `_speed_scale_*` by 1–2%
/// for beat sync and up to ~6% for a Tempo Morph.
pub const MAX_STRETCH: f64 = 0.06;

pub struct BeatGrid {
    pub bpm: f32,
    /// Beat onset times in seconds.
    pub beats: Vec<f32>,
}

impl BeatGrid {
    pub fn beat_period(&self) -> f32 {
        if self.bpm > 0.0 {
            60.0 / self.bpm
        } else {
            0.5
        }
    }

    /// The first beat at or after `t`.
    pub fn beat_at_or_after(&self, t: f32) -> Option<f32> {
        self.beats.iter().copied().find(|&b| b >= t)
    }
}

/// Why a beat-matched transition could not be set up.
#[derive(Debug, PartialEq, Eq)]
pub enum MatchError {
    /// Tempo difference exceeds what stretching can bridge musically.
    TempoTooFar,
    /// A track has no usable grid — unanalyzed, as the DSP stub now reports
    /// honestly rather than fabricating 120 BPM.
    NoGrid,
}

pub struct Mixer {
    pub deck_a: Deck,
    pub deck_b: Deck,
    /// True when deck A is the outgoing deck.
    a_is_outgoing: bool,
    transition: Option<Transition>,
    sample_rate: f32,
    scratch: Vec<[f32; 2]>,
}

impl Mixer {
    pub fn new(sample_rate: f32, max_block: usize) -> Self {
        Mixer {
            deck_a: Deck::new(sample_rate),
            deck_b: Deck::new(sample_rate),
            a_is_outgoing: true,
            transition: None,
            sample_rate,
            scratch: vec![[0.0; 2]; max_block],
        }
    }

    pub fn outgoing(&mut self) -> &mut Deck {
        if self.a_is_outgoing {
            &mut self.deck_a
        } else {
            &mut self.deck_b
        }
    }

    pub fn incoming(&mut self) -> &mut Deck {
        if self.a_is_outgoing {
            &mut self.deck_b
        } else {
            &mut self.deck_a
        }
    }

    pub fn is_transitioning(&self) -> bool {
        self.transition.is_some()
    }

    pub fn transition_progress(&self) -> f32 {
        self.transition.as_ref().map_or(0.0, |t| t.progress())
    }

    /// Compute the tempo ratio needed to run `incoming` at `outgoing`'s tempo.
    ///
    /// The ratio is source frames consumed per output frame, so a *slower*
    /// incoming track needs a ratio **above** 1.0 — it must be played faster to
    /// keep up. Writing this the intuitive way round (`incoming / outgoing`)
    /// produces beats that drift to the offbeat within a few bars.
    pub fn tempo_ratio(outgoing: &BeatGrid, incoming: &BeatGrid) -> Result<f64, MatchError> {
        if outgoing.bpm <= 0.0 || incoming.bpm <= 0.0 {
            return Err(MatchError::NoGrid);
        }
        let ratio = outgoing.bpm as f64 / incoming.bpm as f64;
        if (ratio - 1.0).abs() > MAX_STRETCH {
            return Err(MatchError::TempoTooFar);
        }
        Ok(ratio)
    }

    /// Seek position for the incoming deck so its beat lands on the outgoing
    /// beat at `start_time_out`.
    ///
    /// Returns the incoming position in seconds. `cue_in` is where the incoming
    /// track should start musically — the first beat at or after it is the one
    /// that gets aligned.
    pub fn aligned_incoming_position(
        outgoing: &BeatGrid,
        incoming: &BeatGrid,
        start_time_out: f32,
        cue_in: f32,
    ) -> Result<f32, MatchError> {
        let ratio = Self::tempo_ratio(outgoing, incoming)? as f32;

        // The outgoing beat the transition starts on.
        let out_beat = outgoing
            .beat_at_or_after(start_time_out)
            .ok_or(MatchError::NoGrid)?;
        // The incoming beat we want to hear at that instant.
        let in_beat = incoming
            .beat_at_or_after(cue_in)
            .ok_or(MatchError::NoGrid)?;

        // `lead` is how long, in *output* time, until that outgoing beat sounds.
        // The incoming deck consumes `ratio` source seconds per output second,
        // so backing up by `lead` in output time means backing up `lead * ratio`
        // in the incoming track's own timeline. Omitting the ratio leaves an
        // error proportional to the tempo difference, which reads as a beat
        // that is almost — but not quite — locked.
        let lead = out_beat - start_time_out;
        Ok((in_beat - lead * ratio).max(0.0))
    }

    /// Begin a beat-matched transition.
    #[allow(clippy::too_many_arguments)]
    pub fn start_transition(
        &mut self,
        kind: TransitionType,
        duration: f32,
        outgoing_grid: &BeatGrid,
        incoming_grid: &BeatGrid,
        start_time_out: f32,
        cue_in: f32,
    ) -> Result<(), MatchError> {
        let ratio = Self::tempo_ratio(outgoing_grid, incoming_grid)?;
        let pos = Self::aligned_incoming_position(
            outgoing_grid,
            incoming_grid,
            start_time_out,
            cue_in,
        )?;

        let inc = self.incoming();
        inc.seek_seconds(pos as f64);
        inc.ratio = ratio;
        inc.set_gain_db(-60.0);
        inc.play();

        self.transition = Some(Transition::new(kind, duration));
        Ok(())
    }

    /// Render one block. Returns frames written.
    ///
    /// Automation is sampled **once per block**, not per sample: coefficient
    /// recomputation is the expensive part and a block is short enough
    /// (typically 256–1024 frames, 6–23 ms) that the stepping is inaudible.
    pub fn render(&mut self, out: &mut [[f32; 2]]) -> usize {
        for s in out.iter_mut() {
            *s = [0.0; 2];
        }
        let block = out.len().min(self.scratch.len());
        let dt = block as f32 / self.sample_rate;

        if let Some(t) = &mut self.transition {
            t.advance(dt);
            let a = t.automation();
            let complete = t.is_complete();

            let a_out = self.a_is_outgoing;
            {
                let (o, i) = if a_out {
                    (&mut self.deck_a, &mut self.deck_b)
                } else {
                    (&mut self.deck_b, &mut self.deck_a)
                };
                apply(o, &a.outgoing);
                apply(i, &a.incoming);
            }

            if complete {
                self.transition = None;
                let o = self.outgoing();
                o.stop();
                self.a_is_outgoing = !a_out;
                let now_playing = self.outgoing();
                now_playing.set_gain_db(0.0);
                now_playing.set_eq_db(0.0, 0.0, 0.0);
                now_playing.set_sweep(None);
                // Tempo returns to the track's own after the mix completes —
                // the "post-transition speed glide" in test_tempo_stretching.
                now_playing.ratio = 1.0;
            }
        }

        let mut produced = 0;
        {
            let scratch = &mut self.scratch[..block];
            produced = produced.max(self.deck_a.render_additive(&mut out[..block], scratch));
        }
        {
            let scratch = &mut self.scratch[..block];
            produced = produced.max(self.deck_b.render_additive(&mut out[..block], scratch));
        }

        // Hard-clip guard. The Godot build runs three-band RMS clipping
        // prevention on the master; this is only a safety net so a bug cannot
        // produce a speaker-damaging spike.
        for s in out[..block].iter_mut() {
            for v in s.iter_mut() {
                *v = v.clamp(-1.0, 1.0);
            }
        }

        produced
    }
}

fn apply(deck: &mut Deck, a: &crate::transition::DeckAutomation) {
    deck.set_gain_db(a.gain_db);
    deck.set_eq_db(a.eq_low_db, a.eq_mid_db, a.eq_high_db);
    deck.set_sweep(a.sweep);
}

/// Convenience for callers that only have a Sweep to clear.
pub fn no_sweep() -> Option<Sweep> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(bpm: f32, start: f32, count: usize) -> BeatGrid {
        let period = 60.0 / bpm;
        BeatGrid {
            bpm,
            beats: (0..count).map(|i| start + i as f32 * period).collect(),
        }
    }

    #[test]
    fn tempo_ratio_speeds_up_a_slower_incoming_track() {
        let out = grid(128.0, 0.0, 100);
        let inc = grid(126.0, 0.0, 100);
        let r = Mixer::tempo_ratio(&out, &inc).unwrap();
        assert!((r - 128.0 / 126.0).abs() < 1e-6, "got {r}");
        assert!(r > 1.0, "a slower incoming track must be played faster");
    }

    #[test]
    fn tempo_ratio_slows_down_a_faster_incoming_track() {
        let out = grid(124.0, 0.0, 100);
        let inc = grid(128.0, 0.0, 100);
        let r = Mixer::tempo_ratio(&out, &inc).unwrap();
        assert!(r < 1.0, "a faster incoming track must be played slower");
    }

    /// Independent of the arithmetic: after stretching, one beat period of the
    /// incoming track must occupy exactly one beat period of the outgoing
    /// track in output time. This is the property the ratio exists to satisfy.
    #[test]
    fn stretched_incoming_beat_period_equals_the_outgoing_period() {
        for &in_bpm in &[122.0f32, 126.0, 130.0, 133.0] {
            let out = grid(128.0, 0.0, 100);
            let inc = grid(in_bpm, 0.0, 100);
            let ratio = Mixer::tempo_ratio(&out, &inc).unwrap() as f32;

            let output_period = inc.beat_period() / ratio;
            assert!(
                (output_period - out.beat_period()).abs() < 1e-5,
                "{in_bpm} BPM stretched to a {output_period:.5}s period, \
                 outgoing is {:.5}s",
                out.beat_period()
            );
        }
    }

    #[test]
    fn refuses_musically_implausible_stretches() {
        let out = grid(128.0, 0.0, 100);
        let far = grid(175.0, 0.0, 100);
        assert_eq!(
            Mixer::tempo_ratio(&out, &far),
            Err(MatchError::TempoTooFar)
        );
    }

    /// Unanalyzed tracks must be refused, not silently mixed at a guessed
    /// tempo — the same principle as the DSP stub no longer fabricating
    /// 120 BPM.
    #[test]
    fn refuses_tracks_with_no_grid() {
        let out = grid(128.0, 0.0, 100);
        let none = BeatGrid {
            bpm: 0.0,
            beats: vec![],
        };
        assert_eq!(Mixer::tempo_ratio(&out, &none), Err(MatchError::NoGrid));
        assert_eq!(Mixer::tempo_ratio(&none, &out), Err(MatchError::NoGrid));
    }

    /// The core scheduling property: after alignment, the incoming track's
    /// beat is heard at the same instant as the outgoing track's beat.
    #[test]
    fn alignment_puts_beats_on_the_same_instant() {
        let out = grid(128.0, 0.10, 200);
        let inc = grid(126.0, 0.37, 200);

        // Start somewhere that is deliberately not on a beat.
        let start = 30.0;
        let ratio = Mixer::tempo_ratio(&out, &inc).unwrap() as f32;
        let pos = Mixer::aligned_incoming_position(&out, &inc, start, 8.0).unwrap();

        let out_beat = out.beat_at_or_after(start).unwrap();
        let in_beat = inc.beat_at_or_after(8.0).unwrap();

        // Both delays must be expressed in *output* time. The incoming deck's
        // source clock runs at `ratio`, so its source-time delay divides down.
        let out_delay = out_beat - start;
        let in_delay = (in_beat - pos) / ratio;

        assert!(
            (out_delay - in_delay).abs() < 1e-4,
            "beats land {:.4}s apart (out {out_delay:.4}s, in {in_delay:.4}s)",
            (out_delay - in_delay).abs()
        );
    }

    /// Alignment must hold regardless of where in the bar the transition is
    /// triggered — the failure mode is an error that grows with phase offset.
    #[test]
    fn alignment_holds_at_every_phase_offset() {
        let out = grid(128.0, 0.0, 400);
        let inc = grid(130.0, 0.0, 400);
        let period = out.beat_period();
        let ratio = Mixer::tempo_ratio(&out, &inc).unwrap() as f32;

        for i in 0..40 {
            let start = 40.0 + period * i as f32 / 40.0;
            let pos = Mixer::aligned_incoming_position(&out, &inc, start, 10.0).unwrap();

            let out_delay = out.beat_at_or_after(start).unwrap() - start;
            let in_delay = (inc.beat_at_or_after(10.0).unwrap() - pos) / ratio;

            assert!(
                (out_delay - in_delay).abs() < 1e-4,
                "phase offset {i}: {:.5}s apart",
                (out_delay - in_delay).abs()
            );
        }
    }
}

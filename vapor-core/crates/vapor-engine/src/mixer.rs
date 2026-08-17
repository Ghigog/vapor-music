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
use crate::clipping;
use crate::deck::Deck;
use crate::limiter::Limiter;
use crate::stretch::Quality;
use crate::transition::{sine_in_out, Transition, TransitionType};

/// Maximum tempo adjustment. The Godot build ramps `_speed_scale_*` by 1–2%
/// for beat sync and up to ~6% for a Tempo Morph.
pub const MAX_STRETCH: f64 = 0.06;

/// How long the incoming deck takes to return to its own tempo after a mix.
///
/// From `audio_manager.gd`, which glides `_speed_scale_*` back to 1.0 over 6 s
/// with sine easing rather than snapping. Snapping is audible: the track jumps
/// tempo the instant the mix ends, which is precisely the seam a beat-matched
/// transition exists to hide.
pub const TEMPO_GLIDE_SECS: f32 = 6.0;

/// Ratio deviation below which no glide is worth scheduling.
const GLIDE_EPSILON: f64 = 1e-4;

/// A transition that has been decided but has not begun.
///
/// Real transitions are scheduled ahead of time — the Godot build passes a
/// `target_trigger_time` for the same reason. Deciding now and starting later
/// also keeps the decision (which can fail, and which reads the beat grids) off
/// the audio thread, leaving only the activation itself in the render path.
struct PendingTransition {
    kind: TransitionType,
    duration: f32,
    /// Both tracks have vocals, so the outgoing mids duck (TD-21).
    mid_cut: bool,
    incoming_pos: f32,
    ratio: f64,
    /// Stretch applied to the *outgoing* deck.
    ///
    /// 1.0 for every transition but Tempo Morph, where both tracks bend toward
    /// a tempo between them rather than one being dragged to the other.
    outgoing_ratio: f64,
    delay_frames: usize,
}

/// Returns the incoming deck to its own tempo after a transition completes.
struct TempoGlide {
    from: f64,
    elapsed: f32,
    duration: f32,
}

impl TempoGlide {
    fn ratio(&self) -> f64 {
        let t = sine_in_out(self.elapsed / self.duration) as f64;
        self.from + (1.0 - self.from) * t
    }

    fn is_complete(&self) -> bool {
        self.elapsed >= self.duration
    }
}

#[derive(Clone)]
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
    pending: Option<PendingTransition>,
    glide: Option<TempoGlide>,
    sample_rate: f32,
    scratch: Vec<[f32; 2]>,
    limiter: Limiter,

    /// The incoming deck's stretch before drift correction.
    ///
    /// Kept apart from `Deck::ratio` because the PLL multiplies it every block
    /// (TD-21). Folding the correction into the deck's own ratio would
    /// compound: each block would correct the already-corrected value, and a
    /// steady 0.2% nudge would run away within seconds.
    incoming_base_ratio: f64,
    /// Latest correction from the PLL, which runs on a control thread.
    pll_adjustment: f64,
}

impl Mixer {
    pub fn new(sample_rate: f32, max_block: usize) -> Self {
        Mixer::with_quality(sample_rate, max_block, Quality::default())
    }

    /// A mixer whose decks use a named stretcher. See [`Deck::with_quality`].
    pub fn with_quality(sample_rate: f32, max_block: usize, quality: Quality) -> Self {
        Mixer {
            deck_a: Deck::with_quality(sample_rate, quality),
            deck_b: Deck::with_quality(sample_rate, quality),
            a_is_outgoing: true,
            transition: None,
            pending: None,
            glide: None,
            sample_rate,
            scratch: vec![[0.0; 2]; max_block],
            limiter: Limiter::new(sample_rate, max_block),
            incoming_base_ratio: 1.0,
            pll_adjustment: 0.0,
        }
    }

    /// Apply a drift correction to the incoming deck (TD-21).
    ///
    /// Computed by [`crate::pll`] on a control thread — the phase detector needs
    /// both beat grids, which must never reach the audio thread — so only this
    /// scalar crosses, exactly as with `schedule_prepared`.
    ///
    /// Clamped here as well as at the source: this is the value that multiplies
    /// a playback rate, and a caller that passed something wild would be heard
    /// rather than merely logged.
    pub fn set_pll_adjustment(&mut self, adjustment: f64) {
        self.pll_adjustment =
            adjustment.clamp(-crate::pll::MAX_ADJUSTMENT, crate::pll::MAX_ADJUSTMENT);
    }

    /// The correction currently applied.
    pub fn pll_adjustment(&self) -> f64 {
        self.pll_adjustment
    }

    /// Gain reduction currently applied by the master limiter, in dB.
    /// Persistent large values mean the mix is being driven too hard.
    pub fn limiter_reduction_db(&self) -> f32 {
        self.limiter.reduction_db()
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

    /// True while the active deck is easing back to its own tempo after a mix.
    pub fn is_gliding(&self) -> bool {
        self.glide.is_some()
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

    /// Begin a beat-matched transition immediately.
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
        self.schedule_transition(
            kind,
            duration,
            outgoing_grid,
            incoming_grid,
            start_time_out,
            cue_in,
            0,
            false,
        )
    }

    /// Decide a beat-matched transition now and begin it after `delay_frames`.
    ///
    /// The decision — reading both grids, computing the ratio and the aligned
    /// position, and failing if the tempi are too far apart — happens here, on
    /// the calling thread. Only the activation runs later in the render path.
    #[allow(clippy::too_many_arguments)]
    pub fn schedule_transition(
        &mut self,
        kind: TransitionType,
        duration: f32,
        outgoing_grid: &BeatGrid,
        incoming_grid: &BeatGrid,
        start_time_out: f32,
        cue_in: f32,
        delay_frames: usize,
        mid_cut: bool,
    ) -> Result<(), MatchError> {
        let ratio = Self::tempo_ratio(outgoing_grid, incoming_grid)?;
        let incoming_pos =
            Self::aligned_incoming_position(outgoing_grid, incoming_grid, start_time_out, cue_in)?;

        // Tempo Morph meets in the middle: the target is the mean of the two
        // tempi, so each deck travels half the distance. `audio_manager.gd`
        // sets `_incoming_base_pitch` to `target / bpm_in` for exactly this.
        let (ratio, outgoing_ratio) = if kind.morphs_tempo() {
            let target = (outgoing_grid.bpm + incoming_grid.bpm) as f64 / 2.0;
            (
                target / incoming_grid.bpm as f64,
                target / outgoing_grid.bpm as f64,
            )
        } else {
            (ratio, 1.0)
        };

        self.schedule_prepared(
            kind,
            duration,
            incoming_pos,
            ratio,
            outgoing_ratio,
            delay_frames,
            mid_cut,
        );
        Ok(())
    }

    /// Schedule a transition whose alignment has already been worked out.
    ///
    /// [`schedule_transition`](Self::schedule_transition) reads both beat grids
    /// to derive the tempo ratio and the incoming cue position. A player cannot
    /// do that where the mixer is actually driven: the grids are `Vec<f32>`, the
    /// mixer runs on the audio thread, and neither allocating nor freeing a
    /// `Vec` is allowed there.
    ///
    /// So the shell computes the alignment on a control thread —
    /// [`tempo_ratio`](Self::tempo_ratio) and
    /// [`aligned_incoming_position`](Self::aligned_incoming_position) are public
    /// and pure precisely so it can — and hands the two scalars here. No grid
    /// ever crosses the boundary.
    ///
    /// Infallible by construction: every way this could fail was already decided
    /// by the caller that produced `ratio`.
    #[allow(clippy::too_many_arguments)]
    pub fn schedule_prepared(
        &mut self,
        kind: TransitionType,
        duration: f32,
        incoming_pos: f32,
        ratio: f64,
        outgoing_ratio: f64,
        delay_frames: usize,
        mid_cut: bool,
    ) {
        let pending = PendingTransition {
            kind,
            duration,
            incoming_pos,
            ratio,
            outgoing_ratio,
            delay_frames,
            mid_cut,
        };

        if delay_frames == 0 {
            self.activate(pending);
        } else {
            self.pending = Some(pending);
        }
    }

    /// Abandon a transition, scheduled or in flight, and restore the outgoing
    /// deck.
    ///
    /// Needed because a transition is decided seconds before it is heard, and a
    /// person can pick a different track in between. Without this, an explicit
    /// choice would be interrupted moments later by a mix that was arranged
    /// before it was made.
    ///
    /// Cancelling a transition already under way is audible — the outgoing deck
    /// returns to full immediately. That is accepted: the only caller is a
    /// person choosing something else, and whatever is playing is about to be
    /// replaced anyway. Allocation-free, so it is safe to call from the render
    /// path.
    pub fn cancel_transition(&mut self) {
        self.pending = None;
        self.glide = None;
        self.pll_adjustment = 0.0;

        if self.transition.take().is_some() {
            let incoming = self.incoming();
            incoming.stop();
            incoming.set_gain_db(-60.0);
            incoming.ratio = 1.0;

            let outgoing = self.outgoing();
            outgoing.set_gain_db(0.0);
            outgoing.set_eq_db(0.0, 0.0, 0.0);
            outgoing.set_clip_attenuation(clipping::Bands::default());
            outgoing.set_sweep(None);
            outgoing.set_delay_mix(0.0);
            outgoing.set_reverb_mix(0.0);
            outgoing.ratio = 1.0;
        }
    }

    /// True when a transition has been decided but has not begun.
    pub fn has_pending_transition(&self) -> bool {
        self.pending.is_some()
    }

    /// Cue the incoming deck and start the envelope. Allocation-free.
    fn activate(&mut self, p: PendingTransition) {
        // A new mix starts uncorrected: the alignment was just computed, so
        // there is nothing yet to have drifted from.
        self.incoming_base_ratio = p.ratio;
        self.pll_adjustment = 0.0;

        let inc = self.incoming();
        inc.seek_seconds(p.incoming_pos as f64);
        inc.ratio = p.ratio;
        inc.set_gain_db(-60.0);
        inc.play();
        // Only a Tempo Morph moves the outgoing deck; for everything else this
        // is 1.0 and the assignment is a no-op.
        self.outgoing().ratio = p.outgoing_ratio;
        self.transition = Some(Transition::new(p.kind, p.duration).with_mid_cut(p.mid_cut));
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
            let corrected = self.incoming_base_ratio * (1.0 + self.pll_adjustment);
            {
                let (o, i) = if a_out {
                    (&mut self.deck_a, &mut self.deck_b)
                } else {
                    (&mut self.deck_b, &mut self.deck_a)
                };
                apply(o, &a.outgoing);
                apply(i, &a.incoming);
                // Re-derived from the base every block rather than nudged, so
                // the correction cannot compound (TD-21).
                i.ratio = corrected;
            }

            if complete {
                self.transition = None;
                // The mix is over; there is no incoming deck left to correct,
                // and the glide below owns the ratio from here.
                self.pll_adjustment = 0.0;
                let o = self.outgoing();
                o.stop();
                self.a_is_outgoing = !a_out;
                let now_playing = self.outgoing();
                now_playing.set_gain_db(0.0);
                now_playing.set_eq_db(0.0, 0.0, 0.0);
                now_playing.set_clip_attenuation(crate::clipping::Bands::default());
                now_playing.set_sweep(None);
                // The deck that just became the one playing must not keep the
                // effects the transition left switched on.
                now_playing.set_delay_mix(0.0);
                now_playing.set_reverb_mix(0.0);

                // Tempo returns to the track's own — but eased, not snapped.
                // See TEMPO_GLIDE_SECS and test_post_transition_speed_glide.
                let from = now_playing.ratio;
                if (from - 1.0).abs() > GLIDE_EPSILON {
                    self.glide = Some(TempoGlide {
                        from,
                        elapsed: 0.0,
                        duration: TEMPO_GLIDE_SECS,
                    });
                } else {
                    now_playing.ratio = 1.0;
                }
            }
        }

        // Start a scheduled transition once its lead-in has been rendered.
        if let Some(p) = &mut self.pending {
            if p.delay_frames <= block {
                let p = self.pending.take().expect("checked above");
                self.activate(p);
            } else {
                p.delay_frames -= block;
            }
        }

        // Advance the post-transition tempo glide. Runs after the transition
        // has ended, so it is deliberately outside the transition branch above.
        if let Some(g) = &mut self.glide {
            g.elapsed = (g.elapsed + dt).min(g.duration);
            let r = g.ratio();
            let done = g.is_complete();
            self.outgoing().ratio = if done { 1.0 } else { r };
            if done {
                self.glide = None;
            }
        }

        // Clipping guard (MIG-006). Applied only during a transition, which is
        // the only time two decks sum. Uses the previous block's measurement —
        // the same one-block lag the Godot implementation has, since the level
        // cannot be known until the block is rendered.
        if self.transition.is_some() {
            let a_out = self.a_is_outgoing;
            let (o, i) = if a_out {
                (&self.deck_a, &self.deck_b)
            } else {
                (&self.deck_b, &self.deck_a)
            };
            let atten = clipping::attenuation_db(
                o.last_rms(),
                i.last_rms(),
                o.gain(),
                i.gain(),
                o.eq_db(),
                i.eq_db(),
            );
            let outgoing = if a_out {
                &mut self.deck_a
            } else {
                &mut self.deck_b
            };
            outgoing.set_clip_attenuation(atten);
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

        // Master limiter. Two decks at full level sum past full scale on
        // transients even when their combined RMS is modest — measured crest
        // factor on real material is ~4x — so the three-band RMS guard above
        // cannot catch this and neither could the Godot original.
        self.limiter.process(&mut out[..block]);

        // Hard clamp remains as a backstop only. If it ever engages, the
        // limiter has been bypassed or a bug has produced a non-finite sample.
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
    deck.set_delay_mix(a.delay_mix);
    deck.set_reverb_mix(a.reverb_mix);
}

/// Convenience for callers that only have a Sweep to clear.
pub fn no_sweep() -> Option<Sweep> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 44_100.0;

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
        assert_eq!(Mixer::tempo_ratio(&out, &far), Err(MatchError::TempoTooFar));
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

    /// The scalars route must place the incoming deck exactly where the grid
    /// route does — otherwise a player using it would beat-match differently
    /// from every test that proves beat-matching works.
    #[test]
    fn preparing_by_hand_matches_scheduling_from_grids() {
        let out = grid(128.0, 0.1, 400);
        let inc = grid(126.0, 0.37, 400);
        let (start, cue) = (30.0, 8.0);

        let mut from_grids = Mixer::new(RATE, 512);
        from_grids
            .start_transition(TransitionType::BassSwap, 6.0, &out, &inc, start, cue)
            .expect("schedule");

        let ratio = Mixer::tempo_ratio(&out, &inc).expect("ratio");
        let pos = Mixer::aligned_incoming_position(&out, &inc, start, cue).expect("position");
        let mut prepared = Mixer::new(RATE, 512);
        prepared.schedule_prepared(TransitionType::BassSwap, 6.0, pos, ratio, 1.0, 0, false);

        assert!((from_grids.incoming().ratio - prepared.incoming().ratio).abs() < 1e-12);
        assert!(
            (from_grids.incoming().position_seconds() - prepared.incoming().position_seconds())
                .abs()
                < 1e-9,
            "the two routes cued the incoming deck to different positions"
        );
    }

    /// A person picking a different track must not be interrupted by a mix
    /// arranged before they chose.
    #[test]
    fn a_cancelled_transition_leaves_the_outgoing_deck_playing() {
        let out = grid(128.0, 0.0, 400);
        let inc = grid(126.0, 0.0, 400);

        let mut mixer = Mixer::new(RATE, 512);
        mixer
            .deck_a
            .load(vec![[16_384i16; 2]; (RATE * 60.0) as usize]);
        mixer
            .deck_b
            .load(vec![[16_384i16; 2]; (RATE * 60.0) as usize]);
        mixer.deck_a.play();
        mixer
            .start_transition(TransitionType::BassSwap, 6.0, &out, &inc, 10.0, 1.0)
            .expect("schedule");
        assert!(mixer.is_transitioning());

        let mut block = [[0.0f32; 2]; 512];
        for _ in 0..8 {
            mixer.render(&mut block);
        }

        mixer.cancel_transition();

        assert!(!mixer.is_transitioning());
        assert!(!mixer.has_pending_transition());
        assert!(
            mixer.outgoing().is_playing(),
            "cancelling stopped the track that was already playing"
        );
        assert!(
            !mixer.incoming().is_playing(),
            "the incoming deck was left running after a cancel"
        );

        // And it is audible again at full level, not stuck wherever the
        // transition's automation had faded it to.
        for _ in 0..4 {
            mixer.render(&mut block);
        }
        let peak = block
            .iter()
            .flat_map(|s| [s[0].abs(), s[1].abs()])
            .fold(0.0f32, f32::max);
        assert!(peak > 0.4, "the restored deck is inaudible: peak {peak}");
    }

    /// Cancelling with nothing scheduled must be a no-op rather than a panic —
    /// the shell calls it before every explicit load.
    #[test]
    fn cancelling_nothing_is_harmless() {
        let mut mixer = Mixer::new(RATE, 512);
        mixer.cancel_transition();
        mixer.cancel_transition();
        assert!(!mixer.is_transitioning());
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

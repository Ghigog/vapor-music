//! A single playback deck.
//!
//! Mirrors one `AudioStreamPlayer` + its `DeckA`/`DeckB` bus in the Godot
//! build: source audio, a stretch ratio, an EQ/filter chain, and a gain.
//!
//! ## Samples are stored as `i16`
//!
//! A deck holds a whole decoded track, which makes the sample format the app's
//! largest memory cost: a five-minute track at 48 kHz is 110 MB as stereo `f32`
//! and 55 MB as `i16`, and two decks are loaded near a transition. The sources
//! are lossy AAC and MP3 whose own noise floor sits far above 16 bit, so this
//! discards nothing that was in the recording.
//!
//! The critical structural difference: Godot polled `get_playback_position()`
//! every frame from three separate `_process` loops and drove bus parameters
//! through tweens on the main thread. Here the deck advances by exactly the
//! number of frames rendered, so position is derived from the audio clock and
//! cannot drift from what was actually heard.

use crate::biquad::{EqChain, Sweep};
use crate::clipping::{BandRms, Bands};
use crate::delay::{Delay, Reverb};
use crate::stretch::Stretcher;

/// Echo Out's delay time — 350 ms, from `feedback_delay_ms`.
const ECHO_TIME_SECS: f32 = 0.35;
/// Its feedback: −10 dB, from `feedback_level_db`.
const ECHO_FEEDBACK: f32 = 0.316;
/// Reverb Freeze's room and damping, from `room_size` and `damping`.
const FREEZE_ROOM: f32 = 0.95;
const FREEZE_DAMPING: f32 = 0.1;

pub struct Deck {
    samples: Vec<[i16; 2]>,
    sample_rate: f32,
    stretcher: Stretcher,
    eq: EqChain,
    /// Echo Out and Reverb Freeze. Always present, dry unless a transition asks
    /// for them — allocating one mid-transition is exactly what the audio
    /// thread cannot do.
    delay: Delay,
    reverb: Reverb,

    /// Linear gain. The transition scheduler writes dB; conversion happens once
    /// per block, not per sample.
    gain: f32,
    /// Source frames consumed per output frame. 1.0 = original tempo.
    pub ratio: f64,
    playing: bool,

    /// Three-band level meter, fed the raw signal before EQ and gain so the
    /// clipping guard sees the source level rather than the result of its own
    /// previous correction.
    meter: BandRms,
    last_rms: Bands,
    /// Per-band ducking from the clipping guard, applied on top of the
    /// transition's own EQ automation.
    clip_atten: Bands,
    eq_db: Bands,
}

impl Deck {
    pub fn new(sample_rate: f32) -> Self {
        Deck {
            samples: Vec::new(),
            sample_rate,
            stretcher: Stretcher::new(),
            eq: EqChain::new(sample_rate),
            delay: Delay::new(sample_rate),
            reverb: Reverb::new(sample_rate),
            gain: 1.0,
            ratio: 1.0,
            playing: false,
            meter: BandRms::default(),
            last_rms: Bands::default(),
            clip_atten: Bands::default(),
            eq_db: Bands::default(),
        }
    }

    pub fn load(&mut self, samples: Vec<[i16; 2]>) {
        drop(self.swap_samples(samples));
    }

    /// Replace the loaded audio and hand back the previous buffer.
    ///
    /// [`load`](Self::load) drops the old samples where it stands. For a
    /// player, "where it stands" is the audio callback, and freeing a
    /// hundred-megabyte buffer there can block on a lock inside the allocator —
    /// the exact failure MIG-010 exists to prevent, and one that only shows up
    /// as a dropout at the moment a track changes.
    ///
    /// Returning it lets the caller move the buffer back to a control thread
    /// and drop it there. Everything else about the two is identical, so
    /// `load` is written in terms of this rather than beside it.
    pub fn swap_samples(&mut self, samples: Vec<[i16; 2]>) -> Vec<[i16; 2]> {
        let previous = std::mem::replace(&mut self.samples, samples);
        self.stretcher.reset(0.0);
        self.eq.reset();
        // A new track must not inherit the tail of the one before it.
        self.delay.reset();
        self.reverb.reset();
        self.meter.reset();
        self.last_rms = Bands::default();
        self.clip_atten = Bands::default();
        self.playing = false;
        previous
    }

    /// Band levels of the most recently rendered block, before gain and EQ.
    pub fn last_rms(&self) -> Bands {
        self.last_rms
    }

    /// Linear gain currently applied, for the clipping guard.
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// EQ gains currently requested by the transition, before ducking.
    pub fn eq_db(&self) -> Bands {
        self.eq_db
    }

    /// Apply per-band ducking from the clipping guard.
    pub fn set_clip_attenuation(&mut self, atten: Bands) {
        self.clip_atten = atten;
        self.refresh_eq();
    }

    fn refresh_eq(&mut self) {
        // Ducking stacks on top of the transition's own EQ automation, matching
        // `_apply_final_eq_gains` in audio_manager.gd.
        self.eq.set_gains(
            (self.eq_db.low + self.clip_atten.low).clamp(-40.0, 0.0),
            (self.eq_db.mid + self.clip_atten.mid).clamp(-40.0, 0.0),
            (self.eq_db.high + self.clip_atten.high).clamp(-40.0, 0.0),
        );
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn seek_seconds(&mut self, secs: f64) {
        self.stretcher.reset(secs * self.sample_rate as f64);
        self.eq.reset();
        self.meter.reset();
    }

    pub fn position_seconds(&self) -> f64 {
        self.stretcher.source_position() / self.sample_rate as f64
    }

    pub fn duration_seconds(&self) -> f64 {
        self.samples.len() as f64 / self.sample_rate as f64
    }

    pub fn set_gain_db(&mut self, db: f32) {
        // -60 dB is the floor the Godot transitions fade to; treat it as
        // silence so a "faded out" deck contributes exactly nothing.
        self.gain = if db <= -60.0 {
            0.0
        } else {
            10f32.powf(db / 20.0)
        };
    }

    pub fn set_eq_db(&mut self, low: f32, mid: f32, high: f32) {
        self.eq_db = Bands { low, mid, high };
        self.refresh_eq();
    }

    pub fn set_sweep(&mut self, sweep: Option<Sweep>) {
        self.eq.set_sweep(sweep);
    }

    /// Delay wet/dry for an Echo Out. The time and feedback are fixed at the
    /// values `audio_manager.gd` configures the bus with.
    pub fn set_delay_mix(&mut self, mix: f32) {
        self.delay.set_time(ECHO_TIME_SECS);
        self.delay.set_feedback(ECHO_FEEDBACK);
        self.delay.set_mix(mix);
    }

    /// Reverb wet/dry for a Reverb Freeze.
    pub fn set_reverb_mix(&mut self, mix: f32) {
        self.reverb.set_room(FREEZE_ROOM);
        self.reverb.set_damping(FREEZE_DAMPING);
        self.reverb.set_mix(mix);
    }

    /// Render `out.len()` frames, **adding** into `out` rather than replacing.
    /// Both decks sum into the same buffer, which is what makes the crossfade a
    /// true mix rather than a switch.
    ///
    /// Returns the number of frames actually produced; a short return means the
    /// source ran out.
    pub fn render_additive(&mut self, out: &mut [[f32; 2]], scratch: &mut [[f32; 2]]) -> usize {
        if !self.playing || self.samples.is_empty() {
            return 0;
        }

        let n = out.len().min(scratch.len());
        let produced = self
            .stretcher
            .process(&self.samples, self.ratio, &mut scratch[..n]);

        if produced == 0 {
            self.playing = false;
            return 0;
        }

        // Measure before EQ and gain: the guard needs the deck's source level,
        // not the level after its own previous correction, or the ducking
        // becomes a feedback loop.
        self.last_rms = self.meter.measure(&scratch[..produced]);

        for i in 0..produced {
            let mut frame = [
                self.eq.process(0, scratch[i][0]),
                self.eq.process(1, scratch[i][1]),
            ];
            // After the EQ, before the gain: the effects should hear the track
            // as shaped by the transition, and the gain envelope should apply
            // to the result including its tail.
            frame = self.delay.process(frame);
            frame = self.reverb.process(frame);
            out[i][0] += frame[0] * self.gain;
            out[i][1] += frame[1] * self.gain;
        }

        if produced < n {
            self.playing = false;
        }
        produced
    }
}

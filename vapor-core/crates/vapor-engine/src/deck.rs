//! A single playback deck.
//!
//! Mirrors one `AudioStreamPlayer` + its `DeckA`/`DeckB` bus in the Godot
//! build: source audio, a stretch ratio, an EQ/filter chain, and a gain.
//!
//! ## Where the audio comes from
//!
//! A deck reads through a [`TrackSource`], which is either a whole decoded
//! track in memory or a few seconds of window onto one being decoded as it
//! plays (TD-09). Playback uses the window — 1 MB a deck rather than 55 — and
//! the offline tools and every alignment test use the whole buffer, which is
//! what lets a test run the identical mixer over both and compare.
//!
//! Samples are stored as `i16` either way: the sources are lossy AAC and MP3
//! whose own noise floor sits far above 16 bit, so it discards nothing that was
//! in the recording and halves what it costs to hold.
//!
//! The critical structural difference: Godot polled `get_playback_position()`
//! every frame from three separate `_process` loops and drove bus parameters
//! through tweens on the main thread. Here the deck advances by exactly the
//! number of frames rendered, so position is derived from the audio clock and
//! cannot drift from what was actually heard.

use crate::biquad::{EqChain, Sweep};
use crate::clipping::{BandRms, Bands};
use crate::delay::{Delay, Reverb};
use crate::source::TrackSource;
use crate::stretch::{Any as Stretcher, Quality};

/// Echo Out's delay time — 350 ms, from `feedback_delay_ms`.
const ECHO_TIME_SECS: f32 = 0.35;
/// Its feedback: −10 dB, from `feedback_level_db`.
const ECHO_FEEDBACK: f32 = 0.316;
/// Reverb Freeze's room and damping, from `room_size` and `damping`.
const FREEZE_ROOM: f32 = 0.95;
const FREEZE_DAMPING: f32 = 0.1;

/// The most a deck may be turned up.
///
/// Transitions only ever fade *down* from 0 dB, so this is not a musical
/// limit — it is the ceiling that stops an arithmetic accident becoming an
/// infinite gain. `10f32.powf(1e9 / 20.0)` is `inf`, and `inf` reaches the
/// output device just as readily as a number does.
const MAX_GAIN_DB: f32 = 24.0;

/// A decibel value fit to be used, clamped into `[lo, hi]`.
///
/// The finite check has to come first. `f32::clamp` returns NaN for a NaN
/// input rather than a bound, so clamping alone lets a NaN straight through to
/// a gain coefficient or a filter — and a NaN in a feedback path does not pass:
/// it poisons every sample after it.
///
/// A non-finite input is treated as the floor rather than ignored, because the
/// two things it can mean — automation divided by a zero-length transition, or
/// a value that overflowed — are both better answered with silence than with
/// whatever the deck happened to be doing before.
fn db(value: f32, lo: f32, hi: f32) -> f32 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        lo
    }
}

pub struct Deck {
    source: TrackSource,
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
    /// The last block came up short because the decoder had not caught up.
    starved: bool,

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
        Deck::with_quality(sample_rate, Quality::default())
    }

    /// A deck using a named stretcher.
    ///
    /// Exists so that the beat-alignment measurement can name which stretcher
    /// it is measuring. Without it the only way to compare the two was to edit
    /// the default and rebuild, which produces numbers that no longer reproduce
    /// once the edit is reverted.
    pub fn with_quality(sample_rate: f32, quality: Quality) -> Self {
        Deck {
            source: TrackSource::Empty,
            sample_rate,
            stretcher: Stretcher::new(quality),
            eq: EqChain::new(sample_rate),
            delay: Delay::new(sample_rate),
            reverb: Reverb::new(sample_rate),
            gain: 1.0,
            ratio: 1.0,
            playing: false,
            starved: false,
            meter: BandRms::default(),
            last_rms: Bands::default(),
            clip_atten: Bands::default(),
            eq_db: Bands::default(),
        }
    }

    /// Load a whole decoded track. The offline and test path; playback streams.
    pub fn load(&mut self, samples: Vec<[i16; 2]>) {
        drop(self.swap_source(TrackSource::Memory(samples)));
    }

    /// Replace the deck's audio and hand back what was there.
    ///
    /// Dropping the old source where it stands is what a player must not do:
    /// "where it stands" is the audio callback, and releasing a track's buffer
    /// there can block on a lock inside the allocator — the exact failure
    /// MIG-010 exists to prevent, and one that only shows up as a dropout at
    /// the moment a track changes.
    ///
    /// Returning it lets the caller move it to a control thread and drop it
    /// there. Streaming shrinks the buffer this hands back from tens of
    /// megabytes to about one, but it does not remove the rule: the window is
    /// still an allocation, and the decoder thread behind it still has to be
    /// stopped by someone who is allowed to wait.
    pub fn swap_source(&mut self, source: TrackSource) -> TrackSource {
        let previous = std::mem::replace(&mut self.source, source);
        self.stretcher.reset(0.0);
        self.eq.reset();
        // A new track must not inherit the tail of the one before it.
        self.delay.reset();
        self.reverb.reset();
        self.meter.reset();
        self.last_rms = Bands::default();
        self.clip_atten = Bands::default();
        self.playing = false;
        self.starved = false;
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
        //
        // `clamp` is not a guard against a non-finite input: `f32::clamp`
        // returns NaN for NaN, so a NaN reaching here becomes a NaN filter
        // coefficient, and a biquad with a NaN coefficient is poisoned for the
        // life of the deck — its state never recovers, because every output
        // feeds the next input. `db` handles that before the clamp can.
        self.eq.set_gains(
            db(self.eq_db.low + self.clip_atten.low, -40.0, 0.0),
            db(self.eq_db.mid + self.clip_atten.mid, -40.0, 0.0),
            db(self.eq_db.high + self.clip_atten.high, -40.0, 0.0),
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
        let frames = (secs * self.sample_rate as f64).max(0.0);
        self.stretcher.reset(frames);
        // A streaming source has to be told: the audio for that position may
        // not be in the window, and only the decoder thread can fetch it. A
        // target already inside the window costs nothing, which is what makes a
        // short scrub gapless.
        self.source.seek(frames as u64);
        self.eq.reset();
        self.meter.reset();
    }

    pub fn position_seconds(&self) -> f64 {
        self.stretcher.source_position() / self.sample_rate as f64
    }

    pub fn duration_seconds(&self) -> f64 {
        self.source.duration_frames() as f64 / self.sample_rate as f64
    }

    /// True while the deck is waiting on its decoder rather than playing.
    ///
    /// Distinct from being stopped: the deck still intends to play and will
    /// resume the instant frames arrive. Surfaced so a player can tell the
    /// difference between "buffering" and "finished" without inferring it.
    pub fn is_starved(&self) -> bool {
        self.starved
    }

    pub fn set_gain_db(&mut self, decibels: f32) {
        // -60 dB is the floor the Godot transitions fade to; treat it as
        // silence so a "faded out" deck contributes exactly nothing.
        //
        // The clamp above the floor is what stops a runaway automation value
        // reaching the output device. `NaN <= -60.0` is false, so without a
        // finite check a NaN decibel became `10^(NaN/20)` — a NaN gain, and
        // then every sample of both channels is NaN for as long as it is set.
        // That is not a quiet failure on real hardware.
        let decibels = db(decibels, -60.0, MAX_GAIN_DB);
        self.gain = if decibels <= -60.0 {
            0.0
        } else {
            10f32.powf(decibels / 20.0)
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
    /// Returns the number of frames actually produced. A short return means
    /// either that the track ended — in which case the deck stops — or that its
    /// decoder is behind, in which case the deck stays exactly where it is and
    /// picks up on the next block.
    pub fn render_additive(&mut self, out: &mut [[f32; 2]], scratch: &mut [[f32; 2]]) -> usize {
        if !self.playing || self.source.is_empty() {
            return 0;
        }

        let n = out.len().min(scratch.len());

        // Publish before reading, never after: this is what stops the decoder
        // thread retiring the history the WSOLA search is about to read. See
        // `source`'s module docs.
        self.source
            .publish_read(self.stretcher.source_position() as u64);
        let view = self.source.view();
        let rendered = self.stretcher.process(&view, self.ratio, &mut scratch[..n]);

        // Starvation is not the end of a track, and treating it as one would
        // make the player skip to the next song because a disk read was slow.
        self.starved = rendered.frames < n && !rendered.ended;

        let produced = rendered.frames;
        if produced == 0 {
            if rendered.ended {
                self.playing = false;
            }
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

        if rendered.ended {
            self.playing = false;
        }
        produced
    }
}

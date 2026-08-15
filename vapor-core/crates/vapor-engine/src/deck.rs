//! A single playback deck.
//!
//! Mirrors one `AudioStreamPlayer` + its `DeckA`/`DeckB` bus in the Godot
//! build: source audio, a stretch ratio, an EQ/filter chain, and a gain.
//!
//! The critical structural difference: Godot polled `get_playback_position()`
//! every frame from three separate `_process` loops and drove bus parameters
//! through tweens on the main thread. Here the deck advances by exactly the
//! number of frames rendered, so position is derived from the audio clock and
//! cannot drift from what was actually heard.

use crate::biquad::{EqChain, Sweep};
use crate::clipping::{BandRms, Bands};
use crate::stretch::Stretcher;

pub struct Deck {
    samples: Vec<[f32; 2]>,
    sample_rate: f32,
    stretcher: Stretcher,
    eq: EqChain,

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
            gain: 1.0,
            ratio: 1.0,
            playing: false,
            meter: BandRms::default(),
            last_rms: Bands::default(),
            clip_atten: Bands::default(),
            eq_db: Bands::default(),
        }
    }

    pub fn load(&mut self, samples: Vec<[f32; 2]>) {
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
    pub fn swap_samples(&mut self, samples: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
        let previous = std::mem::replace(&mut self.samples, samples);
        self.stretcher.reset(0.0);
        self.eq.reset();
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
            for ch in 0..2 {
                let s = self.eq.process(ch, scratch[i][ch]);
                out[i][ch] += s * self.gain;
            }
        }

        if produced < n {
            self.playing = false;
        }
        produced
    }
}

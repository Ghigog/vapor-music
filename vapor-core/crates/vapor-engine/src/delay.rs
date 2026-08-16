//! Delay and reverb — what Echo Out and Reverb Freeze are made of.
//!
//! Replaces `AudioEffectDelay` and `AudioEffectReverb` on the Godot deck buses.
//! Three of the six transitions need these, so without them the shell can only
//! ever choose from half the vocabulary (TD-20).
//!
//! ## Both are allocation-free once built
//!
//! Every buffer is sized in `new` and never grows. These run inside
//! `Mixer::render`, which is asserted allocation-free by a counting allocator,
//! and an effect that allocated on a parameter change would break that
//! guarantee at exactly the moment a transition starts.
//!
//! ## Why a hand-written reverb rather than a crate
//!
//! Schroeder–Moorer is eight filters and about eighty lines, it has no
//! dependencies, and it compiles to wasm without thought. The alternative is a
//! convolution reverb, which needs an impulse response to ship, an FFT per
//! block, and a licence for the recording. For hiding a key clash under a tail,
//! this is the right size of tool.

/// Longest delay the line can hold, in seconds.
///
/// The Godot bus tops out around a second and Echo Out uses a fraction of that;
/// two gives room for a slow tempo's dotted-eighth without ever reallocating.
const MAX_DELAY_SECS: f32 = 2.0;

/// A feedback delay line with a wet/dry mix.
///
/// The echo half of Echo Out: feedback rising as the outgoing track fades, so
/// what is left is a decaying tail of itself rather than an abrupt silence.
pub struct Delay {
    buffer: Vec<[f32; 2]>,
    write: usize,
    sample_rate: f32,

    /// Delay in samples. Fractional would need interpolation; a whole-sample
    /// delay is inaudible as a difference here and cheaper.
    delay_samples: usize,
    feedback: f32,
    mix: f32,
}

impl Delay {
    pub fn new(sample_rate: f32) -> Self {
        let capacity = (sample_rate * MAX_DELAY_SECS) as usize + 1;
        Delay {
            buffer: vec![[0.0; 2]; capacity],
            write: 0,
            sample_rate,
            delay_samples: (sample_rate * 0.375) as usize,
            feedback: 0.0,
            mix: 0.0,
        }
    }

    /// Set the delay time, clamped to what the line can hold.
    pub fn set_time(&mut self, secs: f32) {
        let want = (secs.max(0.0) * self.sample_rate) as usize;
        // `len() - 1` so a read never lands on the slot being written.
        self.delay_samples = want.clamp(1, self.buffer.len() - 1);
    }

    /// Feedback, 0 to just under 1.
    ///
    /// Capped below unity: at 1.0 the line never decays, and a transition that
    /// ends with a tail still ringing is a bug that outlives the mix.
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(0.0, 0.95);
    }

    /// Wet/dry balance, 0 = dry, 1 = only the echo.
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Silence the line without disturbing its settings.
    ///
    /// Called when a deck loads: a new track must not inherit the tail of the
    /// one before it.
    pub fn reset(&mut self) {
        self.buffer.iter_mut().for_each(|s| *s = [0.0; 2]);
        self.write = 0;
    }

    pub fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let len = self.buffer.len();
        let read = (self.write + len - self.delay_samples) % len;
        let delayed = self.buffer[read];

        // Written *after* reading, so a delay of one sample is one sample and
        // not zero.
        self.buffer[self.write] = [
            frame[0] + delayed[0] * self.feedback,
            frame[1] + delayed[1] * self.feedback,
        ];
        self.write = (self.write + 1) % len;

        [
            frame[0] * (1.0 - self.mix) + delayed[0] * self.mix,
            frame[1] * (1.0 - self.mix) + delayed[1] * self.mix,
        ]
    }
}

/// Comb filter delays in samples at 44.1 kHz, from Schroeder–Moorer.
///
/// Mutually prime so their echo patterns do not line up and produce a ringing
/// pitch instead of a diffuse tail.
const COMB_DELAYS: [usize; 4] = [1116, 1188, 1277, 1356];
/// All-pass delays, which diffuse without colouring.
const ALLPASS_DELAYS: [usize; 2] = [556, 441];
/// The all-pass coefficient every Schroeder design uses.
const ALLPASS_GAIN: f32 = 0.5;
/// Rate the tuning above was chosen at; other rates scale from it.
const TUNING_RATE: f32 = 44_100.0;

/// A Schroeder–Moorer reverb: four parallel combs into two series all-passes.
///
/// The freeze half of Reverb Freeze. Mono processing summed to both channels —
/// a stereo tail would need decorrelated delay sets, and this exists to blur a
/// transition rather than to place anything in a room.
pub struct Reverb {
    combs: [Comb; 4],
    allpasses: [AllPass; 2],
    mix: f32,
}

impl Reverb {
    pub fn new(sample_rate: f32) -> Self {
        let scale = |n: usize| ((n as f32 * sample_rate / TUNING_RATE) as usize).max(1);
        Reverb {
            combs: [
                Comb::new(scale(COMB_DELAYS[0])),
                Comb::new(scale(COMB_DELAYS[1])),
                Comb::new(scale(COMB_DELAYS[2])),
                Comb::new(scale(COMB_DELAYS[3])),
            ],
            allpasses: [
                AllPass::new(scale(ALLPASS_DELAYS[0])),
                AllPass::new(scale(ALLPASS_DELAYS[1])),
            ],
            mix: 0.0,
        }
    }

    /// Tail length, 0 to just under 1. Higher is longer.
    pub fn set_room(&mut self, room: f32) {
        let feedback = room.clamp(0.0, 0.98);
        for comb in &mut self.combs {
            comb.feedback = feedback;
        }
    }

    /// How much of the tail is heard. 0 = dry.
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// How dark the tail is: 0 keeps the highs, 1 damps them heavily.
    pub fn set_damping(&mut self, damping: f32) {
        for comb in &mut self.combs {
            comb.damping = damping.clamp(0.0, 0.95);
        }
    }

    pub fn reset(&mut self) {
        for comb in &mut self.combs {
            comb.reset();
        }
        for allpass in &mut self.allpasses {
            allpass.reset();
        }
    }

    pub fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        if self.mix <= 0.0 {
            return frame;
        }

        let input = (frame[0] + frame[1]) * 0.5;
        let mut wet: f32 = self.combs.iter_mut().map(|c| c.process(input)).sum();
        // Four combs in parallel sum to four times the input; scaled back so a
        // change of mix is a change of balance rather than of level.
        wet *= 0.25;
        for allpass in &mut self.allpasses {
            wet = allpass.process(wet);
        }

        [
            frame[0] * (1.0 - self.mix) + wet * self.mix,
            frame[1] * (1.0 - self.mix) + wet * self.mix,
        ]
    }
}

/// One low-pass-damped feedback comb.
struct Comb {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
    damping: f32,
    /// The one-pole low-pass state inside the feedback path, which is what
    /// makes the tail get darker as it decays rather than staying bright.
    filtered: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Comb {
            buffer: vec![0.0; len],
            index: 0,
            feedback: 0.5,
            damping: 0.2,
            filtered: 0.0,
        }
    }

    fn reset(&mut self) {
        self.buffer.iter_mut().for_each(|s| *s = 0.0);
        self.index = 0;
        self.filtered = 0.0;
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filtered = output * (1.0 - self.damping) + self.filtered * self.damping;
        self.buffer[self.index] = input + self.filtered * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

/// One all-pass, for diffusion without colouration.
struct AllPass {
    buffer: Vec<f32>,
    index: usize,
}

impl AllPass {
    fn new(len: usize) -> Self {
        AllPass {
            buffer: vec![0.0; len],
            index: 0,
        }
    }

    fn reset(&mut self) {
        self.buffer.iter_mut().for_each(|s| *s = 0.0);
        self.index = 0;
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;
        self.buffer[self.index] = input + buffered * ALLPASS_GAIN;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 44_100.0;

    /// An impulse must come back one delay later, at the feedback level.
    #[test]
    fn a_delay_repeats_the_input_after_its_delay_time() {
        let mut delay = Delay::new(RATE);
        delay.set_time(0.1);
        delay.set_feedback(0.5);
        delay.set_mix(1.0);

        let expected_at = (RATE * 0.1) as usize;
        let mut heard_at = None;
        let mut out = delay.process([1.0, 1.0]);
        for i in 1..expected_at + 10 {
            if out[0].abs() > 0.5 && heard_at.is_none() && i > 1 {
                heard_at = Some(i);
            }
            out = delay.process([0.0, 0.0]);
            if out[0].abs() > 0.5 && heard_at.is_none() {
                heard_at = Some(i);
            }
        }

        let at = heard_at.expect("the echo never arrived");
        assert!(
            (at as i64 - expected_at as i64).abs() <= 2,
            "echo arrived at sample {at}, expected about {expected_at}"
        );
    }

    /// A tail that never decays outlives the transition it belongs to.
    #[test]
    fn delay_feedback_cannot_reach_unity() {
        let mut delay = Delay::new(RATE);
        delay.set_feedback(5.0);
        delay.set_time(0.01);
        delay.set_mix(1.0);

        delay.process([1.0, 1.0]);
        for _ in 0..(RATE as usize * 8) {
            delay.process([0.0, 0.0]);
        }
        let residual = (0..64)
            .map(|_| delay.process([0.0, 0.0])[0].abs())
            .fold(0.0f32, f32::max);
        assert!(
            residual < 0.2,
            "still ringing at {residual} after 8 seconds"
        );
    }

    /// A dry effect must be bit-exact, so an unused one cannot colour the mix.
    #[test]
    fn both_effects_are_transparent_when_dry() {
        let mut delay = Delay::new(RATE);
        let mut reverb = Reverb::new(RATE);
        delay.set_mix(0.0);
        reverb.set_mix(0.0);

        for i in 0..1000 {
            let frame = [(i as f32 * 0.01).sin(), (i as f32 * 0.013).cos()];
            assert_eq!(delay.process(frame), frame);
            assert_eq!(reverb.process(frame), frame);
        }
    }

    /// The point of a reverb: sound continues after the input stops.
    #[test]
    fn a_reverb_rings_on_after_the_input_stops() {
        let mut reverb = Reverb::new(RATE);
        reverb.set_room(0.85);
        reverb.set_mix(1.0);

        for i in 0..(RATE * 0.2) as usize {
            let t = i as f32 / RATE;
            reverb.process([(t * 440.0 * std::f32::consts::TAU).sin() * 0.5; 2]);
        }

        // A tenth of a second after silence, there should still be signal.
        let mut tail = 0.0f32;
        for _ in 0..(RATE * 0.1) as usize {
            tail = tail.max(reverb.process([0.0, 0.0])[0].abs());
        }
        assert!(tail > 0.01, "the tail died immediately: {tail}");
    }

    /// And it must eventually stop, or a frozen reverb never thaws.
    #[test]
    fn a_reverb_tail_decays() {
        let mut reverb = Reverb::new(RATE);
        reverb.set_room(0.8);
        reverb.set_mix(1.0);
        for _ in 0..1000 {
            reverb.process([1.0, 1.0]);
        }

        let early = (0..1000)
            .map(|_| reverb.process([0.0, 0.0])[0].abs())
            .fold(0.0f32, f32::max);
        for _ in 0..(RATE as usize * 5) {
            reverb.process([0.0, 0.0]);
        }
        let late = (0..1000)
            .map(|_| reverb.process([0.0, 0.0])[0].abs())
            .fold(0.0f32, f32::max);

        assert!(late < early * 0.5, "tail did not decay: {early} -> {late}");
    }

    /// Neither may produce a non-finite sample, which would propagate through
    /// the mixer and out to the device as a click or worse.
    #[test]
    fn neither_effect_produces_a_non_finite_sample() {
        let mut delay = Delay::new(RATE);
        let mut reverb = Reverb::new(RATE);
        delay.set_feedback(0.95);
        delay.set_mix(1.0);
        delay.set_time(0.001);
        reverb.set_room(0.98);
        reverb.set_mix(1.0);

        for _ in 0..(RATE as usize * 3) {
            let a = delay.process([1.0, -1.0]);
            let b = reverb.process([1.0, -1.0]);
            assert!(a.iter().chain(b.iter()).all(|v| v.is_finite()));
        }
    }

    /// Reset has to clear the tail, or a new track inherits the last one's.
    #[test]
    fn reset_clears_the_tail() {
        let mut reverb = Reverb::new(RATE);
        reverb.set_room(0.9);
        reverb.set_mix(1.0);
        for _ in 0..2000 {
            reverb.process([1.0, 1.0]);
        }
        reverb.reset();

        let after = (0..1000)
            .map(|_| reverb.process([0.0, 0.0])[0].abs())
            .fold(0.0f32, f32::max);
        assert!(after < 1e-6, "tail survived a reset: {after}");
    }
}

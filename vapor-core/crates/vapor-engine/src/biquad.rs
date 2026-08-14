//! Biquad filters — the replacement for Godot's `AudioServer` bus effects.
//!
//! The Godot build wires `AudioEffectEQ6` + `AudioEffectLowPassFilter` +
//! `AudioEffectHighPassFilter` onto each deck bus and automates their
//! parameters with tweens. Here that is an explicit chain of RBJ cookbook
//! biquads with per-channel state.
//!
//! Every method here is allocation-free and branch-light, because this runs in
//! the audio callback. `set_*` recomputes coefficients, which is cheap but not
//! free — the transition scheduler calls it at control rate (once per block),
//! never per sample.

/// Direct Form I biquad. Form I rather than II because it is better behaved
/// when coefficients change during playback, which is exactly what a filter
/// sweep does.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // Per-channel state: [left, right].
    x1: [f32; 2],
    x2: [f32; 2],
    y1: [f32; 2],
    y2: [f32; 2],
}

impl Default for Biquad {
    fn default() -> Self {
        Self::identity()
    }
}

impl Biquad {
    /// A pass-through filter.
    pub fn identity() -> Self {
        Biquad {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: [0.0; 2],
            x2: [0.0; 2],
            y1: [0.0; 2],
            y2: [0.0; 2],
        }
    }

    /// Clear the delay lines. Call on seek or deck reload — not during a
    /// transition, or the filter will click.
    pub fn reset(&mut self) {
        self.x1 = [0.0; 2];
        self.x2 = [0.0; 2];
        self.y1 = [0.0; 2];
        self.y2 = [0.0; 2];
    }

    fn set_coeffs(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        let inv = 1.0 / a0;
        self.b0 = b0 * inv;
        self.b1 = b1 * inv;
        self.b2 = b2 * inv;
        self.a1 = a1 * inv;
        self.a2 = a2 * inv;
    }

    /// Peaking EQ — the band type `AudioEffectEQ6` exposes.
    pub fn set_peaking(&mut self, sample_rate: f32, freq: f32, q: f32, gain_db: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * clamp_freq(freq, sample_rate) / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q.max(0.01));

        self.set_coeffs(
            1.0 + alpha * a,
            -2.0 * cos_w0,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos_w0,
            1.0 - alpha / a,
        );
    }

    pub fn set_lowpass(&mut self, sample_rate: f32, freq: f32, q: f32) {
        let w0 = 2.0 * std::f32::consts::PI * clamp_freq(freq, sample_rate) / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q.max(0.01));
        let b1 = 1.0 - cos_w0;

        self.set_coeffs(
            b1 / 2.0,
            b1,
            b1 / 2.0,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        );
    }

    pub fn set_highpass(&mut self, sample_rate: f32, freq: f32, q: f32) {
        let w0 = 2.0 * std::f32::consts::PI * clamp_freq(freq, sample_rate) / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q.max(0.01));
        let b1 = -(1.0 + cos_w0);

        self.set_coeffs(
            -b1 / 2.0,
            b1,
            -b1 / 2.0,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        );
    }

    /// Process one sample on one channel. `ch` must be 0 or 1.
    #[inline]
    pub fn process(&mut self, ch: usize, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1[ch] + self.b2 * self.x2[ch]
            - self.a1 * self.y1[ch]
            - self.a2 * self.y2[ch];
        self.x2[ch] = self.x1[ch];
        self.x1[ch] = x;
        self.y2[ch] = self.y1[ch];
        self.y1[ch] = y;
        y
    }
}

/// Keep the cutoff below Nyquist with margin. A sweep that runs the cutoff into
/// Nyquist produces coefficients that blow up, which is audible as a crack
/// rather than a graceful stop.
fn clamp_freq(freq: f32, sample_rate: f32) -> f32 {
    freq.clamp(10.0, sample_rate * 0.45)
}

/// Three-band EQ per deck, matching how the Godot transitions actually drive
/// `AudioEffectEQ6`: low / mid / high gains automated independently.
///
/// Bass Swap cuts the outgoing low band while raising the incoming one; the
/// mid-cut in Echo Out and Reverb Freeze drives the mid band.
#[derive(Clone, Copy, Debug)]
pub struct EqChain {
    low: Biquad,
    mid: Biquad,
    high: Biquad,
    filter: Biquad,
    filter_active: bool,
    sample_rate: f32,
}

pub const LOW_FREQ: f32 = 100.0;
pub const MID_FREQ: f32 = 1000.0;
pub const HIGH_FREQ: f32 = 8000.0;
const BAND_Q: f32 = 0.9;

impl EqChain {
    pub fn new(sample_rate: f32) -> Self {
        let mut c = EqChain {
            low: Biquad::identity(),
            mid: Biquad::identity(),
            high: Biquad::identity(),
            filter: Biquad::identity(),
            filter_active: false,
            sample_rate,
        };
        c.set_gains(0.0, 0.0, 0.0);
        c
    }

    pub fn set_gains(&mut self, low_db: f32, mid_db: f32, high_db: f32) {
        self.low
            .set_peaking(self.sample_rate, LOW_FREQ, BAND_Q, low_db);
        self.mid
            .set_peaking(self.sample_rate, MID_FREQ, BAND_Q, mid_db);
        self.high
            .set_peaking(self.sample_rate, HIGH_FREQ, BAND_Q, high_db);
    }

    /// Drive the sweep filter. `None` bypasses it entirely, which matters:
    /// a bypassed filter costs nothing and cannot colour the signal, whereas a
    /// "wide open" lowpass still has phase response.
    pub fn set_sweep(&mut self, sweep: Option<Sweep>) {
        match sweep {
            None => self.filter_active = false,
            Some(Sweep::LowPass { freq, q }) => {
                self.filter.set_lowpass(self.sample_rate, freq, q);
                self.filter_active = true;
            }
            Some(Sweep::HighPass { freq, q }) => {
                self.filter.set_highpass(self.sample_rate, freq, q);
                self.filter_active = true;
            }
        }
    }

    pub fn reset(&mut self) {
        self.low.reset();
        self.mid.reset();
        self.high.reset();
        self.filter.reset();
    }

    #[inline]
    pub fn process(&mut self, ch: usize, x: f32) -> f32 {
        let mut y = self.low.process(ch, x);
        y = self.mid.process(ch, y);
        y = self.high.process(ch, y);
        if self.filter_active {
            y = self.filter.process(ch, y);
        }
        y
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Sweep {
    LowPass { freq: f32, q: f32 },
    HighPass { freq: f32, q: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Measure a filter's gain at one frequency by driving it with a sine and
    /// taking the steady-state RMS ratio.
    fn gain_at(mut f: impl FnMut(usize, f32) -> f32, freq: f32, rate: f32) -> f32 {
        let n = (rate * 0.5) as usize;
        let mut in_sq = 0.0f64;
        let mut out_sq = 0.0f64;
        for i in 0..n {
            let t = i as f32 / rate;
            let x = (2.0 * std::f32::consts::PI * freq * t).sin();
            let y = f(0, x);
            // Skip the first 20% so the filter has settled.
            if i > n / 5 {
                in_sq += (x * x) as f64;
                out_sq += (y * y) as f64;
            }
        }
        (out_sq / in_sq).sqrt() as f32
    }

    #[test]
    fn identity_passes_signal_unchanged() {
        let mut b = Biquad::identity();
        for x in [0.0f32, 0.5, -1.0, 0.25] {
            assert_eq!(b.process(0, x), x);
        }
    }

    #[test]
    fn peaking_boost_and_cut_hit_their_targets() {
        let rate = 44100.0;
        for &db in &[6.0f32, -6.0, -24.0] {
            let mut b = Biquad::identity();
            b.set_peaking(rate, 1000.0, 0.9, db);
            let g = gain_at(|c, x| b.process(c, x), 1000.0, rate);
            let measured_db = 20.0 * g.log10();
            assert!(
                (measured_db - db).abs() < 0.5,
                "peaking {db} dB measured {measured_db:.2} dB"
            );
        }
    }

    #[test]
    fn lowpass_attenuates_above_cutoff_and_passes_below() {
        let rate = 44100.0;
        let mut b = Biquad::identity();
        b.set_lowpass(rate, 500.0, 0.707);

        let mut b1 = b;
        let pass = gain_at(|c, x| b1.process(c, x), 100.0, rate);
        let mut b2 = b;
        let stop = gain_at(|c, x| b2.process(c, x), 5000.0, rate);

        assert!(pass > 0.9, "passband gain {pass:.3} should be near unity");
        assert!(stop < 0.05, "stopband gain {stop:.3} should be well down");
    }

    #[test]
    fn highpass_is_the_mirror_of_lowpass() {
        let rate = 44100.0;
        let mut b = Biquad::identity();
        b.set_highpass(rate, 500.0, 0.707);

        let mut b1 = b;
        let stop = gain_at(|c, x| b1.process(c, x), 50.0, rate);
        let mut b2 = b;
        let pass = gain_at(|c, x| b2.process(c, x), 5000.0, rate);

        assert!(pass > 0.9, "passband gain {pass:.3}");
        assert!(stop < 0.05, "stopband gain {stop:.3}");
    }

    /// A sweep must stay stable when driven to the top of its range — the
    /// coefficient blow-up near Nyquist is audible as a crack, not a fade.
    #[test]
    fn sweep_to_nyquist_stays_finite() {
        let rate = 44100.0;
        let mut chain = EqChain::new(rate);
        for i in 0..=100 {
            let freq = 20.0 + (rate * 0.6 - 20.0) * i as f32 / 100.0;
            chain.set_sweep(Some(Sweep::LowPass { freq, q: 0.707 }));
            for j in 0..64 {
                let x = ((j as f32) * 0.1).sin();
                let y = chain.process(0, x);
                assert!(y.is_finite(), "non-finite output at cutoff {freq} Hz");
            }
        }
    }

    /// Channels must not bleed into each other through shared filter state.
    #[test]
    fn channels_are_independent() {
        let rate = 44100.0;
        let mut chain = EqChain::new(rate);
        chain.set_gains(-12.0, 0.0, 0.0);

        // Drive only the left channel; the right must stay silent.
        for _ in 0..1000 {
            chain.process(0, 1.0);
            let r = chain.process(1, 0.0);
            assert_eq!(r, 0.0, "right channel picked up left channel state");
        }
    }
}

// Musical resonant filter for RayDrone — the "swell" of a pad.
//
// A stereo TPT state-variable low-pass (Zavalishin / Simper topology: stable at
// any cutoff, clean resonance) with an optional cutoff LFO whose rate is set in
// Hz — the UI derives that from BPM + a note division, so the filter sweep locks
// to the track's tempo. `core`-side and `no_std`, shared by both engines.
//
// `no_std` means no `tan`, so we approximate `tan(pi*fc/sr)` with Bhaskara's
// sine (error < ~0.2 %, far below audible for a filter coefficient).

use crate::clampf;

const PI: f32 = core::f32::consts::PI;

// Bhaskara I sine approximation, valid on [0, pi]. ~0.16% max error.
#[inline]
fn bsin(x: f32) -> f32 {
    16.0 * x * (PI - x) / (5.0 * PI * PI - 4.0 * x * (PI - x))
}

// tan(w) for w in [0, pi/2) via sin/cos, both from Bhaskara (cos w = sin(pi/2+w)).
#[inline]
fn tan_w(w: f32) -> f32 {
    let s = bsin(w);
    let c = bsin(PI * 0.5 + w);
    s / if c < 1e-4 { 1e-4 } else { c }
}

#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f32, // integrator states
    ic2: f32,
}

impl Svf {
    #[inline]
    fn lp(&mut self, v0: f32, a1: f32, a2: f32, a3: f32) -> f32 {
        let v3 = v0 - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v2 // low-pass output
    }
}

pub struct Filter {
    l: Svf,
    r: Svf,
    sr: f32,
    base_cut: f32, // Hz
    res: f32,      // 0..1 (0 = none, →1 = self-oscillation-ish)
    // Tempo LFO: triangle sweeping the cutoff upward by up to `lfo_depth` octaves.
    lfo_phase: f32,
    lfo_inc: f32, // cycles per sample
    lfo_depth: f32,
    enabled: bool,
    // Cached coefficients (recomputed each sample only when the LFO moves cutoff).
    cur_cut: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl Filter {
    pub const fn new() -> Self {
        Filter {
            l: Svf { ic1: 0.0, ic2: 0.0 },
            r: Svf { ic1: 0.0, ic2: 0.0 },
            sr: 44100.0,
            base_cut: 20000.0,
            res: 0.0,
            lfo_phase: 0.0,
            lfo_inc: 0.0,
            lfo_depth: 0.0,
            enabled: false,
            cur_cut: -1.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = if sr > 1.0 { sr } else { 44100.0 };
        self.cur_cut = -1.0; // force coeff recompute
    }

    /// Cutoff in Hz and resonance 0..1. `enabled` is on whenever cutoff is below
    /// roughly Nyquist (otherwise it's a no-op pass-through and we bypass).
    pub fn set(&mut self, cutoff_hz: f32, res01: f32) {
        self.base_cut = clampf(cutoff_hz, 20.0, self.sr * 0.45);
        self.res = clampf(res01, 0.0, 1.0);
        self.enabled = cutoff_hz < self.sr * 0.45 || self.lfo_depth > 0.0;
        self.cur_cut = -1.0;
    }

    /// Cutoff LFO: `rate_hz` is the sweep frequency (UI computes it from BPM and a
    /// note division), `depth_oct` how many octaves it opens the filter (0 = off).
    pub fn set_lfo(&mut self, rate_hz: f32, depth_oct: f32) {
        self.lfo_inc = clampf(rate_hz, 0.0, 100.0) / self.sr;
        self.lfo_depth = clampf(depth_oct, 0.0, 6.0);
        if self.lfo_depth > 0.0 {
            self.enabled = true;
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
    }

    pub fn reset(&mut self) {
        self.l = Svf::default();
        self.r = Svf::default();
        self.lfo_phase = 0.0;
        self.cur_cut = -1.0;
    }

    #[inline]
    fn recompute(&mut self, cut: f32) {
        let cut = clampf(cut, 20.0, self.sr * 0.45);
        self.cur_cut = cut;
        let g = tan_w(PI * cut / self.sr);
        let k = 2.0 - 1.9 * self.res; // res 0..1 → k 2..0.1 (Q = 1/k)
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// One stereo frame.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        if !self.enabled {
            return (l, r);
        }
        // Tempo LFO sweeps the cutoff up by up to `lfo_depth` octaves (triangle).
        let cut = if self.lfo_depth > 0.0 && self.lfo_inc > 0.0 {
            let tri = 1.0 - (2.0 * self.lfo_phase - 1.0).abs(); // 0..1..0
            self.lfo_phase += self.lfo_inc;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
            // Give the sweep headroom: `recompute` clamps to the same ceiling
            // (sr*0.45) that `base_cut` can already sit at (e.g. cutoff left
            // "fully open"), so without this the LFO has nowhere to sweep up
            // into and `depth` does nothing audible. Scale the swept base down
            // so its peak excursion (oct == lfo_depth) lands at the ceiling
            // instead of being silently clipped there every cycle.
            let ceiling = self.sr * 0.45;
            let peak_factor = pow2(self.lfo_depth);
            let swept_base = if self.base_cut * peak_factor > ceiling {
                ceiling / peak_factor
            } else {
                self.base_cut
            };
            // 2^(depth·tri) via the integer-octave + fractional blend; cheap & smooth.
            let oct = self.lfo_depth * tri;
            swept_base * pow2(oct)
        } else {
            self.base_cut
        };
        // Recompute coeffs only when the cutoff actually moved (static = once).
        if (cut - self.cur_cut).abs() > 0.5 {
            self.recompute(cut);
        }
        let (a1, a2, a3) = (self.a1, self.a2, self.a3);
        (self.l.lp(l, a1, a2, a3), self.r.lp(r, a1, a2, a3))
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self::new()
    }
}

// 2^x for x >= 0, no_std: 2^floor(x) times a small-poly fractional part.
// The 0.693_147 coefficient is intentionally ln(2) (the minimax poly's leading
// term) — not a typo'd PI/E/SQRT_2, so silence clippy's approx-constant lint.
#[inline]
#[allow(clippy::approx_constant)]
fn pow2(x: f32) -> f32 {
    let i = x as u32;
    let f = x - i as f32; // 0..1
    // 2^f ≈ 1 + f·(0.6931 + f·(0.2402 + f·0.0558)) — ~0.1% on [0,1].
    let frac = 1.0 + f * (0.693_147 + f * (0.240_226 + f * 0.055_82));
    ((1u32 << i.min(30)) as f32) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    // Crude single-bin energy of the filter's response to a sine at `freq`.
    fn response(filter: &mut Filter, freq: f32, sr: f32) -> f32 {
        // settle, then measure RMS over a window
        for i in 0..2000 {
            let x = (2.0 * PI * freq * i as f32 / sr).sin();
            filter.process(x, x);
        }
        let mut s = 0.0f32;
        let n = 4000;
        for i in 0..n {
            let x = (2.0 * PI * freq * i as f32 / sr).sin();
            let (y, _) = filter.process(x, x);
            s += y * y;
        }
        (s / n as f32).sqrt()
    }

    #[test]
    fn lowpass_attenuates_highs_more_than_lows() {
        let sr = 44100.0;
        let mut lo = Filter::new();
        lo.set_sample_rate(sr);
        lo.set(500.0, 0.1);
        let mut hi = Filter::new();
        hi.set_sample_rate(sr);
        hi.set(500.0, 0.1);
        let low = response(&mut lo, 200.0, sr); // below cutoff → mostly passes
        let high = response(&mut hi, 5000.0, sr); // well above → attenuated
        assert!(low > 0.5, "passband too quiet: {low}");
        assert!(high < 0.1, "stopband not attenuated: {high}");
        assert!(low > high * 5.0, "low {low} not >> high {high}");
    }

    #[test]
    fn resonance_lifts_the_corner() {
        let sr = 44100.0;
        let cut = 1000.0;
        let mut flat = Filter::new();
        flat.set_sample_rate(sr);
        flat.set(cut, 0.0);
        let mut reso = Filter::new();
        reso.set_sample_rate(sr);
        reso.set(cut, 0.9);
        // Near the corner, high resonance should boost vs. no resonance.
        let a = response(&mut flat, cut, sr);
        let b = response(&mut reso, cut, sr);
        assert!(b > a, "resonance did not lift the corner: flat {a} vs reso {b}");
    }

    #[test]
    fn disabled_is_passthrough() {
        let mut f = Filter::new(); // enabled defaults false
        assert_eq!(f.process(0.7, -0.3), (0.7, -0.3));
    }

    #[test]
    fn lfo_sweeps_even_with_cutoff_left_fully_open() {
        // Regression: with the manual cutoff at the ceiling (as when the UI's
        // "Cutoff" control is left at its default, fully-open position), the
        // LFO used to have no headroom to sweep into and `depth` was
        // inaudible — this compares an 8 kHz tone's RMS with the LFO off vs.
        // on and requires a real, audible drop, not just "some" numeric change.
        let sr = 44100.0;
        let freq = 8000.0;

        let mut no_lfo = Filter::new();
        no_lfo.set_sample_rate(sr);
        no_lfo.set(sr * 0.45, 0.0); // cutoff at the absolute ceiling: "fully open"
        let flat = response(&mut no_lfo, freq, sr);

        let mut with_lfo = Filter::new();
        with_lfo.set_sample_rate(sr);
        with_lfo.set(sr * 0.45, 0.0); // same "fully open" cutoff
        with_lfo.set_lfo(2.0, 4.0); // 2 Hz sweep, 4-octave depth
        let swept = response(&mut with_lfo, freq, sr);

        assert!(
            swept < flat * 0.7,
            "LFO depth had no real headroom to sweep into: flat {flat} vs swept {swept}"
        );
    }

    #[test]
    fn pow2_is_close() {
        for &x in &[0.0f32, 0.5, 1.0, 2.3, 4.0, 6.0] {
            let approx = pow2(x);
            let exact = 2.0f32.powf(x);
            assert!((approx - exact).abs() / exact < 5e-3, "pow2({x})={approx} vs {exact}");
        }
    }
}

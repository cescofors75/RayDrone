// RayDrone — shared DSP kernel.
//
// The numeric primitives that were duplicated *verbatim* between the WASM engine
// (`wasm/raydrone.rs`) and the VST engine (`vst/src/engine.rs`) live here, in one
// place, so a fix or improvement lands in both at once.
//
// `#![no_std]` and dependency-free **on purpose**: the WASM engine is built with
// raw `rustc` (no Cargo, no crates.io) and has no allocator, so anything shared
// must avoid `std`, `alloc` and external crates. The VST (a `std` crate) consumes
// this just fine — a `std` crate can depend on a `no_std` one.
//
// Deliberately NOT shared yet (they would change one side's output):
//   - `tri_inv` / `sqrtf`: the WASM build approximates the square root with a
//     no_std Newton iteration, while the VST uses the hardware `f32::sqrt`. Their
//     results differ at ~1e-6, so unifying them would alter the render. Each side
//     keeps its own until we decide on a single sqrt backend.
//   - The Freeverb-lite reverb: identical algorithm, but different storage
//     (fixed arrays in WASM vs. `Vec` in the VST). A follow-up can share it once
//     the buffers are reconciled.

// `no_std` for real builds (WASM has no allocator/std); the test harness needs
// std, so only drop it when not testing.
#![cfg_attr(not(test), no_std)]

/// Clamp `x` into `[a, b]`.
#[inline]
pub fn clampf(x: f32, a: f32, b: f32) -> f32 {
    if x < a {
        a
    } else if x > b {
        b
    } else {
        x
    }
}

/// Cubic soft-clip (gentle saturation). Limits to ±2/3 outside [-1, 1].
#[inline]
pub fn soft(x: f32) -> f32 {
    if x > 1.0 {
        0.666_666_7
    } else if x < -1.0 {
        -0.666_666_7
    } else {
        x - x * x * x * (1.0 / 3.0)
    }
}

/// Advance an xorshift32 PRNG in place and return the new state.
#[inline]
pub fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// Uniform pseudo-random `f32` in `[0, 1)` from an xorshift32 state.
#[inline]
pub fn rng01(state: &mut u32) -> f32 {
    (xorshift32(state) as f32) * (1.0 / 4_294_967_296.0)
}

/// Read `sample` at fractional position `pos` with 4-point Catmull-Rom cubic
/// interpolation. Less aliasing / more brightness than linear when reading at a
/// speed != 1 (octave, detune, host/sample SR mismatch). Returns 0 out of range.
#[inline]
pub fn sample_at(sample: &[f32], pos: f32) -> f32 {
    let len = sample.len();
    if len < 4 {
        return 0.0;
    }
    let i = pos as usize;
    if i + 2 >= len {
        return 0.0;
    }
    let frac = pos - (i as f32);
    let s0 = if i >= 1 { sample[i - 1] } else { sample[i] };
    let s1 = sample[i];
    let s2 = sample[i + 1];
    let s3 = sample[i + 2];
    let a = -0.5 * s0 + 1.5 * s1 - 1.5 * s2 + 0.5 * s3;
    let b = s0 - 2.5 * s1 + 2.0 * s2 - 0.5 * s3;
    let c = -0.5 * s0 + 0.5 * s2;
    ((a * frac + b) * frac + c) * frac + s1
}

/// Sample the (Hann) `window` at phase `ph` in `[0, 1]`. Index is scaled to the
/// window length, so a 2048-point window behaves exactly as the engines' local
/// versions did. Returns 0 past the end.
#[inline]
pub fn win_at(window: &[f32], ph: f32) -> f32 {
    let n = window.len();
    if n == 0 {
        return 0.0;
    }
    let idx = (ph * ((n - 1) as f32)) as usize;
    if idx >= n {
        0.0
    } else {
        window[idx]
    }
}

// ── DC blocker ──────────────────────────────────────────────────────────────
// Stereo one-pole highpass (~10 Hz) that removes the DC offset / rumble the
// grain cloud accumulates. The cutoff coefficient tracks the sample rate.
const TAU: f32 = 2.0 * core::f32::consts::PI;

pub struct DcBlocker {
    r: f32,
    xl: f32,
    yl: f32,
    xr: f32,
    yr: f32,
}

impl DcBlocker {
    pub const fn new() -> Self {
        DcBlocker {
            r: 0.998_575, // ~10 Hz @44.1k; recomputed by set_sample_rate
            xl: 0.0,
            yl: 0.0,
            xr: 0.0,
            yr: 0.0,
        }
    }

    /// Recompute the cutoff coefficient for `sr` (keeps the ~10 Hz corner).
    pub fn set_sample_rate(&mut self, sr: f32) {
        self.r = clampf(1.0 - TAU * 10.0 / sr, 0.9, 0.99999);
    }

    /// Clear the filter memory (call on reset / new sample).
    pub fn reset(&mut self) {
        self.xl = 0.0;
        self.yl = 0.0;
        self.xr = 0.0;
        self.yr = 0.0;
    }

    /// One stereo frame: `y = x - x1 + R·y1` per channel.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let yl = l - self.xl + self.r * self.yl;
        self.xl = l;
        self.yl = yl;
        let yr = r - self.xr + self.r * self.yr;
        self.xr = r;
        self.yr = yr;
        (yl, yr)
    }
}

impl Default for DcBlocker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Reverb (Freeverb-lite) ──────────────────────────────────────────────────
// 4 combs + 2 allpass per channel, stereo. Delay-line *capacity* is fixed (so
// the WASM engine can hold it with no allocator), while the *effective* lengths
// are scaled to the sample rate at runtime — same tail time at 44.1k/48k/96k.
pub const NC: usize = 4; // combs per channel
pub const NA: usize = 2; // allpass per channel
const CMAX: usize = 3600; // comb capacity (covers up to ~96 kHz)
const AMAX: usize = 1280; // allpass capacity
const CLEN_L: [usize; NC] = [1557, 1617, 1491, 1422]; // @44.1k
const CLEN_R: [usize; NC] = [1580, 1640, 1514, 1445]; // +stereo spread
const ALEN: [usize; NA] = [556, 441];
const REV_ROOM: f32 = 0.84; // size / feedback
const REV_DAMP: f32 = 0.5; // high-frequency damping

pub struct Reverb {
    combl: [[f32; CMAX]; NC],
    combr: [[f32; CMAX]; NC],
    combl_i: [usize; NC],
    combr_i: [usize; NC],
    combl_s: [f32; NC],
    combr_s: [f32; NC],
    clen_l: [usize; NC], // effective comb lengths @ current SR
    clen_r: [usize; NC],
    apl: [[f32; AMAX]; NA],
    apr: [[f32; AMAX]; NA],
    apl_i: [usize; NA],
    apr_i: [usize; NA],
    alen: [usize; NA], // effective allpass lengths @ current SR
    wet: f32,
}

impl Reverb {
    /// Fully zero-initialized. The effective delay-line lengths are 0 until
    /// `set_sample_rate` is called — do that before processing (both engines do,
    /// via their `update_coeffs`). Keeping `new()` all-zeros lets the WASM static
    /// land in `.bss` instead of bloating the binary with 130 KB of zero bytes.
    pub const fn new() -> Self {
        Reverb {
            combl: [[0.0; CMAX]; NC],
            combr: [[0.0; CMAX]; NC],
            combl_i: [0; NC],
            combr_i: [0; NC],
            combl_s: [0.0; NC],
            combr_s: [0.0; NC],
            clen_l: [0; NC],
            clen_r: [0; NC],
            apl: [[0.0; AMAX]; NA],
            apr: [[0.0; AMAX]; NA],
            apl_i: [0; NA],
            apr_i: [0; NA],
            alen: [0; NA],
            wet: 0.0,
        }
    }

    pub fn set_wet(&mut self, wet: f32) {
        self.wet = clampf(wet, 0.0, 1.0);
    }
    pub fn wet(&self) -> f32 {
        self.wet
    }

    /// Scale the delay-line lengths to `sr` (keeps the tail time constant). Does
    /// not clear the buffers — matches the WASM engine, which keeps its tail
    /// across sample-rate updates.
    pub fn set_sample_rate(&mut self, sr: f32) {
        let k = sr / 44100.0;
        for c in 0..NC {
            self.clen_l[c] = ((CLEN_L[c] as f32 * k) as usize).clamp(4, CMAX);
            self.clen_r[c] = ((CLEN_R[c] as f32 * k) as usize).clamp(4, CMAX);
            if self.combl_i[c] >= self.clen_l[c] {
                self.combl_i[c] = 0;
            }
            if self.combr_i[c] >= self.clen_r[c] {
                self.combr_i[c] = 0;
            }
        }
        for a in 0..NA {
            self.alen[a] = ((ALEN[a] as f32 * k) as usize).clamp(4, AMAX);
            if self.apl_i[a] >= self.alen[a] {
                self.apl_i[a] = 0;
            }
            if self.apr_i[a] >= self.alen[a] {
                self.apr_i[a] = 0;
            }
        }
    }

    /// Clear all delay lines + indices + comb states (call on reset / new sample).
    pub fn reset(&mut self) {
        for c in 0..NC {
            self.combl[c] = [0.0; CMAX];
            self.combr[c] = [0.0; CMAX];
            self.combl_i[c] = 0;
            self.combr_i[c] = 0;
            self.combl_s[c] = 0.0;
            self.combr_s[c] = 0.0;
        }
        for a in 0..NA {
            self.apl[a] = [0.0; AMAX];
            self.apr[a] = [0.0; AMAX];
            self.apl_i[a] = 0;
            self.apr_i[a] = 0;
        }
    }

    /// One stereo frame. Passes the dry input through untouched when wet == 0.
    #[inline]
    pub fn process(&mut self, inl: f32, inr: f32) -> (f32, f32) {
        if self.wet <= 0.0 {
            return (inl, inr);
        }
        let fb = 0.7 + REV_ROOM * 0.28;
        // Feed each channel separately (with a little cross-feed) so the tail
        // keeps its stereo width instead of collapsing to mono.
        let il_in = (inl * 0.85 + inr * 0.15) * 0.015;
        let ir_in = (inr * 0.85 + inl * 0.15) * 0.015;
        let mut ol = 0.0f32;
        let mut orr = 0.0f32;
        for c in 0..NC {
            let il = self.combl_i[c];
            let yl = self.combl[c][il];
            self.combl_s[c] = yl * (1.0 - REV_DAMP) + self.combl_s[c] * REV_DAMP;
            self.combl[c][il] = il_in + self.combl_s[c] * fb;
            self.combl_i[c] = il + 1;
            if self.combl_i[c] >= self.clen_l[c] {
                self.combl_i[c] = 0;
            }
            ol += yl;
            let ir = self.combr_i[c];
            let yr = self.combr[c][ir];
            self.combr_s[c] = yr * (1.0 - REV_DAMP) + self.combr_s[c] * REV_DAMP;
            self.combr[c][ir] = ir_in + self.combr_s[c] * fb;
            self.combr_i[c] = ir + 1;
            if self.combr_i[c] >= self.clen_r[c] {
                self.combr_i[c] = 0;
            }
            orr += yr;
        }
        for a in 0..NA {
            let il = self.apl_i[a];
            let bl = self.apl[a][il];
            let yl = -ol + bl;
            self.apl[a][il] = ol + bl * 0.5;
            self.apl_i[a] = il + 1;
            if self.apl_i[a] >= self.alen[a] {
                self.apl_i[a] = 0;
            }
            ol = yl;
            let ir = self.apr_i[a];
            let br = self.apr[a][ir];
            let yr = -orr + br;
            self.apr[a][ir] = orr + br * 0.5;
            self.apr_i[a] = ir + 1;
            if self.apr_i[a] >= self.alen[a] {
                self.apr_i[a] = 0;
            }
            orr = yr;
        }
        let w = self.wet;
        (inl * (1.0 - w * 0.5) + ol * w * 3.0, inr * (1.0 - w * 0.5) + orr * w * 3.0)
    }
}

impl Default for Reverb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clampf_bounds() {
        assert_eq!(clampf(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(clampf(2.0, 0.0, 1.0), 1.0);
        assert_eq!(clampf(0.4, 0.0, 1.0), 0.4);
    }

    #[test]
    fn soft_clips_to_two_thirds() {
        assert_eq!(soft(5.0), 0.666_666_7);
        assert_eq!(soft(-5.0), -0.666_666_7);
        assert_eq!(soft(0.0), 0.0);
        // Strictly compressive inside the range.
        assert!(soft(0.9) < 0.9 && soft(0.9) > 0.0);
    }

    #[test]
    fn rng01_is_deterministic_and_in_range() {
        let mut a = 0x1234_5678u32;
        let mut b = 0x1234_5678u32;
        for _ in 0..1000 {
            let x = rng01(&mut a);
            assert_eq!(x, rng01(&mut b)); // same seed → same stream
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn sample_at_hits_grid_points() {
        let s = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        // Integer positions return the sample exactly (Catmull-Rom passes through).
        for i in 1..s.len() - 2 {
            assert!((sample_at(&s, i as f32) - s[i]).abs() < 1e-5);
        }
        assert_eq!(sample_at(&s, 100.0), 0.0); // out of range
        assert_eq!(sample_at(&[1.0, 2.0], 0.0), 0.0); // too short
    }

    #[test]
    fn win_at_endpoints() {
        let w = [0.0, 0.5, 1.0];
        assert_eq!(win_at(&w, 0.0), 0.0);
        assert_eq!(win_at(&w, 1.0), 1.0);
        assert_eq!(win_at(&w, 2.0), 0.0); // past the end
        assert_eq!(win_at(&[], 0.5), 0.0); // empty
    }

    // Reference Freeverb-lite, a verbatim copy of the engines' original inline
    // implementation (Vec storage, wrap at buffer length). The shared `Reverb`
    // must match it sample-for-sample, which is what proves the refactor kept
    // the render bit-identical.
    struct RefReverb {
        combl: Vec<Vec<f32>>,
        combr: Vec<Vec<f32>>,
        combl_i: [usize; NC],
        combr_i: [usize; NC],
        combl_s: [f32; NC],
        combr_s: [f32; NC],
        apl: Vec<Vec<f32>>,
        apr: Vec<Vec<f32>>,
        apl_i: [usize; NA],
        apr_i: [usize; NA],
        wet: f32,
    }

    impl RefReverb {
        fn new(sr: f32, wet: f32) -> Self {
            let k = sr / 44100.0;
            let mk = |base: usize| vec![0.0f32; ((base as f32 * k) as usize).max(4)];
            RefReverb {
                combl: CLEN_L.iter().map(|&b| mk(b)).collect(),
                combr: CLEN_R.iter().map(|&b| mk(b)).collect(),
                combl_i: [0; NC],
                combr_i: [0; NC],
                combl_s: [0.0; NC],
                combr_s: [0.0; NC],
                apl: ALEN.iter().map(|&b| mk(b)).collect(),
                apr: ALEN.iter().map(|&b| mk(b)).collect(),
                apl_i: [0; NA],
                apr_i: [0; NA],
                wet,
            }
        }
        fn process(&mut self, inl: f32, inr: f32) -> (f32, f32) {
            if self.wet <= 0.0 {
                return (inl, inr);
            }
            let fb = 0.7 + REV_ROOM * 0.28;
            let il_in = (inl * 0.85 + inr * 0.15) * 0.015;
            let ir_in = (inr * 0.85 + inl * 0.15) * 0.015;
            let mut ol = 0.0f32;
            let mut orr = 0.0f32;
            for c in 0..NC {
                let il = self.combl_i[c];
                let yl = self.combl[c][il];
                self.combl_s[c] = yl * (1.0 - REV_DAMP) + self.combl_s[c] * REV_DAMP;
                self.combl[c][il] = il_in + self.combl_s[c] * fb;
                self.combl_i[c] = il + 1;
                if self.combl_i[c] >= self.combl[c].len() {
                    self.combl_i[c] = 0;
                }
                ol += yl;
                let ir = self.combr_i[c];
                let yr = self.combr[c][ir];
                self.combr_s[c] = yr * (1.0 - REV_DAMP) + self.combr_s[c] * REV_DAMP;
                self.combr[c][ir] = ir_in + self.combr_s[c] * fb;
                self.combr_i[c] = ir + 1;
                if self.combr_i[c] >= self.combr[c].len() {
                    self.combr_i[c] = 0;
                }
                orr += yr;
            }
            for a in 0..NA {
                let il = self.apl_i[a];
                let bl = self.apl[a][il];
                let yl = -ol + bl;
                self.apl[a][il] = ol + bl * 0.5;
                self.apl_i[a] = il + 1;
                if self.apl_i[a] >= self.apl[a].len() {
                    self.apl_i[a] = 0;
                }
                ol = yl;
                let ir = self.apr_i[a];
                let br = self.apr[a][ir];
                let yr = -orr + br;
                self.apr[a][ir] = orr + br * 0.5;
                self.apr_i[a] = ir + 1;
                if self.apr_i[a] >= self.apr[a].len() {
                    self.apr_i[a] = 0;
                }
                orr = yr;
            }
            let w = self.wet;
            (inl * (1.0 - w * 0.5) + ol * w * 3.0, inr * (1.0 - w * 0.5) + orr * w * 3.0)
        }
    }

    #[test]
    fn reverb_matches_reference_bit_for_bit() {
        for &sr in &[44100.0f32, 48000.0, 96000.0] {
            let mut rv = Reverb::new();
            rv.set_sample_rate(sr);
            rv.set_wet(0.35);
            let mut rf = RefReverb::new(sr, 0.35);

            let mut seed = 0x9e37_79b9u32;
            for _ in 0..20_000 {
                let l = rng01(&mut seed) * 2.0 - 1.0;
                let r = rng01(&mut seed) * 2.0 - 1.0;
                let (al, ar) = rv.process(l, r);
                let (bl, br) = rf.process(l, r);
                assert_eq!(al.to_bits(), bl.to_bits(), "L mismatch @ {sr} Hz");
                assert_eq!(ar.to_bits(), br.to_bits(), "R mismatch @ {sr} Hz");
            }
        }
    }

    #[test]
    fn reverb_dry_passthrough_when_wet_zero() {
        let mut rv = Reverb::new(); // wet defaults to 0
        assert_eq!(rv.process(0.7, -0.3), (0.7, -0.3));
    }

    #[test]
    fn dc_blocker_removes_offset_and_is_deterministic() {
        let mut a = DcBlocker::new();
        let mut b = DcBlocker::new();
        a.set_sample_rate(48000.0);
        b.set_sample_rate(48000.0);
        // Feed a constant offset; the highpass should pull the output toward 0.
        let mut last = 1.0f32;
        for _ in 0..48_000 {
            let (yl, yr) = a.process(1.0, 1.0);
            let (cl, _cr) = b.process(1.0, 1.0); // same input, same state → identical
            assert_eq!(yl.to_bits(), cl.to_bits());
            assert_eq!(yl, yr);
            last = yl;
        }
        assert!(last.abs() < 0.01, "DC not blocked: residual {last}");
    }
}

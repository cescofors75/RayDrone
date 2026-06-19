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
}

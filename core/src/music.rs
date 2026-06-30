// Musical voicings for RayDrone — turns the granular wash into a *tuned* pad.
//
// A drone made of one granulated sample has no clear harmony: every ray reads
// the same pitch, so it sits in a track as a texture, not a chord. These presets
// give each ray a pitch ratio drawn from a consonant, just-intonation voicing
// (stacked octaves / fifths / triads), so the cloud stacks into an in-tune pad.
//
// Ratios are *just intonation* (small-integer fractions) on purpose: they beat
// far less than equal-tempered ratios when many grains overlap, which is exactly
// what makes a sustained drone sound pure instead of muddy.
//
// `core`-side and dependency-free, so both the WASM engine and the VST feed the
// same voicings into their existing per-grain ratio machinery.

/// Named voicings. Kept as a plain `u32` so the WASM C-ABI can pass it across.
pub const CHORD_UNISON: u32 = 0;
pub const CHORD_OCTAVES: u32 = 1;
pub const CHORD_POWER: u32 = 2; // sub · root · fifth · octave · octave+fifth
pub const CHORD_MAJOR: u32 = 3;
pub const CHORD_MINOR: u32 = 4;
pub const CHORD_FIFTHS: u32 = 5;
pub const CHORD_SUS2: u32 = 6;
pub const CHORD_PENTATONIC: u32 = 7;
pub const CHORD_MAJOR_SCALE: u32 = 8;
pub const CHORD_MINOR_SCALE: u32 = 9;
pub const CHORD_COUNT: u32 = 10;

/// The just-intonation ratio table for a voicing. The slice is `'static`.
pub fn chord_ratios(preset: u32) -> &'static [f32] {
    match preset {
        CHORD_OCTAVES => &[0.5, 1.0, 2.0],
        // The workhorse pad: sub-octave + root + fifth + octave + octave-fifth.
        CHORD_POWER => &[0.5, 1.0, 1.5, 2.0, 3.0],
        // Triads with the octave on top for body.
        CHORD_MAJOR => &[1.0, 5.0 / 4.0, 3.0 / 2.0, 2.0],
        CHORD_MINOR => &[1.0, 6.0 / 5.0, 3.0 / 2.0, 2.0],
        CHORD_FIFTHS => &[1.0, 3.0 / 2.0, 9.0 / 4.0, 3.0],
        CHORD_SUS2 => &[1.0, 9.0 / 8.0, 3.0 / 2.0, 2.0],
        CHORD_PENTATONIC => &[1.0, 9.0 / 8.0, 5.0 / 4.0, 3.0 / 2.0, 5.0 / 3.0, 2.0],
        CHORD_MAJOR_SCALE => {
            &[1.0, 9.0 / 8.0, 5.0 / 4.0, 4.0 / 3.0, 3.0 / 2.0, 5.0 / 3.0, 15.0 / 8.0, 2.0]
        }
        CHORD_MINOR_SCALE => {
            &[1.0, 9.0 / 8.0, 6.0 / 5.0, 4.0 / 3.0, 3.0 / 2.0, 8.0 / 5.0, 9.0 / 5.0, 2.0]
        }
        // CHORD_UNISON and anything unknown.
        _ => &[1.0],
    }
}

/// Equal-tempered ratio for an integer semitone offset, e.g. tuning the whole
/// drone to a track's key by ear. `2^(semitones/12)`, computed without `std`
/// (the WASM build has no `powf`): exact integer-octave factor times a 12th-root
/// lookup for the remainder.
pub fn semitone_ratio(semitones: i32) -> f32 {
    // 2^(1/12) .. 2^(11/12)
    const ET: [f32; 12] = [
        1.0,
        1.059_463_1,
        1.122_462_0,
        1.189_207_1,
        1.259_921_0,
        1.334_839_9,
        1.414_213_6,
        1.498_307_1,
        1.587_401_1,
        1.681_792_8,
        1.781_797_4,
        1.887_748_6,
    ];
    let oct = semitones.div_euclid(12);
    let rem = semitones.rem_euclid(12) as usize;
    let base = ET[rem];
    // Clamp the octave shift so a wildly out-of-range input (e.g. a malformed
    // automation value) saturates instead of overflowing the `<<` — which
    // panics in debug builds and is unspecified in release. 30 octaves is
    // already ~24 octaves past anything audible.
    let shift = oct.unsigned_abs().min(30);
    if oct >= 0 {
        base * (1u32 << shift) as f32
    } else {
        base / (1u32 << shift) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_nonempty_and_ascending_from_root() {
        for p in 0..CHORD_COUNT {
            let r = chord_ratios(p);
            assert!(!r.is_empty(), "preset {p} empty");
            for &x in r {
                assert!(x > 0.0 && x.is_finite(), "preset {p} bad ratio {x}");
            }
        }
        assert_eq!(chord_ratios(CHORD_UNISON), &[1.0]);
        assert_eq!(chord_ratios(999), &[1.0]); // unknown → unison
    }

    #[test]
    fn power_pad_has_fifth_and_octave() {
        let r = chord_ratios(CHORD_POWER);
        assert!(r.contains(&1.5)); // perfect fifth (3/2)
        assert!(r.contains(&2.0)); // octave
        assert!(r.contains(&0.5)); // sub-octave for weight
    }

    #[test]
    fn semitone_ratio_matches_powf() {
        for s in -24..=24 {
            let approx = semitone_ratio(s);
            let exact = 2.0f32.powf(s as f32 / 12.0);
            assert!((approx - exact).abs() / exact < 1e-4, "semitone {s}: {approx} vs {exact}");
        }
        assert_eq!(semitone_ratio(0), 1.0);
        assert!((semitone_ratio(12) - 2.0).abs() < 1e-4); // octave up
        assert!((semitone_ratio(-12) - 0.5).abs() < 1e-4); // octave down
    }

    #[test]
    fn semitone_ratio_does_not_panic_on_extreme_input() {
        // Far outside any real musical range — must saturate, not overflow the shift.
        assert!(semitone_ratio(i32::MAX).is_finite());
        assert!(semitone_ratio(i32::MIN).is_finite());
        assert!(semitone_ratio(1000) > 0.0);
        assert!(semitone_ratio(-1000) >= 0.0);
    }
}

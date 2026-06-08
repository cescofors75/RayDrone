// RayDrone — motor granular continuo en Rust (no_std, sin dependencias).
//
// Cada grano es un "rayo" lanzado a un offset aleatorio dentro del cono de dispersión
// alrededor del foco. Se mezclan muestra a muestra (sin nodos, sin tope de voces, sin
// jitter). Incluye: muestreo random/stratified/QMC, aberración cromática por bandas,
// rebotes (Russian roulette), autoevolución recursiva, paneo estéreo y octava (shimmer).
//
// Compilar (ver build.sh):
//   rustc --edition 2021 --target wasm32-unknown-unknown -O -C panic=abort \
//         --crate-type=cdylib raydrone.rs -o raydrone.wasm

#![no_std]
#![allow(static_mut_refs)]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

const SAMPLE_CAP: usize = 4_000_000; // ~90 s @44.1k mono
const WIN: usize = 2048;
const MAX_VOICES: usize = 512;
const BLOCK: usize = 256;
const SLOG_CAP: usize = 512;

static mut SAMPLE: [f32; SAMPLE_CAP] = [0.0; SAMPLE_CAP];
static mut WINDOW: [f32; WIN] = [0.0; WIN];
static mut OUTL: [f32; BLOCK] = [0.0; BLOCK];
static mut OUTR: [f32; BLOCK] = [0.0; BLOCK];

static mut SAMPLE_LEN: usize = 0;
static mut SR: f32 = 44100.0;

// Parámetros base
static mut FOCUS: f32 = 0.3;
static mut APERTURE: f32 = 0.1;
static mut GRAIN_DUR: f32 = 0.15;
static mut GRAIN_RATE: f32 = 200.0;
static mut GAIN: f32 = 0.3;
static mut MASTER: f32 = 1.0;

static mut SPAWN_ACC: f32 = 0.0;
static mut RNG: u32 = 0x1234_5678;

// Muestreo: 0 random, 1 QMC, 2 stratified
static mut MODE: u32 = 1;
static mut QMC: f32 = 0.5;
static mut STRAT_I: u32 = 0;
const GOLDEN: f32 = 0.618_034;
const STRATA: u32 = 17;

// FX
static mut ABER: f32 = 0.0;
static mut A_LOW: f32 = 0.06;
static mut A_HIGH: f32 = 0.35;
static mut BOUNCES: u32 = 0;
static mut REFL: f32 = 0.5;
static mut FEEDBACK: f32 = 0.0;
static mut ENV: f32 = 0.0;
static mut EVO: f32 = 0.5;
static mut EVO_DIR: f32 = 1.0;

// Espacio: ancho estéreo y probabilidad de octava (shimmer)
static mut WIDTH: f32 = 0.0;
static mut OCT: f32 = 0.0;
static mut PITCH_STEP: f32 = 1.0; // multiplicador de velocidad de lectura (transposición)

// Registro de rayos para la visualización (offset en seg + banda)
static mut SLOG: [f32; SLOG_CAP] = [0.0; SLOG_CAP];
static mut SLOG_B: [f32; SLOG_CAP] = [0.0; SLOG_CAP];
static mut SLOG_W: u32 = 0;

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    pos: f32,
    age: f32,
    inv_dur: f32,
    gain: f32,
    band: u8,
    lp: f32,
    depth: u32,
    step: f32, // velocidad de lectura (1.0 normal, 2.0 octava arriba)
    panl: f32,
    panr: f32,
}

static mut VOICES: [Voice; MAX_VOICES] = [Voice {
    active: false,
    pos: 0.0,
    age: 0.0,
    inv_dur: 0.0,
    gain: 0.0,
    band: 1,
    lp: 0.0,
    depth: 0,
    step: 1.0,
    panl: 0.707,
    panr: 0.707,
}; MAX_VOICES];

#[inline]
fn rng01() -> f32 {
    unsafe {
        let mut x = RNG;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        RNG = x;
        (x as f32) * (1.0 / 4_294_967_296.0)
    }
}

#[inline]
fn clampf(x: f32, a: f32, b: f32) -> f32 {
    if x < a {
        a
    } else if x > b {
        b
    } else {
        x
    }
}

#[inline]
fn sqrtf(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut y = f32::from_bits((x.to_bits() >> 1) + 0x1fbd_1df5);
    y = 0.5 * (y + x / y);
    y = 0.5 * (y + x / y);
    y = 0.5 * (y + x / y);
    y
}

#[inline]
fn tri_inv(u: f32) -> f32 {
    if u < 0.5 {
        -1.0 + sqrtf(2.0 * u)
    } else {
        1.0 - sqrtf(2.0 * (1.0 - u))
    }
}

#[inline]
fn next_u() -> f32 {
    unsafe {
        match MODE {
            1 => {
                QMC += GOLDEN;
                if QMC >= 1.0 {
                    QMC -= 1.0;
                }
                QMC
            }
            2 => {
                let u = (STRAT_I as f32 + rng01()) / (STRATA as f32);
                STRAT_I = (STRAT_I + 1) % STRATA;
                u
            }
            _ => rng01(),
        }
    }
}

fn update_coeffs() {
    unsafe {
        let tp = 6.283_185_5f32;
        A_LOW = clampf(tp * 500.0 / SR, 0.0, 0.99);
        A_HIGH = clampf(tp * 2500.0 / SR, 0.0, 0.99);
    }
}

#[inline]
fn soft(x: f32) -> f32 {
    if x > 1.0 {
        0.666_666_7
    } else if x < -1.0 {
        -0.666_666_7
    } else {
        x - x * x * x * (1.0 / 3.0)
    }
}

// ── Punteros / capacidad ───────────────────────────────────────────────────
#[no_mangle]
pub extern "C" fn sample_ptr() -> *mut f32 {
    unsafe { SAMPLE.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn window_ptr() -> *mut f32 {
    unsafe { WINDOW.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn out_l_ptr() -> *mut f32 {
    unsafe { OUTL.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn out_r_ptr() -> *mut f32 {
    unsafe { OUTR.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn sample_capacity() -> usize {
    SAMPLE_CAP
}
#[no_mangle]
pub extern "C" fn slog_ptr() -> *mut f32 {
    unsafe { SLOG.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn slog_b_ptr() -> *mut f32 {
    unsafe { SLOG_B.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn slog_w() -> u32 {
    unsafe { SLOG_W }
}
#[no_mangle]
pub extern "C" fn slog_cap() -> usize {
    SLOG_CAP
}
#[no_mangle]
pub extern "C" fn out_level() -> f32 {
    unsafe { ENV }
}

// ── Setters ────────────────────────────────────────────────────────────────
#[no_mangle]
pub extern "C" fn set_sample(len: usize, sr: f32) {
    unsafe {
        SAMPLE_LEN = if len > SAMPLE_CAP { SAMPLE_CAP } else { len };
        SR = if sr > 1.0 { sr } else { 44100.0 };
    }
    update_coeffs();
}

#[no_mangle]
pub extern "C" fn set_params(focus: f32, aperture: f32, grain_ms: f32, grain_rate: f32, gain: f32, master: f32) {
    unsafe {
        FOCUS = focus;
        APERTURE = if aperture < 0.0 { 0.0 } else { aperture };
        GRAIN_DUR = grain_ms * 0.001;
        GRAIN_RATE = if grain_rate < 0.0 { 0.0 } else { grain_rate };
        GAIN = gain;
        MASTER = master;
    }
}

#[no_mangle]
pub extern "C" fn set_mode(m: u32) {
    unsafe {
        MODE = m;
    }
}

#[no_mangle]
pub extern "C" fn set_fx(aber: f32, bounces: u32, refl: f32, feedback: f32) {
    unsafe {
        ABER = clampf(aber, 0.0, 1.0);
        BOUNCES = bounces;
        REFL = clampf(refl, 0.0, 1.0);
        FEEDBACK = clampf(feedback, 0.0, 1.0);
        update_coeffs();
    }
}

#[no_mangle]
pub extern "C" fn set_space(width: f32, oct: f32) {
    unsafe {
        WIDTH = clampf(width, 0.0, 1.0);
        OCT = clampf(oct, 0.0, 1.0);
    }
}

// Transposición: el multiplicador 2^(semitons/12) se calcula en JS (sin exp en no_std).
#[no_mangle]
pub extern "C" fn set_pitch(mult: f32) {
    unsafe {
        PITCH_STEP = if mult > 0.01 { mult } else { 1.0 };
    }
}

#[no_mangle]
pub extern "C" fn seed(s: u32) {
    unsafe {
        RNG = if s == 0 { 1 } else { s };
    }
}

#[inline]
fn sample_at(pos: f32) -> f32 {
    unsafe {
        if SAMPLE_LEN < 2 {
            return 0.0;
        }
        let i = pos as usize;
        if i + 1 >= SAMPLE_LEN {
            return 0.0;
        }
        let frac = pos - (i as f32);
        SAMPLE[i] * (1.0 - frac) + SAMPLE[i + 1] * frac
    }
}

#[inline]
fn win_at(ph: f32) -> f32 {
    unsafe {
        let idx = (ph * ((WIN - 1) as f32)) as usize;
        if idx >= WIN {
            0.0
        } else {
            WINDOW[idx]
        }
    }
}

#[inline]
fn band_filter(i: usize, x: f32) -> f32 {
    unsafe {
        if ABER <= 0.0 {
            return x;
        }
        match VOICES[i].band {
            0 => {
                VOICES[i].lp += A_LOW * (x - VOICES[i].lp);
                VOICES[i].lp
            }
            2 => {
                VOICES[i].lp += A_HIGH * (x - VOICES[i].lp);
                x - VOICES[i].lp
            }
            _ => x,
        }
    }
}

#[inline]
fn log_push(off_sec: f32, band: u8) {
    unsafe {
        let w = (SLOG_W as usize) % SLOG_CAP;
        SLOG[w] = off_sec;
        SLOG_B[w] = band as f32;
        SLOG_W = SLOG_W.wrapping_add(1);
    }
}

fn alloc_voice() -> usize {
    unsafe {
        let mut oldest = -1.0f32;
        let mut oldest_i = 0usize;
        for i in 0..MAX_VOICES {
            if !VOICES[i].active {
                return i;
            }
            if VOICES[i].age > oldest {
                oldest = VOICES[i].age;
                oldest_i = i;
            }
        }
        oldest_i
    }
}

// Coloca un grano (usado por granos nuevos y rebotes). Decide octava, transposición y paneo.
fn place(pos: f32, band: u8, depth: u32) {
    unsafe {
        let dur_samp = GRAIN_DUR * SR;
        if dur_samp < 1.0 {
            return;
        }
        let step = (if rng01() < OCT { 2.0 } else { 1.0 }) * PITCH_STEP; // octava (shimmer) × transposición
        let pan = (rng01() * 2.0 - 1.0) * WIDTH; // paneo aleatorio según el ancho
        let panl = sqrtf((1.0 - pan) * 0.5); // equal-power
        let panr = sqrtf((1.0 + pan) * 0.5);
        let slot = alloc_voice();
        VOICES[slot] = Voice {
            active: true,
            pos,
            age: 0.0,
            inv_dur: 1.0 / dur_samp,
            gain: GAIN,
            band,
            lp: 0.0,
            depth,
            step,
            panl,
            panr,
        };
        log_push(pos / SR, band);
    }
}

fn spawn() {
    unsafe {
        if SAMPLE_LEN < 2 {
            return;
        }
        let r = rng01();
        let band: u8 = if r < 0.4 {
            0
        } else if r < 0.8 {
            1
        } else {
            2
        };
        let scale = if ABER <= 0.0 {
            1.0
        } else {
            match band {
                0 => 1.0 + ABER * 2.2,
                2 => clampf(1.0 - ABER * 0.72, 0.08, 1.0),
                _ => 1.0,
            }
        };
        let span = (SAMPLE_LEN as f32) / SR;
        let eff_focus = clampf(FOCUS + (EVO - 0.5) * FEEDBACK * span, 0.0, span);
        let eff_ap = APERTURE * (1.0 + FEEDBACK * ENV * 1.5);
        let off_sec = eff_focus + tri_inv(next_u()) * eff_ap * scale;
        let maxp = (SAMPLE_LEN - 2) as f32;
        let pos = clampf(off_sec * SR, 0.0, maxp);
        place(pos, band, BOUNCES);
    }
}

#[no_mangle]
pub extern "C" fn process(frames: usize) {
    unsafe {
        let n = if frames > BLOCK { BLOCK } else { frames };
        let rate_per_sample = GRAIN_RATE / SR;
        let maxp = if SAMPLE_LEN >= 2 {
            (SAMPLE_LEN - 2) as f32
        } else {
            0.0
        };
        for f in 0..n {
            SPAWN_ACC += rate_per_sample;
            while SPAWN_ACC >= 1.0 {
                spawn();
                SPAWN_ACC -= 1.0;
            }
            let mut accl = 0.0f32;
            let mut accr = 0.0f32;
            for i in 0..MAX_VOICES {
                if !VOICES[i].active {
                    continue;
                }
                let ph = VOICES[i].age * VOICES[i].inv_dur;
                if ph >= 1.0 {
                    let dep = VOICES[i].depth;
                    let bnd = VOICES[i].band;
                    let endpos = VOICES[i].pos;
                    VOICES[i].active = false;
                    if dep > 0 && rng01() < REFL {
                        let jitter = (rng01() - 0.5) * 0.1 * SR;
                        let cpos = clampf(endpos + jitter, 0.0, maxp);
                        place(cpos, bnd, dep - 1);
                    }
                    continue;
                }
                let raw = sample_at(VOICES[i].pos);
                let s = band_filter(i, raw) * win_at(ph) * VOICES[i].gain;
                accl += s * VOICES[i].panl;
                accr += s * VOICES[i].panr;
                VOICES[i].pos += VOICES[i].step;
                VOICES[i].age += 1.0;
            }
            OUTL[f] = soft(accl * MASTER);
            OUTR[f] = soft(accr * MASTER);
        }

        // Envolvente (para el medidor) + autoevolución recursiva.
        let mut s = 0.0f32;
        for f in 0..n {
            let l = if OUTL[f] < 0.0 { -OUTL[f] } else { OUTL[f] };
            let r = if OUTR[f] < 0.0 { -OUTR[f] } else { OUTR[f] };
            s += (l + r) * 0.5;
        }
        let blk = s / (n as f32);
        ENV = ENV * 0.9 + blk * 0.1;
        if FEEDBACK > 0.0 {
            let step = FEEDBACK * (0.0006 + ENV * 0.004);
            EVO += EVO_DIR * step;
            if EVO >= 1.0 {
                EVO = 1.0;
                EVO_DIR = -1.0;
            }
            if EVO <= 0.0 {
                EVO = 0.0;
                EVO_DIR = 1.0;
            }
        }
    }
}

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

// ── Reverb (Freeverb-lite): 4 combs + 2 allpass por canal, estéreo ──────────
const NC: usize = 4;
const NA: usize = 2;
// Capacidad para hasta 96 kHz (las longitudes base están en muestras @44.1k).
const CMAX: usize = 3600;
const AMAX: usize = 1280;
const CLEN_L: [usize; NC] = [1557, 1617, 1491, 1422]; // @44100
const CLEN_R: [usize; NC] = [1580, 1640, 1514, 1445]; // +stereo spread
const ALEN: [usize; NA] = [556, 441];
// Longitudes efectivas escaladas al SR real (update_coeffs) → mismo tiempo de
// reverb a 44.1k, 48k o 96k.
static mut CLEN_L_RT: [usize; NC] = [1557, 1617, 1491, 1422];
static mut CLEN_R_RT: [usize; NC] = [1580, 1640, 1514, 1445];
static mut ALEN_RT: [usize; NA] = [556, 441];
const REV_ROOM: f32 = 0.84; // tamaño/feedback fijo
const REV_DAMP: f32 = 0.5;  // amortiguación de agudos
static mut COMBL: [[f32; CMAX]; NC] = [[0.0; CMAX]; NC];
static mut COMBR: [[f32; CMAX]; NC] = [[0.0; CMAX]; NC];
static mut COMBL_I: [usize; NC] = [0; NC];
static mut COMBR_I: [usize; NC] = [0; NC];
static mut COMBL_S: [f32; NC] = [0.0; NC];
static mut COMBR_S: [f32; NC] = [0.0; NC];
static mut APL: [[f32; AMAX]; NA] = [[0.0; AMAX]; NA];
static mut APR: [[f32; AMAX]; NA] = [[0.0; AMAX]; NA];
static mut APL_I: [usize; NA] = [0; NA];
static mut APR_I: [usize; NA] = [0; NA];
static mut REV_WET: f32 = 0.0; // mezcla de reverb (0 = seco)

// DC blocker a la salida (one-pole highpass ~10 Hz) — quita offset/retumbe acumulado.
// El coeficiente se recalcula con el SR real en update_coeffs().
static mut DC_R: f32 = 0.998_575;
static mut DC_XL: f32 = 0.0;
static mut DC_YL: f32 = 0.0;
static mut DC_XR: f32 = 0.0;
static mut DC_YR: f32 = 0.0;

// Micro-detune por grano (±0.25% ≈ ±4 cents): batidos entre granos → drone lush, no estático.
const DETUNE: f32 = 0.005;

// ── Trazado inverso: envolvente de energía del sample para lanzar los rayos
// hacia donde hay señal (importance desde la estructura de la fuente).
const EBINS: usize = 1024;
static mut ENERGY: [f32; EBINS] = [0.0; EBINS];
static mut EMAX: f32 = 0.000001;
static mut SMART: u32 = 0; // 1 = rayos inteligentes (trazado inverso)

// Registro de rayos para la visualización (offset en seg + banda)
static mut SLOG: [f32; SLOG_CAP] = [0.0; SLOG_CAP];
static mut SLOG_B: [f32; SLOG_CAP] = [0.0; SLOG_CAP];
static mut SLOG_W: u32 = 0;

// Diagnóstico de rendimiento
static mut SPAWN_COUNT: u32 = 0; // total de granos disparados (para granos/seg)

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

// Lista compacta de voces activas + pila de slots libres: el bucle caliente
// recorre solo las voces que suenan (O(activas)), no las 512.
static mut ACTIVE: [u16; MAX_VOICES] = [0; MAX_VOICES];
static mut NACTIVE: usize = 0;
static mut FREE: [u16; MAX_VOICES] = [0; MAX_VOICES];
static mut NFREE: usize = 0;
static mut VINIT: bool = false;

#[inline]
fn ensure_voice_init() {
    unsafe {
        if !VINIT {
            for i in 0..MAX_VOICES {
                FREE[i] = (MAX_VOICES - 1 - i) as u16;
            }
            NFREE = MAX_VOICES;
            NACTIVE = 0;
            VINIT = true;
        }
    }
}

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
        // DC blocker ~10 Hz al SR real
        DC_R = clampf(1.0 - tp * 10.0 / SR, 0.9, 0.99999);
        // Reverb: longitudes en muestras escaladas para mantener el mismo
        // tiempo de cola a cualquier SR (las bases están en @44.1k).
        let k = SR / 44100.0;
        for c in 0..NC {
            let l = ((CLEN_L[c] as f32) * k) as usize;
            let r = ((CLEN_R[c] as f32) * k) as usize;
            CLEN_L_RT[c] = if l < 4 { 4 } else if l > CMAX { CMAX } else { l };
            CLEN_R_RT[c] = if r < 4 { 4 } else if r > CMAX { CMAX } else { r };
            if COMBL_I[c] >= CLEN_L_RT[c] {
                COMBL_I[c] = 0;
            }
            if COMBR_I[c] >= CLEN_R_RT[c] {
                COMBR_I[c] = 0;
            }
        }
        for a in 0..NA {
            let l = ((ALEN[a] as f32) * k) as usize;
            ALEN_RT[a] = if l < 4 { 4 } else if l > AMAX { AMAX } else { l };
            if APL_I[a] >= ALEN_RT[a] {
                APL_I[a] = 0;
            }
            if APR_I[a] >= ALEN_RT[a] {
                APR_I[a] = 0;
            }
        }
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
// Diagnóstico: nº de voces (rayos) activas mezcladas por bloque.
#[no_mangle]
pub extern "C" fn active_voices() -> u32 {
    unsafe { NACTIVE as u32 }
}
#[no_mangle]
pub extern "C" fn spawn_count() -> u32 {
    unsafe { SPAWN_COUNT }
}

// ── Setters ────────────────────────────────────────────────────────────────
#[no_mangle]
pub extern "C" fn set_sample(len: usize, sr: f32) {
    unsafe {
        SAMPLE_LEN = if len > SAMPLE_CAP { SAMPLE_CAP } else { len };
        SR = if sr > 1.0 { sr } else { 44100.0 };
    }
    update_coeffs();
    build_energy();
}

// Envolvente de energía (RMS por bin) de todo el sample, para el trazado inverso.
fn build_energy() {
    unsafe {
        EMAX = 0.000001;
        if SAMPLE_LEN < 2 {
            let mut b = 0;
            while b < EBINS {
                ENERGY[b] = 0.0;
                b += 1;
            }
            return;
        }
        for b in 0..EBINS {
            let start = b * SAMPLE_LEN / EBINS;
            let end = ((b + 1) * SAMPLE_LEN / EBINS).min(SAMPLE_LEN);
            let mut s = 0.0f32;
            let mut cnt = 0u32;
            let mut i = start;
            while i < end {
                let v = SAMPLE[i];
                s += v * v;
                cnt += 1;
                i += 4; // stride para abaratar
            }
            let e = if cnt > 0 { sqrtf(s / (cnt as f32)) } else { 0.0 };
            ENERGY[b] = e;
            if e > EMAX {
                EMAX = e;
            }
        }
    }
}

#[inline]
fn energy_at(sec: f32) -> f32 {
    unsafe {
        let span = (SAMPLE_LEN as f32) / SR;
        if span <= 0.0 {
            return 0.0;
        }
        let mut b = (sec / span * (EBINS as f32)) as usize;
        if b >= EBINS {
            b = EBINS - 1;
        }
        ENERGY[b]
    }
}

#[no_mangle]
pub extern "C" fn set_smart(on: u32) {
    unsafe {
        SMART = on;
    }
}

#[no_mangle]
pub extern "C" fn set_reverb(wet: f32) {
    unsafe {
        REV_WET = clampf(wet, 0.0, 1.0);
    }
}

// Reverb estéreo Freeverb-lite (4 combs + 2 allpass por canal).
#[inline]
fn reverb(inl: f32, inr: f32) -> (f32, f32) {
    unsafe {
        if REV_WET <= 0.0 {
            return (inl, inr);
        }
        let fb = 0.7 + REV_ROOM * 0.28;
        // Alimentar cada canal por separado (con un poco de cross-feed) preserva el
        // ancho estéreo en la cola, en vez de colapsar la reverb a mono.
        let il_in = (inl * 0.85 + inr * 0.15) * 0.015;
        let ir_in = (inr * 0.85 + inl * 0.15) * 0.015;
        let mut ol = 0.0f32;
        let mut orr = 0.0f32;
        for c in 0..NC {
            let il = COMBL_I[c];
            let yl = COMBL[c][il];
            COMBL_S[c] = yl * (1.0 - REV_DAMP) + COMBL_S[c] * REV_DAMP;
            COMBL[c][il] = il_in + COMBL_S[c] * fb;
            COMBL_I[c] = il + 1;
            if COMBL_I[c] >= CLEN_L_RT[c] {
                COMBL_I[c] = 0;
            }
            ol += yl;
            let ir = COMBR_I[c];
            let yr = COMBR[c][ir];
            COMBR_S[c] = yr * (1.0 - REV_DAMP) + COMBR_S[c] * REV_DAMP;
            COMBR[c][ir] = ir_in + COMBR_S[c] * fb;
            COMBR_I[c] = ir + 1;
            if COMBR_I[c] >= CLEN_R_RT[c] {
                COMBR_I[c] = 0;
            }
            orr += yr;
        }
        for a in 0..NA {
            let il = APL_I[a];
            let bl = APL[a][il];
            let yl = -ol + bl;
            APL[a][il] = ol + bl * 0.5;
            APL_I[a] = il + 1;
            if APL_I[a] >= ALEN_RT[a] {
                APL_I[a] = 0;
            }
            ol = yl;
            let ir = APR_I[a];
            let br = APR[a][ir];
            let yr = -orr + br;
            APR[a][ir] = orr + br * 0.5;
            APR_I[a] = ir + 1;
            if APR_I[a] >= ALEN_RT[a] {
                APR_I[a] = 0;
            }
            orr = yr;
        }
        let w = REV_WET;
        (inl * (1.0 - w * 0.5) + ol * w * 3.0, inr * (1.0 - w * 0.5) + orr * w * 3.0)
    }
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
        if SAMPLE_LEN < 4 {
            return 0.0;
        }
        let i = pos as usize;
        if i + 2 >= SAMPLE_LEN {
            return 0.0;
        }
        let frac = pos - (i as f32);
        // Interpolación cúbica Catmull-Rom (4 puntos): menos aliasing y más
        // brillo que la lineal, sobre todo al leer a velocidad != 1 (octava/pitch).
        let s0 = if i >= 1 { SAMPLE[i - 1] } else { SAMPLE[i] };
        let s1 = SAMPLE[i];
        let s2 = SAMPLE[i + 1];
        let s3 = SAMPLE[i + 2];
        let a = -0.5 * s0 + 1.5 * s1 - 1.5 * s2 + 0.5 * s3;
        let b = s0 - 2.5 * s1 + 2.0 * s2 - 0.5 * s3;
        let c = -0.5 * s0 + 0.5 * s2;
        ((a * frac + b) * frac + c) * frac + s1
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
        ensure_voice_init();
        if NFREE > 0 {
            NFREE -= 1;
            let slot = FREE[NFREE] as usize;
            ACTIVE[NACTIVE] = slot as u16;
            NACTIVE += 1;
            return slot;
        }
        // Sin slots libres: robar la voz más APAGADA (fase más avanzada → la cola
        // de la Hann está cerca de 0) en vez de la más vieja → robo sin click.
        let mut best = 0usize;
        let mut best_ph = -1.0f32;
        for k in 0..NACTIVE {
            let v = ACTIVE[k] as usize;
            let ph = VOICES[v].age * VOICES[v].inv_dur;
            if ph > best_ph {
                best_ph = ph;
                best = k;
            }
        }
        ACTIVE[best] as usize // ya está en la lista activa; se reutiliza en sitio
    }
}

// Coloca un grano (usado por granos nuevos y rebotes). Decide octava, transposición y paneo.
fn place(pos: f32, band: u8, depth: u32) {
    unsafe {
        let dur_samp = GRAIN_DUR * SR;
        if dur_samp < 1.0 {
            return;
        }
        let detune = 1.0 + (rng01() - 0.5) * DETUNE; // micro-detune lush (batidos entre granos)
        let step = (if rng01() < OCT { 2.0 } else { 1.0 }) * PITCH_STEP * detune; // octava × transposición × detune
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
        SPAWN_COUNT = SPAWN_COUNT.wrapping_add(1);
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
        // Autoevolución acotada: el barrido del foco y la apertura crecen con FEEDBACK
        // pero con techos suaves, para que valores altos modulen sin volverse caóticos.
        let eff_focus = clampf(FOCUS + (EVO - 0.5) * FEEDBACK * span * 0.45, 0.0, span);
        let eff_ap = APERTURE * (1.0 + FEEDBACK * ENV * 0.8);
        let mut off_sec = eff_focus + tri_inv(next_u()) * eff_ap * scale;
        // Trazado inverso: rejection ∝ energía → los rayos caen donde hay señal.
        if SMART == 1 {
            let mut tries = 0;
            while tries < 6 {
                if energy_at(off_sec) >= EMAX * rng01() {
                    break;
                }
                off_sec = eff_focus + tri_inv(next_u()) * eff_ap * scale;
                tries += 1;
            }
        }
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
            let mut k = 0usize;
            while k < NACTIVE {
                let i = ACTIVE[k] as usize;
                let ph = VOICES[i].age * VOICES[i].inv_dur;
                if ph >= 1.0 {
                    let dep = VOICES[i].depth;
                    let bnd = VOICES[i].band;
                    let endpos = VOICES[i].pos;
                    VOICES[i].active = false;
                    // swap-remove de la lista activa + devolver el slot al pool
                    NACTIVE -= 1;
                    ACTIVE[k] = ACTIVE[NACTIVE];
                    FREE[NFREE] = i as u16;
                    NFREE += 1;
                    if dep > 0 && rng01() < REFL {
                        let jitter = (rng01() - 0.5) * 0.1 * SR;
                        let cpos = clampf(endpos + jitter, 0.0, maxp);
                        place(cpos, bnd, dep - 1);
                    }
                    continue; // ACTIVE[k] ahora es otra voz: no avanzar k
                }
                let raw = sample_at(VOICES[i].pos);
                let s = band_filter(i, raw) * win_at(ph) * VOICES[i].gain;
                accl += s * VOICES[i].panl;
                accr += s * VOICES[i].panr;
                VOICES[i].pos += VOICES[i].step;
                VOICES[i].age += 1.0;
                k += 1;
            }
            let (rl, rr) = reverb(accl * MASTER, accr * MASTER);
            // DC blocker (one-pole highpass): y = x - x1 + R·y1
            let yl = rl - DC_XL + DC_R * DC_YL;
            DC_XL = rl;
            DC_YL = yl;
            let yr = rr - DC_XR + DC_R * DC_YR;
            DC_XR = rr;
            DC_YR = yr;
            OUTL[f] = soft(yl);
            OUTR[f] = soft(yr);
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
            // Barrido más lento y menos dependiente del nivel → evoluciona en vez de saltar.
            let step = FEEDBACK * (0.0004 + ENV * 0.0018);
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

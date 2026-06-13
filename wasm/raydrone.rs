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

// ── Escala microtonal: tabla de ratios de un período (octava, tritava…).
// Vacía (len=0) = pitch continuo, comportamiento de siempre. Cada grano coge
// un grado de la tabla; el muestreo del grado respeta MODE: estratificado
// recorre los grados como estratos exactos (cobertura homogénea), QMC usa una
// secuencia de Kronecker con la constante plástica (R2), decorrelada de la
// áurea que ya muestrea el eje temporal.
const SCALE_CAP: usize = 64;
static mut SCALE: [f32; SCALE_CAP] = [1.0; SCALE_CAP];
static mut SCALE_LEN: usize = 0;
static mut SCALE_I: u32 = 0; // estratificado: round-robin por grados
static mut QMC_P: f32 = 0.5; // QMC pitch (componente R2)
const R2_ALPHA: f32 = 0.754_877_7; // 1/φ₂, φ₂ = constante plástica ≈ 1.324718

// ── Ambient: focos múltiples autónomos + árbol recursivo de focos ───────────
// Modo ambient (AMB_ON=1): en vez de un único FOCUS, una constelación de focos.
// Las SEMILLAS se colocan en baja discrepancia (golden ratio) a lo largo del
// sample y derivan solas (paseo aleatorio lento, independiente por foco).
// Recursión: cada cierto tiempo un foco "padre" (depth>0) engendra un foco hijo
// cerca suyo, con offset/apertura/vida que encogen por nivel → estructura
// auto-similar (secciones → frases → granos comparten la misma ley generativa).
// Cada grano elige su foco ponderado por el peso (envolvente de fade del foco).
const FMAX: usize = 48;
static mut FPOS: [f32; FMAX] = [0.0; FMAX];   // posición (segundos)
static mut FVEL: [f32; FMAX] = [0.0; FMAX];   // deriva (seg/seg)
static mut FW: [f32; FMAX] = [0.0; FMAX];     // peso actual (0..1, con fade)
static mut FWT: [f32; FMAX] = [0.0; FMAX];    // peso objetivo
static mut FDEPTH: [u8; FMAX] = [0; FMAX];    // niveles de recursión restantes
static mut FAGE: [f32; FMAX] = [0.0; FMAX];   // edad (s)
static mut FTTL: [f32; FMAX] = [0.0; FMAX];   // vida (s); <0 = inmortal (semilla)
static mut FACT: [bool; FMAX] = [false; FMAX];
static mut AMB_ON: u32 = 0;
static mut AMB_SEEDS: u32 = 3;
static mut AMB_DEPTH: u32 = 2;
static mut AMB_SPREAD: f32 = 0.25; // offset del hijo = SPREAD·span·shrink
static mut AMB_DRIFT: f32 = 0.3;   // velocidad de deriva
static mut AMB_RATE: f32 = 0.4;    // nacimientos de focos por segundo
static mut AMB_DIRTY: bool = true; // recolocar semillas
static mut FOCI_ACC: f32 = 0.0;    // acumulador de nacimientos

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

// Registro de rayos para la visualización (offset en seg + banda + ratio del grado)
static mut SLOG: [f32; SLOG_CAP] = [0.0; SLOG_CAP];
static mut SLOG_B: [f32; SLOG_CAP] = [0.0; SLOG_CAP];
static mut SLOG_S: [f32; SLOG_CAP] = [1.0; SLOG_CAP];
static mut SLOG_W: u32 = 0;

// Diagnóstico de rendimiento
static mut SPAWN_COUNT: u32 = 0; // total de granos disparados (para granos/seg)

// ── Convergence Lab: el MISMO estimador del motor, medible offline.
// Corre en una instancia aparte de este módulo dentro de un Web Worker (no toca
// el hilo de audio). Acumuladores en f64 para que el suelo de error medido sea
// del estimador, no de la precisión de la suma. RNG sembrable → reproducible.
const LAB_D: usize = 4096;
const LAB_AMAX: usize = 6600;
const LAB_LEN: usize = 2 * LAB_AMAX + 1;
const LAB_GOLDEN64: f64 = 0.618_033_988_749_894_9;
static mut LAB_WIN: [f32; LAB_D] = [0.0; LAB_D];
static mut LAB_TGT: [f64; LAB_D] = [0.0; LAB_D];
static mut LAB_EST: [f64; LAB_D] = [0.0; LAB_D];
static mut LAB_T32: [f32; LAB_D] = [0.0; LAB_D];
static mut LAB_PM: [f32; LAB_LEN] = [0.0; LAB_LEN];
static mut LAB_QM: [f32; LAB_LEN] = [0.0; LAB_LEN];
static mut LAB_CUM: [f32; LAB_LEN] = [0.0; LAB_LEN];
static mut LAB_EN: [f32; LAB_LEN] = [0.0; LAB_LEN];
static mut LAB_EMAX: f32 = 0.000_001;

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
pub extern "C" fn slog_s_ptr() -> *mut f32 {
    unsafe { SLOG_S.as_mut_ptr() }
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
        AMB_DIRTY = true; // recolocar las semillas al span del nuevo sample
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

// ── Escala microtonal ──
#[no_mangle]
pub extern "C" fn scale_ptr() -> *mut f32 {
    unsafe { SCALE.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn scale_capacity() -> usize {
    SCALE_CAP
}
#[no_mangle]
pub extern "C" fn set_scale(len: usize) {
    unsafe {
        SCALE_LEN = if len > SCALE_CAP { SCALE_CAP } else { len };
        SCALE_I = 0;
    }
}

// Grado de la escala para el grano nuevo. La dimensión pitch se muestrea
// aparte de la temporal: estratificado puro sobre los grados (cada grado es
// un estrato → ningún micro-intervalo queda sin cubrir) o Kronecker R2 en QMC.
#[inline]
fn scale_ratio() -> f32 {
    unsafe {
        if SCALE_LEN == 0 {
            return 1.0;
        }
        let n = SCALE_LEN;
        let idx = match MODE {
            1 => {
                QMC_P += R2_ALPHA;
                if QMC_P >= 1.0 {
                    QMC_P -= 1.0;
                }
                (QMC_P * (n as f32)) as usize
            }
            2 => {
                let i = (SCALE_I as usize) % n;
                SCALE_I = SCALE_I.wrapping_add(1);
                i
            }
            _ => (rng01() * (n as f32)) as usize,
        };
        SCALE[if idx >= n { n - 1 } else { idx }]
    }
}

// ── Ambient: focos múltiples + recursión ────────────────────────────────────
#[no_mangle]
pub extern "C" fn set_ambient(on: u32, seeds: u32, depth: u32, spread: f32, drift: f32, rate: f32) {
    unsafe {
        let was = AMB_ON;
        AMB_ON = if on != 0 { 1 } else { 0 };
        let ns = if seeds < 1 { 1 } else if seeds > 8 { 8 } else { seeds };
        if ns != AMB_SEEDS || was != AMB_ON {
            AMB_DIRTY = true; // recolocar semillas si cambia el nº o se enciende
        }
        AMB_SEEDS = ns;
        AMB_DEPTH = if depth > 4 { 4 } else { depth };
        AMB_SPREAD = clampf(spread, 0.0, 1.0);
        AMB_DRIFT = clampf(drift, 0.0, 1.0);
        AMB_RATE = clampf(rate, 0.0, 4.0);
    }
}

#[inline]
fn powf_i(base: f32, n: u32) -> f32 {
    let mut r = 1.0f32;
    let mut i = 0;
    while i < n {
        r *= base;
        i += 1;
    }
    r
}

#[inline]
fn focus_span() -> f32 {
    unsafe {
        let s = (SAMPLE_LEN as f32) / SR;
        if s > 0.0 {
            s
        } else {
            0.0
        }
    }
}

// (Re)colocar las semillas en baja discrepancia (golden ratio) a lo largo del
// sample: máxima dispersión, sin solapes. Inmortales y a peso pleno.
fn reinit_foci() {
    unsafe {
        let span = focus_span();
        for i in 0..FMAX {
            FACT[i] = false;
            FW[i] = 0.0;
        }
        let ns = AMB_SEEDS as usize;
        for i in 0..ns {
            let g = 0.5 + (i as f32) * 0.618_034;
            let u = g - (g as u32 as f32); // parte fraccionaria (g siempre > 0)
            FPOS[i] = u * span;
            FVEL[i] = (rng01() - 0.5) * AMB_DRIFT * span * 0.02;
            FW[i] = 1.0;
            FWT[i] = 1.0;
            FDEPTH[i] = AMB_DEPTH as u8;
            FAGE[i] = 0.0;
            FTTL[i] = -1.0; // inmortal
            FACT[i] = true;
        }
        FOCI_ACC = 0.0;
        AMB_DIRTY = false;
    }
}

fn free_focus_slot() -> i32 {
    unsafe {
        for i in 0..FMAX {
            if !FACT[i] {
                return i as i32;
            }
        }
        -1
    }
}

// Engendrar un foco hijo: padre elegido ponderado entre los que aún tienen
// niveles (depth>0). El hijo cae cerca, con offset/vida/peso que encogen por
// nivel → árbol auto-similar.
fn birth_focus() {
    unsafe {
        let slot = free_focus_slot();
        if slot < 0 {
            return;
        }
        // padre ponderado por peso, con depth>0
        let mut sum = 0.0f32;
        for i in 0..FMAX {
            if FACT[i] && FDEPTH[i] > 0 {
                sum += FW[i];
            }
        }
        if sum <= 0.0 {
            return;
        }
        let mut r = rng01() * sum;
        let mut parent = -1i32;
        for i in 0..FMAX {
            if FACT[i] && FDEPTH[i] > 0 {
                r -= FW[i];
                if r <= 0.0 {
                    parent = i as i32;
                    break;
                }
            }
        }
        if parent < 0 {
            return;
        }
        let p = parent as usize;
        let level = (AMB_DEPTH as i32 - FDEPTH[p] as i32 + 1).max(0) as u32; // 1 en el primer nivel
        let shrink = powf_i(0.55, level);
        let span = focus_span();
        let off = (rng01() - 0.5) * 2.0 * AMB_SPREAD * span * shrink;
        let s = slot as usize;
        FPOS[s] = clampf(FPOS[p] + off, 0.0, span);
        FVEL[s] = (rng01() - 0.5) * AMB_DRIFT * span * 0.02 * (1.0 + shrink);
        FW[s] = 0.0; // fade-in
        FWT[s] = FW[p] * 0.72; // hijos más discretos
        FDEPTH[s] = FDEPTH[p] - 1;
        FAGE[s] = 0.0;
        FTTL[s] = 12.0 * shrink; // vida finita, más corta a escala fina
        FACT[s] = true;
    }
}

// Avanzar la constelación dt segundos: deriva, fades, nacimientos, muertes.
fn update_foci(dt: f32) {
    unsafe {
        if AMB_ON == 0 {
            return;
        }
        let span = focus_span();
        if span <= 0.0 {
            return;
        }
        if AMB_DIRTY {
            reinit_foci();
        }
        let maxv = AMB_DRIFT * span * 0.03;
        let fade = if dt / 1.5 < 1.0 { dt / 1.5 } else { 1.0 };
        for i in 0..FMAX {
            if !FACT[i] {
                continue;
            }
            FAGE[i] += dt;
            // deriva: re-aleatorizar velocidad de vez en cuando (~cada 3 s)
            if rng01() < dt / 3.0 {
                FVEL[i] = (rng01() - 0.5) * 2.0 * maxv;
            }
            let mut pos = FPOS[i] + FVEL[i] * dt;
            if pos < 0.0 {
                pos = -pos;
                FVEL[i] = -FVEL[i];
            } else if pos > span {
                pos = 2.0 * span - pos;
                FVEL[i] = -FVEL[i];
            }
            FPOS[i] = clampf(pos, 0.0, span);
            // envolvente de peso: los mortales se apagan al acercarse a su TTL
            if FTTL[i] >= 0.0 && FAGE[i] > FTTL[i] - 1.5 {
                FWT[i] = 0.0;
            }
            FW[i] += (FWT[i] - FW[i]) * fade;
            if FTTL[i] >= 0.0 && FAGE[i] > FTTL[i] && FW[i] < 0.002 {
                FACT[i] = false;
                FW[i] = 0.0;
            }
        }
        // nacimientos
        if AMB_DEPTH > 0 {
            FOCI_ACC += AMB_RATE * dt;
            let mut guard = 0;
            while FOCI_ACC >= 1.0 && guard < 8 {
                birth_focus();
                FOCI_ACC -= 1.0;
                guard += 1;
            }
        }
    }
}

// Elegir foco ponderado por peso → (índice, factor de apertura por nivel).
// Devuelve índice -1 si no hay constelación (cae al FOCUS único de siempre).
fn pick_focus() -> i32 {
    unsafe {
        let mut sum = 0.0f32;
        for i in 0..FMAX {
            if FACT[i] {
                sum += FW[i];
            }
        }
        if sum <= 0.0 {
            return -1;
        }
        let mut r = rng01() * sum;
        for i in 0..FMAX {
            if FACT[i] {
                r -= FW[i];
                if r <= 0.0 {
                    return i as i32;
                }
            }
        }
        -1
    }
}

// Punteros para la visualización de la constelación (posición en seg + peso).
#[no_mangle]
pub extern "C" fn foci_ptr() -> *mut f32 {
    unsafe { FPOS.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn foci_w_ptr() -> *mut f32 {
    unsafe { FW.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn foci_cap() -> usize {
    FMAX
}

#[no_mangle]
pub extern "C" fn seed(s: u32) {
    unsafe {
        RNG = if s == 0 { 1 } else { s };
    }
}

// ── Convergence Lab ─────────────────────────────────────────────────────────
#[inline]
fn lab_s(i: i64) -> f64 {
    unsafe {
        if i >= 0 && (i as usize) < SAMPLE_LEN {
            SAMPLE[i as usize] as f64
        } else {
            0.0
        }
    }
}

#[inline]
fn fract64(x: f64) -> f64 {
    x - (x as i64 as f64) // válido para x >= 0 (aquí siempre)
}

#[inline]
fn roundi(x: f32) -> i64 {
    if x >= 0.0 {
        (x + 0.5) as i64
    } else {
        -(((-x) + 0.5) as i64)
    }
}

#[inline]
fn lab_clamp_a(a: i32) -> i32 {
    if a < 1 {
        1
    } else if a as usize > LAB_AMAX {
        LAB_AMAX as i32
    } else {
        a
    }
}

#[no_mangle]
pub extern "C" fn lab_win_ptr() -> *mut f32 {
    unsafe { LAB_WIN.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn lab_target_ptr() -> *mut f32 {
    unsafe { LAB_T32.as_mut_ptr() } // copia f32 del objetivo (para el A/B audible)
}
#[no_mangle]
pub extern "C" fn lab_grain() -> usize {
    LAB_D
}
#[no_mangle]
pub extern "C" fn lab_max_a() -> usize {
    LAB_AMAX
}

// Objetivo determinista: tgt[n] = win[n] · Σₖ p(k)·s(f0+k+n), p triangular.
#[no_mangle]
pub extern "C" fn lab_target(f0: i32, a: i32) {
    unsafe {
        let a = lab_clamp_a(a);
        let inv_a2 = 1.0f64 / ((a as f64) * (a as f64));
        for n in 0..LAB_D {
            LAB_TGT[n] = 0.0;
        }
        let mut k = -a;
        while k <= a {
            let pk = ((a - if k < 0 { -k } else { k }) as f64) * inv_a2;
            if pk > 0.0 {
                let b = f0 as i64 + k as i64;
                for n in 0..LAB_D {
                    LAB_TGT[n] += pk * lab_s(b + n as i64);
                }
            }
            k += 1;
        }
        for n in 0..LAB_D {
            LAB_TGT[n] *= LAB_WIN[n] as f64;
            LAB_T32[n] = LAB_TGT[n] as f32;
        }
    }
}

// Distribuciones para importance/reverse: p (triangular), q ∝ p·energía, CDF y energía.
#[no_mangle]
pub extern "C" fn lab_imp_build(f0: i32, a: i32) {
    unsafe {
        let a = lab_clamp_a(a);
        let len = (2 * a + 1) as usize;
        let stride = 8usize;
        let taps = (LAB_D + stride - 1) / stride;
        let inv_a2 = 1.0f32 / ((a * a) as f32);
        let mut ps = 0.0f32;
        let mut qs = 0.0f32;
        LAB_EMAX = 0.000_001;
        for idx in 0..len {
            let k = idx as i32 - a;
            let p = ((a - k.abs()) as f32) * inv_a2;
            let b = f0 as i64 + k as i64;
            let mut s = 0.0f32;
            let mut n = 0usize;
            while n < LAB_D {
                let v = lab_s(b + n as i64) as f32;
                s += v * v;
                n += stride;
            }
            let e = sqrtf(s / (taps as f32)) + 0.000_001;
            LAB_EN[idx] = e;
            if e > LAB_EMAX {
                LAB_EMAX = e;
            }
            LAB_PM[idx] = p;
            LAB_QM[idx] = p * e;
            ps += p;
            qs += p * e;
        }
        let mut acc = 0.0f32;
        for idx in 0..len {
            LAB_PM[idx] /= ps;
            LAB_QM[idx] = if qs > 0.0 { LAB_QM[idx] / qs } else { 1.0 / (len as f32) };
            acc += LAB_QM[idx];
            LAB_CUM[idx] = acc;
        }
    }
}

#[inline]
fn lab_cum_search(len: usize, r: f32) -> usize {
    unsafe {
        let mut lo = 0usize;
        let mut hi = len - 1;
        while lo < hi {
            let m = (lo + hi) >> 1;
            if LAB_CUM[m] < r {
                lo = m + 1;
            } else {
                hi = m;
            }
        }
        lo
    }
}

// Una estimación con N rayos. method: 0 random, 1 stratified, 2 QMC (áurea),
// 3 importance (reponderado, insesgado), 4 reverse (rejection ∝ energía, sesgado
// — exactamente lo que hace el motor en vivo con los rayos inteligentes).
#[no_mangle]
pub extern "C" fn lab_estimate(f0: i32, a: i32, n_rays: u32, method: u32, sd: u32) {
    unsafe {
        let a = lab_clamp_a(a);
        RNG = if sd == 0 { 1 } else { sd };
        let n = if n_rays < 1 { 1 } else { n_rays } as usize;
        for m in 0..LAB_D {
            LAB_EST[m] = 0.0;
        }
        let rot = rng01() as f64;
        if method == 3 {
            let len = (2 * a + 1) as usize;
            let mut ws = 0.0f64;
            for i in 0..n {
                let u = ((i as f32) + rng01()) / (n as f32);
                let idx = lab_cum_search(len, u);
                let k = idx as i32 - a;
                let wi = (LAB_PM[idx] / LAB_QM[idx]) as f64;
                ws += wi;
                let b = f0 as i64 + k as i64;
                for m in 0..LAB_D {
                    LAB_EST[m] += wi * lab_s(b + m as i64);
                }
            }
            let inv = if ws > 0.0 { 1.0 / ws } else { 0.0 };
            for m in 0..LAB_D {
                LAB_EST[m] *= inv * (LAB_WIN[m] as f64);
            }
        } else if method == 4 {
            let len = (2 * a + 1) as i64;
            for _ in 0..n {
                let mut k = roundi(tri_inv(rng01()) * (a as f32));
                let mut tries = 0;
                while tries < 6 {
                    let mut idx = k + a as i64;
                    if idx < 0 {
                        idx = 0;
                    }
                    if idx >= len {
                        idx = len - 1;
                    }
                    if LAB_EN[idx as usize] >= LAB_EMAX * rng01() {
                        break;
                    }
                    k = roundi(tri_inv(rng01()) * (a as f32));
                    tries += 1;
                }
                let b = f0 as i64 + k;
                for m in 0..LAB_D {
                    LAB_EST[m] += lab_s(b + m as i64);
                }
            }
            let inv = 1.0 / (n as f64);
            for m in 0..LAB_D {
                LAB_EST[m] *= inv * (LAB_WIN[m] as f64);
            }
        } else {
            for i in 0..n {
                let u: f32 = match method {
                    1 => ((i as f32) + rng01()) / (n as f32),
                    // f64: con N grande, la fase de Kronecker en f32 perdería la
                    // estructura de baja discrepancia.
                    2 => fract64(rot + (i as f64) * LAB_GOLDEN64) as f32,
                    _ => rng01(),
                };
                let k = roundi(tri_inv(u) * (a as f32));
                let b = f0 as i64 + k;
                for m in 0..LAB_D {
                    LAB_EST[m] += lab_s(b + m as i64);
                }
            }
            let inv = 1.0 / (n as f64);
            for m in 0..LAB_D {
                LAB_EST[m] *= inv * (LAB_WIN[m] as f64);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn lab_rms() -> f32 {
    unsafe {
        let mut s = 0.0f64;
        for m in 0..LAB_D {
            let d = LAB_EST[m] - LAB_TGT[m];
            s += d * d;
        }
        sqrtf((s / (LAB_D as f64)) as f32)
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
fn log_push(off_sec: f32, band: u8, ratio: f32) {
    unsafe {
        let w = (SLOG_W as usize) % SLOG_CAP;
        SLOG[w] = off_sec;
        SLOG_B[w] = band as f32;
        SLOG_S[w] = ratio;
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
        // octava × transposición × grado microtonal × detune: la retícula exacta
        // de la escala + el detune ⇒ enjambre alrededor de cada grado, no comb estático.
        let ratio = scale_ratio();
        let step = (if rng01() < OCT { 2.0 } else { 1.0 }) * PITCH_STEP * ratio * detune;
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
        log_push(pos / SR, band, ratio);
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
        // Foco efectivo: en ambient, un foco de la constelación (ponderado por peso),
        // con apertura más cerrada a escala fina (auto-similar). Si no, el FOCUS
        // único de siempre con su autoevolución acotada por FEEDBACK.
        let (eff_focus, eff_ap);
        let fsel = if AMB_ON == 1 { pick_focus() } else { -1 };
        if fsel >= 0 {
            let fi = fsel as usize;
            let level = (AMB_DEPTH as i32 - FDEPTH[fi] as i32).max(0) as u32;
            eff_focus = clampf(FPOS[fi], 0.0, span);
            eff_ap = APERTURE * (1.0 + FEEDBACK * ENV * 0.8) * powf_i(0.7, level);
        } else {
            eff_focus = clampf(FOCUS + (EVO - 0.5) * FEEDBACK * span * 0.45, 0.0, span);
            eff_ap = APERTURE * (1.0 + FEEDBACK * ENV * 0.8);
        }
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
        // La constelación de focos avanza una vez por bloque (deriva lenta).
        update_foci(n as f32 / SR);
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

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

// Primitivas DSP compartidas con el motor del VST (un único sitio donde viven).
// El crate es no_std y sin deps, así que el build de `rustc` crudo (sin Cargo) lo
// enlaza vía `--extern raydrone_core=...` — ver build.sh. No toca crates.io.
use raydrone_core::{
    clampf, filter::Filter, sample_at as core_sample_at, soft, sqrtf, tri_inv,
    win_at as core_win_at, DcBlocker, Reverb,
};

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

// ── Teclas pulsadas (piano por teclado/ratón/táctil) ────────────────────────
// Ratios de las notas actualmente sostenidas (2^(semitono/12) respecto a la
// raíz), enviados por la UI cada vez que cambia el conjunto de teclas. No
// vacío = cada grano nuevo coge una nota sostenida (así se tocan acordes);
// vacío = se cae al comportamiento de siempre (Microtonal/Voicing o pitch
// continuo). Mismo mecanismo de muestreo por grado que la escala microtonal,
// con su propio acumulador QMC para no correlacionar ambos ejes.
const KEYS_CAP: usize = 16;
static mut KEYS: [f32; KEYS_CAP] = [1.0; KEYS_CAP];
static mut KEYS_LEN: usize = 0;
static mut KEYS_I: u32 = 0; // estratificado: round-robin por nota sostenida
static mut QMC_KEY: f32 = 0.5; // QMC teclas (componente R2, propio)

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

// Reverb (Freeverb-lite) y DC blocker viven en el kernel compartido `raydrone_core`
// (el mismo código que enlaza el VST). Una sola instancia estática de cada uno.
static mut REVERB: Reverb = Reverb::new();
static mut DC: DcBlocker = DcBlocker::new();
// Filtro resonante (SVF) con LFO sincronizable a BPM — vive en core::filter.
static mut FILTER: Filter = Filter::new();

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

// ── Convergence Lab: el mismo ESTIMADOR del motor (mismo kernel triangular,
// mismo objetivo g[n]), medible offline. Ojo con el matiz para el paper: los
// MUESTREADORES del Lab son las formas canónicas — estratificado de N estratos,
// Kronecker áureo en f64 con rotación Cranley–Patterson, e importance
// reponderado p/q (insesgado) — mientras que el motor en vivo usa variantes
// streaming: recurrencia áurea iterativa en f32 sin rotación, 17 estratos
// fijos round-robin, y los "rayos inteligentes" son exactamente el método
// reverse (rejection sin reponderar, sesgado). Las curvas medidas aquí
// caracterizan las formas canónicas, no las variantes en vivo.
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
    unsafe { raydrone_core::rng01(&mut RNG) }
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
        // DC blocker (~10 Hz) y longitudes de reverb escaladas al SR real.
        DC.set_sample_rate(SR);
        REVERB.set_sample_rate(SR);
        FILTER.set_sample_rate(SR);
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
        REVERB.set_wet(wet);
    }
}

// Filtro resonante: cutoff en Hz (>= ~Nyquist = abierto/bypass) y resonancia 0..1.
#[no_mangle]
pub extern "C" fn set_filter(cutoff_hz: f32, res: f32) {
    unsafe {
        FILTER.set(cutoff_hz, res);
    }
}

// LFO de cutoff: rate en Hz (la UI lo deriva de BPM + división de nota) y
// profundidad en octavas (0 = LFO apagado). Sincroniza el barrido al tempo.
#[no_mangle]
pub extern "C" fn set_filter_lfo(rate_hz: f32, depth_oct: f32) {
    unsafe {
        FILTER.set_lfo(rate_hz, depth_oct);
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
        BOUNCES = bounces.min(6); // same cap as the VST engine's set_bounce
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

// Cargar una de las voces armónicas curadas de `core::music` (acordes/escalas en
// entonación justa) en la tabla de escala — cada grano coge un grado, así la nube
// se apila en un pad afinado en vez de un lavado a una sola altura. preset 0 =
// unísono (continuo de siempre).
#[no_mangle]
pub extern "C" fn set_chord(preset: u32) {
    unsafe {
        if preset == raydrone_core::music::CHORD_UNISON {
            SCALE_LEN = 0;
            SCALE_I = 0;
            return;
        }
        let ratios = raydrone_core::music::chord_ratios(preset);
        let n = if ratios.len() > SCALE_CAP { SCALE_CAP } else { ratios.len() };
        for i in 0..n {
            SCALE[i] = ratios[i];
        }
        SCALE_LEN = n;
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

// ── Teclas pulsadas (piano) ──
#[no_mangle]
pub extern "C" fn keys_ptr() -> *mut f32 {
    unsafe { KEYS.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn keys_capacity() -> usize {
    KEYS_CAP
}
// La UI llama a esto cada vez que cambia el conjunto de teclas sostenidas
// (teclado, ratón o táctil), con los ratios ya escritos en `keys_ptr()`.
// len=0 = ninguna tecla pulsada → se cae a Microtonal/Voicing o pitch continuo.
#[no_mangle]
pub extern "C" fn set_keys(len: usize) {
    unsafe {
        KEYS_LEN = if len > KEYS_CAP { KEYS_CAP } else { len };
        KEYS_I = 0;
    }
}

// Nota para el grano nuevo, elegida entre las teclas sostenidas — mismo
// mecanismo de muestreo por grado que `scale_ratio`, con su propio
// acumulador QMC para no correlacionar los dos ejes de "grado".
#[inline]
fn key_ratio() -> f32 {
    unsafe {
        if KEYS_LEN == 0 {
            return 1.0;
        }
        let n = KEYS_LEN;
        let idx = match MODE {
            1 => {
                QMC_KEY += R2_ALPHA;
                if QMC_KEY >= 1.0 {
                    QMC_KEY -= 1.0;
                }
                (QMC_KEY * (n as f32)) as usize
            }
            2 => {
                let i = (KEYS_I as usize) % n;
                KEYS_I = KEYS_I.wrapping_add(1);
                i
            }
            _ => (rng01() * (n as f32)) as usize,
        };
        KEYS[if idx >= n { n - 1 } else { idx }]
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
                // Borde f32: con N grande, u puede redondear a 1.0 (o superar el
                // máximo de la CDF, que es una suma acumulada en f32) y la
                // búsqueda devuelve el bin extremo de la apertura, donde el
                // kernel triangular vale exactamente 0 → p = q = 0 y 0/0 = NaN
                // envenenaría la estimación entera. Un bin de masa nula no
                // debería poder salir elegido: su peso correcto es 0, así que
                // se descarta la muestra (equivale a wi = 0, sin sesgo).
                let qm = LAB_QM[idx];
                if qm <= 0.0 {
                    continue;
                }
                let k = idx as i32 - a;
                let wi = (LAB_PM[idx] / qm) as f64;
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
        // octava × transposición × grado microtonal/tecla × detune: la retícula
        // exacta + el detune ⇒ enjambre alrededor de cada grado, no comb estático.
        // Las teclas sostenidas (piano) tienen prioridad: mientras haya alguna
        // pulsada, cada grano toca una de esas notas en vez del grado de
        // Microtonal/Voicing — igual que en el VST, tocar anula la textura fija.
        let ratio = if KEYS_LEN > 0 { key_ratio() } else { scale_ratio() };
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
                let raw = core_sample_at(&SAMPLE[..SAMPLE_LEN], VOICES[i].pos);
                let s = band_filter(i, raw) * core_win_at(&WINDOW, ph) * VOICES[i].gain;
                accl += s * VOICES[i].panl;
                accr += s * VOICES[i].panr;
                VOICES[i].pos += VOICES[i].step;
                VOICES[i].age += 1.0;
                k += 1;
            }
            let (fl, fr) = FILTER.process(accl * MASTER, accr * MASTER);
            let (rl, rr) = REVERB.process(fl, fr);
            let (yl, yr) = DC.process(rl, rr);
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

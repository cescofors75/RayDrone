// RayDrone — motor granular continuo en Rust (no_std, sin dependencias).
//
// Filosofía: en vez de crear nodos de Web Audio por grano (con su tope de voces,
// jitter de setTimeout y pulsos por inanición), aquí mezclamos CADA MUESTRA en un
// bucle. Los granos nacen como un flujo continuo (Poisson aproximado), se leen del
// sample con interpolación lineal, se enventanan (Hann) y se acumulan. Un soft-clip
// final evita la saturación. Cero `core`/`std` de transcendentales: la distribución
// triangular sale de la suma de dos uniformes y la ventana viene de una LUT que
// rellena JS (que sí tiene Math.cos).
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
const WIN: usize = 2048; // tamaño de la LUT de ventana
const MAX_VOICES: usize = 512; // granos simultáneos máximos
const BLOCK: usize = 256; // capacidad del buffer de salida (quantum = 128)

static mut SAMPLE: [f32; SAMPLE_CAP] = [0.0; SAMPLE_CAP];
static mut WINDOW: [f32; WIN] = [0.0; WIN];
static mut OUT: [f32; BLOCK] = [0.0; BLOCK];

static mut SAMPLE_LEN: usize = 0;
static mut SR: f32 = 44100.0;

// Parámetros del motor (los fija JS con set_params)
static mut FOCUS: f32 = 0.3; // s — centro de la nube en el sample
static mut APERTURE: f32 = 0.1; // s — ancho de dispersión (±)
static mut GRAIN_DUR: f32 = 0.15; // s — duración de cada grano
static mut GRAIN_RATE: f32 = 200.0; // granos por segundo
static mut GAIN: f32 = 0.3; // ganancia por grano
static mut MASTER: f32 = 1.0; // ganancia global (pre soft-clip)

static mut SPAWN_ACC: f32 = 0.0; // acumulador para repartir nacimientos
static mut RNG: u32 = 0x1234_5678;

// Estrategia de muestreo del offset: 0 = random, 1 = quasi-MC (golden ratio),
// 2 = stratified. QMC/stratified reparten los rayos uniformemente por la apertura
// (baja discrepancia) → menos "grumos" aleatorios → drone más liso con menos granos.
static mut MODE: u32 = 1;
static mut QMC: f32 = 0.5; // estado de la secuencia aditiva golden-ratio
static mut STRAT_I: u32 = 0; // índice de estrato (modo stratified)
const GOLDEN: f32 = 0.618_034; // 1/φ — base de la secuencia de baja discrepancia
const STRATA: u32 = 17; // nº de estratos (coprimo para buen barrido)

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    pos: f32,     // índice de lectura en el sample
    age: f32,     // muestras transcurridas
    inv_dur: f32, // 1 / duración_en_muestras
    gain: f32,
}

static mut VOICES: [Voice; MAX_VOICES] = [Voice {
    active: false,
    pos: 0.0,
    age: 0.0,
    inv_dur: 0.0,
    gain: 0.0,
}; MAX_VOICES];

#[inline]
fn rng01() -> f32 {
    unsafe {
        // xorshift32 → [0,1)
        let mut x = RNG;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        RNG = x;
        (x as f32) * (1.0 / 4_294_967_296.0)
    }
}

// ── Punteros / capacidad para que JS escriba en la memoria lineal ──────────
#[no_mangle]
pub extern "C" fn sample_ptr() -> *mut f32 {
    unsafe { SAMPLE.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn window_ptr() -> *mut f32 {
    unsafe { WINDOW.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn out_ptr() -> *mut f32 {
    unsafe { OUT.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn sample_capacity() -> usize {
    SAMPLE_CAP
}

#[no_mangle]
pub extern "C" fn set_sample(len: usize, sr: f32) {
    unsafe {
        SAMPLE_LEN = if len > SAMPLE_CAP { SAMPLE_CAP } else { len };
        SR = if sr > 1.0 { sr } else { 44100.0 };
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
pub extern "C" fn seed(s: u32) {
    unsafe {
        RNG = if s == 0 { 1 } else { s };
    }
}

#[no_mangle]
pub extern "C" fn set_mode(m: u32) {
    unsafe {
        MODE = m;
    }
}

// Raíz cuadrada sin std: estimación por bit-hack + 3 iteraciones de Newton.
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

// Inversa de la CDF triangular en [-1,1] (pico en 0): mapea u∈[0,1) → offset.
#[inline]
fn tri_inv(u: f32) -> f32 {
    if u < 0.5 {
        -1.0 + sqrtf(2.0 * u)
    } else {
        1.0 - sqrtf(2.0 * (1.0 - u))
    }
}

// Siguiente muestra u∈[0,1) según la estrategia activa.
#[inline]
fn next_u() -> f32 {
    unsafe {
        match MODE {
            1 => {
                // Quasi-Monte Carlo: recurrencia aditiva golden-ratio (baja discrepancia).
                QMC += GOLDEN;
                if QMC >= 1.0 {
                    QMC -= 1.0;
                }
                QMC
            }
            2 => {
                // Stratified: un rayo por estrato, ciclando, con jitter dentro del estrato.
                let u = (STRAT_I as f32 + rng01()) / (STRATA as f32);
                STRAT_I = (STRAT_I + 1) % STRATA;
                u
            }
            _ => rng01(), // Random (Monte Carlo puro)
        }
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
fn soft(x: f32) -> f32 {
    // Soft-clip cúbico (≈ tanh para |x|<1, plano más allá): evita la saturación dura.
    if x > 1.0 {
        0.666_666_7
    } else if x < -1.0 {
        -0.666_666_7
    } else {
        x - x * x * x * (1.0 / 3.0)
    }
}

fn spawn() {
    unsafe {
        if SAMPLE_LEN < 2 {
            return;
        }
        // Offset dentro del cono de dispersión, según la estrategia de muestreo.
        let off_sec = FOCUS + tri_inv(next_u()) * APERTURE;
        let mut pos = off_sec * SR;
        let maxp = (SAMPLE_LEN - 2) as f32;
        if pos < 0.0 {
            pos = 0.0;
        }
        if pos > maxp {
            pos = maxp;
        }
        let dur_samp = GRAIN_DUR * SR;
        if dur_samp < 1.0 {
            return;
        }
        // Buscar una voz libre; si no hay, robar la más vieja.
        let mut slot = usize::MAX;
        let mut oldest = -1.0f32;
        let mut oldest_i = 0usize;
        for i in 0..MAX_VOICES {
            if !VOICES[i].active {
                slot = i;
                break;
            }
            if VOICES[i].age > oldest {
                oldest = VOICES[i].age;
                oldest_i = i;
            }
        }
        if slot == usize::MAX {
            slot = oldest_i;
        }
        VOICES[slot] = Voice {
            active: true,
            pos,
            age: 0.0,
            inv_dur: 1.0 / dur_samp,
            gain: GAIN,
        };
    }
}

#[no_mangle]
pub extern "C" fn process(frames: usize) {
    unsafe {
        let n = if frames > BLOCK { BLOCK } else { frames };
        let rate_per_sample = GRAIN_RATE / SR;
        for f in 0..n {
            // Nacimiento continuo de granos (reparto fino, sin bursts → sin pulso).
            SPAWN_ACC += rate_per_sample;
            while SPAWN_ACC >= 1.0 {
                spawn();
                SPAWN_ACC -= 1.0;
            }
            // Mezcla de todas las voces activas.
            let mut acc = 0.0f32;
            for i in 0..MAX_VOICES {
                let v = &mut VOICES[i];
                if !v.active {
                    continue;
                }
                let ph = v.age * v.inv_dur;
                if ph >= 1.0 {
                    v.active = false;
                    continue;
                }
                acc += sample_at(v.pos) * win_at(ph) * v.gain;
                v.pos += 1.0;
                v.age += 1.0;
            }
            OUT[f] = soft(acc * MASTER);
        }
    }
}

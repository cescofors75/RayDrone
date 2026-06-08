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

// FX: aberración cromática, rebotes (Russian roulette) y autoevolución recursiva.
static mut ABER: f32 = 0.0; // 0..1 — graves abren, agudos enfocan
static mut A_LOW: f32 = 0.06; // coef. one-pole del paso-bajo (banda grave)
static mut A_HIGH: f32 = 0.35; // coef. one-pole para el paso-alto (banda aguda)
static mut BOUNCES: u32 = 0; // profundidad máxima de rebote
static mut REFL: f32 = 0.5; // probabilidad de supervivencia del rebote (Russian roulette)
static mut FEEDBACK: f32 = 0.0; // 0..1 — cantidad de autoevolución
static mut ENV: f32 = 0.0; // envolvente de la salida (la recursión)
static mut EVO: f32 = 0.5; // fase de evolución (ping-pong del foco)
static mut EVO_DIR: f32 = 1.0;

// Registro circular de posiciones (segundos) de los granos disparados, para que el
// worklet lo lea y la página dibuje los rayos. Es solo visualización.
const SLOG_CAP: usize = 512;
static mut SLOG: [f32; SLOG_CAP] = [0.0; SLOG_CAP];
static mut SLOG_W: u32 = 0;

#[inline]
fn log_push(off_sec: f32) {
    unsafe {
        SLOG[(SLOG_W as usize) % SLOG_CAP] = off_sec;
        SLOG_W = SLOG_W.wrapping_add(1);
    }
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    pos: f32,     // índice de lectura en el sample
    age: f32,     // muestras transcurridas
    inv_dur: f32, // 1 / duración_en_muestras
    gain: f32,
    band: u8,     // 0 grave, 1 medio, 2 agudo (aberración)
    lp: f32,      // estado del filtro one-pole por voz
    depth: u32,   // rebotes restantes
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
pub extern "C" fn slog_ptr() -> *mut f32 {
    unsafe { SLOG.as_mut_ptr() }
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

// Coeficientes one-pole de las bandas (a ≈ 2π·fc/sr, válido para fc << sr).
fn update_coeffs() {
    unsafe {
        let tp = 6.283_185_5f32;
        A_LOW = clampf(tp * 500.0 / SR, 0.0, 0.99);
        A_HIGH = clampf(tp * 2500.0 / SR, 0.0, 0.99);
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

// Reserva una voz libre; si no hay, roba la más vieja.
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

// Coloca un grano en una posición dada del sample (usado por granos y rebotes).
fn place(pos: f32, band: u8, depth: u32) {
    unsafe {
        let dur_samp = GRAIN_DUR * SR;
        if dur_samp < 1.0 {
            return;
        }
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
        };
        log_push(pos / SR); // registrar el rayo para la visualización
    }
}

// Filtro de banda por voz (aberración): grave → paso-bajo, agudo → paso-alto.
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

fn spawn() {
    unsafe {
        if SAMPLE_LEN < 2 {
            return;
        }
        // Banda del rayo (40% grave / 40% medio / 20% agudo).
        let r = rng01();
        let band: u8 = if r < 0.4 {
            0
        } else if r < 0.8 {
            1
        } else {
            2
        };
        // Aberración cromática: los graves abren la apertura, los agudos la cierran.
        let scale = if ABER <= 0.0 {
            1.0
        } else {
            match band {
                0 => 1.0 + ABER * 2.2,
                2 => clampf(1.0 - ABER * 0.72, 0.08, 1.0),
                _ => 1.0,
            }
        };
        // Autoevolución (recursión): la envolvente de la salida modula foco y apertura.
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
            // Nacimiento continuo de granos (reparto fino, sin bursts → sin pulso).
            SPAWN_ACC += rate_per_sample;
            while SPAWN_ACC >= 1.0 {
                spawn();
                SPAWN_ACC -= 1.0;
            }
            // Mezcla de todas las voces activas (acceso por índice, sin refs colgando).
            let mut acc = 0.0f32;
            for i in 0..MAX_VOICES {
                if !VOICES[i].active {
                    continue;
                }
                let ph = VOICES[i].age * VOICES[i].inv_dur;
                if ph >= 1.0 {
                    // Fin del grano: Russian roulette → rebote con prob. REFL.
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
                let s = band_filter(i, raw);
                acc += s * win_at(ph) * VOICES[i].gain;
                VOICES[i].pos += 1.0;
                VOICES[i].age += 1.0;
            }
            OUT[f] = soft(acc * MASTER);
        }

        // Recursión / autoevolución: la envolvente de la salida realimenta los
        // parámetros (foco y apertura via EVO/ENV), así el drone se modula solo.
        let mut s = 0.0f32;
        for f in 0..n {
            let a = OUT[f];
            s += if a < 0.0 { -a } else { a };
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

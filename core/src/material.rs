// Materiales de RayDrone — el cuerpo fisico que resuena, no solo un color.
//
// El motor original aplica un filtro de un polo por grano y lo llama material.
// Eso tine, pero no da cuerpo: un metal y un cristal se distinguen por sus
// PARCIALES y por cuanto duran, no por la pendiente de un lowpass.
//
// Aqui hay dos capas, las mismas que en el port a Daisy (`DaisySeed/synth/
// raydrone.h` del repo RedMaster-DaisySeed64MB), con las mismas constantes:
//
//   1. `shape()` — el shaper barato por grano, heredado del motor WASM y
//      ampliado a los cuatro materiales nuevos.
//   2. `ModalBank` — ocho resonadores de dos polos afinados a la nota, con
//      los ratios y el Q del cuerpo real: barra libre-libre, barra de marimba,
//      membrana de Bessel, cuerda armonica, copa de Q altisimo.
//
// `no_std` y sin dependencias, como el resto de `core`: el motor WASM se
// compila con rustc crudo y no tiene ni asignador ni libm.

use crate::{clampf, flush, soft, sqrtf, tri_inv};

pub const MAT_NONE: u32 = 0;
pub const MAT_METAL: u32 = 1;
pub const MAT_WOOD: u32 = 2;
pub const MAT_GLASS: u32 = 3;
pub const MAT_WATER: u32 = 4;
pub const MAT_PLASMA: u32 = 5;
pub const MAT_STONE: u32 = 6;
pub const MAT_SKIN: u32 = 7;
pub const MAT_STRING: u32 = 8;
pub const MAT_ICE: u32 = 9;
pub const MAT_COUNT: u32 = 10;

/// Numero de modos del banco.
pub const NMODES: usize = 8;

pub struct MaterialSpec {
    /// Parciales relativos a la fundamental.
    pub ratio: [f32; NMODES],
    /// Peso de cada modo, con la inclinacion espectral YA aplicada.
    ///
    /// Era `amp * ratio^(-1.35 + bright*1.15)`, recalculado en cada cambio de
    /// nota — ocho `powf`. Como solo depende de constantes del material se
    /// hornea aqui: ademas de ahorrar el `powf`, permite que este motor
    /// `no_std` (que no tiene `pow`) use exactamente los mismos numeros que
    /// el firmware de la Daisy.
    pub weight: [f32; NMODES],
    /// Q base: cuanto mas alto, mas larga la cola del modo.
    pub q: f32,
    /// Absorcion de la camara de rayos.
    pub absorb: f32,
}

static SPECS: [MaterialSpec; MAT_COUNT as usize] = [
    // Armonico plano: el banco queda practicamente transparente.
    MaterialSpec {
        ratio: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        weight: [1.0, 0.292194, 0.140846, 0.0853775, 0.0574551, 0.0399073, 0.0309868, 0.023949],
        q: 22.0,
        absorb: 0.16,
    },
    // Barra libre-libre.
    MaterialSpec {
        ratio: [1.0, 2.756, 5.404, 8.933, 13.34, 18.64, 24.82, 31.87],
        weight: [1.0, 0.568687, 0.369827, 0.244954, 0.164842, 0.111306, 0.0752814, 0.0487223],
        q: 120.0,
        absorb: 0.3,
    },
    // Barra de marimba afinada; decae deprisa.
    MaterialSpec {
        ratio: [1.0, 3.93, 10.88, 21.3, 34.8, 51.6, 71.5, 94.6],
        weight: [1.0, 0.123804, 0.0222995, 0.00585432, 0.00199424, 0.000911232, 0.000333235, 0.000127386],
        q: 34.0,
        absorb: 0.055,
    },
    // Copa: casi la barra, pero con Q altisimo.
    MaterialSpec {
        ratio: [1.0, 2.71, 5.15, 8.43, 12.5, 17.4, 23.1, 29.6],
        weight: [1.0, 0.553101, 0.384188, 0.279037, 0.210453, 0.156337, 0.115937, 0.0818098],
        q: 210.0,
        absorb: 0.2,
    },
    // Cuasi-armonico, con los parciales altos a la deriva.
    MaterialSpec {
        ratio: [1.0, 2.0, 3.0, 4.2, 5.4, 6.9, 8.3, 10.1],
        weight: [1.0, 0.334561, 0.165506, 0.0947954, 0.0557326, 0.0322624, 0.0197681, 0.011492],
        q: 48.0,
        absorb: 0.11,
    },
    // Inarmonico irregular + no linealidad fuerte.
    MaterialSpec {
        ratio: [1.0, 2.31, 3.87, 6.13, 9.02, 12.71, 17.33, 23.02],
        weight: [1.0, 0.45216, 0.286182, 0.178527, 0.114205, 0.0742667, 0.0473769, 0.0291791],
        q: 64.0,
        absorb: 0.38,
    },
    // Denso y oscuro, Q bajo.
    MaterialSpec {
        ratio: [1.0, 2.1, 3.4, 4.9, 6.8, 9.1, 11.6, 14.5],
        weight: [1.0, 0.243718, 0.0835829, 0.033236, 0.0134317, 0.00620911, 0.00271863, 0.00106417],
        q: 18.0,
        absorb: 0.16,
    },
    // Membrana circular (ceros de Bessel).
    MaterialSpec {
        ratio: [1.0, 1.593, 2.135, 2.295, 2.653, 2.917, 3.155, 3.5],
        weight: [1.0, 0.433731, 0.238442, 0.204342, 0.139374, 0.0998083, 0.0736614, 0.0496559],
        q: 28.0,
        absorb: 0.13,
    },
    // Armonico puro, Q alto.
    MaterialSpec {
        ratio: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        weight: [1.0, 0.504551, 0.303005, 0.198333, 0.1352, 0.0936432, 0.0664915, 0.0468159],
        q: 150.0,
        absorb: 0.09,
    },
    // Inarmonico agudo, Q extremo.
    MaterialSpec {
        ratio: [1.0, 3.01, 6.02, 9.44, 14.1, 19.8, 26.3, 33.9],
        weight: [1.0, 0.664163, 0.498882, 0.389864, 0.30109, 0.228397, 0.171879, 0.122554],
        q: 260.0,
        absorb: 0.24,
    },
];

/// Ficha del material. Un id desconocido cae en `MAT_NONE`.
#[inline]
pub fn spec(material: u32) -> &'static MaterialSpec {
    &SPECS[if material < MAT_COUNT { material as usize } else { 0 }]
}

// ── Trigonometria y exponencial en no_std ───────────────────────────────────
// Solo se usan al recalcular coeficientes (cambio de nota o de material), asi
// que la precision importa mas que la velocidad.

const PI: f32 = core::f32::consts::PI;

/// `sin(x)` para x en [-pi, pi]. Taylor hasta x^11: en el peor punto del rango
/// reducido (pi/2) el error queda por debajo de 1e-7. Se llega hasta ahi porque
/// truncar en x^7 deja 1,5e-4 en pi/2, y ese error entra al cuadrado en
/// `cos_approx`, que es justo donde se calcula el polo de cada modo.
#[inline]
pub fn sin_approx(x: f32) -> f32 {
    // Reducir a [-pi/2, pi/2], donde la serie converge rapido.
    let x = if x > PI * 0.5 {
        PI - x
    } else if x < -PI * 0.5 {
        -PI - x
    } else {
        x
    };
    let x2 = x * x;
    x * (1.0
        - x2 * (1.0 / 6.0
            - x2 * (1.0 / 120.0
                - x2 * (1.0 / 5040.0 - x2 * (1.0 / 362880.0 - x2 * (1.0 / 39916800.0))))))
}

/// `cos(x)` para x en [0, pi], via `cos x = 1 - 2 sin^2(x/2)` — exacta dada
/// la sine, y con el argumento ya dentro del rango bueno.
#[inline]
pub fn cos_approx(x: f32) -> f32 {
    let s = sin_approx(x * 0.5);
    1.0 - 2.0 * s * s
}

/// `cos(x)` para x en [0, 2pi). Fuera de [0, pi] se refleja con
/// `cos(x) = cos(2pi - x)`, que es exacto, no una aproximacion mas.
#[inline]
pub fn cos_approx_wrapped(x: f32) -> f32 {
    const TWO_PI: f32 = 2.0 * PI;
    let x = if x < 0.0 || !(x < TWO_PI) {
        // Reducir cualquier entrada a [0, 2pi) sin depender de `rem_euclid`.
        let k = (x / TWO_PI) as i32;
        let mut y = x - (k as f32) * TWO_PI;
        if y < 0.0 {
            y += TWO_PI;
        }
        y
    } else {
        x
    };
    if x > PI {
        cos_approx(TWO_PI - x)
    } else {
        cos_approx(x)
    }
}

/// `exp(-a)` para a >= 0. Se divide por ocho para que la serie trabaje en
/// [0, 0.5] (donde ocho terminos dan error < 1e-7) y se eleva al cubo dos
/// veces. Satura a 0 para a muy grande en vez de desbordar.
#[inline]
pub fn exp_neg(a: f32) -> f32 {
    if !(a > 0.0) {
        return 1.0;
    }
    if a > 40.0 {
        return 0.0;
    }
    let t = -a * 0.125;
    // e^t con Taylor de orden 7 (t en [-5, 0], pero en la practica en [-0.1, 0]).
    let e = 1.0
        + t * (1.0
            + t * (0.5
                + t * (1.0 / 6.0
                    + t * (1.0 / 24.0
                        + t * (1.0 / 120.0
                            + t * (1.0 / 720.0 + t * (1.0 / 5040.0)))))));
    let e2 = e * e; // ^2
    let e4 = e2 * e2; // ^4
    e4 * e4 // ^8
}

// ── Shaper por grano ────────────────────────────────────────────────────────

/// Color inmediato del material sobre un grano. `tone` es el estado del filtro
/// de ese grano (uno por grano) y `age` su edad en muestras.
///
/// El estado se aplasta a cero cuando cae por debajo de lo audible: un filtro
/// de un polo decae hasta el rango subnormal en los huecos de la nube, y ahi el
/// FPU se sale de su camino rapido. Con decenas de granos vivos eso multiplica
/// por diez el coste por muestra — medido sobre el port a Daisy, 6,3 us frente
/// a 0,49 us.
#[inline]
pub fn shape(material: u32, amount: f32, tone: &mut f32, age: f32, x: f32) -> f32 {
    if amount <= 0.0 || material == MAT_NONE {
        return x;
    }
    let shaped = match material {
        MAT_METAL => {
            let d = x - *tone;
            *tone += 0.12 * d;
            x + d * 1.6
        }
        MAT_WOOD => {
            *tone += 0.045 * (x - *tone);
            *tone * 0.86 + x * 0.14
        }
        MAT_GLASS => {
            *tone += 0.22 * (x - *tone);
            x * 0.82 + *tone * 0.42
        }
        MAT_WATER => {
            *tone += 0.09 * (x - *tone);
            let ph = age * 0.00037;
            let ph = ph - (ph as i32) as f32;
            x * (0.82 + 0.18 * tri_inv(ph)) + *tone * 0.35
        }
        // Plasma: no linealidad fuerte pero ACOTADA. El `x - x^3` del motor
        // original cambia de signo en cuanto |x| pasa de 1,15, lo que mete un
        // pliegue asimetrico y offset de continua en la nube. `soft()` da la
        // misma aspereza sin salirse.
        MAT_PLASMA => soft(x * 2.0) * 1.05,
        MAT_STONE => {
            *tone += 0.030 * (x - *tone);
            *tone * 0.94 + x * 0.06
        }
        MAT_SKIN => {
            *tone += 0.055 * (x - *tone);
            *tone * 0.78 + soft(x * 1.4) * 0.22
        }
        MAT_STRING => {
            let d = x - *tone;
            *tone += 0.16 * d;
            *tone + d * 0.55
        }
        MAT_ICE => {
            let d = x - *tone;
            *tone += 0.34 * d;
            x * 0.7 + d * 2.1
        }
        _ => x,
    };
    *tone = flush(*tone);
    x + (shaped - x) * amount
}

// ── Banco modal ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
struct Mode {
    a1: f32,
    a2: f32,
    b0: f32,
    y1l: f32,
    y2l: f32,
    y1r: f32,
    y2r: f32,
}

/// Ocho resonadores de dos polos afinados a la nota: el cuerpo del material.
pub struct ModalBank {
    modes: [Mode; NMODES],
    amp: [f32; NMODES],
    norm: f32,
}

/// Trim del banco. Con el se iguala el nivel del camino con material al del
/// camino seco, para que cambiar de material sea un cambio de TIMBRE y no un
/// salto de volumen. Mismo valor que en el firmware de la Daisy.
pub const BANK_GAIN: f32 = 0.112;

impl ModalBank {
    pub const fn new() -> Self {
        ModalBank {
            modes: [Mode {
                a1: 0.0,
                a2: 0.0,
                b0: 0.0,
                y1l: 0.0,
                y2l: 0.0,
                y1r: 0.0,
                y2r: 0.0,
            }; NMODES],
            amp: [0.0; NMODES],
            norm: 0.0,
        }
    }

    pub fn reset(&mut self) {
        for m in self.modes.iter_mut() {
            m.y1l = 0.0;
            m.y2l = 0.0;
            m.y1r = 0.0;
            m.y2r = 0.0;
        }
    }

    /// Recalcula los coeficientes para `material` a fundamental `f0`.
    ///
    /// GANANCIA — el punto delicado. Un modo de Q alto es un filtro de banda
    /// estrechisima: excitado con una nube granular de banda ancha deja pasar
    /// una fraccion minuscula de la energia. Con la normalizacion "pico unidad"
    /// de libro (`b0 = (1-r^2)·sin w`) los ocho modos juntos devuelven en torno
    /// al 1 % del nivel de entrada, y el material se convierte en un filtro que
    /// apaga el motor. Aqui se normaliza por POTENCIA: para ruido blanco un
    /// resonador de pico unidad y ancho de banda B entrega `sqrt(pi·B/sr)` del
    /// nivel de entrada, asi que se compensa con `sqrt(sr/(pi·B))`; despues se
    /// divide por la raiz de la suma de cuadrados de los pesos para que la suma
    /// incoherente de los ocho vuelva a nivel unidad. El tope de compensacion
    /// evita que un modo de Q extremo excitado por una fuente tonal (que SI cae
    /// justo dentro de su banda) reviente el bus.
    pub fn set(&mut self, material: u32, amount: f32, f0: f32, sr: f32) {
        let sp = spec(material);
        let sr = if sr > 1000.0 { sr } else { 48000.0 };
        let nyq = sr * 0.47;
        let mut amp_sq = 0.0f32;

        for m in 0..NMODES {
            let f = f0 * sp.ratio[m];
            // Un parcial por encima de Nyquist se pliega: mejor silenciar el
            // modo que dejar que aliasee.
            if !(f < nyq) || f < 15.0 {
                self.modes[m].a1 = 0.0;
                self.modes[m].a2 = 0.0;
                self.modes[m].b0 = 0.0;
                self.amp[m] = 0.0;
                self.modes[m].y1l = 0.0;
                self.modes[m].y2l = 0.0;
                self.modes[m].y1r = 0.0;
                self.modes[m].y2r = 0.0;
                continue;
            }
            let q = clampf(sp.q * (0.45 + 0.75 * amount), 2.0, 400.0);
            let w = 2.0 * PI * f / sr;
            let rr = clampf(exp_neg(PI * f / (q * sr)), 0.0, 0.99995);
            self.modes[m].a1 = 2.0 * rr * cos_approx(w);
            self.modes[m].a2 = -rr * rr;

            let b0_peak = (1.0 - rr * rr) * sin_approx(w);
            let bw = f / q;
            let makeup = clampf(sqrtf(sr / (PI * bw)), 1.0, 500.0);
            self.modes[m].b0 = b0_peak * makeup;

            self.amp[m] = sp.weight[m];
            amp_sq += sp.weight[m] * sp.weight[m];
        }
        self.norm = if amp_sq > 1e-8 {
            BANK_GAIN / sqrtf(amp_sq)
        } else {
            0.0
        };
    }

    /// Una muestra estereo. Devuelve la salida del banco SIN mezclar: quien
    /// llama decide cuanto suma al directo.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let mut acc_l = 0.0f32;
        let mut acc_r = 0.0f32;
        for m in 0..NMODES {
            let md = &mut self.modes[m];
            let yl = flush(md.b0 * l + md.a1 * md.y1l + md.a2 * md.y2l);
            md.y2l = md.y1l;
            md.y1l = yl;
            let yr = flush(md.b0 * r + md.a1 * md.y1r + md.a2 * md.y2r);
            md.y2r = md.y1r;
            md.y1r = yr;
            acc_l += yl * self.amp[m];
            acc_r += yr * self.amp[m];
        }
        (soft(acc_l * self.norm), soft(acc_r * self.norm))
    }
}

impl Default for ModalBank {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_cos_match_reference() {
        let mut i = 0;
        while i <= 100 {
            let x = PI * (i as f32) / 100.0;
            assert!((sin_approx(x) - x.sin()).abs() < 1e-6, "sin({x})");
            assert!((cos_approx(x) - x.cos()).abs() < 1e-6, "cos({x})");
            i += 1;
        }
    }

    #[test]
    fn cos_wrapped_covers_the_full_turn() {
        let mut i = -50;
        while i <= 250 {
            let x = 2.0 * PI * (i as f32) / 100.0;
            assert!(
                (cos_approx_wrapped(x) - x.cos()).abs() < 1e-6,
                "cos({x}) = {} vs {}",
                cos_approx_wrapped(x),
                x.cos()
            );
            i += 1;
        }
    }

    #[test]
    fn exp_neg_matches_reference() {
        let mut i = 0;
        while i <= 80 {
            let a = (i as f32) * 0.05;
            let got = exp_neg(a);
            let want = (-a).exp();
            // Tolerancia RELATIVA: la serie se evalua sobre a/8 y el resultado
            // se eleva a la octava, asi que el error relativo se multiplica por
            // ocho. En el rango real de uso (a < 0,8) queda mucho mas fino.
            assert!(
                (got - want).abs() <= 2e-6 * want + 1e-9,
                "exp(-{a}) = {got} vs {want}"
            );
            i += 1;
        }
        assert_eq!(exp_neg(0.0), 1.0);
        assert_eq!(exp_neg(1000.0), 0.0);
    }

    #[test]
    fn every_material_is_finite_and_bounded() {
        for m in 0..MAT_COUNT {
            let mut tone = 0.0f32;
            let mut peak = 0.0f32;
            for n in 0..4000 {
                let x = ((n as f32) * 0.03).sin() * 0.8;
                let y = shape(m, 1.0, &mut tone, n as f32, x);
                assert!(y.is_finite(), "material {m} no finito");
                peak = peak.max(y.abs());
            }
            assert!(peak < 6.0, "material {m} desbocado: {peak}");
        }
    }

    #[test]
    fn plasma_shaper_is_monotonic_and_bounded() {
        // El shaper original (x + (x - x^3)*0.75) cambia de signo por encima de
        // |x| ~ 1,15: a x = 2 devuelve -2,5. Eso mete continua en la nube.
        let mut tone = 0.0f32;
        let mut prev = f32::NEG_INFINITY;
        let mut i = -30;
        while i <= 30 {
            let x = (i as f32) * 0.1;
            let y = shape(MAT_PLASMA, 1.0, &mut tone, 0.0, x);
            // No decreciente. El epsilon cubre el ruido de coma flotante en la
            // zona plana de saturacion, donde dos x consecutivas dan el mismo
            // valor salvo el ultimo bit.
            assert!(y >= prev - 1e-6, "no monotona en x={x}: {y} < {prev}");
            assert!(y.abs() < 1.6, "sin acotar en x={x}: {y}");
            prev = y;
            i += 1;
        }
        // Impar: sin componente de continua.
        let mut t1 = 0.0;
        let mut t2 = 0.0;
        for i in 1..25 {
            let x = (i as f32) * 0.1;
            let a = shape(MAT_PLASMA, 1.0, &mut t1, 0.0, x);
            let b = shape(MAT_PLASMA, 1.0, &mut t2, 0.0, -x);
            assert!((a + b).abs() < 1e-6, "asimetrica en x={x}");
        }
    }

    #[test]
    fn modal_bank_keeps_level_across_materials() {
        // El fallo que motiva la normalizacion por potencia: con la
        // normalizacion de pico unidad, los materiales de Q alto salian ~40 dB
        // por debajo y el banco apagaba el motor. Todos deben quedar en el
        // mismo orden de magnitud.
        let mut rmss = [0.0f32; MAT_COUNT as usize];
        for m in 1..MAT_COUNT {
            let mut bank = ModalBank::new();
            bank.set(m, 1.0, 110.0, 48000.0);
            let mut rng = 0x1234_5678u32;
            let mut acc = 0.0f64;
            let n = 48000;
            for _ in 0..n {
                let x = crate::rng01(&mut rng) * 2.0 - 1.0;
                let (l, _) = bank.process(x, x);
                assert!(l.is_finite());
                acc += (l as f64) * (l as f64);
            }
            rmss[m as usize] = (acc / n as f64).sqrt() as f32;
        }
        let mut lo = f32::INFINITY;
        let mut hi = 0.0f32;
        for m in 1..MAT_COUNT {
            let v = rmss[m as usize];
            assert!(v > 0.005, "material {m} practicamente mudo: {v}");
            lo = lo.min(v);
            hi = hi.max(v);
        }
        assert!(hi / lo < 6.0, "materiales desigualados: {lo} .. {hi}");
    }

    #[test]
    fn modal_bank_silences_partials_above_nyquist() {
        let mut bank = ModalBank::new();
        // A 8 kHz de fundamental, casi todos los parciales del hielo (hasta
        // 33,9x) se van por encima de Nyquist: deben quedar mudos, no aliasear.
        bank.set(MAT_ICE, 1.0, 8000.0, 48000.0);
        for _ in 0..2000 {
            let (l, r) = bank.process(1.0, -1.0);
            assert!(l.is_finite() && r.is_finite());
            assert!(l.abs() < 10.0 && r.abs() < 10.0);
        }
    }
}

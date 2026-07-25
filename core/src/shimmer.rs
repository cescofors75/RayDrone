// Shimmer para RayDrone — la cola que asciende sola.
//
// El motor original solo tiene `OCT`: la probabilidad de que un grano lea a 2x.
// Eso es una capa de octava, no shimmer. El shimmer de verdad vive DENTRO de la
// realimentacion: la cola se transpone hacia arriba y se re-inyecta, asi que
// cada vuelta sube otra octava y el tono de la cola crece muy por encima de una
// sola octava. Medido sobre el port a Daisy con este mismo algoritmo, el tono
// de la cola pasa de 174 Hz a 2114 Hz con +12, y a 4545 Hz con +19.
//
// Dos cabezas de lectura desfasadas media ventana y cruzadas con una
// cosinusoide: el granular clasico. Barato y sin artefactos audibles dentro de
// una cola difusa. `no_std`, buffers fijos (el motor WASM no tiene asignador).

use crate::flush;

/// Longitud de la ventana del desplazador, en muestras. Potencia de dos: el
/// indice se envuelve con una mascara y no con un modulo.
pub const LEN: usize = 4096;
const MASK: usize = LEN - 1;

const TWO_PI: f32 = 2.0 * core::f32::consts::PI;

pub struct Shimmer {
    buf_l: [f32; LEN],
    buf_r: [f32; LEN],
    w: usize,
    ph12: f32,
    ph19: f32,
}

impl Shimmer {
    pub const fn new() -> Self {
        Shimmer {
            buf_l: [0.0; LEN],
            buf_r: [0.0; LEN],
            w: 0,
            ph12: 0.0,
            ph19: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.buf_l = [0.0; LEN];
        self.buf_r = [0.0; LEN];
        self.w = 0;
        self.ph12 = 0.0;
        self.ph19 = 0.0;
    }

    /// Lee la linea a un retardo `phase` con las dos cabezas cruzadas.
    #[inline]
    fn read_at(&self, phase: f32) -> (f32, f32) {
        const FLEN: f32 = LEN as f32;
        const HALF: f32 = FLEN * 0.5;

        let da = phase;
        let mut db = phase + HALF;
        if db >= FLEN {
            db -= FLEN;
        }

        // ga vale 0 justo cuando la cabeza A acaba de dar el salto de wrap, y 1
        // en el centro; gb es su complemento, asi que la cabeza que salta
        // siempre pasa por ganancia cero.
        let ga = 0.5 - 0.5 * crate::material::cos_approx_wrapped(TWO_PI * (da / FLEN));
        let gb = 1.0 - ga;

        let ia = da as usize;
        let fa = da - (ia as f32);
        let ib = db as usize;
        let fb = db - (ib as f32);

        let base = self.w + LEN - 1;
        let a0 = (base - ia) & MASK;
        let a1 = (base - ia - 1) & MASK;
        let b0 = (base - ib) & MASK;
        let b1 = (base - ib - 1) & MASK;

        let al = self.buf_l[a0] + (self.buf_l[a1] - self.buf_l[a0]) * fa;
        let bl = self.buf_l[b0] + (self.buf_l[b1] - self.buf_l[b0]) * fb;
        let ar = self.buf_r[a0] + (self.buf_r[a1] - self.buf_r[a0]) * fa;
        let br = self.buf_r[b0] + (self.buf_r[b1] - self.buf_r[b0]) * fb;

        (al * ga + bl * gb, ar * ga + br * gb)
    }

    /// Salida transpuesta: mezcla de +12 y +19 segun `tilt` (0 = solo +12,
    /// 1 = solo +19). Llamar UNA vez por muestra, antes de `write`.
    #[inline]
    pub fn read(&self, tilt: f32) -> (f32, f32) {
        let (l12, r12) = self.read_at(self.ph12);
        let (l19, r19) = self.read_at(self.ph19);
        let t = crate::clampf(tilt, 0.0, 1.0);
        (l12 + (l19 - l12) * t, r12 + (r19 - r12) * t)
    }

    /// Avanza las fases y escribe la muestra nueva de la cola.
    ///
    /// La posicion leida es escritura menos retardo. La escritura avanza a una
    /// muestra por muestra, luego para que la LECTURA avance a `rate` el
    /// retardo tiene que MENGUAR a (rate - 1) por muestra. Con el signo al
    /// reves y rate = 2 el retardo crece a la misma velocidad que la escritura,
    /// la cabeza se queda clavada en una muestra y en vez de una octava arriba
    /// sale continua.
    #[inline]
    pub fn write(&mut self, l: f32, r: f32) {
        advance(&mut self.ph12, 2.0); // +12
        advance(&mut self.ph19, 3.0); // +19
        self.buf_l[self.w] = flush(l);
        self.buf_r[self.w] = flush(r);
        self.w = (self.w + 1) & MASK;
    }
}

#[inline]
fn advance(phase: &mut f32, rate: f32) {
    *phase -= rate - 1.0;
    if *phase >= LEN as f32 {
        *phase -= LEN as f32;
    }
    if *phase < 0.0 {
        *phase += LEN as f32;
    }
}

impl Default for Shimmer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cuenta cruces por cero: mide el tono sin necesitar una FFT.
    fn zcr(samples: &[f32]) -> f32 {
        let mut z = 0;
        let mut prev = 0.0f32;
        for &s in samples {
            if (s > 0.0) != (prev > 0.0) {
                z += 1;
            }
            prev = s;
        }
        (z as f32) * 48000.0 / (2.0 * samples.len() as f32)
    }

    /// Pasa un seno por el desplazador y comprueba que SUBE de tono.
    ///
    /// Es la prueba que caza el bug de signo: con el retardo avanzando al
    /// reves, la cabeza se congela y la salida es continua, no una octava.
    fn shifted_zcr(f_in: f32, tilt: f32) -> f32 {
        let mut sh = Shimmer::new();
        let mut out = [0.0f32; 24000];
        // Precargar la linea para que las cabezas no lean ceros.
        for n in 0..LEN * 2 {
            let x = (TWO_PI * f_in * (n as f32) / 48000.0).sin();
            let _ = sh.read(tilt);
            sh.write(x, x);
        }
        for (n, o) in out.iter_mut().enumerate() {
            let t = (LEN * 2 + n) as f32;
            let x = (TWO_PI * f_in * t / 48000.0).sin();
            let (l, _) = sh.read(tilt);
            *o = l;
            sh.write(x, x);
        }
        zcr(&out)
    }

    #[test]
    fn plus_twelve_raises_an_octave() {
        let f = 220.0;
        let got = shifted_zcr(f, 0.0);
        // Una octava arriba: 440 Hz. Se admite holgura por el crossfade.
        assert!(
            got > f * 1.7 && got < f * 2.35,
            "esperaba ~{} Hz, salio {got} Hz",
            f * 2.0
        );
    }

    #[test]
    fn plus_nineteen_raises_more_than_plus_twelve() {
        let f = 220.0;
        let a = shifted_zcr(f, 0.0);
        let b = shifted_zcr(f, 1.0);
        assert!(b > a * 1.3, "+19 ({b} Hz) deberia superar a +12 ({a} Hz)");
        assert!(b > f * 2.5 && b < f * 3.5, "esperaba ~{} Hz, salio {b}", f * 3.0);
    }

    #[test]
    fn output_is_finite_and_bounded() {
        let mut sh = Shimmer::new();
        let mut rng = 0x9E37_79B9u32;
        for n in 0..200_000 {
            let (l, r) = sh.read(((n % 100) as f32) / 100.0);
            assert!(l.is_finite() && r.is_finite());
            assert!(l.abs() <= 1.01 && r.abs() <= 1.01, "sin acotar: {l} {r}");
            let x = crate::rng01(&mut rng) * 2.0 - 1.0;
            sh.write(x, -x);
        }
    }

    #[test]
    fn silence_in_silence_out() {
        let mut sh = Shimmer::new();
        for _ in 0..LEN * 3 {
            let (l, r) = sh.read(0.5);
            assert_eq!(l, 0.0);
            assert_eq!(r, 0.0);
            sh.write(0.0, 0.0);
        }
    }
}

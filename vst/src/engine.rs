// RayDrone — simplified granular engine (per-instance, std).
//
// A direct, slimmed-down port of `wasm/raydrone.rs`. The core idea is intact:
// each grain is a "ray" cast at a random offset inside the dispersion cone
// around the focus, and the drone *emerges* from the convergence of N rays.
//
// What the simplified VERSION keeps:
//   - Continuous grain cloud, per-sample mixing (no voice cap clicks, no jitter).
//   - Low-discrepancy sampling (golden-ratio quasi-Monte Carlo) for smooth render.
//   - Triangular dispersion around the focus (the "depth of field").
//   - Catmull-Rom interpolation, micro-detune, equal-power stereo spread.
//   - Freeverb-lite stereo reverb + DC blocker + soft clip.
//
// What it drops vs. the WASM engine (to stay "simple"): chromatic aberration,
// Russian-roulette bounces, recursive autoevolution, ambient foci, microtonal
// scales, inverse tracing and the Convergence Lab.

use std::f32::consts::PI;

const WIN: usize = 2048;
const MAX_VOICES: usize = 512;

// Reverb (Freeverb-lite): 4 combs + 2 allpass per channel, lengths @44.1k.
const NC: usize = 4;
const NA: usize = 2;
const CLEN_L: [usize; NC] = [1557, 1617, 1491, 1422];
const CLEN_R: [usize; NC] = [1580, 1640, 1514, 1445]; // +stereo spread
const ALEN: [usize; NA] = [556, 441];
const REV_ROOM: f32 = 0.84;
const REV_DAMP: f32 = 0.5;

const GOLDEN: f32 = 0.618_034;
const STRATA: u32 = 17;

// Ring buffer of recent ray landing positions (normalized 0..1) for the visualizer.
pub const VLOG: usize = 128;

// Per-grain micro-detune (±0.25%): beats between grains → lush, non-static drone.
const DETUNE: f32 = 0.005;
// Fixed grain shape / level / spread for the simplified build.
const GRAIN_DUR: f32 = 0.15; // seconds
const GRAIN_GAIN: f32 = 0.3;
const WIDTH: f32 = 0.7;

#[derive(Clone, Copy)]
struct Voice {
    pos: f32,
    age: f32,
    inv_dur: f32,
    gain: f32,
    step: f32,
    panl: f32,
    panr: f32,
    depth: u32, // remaining recursive bounces (Russian roulette)
}

impl Default for Voice {
    fn default() -> Self {
        Voice {
            pos: 0.0,
            age: 0.0,
            inv_dur: 0.0,
            gain: 0.0,
            step: 1.0,
            panl: 0.707,
            panr: 0.707,
            depth: 0,
        }
    }
}

pub struct Engine {
    // Source ("the scene") and its native rate.
    sample: Vec<f32>,
    samp_sr: f32,
    // Host output rate (may differ from samp_sr; we keep pitch correct).
    host_sr: f32,
    window: Vec<f32>,

    // Params (set from the plugin each block).
    focus01: f32,     // 0..1 position in the sample
    aperture_ms: f32, // dispersion width
    grain_rate: f32,  // grains per second (density / "N rays")
    master: f32,      // output gain (linear)
    reverb_wet: f32,
    feedback: f32,    // autoevolution amount (recursive feedback)
    octave: f32,      // probability a grain plays an octave up (shimmer)
    bounces: u32,     // recursive ray bounces (Russian-roulette depth)
    refl: f32,        // reflection coefficient: probability each bounce survives
    mode: u32, // 0 random, 1 QMC (golden), 2 stratified

    // Recursive autoevolution: the output envelope feeds back into focus/aperture,
    // and a slow self-sweep (EVO) makes the render drift on its own.
    env: f32,
    evo: f32,
    evo_dir: f32,

    // Visualization state (read by the GUI thread via the plugin).
    vlog_pos: Vec<f32>,
    vlog_w: usize,
    viz_focus: f32, // effective focus, normalized 0..1
    viz_ap: f32,    // effective aperture as a fraction of the scene span, 0..1

    // Runtime.
    spawn_acc: f32,
    rng: u32,
    qmc: f32,
    strat_i: u32,
    // Live capture: when on, `sample` is a rolling buffer fed by the host input
    // (the plugin works as an ambient effect on a track instead of an instrument).
    live: bool,
    cap_w: usize,
    voices: Vec<Voice>,
    active: Vec<usize>,
    free: Vec<usize>,
    nactive: usize,
    nfree: usize,

    // Reverb state.
    combl: Vec<Vec<f32>>,
    combr: Vec<Vec<f32>>,
    combl_i: [usize; NC],
    combr_i: [usize; NC],
    combl_s: [f32; NC],
    combr_s: [f32; NC],
    apl: Vec<Vec<f32>>,
    apr: Vec<Vec<f32>>,
    apl_i: [usize; NA],
    apr_i: [usize; NA],

    // DC blocker (one-pole highpass ~10 Hz).
    dc_r: f32,
    dc_xl: f32,
    dc_yl: f32,
    dc_xr: f32,
    dc_yr: f32,
}

impl Engine {
    pub fn new(host_sr: f32) -> Self {
        let host_sr = if host_sr > 1.0 { host_sr } else { 44100.0 };
        let window: Vec<f32> = (0..WIN)
            .map(|i| {
                let x = (i as f32) / ((WIN - 1) as f32);
                0.5 - 0.5 * (2.0 * PI * x).cos() // Hann
            })
            .collect();

        let mut e = Engine {
            sample: Vec::new(),
            samp_sr: 44100.0,
            host_sr,
            window,
            focus01: 0.3,
            aperture_ms: 100.0,
            grain_rate: 200.0,
            master: 0.5,
            reverb_wet: 0.2,
            feedback: 0.3,
            octave: 0.0,
            bounces: 0,
            refl: 0.5,
            mode: 1,
            env: 0.0,
            evo: 0.5,
            evo_dir: 1.0,
            vlog_pos: vec![0.5; VLOG],
            vlog_w: 0,
            viz_focus: 0.3,
            viz_ap: 0.1,
            spawn_acc: 0.0,
            rng: 0x1234_5678,
            qmc: 0.5,
            strat_i: 0,
            live: false,
            cap_w: 0,
            voices: vec![Voice::default(); MAX_VOICES],
            active: vec![0usize; MAX_VOICES],
            free: (0..MAX_VOICES).map(|i| MAX_VOICES - 1 - i).collect(),
            nactive: 0,
            nfree: MAX_VOICES,
            combl: Vec::new(),
            combr: Vec::new(),
            combl_i: [0; NC],
            combr_i: [0; NC],
            combl_s: [0.0; NC],
            combr_s: [0.0; NC],
            apl: Vec::new(),
            apr: Vec::new(),
            apl_i: [0; NA],
            apr_i: [0; NA],
            dc_r: 0.998_575,
            dc_xl: 0.0,
            dc_yl: 0.0,
            dc_xr: 0.0,
            dc_yr: 0.0,
        };
        e.update_coeffs();
        e
    }

    pub fn is_empty(&self) -> bool {
        self.sample.len() < 2
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.host_sr = if sr > 1.0 { sr } else { 44100.0 };
        self.update_coeffs();
    }

    pub fn load(&mut self, sample: Vec<f32>, sr: f32) {
        self.live = false;
        self.sample = sample;
        self.samp_sr = if sr > 1.0 { sr } else { 44100.0 };
        self.reset();
    }

    /// Switch to live-input mode: `sample` becomes a rolling buffer of the last
    /// `secs` seconds of host audio, which the rays render into an ambient cloud.
    pub fn begin_live_capture(&mut self, secs: f32) {
        let len = ((secs * self.host_sr) as usize).max(2);
        self.sample = vec![0.0; len];
        self.samp_sr = self.host_sr; // captured at the host's rate
        self.cap_w = 0;
        self.live = true;
        self.reset();
    }

    /// Feed one input sample into the rolling capture buffer (no-op unless live).
    #[inline]
    pub fn push_input(&mut self, x: f32) {
        if self.live && !self.sample.is_empty() {
            self.sample[self.cap_w] = x;
            self.cap_w += 1;
            if self.cap_w >= self.sample.len() {
                self.cap_w = 0;
            }
        }
    }

    pub fn reset(&mut self) {
        self.nactive = 0;
        self.nfree = MAX_VOICES;
        for i in 0..MAX_VOICES {
            self.free[i] = MAX_VOICES - 1 - i;
        }
        self.spawn_acc = 0.0;
        self.env = 0.0;
        self.evo = 0.5;
        self.evo_dir = 1.0;
        for v in self.vlog_pos.iter_mut() {
            *v = 0.5;
        }
        self.vlog_w = 0;
        self.dc_xl = 0.0;
        self.dc_yl = 0.0;
        self.dc_xr = 0.0;
        self.dc_yr = 0.0;
        for c in 0..NC {
            for v in self.combl[c].iter_mut() {
                *v = 0.0;
            }
            for v in self.combr[c].iter_mut() {
                *v = 0.0;
            }
            self.combl_i[c] = 0;
            self.combr_i[c] = 0;
            self.combl_s[c] = 0.0;
            self.combr_s[c] = 0.0;
        }
        for a in 0..NA {
            for v in self.apl[a].iter_mut() {
                *v = 0.0;
            }
            for v in self.apr[a].iter_mut() {
                *v = 0.0;
            }
            self.apl_i[a] = 0;
            self.apr_i[a] = 0;
        }
    }

    // ── Setters from the plugin ─────────────────────────────────────────────
    pub fn set_density(&mut self, rays_per_s: f32) {
        self.grain_rate = rays_per_s.max(0.0);
    }
    pub fn set_aperture_ms(&mut self, ms: f32) {
        self.aperture_ms = ms.max(0.0);
    }
    pub fn set_focus(&mut self, f01: f32) {
        self.focus01 = clampf(f01, 0.0, 1.0);
    }
    pub fn set_reverb(&mut self, wet: f32) {
        self.reverb_wet = clampf(wet, 0.0, 1.0);
    }
    pub fn set_master(&mut self, gain: f32) {
        self.master = gain.max(0.0);
    }
    pub fn set_feedback(&mut self, amount: f32) {
        self.feedback = clampf(amount, 0.0, 1.0);
    }
    pub fn set_octave(&mut self, p: f32) {
        self.octave = clampf(p, 0.0, 1.0);
    }
    pub fn set_bounce(&mut self, n: u32) {
        self.bounces = n.min(6);
    }
    pub fn set_reflect(&mut self, r: f32) {
        self.refl = clampf(r, 0.0, 1.0);
    }

    // ── Visualization getters (read by the GUI thread) ──────────────────────
    pub fn viz_level(&self) -> f32 {
        self.env
    }
    pub fn viz_focus(&self) -> f32 {
        self.viz_focus
    }
    pub fn viz_aperture(&self) -> f32 {
        self.viz_ap
    }
    pub fn ray_buffer(&self) -> &[f32] {
        &self.vlog_pos
    }
    pub fn ray_write(&self) -> usize {
        self.vlog_w
    }

    fn update_coeffs(&mut self) {
        let tp = 2.0 * PI;
        self.dc_r = clampf(1.0 - tp * 10.0 / self.host_sr, 0.9, 0.99999);
        // Rebuild reverb delay lines scaled to host SR (keeps the same tail time).
        let k = self.host_sr / 44100.0;
        let mk = |base: usize| -> Vec<f32> {
            let n = ((base as f32) * k) as usize;
            vec![0.0; n.max(4)]
        };
        self.combl = CLEN_L.iter().map(|&b| mk(b)).collect();
        self.combr = CLEN_R.iter().map(|&b| mk(b)).collect();
        self.apl = ALEN.iter().map(|&b| mk(b)).collect();
        self.apr = ALEN.iter().map(|&b| mk(b)).collect();
        self.combl_i = [0; NC];
        self.combr_i = [0; NC];
        self.combl_s = [0.0; NC];
        self.combr_s = [0.0; NC];
        self.apl_i = [0; NA];
        self.apr_i = [0; NA];
    }

    #[inline]
    fn rng01(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32) * (1.0 / 4_294_967_296.0)
    }

    #[inline]
    fn next_u(&mut self) -> f32 {
        match self.mode {
            1 => {
                self.qmc += GOLDEN;
                if self.qmc >= 1.0 {
                    self.qmc -= 1.0;
                }
                self.qmc
            }
            2 => {
                let u = (self.strat_i as f32 + self.rng01()) / (STRATA as f32);
                self.strat_i = (self.strat_i + 1) % STRATA;
                u
            }
            _ => self.rng01(),
        }
    }

    fn alloc_voice(&mut self) -> usize {
        if self.nfree > 0 {
            self.nfree -= 1;
            let slot = self.free[self.nfree];
            self.active[self.nactive] = slot;
            self.nactive += 1;
            return slot;
        }
        // No free slots: steal the most faded voice (phase closest to 1 → the
        // Hann tail is near 0) so the theft is click-free.
        let mut best = 0usize;
        let mut best_ph = -1.0f32;
        for k in 0..self.nactive {
            let v = self.active[k];
            let ph = self.voices[v].age * self.voices[v].inv_dur;
            if ph > best_ph {
                best_ph = ph;
                best = k;
            }
        }
        self.active[best]
    }

    fn place(&mut self, pos: f32, depth: u32) {
        let dur_samp = GRAIN_DUR * self.host_sr;
        if dur_samp < 1.0 {
            return;
        }
        let detune = 1.0 + (self.rng01() - 0.5) * DETUNE;
        // Read speed: sample-rate ratio keeps pitch correct across host SR.
        // Shimmer: some grains read an octave up (×2) for a bright, airy sheen.
        let oct = if self.rng01() < self.octave { 2.0 } else { 1.0 };
        let step = oct * (self.samp_sr / self.host_sr) * detune;
        let pan = (self.rng01() * 2.0 - 1.0) * WIDTH;
        let panl = ((1.0 - pan) * 0.5).sqrt(); // equal-power
        let panr = ((1.0 + pan) * 0.5).sqrt();
        let slot = self.alloc_voice();
        self.voices[slot] = Voice {
            pos,
            age: 0.0,
            inv_dur: 1.0 / dur_samp,
            gain: GRAIN_GAIN,
            step,
            panl,
            panr,
            depth,
        };
    }

    fn spawn(&mut self) {
        let len = self.sample.len();
        if len < 2 {
            return;
        }
        let span = (len as f32) / self.samp_sr;
        // Recursive autoevolution: EVO slowly sweeps the focus and the output
        // envelope (ENV) widens the aperture — the render feeds back into itself.
        let eff_focus = clampf(
            self.focus01 * span + (self.evo - 0.5) * self.feedback * span * 0.45,
            0.0,
            span,
        );
        let eff_ap = (self.aperture_ms * 0.001) * (1.0 + self.feedback * self.env * 0.8);
        let off_sec = eff_focus + tri_inv(self.next_u()) * eff_ap;
        let maxp = (len - 2) as f32;
        let pos = clampf(off_sec * self.samp_sr, 0.0, maxp);

        // Log for the visualizer.
        if span > 0.0 {
            self.viz_focus = clampf(eff_focus / span, 0.0, 1.0);
            self.viz_ap = clampf(eff_ap / span, 0.0, 1.0);
        }
        let w = self.vlog_w % VLOG;
        self.vlog_pos[w] = if maxp > 0.0 { pos / maxp } else { 0.5 };
        self.vlog_w = self.vlog_w.wrapping_add(1);

        self.place(pos, self.bounces);
    }

    // One stereo output frame.
    #[inline]
    pub fn tick(&mut self) -> (f32, f32) {
        if self.sample.len() < 2 {
            return (0.0, 0.0);
        }
        let rate_ps = self.grain_rate / self.host_sr;
        self.spawn_acc += rate_ps;
        while self.spawn_acc >= 1.0 {
            self.spawn();
            self.spawn_acc -= 1.0;
        }

        let mut accl = 0.0f32;
        let mut accr = 0.0f32;
        let mut k = 0usize;
        while k < self.nactive {
            let i = self.active[k];
            let ph = self.voices[i].age * self.voices[i].inv_dur;
            if ph >= 1.0 {
                // Capture the dying grain's state before recycling its slot.
                let depth = self.voices[i].depth;
                let endpos = self.voices[i].pos;
                // swap-remove from active list + return slot to the free pool
                self.nactive -= 1;
                self.active[k] = self.active[self.nactive];
                self.free[self.nfree] = i;
                self.nfree += 1;
                // Recursive bounce (Russian roulette): the ray survives with
                // probability = reflection coefficient, relaunching a child grain
                // near where it landed. The decaying chain is the Neumann tail.
                if depth > 0 && self.rng01() < self.refl {
                    let maxp = (self.sample.len().max(2) - 2) as f32;
                    let jitter = (self.rng01() - 0.5) * 0.1 * self.samp_sr;
                    let cpos = clampf(endpos + jitter, 0.0, maxp);
                    self.place(cpos, depth - 1);
                }
                continue; // active[k] is now another voice: don't advance k
            }
            let raw = sample_at(&self.sample, self.voices[i].pos);
            let s = raw * win_at(&self.window, ph) * self.voices[i].gain;
            accl += s * self.voices[i].panl;
            accr += s * self.voices[i].panr;
            self.voices[i].pos += self.voices[i].step;
            self.voices[i].age += 1.0;
            k += 1;
        }

        let (rl, rr) = self.reverb(accl * self.master, accr * self.master);
        // DC blocker (one-pole highpass): y = x - x1 + R·y1
        let yl = rl - self.dc_xl + self.dc_r * self.dc_yl;
        self.dc_xl = rl;
        self.dc_yl = yl;
        let yr = rr - self.dc_xr + self.dc_r * self.dc_yr;
        self.dc_xr = rr;
        self.dc_yr = yr;
        let ol = soft(yl);
        let orr = soft(yr);

        // Recursive autoevolution: follow the output level (ENV) and advance the
        // self-sweep (EVO). ENV widens the aperture; EVO drifts the focus — both
        // gated by `feedback`. Per-sample coefficients ≈ the WASM per-block ones.
        let lvl = (ol.abs() + orr.abs()) * 0.5;
        self.env = self.env * 0.9996 + lvl * 0.0004;
        if self.feedback > 0.0 {
            let step = self.feedback * (0.0004 + self.env * 0.0018) / 256.0;
            self.evo += self.evo_dir * step;
            if self.evo >= 1.0 {
                self.evo = 1.0;
                self.evo_dir = -1.0;
            }
            if self.evo <= 0.0 {
                self.evo = 0.0;
                self.evo_dir = 1.0;
            }
        }
        (ol, orr)
    }

    // Stereo Freeverb-lite (4 combs + 2 allpass per channel).
    #[inline]
    fn reverb(&mut self, inl: f32, inr: f32) -> (f32, f32) {
        if self.reverb_wet <= 0.0 {
            return (inl, inr);
        }
        let fb = 0.7 + REV_ROOM * 0.28;
        let il_in = (inl * 0.85 + inr * 0.15) * 0.015;
        let ir_in = (inr * 0.85 + inl * 0.15) * 0.015;
        let mut ol = 0.0f32;
        let mut orr = 0.0f32;
        for c in 0..NC {
            let il = self.combl_i[c];
            let yl = self.combl[c][il];
            self.combl_s[c] = yl * (1.0 - REV_DAMP) + self.combl_s[c] * REV_DAMP;
            self.combl[c][il] = il_in + self.combl_s[c] * fb;
            self.combl_i[c] = il + 1;
            if self.combl_i[c] >= self.combl[c].len() {
                self.combl_i[c] = 0;
            }
            ol += yl;
            let ir = self.combr_i[c];
            let yr = self.combr[c][ir];
            self.combr_s[c] = yr * (1.0 - REV_DAMP) + self.combr_s[c] * REV_DAMP;
            self.combr[c][ir] = ir_in + self.combr_s[c] * fb;
            self.combr_i[c] = ir + 1;
            if self.combr_i[c] >= self.combr[c].len() {
                self.combr_i[c] = 0;
            }
            orr += yr;
        }
        for a in 0..NA {
            let il = self.apl_i[a];
            let bl = self.apl[a][il];
            let yl = -ol + bl;
            self.apl[a][il] = ol + bl * 0.5;
            self.apl_i[a] = il + 1;
            if self.apl_i[a] >= self.apl[a].len() {
                self.apl_i[a] = 0;
            }
            ol = yl;
            let ir = self.apr_i[a];
            let br = self.apr[a][ir];
            let yr = -orr + br;
            self.apr[a][ir] = orr + br * 0.5;
            self.apr_i[a] = ir + 1;
            if self.apr_i[a] >= self.apr[a].len() {
                self.apr_i[a] = 0;
            }
            orr = yr;
        }
        let w = self.reverb_wet;
        (inl * (1.0 - w * 0.5) + ol * w * 3.0, inr * (1.0 - w * 0.5) + orr * w * 3.0)
    }
}

// ── Free helpers (so the hot loop can split-borrow engine fields) ───────────
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
fn tri_inv(u: f32) -> f32 {
    // Inverse CDF of a symmetric triangular distribution on [-1, 1].
    if u < 0.5 {
        -1.0 + (2.0 * u).sqrt()
    } else {
        1.0 - (2.0 * (1.0 - u)).sqrt()
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

#[inline]
fn sample_at(sample: &[f32], pos: f32) -> f32 {
    let len = sample.len();
    if len < 4 {
        return 0.0;
    }
    let i = pos as usize;
    if i + 2 >= len {
        return 0.0;
    }
    let frac = pos - (i as f32);
    // Catmull-Rom cubic (4-point): less aliasing / more brightness than linear,
    // especially when reading at speed != 1 (host/sample SR mismatch, detune).
    let s0 = if i >= 1 { sample[i - 1] } else { sample[i] };
    let s1 = sample[i];
    let s2 = sample[i + 1];
    let s3 = sample[i + 2];
    let a = -0.5 * s0 + 1.5 * s1 - 1.5 * s2 + 0.5 * s3;
    let b = s0 - 2.5 * s1 + 2.0 * s2 - 0.5 * s3;
    let c = -0.5 * s0 + 0.5 * s2;
    ((a * frac + b) * frac + c) * frac + s1
}

#[inline]
fn win_at(window: &[f32], ph: f32) -> f32 {
    let idx = (ph * ((WIN - 1) as f32)) as usize;
    if idx >= WIN {
        0.0
    } else {
        window[idx]
    }
}

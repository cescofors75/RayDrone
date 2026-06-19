// Headless functional test for the RayDrone WASM engine (the "web rust" build).
// Drives raydrone.wasm exactly like wasm/processor.js does — load a sample, set
// the Hann window + params, run process() — and checks it renders real, finite,
// non-silent stereo audio while exercising reverb / ambient / scale / smart paths.
//
// Run:  node wasm/test_engine.mjs
'use strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const bytes = readFileSync(join(here, 'raydrone.wasm'));
const mod = new WebAssembly.Module(bytes);
const inst = new WebAssembly.Instance(mod, {});
const ex = inst.exports;
const mem = ex.memory;
const f32 = (ptr, len) => new Float32Array(mem.buffer, ptr, len);

const SR = 48000;
const BLOCK = 128;
let failures = 0;
const check = (name, cond, extra = '') => {
    const ok = !!cond;
    if (!ok) failures++;
    console.log(`  ${ok ? '✓' : '✗ FAIL'}  ${name}${extra ? '  — ' + extra : ''}`);
};

// 1) Hann window (2048) — without it win_at() returns 0 and the engine is silent.
const WIN = 2048;
const win = new Float32Array(WIN);
for (let i = 0; i < WIN; i++) win[i] = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / (WIN - 1));
f32(ex.window_ptr(), WIN).set(win);

// 2) A test "scene": 2 s of a 220 Hz sine (the rays will granulate this).
const cap = ex.sample_capacity();
const len = Math.min(SR * 2, cap);
const sample = f32(ex.sample_ptr(), len);
for (let i = 0; i < len; i++) sample[i] = 0.6 * Math.sin((2 * Math.PI * 220 * i) / SR);
ex.seed(0x9e3779b9);
ex.set_sample(len, SR);

// 3) Params: a continuous drone. set_params(focus_s, aperture_s, grain_ms, grain_rate, gain, master)
ex.set_mode(1); // golden-ratio QMC
ex.set_params(1.0, 0.25, 150, 220, 0.3, 1.0);
ex.set_space(0.6, 0.0); // stereo width, no octave shimmer
ex.set_reverb(0.0); // dry first

// Helper: run N blocks, return peak/RMS over the whole run + final-block stats.
function run(blocks) {
    let peak = 0, sumSq = 0, n = 0, nonFinite = 0, lastBlockEnergy = 0;
    for (let b = 0; b < blocks; b++) {
        ex.process(BLOCK);
        const L = f32(ex.out_l_ptr(), BLOCK);
        const R = f32(ex.out_r_ptr(), BLOCK);
        let be = 0;
        for (let i = 0; i < BLOCK; i++) {
            const l = L[i], r = R[i];
            if (!Number.isFinite(l) || !Number.isFinite(r)) nonFinite++;
            const a = Math.abs(l), c = Math.abs(r);
            if (a > peak) peak = a;
            if (c > peak) peak = c;
            sumSq += l * l + r * r;
            be += a + c;
            n += 2;
        }
        lastBlockEnergy = be / (2 * BLOCK);
    }
    return { peak, rms: Math.sqrt(sumSq / n), nonFinite, lastBlockEnergy };
}

console.log('RayDrone WASM engine — headless functional test\n');

console.log('[dry drone]');
const dry = run(200); // ~0.53 s of audio
check('output is finite (no NaN/Inf)', dry.nonFinite === 0, `${dry.nonFinite} bad samples`);
check('output is audible (rms > 1e-4)', dry.rms > 1e-4, `rms=${dry.rms.toExponential(2)}`);
check('output within soft-clip bounds (peak <= 0.7)', dry.peak <= 0.7 + 1e-6, `peak=${dry.peak.toFixed(4)}`);
check('rays are spawning', (ex.spawn_count() >>> 0) > 0, `${ex.spawn_count() >>> 0} grains`);
check('voices are active', (ex.active_voices() >>> 0) > 0, `${ex.active_voices() >>> 0} voices`);

console.log('\n[reverb tail]');
ex.set_reverb(0.6);
run(100); // let the tail build
ex.set_params(1.0, 0.25, 150, 0, 0.3, 1.0); // grain_rate=0 → stop spawning new rays
run(60); // let existing grains die out; only the reverb tail should remain
const tail = run(40);
check('reverb leaves a decaying tail after rays stop', tail.rms > 1e-5 && tail.rms < dry.rms, `tail rms=${tail.rms.toExponential(2)}`);
check('tail stays finite', tail.nonFinite === 0);

console.log('\n[microtonal scale]');
// A just-intonation-ish set of ratios within an octave.
const ratios = Float32Array.from([1.0, 9 / 8, 5 / 4, 4 / 3, 3 / 2, 5 / 3, 15 / 8]);
const sc = Math.min(ratios.length, ex.scale_capacity());
f32(ex.scale_ptr(), sc).set(ratios.subarray(0, sc));
ex.set_scale(sc);
ex.set_params(1.0, 0.25, 150, 220, 0.3, 1.0);
const scaled = run(120);
check('renders cleanly with a microtonal scale', scaled.nonFinite === 0 && scaled.rms > 1e-4, `rms=${scaled.rms.toExponential(2)}`);
ex.set_scale(0); // back to continuous pitch

console.log('\n[ambient constellation + smart rays + bounces]');
ex.set_ambient(1, 3, 2, 0.25, 0.3, 0.4);
ex.set_smart(1);
ex.set_fx(0.3, 2, 0.5, 0.4); // aberration, 2 bounces, reflect, feedback
const amb = run(300);
check('ambient/smart/bounces render finite audio', amb.nonFinite === 0, `${amb.nonFinite} bad`);
check('ambient is audible', amb.rms > 1e-4, `rms=${amb.rms.toExponential(2)}`);
let fociAlive = 0;
const fcap = ex.foci_cap();
const fw = f32(ex.foci_w_ptr(), fcap);
for (let i = 0; i < fcap; i++) if (fw[i] > 0.004) fociAlive++;
check('focus constellation is populated', fociAlive > 0, `${fociAlive} foci alive`);

console.log('\n[convergence lab — the offline estimator]');
// The same estimator, measured offline: more rays → lower error vs the target.
f32(ex.lab_win_ptr(), ex.lab_grain()).fill(1.0); // flat window for the measurement
ex.lab_target(50000, 4000);
ex.lab_estimate(50000, 4000, 64, 2, 1);
const errLow = ex.lab_rms();
ex.lab_estimate(50000, 4000, 4096, 2, 1);
const errHigh = ex.lab_rms();
check('estimator converges (more rays → less error)', errHigh < errLow, `err: ${errLow.toExponential(2)} (64) → ${errHigh.toExponential(2)} (4096)`);

console.log(`\n${failures === 0 ? '✅ ALL PASSED' : '❌ ' + failures + ' CHECK(S) FAILED'}`);
process.exit(failures === 0 ? 0 : 1);

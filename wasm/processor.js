// AudioWorkletProcessor que ejecuta el motor RayDrone (Rust→wasm). Estéreo.
// Incluye diagnóstico: CPU del hilo de audio, voces (rayos) activas y granos/seg.

class RayDroneProcessor extends AudioWorkletProcessor {
    constructor(options) {
        super();
        const mod = options.processorOptions.module;
        this.inst = new WebAssembly.Instance(mod, {});
        this.ex = this.inst.exports;
        this.mem = this.ex.memory;
        this.outL = this.ex.out_l_ptr();
        this.outR = this.ex.out_r_ptr();
        this.ready = false;
        this.ex.seed(0x9e3779b9);
        this.lastW = 0;
        this.rayOff = [];
        this.rayBand = [];
        this.blockCount = 0;
        // Diagnóstico de rendimiento
        this.now = (typeof performance !== 'undefined' && performance.now) ? () => performance.now() : null;
        this.cpuAcc = 0;
        this.cpuBlocks = 0;
        this.lastSpawn = 0;
        this.port.onmessage = (e) => this.onMsg(e.data);
    }

    onMsg(d) {
        const ex = this.ex;
        if (d.type === 'sample') {
            const cap = ex.sample_capacity();
            const len = Math.min(d.data.length, cap);
            new Float32Array(this.mem.buffer, ex.sample_ptr(), len).set(d.data.subarray(0, len));
            ex.set_sample(len, d.sampleRate);
            this.ready = true;
        } else if (d.type === 'window') {
            new Float32Array(this.mem.buffer, ex.window_ptr(), d.data.length).set(d.data);
        } else if (d.type === 'params') {
            ex.set_params(d.focus, d.aperture, d.grainMs, d.grainRate, d.gain, d.master);
        } else if (d.type === 'mode') {
            ex.set_mode(d.value >>> 0);
        } else if (d.type === 'fx') {
            ex.set_fx(d.aber, d.bounces >>> 0, d.refl, d.feedback);
        } else if (d.type === 'space') {
            ex.set_space(d.width, d.oct);
        } else if (d.type === 'pitch') {
            ex.set_pitch(d.mult);
        } else if (d.type === 'reverb') {
            ex.set_reverb(d.wet);
        } else if (d.type === 'smart') {
            ex.set_smart(d.on >>> 0);
        }
    }

    process(inputs, outputs) {
        const out = outputs[0];
        const frames = out[0].length;
        if (this.ready) {
            // Cronometrar el render del motor (carga real del hilo de audio).
            const t0 = this.now ? this.now() : 0;
            this.ex.process(frames);
            if (this.now) { this.cpuAcc += this.now() - t0; this.cpuBlocks++; }

            out[0].set(new Float32Array(this.mem.buffer, this.outL, frames));
            if (out[1]) out[1].set(new Float32Array(this.mem.buffer, this.outR, frames));

            // Recoger rayos (offset + banda) para la visualización.
            const w = this.ex.slog_w() >>> 0;
            if (w !== this.lastW) {
                const cap = this.ex.slog_cap();
                const off = new Float32Array(this.mem.buffer, this.ex.slog_ptr(), cap);
                const bnd = new Float32Array(this.mem.buffer, this.ex.slog_b_ptr(), cap);
                let count = (w - this.lastW) >>> 0;
                if (count > cap) count = cap;
                for (let k = 0; k < count; k++) {
                    const idx = (this.lastW + k) % cap;
                    this.rayOff.push(off[idx]);
                    this.rayBand.push(bnd[idx]);
                }
                this.lastW = w;
            }

            if (++this.blockCount >= 16) {
                const blocks = this.blockCount;
                this.blockCount = 0;
                const level = this.ex.out_level();
                // ── Diagnóstico ──
                const blockMs = frames / sampleRate * 1000;
                const cpu = (this.now && this.cpuBlocks > 0) ? (this.cpuAcc / this.cpuBlocks) / blockMs * 100 : -1;
                const voices = this.ex.active_voices();
                const sc = this.ex.spawn_count() >>> 0;
                const spawnsDelta = (sc - this.lastSpawn) >>> 0;
                this.lastSpawn = sc;
                const spawnsPerSec = spawnsDelta / (blocks * frames / sampleRate);
                this.cpuAcc = 0; this.cpuBlocks = 0;
                const perf = { cpu, voices, spawnsPerSec };

                if (this.rayOff.length) {
                    this.port.postMessage({ type: 'rays', offsets: this.rayOff, bands: this.rayBand, level, perf });
                    this.rayOff = [];
                    this.rayBand = [];
                } else {
                    this.port.postMessage({ type: 'level', level, perf });
                }
            }
        } else {
            out[0].fill(0);
            if (out[1]) out[1].fill(0);
        }
        return true;
    }
}

registerProcessor('raydrone', RayDroneProcessor);

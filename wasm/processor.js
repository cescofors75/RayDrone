// AudioWorkletProcessor que ejecuta el motor RayDrone (Rust→wasm). Estéreo.
// Incluye diagnóstico: CPU del hilo de audio, voces (rayos) activas y granos/seg.

class RayDroneProcessor extends AudioWorkletProcessor {
    constructor(options) {
        super();
        // Compilar aquí dentro: Safari no permite clonar un WebAssembly.Module
        // del hilo principal al worklet, así que llegan los bytes crudos.
        const mod = new WebAssembly.Module(options.processorOptions.wasmBytes);
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
        this.rayRatio = [];
        this.blockCount = 0;
        // Diagnóstico de rendimiento
        this.now = (typeof performance !== 'undefined' && performance.now) ? () => performance.now() : null;
        this.cpuAcc = 0;
        this.cpuBlocks = 0;
        this.lastSpawn = 0;
        // Grabación de la salida del motor (export WAV); tope de seguridad 5 min.
        // Se vacía por chunks para no retener/duplicar minutos enteros en el worklet.
        this.recOn = false;
        this.recL = [];
        this.recR = [];
        this.recFrames = 0;
        this.recChunkFrames = 0;
        this.port.onmessage = (e) => this.onMsg(e.data);
    }

    onMsg(d) {
        try {
            this.handleMsg(d);
        } catch (err) {
            // Lo típico: raydrone.wasm está desactualizado respecto al JS (p.ej.
            // tras un `git pull` sin recompilar — el .wasm NO está en git, hay
            // que correr wasm/build.sh) y falta un export nuevo. Sin este
            // try/catch, la excepción moría en silencio aquí dentro del
            // worklet y el control simplemente "no hacía nada".
            this.port.postMessage({
                type: 'enginemismatch',
                messageType: d.type,
                error: String((err && err.message) || err),
            });
        }
    }

    handleMsg(d) {
        const ex = this.ex;
        if (d.type === 'sample') {
            const cap = ex.sample_capacity();
            const len = Math.min(d.data.length, cap);
            new Float32Array(this.mem.buffer, ex.sample_ptr(), len).set(d.data.subarray(0, len));
            ex.set_sample(len, d.sampleRate);
            this.ready = true;
            // Avisar a la UI si el sample no cabe entero (truncado silencioso, no más)
            this.port.postMessage({ type: 'sampleinfo', used: len, total: d.data.length, truncated: d.data.length > cap });
        } else if (d.type === 'record') {
            if (d.on) {
                this.recOn = true;
                this.recL = [];
                this.recR = [];
                this.recFrames = 0;
                this.recChunkFrames = 0;
            } else if (this.recOn) {
                this.recOn = false;
                this.flushRec(true, false);
            }
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
        } else if (d.type === 'scale') {
            // Tabla de ratios microtonales (vacía = pitch continuo)
            const len = Math.min(d.data.length, ex.scale_capacity());
            if (len) new Float32Array(this.mem.buffer, ex.scale_ptr(), len).set(d.data.subarray(0, len));
            ex.set_scale(len);
        } else if (d.type === 'keys') {
            // Ratios de las notas sostenidas (piano por teclado/ratón/táctil);
            // vacía = ninguna tecla pulsada, se cae a Microtonal/Voicing o pitch continuo.
            const len = Math.min(d.data.length, ex.keys_capacity());
            if (len) new Float32Array(this.mem.buffer, ex.keys_ptr(), len).set(d.data.subarray(0, len));
            ex.set_keys(len);
        } else if (d.type === 'chord') {
            // Voicing afinado de core::music (0 = unísono/continuo).
            ex.set_chord(d.preset >>> 0);
        } else if (d.type === 'filter') {
            ex.set_filter(d.cutoff, d.res);
        } else if (d.type === 'filterlfo') {
            ex.set_filter_lfo(d.rate, d.depth);
        } else if (d.type === 'reverb') {
            ex.set_reverb(d.wet);
        } else if (d.type === 'smart') {
            ex.set_smart(d.on >>> 0);
        } else if (d.type === 'ambient') {
            ex.set_ambient(d.on >>> 0, d.seeds >>> 0, d.depth >>> 0, d.spread, d.drift, d.rate);
            this.ambOn = (d.on >>> 0) === 1;
        }
    }

    // Concatenar solo el chunk actual y enviarlo a la página (buffers transferidos).
    flushRec(done = false, limited = false) {
        let total = 0;
        for (const a of this.recL) total += a.length;
        if (total > 0) {
            const L = new Float32Array(total);
            const R = new Float32Array(total);
            let o = 0;
            for (let i = 0; i < this.recL.length; i++) {
                L.set(this.recL[i], o);
                R.set(this.recR[i], o);
                o += this.recL[i].length;
            }
            this.port.postMessage({ type: 'recdata', l: L, r: R, sr: sampleRate, done, limited }, [L.buffer, R.buffer]);
        } else if (done) {
            this.port.postMessage({ type: 'recdata', l: new Float32Array(0), r: new Float32Array(0), sr: sampleRate, done, limited });
        }
        this.recL = [];
        this.recR = [];
        this.recChunkFrames = 0;
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

            if (this.recOn) {
                this.recL.push(out[0].slice());
                this.recR.push((out[1] || out[0]).slice());
                this.recFrames += frames;
                this.recChunkFrames += frames;
                if (this.recChunkFrames >= sampleRate) this.flushRec(false, false);
                if (this.recFrames >= sampleRate * 300) { // tope 5 min
                    this.recOn = false;
                    this.flushRec(true, true);
                }
            }

            // Recoger rayos (offset + banda) para la visualización.
            const w = this.ex.slog_w() >>> 0;
            if (w !== this.lastW) {
                const cap = this.ex.slog_cap();
                const off = new Float32Array(this.mem.buffer, this.ex.slog_ptr(), cap);
                const bnd = new Float32Array(this.mem.buffer, this.ex.slog_b_ptr(), cap);
                const rat = new Float32Array(this.mem.buffer, this.ex.slog_s_ptr(), cap);
                let count = (w - this.lastW) >>> 0;
                if (count > cap) count = cap;
                for (let k = 0; k < count; k++) {
                    const idx = (this.lastW + k) % cap;
                    this.rayOff.push(off[idx]);
                    this.rayBand.push(bnd[idx]);
                    this.rayRatio.push(rat[idx]);
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

                // Constelación de focos (solo en ambient): posición + peso de cada foco vivo.
                let foci = null;
                if (this.ambOn) {
                    const cap = this.ex.foci_cap();
                    const fp = new Float32Array(this.mem.buffer, this.ex.foci_ptr(), cap);
                    const fw = new Float32Array(this.mem.buffer, this.ex.foci_w_ptr(), cap);
                    foci = [];
                    for (let i = 0; i < cap; i++) if (fw[i] > 0.004) foci.push(fp[i], fw[i]);
                }

                if (this.rayOff.length) {
                    this.port.postMessage({ type: 'rays', offsets: this.rayOff, bands: this.rayBand, ratios: this.rayRatio, foci, level, perf });
                    this.rayOff = [];
                    this.rayBand = [];
                    this.rayRatio = [];
                } else {
                    this.port.postMessage({ type: 'level', foci, level, perf });
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

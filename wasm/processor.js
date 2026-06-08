// AudioWorkletProcessor que ejecuta el motor RayDrone (Rust→wasm).
//
// El módulo wasm se compila en el hilo principal y se pasa ya compilado por
// processorOptions (WebAssembly.Module es clonable hacia el worklet). Aquí lo
// instanciamos de forma SÍNCRONA (permitido en el constructor del worklet) y, en
// cada bloque de 128 muestras, llamamos a process() y copiamos la salida.

class RayDroneProcessor extends AudioWorkletProcessor {
    constructor(options) {
        super();
        const mod = options.processorOptions.module;
        this.inst = new WebAssembly.Instance(mod, {});
        this.ex = this.inst.exports;
        this.mem = this.ex.memory;
        this.outPtr = this.ex.out_ptr();
        this.ready = false;
        this.ex.seed(0x9e3779b9);
        this.lastW = 0;        // último índice de rayo leído del registro
        this.rayAccum = [];    // rayos acumulados para enviar al hilo principal
        this.blockCount = 0;
        this.port.onmessage = (e) => this.onMsg(e.data);
    }

    onMsg(d) {
        const ex = this.ex;
        if (d.type === 'sample') {
            const cap = ex.sample_capacity();
            const len = Math.min(d.data.length, cap);
            const dst = new Float32Array(this.mem.buffer, ex.sample_ptr(), len);
            dst.set(d.data.subarray(0, len));
            ex.set_sample(len, d.sampleRate);
            this.ready = true;
        } else if (d.type === 'window') {
            const w = new Float32Array(this.mem.buffer, ex.window_ptr(), d.data.length);
            w.set(d.data);
        } else if (d.type === 'params') {
            ex.set_params(d.focus, d.aperture, d.grainMs, d.grainRate, d.gain, d.master);
        } else if (d.type === 'mode') {
            ex.set_mode(d.value >>> 0);
        } else if (d.type === 'fx') {
            ex.set_fx(d.aber, d.bounces >>> 0, d.refl, d.feedback);
        }
    }

    process(inputs, outputs) {
        const out = outputs[0];
        const frames = out[0].length; // 128
        if (this.ready) {
            this.ex.process(frames);
            // Reconstruimos la vista cada bloque por si la memoria creciera (no debería).
            const o = new Float32Array(this.mem.buffer, this.outPtr, frames);
            out[0].set(o);
            if (out[1]) out[1].set(o); // mono → estéreo

            // Recoger los rayos (posiciones de granos) y enviarlos throttle ~21 fps.
            const w = this.ex.slog_w() >>> 0;
            if (w !== this.lastW) {
                const cap = this.ex.slog_cap();
                const log = new Float32Array(this.mem.buffer, this.ex.slog_ptr(), cap);
                let count = (w - this.lastW) >>> 0;
                if (count > cap) count = cap;
                for (let k = 0; k < count; k++) this.rayAccum.push(log[(this.lastW + k) % cap]);
                this.lastW = w;
            }
            if (++this.blockCount >= 16) {
                this.blockCount = 0;
                if (this.rayAccum.length) {
                    this.port.postMessage({ type: 'rays', offsets: this.rayAccum });
                    this.rayAccum = [];
                }
            }
        } else {
            out[0].fill(0);
            if (out[1]) out[1].fill(0);
        }
        return true; // mantener vivo el procesador
    }
}

registerProcessor('raydrone', RayDroneProcessor);

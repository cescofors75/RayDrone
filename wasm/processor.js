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
        } else {
            out[0].fill(0);
            if (out[1]) out[1].fill(0);
        }
        return true; // mantener vivo el procesador
    }
}

registerProcessor('raydrone', RayDroneProcessor);

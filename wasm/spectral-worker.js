/* STFT fuera del main thread. El motor recibe sólo q/CDF ya normalizadas. */
self.onmessage = ({ data }) => {
  const t0 = performance.now();
  const { samples, sampleRate, fftSize, hopSize, minHz, maxHz, smoothing } = data;
  const source = new Float32Array(samples);
  const frames = Math.max(1, Math.min(16384, Math.floor(Math.max(0, source.length - fftSize) / hopSize) + 1));
  const bins = Math.min(192, fftSize >> 1);
  const image = new Uint8Array(frames * bins);
  const energy = new Float32Array(frames);
  const lo = Math.max(0, Math.floor(minHz / sampleRate * fftSize));
  const hi = Math.min(fftSize >> 1, Math.ceil(maxHz / sampleRate * fftSize));
  // DFT reducida: estable y portable para el MVP; FFT Rust radix-2 vive en
  // spectral_dsp.rs para la ruta WASM futura. Este worker evita bloquear UI.
  for (let f = 0; f < frames; f++) {
    const start = Math.floor(f * (source.length - fftSize) / Math.max(1, frames - 1));
    let bandEnergy = 0, maxDb = -120;
    for (let b = 0; b < bins; b++) {
      const k = Math.floor(b * (fftSize / 2 - 1) / Math.max(1, bins - 1));
      let re = 0, im = 0;
      for (let n = 0; n < fftSize; n++) {
        const w = .5 - .5 * Math.cos(2 * Math.PI * n / (fftSize - 1));
        const p = 2 * Math.PI * k * n / fftSize;
        const x = source[start + n] * w;
        re += x * Math.cos(p); im -= x * Math.sin(p);
      }
      const mag2 = (re * re + im * im) / (fftSize * fftSize);
      if (k >= lo && k <= hi) bandEnergy += mag2;
      const db = 10 * Math.log10(mag2 + 1e-12); maxDb = Math.max(maxDb, db);
      image[f * bins + (bins - 1 - b)] = Math.max(0, Math.min(255, Math.round((db + 90) / 90 * 255)));
    }
    energy[f] = bandEnergy;
  }
  const q = new Float32Array(frames);
  let sum = 0;
  for (let i = 0; i < frames; i++) { const e = energy[i]; const v = Math.pow(e + 1e-10, 0.5); q[i] = v; sum += v; }
  if (!Number.isFinite(sum) || sum <= 0) { q.fill(1 / frames); } else for (let i = 0; i < frames; i++) q[i] /= sum;
  const alpha = Math.max(0, Math.min(.98, smoothing));
  if (alpha) { for (let i = 1; i < frames; i++) q[i] = alpha * q[i - 1] + (1 - alpha) * q[i]; sum = q.reduce((a, b) => a + b, 0); for (let i=0;i<frames;i++) q[i] /= sum; }
  const cdf = new Float32Array(frames); let acc = 0;
  for (let i = 0; i < frames; i++) { acc += q[i]; cdf[i] = acc; } cdf[frames - 1] = 1;
  self.postMessage({ type:'analysis', frames, bins, image, q, cdf, analysisMs: performance.now() - t0 }, [image.buffer, q.buffer, cdf.buffer]);
};

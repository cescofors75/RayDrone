# RayDrone Spectral Lab — notas MVP

## Hipótesis

Una STFT del buffer puede construir una importancia temporal `q(τ)` para guiar el nacimiento de grains/rays, sin alterar bins ni hacer síntesis iSTFT. Es una investigación tímbrica, no una afirmación de mejor convergencia perceptual.

## Arquitectura

`spectral.html` carga y decodifica audio. `spectral-worker.js` calcula el mapa STFT y devuelve espectrograma, `q` y CDF. El AudioWorklet copia esas tablas a memoria WASM preasignada; `raydrone.rs` hace la búsqueda binaria y crea el grano dentro del callback sin asignaciones. `spectral_dsp.rs` contiene la FFT radix-2 y pruebas reproducibles: es la ruta Rust/WASM prevista para sustituir el kernel de análisis del Worker.

## Matemática y modos

Para un frame `t`, `E[t] = Σ_{k∈banda}|X[t,k]|²`; `q[t]=sqrt(E[t]+ε)/Σsqrt(E+ε)`. Random, Stratified, QMC y Time Energy conservan las rutas existentes. Spectral Energy lee `q`; Hybrid alterna temporal/espectral. Creative Spectral Bias usa `q` directamente. Unbiased aplica `p/q`, con `p=1/N`, suelo configurable y tope 4 para mantener seguridad de nivel.

## Medición, límites y próximos pasos

La UI enseña tiempo de análisis, frames, memoria del mapa, CPU observada del AudioWorklet, voces y grains/s. La DFT reducida del Worker es deliberadamente conservadora para mantener el hilo principal libre; no es la medición final de CPU Rust. No hay iSTFT, modificación de bins, flux activo, entrada en directo, GPU ni raytracing 2D. Próximo paso: compilar `spectral_dsp.rs` como wasm dedicado y comparar, con señales fijas, el coste y la elección temporal frente a la implementación Worker.

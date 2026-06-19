# RayDrone

![license](https://img.shields.io/badge/license-MIT-ff355e)
![engine](https://img.shields.io/badge/engine-Rust%20%2B%20WebAssembly-ff355e)
![web audio](https://img.shields.io/badge/Web%20Audio-AudioWorklet-ff355e)
![dependencies](https://img.shields.io/badge/dependencies-zero-ff355e)

> **A new methodology for audio rendering: applying the stochastic ray tracing paradigm to the time domain of an audio buffer.**

RayDrone turns any WAV/MP3 into evolving **drones, pads and textures** by treating the
sample as a *scene* and casting **stochastic rays** (audio grains) at a focal point.
The drone is not stored in the sample — it **emerges** from the convergence of the rays,
exactly as a rendered image emerges from the convergence of light rays.

---

## 🌐 Language / Idioma

- [English](#english)
- [Català](#català)
- [Castellano](#castellano)

---

## 📦 Two versions

| Version | File | Engine | Status |
|---|---|---|---|
| **WASM** (current) | [`wasm/`](wasm/) → [`wasm/README.md`](wasm/README.md) | **Rust → WebAssembly + AudioWorklet** | ✅ **Actively developed** — per-sample engine, stereo, reverb, Convergence Lab, WAV export |
| **VST** (plugin) | [`vst/`](vst/) → [`vst/README.md`](vst/README.md) | **Rust + nih-plug (VST3 / CLAP)** | 🎛️ **Simplified** — built-in scenes or load a WAV, 7 knobs + Tonal/Drone/Shimmer presets, recursive autoevolution, live ray visualizer |
| **Classic** (legacy) | [`rta.html`](rta.html) | Vanilla JS + Web Audio API | 🧊 **Frozen** — kept as a no-build demo; no new features |

Both implement the same idea, but **all development happens in the WASM version**: it
mixes **every sample** in a tight loop on the audio thread, which removes the JS engine's
limits by design (no voice cap, no `setTimeout` jitter, no pulsing). The Classic build
remains useful as a zero-toolchain demo (open the file, it runs).
See **[wasm/README.md](wasm/README.md)** for how to build and run the current version.

---
---

# English

## What is RayDrone?

RayDrone is a **rendering methodology** for sound: instead of synthesizing audio from
blocks or particles, it takes a recorded buffer and treats it as a *temporal scene*. Each
**ray** is a short grain extracted from a random position within a dispersion cone around a
focal point. All rays play together and their statistical convergence **is** the drone.

It is, mechanically, a close relative of **asynchronous granular synthesis** — but
reinterpreted through the lens of optical rendering, which gives it both an intuitive
control set (depth of field, aperture, focus) and a *measurable* mathematical foundation.

---

## Genesis: from a pixel to a sound

The whole idea started with a question about graphics: **how is a single pixel actually
defined in ray tracing?**

A pixel is not a stored colour. In physically based rendering it is the solution of
Kajiya's **rendering equation** — the light leaving a point, an *integral* over every
incoming direction:

```
L_o(x, ω_o) = L_e(x, ω_o) + ∫_Ω f_r(x, ω_i, ω_o) · L_i(x, ω_i) · (ω_i · n) dω_i
```

That integral has no closed form, so a renderer **estimates** it by Monte Carlo: cast `N`
random rays, evaluate the integrand along each one, and average them:

```
pixel ≈ (1/N) Σᵢ contribution(rayᵢ)
```

The pixel **emerges** from the average of N stochastic samples; more rays → the estimate
converges → less noise.

Then came the flip that started everything: **what if a single audio sample were defined
the same way?** Not stored, but the result of an integral estimated by stochastic samples.
Swap the hemisphere of directions for a *window of time* around a focal instant, light rays
for *grains*, and radiance for *amplitude*:

```
output[n] ≈ (1/N) Σᵢ s(τᵢ + n) · w[n]      (τᵢ drawn from the dispersion around the focus)
```

Same shape. Same Monte Carlo estimator. Same `1/√N` convergence. The **drone is the audio
pixel**: it does not exist in the sample — it emerges when you render it. That parallel
between the pixel formula and the sample formula *is* the whole project.

---

## The theory

### Ray tracing in graphics (the origin)

In 3D rendering, ray tracing casts **N stochastic rays** from a camera through a scene.
Each ray samples a different path. The final pixel is not stored anywhere — it **emerges**
from the statistical convergence of all those rays. More rays → less noise → cleaner
render. The key insight: *the image does not exist until you render it.*

### Transposing the paradigm to audio

| Optical ray tracing | RayDrone |
|---|---|
| 3D scene geometry | Audio buffer (time-domain waveform) |
| Camera focal point | Playhead position (seconds) |
| Aperture / depth of field | Temporal dispersion (milliseconds) |
| N stochastic rays | N audio grains fired at random offsets |
| Pixel luminance convergence | Drone / sustained texture emergence |
| Render output | Acoustic field |

---

## Why this is different from granular synthesis

| Granular synthesis | RayDrone |
|---|---|
| Synthesis technique | Rendering methodology |
| Grains are building blocks | Rays are measurement samples |
| Result is constructed | Result converges |
| Rooted in time-frequency theory | Rooted in stochastic geometry |
| Mental model: particles | Mental model: optics / physics of light |

> **Honest framing.** The engine is a close relative of *asynchronous granular synthesis*
> (Xenakis, Roads, Truax, Gabor). The claim is **not** "a brand-new kind of sound
> generation"; it is that granular resynthesis can be cast as **Monte Carlo estimation of a
> transport integral**, which lets us import the *machinery* of the rendering equation —
> importance sampling, stratification, Russian roulette, recursive transport — as a
> control-and-quality paradigm. That reframing is what is new, and unlike a metaphor it is
> **measurable** (see the Convergence Lab).

---

## Mathematical foundation (the theory, for real)

The optical metaphor only earns the name "ray tracing" if it has the one property that
defines Monte Carlo rendering: **a fixed ground truth that the samples converge toward,
with quantifiable variance.** Here it does.

- **The target is a defined integral.** For a source `s`, focus `τ₀` and a sampling density
  `p(τ)` (triangular, peaking at the focus), the rendered grain is the *deterministic*
  signal `target[n] = w[n] · ∫ p(τ)·s(τ+n) dτ` — a windowed blur of the source. It is the
  audio analogue of the pixel `L(x) = ∫ f(x,ω) dω` in Kajiya's rendering equation.
- **Each ray is an unbiased estimator.** With `N` offsets `τᵢ ~ p`,
  `render_N[n] = w[n]·(1/N) Σ s(τᵢ+n)` satisfies `E[render_N] = target`, and the error
  falls as `RMS(render_N − target) ∝ 1/√N`. *More rays → less noise → cleaner render* is now
  literally true and falsifiable.
- **Variance reduction (straight from the renderer):** *stratified* (≈ −0.75), *quasi-Monte
  Carlo* golden-ratio (≈ −0.73) and *importance* (≈ −0.75) all beat the textbook `1/√N`.
- **Russian roulette** bounces: a path survives with probability = reflection coefficient at
  constant gain, so the tail is the unbiased solution of a Neumann series — the *recursive*
  form of the transport equation, not an ad-hoc delay.

### Convergence Lab (classic version)

The `rta.html` build ships a **Convergence Lab** panel that runs the experiment live:
computes the exact deterministic target, renders Monte Carlo estimates across `N = 1…1024`
for all four sampling strategies, plots **RMS error vs N** on a log–log axis with the ideal
`1/√N` line, and offers an audible A/B (target vs N=4 vs N=256 at the same level). The
theory holds by measurement, not by adjective.

---

## Acoustic analogy: depth of field

- **Narrow aperture / short focal distance** → sharp focus, coherent, almost pitched.
- **Wide aperture / long focal distance** → deep blur, dense atmospheric texture — the drone.
- **More rays (N)** → less stochastic noise, smoother convergence — cleaner render.

---

## Parameters

### Core

| Parameter | Description |
|---|---|
| **Master Volume** | Global output level 0–100 % |
| **N (Number of Rays)** | Density of the render. More rays → smoother, richer drone |
| **Aperture (Dispersion)** | Width of the temporal cone (ms). Narrow = tonal, wide = textural |
| **Focal Point (Position)** | The point in time the rays are cast around |

### Advanced optical extensions

| Parameter | Optical analogy | Acoustic effect |
|---|---|---|
| **Sampling Strategy** | Monte Carlo estimator | `Random` / `Stratified` / `Quasi-MC` / `Importance` (the last three converge faster than 1/√N) |
| **Chromatic Aberration** | Lens dispersion by wavelength | Lows get a wider aperture, highs a narrow one → big low clouds, crisp highs |
| **BRDF Roughness** | Surface micro-geometry | 100 % = diffuse Monte Carlo drone; 0 % = specular / harmonic placement |
| **Bounce Count** | Recursive transport depth | Depth cap for Russian-roulette secondary rays (the Neumann tail) |
| **Reflection Coefficient** | Path survival probability | Probability a bounce survives → tail energy & length |
| **Autoevolution (α/β/γ)** | Recursive feedback | Low/Mid/High FFT energy is fed back to modulate the next cycle |

---

## The WASM upgrade (Rust + WebAssembly)

A separate page, [`wasm/`](wasm/), reimplements the engine in **Rust compiled to
WebAssembly**, running inside an **AudioWorklet** — mixing every sample in a tight loop.
This is the architecture where WASM genuinely helps, and it fixes the JS engine's pain
points by design:

- **No voice cap / no starvation pulsing**, **sample-accurate timing** (no `setTimeout`),
  a **continuous grain cloud**, and an integrated **soft-clip**.
- Sampling: **Random / Stratified / Quasi-MC**.
- **Chromatic aberration** (per-band aperture + one-pole filter), **Russian-roulette
  bounces**, **recursive autoevolution** (output envelope feeds back into focus/aperture).
- **Stereo width** (per-grain equal-power pan), **octave / shimmer** and **±12-semitone pitch**.
- **Visuals:** dispersion cone, **rays colored by band** (low/mid/high), a live output meter
  and level-reactive glow.
- **Record & export:** capture the engine's stereo output and download it as a 16-bit WAV.
- **Convergence Lab with CSV export** — reproducible convergence data (curves + fitted
  slopes) straight from the browser.
- **Dependency-free build:** no `wasm-bindgen`, no crates → no crates.io needed, just `rustc`
  + the `wasm32-unknown-unknown` target. Build & run instructions in
  **[wasm/README.md](wasm/README.md)**.

---

## Usage (classic)

1. Open `rta.html` in any modern browser.
2. Load a WAV or MP3 file.
3. It starts in **Simple mode**: pick a preset (Original / Drone / Shimmer), move the single
   **Character (Tonal → Drone)** knob, set volume and play.
4. Hit **⚙ Advanced** to reveal the full optical controls, the sampling selector and the
   **Convergence Lab**.
5. Record a WAV straight from the interface and drag it into your DAW.

---

## Inspired by

- **Iannis Xenakis** — stochastic music theory
- **James Kajiya** — The Rendering Equation (1986)
- **Fred Again** — vocal texture processing
- **Hans Zimmer** — sustained orchestral drones
- **Curtis Roads** — microsound and granular theory (as a contrast reference)

---

## License

MIT — free to use, adapt and build upon. Credit appreciated.

---
---

# Català

## Què és RayDrone?

RayDrone és una **metodologia de renderitzat** per al so: en comptes de sintetitzar l'àudio
des de blocs o partícules, agafa un buffer gravat i el tracta com una *escena temporal*.
Cada **raig** és un gra curt extret d'una posició aleatòria dins del con de dispersió al
voltant d'un punt focal. Tots els raigs sonen alhora i la seva convergència estadística
**és** el drone — no existeix a la mostra, **emergeix**.

Mecànicament és parent proper de la **síntesi granular asíncrona**, però reinterpretada amb
la lent del renderitzat òptic, cosa que li dóna controls intuïtius (profunditat de camp,
obertura, focus) i una base matemàtica **mesurable**.

---

## Dues versions

| Versió | Fitxer | Motor | Estat |
|---|---|---|---|
| **WASM** (actual) | [`wasm/`](wasm/) → [`wasm/README.md`](wasm/README.md) | **Rust → WebAssembly + AudioWorklet** | ✅ **En desenvolupament actiu** |
| **Clàssica** (legacy) | [`rta.html`](rta.html) | JavaScript vanilla + Web Audio | 🧊 **Congelada** — demo sense compilació, sense funcions noves |

---

## Gènesi: d'un píxel a un so

Tot va començar amb una pregunta sobre gràfics: **com es defineix realment un píxel en el
ray tracing?**

Un píxel no és un color emmagatzemat. En el renderitzat basat en física és la solució de
l'**equació de renderitzat** de Kajiya — la llum que surt d'un punt, una *integral* sobre
totes les direccions d'entrada:

```
L_o(x, ω_o) = L_e(x, ω_o) + ∫_Ω f_r(x, ω_i, ω_o) · L_i(x, ω_i) · (ω_i · n) dω_i
```

Aquesta integral no té forma tancada, així que el renderer l'**estima** per Monte Carlo:
llança `N` raigs aleatoris, avalua l'integrand a cadascun i en fa la mitjana:

```
píxel ≈ (1/N) Σᵢ contribució(raigᵢ)
```

El píxel **emergeix** de la mitjana de N mostres estocàstiques; més raigs → l'estimació
convergeix → menys soroll.

Llavors va arribar el gir que ho va engegar tot: **i si una mostra d'àudio es definís igual?**
No emmagatzemada, sinó el resultat d'una integral estimada amb mostres estocàstiques.
Canvia l'hemisferi de direccions per una *finestra de temps* al voltant d'un instant focal,
els raigs de llum per *grans* i la radiància per *amplitud*:

```
output[n] ≈ (1/N) Σᵢ s(τᵢ + n) · w[n]      (τᵢ pres de la dispersió al voltant del focus)
```

Mateixa forma. Mateix estimador Monte Carlo. Mateixa convergència `1/√N`. El **drone és el
píxel d'àudio**: no existeix a la mostra — emergeix quan el renderitzes. Aquest paral·lelisme
entre la fórmula del píxel i la de la mostra *és* tot el projecte.

---

## La teoria

En el renderitzat 3D, el ray tracing llança **N raigs estocàstics** des d'una càmera. El
píxel final no s'emmagatzema: **emergeix** de la convergència de tots els raigs. Més raigs →
menys soroll → render més net. *La imatge no existeix fins que la renderitzes.*

| Ray tracing òptic | RayDrone |
|---|---|
| Geometria de l'escena 3D | Buffer d'àudio (domini temporal) |
| Punt focal de la càmera | Posició del capçal (segons) |
| Obertura / profunditat de camp | Dispersió temporal (mil·lisegons) |
| N raigs estocàstics | N grans disparats a offsets aleatoris |
| Convergència de luminància | Emergència del drone |

---

## Per què és diferent de la síntesi granular

> **Marc honest.** El motor és parent proper de la *síntesi granular asíncrona* (Xenakis,
> Roads, Truax, Gabor). L'afirmació **no** és "un tipus nou de generació de so"; és que la
> resíntesi granular es pot plantejar com a **estimació Monte Carlo d'una integral de
> transport**, cosa que permet importar la *maquinària* de l'equació de renderitzat —
> importance sampling, estratificació, Russian roulette, transport recursiu — com a paradigma
> de control i qualitat. Això és el nou, i a diferència d'una metàfora, és **mesurable**.

---

## Fonament matemàtic

- **L'objectiu és una integral definida:** `target[n] = w[n] · ∫ p(τ)·s(τ+n) dτ` (un "blur"
  determinista del sample), l'anàleg del píxel `L(x) = ∫ f(x,ω) dω` de Kajiya.
- **Cada raig és un estimador insesgat:** `E[render_N] = target` i l'error cau com `1/√N`.
  *Més raigs → menys soroll → render més net* és ara literalment cert.
- **Reducció de variància:** *stratified*, *quasi-Monte Carlo* (golden ratio) i *importance*
  baten l'`1/√N` de manual.
- **Russian roulette** als rebots: la cua és la solució insesgada d'una sèrie de Neumann.

El **Convergence Lab** (ara també a la versió WASM, amb **export CSV**) ho demostra en viu:
dibuixa l'error RMS vs N en log-log amb la línia ideal `1/√N` i ofereix una A/B audible.

---

## Analogia: profunditat de camp

- **Obertura estreta** → enfocament nítid, coherent, gairebé afinat.
- **Obertura àmplia** → desenfocament profund, textura atmosfèrica densa: el drone.
- **Més raigs (N)** → menys soroll, convergència més suau.

---

## Paràmetres

| Paràmetre | Descripció |
|---|---|
| **Volum Master** | Nivell de sortida 0–100 % |
| **N (Nombre de Raigs)** | Densitat del render |
| **Obertura (Dispersió)** | Amplada del con temporal (ms). Estret = tonal, ample = textural |
| **Punt Focal** | Punt en el temps al voltant del qual es llancen els raigs |
| **Estratègia de Mostreig** | `Random` / `Stratified` / `Quasi-MC` / `Importance` |
| **Aberració Cromàtica** | Els greus obren més, els aguts menys → núvols greus i aguts cristal·lins |
| **Rugositat BRDF** | 100 % = drone difús; 0 % = especular / harmònic |
| **Rebots + Reflexió** | Cua per Russian roulette (sèrie de Neumann) |
| **Autoevolució (α/β/γ)** | L'energia FFT low/mid/high realimenta el cicle següent |

---

## La millora WASM (Rust + WebAssembly)

La pàgina [`wasm/`](wasm/) reimplementa el motor en **Rust compilat a WebAssembly** dins
d'un **AudioWorklet**, mesclant cada mostra. Soluciona per disseny els problemes de la versió
JS i afegeix:

- **Sense tope de veus ni pulsos**, **timing perfecte**, núvol continu i soft-clip integrat.
- Mostreig **Random / Stratified / Quasi-MC**, **aberració cromàtica**, **rebots (Russian
  roulette)** i **autoevolució recursiva**.
- **Estèreo (Width)** amb paneo equal-power, **octava / shimmer** i **pitch ±12 semitons**.
- **Gravació i export WAV** de la sortida del motor.
- **Convergence Lab amb export CSV** (dades reproduïbles).
- **Visual:** con de dispersió, **raigs acolorits per banda**, medidor de sortida i glow reactiu.
- **Compilació sense dependències** (només `rustc` + target `wasm32`, sense crates.io).
  Instruccions a **[wasm/README.md](wasm/README.md)**.

---

## Ús (clàssica)

1. Obre `rta.html` en qualsevol navegador modern.
2. Carrega un WAV o MP3.
3. Arrenca en **Mode Simple**: tria un preset, mou el knob **Caràcter (Tonal → Drone)**,
   ajusta el volum i dóna-li a play.
4. Prem **⚙ Mode avançat** per als controls òptics complets, el selector de mostreig i el
   **Convergence Lab**.
5. Grava un WAV des de la mateixa interfície i arrossega'l al teu DAW.

---

## Inspiració

Iannis Xenakis (música estocàstica) · James Kajiya (The Rendering Equation, 1986) ·
Fred Again (textures vocals) · Hans Zimmer (drones orquestrals) · Curtis Roads (microsound).

## Llicència

MIT — lliure d'usar, adaptar i construir-hi a sobre. S'agraeix el crèdit.

---
---

# Castellano

## Qué es RayDrone

RayDrone es una **metodología de renderizado** sonoro: en vez de sintetizar el audio desde
bloques o partículas, toma un buffer grabado y lo trata como una *escena temporal*. Cada
**rayo** es un grano corto extraído de una posición aleatoria dentro del cono de dispersión
alrededor de un punto focal. Todos los rayos suenan a la vez y su convergencia estadística
**es** el drone — no existe en el sample, **emerge**.

Mecánicamente es pariente cercano de la **síntesis granular asíncrona**, pero reinterpretada
con la lente del renderizado óptico: eso le da controles intuitivos (profundidad de campo,
apertura, foco) y una base matemática **medible**.

---

## Dos versiones

| Versión | Archivo | Motor | Estado |
|---|---|---|---|
| **WASM** (actual) | [`wasm/`](wasm/) → [`wasm/README.md`](wasm/README.md) | **Rust → WebAssembly + AudioWorklet** | ✅ **En desarrollo activo** |
| **Clásica** (legacy) | [`rta.html`](rta.html) | JavaScript vanilla + Web Audio | 🧊 **Congelada** — demo sin compilación, sin funciones nuevas |

Ambas implementan la misma idea. La versión **Rust/WebAssembly** mezcla **cada muestra** en
un bucle en el hilo de audio, lo que elimina por diseño los límites de la versión JS (sin
tope de voces, sin jitter, sin pulsos) y añade **estéreo** y **octava/shimmer**. Cómo
compilarla y usarla: **[wasm/README.md](wasm/README.md)**.

---

## Génesis: de un píxel a un sonido

Toda la idea empezó con una pregunta sobre gráficos: **¿cómo se define realmente un píxel
en ray tracing?**

Un píxel no es un color almacenado. En el renderizado basado en física es la solución de la
**ecuación de renderizado** de Kajiya — la luz que sale de un punto, una *integral* sobre
todas las direcciones de entrada:

```
L_o(x, ω_o) = L_e(x, ω_o) + ∫_Ω f_r(x, ω_i, ω_o) · L_i(x, ω_i) · (ω_i · n) dω_i
```

Esa integral no tiene forma cerrada, así que el renderer la **estima** por Monte Carlo:
lanza `N` rayos aleatorios, evalúa el integrando en cada uno y los promedia:

```
píxel ≈ (1/N) Σᵢ contribución(rayoᵢ)
```

El píxel **emerge** del promedio de N muestras estocásticas; más rayos → la estimación
converge → menos ruido.

Entonces llegó el giro que lo arrancó todo: **¿y si una muestra de audio se definiera
igual?** No almacenada, sino el resultado de una integral estimada con muestras
estocásticas. Cambia el hemisferio de direcciones por una *ventana de tiempo* alrededor de
un instante focal, los rayos de luz por *granos* y la radiancia por *amplitud*:

```
output[n] ≈ (1/N) Σᵢ s(τᵢ + n) · w[n]      (τᵢ tomado de la dispersión alrededor del foco)
```

Misma forma. Mismo estimador Monte Carlo. Misma convergencia `1/√N`. El **drone es el píxel
de audio**: no existe en el sample — emerge cuando lo renderizas. Ese paralelismo entre la
fórmula del píxel y la de la muestra *es* todo el proyecto.

---

## La teoría

En el renderizado 3D, el ray tracing lanza **N rayos estocásticos** desde una cámara. El
píxel final no se guarda: **emerge** de la convergencia de todos los rayos. Más rayos →
menos ruido → render más limpio. *La imagen no existe hasta que la renderizas.*

| Ray tracing óptico | RayDrone |
|---|---|
| Geometría de la escena 3D | Buffer de audio (dominio temporal) |
| Punto focal de la cámara | Posición del cabezal (segundos) |
| Apertura / profundidad de campo | Dispersión temporal (milisegundos) |
| N rayos estocásticos | N granos disparados a offsets aleatorios |
| Convergencia de luminancia | Emergencia del drone |

---

## Por qué es diferente de la síntesis granular

> **Marco honesto.** El motor es pariente cercano de la *síntesis granular asíncrona*
> (Xenakis, Roads, Truax, Gabor). La afirmación **no** es "un tipo nuevo de generación de
> sonido"; es que la resíntesis granular se puede plantear como **estimación Monte Carlo de
> una integral de transporte**, lo que permite importar la *maquinaria* de la ecuación de
> renderizado — importance sampling, estratificación, Russian roulette, transporte recursivo
> — como paradigma de control y calidad. Eso es lo nuevo, y a diferencia de una metáfora, es
> **medible** (ver el Convergence Lab).

---

## Fundamento matemático

- **El objetivo es una integral definida:** `target[n] = w[n] · ∫ p(τ)·s(τ+n) dτ` (un "blur"
  determinista del sample), el análogo del píxel `L(x) = ∫ f(x,ω) dω` de Kajiya.
- **Cada rayo es un estimador insesgado:** `E[render_N] = target` y el error cae como `1/√N`.
  *Más rayos → menos ruido → render más limpio* es ahora literalmente cierto y falsable.
- **Reducción de varianza:** *stratified*, *quasi-Monte Carlo* (golden ratio) e *importance*
  baten el `1/√N` de manual.
- **Russian roulette** en los rebotes: la cola es la solución insesgada de una serie de
  Neumann (la forma recursiva de la ecuación de transporte).

El **Convergence Lab** (ahora también en la versión WASM, con **export CSV**) lo demuestra
en vivo: dibuja el error RMS vs N en log-log con la línea ideal `1/√N` y ofrece una A/B
audible (objetivo vs N=4 vs N=256 al mismo nivel).

---

## Analogía: profundidad de campo

- **Apertura estrecha** → enfoque nítido, coherente, casi afinado.
- **Apertura amplia** → desenfoque profundo, textura atmosférica densa: el drone.
- **Más rayos (N)** → menos ruido, convergencia más suave.

---

## Parámetros

| Parámetro | Descripción |
|---|---|
| **Volumen Master** | Nivel de salida 0–100 % |
| **N (Número de Rayos)** | Densidad del render |
| **Apertura (Dispersión)** | Ancho del cono temporal (ms). Estrecho = tonal, ancho = textural |
| **Punto Focal** | Punto en el tiempo alrededor del cual se lanzan los rayos |
| **Estrategia de Muestreo** | `Random` / `Stratified` / `Quasi-MC` / `Importance` |
| **Aberración Cromática** | Los graves abren más, los agudos menos → nubes graves y agudos cristalinos |
| **Rugosidad BRDF** | 100 % = drone difuso; 0 % = especular / armónico |
| **Rebotes + Reflexión** | Cola por Russian roulette (serie de Neumann) |
| **Autoevolución (α/β/γ)** | La energía FFT low/mid/high realimenta el siguiente ciclo |

---

## La mejora WASM (Rust + WebAssembly)

La página [`wasm/`](wasm/) reimplementa el motor en **Rust compilado a WebAssembly** dentro
de un **AudioWorklet**, mezclando cada muestra en un bucle. Soluciona por diseño los
problemas de la versión JS y añade:

- **Sin tope de voces ni pulsos por inanición**, **timing perfecto** (sin `setTimeout`),
  nube continua y **soft-clip** integrado.
- Muestreo **Random / Stratified / Quasi-MC**, **aberración cromática** (apertura por banda
  + filtro), **rebotes (Russian roulette)** y **autoevolución recursiva** (la envolvente de
  la salida realimenta foco y apertura).
- **Estéreo (Width)** con paneo equal-power, **octava / shimmer** y **pitch ±12 semitonos**.
- **Grabación y export WAV** de la salida del motor (16-bit estéreo, directo al DAW).
- **Convergence Lab con export CSV** — datos de convergencia reproducibles desde el navegador.
- **Visual:** cono de dispersión, **rayos coloreados por banda** (grave/medio/agudo),
  medidor de salida y glow reactivo al nivel.
- **Compilación sin dependencias:** sin `wasm-bindgen` ni crates → no necesita crates.io,
  solo `rustc` + el target `wasm32-unknown-unknown`. Instrucciones en
  **[wasm/README.md](wasm/README.md)**.

---

## Uso (clásica)

La interfaz arranca en **Modo Simple**: cargar audio, elegir un preset, un único knob
**Carácter (Tonal → Drone)** que mueve densidad, apertura y rugosidad a la vez, volumen y
play. El botón **⚙ Modo avanzado** despliega los controles ópticos completos, el selector de
muestreo y el Convergence Lab.

1. Carga un WAV o MP3.
2. Elige un preset, o mueve el knob **Carácter** de tonal a drone.
3. Pulsa **Play** para escuchar el motor; **Original (Dry)** para comparar con el sample.
4. ¿Más control? **Modo avanzado**: foco, barrido, selección con ratón, aberración, rebotes,
   estrategias de muestreo y autoevolución (α/β/γ).
5. Si te gusta, graba un WAV desde la interfaz y arrástralo a tu DAW.

---

## Inspiración

Iannis Xenakis (música estocástica) · James Kajiya (The Rendering Equation, 1986) ·
Fred Again (texturas vocales) · Hans Zimmer (drones orquestales) · Curtis Roads (microsound).

## Licencia

MIT — libre de usar, adaptar y construir sobre ello. Se agradece el crédito.

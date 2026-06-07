# Acoustic Raytracer

> **A new methodology for audio rendering: applying the stochastic ray tracing paradigm to time-domain sampling.**

---

## 🌐 Language / Idioma

- [English](#english)
- [Català](#català)
- [Castellano](#castellano)

---

---

# English

## What is Acoustic Raytracing?

Acoustic Raytracing is a **new methodology** for generating drone textures and sustained soundscapes by borrowing the mathematical framework of optical ray tracing and applying it to the time domain of an audio buffer.

This is **not** granular synthesis. The conceptual framework is fundamentally different.

---

## The Theory

### Ray Tracing in Graphics (the origin)

In 3D rendering, ray tracing works by casting **N stochastic rays** from a camera through a scene. Each ray samples a different path, a different light interaction. The final pixel is not stored anywhere — it **emerges** from the statistical convergence of all those rays. More rays → less noise → cleaner render.

The key insight: **the image does not exist until you render it.**

---

### Transposing the Paradigm to Audio

Acoustic Raytracing applies the same logic to the time axis of a recorded audio buffer:

| Optical Ray Tracing | Acoustic Raytracing |
|---|---|
| 3D scene geometry | Audio buffer (time-domain waveform) |
| Camera focal point | Playhead position (seconds) |
| Aperture / depth of field | Temporal dispersion (milliseconds) |
| N stochastic rays | N audio grains fired at random offsets |
| Pixel luminance convergence | Drone / sustained texture emergence |
| Render output | Acoustic field |

Each **ray** is a short audio fragment extracted from a random position within the dispersion cone around the focal point. All N rays play simultaneously. Their amplitudes are normalized by `1/√N` to prevent clipping regardless of ray count.

The **drone does not exist in the sample.** It emerges from the stochastic convergence of the rays, exactly as a rendered image emerges from the convergence of light rays.

---

## Why This Is Different from Granular Synthesis

| Granular Synthesis | Acoustic Raytracing |
|---|---|
| Synthesis technique | Rendering methodology |
| Grains are building blocks | Rays are measurement samples |
| Result is constructed | Result converges |
| Rooted in time-frequency theory | Rooted in stochastic geometry |
| Controls: grain size, density, pitch | Controls: N (rays), aperture, focal point |
| Mental model: particles | Mental model: optics / physics of light |

The distinction is not merely semantic. The mental model determines how you **compose** with the instrument. A photographer controlling depth of field thinks differently from a synthesis programmer controlling grain density — even if some results can sound similar on the surface.

> **Honest framing.** Mechanically, the engine is a close relative of *asynchronous granular synthesis* (Xenakis, Roads, Truax, Gabor). We don't hide that — we **reinterpret** it. The claim here is not "a brand-new kind of sound generation"; it is that granular resynthesis can be cast as **Monte Carlo estimation of a transport integral**, which lets us import the *machinery* of the rendering equation — importance sampling, stratification, Russian roulette, recursive transport — as a control-and-quality paradigm. That reframing is what is new, and unlike a metaphor, it is **measurable**. See the Convergence Lab.

---

## Mathematical Foundation (the theory, for real)

The optical metaphor only earns the name "ray tracing" if it has the one property that defines Monte Carlo rendering: **a fixed ground truth that the samples converge toward, with quantifiable variance.** Here it does.

### The render target is a defined integral

Given a source buffer `s`, a focal point `τ₀`, and an aperture described by a sampling density `p(τ)` (triangular, peaking at the focus), define the rendered grain as the **deterministic** signal:

```
target[n] = w[n] · ∫ p(τ) · s(τ + n) dτ        (w = window)
```

This is exactly a windowed blur of the source by the aperture kernel — a real, computable signal. It is the audio analogue of the pixel value `L(x) = ∫ f(x,ω) dω` in Kajiya's rendering equation.

### Each ray is an unbiased estimator

Drawing `N` offsets `τᵢ ~ p`, the Monte Carlo estimator

```
render_N[n] = w[n] · (1/N) Σᵢ s(τᵢ + n)
```

is **unbiased**: `E[render_N] = target`. The error falls as

```
RMS(render_N − target) ∝ 1/√N
```

This is the law the README always claimed ("more rays → less noise → cleaner render"). It is now literally true and **falsifiable** — and the Convergence Lab plots it.

### Variance reduction (straight from the renderer)

Plain random sampling gives the textbook `1/√N`. Three techniques from rendering do measurably better — verified live by the Convergence Lab (measured log-log exponents on a test signal in parentheses):

- **Stratified sampling** (≈ −0.75) — one sample per stratum of the aperture's CDF, like pixel supersampling. Lower variance at the same `N`, and never worse.
- **Quasi-Monte Carlo** (≈ −0.73) — a golden-ratio low-discrepancy sequence with a per-burst Cranley-Patterson rotation. Robust: it doesn't depend on the source content. (On smooth integrands it tends toward `1/N`; broadband audio noise caps the achievable rate.)
- **Importance sampling** (≈ −0.75) — draw `τ ∝ p(τ)·energy(τ)` and reweight by `p/q` (self-normalized), with the importance CDF sampled *stratified* — the correct renderer recipe. Wins most where the source energy is uneven.

> **A real, documented finding.** The first importance implementation was *worse* than random: it measured energy over only the first eighth of each grain and drew the CDF with plain random numbers. The Lab caught it (the yellow curve sat above red). Measuring energy over the full grain and stratifying the CDF draws fixed it. This is the methodology working as intended — the experiment falsified a bad estimator instead of marketing it.

- **Russian roulette** — secondary "bounce" rays survive with probability = reflection coefficient at constant gain, instead of a fixed, attenuated echo count. Expected tail energy `= vol · Σ rᵈ`, so the bounce tail is the unbiased solution of a Neumann series — i.e. the *recursive* form of the transport equation, not an ad-hoc delay.

### How to prove it isn't hype

The **Convergence Lab** panel (below the scene) runs the experiment live:

1. Computes the exact deterministic `target` for the current focus/aperture.
2. Renders Monte Carlo estimates across `N = 1 … 1024` for all four sampling strategies.
3. Plots **RMS error vs N** on a log–log axis, overlaid with the ideal `1/√N` slope, and fits the measured exponent (expect ≈ −0.5).
4. **A/B audio**: play the `target` (reference) vs `N=4` (noisy) vs `N=256` (clean), all normalized to the *same level* so you hear the **noise floor falling**, not a volume change.

If the random curve tracks `1/√N` and stratified/importance sit below it, the theory holds — by measurement, not by adjective.

---

## Acoustic Analogy: Depth of Field

Think of a photographic lens:

- **Narrow aperture, short focal distance** → sharp focus, few overlapping planes → coherent tonal result, almost pitched
- **Wide aperture, long focal distance** → deep blur, many planes collapsing → dense atmospheric texture, the "drone"
- **More rays (N)** → less stochastic noise, smoother convergence → cleaner render

This is exactly how Hans Zimmer's brass drones or Fred Again's vocal textures behave: a voice or instrument ceases to be a recognizable sample and becomes an **acoustic field**.

---

## Visual Rendering

The interface renders the ray tracing process in real time across two canvases:

### SOURCE Canvas (top)
- The original waveform of the loaded audio file
- A **focal line** (vertical, glowing) showing the current playhead focus
- A **dispersion zone** (semi-transparent gradient) showing the aperture cone
- **Animated rays**: thin lines appearing at their random offset positions and converging toward the focal point, fading as they decay
- Time axis markers (seconds)

### RENDER OUTPUT Canvas (bottom)
- The **emergent waveform** in real time, read directly from the Web Audio AnalyserNode
- Shows the acoustic result of ray convergence — the rendered drone

### Metering
- Real-time RMS and Peak meters with dB readouts
- Red **CLIP** indicator if the output reaches 0 dBFS
- Compact **FFT Feedback** panel showing live **Low / Mid / High** band energy used by the recursive modulation system

---

## Parameters

### Core

| Parameter | Description |
|---|---|
| **Master Volume** | Global output level 0–100 % |
| **N (Number of Rays)** | Density of the render. More rays → smoother, richer drone. Exponential scale: `floor(slider^1.6)` |
| **Aperture (Dispersion)** | Width of the temporal cone in milliseconds. Up to 10 s. Narrow = coherent/tonal. Wide = diffuse/textural. |
| **Focal Point (Position)** | The point in time around which rays are cast. Moving this across the waveform changes the timbral character entirely. |

### Advanced Optical Extensions

| Parameter | Optical Analogy | Acoustic Effect |
|---|---|---|
| **Sampling Strategy** | Monte Carlo estimator | `Random` = plain MC (error ~ 1/√N). `Stratified` = one sample per CDF stratum. `Quasi-MC` = golden-ratio low-discrepancy sequence. `Importance` = rays concentrate where the source has energy. The last three converge faster than 1/√N. |
| **Chromatic Aberration** | Lens dispersion by wavelength | Low frequencies get a wider aperture; highs get a narrow aperture. Big low-end clouds and crystalline high-end detail. |
| **Autoevolution** | Recursive feedback loop | Reads the previous FFT frame and feeds spectral energy back into the engine so the render can self-modulate over time. |
| **α Dispersion** | High-band spectral gain | More high-frequency energy widens the aperture on the next cycle. |
| **β Bounces** | Mid-band structural density | More mid energy increases secondary ray generation. |
| **γ Roughness** | Low-band diffusion driver | More low-frequency energy increases BRDF roughness and cloudiness. |
| **BRDF Roughness** | Surface micro-geometry | 100 % = diffuse Monte Carlo drone. 0 % = specular / harmonic placement. |
| **Bounce Count** | Recursive transport depth | Safety cap on secondary-ray recursion. Each ray spawns a child via **Russian roulette**, building the Neumann-series tail. |
| **Reflection Coefficient** | Path survival probability | Probability a bounce survives (and, in expectation, the tail energy `Σ rᵈ` and its length). |

---

## Technical Implementation

Built entirely with **vanilla JavaScript** and the **Web Audio API**. No libraries, no frameworks.

```
AudioBufferSourceNode (×burst)
        ↓
  GainNode (fade envelope per ray)
        ├──────────────→ dryBus ─────────────┐  (full range)
        ↓                                     │
  Linkwitz-Riley band (LR4: low / mid / high) │
        ↓                                     │
  band gain (1/√P compensation) → wetBus ─────┤
                                              ↓
                                  mainGainNode (master volume)
                                              ↓
                          AnalyserNode (FFT 2048, visual + metering)
                                              ↓
                                  AudioContext.destination
```

**Scheduling — lookahead clock.** Instead of firing bursts straight from `setTimeout`, a lookahead scheduler (Chris Wilson's *A Tale of Two Clocks*) wakes every ~25 ms and schedules every burst that falls inside a ~120 ms window with **exact start times on `audioCtx.currentTime`**. Timing no longer depends on `setTimeout` jitter and doesn't clump or stall when the tab is backgrounded.

**Chromatic aberration — continuous crossover.** The aberration routes each ray through a 3-band **Linkwitz-Riley (LR4)** crossover (250 Hz / 2500 Hz), with per-band `1/√P` gain so the bands reconstruct ~flat (±3 dB, only −3 dB dips at the crossovers — no spectral holes). A **dry/wet bus** crossfades from fully dry at aberration 0, so the effect grows smoothly from zero instead of the old hard 0→1 % jump.

Each burst:
1. Read focal point, aperture and all optical parameters from sliders
2. Calculate `burstN = min(rawN, burstCap, headroom)`
3. Assign a frequency band per ray (low / mid / high) and scale the aperture per band (**Chromatic Aberration**)
4. Draw the diffuse offset with the chosen **sampling strategy** (random / stratified / quasi-MC / importance)
5. Apply **BRDF Roughness** by interpolating between harmonic and stochastic placement
6. Spawn `BufferSourceNode`s with fade envelopes at their scheduled audio-clock time
7. On each ray end, **Russian roulette** may spawn a child ray (the transport tail)
8. Push visual rays to `visRays[]` and animate them with `requestAnimationFrame`
9. Update RMS / Peak meters and the clip indicator in real time
10. If **Autoevolution** is enabled, smooth low / mid / high FFT energies and apply them to the next engine cycle through `α`, `β`, and `γ`

---

## Inspired By

- **Iannis Xenakis** — stochastic music theory
- **James Kajiya** — The Rendering Equation (1986)
- **Fred Again** — vocal texture processing
- **Hans Zimmer** — sustained orchestral drones
- **Curtis Roads** — microsound and granular theory (as a contrast reference)

---

## Usage

1. Open `rta.html` in any modern browser
2. Load a WAV or MP3 file
3. Adjust the **Focal Point** to a region of the waveform you find interesting
4. Set a narrow **Aperture** and low **N** first — listen to the coherent result
5. Gradually open the aperture and increase N — hear the drone emerge
6. Watch the SOURCE canvas: the rays converge visually as they converge acoustically
7. Enable **Autoevolution** and raise `α`, `β`, or `γ` carefully to move from slow breathing motion into unstable attractors

### Sound Character Presets

| Sound Goal | N | Aperture | Aberration | Roughness | Bounces |
|---|---|---|---|---|---|
| Tonal / Pitched | low | narrow | 0 % | 0 % | 0 |
| Dense Drone | high | wide | 0 % | 100 % | 0 |
| Zimmer Brass | high | 2–5 s | 60 % | 80 % | 1–2 |
| Fred Again Vocal | mid | 500 ms–2 s | 30 % | 100 % | 0 |
| Infinite Shimmer | mid | wide | 40 % | 60 % | 4 + high reflection |

---

## License

MIT — free to use, adapt, and build upon. Credit appreciated.

---

---

# Català

## Què és el Rastreig Acústic de Raigs?

El Rastreig Acústic de Raigs és una **nova metodologia** per generar textures de drone i paisatges sonors sostinguts, prenent prestada l'estructura matemàtica del rastreig de raigs òptic i aplicant-la al domini temporal d'un buffer d'àudio.

Això **no** és síntesi granular. El marc conceptual és fonamentalment diferent.

---

## La Teoria

### Ray Tracing en Gràfics (l'origen)

En el renderitzat 3D, el ray tracing funciona llançant **N raigs estocàstics** des d'una càmera a través d'una escena. Cada raig mostra un camí diferent, una interacció de llum diferent. El píxel final no està emmagatzemat en cap lloc — **emergeix** de la convergència estadística de tots aquells raigs. Més raigs → menys soroll → renderitzat més net.

La idea clau: **la imatge no existeix fins que no la renderitzes.**

---

### Transposant el Paradigma a l'Àudio

El Rastreig Acústic de Raigs aplica la mateixa lògica a l'eix temporal d'un buffer d'àudio gravat:

| Ray Tracing Òptic | Rastreig Acústic de Raigs |
|---|---|
| Geometria de l'escena 3D | Buffer d'àudio (forma d'ona en domini temporal) |
| Punt focal de la càmera | Posició del capçal de reproducció (segons) |
| Obertura / profunditat de camp | Dispersió temporal (mil·lisegons) |
| N raigs estocàstics | N grans d'àudio disparats a offsets aleatoris |
| Convergència de luminància del píxel | Emergència del drone / textura sostinguda |
| Sortida del renderitzat | Camp acústic |

Cada **raig** és un fragment d'àudio curt extret d'una posició aleatòria dins del con de dispersió al voltant del punt focal. Tots els N raigs sonen simultàniament. Les seves amplituds es normalitzen per `1/√N` per evitar la saturació independentment del nombre de raigs.

El **drone no existeix a la mostra.** Emergeix de la convergència estocàstica dels raigs, exactament com una imatge renderitzada emergeix de la convergència de raigs de llum.

---

## Per Què Això és Diferent de la Síntesi Granular

| Síntesi Granular | Rastreig Acústic de Raigs |
|---|---|
| Tècnica de síntesi | Metodologia de renderitzat |
| Els grans són blocs constructius | Els raigs són mostres de mesurament |
| El resultat es construeix | El resultat convergeix |
| Arrelada en la teoria temps-freqüència | Arrelada en la geometria estocàstica |
| Controls: mida del gra, densitat, to | Controls: N (raigs), obertura, punt focal |
| Model mental: partícules | Model mental: òptica / física de la llum |

La distinció no és merament semàntica. El model mental determina com **composes** amb l'instrument. Un fotògraf que controla la profunditat de camp pensa diferent d'un programador de síntesi que controla la densitat granular — fins i tot si alguns resultats poden sonar similars en la superfície.

---

## Analogia Acústica: Profunditat de Camp

Pensa en un objectiu fotogràfic:

- **Obertura estreta, distància focal curta** → enfocament nítid, poques plans superposats → resultat tonal coherent, gairebé afinat
- **Obertura àmplia, distància focal llarga** → desenfocament profund, molts plans que col·lapsen → textura atmosfèrica densa, el "drone"
- **Més raigs (N)** → menys soroll estocàstic, convergència més suau → renderitzat més net

Exactament així es comporten els drones de metalls de Hans Zimmer o les textures vocals de Fred Again: una veu o instrument deixa de ser una mostra reconeixible i es converteix en un **camp acústic**.

---

## Renderitzat Visual

La interfície renderitza el procés de rastreig de raigs en temps real en dos canvas:

### Canvas SOURCE (superior)
- La forma d'ona original del fitxer d'àudio carregat
- Una **línia focal** (vertical, amb resplendor) que mostra l'enfocament actual del capçal de reproducció
- Una **zona de dispersió** (gradient semitransparent) que mostra el con d'obertura
- **Raigs animats**: línies primes que apareixen a les seves posicions d'offset aleatòries i convergeixen cap al punt focal, esvaïnt-se mentre decauen
- Marcadors de l'eix temporal (en segons)

### Canvas RENDER OUTPUT (inferior)
- La **forma d'ona emergent** en temps real, llegida directament de l'`AnalyserNode` de Web Audio
- Mostra el resultat acústic de la convergència de raigs — el drone renderitzat

### Metreig
- Mesuradors RMS i Peak en temps real amb lectura en dB
- Indicador **CLIP** en vermell si la sortida arriba a 0 dBFS
- Panell compacte de **FFT Feedback** amb energia en viu de **Low / Mid / High** usada pel sistema de modulació recursiva

---

## Paràmetres

### Nucli

| Paràmetre | Descripció |
|---|---|
| **Volum Master** | Nivell de sortida global 0–100 % |
| **N (Nombre de Raigs)** | Densitat del renderitzat. Més raigs → drone més ric i suau. Escala exponencial: `floor(slider^1.6)` |
| **Obertura (Dispersió)** | Amplada del con temporal en mil·lisegons. Fins a 10 s. Estret = coherent/tonal. Ample = difús/textural. |
| **Punt Focal (Posició)** | El punt en el temps al voltant del qual es llancen els raigs. Moure'l per la forma d'ona canvia completament el caràcter tímbric. |

### Extensions Òptiques Avançades

| Paràmetre | Analogia òptica | Efecte acústic |
|---|---|---|
| **Aberració Cromàtica** | Dispersió de la lent per longitud d'ona | Els greus obren més; els aguts obren menys. Núvols de baix profunds i aguts cristal·lins. |
| **Autoevolució** | Bucle de feedback recursiu | Llegeix el frame FFT anterior i retorna l'energia espectral al motor perquè el render es moduli a si mateix amb el temps. |
| **α Dispersió** | Guany espectral dels aguts | Més energia d'altes freqüències obre més l'obertura al cicle següent. |
| **β Rebotes** | Densitat estructural dels mitjos | Més energia en mitjos incrementa la generació de rebots secundaris. |
| **γ Rugositat** | Motor de difusió dels greus | Més energia de greus incrementa la rugositat BRDF i la nebulosa sonora. |
| **Rugositat BRDF** | Microgeometria de la superfície | 100 % = drone difús Monte Carlo. 0 % = especular / harmònic. |
| **Rebotes (Bounce)** | Generació de raigs secundaris | Cada raig pot generar raigs fills des del seu final, creant una cua geomètrica. |
| **Coeficient de Reflexió** | Pèrdua d'energia per rebote | Controla el decaïment de cada rebot i la longitud de la cua. |

---

## Implementació Tècnica

Construït íntegrament amb **JavaScript vanilla** i la **Web Audio API**. Sense llibreries, sense frameworks.

```
AudioBufferSourceNode (×burst)
        ↓
  GainNode (envoltant de fade per raig)
        ↓
  [Opcional] BiquadFilter de banda (low / mid / high)
        ↓
  mainGainNode (volum mestre)
        ↓
  AnalyserNode (FFT 2048, per a la sortida visual + metering)
        ↓
  Estat de feedback (energia suavitzada low / mid / high)
        ↓
  AudioContext.destination
```

Cada cicle de renderitzat (`setTimeout` adaptatiu a l'interval de dispersió):
1. Llegir el punt focal, l'obertura i tots els paràmetres òptics dels sliders
2. Calcular `burstN = min(rawN, burstCap, headroom)`
3. Assignar una banda de freqüència per raig (greus / migs / aguts)
4. Aplicar **Aberració Cromàtica** escalant l'obertura per banda
5. Aplicar **Rugositat BRDF** interpolant entre posició harmònica i aleatòria
6. Crear `BufferSourceNode`s amb envoltant de fade
7. Si **Bounce Count** és actiu, crear raigs fills des del final de cada raig
8. Afegir raigs visuals a `visRays[]` i animar-los amb `requestAnimationFrame`
9. Actualitzar metres RMS / Peak i l'indicador de clip en temps real
10. Si **Autoevolució** està activa, suavitzar les energies FFT low / mid / high i aplicar-les al següent cicle del motor mitjançant `α`, `β` i `γ`

---

## Inspiració

- **Iannis Xenakis** — teoria de la música estocàstica
- **James Kajiya** — The Rendering Equation (1986)
- **Fred Again** — processament de textures vocals
- **Hans Zimmer** — drones orquestrals sostinguts
- **Curtis Roads** — microsound i teoria granular (com a referència de contrast)

---

## Ús

1. Obre `rta.html` a qualsevol navegador modern
2. Carrega un fitxer WAV o MP3
3. Ajusta el **Punt Focal** a una regió de la forma d'ona que et sembli interessant
4. Estableix primer una **Obertura** estreta i una **N** baixa — escolta el resultat coherent
5. Obre gradualment l'obertura i augmenta N — escolta com emergeix el drone
6. Observa el canvas SOURCE: els raigs convergeixen visualment mentre convergeixen acústicament
7. Activa **Autoevolució** i puja `α`, `β` o `γ` amb cura per passar d'un moviment lent a atractors més inestables

### Presets per Caràcter

| Objectiu Sonor | N | Obertura | Aberració | Rugositat | Rebotes |
|---|---|---|---|---|---|
| Tonal / Afinat | baix | estreta | 0 % | 0 % | 0 |
| Drone Dens | alt | ampla | 0 % | 100 % | 0 |
| Metalls Zimmer | alt | 2–5 s | 60 % | 80 % | 1–2 |
| Vocal Fred Again | mig | 500 ms–2 s | 30 % | 100 % | 0 |
| Brillantor Infinita | mig | ampla | 40 % | 60 % | 4 + reflexió alta |

---

## Llicència

MIT — lliure d'usar, adaptar i construir sobre seu. S'agraeix el crèdit.

---

---

# Castellano

## Qué es Acoustic Raytracing

Acoustic Raytracing es una **metodología de renderizado sonoro**: en vez de sintetizar el audio desde bloques o partículas, toma un buffer grabado y lo trata como una escena temporal. Los rayos no “representan” el sonido; lo **convergen**.

La idea no es reproducir el sample de forma fiel. La idea es **renderizarlo como un campo acústico** que puede convertirse en drone, textura, coro granular o nube armónica.

---

## Extensiones Ópticas

- **Aberración cromática**: los graves reciben una apertura más amplia; los agudos una apertura mucho más estrecha.
- **Autoevolución**: un bucle recursivo lee el frame FFT anterior y lo usa para modular el siguiente estado del motor.
- **α dispersión**: los agudos abren más la lente temporal en el siguiente ciclo.
- **β rebotes**: los medios aumentan la densidad de rebotes secundarios.
- **γ rugosidad**: los graves empujan el render hacia una difusión más nubosa.
- **BRDF acústica / rugosidad**: controla si los rayos se distribuyen de forma difusa o más armónica.
- **Bounce count**: cada rayo puede generar rebotes secundarios y construir una cola geométrica de resonancia.
- **Barrido del sampler**: el foco puede recorrer todo el audio automáticamente en modo ping-pong o quedarse fijo.

---

## Interacción

- Puedes hacer click sobre la onda para saltar a un punto concreto.
- Puedes arrastrar sobre la onda para seleccionar una ventana temporal.
- Puedes oír el **original** sin parar el render y parar el **render** sin cortar el original.
- Los medidores RMS y Peak están arriba, de forma sutil, para comprobar nivel y clipping.
- El panel **FFT Feedback** muestra en vivo la energía **Low / Mid / High** que alimenta la autoevolución.

---

## Presets

La interfaz incluye presets rápidos para empezar sin ajustar todo a mano:

- **Tonal / Pitched**
- **Dense Drone**
- **Zimmer Brass**
- **Fred Again Vocal**
- **Infinite Shimmer**

---

## Uso

1. Carga un WAV o MP3.
2. Elige un preset o ajusta los controles manualmente.
3. Usa el foco, el barrido o la selección con mouse para definir qué parte del sampler se renderiza.
4. Pulsa **Renderizar Drone** para escuchar el motor.
5. Pulsa **Oír Original** para comparar el sample sin parar el render.
6. Activa **Autoevolución** y sube `α`, `β` y `γ` poco a poco para entrar en zonas de respiración, densificación o caos controlado.
7. Si te gusta el resultado, graba WAV desde la propia interfaz y arrástralo a tu DAW.


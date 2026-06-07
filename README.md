# Acoustic Raytracer

> **A new methodology for audio rendering: applying the stochastic ray tracing paradigm to time-domain sampling.**

---

## 🌐 Language / Idioma

- [English](#english)
- [Català](#català)

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

---

## Parameters

| Parameter | Description |
|---|---|
| **N (Number of Rays)** | Density of the render. More rays → smoother, richer drone. Exponential scale: `floor(slider^1.6)` |
| **Aperture (Dispersion)** | Width of the temporal cone in milliseconds. Narrow = coherent/tonal. Wide = diffuse/textural. |
| **Focal Point (Position)** | The point in time around which rays are cast. Moving this across the waveform changes the timbral character entirely. |

---

## Technical Implementation

Built entirely with **vanilla JavaScript** and the **Web Audio API**. No libraries, no frameworks.

```
AudioBufferSourceNode (×N)
        ↓
  GainNode (fade envelope per ray)
        ↓
  mainGainNode (master volume, 1/√N normalized)
        ↓
  AnalyserNode (FFT 2048, for visual output)
        ↓
  AudioContext.destination
```

Each render cycle (`setInterval` at dispersion interval ms):
1. Read current focal point and aperture from sliders
2. Fire N `BufferSourceNode`s, each with a random offset within `[focal - disp, focal + disp]`
3. Apply linear fade-in/fade-out envelope per ray
4. Push visual ray to `visRays[]` array for canvas animation
5. `requestAnimationFrame` loop decays each `visRay.life` and redraws both canvases

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

---

## Paràmetres

| Paràmetre | Descripció |
|---|---|
| **N (Nombre de Raigs)** | Densitat del renderitzat. Més raigs → drone més ric i suau. Escala exponencial: `floor(slider^1.6)` |
| **Obertura (Dispersió)** | Amplada del con temporal en mil·lisegons. Estret = coherent/tonal. Ample = difús/textural. |
| **Punt Focal (Posició)** | El punt en el temps al voltant del qual es llancen els raigs. Moure'l per la forma d'ona canvia completament el caràcter tímbric. |

---

## Implementació Tècnica

Construït íntegrament amb **JavaScript vanilla** i la **Web Audio API**. Sense llibreries, sense frameworks.

```
AudioBufferSourceNode (×N)
        ↓
  GainNode (envoltant de fade per raig)
        ↓
  mainGainNode (volum mestre, normalitzat per 1/√N)
        ↓
  AnalyserNode (FFT 2048, per a la sortida visual)
        ↓
  AudioContext.destination
```

Cada cicle de renderitzat (`setInterval` a l'interval de dispersió en ms):
1. Llegir el punt focal actual i l'obertura dels sliders
2. Disparar N `BufferSourceNode`s, cadascun amb un offset aleatori dins de `[focal - disp, focal + disp]`
3. Aplicar envoltant de fade-in/fade-out lineal per raig
4. Afegir raig visual a l'array `visRays[]` per a l'animació del canvas
5. El bucle `requestAnimationFrame` decau cada `visRay.life` i redibuixa ambdós canvas

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

---

## Llicència

MIT — lliure d'usar, adaptar i construir sobre seu. S'agraeix el crèdit.

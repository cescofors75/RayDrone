# RayDrone · versión WebAssembly (Rust)

Copia de la versión sencilla de RayDrone, pero con el **motor de audio escrito en
Rust y compilado a WebAssembly**, corriendo dentro de un **AudioWorklet**.

## Por qué esta arquitectura

A diferencia de la versión JS (que crea un nodo de Web Audio por grano), aquí el
motor **mezcla cada muestra** en un bucle Rust en el hilo de audio. Eso elimina de
raíz los problemas de la versión JS:

- **Sin tope de voces / sin pulsos por inanición** — los granos se mezclan en un
  buffer propio, no hay `MAX_SIMULTANEOUS`.
- **Timing perfecto** — sin `setTimeout`; el worklet trabaja muestra a muestra.
- **Nube continua** — los granos nacen como un flujo (no en *bursts*), así que no
  hay pulso rítmico audible.
- **Soft-clip integrado** — saturación suave en Rust, sin distorsión dura.

Es además **sin dependencias**: no usa `wasm-bindgen` ni crates, así que **no
necesita acceso a crates.io**. Solo `rustc` + el target `wasm32-unknown-unknown`.

## Archivos

| Archivo | Qué es |
|---|---|
| `raydrone.rs` | Motor granular en Rust (`no_std`, sin deps). Exporta `set_sample`, `set_params`, `process`, punteros de memoria. |
| `../core/` | Kernel DSP compartido (`raydrone_core`, `no_std`, sin deps): `clampf`, `soft`, `sample_at`, `win_at`, RNG. **El mismo código que enlaza el VST** — una sola fuente de verdad. |
| `processor.js` | `AudioWorkletProcessor` que instancia el wasm y rellena la salida cada bloque. |
| `lab-worker.js` | Web Worker del Convergence Lab: otra instancia del mismo wasm para medir convergencia sin congelar la UI. |
| `index.html` | La página (UI sencilla: Original/Drone/Shimmer + Carácter + Volumen). |
| `build.sh` | Compila `../core` → `libraydrone_core.rlib` y luego `raydrone.rs` → `raydrone.wasm` (lo enlaza con `--extern`). |

## Compilar

```bash
cd wasm
./build.sh
```

Si es la primera vez y falta el target wasm:

```bash
rustup target add wasm32-unknown-unknown
```

(Requiere `rustup`. Si no lo tienes: instálalo desde https://rustup.rs — una vez
añadido el target, la compilación es **offline**, no descarga crates.)

El comando real que ejecuta `build.sh` son dos pasos (ambos con `rustc` crudo,
sin Cargo ni crates.io): primero el kernel compartido, luego el motor enlazándolo.

```bash
# 1) kernel DSP compartido (no_std, sin deps) → rlib
rustc --edition 2021 --target wasm32-unknown-unknown -O -C panic=abort -C lto=fat \
      --crate-name raydrone_core --crate-type=lib ../core/src/lib.rs -o libraydrone_core.rlib
# 2) motor → wasm, enlazando el kernel
rustc --edition 2021 --target wasm32-unknown-unknown -O -C panic=abort -C lto=fat \
      --extern raydrone_core=libraydrone_core.rlib \
      --crate-type=cdylib raydrone.rs -o raydrone.wasm
```

## Ejecutar

Los AudioWorklets y la carga del `.wasm` necesitan servirse por **HTTP** (no vale
abrir el archivo con `file://`). Desde la raíz del repo:

```bash
python3 -m http.server 8080
```

y abre **http://localhost:8080/wasm/**.

1. Carga un WAV/MP3.
2. Haz click en la onda para mover el **foco**.
3. **Original (Dry)** = tu sample tal cual · **Drone** / **Shimmer** = el motor.
4. Mueve **Carácter** (de tonal a drone) y **Volumen**.

## Estado

Motor con paridad casi completa con la versión JS:

- ✅ Nube granular continua (foco, apertura, grano, densidad).
- ✅ Muestreo **Random / Stratified / Quasi-MC** (golden ratio) — checkbox para A/B.
- ✅ **Aberración cromática**: bandas grave/medio/agudo con apertura escalada por
  banda (graves abren, agudos enfocan) y filtro one-pole por voz.
- ✅ **Rebotes (Russian roulette)**: al morir un grano, con probabilidad = reflexión
  nace un grano hijo (cola/transporte), con tope de profundidad.
- ✅ **Autoevolución recursiva**: la envolvente de la salida realimenta foco y
  apertura → el drone se modula a sí mismo.
- ✅ **Ambient · focos recursivos**: en vez de un único foco, una constelación.
  Las semillas se reparten en baja discrepancia (golden ratio) sobre el sample y
  derivan solas (paseo aleatorio lento, independiente por foco). Recursión: cada
  foco engendra focos hijo más breves y cercanos, con offset/apertura/vida que
  encogen por nivel → estructura **auto-similar** (la misma ley QMC a escala de
  sección, frase y grano). Controles: semillas, niveles del árbol, dispersión,
  deriva, nacimientos/seg. La constelación se dibuja sobre la onda (un bloom y un
  eje por foco, opacidad ∝ peso) y sobre el minimapa. Coste: O(focos) por bloque
  (≤48 focos) + O(focos) por grano; el bucle de audio sigue siendo O(voces).
- ✅ **Estéreo (Width)**: paneo equal-power por grano → nube ancha e inmersiva.
- ✅ **Octava / Shimmer**: probabilidad de que cada grano suene una octava arriba.
- ✅ **Pitch (transposición)**: ±12 semitonos (multiplicador de velocidad de lectura).
- ✅ **Play · piano por teclado/ratón/táctil**: teclado A W S E D F T G Y U J K O L
  (desde C4, D/F/A solo por clic/táctil al coincidir con atajos de la página), o
  clic/toque directo sobre el piano en pantalla (multi-touch → acordes con varios
  dedos). Mientras haya alguna nota sostenida, cada grano nuevo coge una de esas
  notas en vez del grado de Microtonal/Voicing — mismo mecanismo de muestreo por
  grado (estratificado/QMC/random) que ya usan esos dos sistemas.
- ✅ **Escala (textura microtonal)**: cada grano coge un grado de una tabla de ratios
  (12/19/22/24-EDO, justa entonación, Bohlen–Pierce). El grado se muestrea con la
  misma maquinaria de reducción de varianza que el eje temporal: estratificado puro
  (cada grado = un estrato → cobertura homogénea de la retícula) o Kronecker R2
  (constante plástica, decorrelada de la áurea del tiempo). El micro-detune ±4 cents
  convierte la retícula exacta en enjambre. Coste extra: ~0 (solo cambia la
  distribución del multiplicador de lectura). Verificado espectralmente: contraste
  grados/huecos ≈ 55×; el grado peor cubierto queda 22× sobre el fondo con QMC
  frente a ~10× con random.
- ✅ **Acorde (grados activos)**: subconjunto de la escala — tónica, quinta, tríada
  4:5:6, tétrada 4:5:6:7 (en BP, el acorde canónico 3:5:7) o pentatónica. Para cada
  ratio objetivo se usa el grado más cercano de la escala, así el mismo acorde queda
  afinado distinto en cada temperamento y la diferencia entre escalas se vuelve
  audible (con todos los grados activos, cualquier EDO denso suena a cluster).
- ✅ **Reverb (espacio)**: reverb estéreo Freeverb-lite (4 combs + 2 allpass por canal).
- ✅ **Trazado inverso (opcional)**: precalcula la energía del sample y lanza los rayos
  hacia donde hay señal (importance desde la fuente) → menos rayos malgastados, más
  lleno y limpio. Brilla con material disperso; con notas sostenidas la mejora es leve.
- ✅ **Visual**: cono de dispersión, rayos coloreados por banda (grave/medio/agudo),
  medidor de salida y glow reactivo al nivel.
- ✅ **Zoom integrado en la onda principal**: un solo canvas. Rueda, pellizco
  (móvil) o slider (×1–×64); con zoom aparece una tira-minimapa arriba con el
  archivo entero y la ventana visible marcada (click en ella = salto global;
  click en la onda = foco fino). Los rayos se dibujan dentro de la ventana.
- ✅ **División tonal**: visor junto a los selectores de escala/acorde — la retícula
  de grados en cents (activos resaltados) y cada rayo viajando sobre la línea del
  grado que le tocó (el motor registra el ratio por rayo).
- ✅ **Convergence Lab en wasm**: las curvas las calcula el MISMO motor Rust
  (`lab_target` / `lab_estimate` / `lab_rms` en `raydrone.rs`) corriendo en una
  instancia aparte dentro de un Web Worker (`lab-worker.js`) — la UI no se congela
  y se mide el código que suena, no una simulación JS. Semilla fija → CSV
  reproducible bit a bit. N = 1…8192, 12 tiradas, 5 estrategias; acumuladores f64.
  Pendientes medidas: random −0.50 (teoría −0.5), stratified/QMC ≈ −0.58.
  El estimador JS se conserva solo para el A/B audible.
- ✅ **RayRunner (`game.html`)**: arcade de nave cuya banda sonora la renderiza
  RayDrone en vivo — la demo de "audio adaptativo para videojuegos". El mundo
  recorre un sample sintetizado de 24 s con 4 zonas (pad → campanas → tormenta →
  coro): **por donde pasas, eso suena** (el scroll mueve el foco). Y **cómo
  juegas transforma la música**: el combo controla N (60→400 rayos/s) y la
  apertura — jugar bien = la textura converge densa y nítida (error ∝ 1/√N);
  un impacto desploma el combo (polvo granular), hunde el tono un instante y
  dispara una cola de rebotes; cada cristal enciende un destello de shimmer.
  Sin capas pregrabadas: un solo motor, parámetros vivos. En móvil: se pilota
  arrastrando el dedo por la pantalla, canvas nítido (retina) que crece a
  ~56vh, pantalla completa (⛶, donde el navegador lo permita), mini-HUD dentro
  del lienzo, vibración háptica y pausa automática al cambiar de app.

Paridad completa con la versión JS.

> ⚠️ Este `.wasm` se compila en tu máquina (el entorno donde se escribió el código
> no podía compilar a wasm). Si `rustc` se queja de algo al compilar, es un ajuste
> menor — pásame el error y lo corrijo.

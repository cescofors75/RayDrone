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
| `vendor/` | Three.js r185 vendorizado (motor 3D del Nivel 2 de RayRunner) + `three-addons/` (postprocesado: `EffectComposer`/`RenderPass`/`UnrealBloomPass`/`OutputPass`, extraídos del paquete oficial). Sin CDN, offline. El motor de audio sigue sin depender de nada de esto. |

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
- ✅ **RayRunner (`game.html`)**: arcade de **2 niveles** cuya banda sonora la
  renderiza RayDrone en vivo — la demo de "audio adaptativo para videojuegos".
  Nivel 1 (nave espacial, lateral) y Nivel 2 (**Circuito Neón**: un circuito
  cerrado **3D de verdad** — WebGL con **Three.js vendorizado** en
  `vendor/` (sin CDN, sigue funcionando offline; el motor de audio continúa
  sin dependencias) — trazado como spline Catmull-Rom cerrada con curvas,
  chicanes, un túnel con luces, cambios de rasante, asfalto y tierra
  (texturas procedurales por canvas), arco de meta, contador de vueltas,
  fuerza centrífuga desde la curvatura real de la spline, sol de rayas,
  skyline y pilones neón), cada nivel con su propio sample sintetizado de
  24 s y 4 zonas — el espacio (pad → campanas → tormenta → coro) y el
  circuito (amanecer → calipso → niebla → faro 2077, en tono mayor y
  alegre). En el circuito, **cada sector de pista es una zona musical**: una
  vuelta = una pasada entera por el sample (el túnel suena a niebla estática,
  el tramo de tierra a faro 2077). Y **cómo juegas transforma la música**: el combo
  controla N (60→400 rayos/s) y la apertura — jugar bien = la textura converge
  densa y nítida (error ∝ 1/√N); un impacto desploma el combo (polvo
  granular), hunde el tono un instante y dispara una cola de rebotes; cada
  cristal enciende un destello de shimmer. **Power-ups** en ambos niveles:
  🛡 escudo (absorbe un golpe — y la música ni se entera), 🚀 misil (X),
  💣 bomba (C), más un **bláster ilimitado** con cadencia que se dispara con
  **clic de ratón, Z/espacio o un toque corto**. **Enemigos fantasma** en los
  dos niveles: naves translúcidas que persiguen tu altura en el espacio y
  tres coches GT espectrales (material aditivo parpadeante) que dan vueltas
  solos al circuito — alcanzarlos es un golpe (si te adelantan ellos, te
  atraviesan: el barrido de colisión usa el mismo arco recorrido en el
  frame que las barreras, así un bajón de fps nunca "salta" el golpe),
  disiparlos con un arma da +80 y un destello shimmer, y reaparecen más
  adelante. El vehículo del jugador en el circuito es una **moto Tron**:
  lightcycle azul neón (casco bajo extruido, aleta dorsal, visor, piloto
  agachado, una sola vía de ruedas con llanta de disco y buje blanco,
  manillar, faro/piloto) con **doble muro de luz** trasero (de pie, no un
  ribbon plano) cuya longitud es la misma visualización de N que antes —
  **sombra propia** (luz direccional que sigue a la moto, mapa 512²
  ajustado) y **bloom** (Three.js `UnrealBloomPass`, resolución interna fija
  y barata para no penalizar el móvil) sobre el neón. **Conducción con
  inercia**: el mando/dedo acelera el carril (no lo teletransporta), la
  tierra agarra menos (acelera menos y frena más despacio → derrape), la
  curva empuja como una centrífuga real sobre la velocidad lateral (no
  sobre la posición) y el muro de la pista da un rebote suave en vez de un
  tope duro; la física corre en un **paso fijo con acumulador** (hasta 8
  subpasos por frame) para que un renderizado lento (móvil flojo) nunca
  meta el juego en cámara lenta — solo se pintan menos fotogramas.
  **Acelerador y freno de verdad** (↑/↓; en táctil el eje vertical del
  dedo), con crucero si no tocas nada: la velocidad alta multiplica los
  puntos y la centrífuga — arriesgar paga. **Turbo pads** (flechas neón
  fijas en la calzada): pisarlas dispara un subidón con FOV extra,
  vibración de cámara, muro de luz alargado y destello shimmer. La cámara
  es **viva**: baja, se aleja y abre el FOV con la velocidad. Un golpe sin
  escudo = **trompo** (giro de 360°, velocidad clavada, muro de luz
  apagado) + escombros 3D; pasar **rozando** una barrera da +25. Pista
  ancha (semiancho 9.5u) con **bordes Tron continuos** (cintas de neón
  cyan/magenta con bloom a lo largo de toda la vuelta — la referencia
  visual de noche y dentro del túnel), farolas cada ~20u (InstancedMesh,
  1 draw call por color) para que la velocidad se vea pasar, asfalto más
  claro con 3 carriles marcados, niebla más lejana en abierto y el
  **ambiente teñido por la zona musical** (la niebla vira hacia el color
  del sector — el circuito cuenta por dónde va el sample). Los obstáculos
  vienen en **patrones** (suelto, pareja, muro con hueco, eslálon) que se
  endurecen por vuelta. **Minimapa** abajo-izquierda: contorno real del
  trazado coloreado por superficie (asfalto/tierra/túnel), meta, rivales
  fantasma y tu posición con pulso. **Estilo ciudad**: calles cruzadas a
  nivel del suelo que el circuito sobrevuela + ~90 edificios con ventanas
  (InstancedMesh) flanqueando la pista formando cañones de calle — con un
  margen real (medio ancho de calzada + medio ancho de edificio + colchón)
  para que ninguno quede rozando o clavado en otro tramo de la pista (el
  circuito se cruza sobre sí mismo en el espacio). **12 carteles
  publicitarios** con coñas internas del propio proyecto ("SSR Motors:
  reflejamos hasta tu ego", "Convergencia garantizada o le devolvemos sus
  rayos", "Autoescuela Trompo: gira 360° gratis"…), textura procedural con
  texto autoajustado al ancho del panel y marco de neón a juego. **3
  cámaras** (botón 📷 o tecla V, "dentro" por defecto al entrar al
  circuito): persecución, dentro (primera persona desde el manillar, con
  el horizonte inclinándose al girar) y frontal (mirando a la moto). La
  visibilidad de la moto la decide el propio bucle de render cada frame
  (no un ajuste puntual al cambiar de cámara), así no depende de si la
  escena 3D ya había terminado de cargar. La moto lleva un **halo
  billboard** aditivo para leerse sobre
  cualquier fondo (la tierra clara se la comía). **Reflejos SSR en el
  asfalto**: no un cubemap fijo — un barrido real en espacio de pantalla
  (12 pasos, distancia creciente) contra una pasada previa de
  color+profundidad de la propia escena (256×144, a fotogramas alternos
  para abaratarla), con Fresnel (más reflejo a rasante, como el asfalto
  mojado) y el mismo tinte de niebla por zona. Solo el asfalto —la tierra
  no debe verse pulida—, con un coste medido de ~15-20% de fps bajo
  render por software (SwiftShader headless; en una GPU real, mucho
  menos). Sacrifica recibir la sombra del jugador en esos tramos (el
  reflejo del propio kart compensa visualmente).
  **Ranking global** (`wasm/api/leaderboard.js`, Vercel Serverless Function
  sin dependencias + Upstash Redis como sorted set) con **fallback
  automático al ranking local** si no hay backend desplegado o falla la
  red — el mismo objeto `lb` decide en tiempo real y lo indica en la UI
  (🌐 global / 📴 este dispositivo). Sin capas pregrabadas: un solo motor,
  parámetros vivos. En móvil: arrastre táctil (vertical en N1, horizontal
  en N2), canvas retina ~56vh, pantalla completa, mini-HUD en el lienzo,
  vibración y pausa automática.
  > ⚠️ **Importante para el despliegue**: si en Vercel el "Root Directory"
  > del proyecto está puesto a `wasm` (lo normal, ya que ese es el sitio
  > estático que se sirve), la función **tiene** que vivir en
  > `wasm/api/leaderboard.js` — Vercel solo detecta funciones dentro del
  > Root Directory configurado. Si estuviera en `api/` en la raíz del
  > repo (fuera de `wasm/`), Vercel jamás la despliega y visitar
  > `/api/leaderboard` da 404, aunque el resto del sitio funcione bien.
  > Además hay que conectar la integración de Storage (Upstash o Vercel
  > KV) al proyecto para que existan `KV_REST_API_URL`/`KV_REST_API_TOKEN`
  > (o `UPSTASH_REDIS_REST_URL`/`UPSTASH_REDIS_REST_TOKEN`).
  **Circuito mucho más largo** (~5750u, 2.5× la versión anterior) con un
  **primer tramo de calentamiento** (recta larga y llana + curva amplia,
  ~17% de la vuelta) sin obstáculos en la primera vuelta — tiempo de
  acostumbrarse a la moto antes del chicane, la cresta grande, el túnel y
  la tierra; el resto de elementos (pilones, farolas, edificios, dunas,
  turbo pads) escalan su densidad con la longitud real de la vuelta en
  vez de usar recuentos fijos. **Sidebar de teclas** (tecla H o botón ⌨):
  panel semi-transparente (50% de opacidad, no tapa el juego) con todos
  los controles de ambos niveles.
  **Modal "Cómo jugar"**: la portada queda limpia (título, tagline y
  botones); las instrucciones completas viven en un modal aparte que se
  abre solo, una vez, en la primera visita (recordado en localStorage) y
  siempre accesible después con el botón ❓ — se cierra con el botón ✕, con
  Escape o clicando fuera.

Paridad completa con la versión JS.

> ⚠️ Este `.wasm` se compila en tu máquina (el entorno donde se escribió el código
> no podía compilar a wasm). Si `rustc` se queja de algo al compilar, es un ajuste
> menor — pásame el error y lo corrijo.

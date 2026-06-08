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
| `processor.js` | `AudioWorkletProcessor` que instancia el wasm y rellena la salida cada bloque. |
| `index.html` | La página (UI sencilla: Original/Drone/Shimmer + Carácter + Volumen). |
| `build.sh` | Compila `raydrone.rs` → `raydrone.wasm`. |

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

El comando real que ejecuta `build.sh` es:

```bash
rustc --edition 2021 --target wasm32-unknown-unknown -O -C panic=abort -C lto=fat \
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

Pendiente respecto a la versión JS: el *Convergence Lab* (la demostración medible
1/√N) sigue solo en JS.

> ⚠️ Este `.wasm` se compila en tu máquina (el entorno donde se escribió el código
> no podía compilar a wasm). Si `rustc` se queja de algo al compilar, es un ajuste
> menor — pásame el error y lo corrijo.

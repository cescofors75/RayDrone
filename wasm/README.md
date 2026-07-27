# RayDrone · WebAssembly

RayDrone es un instrumento granular: el motor DSP está escrito en Rust y se
ejecuta dentro de un `AudioWorklet` mediante WebAssembly. La interfaz principal
abre en **Básico** y concentra los controles esenciales: material, carácter,
movimiento, espacio y volumen. Los modos Medio y Profesional despliegan el
control detallado sin alterar la escena actual.

## Ejecutar

```powershell
.\build.ps1
python -m http.server 8080
```

Abre `http://localhost:8080/`. AudioWorklet requiere HTTP local o HTTPS.

## Estructura

| Archivo | Función |
| --- | --- |
| `index.html` | Interfaz del instrumento y visualización. |
| `raydrone.rs` | Motor granular Rust `no_std`. |
| `processor.js` | Puente entre Web Audio y WASM. |
| `lab-worker.js` | Cálculos de convergencia fuera de la UI. |
| `build.ps1` / `build.sh` | Compilación del núcleo y del WASM. |

## Verificación

```powershell
.\build.ps1
node test_engine.mjs
```

RayRunner se mantiene ahora en el repositorio independiente
`C:\Users\cesco\Desktop\RayRunner`.

#!/usr/bin/env bash
# Compila el motor RayDrone (Rust → WebAssembly). Sin dependencias externas:
# no usa wasm-bindgen ni crates, así que NO necesita acceso a crates.io.
set -e
cd "$(dirname "$0")"

# El target de wasm hace falta una sola vez. Si ya está, esta línea no hace nada.
# (Necesita rustup; si lo instalaste con rustup, descargará el std de wasm offline-friendly.)
rustup target add wasm32-unknown-unknown 2>/dev/null || true

rustc --edition 2021 \
      --target wasm32-unknown-unknown \
      -O -C panic=abort -C lto=fat \
      --crate-type=cdylib \
      raydrone.rs -o raydrone.wasm

echo "✓ raydrone.wasm generado ($(wc -c < raydrone.wasm) bytes)"
echo "Ahora sirve la carpeta por HTTP, p.ej.:  python3 -m http.server 8080"
echo "y abre  http://localhost:8080/wasm/  (o el puerto que uses)"

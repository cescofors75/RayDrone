# RayDrone VST (simplified)

A **VST3 / CLAP instrument** version of [RayDrone](../README.md): load a WAV
"scene" and a continuous cloud of stochastic grains ("rays") is cast around a
focal point. The drone is not stored in the sample — it **emerges** from the
convergence of N rays, exactly as in the WASM engine, just slimmed down.

Built with [nih-plug](https://github.com/robbert-vdh/nih-plug) (Rust). The DSP
is a per-instance port of `wasm/raydrone.rs` in [`src/engine.rs`](src/engine.rs).

## Controls (minimal set)

| Knob | Optical analogy | Effect |
|---|---|---|
| **Density** | Number of rays (N) | Grains per second. More → smoother, richer render |
| **Aperture** | Depth of field | Width of the temporal dispersion cone (ms). Narrow = tonal, wide = drone |
| **Focus** | Camera focal point | Position in the sample the rays are cast around |
| **Reverb** | — | Freeverb-lite wet mix |
| **Evolve** | Recursive transport | Autoevolution: the output envelope feeds back into focus & aperture so the drone drifts and breathes on its own |
| **Master** | — | Output level |

Load a WAV with the **Load WAV…** button. The path is saved with the DAW
project, so the scene is recalled on reload.

## The visualizer

The top panel renders the methodology live: a focal "camera" at the apex casts
**rays** down onto the scene timeline. The translucent **dispersion cone** is the
depth of field (aperture); each ray lands where a grain is actually fired, newest
rays brightest, colored from accent (near focus) to cyan (far). The whole field
glows with the output level and **drifts with Evolve** — you can see the
autoevolution sweeping the focus and breathing the aperture.

## What it keeps vs. drops

**Keeps** the core methodology: continuous grain cloud, golden-ratio
quasi-Monte Carlo sampling, triangular dispersion (depth of field), Catmull-Rom
interpolation, per-grain micro-detune, equal-power stereo spread, Freeverb-lite,
DC blocker and soft clip.

**Drops** (to stay simple): chromatic aberration, Russian-roulette bounces,
recursive autoevolution, ambient foci, microtonal scales, inverse tracing and
the Convergence Lab. Those live in the full WASM build.

## Build

Requires a recent stable Rust toolchain.

```sh
cd vst
cargo xtask bundle raydrone --release
```

The bundle is written to `target/bundled/`:

- `RayDrone.vst3` — copy to your VST3 folder
  (Linux: `~/.vst3`, macOS: `~/Library/Audio/Plug-Ins/VST3`,
  Windows: `%COMMONPROGRAMFILES%\VST3`).
- `RayDrone.clap` — copy to your CLAP folder
  (Linux: `~/.clap`, macOS: `~/Library/Audio/Plug-Ins/CLAP`,
  Windows: `%COMMONPROGRAMFILES%\CLAP`).

A plain `cargo build --release` also produces the raw shared library under
`target/release/`, but the `xtask bundle` step is what creates the proper
`.vst3` / `.clap` plugin folders.

### macOS (tested target)

```sh
cd vst
cargo xtask bundle raydrone --release
```

This drops `RayDrone.vst3` and `RayDrone.clap` in `target/bundled/`. Copy them to:

- `~/Library/Audio/Plug-Ins/VST3/`
- `~/Library/Audio/Plug-Ins/CLAP/`

Then rescan in your DAW (Ableton, Logic via a VST3 host, Bitwig, Reaper…).

**Universal binary (Apple Silicon + Intel)** — build a fat plugin so it runs on
both architectures:

```sh
rustup target add x86_64-apple-darwin aarch64-apple-darwin
cargo xtask bundle-universal raydrone --release
```

Notes for macOS:

- The GUI uses OpenGL via `baseview`; no extra system packages are needed
  (unlike Linux, which needs the X11/GL `-dev` headers to build).
- The plugin is **unsigned**. If Gatekeeper blocks it, clear the quarantine flag:
  `xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/VST3/RayDrone.vst3`.
- It's an **instrument** (stereo out, no MIDI): add it on an instrument/MIDI
  track, load a WAV, and it renders the drone continuously.

## License

MIT — same as the parent project.

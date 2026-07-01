# From Pixels to Grains: Variance-Reduced Monte Carlo as a Unifying Framework for Granular Synthesis

**Target venue:** DAFx (International Conference on Digital Audio Effects) — short/long paper.
**Status:** working draft / outline. Prose in English; `💡 nota` blocks are Spanish guidance for us, to be deleted before submission.

---

## Abstract

*(~150 words — draft)*

Asynchronous granular synthesis scatters short, windowed excerpts of a source
signal in time. We observe that this process is formally a **Monte Carlo
estimator of a transport integral** over the time axis of the source buffer —
the exact mathematical machinery used to estimate the *rendering equation* in
computer graphics, where each pixel integrates incident light by tracing N
random rays. Under this view a grain is a ray, the playback focus is a pixel,
and the grain density N controls estimator variance: the texture converges to a
deterministic target with error proportional to 1/√N. We show that the
variance-reduction techniques of physically-based rendering — **stratified
sampling, quasi-Monte Carlo (QMC), importance sampling and Russian-roulette
termination** — transfer directly to granular synthesis and measurably improve
convergence on real audio. We further show that *source-energy reverse tracing*,
a tempting "improvement", is a **biased** estimator of a different target, which
explains its distinct timbral character. We provide an open, dependency-free
Rust/WebAssembly implementation and an in-browser convergence laboratory that
reproduces every figure.

💡 nota: el abstract es la pieza que más se reescribe. Esta versión ya dice las
tres cosas que vende el paper: (1) la reformulación, (2) que las técnicas de
gráficos transfieren y se *miden*, (3) la honestidad del trazado inverso sesgado.

---

## 1. Introduction

- Granular synthesis (Gabor → Roads → Truax) as established practice; usually
  framed perceptually ("clouds of sound"), rarely with an estimation-theoretic
  lens.
- Computer graphics solved the *same shape of problem* — estimating a high-
  dimensional integral by random sampling — and built 30 years of variance-
  reduction theory around it.
- **Contribution:**
  1. A formal mapping: granular synthesis ≡ Monte Carlo estimation of a temporal
     transport integral (Sec. 3).
  2. Direct transfer of four graphics variance-reduction methods, with measured
     convergence on real audio (Sec. 4–5).
  3. An honest treatment of *reverse tracing* as a biased estimator (Sec. 5.3).
  4. A reproducible open implementation + browser-based Convergence Lab (Sec. 6).

💡 nota: la contribución hay que poder defenderla en una frase. La nuestra:
"no es una metáfora, es el *mismo* estimador, y lo demostramos midiendo."

---

## 2. Background

### 2.1 The rendering equation and Monte Carlo integration
The pixel value is L(x,ω) = Lₑ + ∫_Ω f_r(x,ω,ω') L_i(x,ω') (ω'·n) dω', estimated by
tracing N rays and averaging. Error of a standard MC estimator falls as σ/√N.

### 2.2 Granular synthesis
Asynchronous grain clouds: grains of duration D, windowed by w(·), drawn around a
read position (the *focus*) within a spread (the *aperture*), at density N.

💡 nota: citar Kajiya 1986 (rendering equation), Veach 1997 (tesis, importance
sampling / MIS), Roads "Microsound", Truax (granular en tiempo real), Gabor 1947.

---

## 3. Granular synthesis as a temporal transport integral  ← **núcleo del paper**

Let `s[·]` be the source buffer, `f` the focus (samples), and `τ` an offset drawn
from a probability density `p(τ)` supported on the aperture `[-A, A]` (e.g. the
triangular kernel we use). Define the **target grain texture** as the windowed
expectation of shifted copies of the source:

```
g[n] = w[n] · E_{τ~p}[ s[f + τ + n] ]
     = w[n] · ∫ p(τ) · s[f + τ + n] dτ ,    n = 0 … D-1
```

A cloud of N grains is precisely the **Monte Carlo estimator** of this integral:

```
ĝ_N[n] = w[n] · (1/N) Σ_{i=1}^{N} s[f + τ_i + n] ,     τ_i ~ p
```

The estimator is unbiased by linearity of expectation (`E[ĝ_N] = g` for every
N), with per-sample variance σ²/N; the central limit theorem then gives the
convergence rate, so the RMS error over the grain falls as

```
‖ ĝ_N − g ‖ ∝ σ / √N .
```

**The correspondence (Table 1):**

| Computer graphics | RayDrone (audio) |
|---|---|
| Pixel | Playback focus `f` |
| Ray / light path | Grain |
| Hemisphere integral | Aperture integral over `τ` |
| Samples per pixel N | Grain density N |
| Pixel noise (variance) | Textural "grain"/roughness |
| Convergence ∝ 1/√N | Texture settling ∝ 1/√N |

💡 nota: ESTA es la sección que justifica el título. La ecuación de g[n] y la de
ĝ_N[n] son las dos fórmulas que tienen que aparecer grandes. Fig. 1 = el dibujo
píxel/rayo ↔ foco/grano (el "genesis" que ya tenemos en el README).

---

## 4. Transferring variance reduction

For each method: one paragraph (what it is in graphics) + how it maps to grains.

### 4.1 Stratified sampling
Partition `[-A,A]` into N strata, draw one `τ` per stratum → removes clumping,
variance falls faster than pure random.

### 4.2 Quasi-Monte Carlo (QMC)
Golden-ratio additive recurrence (`τ_i = frac(τ_0 + i·φ)·2A − A`) with
Cranley–Patterson rotation → low-discrepancy coverage of the aperture.

### 4.3 Importance sampling
Draw `τ ∝ q(τ)` where `q` follows local source energy, and **reweight by
`p(τ)/q(τ)`**. This is the key: reweighting keeps the estimator *unbiased* while
concentrating samples where the integrand is large.

### 4.4 Russian roulette (bounces / tail)
When a grain ends, spawn a child with survival probability `ρ` **at constant
gain** — the *analog Monte Carlo* absorption scheme of particle transport
(terminate with probability = absorption, no reweighting): the bounce chain is
then an unbiased sample of the Neumann series of a medium whose reflection
coefficient *is* `ρ`, extended without unbounded recursion. We deliberately do
**not** apply the graphics-style `1/ρ` compensation: compensated roulette is
unbiased for a series with *fixed* per-order weights, but the compensation
injects gain spikes on surviving paths — musically unacceptable — whereas the
analog scheme keeps `ρ` itself as the "reflect" control (tail length ↔ energy),
exactly as a physical absorption coefficient would.

💡 nota: Fig. 2 = las distribuciones de puntos de las 4 estrategias sobre la
apertura (un panel cada una). Es muy visual y barato de generar.

---

## 5. Evaluation: the Convergence Lab

### 5.1 Methodology
- Deterministic **target** = full weighted sum `g[n]` (all taps in the aperture).
- For N ∈ {1,2,4,…,4096}, compute `ĝ_N` and measure RMS error vs target.
- Average over T independent trials; fit the slope in log–log space (`log RMS`
  vs `log N`). Ideal random estimator → slope −0.5.

### 5.2 Results (real audio)
Report fitted slopes per method. Expected/observed:
- Random ≈ −0.5 (matches theory).
- Stratified / QMC / importance: steeper (faster convergence).

💡 nota: aquí van los números REALES que ya sacamos del Lab (random ≈ −0.5,
estratificado/QMC/importance más bajos). Fig. 3 = la gráfica log-log con la
línea ideal 1/√N. Hay que correr el Lab sobre 2–3 samples distintos (un bajo
303, un pad, percusión) y tabular pendientes. **Pendiente: export CSV del Lab.**

### 5.3 Reverse tracing is biased — and that is the point
Rejection sampling proportional to source energy *without* reweighting does **not**
estimate `g[n]`; it estimates a different, energy-weighted target. The estimator's
error therefore decomposes as `RMS² = bias² + variance/N`, and the data shows both
regimes:

- **On quasi-uniform musical material** (runs 1–4), the energy inside the aperture
  varies little, so the bias term is *below the estimator's noise floor* across the
  measured range: reverse simply tracks random (mean slope −0.496 vs −0.490) and
  gains **nothing** from the variance-reduction budget it spends. Already a negative
  result worth stating: the tempting "smart" sampler buys no convergence.
- **On strongly structured material** (run 5: a source whose aperture straddles a
  loud/near-silent boundary), the bias term dominates at large N and the curve
  **plateaus** at the bias level (~2.8·10⁻³): between N = 8192 and 32768 reverse's
  local slope collapses to ≈ −0.08 while random continues at ≈ −0.45 and *crosses
  below it*, and reweighted importance reaches 1.1·10⁻⁵ — 260× lower. This is the
  bias made visible.

We present the pair as a worked example of bias vs. variance, and argue reverse
tracing is a *timbral* choice (it renders the energy-weighted texture, which can
sound fuller), not a quality improvement.

💡 nota: Fig. 4 = `figures/run5-bias-ap91-foc2.0.png` (reverse se aplana, random
lo cruza por debajo, importance se hunde). El run es sintético y reproducible con
`node paper/make_bias_run.mjs` — en los runs 1–4 (música real) el sesgo queda bajo
el ruido y reverse ≈ random, que es el otro régimen y también se cuenta. Es el
detalle que da credibilidad: distinguimos "converge mejor" de "suena distinto".

---

## 6. Implementation

- `no_std` Rust, no crates, compiled to `wasm32-unknown-unknown`; runs in an
  AudioWorklet, mixing per sample (no per-grain Web Audio nodes, no scheduler
  jitter).
- Continuous grain cloud, equal-power panning, cubic (Catmull-Rom) interpolation,
  per-grain micro-detune, Freeverb-lite stereo reverb, output DC blocker.
- In-browser Convergence Lab reproduces all figures; performance diagnostics
  (audio-thread CPU, active voices, grains/s, latency).
- **Lab vs. live instrument — an honest distinction.** The Lab measures the
  *canonical* forms of each sampler: N-strata stratification, f64 golden-ratio
  Kronecker sequence with Cranley–Patterson rotation, and reweighted (`p/q`,
  unbiased) importance sampling. The real-time instrument ships cheaper
  *streaming* variants of the first two — an iterative f32 golden recurrence
  without rotation, and a fixed 17-stratum round-robin (which improves the
  constant but is asymptotically slope −0.5) — and its "smart rays" mode is
  precisely the **biased reverse** sampler of Sec. 5.3, not the unbiased
  importance sampler. All convergence figures characterise the canonical forms;
  the streaming variants inherit the qualitative behaviour at musical densities
  but not the asymptotic slopes.
- Open source (MIT license).

💡 nota: esto es la sección de "reproducibilidad" que a los revisores les encanta:
todo corre en el navegador y el código está publicado.

---

## 7. Discussion & limitations (honest)

- This is a **mathematical/algorithmic** correspondence, **not** a physical
  simulation of acoustic wave propagation. The integral is over the *time axis of
  one buffer*, not over a room's geometry.
- Terms like "chromatic aberration" and "bounces" are perceptual metaphors built
  on top of the estimator, not claims about physics.
- Evaluation is signal-domain (RMS convergence); a perceptual listening study is
  future work.

💡 nota: poner los límites NOSOTROS antes de que lo haga un revisor es lo que
convierte "venta de humo" en "trabajo serio". Esta sección nos protege.

## 8. Future work
Multiple importance sampling (MIS) combining `p` and energy-`q`; temporal "BRDF"
kernels; adaptive N driven by a perceptual error metric; listening tests.

## 9. Conclusion
Granular synthesis is Monte Carlo transport on the time axis. The variance-
reduction toolkit of rendering transfers, improves convergence measurably, and
clarifies which "improvements" are unbiased and which are timbral.

---

## References (to fill)
- Kajiya, J. *The Rendering Equation.* SIGGRAPH 1986.
- Veach, E. *Robust Monte Carlo Methods for Light Transport Simulation.* PhD, 1997.
- Roads, C. *Microsound.* MIT Press, 2001.
- Truax, B. *Real-time granular synthesis…* Computer Music Journal, 1988.
- Gabor, D. *Acoustical quanta and the theory of hearing.* Nature, 1947.
- Keller & contributors on QMC for rendering; Cranley & Patterson 1976 (rotation).

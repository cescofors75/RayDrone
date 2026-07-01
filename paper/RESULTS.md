# Results — measured convergence

Four runs of the Convergence Lab on real samples (different focus positions and
apertures), plus one synthetic bias-demonstration run. Raw data in
[`data/`](data/), figures in [`figures/`](figures/), regenerate with
`python3 paper/make_figures.py` (run 5: `node paper/make_bias_run.mjs`).

## Fitted convergence slopes (log RMS vs log N) — real audio, runs 1–4

| run | aperture (ms) | focus (s) | random | stratified | qmc | importance | reverse |
|---|---|---|---|---|---|---|---|
| 1 | 93.5 | 4.43 | **−0.490** | −0.714 | −0.706 | −0.709 | −0.501 |
| 2 | 93.5 | 7.01 | **−0.490** | −0.989 | −0.872 | −0.985 | −0.485 |
| 3 | 149.7 | 2.22 | **−0.496** | −0.529 | −0.539 | −0.517 | −0.508 |
| 4 | 149.7 | 58.38 | **−0.481** | −0.646 | −0.654 | −0.658 | −0.487 |
| **mean (1–4)** | | | **−0.490** | −0.720 | −0.693 | −0.718 | −0.496 |

Run 5 (synthetic, tone|near-silence boundary, N up to 32768, 8 trials) is kept
out of the mean on purpose: it is the *bias demonstration*, not more of the same
population. Its overall fitted slopes are random −0.458, stratified −0.959, qmc
−0.786, importance −0.976, reverse −0.436 — but for reverse the single fitted
slope is misleading, which is exactly the point (see result 2b below).

## What the data proves (three headline results)

**1. Random follows the textbook `1/√N` exactly.**
Across four independent samples and conditions the random estimator sits at
−0.490, −0.490, −0.496, −0.481 — mean **−0.490**, essentially the theoretical
−0.5. This is the empirical confirmation that the rendered grain texture is a
genuine Monte Carlo estimator of the transport integral, not a metaphor. *(Fig.
3, the per-run log–log plots.)*

**2a. On real (quasi-uniform) audio, reverse tracing does NOT converge faster —
it tracks random.** Reverse sits at −0.501, −0.485, −0.508, −0.487 (mean
**−0.496**), statistically indistinguishable from random and clearly *above* the
variance-reduced methods. Source-energy rejection without reweighting is a
**biased** estimator of a *different* target; on material whose energy varies
little inside the aperture the bias stays below the noise floor over the
measured N range, so all you see is that it buys no convergence. Its appeal is
timbral, not numerical.

**2b. On strongly structured material, the bias surfaces as a plateau — run 5.**
With the aperture straddling a loud/near-silent boundary, reverse's error
**floors at ≈ 2.8·10⁻³** (`RMS² = bias² + var/N` with the bias term dominating):
between N = 8192 and 32768 its local slope collapses to ≈ −0.08, random
(≈ −0.45) *crosses below it*, and reweighted importance reaches 1.1·10⁻⁵ —
about 260× lower at equal N. Together, 2a + 2b are the falsifiable version of
the paper's §5.3: same estimator, two regimes, and the plateau appears exactly
where the bias/variance decomposition says it must.
*(Fig. 4: `figures/run5-bias-ap91-foc2.0.png`.)*

**3. Variance reduction works — and its strength depends on the integrand.**
Stratified, QMC and importance beat random in **all four** runs. But the size of
the win is not constant: it ranges from a mild −0.53 (run 3) to a dramatic −0.99
(run 2, stratified ≈ `1/N`). This is the importance-sampling story made audible:
when the source energy varies a lot across the aperture (structured material at
the focus) the integrand is "peaky" and smart sampling helps enormously; when the
aperture sits over fairly uniform material, plain random is already near-optimal
and there is little to gain. **The benefit is signal-dependent**, which is a real,
publishable observation about *when* the technique pays off.

## Honest caveats for the paper

- Trials are few (4/point in runs 1–4, 8/point in run 5); slopes carry noise,
  especially the near-`1/N` cases. The submission should average more trials
  (e.g. 16–32) and report confidence on the fitted slope.
- The Lab measures the *canonical* samplers; the live instrument ships streaming
  variants (f32 iterative QMC without rotation, fixed 17-strata round-robin) and
  its "smart rays" mode is the biased reverse method. The paper's §6 states this
  explicitly — the figures characterise the canonical forms.
- The practical takeaway is **"same smoothness with fewer grains"** (lower CPU at
  equal quality at low–moderate N), not "audibly cleaner at musical densities".
  Frame the contribution that way and it is bulletproof.

## Figures

| File | Use |
|---|---|
| `figures/run1…run4*.png` | Per-condition log–log convergence on real audio (paper Fig. 3 candidates) |
| `figures/run5-bias-ap91-foc2.0.png` | Reverse plateaus / random crosses below / importance plunges (paper Fig. 4) |
| `figures/summary-grid.png` | Overview grid of all runs (consistency at a glance) |

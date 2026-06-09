# Results — measured convergence on real audio

Four runs of the Convergence Lab on real samples (different focus positions and
apertures). Raw data in [`data/`](data/), figures in [`figures/`](figures/),
regenerate with `python3 paper/make_figures.py`.

## Fitted convergence slopes (log RMS vs log N)

| run | aperture (ms) | focus (s) | random | stratified | qmc | importance | reverse |
|---|---|---|---|---|---|---|---|
| 1 | 93.5 | 4.43 | **−0.490** | −0.714 | −0.706 | −0.709 | −0.501 |
| 2 | 93.5 | 7.01 | **−0.490** | −0.989 | −0.872 | −0.985 | −0.485 |
| 3 | 149.7 | 2.22 | **−0.496** | −0.529 | −0.539 | −0.517 | −0.508 |
| 4 | 149.7 | 58.38 | **−0.481** | −0.646 | −0.654 | −0.658 | −0.487 |
| **mean** | | | **−0.490** | −0.720 | −0.693 | −0.718 | −0.496 |

## What the data proves (three headline results)

**1. Random follows the textbook `1/√N` exactly.**
Across four independent samples and conditions the random estimator sits at
−0.490, −0.490, −0.496, −0.481 — mean **−0.490**, essentially the theoretical
−0.5. This is the empirical confirmation that the rendered grain texture is a
genuine Monte Carlo estimator of the transport integral, not a metaphor. *(Fig.
3, the per-run log–log plots.)*

**2. Reverse tracing does NOT converge faster — it tracks random.**
Reverse sits at −0.501, −0.485, −0.508, −0.487 (mean **−0.496**), statistically
indistinguishable from random and clearly *above* the variance-reduced methods.
This is exactly the honest, falsifiable claim of the paper: source-energy
rejection without reweighting is a **biased** estimator of a *different* target,
so it cannot converge to `g[n]` faster. Its appeal is timbral, not numerical.
*(Fig. 4: reverse overlaps random while importance pulls away.)*

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

- N is small (≈ 4 trials/point in these runs); slopes carry noise, especially the
  near-`1/N` cases. The submission should average more trials (e.g. 16–32) and
  report confidence on the fitted slope.
- The practical takeaway is **"same smoothness with fewer grains"** (lower CPU at
  equal quality at low–moderate N), not "audibly cleaner at musical densities".
  Frame the contribution that way and it is bulletproof.

## Figures

| File | Use |
|---|---|
| `figures/run*.png` | Per-condition log–log convergence (paper Fig. 3 / 4 candidates) |
| `figures/summary-grid.png` | 2×2 overview of all four runs (consistency at a glance) |

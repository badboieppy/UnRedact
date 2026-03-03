# Synthetic Seed Variance Investigation (Round 4)
Date: 2026-03-02
Source artifact: `.local-agent/runtime/investigations/baseline_synthetic_overfitting.json`

## Scope
- Evaluate whether adding a new "average over 10+ random seeds" non-regression gate is statistically stable.
- Quantify seed-to-seed variance using currently emitted exploratory seed diagnostics.

## Observations
- Current protocol emits 5 exploratory seeds per run (diagnostic only), generated from a fixed base.
- Historical synthetic artifacts under `benchmark/` use the same exploratory seed base and same 5 exploratory seeds.
- Therefore, currently available random-seed variance evidence is from those 5 exploratory seeds.

Exploratory seed metrics in baseline artifact:
- seed `562127815042821759`: r@20 = `0.000000`, mrr = `0.000000`
- seed `83275707321985586`: r@20 = `0.093750`, mrr = `0.012899`
- seed `13406875607376352761`: r@20 = `0.062500`, mrr = `0.013576`
- seed `5684778449485788136`: r@20 = `0.031250`, mrr = `0.002931`
- seed `13243799229639241892`: r@20 = `0.062500`, mrr = `0.010471`

Summary stats (exploratory-only):
- `mean_r20 = 0.050000`
- `sd_r20 = 0.035630`
- `mean_mrr = 0.007975`
- `sd_mrr = 0.006142`

## Stability implications
If the gate is strict "new random-seed mean must be >= baseline mean" with no tolerance:
- false regression probability remains near 0.5 even at larger N (sampling noise around same true mean).
- this makes strict no-tolerance random-seed gating operationally noisy.

Bootstrap estimate (from observed exploratory distribution, 10k trials):
- N=10: strict false regression rate
  - r@20: `0.4358`
  - mrr: `0.4834`
- N=20: strict false regression rate
  - r@20: `0.4561`
  - mrr: `0.4987`
- N=30: strict false regression rate
  - r@20: `0.4579`
  - mrr: `0.4929`

With tolerance bands:
- using `epsilon_r20 = 0.01`, false regression falls with N:
  - N=10: `0.1403`
  - N=20: `0.0797`
  - N=30: `0.0445`
- using `epsilon_mrr = 0.002`, false regression:
  - N=10: `0.1342`
  - N=20: `0.0565`
  - N=30: `0.0227`

## Approximate sample size estimates (95% CI half-width target)
Using `n ~= (1.96 * sd / epsilon)^2` on exploratory variance:

r@20:
- `epsilon = 0.02` -> `n ~ 13`
- `epsilon = 0.015` -> `n ~ 22`
- `epsilon = 0.01` -> `n ~ 49`

mrr:
- `epsilon = 0.004` -> `n ~ 10`
- `epsilon = 0.003` -> `n ~ 17`
- `epsilon = 0.002` -> `n ~ 37`

## Evidence-backed conclusion
- A "10+ random seeds" average gate is feasible, but strict zero-tolerance non-regression will be flaky by construction.
- A statistically safer binding gate needs:
  - explicit tolerance (or confidence-interval rule),
  - and fixed evaluation seed list per run for determinism, with random/exploratory kept diagnostic.
- Current data is limited to 5 exploratory seeds; final thresholds should be confirmed after expanding exploratory sample collection.

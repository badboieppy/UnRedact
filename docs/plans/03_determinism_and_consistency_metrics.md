# Problem 3 Plan: Determinism And Consistency Metrics In Accuracy Benchmarks

## Audience
New engineers working on benchmark reliability, reproducibility, and regression detection.

## Date
2026-02-17

## Problem Statement
Current tests and benchmarks do not explicitly verify determinism. We need consistency metrics that quantify how similar results are across repeated runs with identical code, input, and config.

## Current System Behavior
### What exists today
- `guess_accuracy_benchmark` outputs accuracy and visual quality metrics for two datasets.
- It does not repeat runs or compute consistency.
- No explicit determinism score is emitted.

### Evidence collected
- Two consecutive benchmark runs with identical inputs produced identical JSON hash in current environment.
- This is a positive signal, but it is not protected by tests and could regress silently.

### Why this is still a problem
- Determinism can break from:
  - unstable sort tie handling,
  - parallel execution changes,
  - floating-point tie drift,
  - unordered map iteration introduced in future changes.
- Without consistency metrics, we only detect severe regressions manually.

## Expected Behavior After Fix
- Benchmark can run multiple repeats in one command and report consistency metrics.
- Output includes a deterministic signature and run-to-run similarity measures.
- CI can fail when consistency drops below threshold.
- Developers can quickly identify if a change introduces nondeterminism.

## Feasibility
High.
- Existing benchmark pipeline already emits rich structured data.
- Needed work is metric derivation and repeated-run orchestration.
- No model retraining or external service needed.

## Decisions Made
1. Add repeat support directly to `guess_accuracy_benchmark` (no separate tool).
2. Keep exact byte-level output hash checks plus softer similarity metrics.
3. Report consistency at both row level and dataset summary level.
4. Treat deterministic failures as blocking in CI for fixed seeds and fixed profile.

## Design
### CLI additions
Add benchmark flags:
- `--repeats <N>` default `1`
- `--determinism` shorthand for `--repeats 3`
- `--consistency-out <path>` optional separate JSON

### New output schema
Add top-level section:
- `consistency`
  - `repeats`
  - `all_hashes_identical` (bool)
  - `hash_match_ratio`
  - `top1_agreement_ratio`
  - `topk_jaccard_mean` (k=5 default)
  - `mean_rank_stddev`
  - `unstable_rows_count`
  - `unstable_rows_ratio`

Add per-dataset consistency:
- same fields scoped per dataset.

### Row-level canonicalization
Create canonical row key:
- `(dataset_name, page_index, bbox_rounded, row_index)`

Create canonical guess snapshot:
- top1 text
- top5 ordered list
- full candidate hash (optional)

This supports efficient row alignment across repeats.

### Similarity metrics definitions
1. `hash_match_ratio`
   - fraction of run output hashes equal to run 1 hash.
2. `top1_agreement_ratio`
   - fraction of rows where top1 guess matches across all repeats.
3. `topk_jaccard_mean`
   - average Jaccard similarity of top-K sets across repeat pairs.
4. `mean_rank_stddev`
   - per target standard deviation of best rank, averaged over targets.
5. `unstable_rows_ratio`
   - rows with at least one top1 mismatch divided by total rows.

### Determinism hardening in core code
Audit and enforce total ordering in all sorting paths:
- tie-break by text and stable row identifiers,
- avoid partial ordering ambiguity for NaN,
- preserve stable index order on equal scores.

If parallelization is added:
- collect results in index order before serialization.

## Data To Collect For Better Future Stability
Per repeat:
- run hash,
- stage timings,
- environment fingerprint:
  - OS,
  - rustc version,
  - profile name.

Per row:
- top1 text,
- top5 list,
- visual score fields.

## Testing And Benchmark Updates
### Unit tests
- metric calculators for:
  - Jaccard,
  - agreement ratio,
  - rank stddev,
  - unstable row detection.

### Integration tests
- new integration test:
  - run benchmark twice in-process and assert `all_hashes_identical`.
- add stress test with shuffled dictionary input order and verify same canonical output.

### CI checks
- Consistency gate for release benchmark:
  - `all_hashes_identical == true` for repeats=2 (strict mode),
  - or `top1_agreement_ratio >= 0.99` if strict mode must be relaxed for platform differences.

## Detailed TODO List
### Phase 0: Schema and plumbing
- [ ] Add `ConsistencySummary` structs in benchmark binary.
- [ ] Add CLI flags for repeats and consistency output.
- [ ] Add benchmark run loop to execute repeats.

### Phase 1: Canonical output and hash generation
- [ ] Implement canonical serialization for comparison snapshots.
- [ ] Compute per-repeat SHA-256 hash.
- [ ] Store hashes and repeat metadata in output.

### Phase 2: Metric computation
- [ ] Implement row alignment keying.
- [ ] Implement top1 agreement metric.
- [ ] Implement top5 Jaccard metric.
- [ ] Implement rank variance metric for targets.
- [ ] Implement unstable row ratio metric.

### Phase 3: Core determinism hardening
- [ ] Audit all `sort_by` paths in guess generation and consensus.
- [ ] Add deterministic fallback tie-breakers where missing.
- [ ] Add stable merge behavior for any parallel sections.

### Phase 4: Tests
- [ ] Add unit tests for each consistency metric function.
- [ ] Add integration test for identical repeated outputs.
- [ ] Add shuffled-input determinism test.
- [ ] Add CI job that runs consistency mode on benchmark datasets.

### Phase 5: Documentation
- [ ] Document metric definitions in benchmark output.
- [ ] Add README section on running consistency checks.
- [ ] Document known acceptable variance rules if platform-specific differences occur.

## Definition Of Done
- Benchmark supports repeated runs and emits consistency metrics.
- Determinism is enforced by automated tests and CI.
- Developers can quickly see when results drift between runs.

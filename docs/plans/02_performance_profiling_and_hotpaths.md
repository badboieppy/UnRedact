# Problem 2 Plan: Performance, Profiling, And Hotpath Simplification

## Audience
New engineers responsible for runtime behavior, profiling, and architecture simplification.

## Date
2026-02-17

## Problem Statement
Performance is currently perceived as poor, especially in benchmark and integration-test workflow. We need a systematic plan:
1. simplify design first,
2. add profiling,
3. optimize hot paths.

Target:
- `guess_accuracy_benchmark` should complete in 30 seconds or less in standard benchmarking mode.

## Current System Behavior
### Measured baseline (local machine, this repo state)
- `cargo run --bin guess_accuracy_benchmark` (debug profile): about 207 to 210 seconds.
- `cargo run --release --bin guess_accuracy_benchmark` first run: about 220 seconds (includes release compile).
- `cargo run --release --bin guess_accuracy_benchmark` warm run: about 4.6 seconds.
- `cargo test -q` integration-heavy tests:
  - `efta00038617_guessing`: about 69 seconds
  - `efta00101126_guessing`: about 119 seconds
  - `additional_epstein_files_run_without_file_specific_tuning`: about 153 seconds

### Important interpretation
- The largest pain is debug and test profile runtime, not release runtime.
- Current benchmark target must define which profile is authoritative. For product-level performance claims, release profile is the meaningful one.

### Architectural sources of wasted work
1. PDF parsing and width-table extraction is repeated in multiple stages.
2. Width measurement calls are repeated many times across dictionary words and redactions.
3. Visual scoring builds full overlay structures from cloned guess reports.
4. Candidate scoring still processes large candidate pools before late filtering.
5. Integration tests rerun expensive full pipeline independently.

## Expected Behavior After Fix
- Benchmark runtime is predictable and tracked with explicit stage timing.
- Expensive operations are filtered by cheap gates first.
- Repeated width and rendering work is cached and reused within run scope.
- Performance regressions are visible in CI via structured perf output.
- Team has a clear profile policy:
  - benchmark/CI performance target in release mode,
  - development profile remains usable with guardrails.

## Feasibility
High.
- Core algorithm already works functionally.
- Release runtime headroom is large, so simplification and caching can make debug/test profile acceptable and keep release well below target.
- No external dependency redesign required.

## Decisions Made
1. Define performance target on release profile benchmark execution.
2. Keep debug profile measurements, but treat them as developer-experience metrics, not shipment metrics.
3. Add first-class stage timing in code before changing algorithms.
4. Prioritize simplification and duplication removal before micro-optimizations.
5. Keep deterministic ordering while introducing parallelism.

## Design
### Performance model phases
1. Phase A: instrumentation and visibility
2. Phase B: simplify data flow and eliminate duplicated work
3. Phase C: optimize hot loops and memory churn
4. Phase D: CI threshold enforcement

### Instrumentation plan
Add structured timers around:
- redaction scan
- font run extraction
- width table build
- anchor selection
- dictionary scoring
- consensus passes
- visual scoring stages

Expose in:
- benchmark JSON (`timings` section)
- optional CLI print (`--perf-summary`)

### Dataflow simplification plan
Introduce a run-scoped cache object:
- `PdfAnalysisCache`
  - parsed PDF bytes
  - page boxes
  - width tables
  - font assets

Use it across:
- guess building
- visualization overlay generation
- visual scoring

This removes repeated parsing and repeated map construction.

### Candidate pipeline simplification
Current behavior scores large candidate sets then applies expensive checks late.
Change to staged filtering:
1. context validity gate
2. cheap width gate
3. punctuation and list gate
4. expensive visual scoring on retained subset

Cap candidate set aggressively between stages with configurable knobs.

### Hotpath optimization targets
1. Width measurement:
   - cache by `(font_key, font_size, h_scale, text, shaping_flags)`.
   - avoid repeated shaping for identical strings.
2. Candidate generation:
   - pre-normalize dictionary once per run.
   - avoid repeated `trim`, `split_whitespace`, and case conversion.
3. Visual scoring:
   - avoid cloning full guess reports.
   - compute overlays only for rows under evaluation.
4. Sorting:
   - avoid sorting full candidate vectors when top-K selection is enough.
   - use `select_nth_unstable_by` style selection where safe.

### Test profile strategy
To improve developer experience:
- use a dedicated benchmark command helper that defaults to release,
- keep fast smoke checks in debug,
- run expensive accuracy checks in release in CI schedule.

## Data To Collect For Better Future Performance Decisions
Per run:
- stage duration histogram,
- number of dictionary candidates entering and leaving each filter stage,
- number of shaping cache hits and misses,
- page render count and total rendered pixels,
- memory high-water estimate (optional).

Per dataset:
- rows with anchors,
- rows scored visually,
- candidates per row before and after filtering.

## Testing And Benchmark Updates
### New performance tests
- Add benchmark assertion test (ignored by default locally) that checks release-mode runtime threshold.
- Add regression tests for cache hit ratio floor on known datasets.

### CI benchmark reporting
- Save benchmark JSON artifacts with `timings` block.
- Track trend over commits.

### Correctness guardrails during optimization
- Run `tests/efta00101126_guessing.rs`.
- Run `tests/efta00038617_guessing.rs`.
- Run `tests/generalization_smoke.rs` non-ignored test.
- Ensure output hashes unchanged (or diffs explained) for baseline config.

## Detailed TODO List
### Phase 0: Baseline and instrumentation
- [ ] Add `PerfTimer` helper with scoped API.
- [ ] Instrument orchestrator major stages.
- [ ] Instrument guesser sub-stages and visual scoring sub-stages.
- [ ] Add `timings` object to benchmark JSON.
- [ ] Add CLI flag `--perf-summary`.
- [ ] Capture baseline report from current code and store in `benchmark/`.

### Phase 1: Simplify shared data flow
- [ ] Create `PdfAnalysisCache` type.
- [ ] Move page-box and width-table loading into shared cache builder.
- [ ] Update `orchestrator` and `visual_guess_score` to consume cache.
- [ ] Remove duplicate width-table extraction path from visualization module.
- [ ] Add tests for cache correctness parity.

### Phase 2: Candidate pipeline simplification
- [ ] Add explicit staged filter API with per-stage counts.
- [ ] Add early candidate cap after width filter.
- [ ] Move punctuation penalties to lightweight stage before visual scoring.
- [ ] Ensure consensus runs only on retained candidate sets.
- [ ] Record candidate counts in diagnostics.

### Phase 3: Hotpath optimization
- [ ] Add run-scoped width measurement cache.
- [ ] Precompute normalized dictionary entries once.
- [ ] Replace repeated trim and tokenize operations with precomputed fields.
- [ ] Replace full vector sort with top-K selection where safe.
- [ ] Reduce guess cloning in visual scoring.

### Phase 4: Parallelization and determinism safety
- [ ] Evaluate per-row parallel scoring with stable index merge.
- [ ] Add deterministic merge ordering tests.
- [ ] Keep single-thread fallback path for debugging.

### Phase 5: Benchmark policy and CI
- [ ] Add `scripts/bench_accuracy_release.ps1` wrapper.
- [ ] Document benchmark command contract in README.
- [ ] Add CI check for release runtime <= 30 seconds (warm run environment).
- [ ] Add CI warning threshold for debug runtime regression.

### Phase 6: Validation
- [ ] Run benchmark before and after and compare timing breakdown.
- [ ] Confirm output quality metrics are not degraded.
- [ ] Confirm determinism metrics remain stable.

## Definition Of Done
- Benchmark JSON includes stage timings.
- Release benchmark warm run is <= 30 seconds.
- Debug and test profile runtime is substantially improved with documented targets.
- Accuracy tests still pass and no major metric regressions are introduced.

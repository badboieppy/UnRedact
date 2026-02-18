# Benchmark Metrics Improvement Plan

Date: 2026-02-17
Owner: Codex

## Scope
Analyze and improve benchmark metrics in `guess_accuracy_benchmark` across:
- Performance
- Accuracy
- Consistency

## Baseline (before changes)
Source: `benchmark/guess_accuracy_before.json`

- Overall:
  - recall@1: 0.1667
  - recall@5: 0.1667
  - recall@20: 0.5000
  - mrr: 0.1980
  - mean_rank_found: 23.60
- EFTA00038617:
  - found_items: 8/10
  - recall@1: 0.0
  - recall@5: 0.0
  - recall@20: 0.4
  - mean_rank_found: 29.25
- Timing (overall):
  - redactions_ms: 228.0
  - fonts_ms: 9.5
  - guess_ms: 279.0
  - total_ms: 517.0
- Consistency:
  - hashes_identical: true
  - top1_agreement_ratio: 1.0
  - top5_jaccard_mean: 1.0

## Root Cause Analysis

### Accuracy
Primary issue: scoring assumed a single redacted span between anchors.

Observed in `EFTA00038617` page 2 (served names list):
- Multiple redaction boxes shared the same broad anchor pair.
- Anchor gap was much larger than individual box widths.
- Candidate scoring based on right-anchor closure was mis-modeled for those rows.
- Cluster consensus then amplified frequent but incorrect candidates.

### Performance
- Warm benchmark runtime already under target (< 30s) by a wide margin.
- Main cost in runtime diagnostics is guess stage.
- Accuracy fixes must avoid large new per-candidate render/scoring loops.

### Consistency
- Already strong and deterministic; no unstable rows observed.

## Implementation Plan

### Plan A: Span-aware scoring model (Accuracy)
1. Detect likely multi-span rows using `gap_ratio = anchor_gap / redaction_width`.
2. Use hybrid score:
   - single-span rows: anchor-right fit + small box-width prior.
   - multi-span rows: box-width fit + small anchor prior.
3. Add mode-specific candidate filtering:
   - single-span: anchor-error threshold.
   - multi-span: box-width threshold.

### Plan B: Consensus safety (Accuracy)
1. Exclude multi-span rows from cluster consensus.
2. Keep cluster consensus for likely single-span rows.
3. Preserve deterministic ordering.

### Plan C: Performance guardrails
1. Reuse per-font candidate width caches (existing mechanism).
2. Ensure no candidate rendering loops were introduced.
3. Re-measure benchmark warm runtime and stage timings.

### Plan D: Validation and non-regression
1. Run `efta00101126_guessing`.
2. Run `efta00038617_guessing`.
3. Run benchmark with repeats=3 and compare before/after metrics.

## Detailed TODOs

### Accuracy TODOs
- [x] Add gap-ratio constants and span-mode thresholds.
- [x] Implement span-aware raw error calculation in `build_guess_for_anchor`.
- [x] Implement mode-specific prefilters for candidates.
- [x] Keep punctuation/context penalty in scoring pipeline.
- [x] Keep deterministic sort keys after score updates.

### Consensus TODOs
- [x] Skip cluster consensus when gap ratio indicates multi-span row.
- [x] Keep consensus ordering for single-span rows.
- [x] Keep row-sequence consensus deterministic.

### Performance TODOs
- [x] Keep cache use in width scoring.
- [x] Avoid visual per-candidate reranking loops.
- [x] Re-run warm benchmark timing and verify under target.

### Validation TODOs
- [x] `cargo test -q --test efta00101126_guessing`
- [x] `cargo test -q --test efta00038617_guessing`
- [x] `cargo run --release --quiet --bin guess_accuracy_benchmark -- --out benchmark/guess_accuracy_after_tuned.json --repeats 3 --consistency-out benchmark/guess_consistency_after_tuned.json`

## Results (after changes)
Source: `benchmark/guess_accuracy_after_tuned.json`

- Overall:
  - found_items: 12/12 (was 10/12)
  - recall@1: 0.1667 (unchanged)
  - recall@5: 0.3333 (was 0.1667)
  - recall@20: 0.6667 (was 0.5000)
  - mrr: 0.2430 (was 0.1980)
  - mean_rank_found: 15.75 (was 23.60)
- EFTA00038617:
  - found_items: 10/10 (was 8/10)
  - recall@5: 0.2 (was 0.0)
  - recall@20: 0.6 (was 0.4)
  - mrr: 0.0920 (was 0.0376)
  - mean_rank_found: 18.70 (was 29.25)
- Consistency:
  - unchanged, still fully deterministic.
- Performance:
  - warm benchmark remains well under target (< 30s), measured around ~3.6s for repeats=3.
  - overall guess stage improved from 279.0 ms to 272.5 ms.
  - overall orchestrator total improved from 517.0 ms to 505.5 ms.

## Remaining Gaps
- Top-1 ranking for EFTA00038617 remains weak (recall@1 still 0.0).
- Next likely improvement path:
  - add row-level joint assignment across contiguous redactions,
  - optionally use low-cost visual tie-breakers only for top-N ambiguous candidates.

# Stage-Decoupled Benchmarking

This benchmark flow is split into two measured stages, then rolled up for visibility:

1. `redaction_accuracy_benchmark` (detector quality)
2. `anchor_accuracy_benchmark` (anchor resolver quality)
3. `combined_accuracy_benchmark` (runs both stages, then publishes the weighted summary)

## Service Interaction Model

The anchor stage is intentionally independent of detector output generation.

- Redaction stage path:
  - Input: `PDF`
  - Execution: detector service path
  - Output: `RedactionReport`
- Anchor stage path:
  - Input: `PDF bytes + RedactionReport`
  - Execution: `run_anchor_from_redactions(...) -> AnchorReport`
  - Output: `AnchorReport`

The full guessing pipeline still works end-to-end, but anchor benchmarking now calls the anchor resolver path directly so detector quality and anchor quality can be tracked separately.

## Default Entry Point

Use `combined_accuracy_benchmark` when you want the normal benchmark run. It executes the redaction and anchor stage benchmarks itself, writes `redaction_benchmark_report.json` and `anchor_benchmark_report.json` into the output directory, then writes `combined_benchmark_report.json`. Each report also gets a matching Markdown summary sidecar (`*.summary.md`). You should not have to pass stage artifact paths around manually.

## Accuracy Semantics

### Redaction Accuracy
- Recall / precision / F1 from IoU-based matching against curated ground truth.
- Matched IoU median and p90.
- Page-level count error (mean absolute error).

### Anchor Accuracy
- Curated mode (headline):
  - Uses locked redaction inputs.
  - Measures selected-row ratio, anchor mode match ratio, side text recall, and side x-position error.
  - Headline score is curated-only.
- Synthetic mode (diagnostic):
  - Uses deterministic seeded synthetic redactions.
  - Reports robustness telemetry and trends only.

## Reporting Policy

- No hard pass/fail thresholds.
- Always report:
  - absolute metrics,
  - baseline deltas,
  - trend direction.
- Rollup score is visibility-only and does not replace stage-level diagnosis.

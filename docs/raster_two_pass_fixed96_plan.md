# Raster Redaction Performance Plan: Always Two-Pass + Fixed 96 DPI Highpass

## Objective
Reduce raster redaction compute time by enforcing a deterministic two-pass raster policy:

1. **Always run two-pass raster detection** (remove all single-pass branches).
2. Use a **tiny fixed prepass DPI** for presence detection.
3. Use a **fixed 96 DPI highpass** on candidate pages only.

This work is intentionally scoped to these two changes only.

## Why This Change
Current redaction stage runtime is dominated by raster rendering and pixel analysis. Prior code had single-pass fallback branches that rendered all pages at higher effective DPI for small docs, causing large runtime spikes.

## Baseline (Before Change)
Captured with:

```powershell
cargo run --quiet --bin guess_accuracy_benchmark -- --out benchmark/guess_accuracy_before_fixed_twopass96.json
```

Key baseline metrics:

- `OVERALL timing redactions_ms=15732.0`
- `OVERALL timing total_ms=15955.5`
- `OVERALL r@1=33.3% r@5=58.3% r@20=100.0% mrr=0.424`
- `EFTA00101126 redactions_ms=21206.0`
- `EFTA00038617 redactions_ms=10258.0`

## Design

### Redaction Scan Policy
In `redaction_impl::collect_raster_redactions_two_pass`:

- Prepass DPI: fixed constant `RASTER_PREPASS_DPI = 18.0`
- Highpass DPI: fixed constant `RASTER_HIGHPASS_DPI = 96.0`
- Run prepass on **all pages**.
- Candidate pages = pages with non-empty prepass detections.
- Run highpass only on candidate pages.
- If highpass returns empty/error for a candidate page, preserve prepass detections for that page.
- Remove all single-pass mode branches.

### 96 DPI Stability Adjustment
Fixed 96 DPI introduces more width quantization noise for raster-detected boxes than 120 DPI.
To keep ranking stable without per-file tuning:

- Quantize raster region bounds conservatively when mapping normalized rectangles to pixels
  (`floor` for min edges, `ceil` for max edges).
- Apply a raster-only width noise tolerance in guess scoring
  (`RASTER_WIDTH_NOISE_PT = 2.50`) so small raster width drift is treated as measurement noise,
  not strong evidence against a candidate.
- Prepass DPI sweep validated that `18.0` retains detection coverage in regression tests, while
  `<=10.0` started dropping expected detections in `EFTA00101126`.

### Diagnostics
Keep/update one diagnostic line summarizing:

- page counts
- candidate/non-candidate counts
- fixed prepass/highpass DPIs
- requested DPI (for audit visibility)
- timings and fallback counts

### Ownership Pattern
No ownership boundary changes. Keep existing layering:

- Service -> Logic (`redaction_guessing_component`)
- Logic -> Data (`RedactionDataRetriever` abstraction)
- Data -> Dependency (`pdf_redaction_accessor` rendering + detection)

## Testing Plan

### Unit Tests (Redaction Logic)
Add tests in `redaction_impl` test module to verify:

1. Two-pass uses fixed DPIs and candidate-only highpass calls.
2. Small docs do not enter single-pass paths.
3. Diagnostics reflect two-pass behavior and no `single_pass` mode text.

### Regression Suite
Run:

```powershell
cargo test -q --lib
cargo test -q --test efta00101126_guessing
cargo test -q --test efta00038617_guessing
cargo test -q --test raster_api
cargo test -q
```

### Benchmark Validation (After Change)
Run and compare against baseline:

```powershell
cargo run --quiet --bin guess_accuracy_benchmark -- --out benchmark/guess_accuracy_after_fixed_twopass96.json
```

Evaluation criteria:

- Performance: `timing_redactions_ms` and `timing_orchestrator_total_ms` should improve.
- Accuracy: dataset and overall `r@1/r@5/r@20/mrr` should not regress.
- Consistency: benchmark consistency hashes should remain stable.

## TODO Checklist

- [x] Capture baseline benchmark JSON.
- [x] Refactor raster scan to always two-pass (remove single-pass branches).
- [x] Introduce fixed DPIs: prepass tiny, highpass 96.
- [x] Preserve existing fallback from highpass miss/error to prepass hits on candidate pages.
- [x] Update two-pass diagnostics to reflect fixed policy.
- [x] Add unit tests for fixed two-pass call pattern and no-single-pass behavior.
- [x] Run formatting + full test suite.
- [x] Capture after-change benchmark JSON.
- [x] Compare before/after metrics and record outcomes.
- [x] Summarize results for user (accuracy + performance + consistency).

## Results (After Change)

After benchmark command:

```powershell
cargo run --quiet --bin guess_accuracy_benchmark -- --out benchmark/guess_accuracy_after_fixed_twopass96.json
```

Before vs after:

- `OVERALL timing redactions_ms`: `15732.0 -> 8702.0` (improved)
- `OVERALL timing total_ms`: `15955.5 -> 8887.0` (improved)
- `OVERALL r@1`: `33.3% -> 33.3%` (no regression)
- `OVERALL r@5`: `58.3% -> 66.7%` (improved)
- `OVERALL r@20`: `100.0% -> 100.0%` (no regression)
- `OVERALL mrr`: `0.424 -> 0.456` (improved)

Dataset checks:

- `EFTA00101126` redactions timing improved (`21206.0 -> 8574.0`) with accuracy unchanged.
- `EFTA00038617` redactions timing improved (`10258.0 -> 8830.0`) and ranking improved (`r@5 50.0% -> 60.0%`, `mrr 0.308 -> 0.347`).

Consistency:

- `CONSISTENCY hashes_identical=true`
- `top1_agree=1.000`, `top5_jaccard=1.000`, `unstable_rows=0`

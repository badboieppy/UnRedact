# Determinism and Anchor Alignment Incident Probe
Date: 2026-03-02

## Scope
User-reported issues:
- anchor/guess visualization does not line up with redactions,
- multi-line/overlap behavior is broken,
- overall accuracy is very low,
- request to render and inspect visualized outputs for:
  - `test_data/EFTA00038617.pdf`
  - `test_data/EFTA00101126.pdf`

## Commands Executed
1. Generate visualized artifacts
- `cargo run --bin unredact-cli --release -- test_data/EFTA00038617.pdf --output-dir .local-agent/runtime/investigations/determinism_probe/EFTA00038617 --visualize`
- `cargo run --bin unredact-cli --release -- test_data/EFTA00101126.pdf --output-dir .local-agent/runtime/investigations/determinism_probe/EFTA00101126 --visualize`

2. Render visualized PDFs to PNG
- `cargo run --bin pdf_to_png --release -- .local-agent/runtime/investigations/determinism_probe/EFTA00038617/EFTA00038617.visualized.pdf --all-pages --output-dir .local-agent/runtime/investigations/determinism_probe/EFTA00038617/png_visualized --dpi 200`
- `cargo run --bin pdf_to_png --release -- .local-agent/runtime/investigations/determinism_probe/EFTA00101126/EFTA00101126.visualized.pdf --all-pages --output-dir .local-agent/runtime/investigations/determinism_probe/EFTA00101126/png_visualized --dpi 200`

3. Render original PDFs to PNG for comparison
- `cargo run --bin pdf_to_png --release -- test_data/EFTA00038617.pdf --all-pages --output-dir .local-agent/runtime/investigations/determinism_probe/EFTA00038617/png_original --dpi 200`
- `cargo run --bin pdf_to_png --release -- test_data/EFTA00101126.pdf --all-pages --output-dir .local-agent/runtime/investigations/determinism_probe/EFTA00101126/png_original --dpi 200`

4. Determinism hash check (repeat runs)
- Same input/config executed twice per PDF into:
  - `.local-agent/runtime/investigations/determinism_probe/repeatability/<pdf>/run_a`
  - `.local-agent/runtime/investigations/determinism_probe/repeatability/<pdf>/run_b`
- Hash results written to:
  - `.local-agent/runtime/investigations/determinism_probe/repeatability/repeatability_hashes.json`

5. Accuracy benchmark snapshot
- `cargo run --bin guess_accuracy_benchmark --release -- --out .local-agent/runtime/investigations/determinism_probe/current_guess_accuracy.json`

## Visual Inspection Findings
### EFTA00038617
- Visualized pages show heavy text overlap around redaction rows on p002/p003.
- Multiple redactions on the same textual line reuse distant anchors; overlays include contextual anchor words that stack over nearby rows.
- Example behavior: same left anchor text reused across adjacent redactions while left anchor x is >120 pt from redaction edge.

### EFTA00101126
- p008 alignment around actual black bars is comparatively stable.
- p002 contains clear false-positive raster redactions over red UI buttons (`Print`, `Save As...`, `Reset`).
- Resulting overlays are "wrong" relative to user intent because these are not true redactions.

## Artifact Evidence
- Anchor reuse anomaly report:
  - `.local-agent/runtime/investigations/determinism_probe/anchor_reuse_anomaly_report.json`
- Multiline top-guess check:
  - `.local-agent/runtime/investigations/determinism_probe/multiline_guess_report.json`

Key values from `anchor_reuse_anomaly_report.json`:
- `EFTA00038617`: `selected_rows=29`, `far_anchor_rows=6` (left/right anchor gap >120 pt)
- `EFTA00101126`: `selected_rows=8`, `far_anchor_rows=0`

Key values from `multiline_guess_report.json`:
- no top candidate in either file contained embedded newlines (`multiline_top_guess_rows=0` for both files)
- current overlap on these two files is not from newline-containing top guess strings; it is from anchor/context placement behavior and false-positive redaction regions.

## Determinism Findings
- Raw file hashes differed between run A/B for both PDFs.
- After normalizing out diagnostics/timing fields, guesses and anchors payloads matched exactly for both files.
- Interpretation: deterministic decision content, nondeterministic diagnostics/timing strings.

## Benchmark Snapshot
From `.local-agent/runtime/investigations/determinism_probe/current_guess_accuracy.json`:
- `OVERALL recall_at_1=0.0000`
- `OVERALL recall_at_5=0.1818`
- `OVERALL recall_at_20=0.4545`
- `OVERALL mrr=0.0854`
- `DETERMINISM_GATE passed=true`

## Root-Cause Candidates (code-trace)
1. Shared row-cluster hint mixing in anchor selection:
- `build_shared_context_hints` merges line-cluster hints across linked redaction rows.
- `select_cluster_hint` allows large edge gaps (`CLUSTER_ANCHOR_HINT_MAX_GAP_PT=240`) and can select non-local anchors when rows are dense.
- Affected file: `src/logic/redaction_guessing_component/guess_logic.rs`.

2. Dense-row visualization still injects anchor words:
- Even when dense rows force selected-only pair behavior, selected-only overlay rendering still prefixes/suffixes anchor text.
- This increases overlap in packed multi-redaction lines.
- Affected file: `src/data/visualization_data.rs`.

3. Raster false positives on non-black UI elements:
- Red UI buttons are detected as raster dark redactions in `EFTA00101126` p002.
- Affected area: raster redaction detection heuristics in `src/data/redaction_scan_data.rs` and dependency raster implementation.

## Open Technical Unknowns
- Which redaction classes should be excluded from visualization overlays by default (e.g., raster-dark-only with weak context)?
- Whether dense-row visual overlays should render selected guess only (no anchor words) or be configurable.
- Whether cluster hint max-gap and local-hint weighting should be hard-gated per mode (`two_sided`, `left_only`, `right_only`).
- Whether diagnostics nondeterminism should be normalized at emission to preserve hash stability on full artifacts.

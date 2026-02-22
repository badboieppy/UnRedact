# Complexity Delta (Before vs After)

Date: 2026-02-22

## Sources
- Baseline (before simplification):
  - `docs/generated/deep_analysis_baseline_2026-02-22/deep_simplification_analysis.md`
  - `docs/generated/deep_analysis_baseline_2026-02-22/conditions_inventory.md`
  - `docs/generated/deep_analysis_baseline_2026-02-22/call_patterns_inventory.md`
- Current (after simplification):
  - `docs/deep_simplification_analysis.md`
  - `docs/generated/deep_analysis/conditions_inventory.md`
  - `docs/generated/deep_analysis/call_patterns_inventory.md`
- Secondary current snapshot:
  - `benchmark/complexity_snapshot.json`

## Method
- Before/after deltas below are based on the same deep-analysis generator (`tmp/generate_deep_analysis.ps1`) to keep comparisons apples-to-apples.
- `complexity_snapshot.json` is included as an additional fast snapshot, but it uses different heuristics and should not be mixed directly with deep-analysis counts.

## Summary Delta

| Metric | Before | After | Delta |
|---|---:|---:|---:|
| Compile features | 5 | 5 | 0 |
| `cfg` mentions | 31 | 29 | -2 |
| Clap-defined flags | 17 | 19 | +2 |
| Runtime `--flag` mentions | 24 | 5 | -19 |
| Env var mentions | 4 | 4 | 0 |
| Total condition lines | 913 | 902 | -11 |
| Rust condition lines | 782 | 771 | -11 |
| JS condition lines | 131 | 131 | 0 |
| Rust public export lines | 214 | 199 | -15 |
| Rust cross-file import lines | 101 | 96 | -5 |
| JS function interface lines | 79 | 79 | 0 |
| HTML UI IDs | 19 | 19 | 0 |
| Files covered | 62 | 63 | +1 |
| Rust import-derived call edges | 123 | 120 | -3 |
| Layer-to-layer edge pairs | 23 | 20 | -3 |

## Interpretation
- Complexity decreased overall, but not dramatically:
  - Condition count dropped by 11 lines.
  - Public interface/import coupling dropped (`exports -15`, `imports -5`).
  - Layer call variety dropped (`23 -> 20`).
- The biggest visible simplification win is CLI/runtime flag surface:
  - Runtime flag mentions dropped from 24 to 5.
- Clap-defined flags increased by 2 because `guess_accuracy_benchmark` moved from manual parsing to clap (fewer condition branches, but now represented as typed clap flags).

## Condition Hotspot Delta (Top Changes)

| File | Before | After | Delta |
|---|---:|---:|---:|
| `src/bin/guess_accuracy_benchmark.rs` | 57 | 49 | -8 |
| `src/dependency/pdf_font_occurrence_accessor.rs` | 28 | 26 | -2 |
| `src/bin/visual_score_impact_benchmark.rs` | 44 | 43 | -1 |
| `src/dependency/file_store.rs` | 5 | 4 | -1 |
| `src/service/tooling_entry.rs` | 0 | 1 | +1 |

Files with no change but still major hotspots:
- `src/logic/redaction_guessing_component.rs`: `356 -> 356`
- `src/dependency/pdf_redaction_accessor.rs`: `116 -> 116`
- `src/dependency/pdf_font_run_accessor.rs`: `59 -> 59`
- `web/app.js`: `82 -> 82`

## Layer-Call Pattern Delta (Largest Shifts)

| Layer Edge | Before | After | Delta |
|---|---:|---:|---:|
| `entry_bin -> data` | 3 | 0 | -3 |
| `entry_bin -> service` | 1 | 4 | +3 |
| `dependency -> dependency` | 2 | 0 | -2 |
| `service -> types` | 4 | 2 | -2 |
| `service -> data` | 0 | 2 | +2 |
| `logic -> data` | 8 | 9 | +1 |
| `entry_bin -> dependency` | 1 | 0 | -1 |
| `entry_bin -> logic` | 1 | 0 | -1 |
| `entry_bin -> types` | 5 | 4 | -1 |
| `service -> logic` | 4 | 5 | +1 |

Notable structural outcome:
- Bin targets now route through `service` more consistently (`entry_bin -> service` up, direct `entry_bin -> data/dependency/logic` down).

## Secondary Snapshot (Current Only)
From `benchmark/complexity_snapshot.json`:
- compile_features: 5
- cfg_mentions: 27
- runtime_flag_mentions: 4
- env_var_mentions: 33
- public_export_lines: 443
- condition_lines: 833

Note:
- These values are useful for quick trend monitoring, but are computed with a different scanner than deep-analysis inventories and should not be directly subtracted against the baseline deep-analysis metrics.

## Next Refactor Pass (Current Turn)

Compared snapshots:
- Before: `benchmark/complexity_snapshot_before_next_refactor.json`
- After: `benchmark/complexity_snapshot_after_next_refactor.json`

| Metric | Before | After | Delta |
|---|---:|---:|---:|
| Compile features | 5 | 5 | 0 |
| `cfg` mentions | 27 | 27 | 0 |
| Runtime flag mentions | 4 | 4 | 0 |
| Env var mentions | 33 | 33 | 0 |
| Public export lines | 443 | 442 | -1 |
| Condition lines | 833 | 814 | -19 |

Top condition hotspots after this pass:
- `src/logic/redaction_guessing_component.rs` (319)
- `src/dependency/pdf_redaction_accessor.rs` (103)
- `web/app.js` (69)

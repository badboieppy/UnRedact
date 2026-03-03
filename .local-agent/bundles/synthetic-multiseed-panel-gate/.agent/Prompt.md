# Bundle Prompt

## Bundle
- Slug: synthetic-multiseed-panel-gate
- Intent: neutral

## Objective
Add required synthetic multi-seed average panel evaluation with N>=20 and accepted tolerance deltas.

## Value
Provides durable over-time signal while preserving deterministic binding checks and reducing overreaction to single-seed variance.

## Scope In
- Emit required multi-seed average panel diagnostics on every benchmark run.
- Evaluate panel deltas against accepted tolerances with margin policy.
- Keep fixed-seed determinism checks binding and explicit.
- Record sample-size and tolerance calculations used by gate outputs.

## Scope Out
- No anchor resolver policy changes.
- No output publication surface changes.
- No documentation-only governance updates outside panel behavior.

## Prerequisites
- Synthetic protocol root contract remains active.
- Minimum panel size accepted at N>=20.
- Tolerance policy for panel deltas is fixed.

## Prerequisites And Contracts
### Consumed Contracts
- CON-002
- CON-021
- CON-028

### Introduced Contracts
- synthetic_multiseed_panel_contract

## Doctrine Constraints
- Doctrine path: D:\Development\RandomProjects\Agents\doctrines\engineering_doctrine.md
- Architecture boundaries must remain strict Service -> Logic -> Data -> Dependency.
- Boundary type and error translation must be explicit at every adjacent layer crossing.
- Public interfaces must stay minimal; compatibility decisions must be explicitly recorded.

## Validation Intent
### Required Commands
- cargo test
- cargo run --bin guess_accuracy_benchmark --release
- cargo run --bin synthetic_overfitting_benchmark --release
- cargo run --bin visual_score_impact_benchmark --release
- cargo run --bin evidence_first_change_gate --release

### Quality Gates
- All required validation commands complete successfully.
- No core metric regression beyond accepted tolerance bands for neutral intent.
- Panel section must be present with seed_count >= 20 and explicit threshold deltas.

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers and consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
Synthetic benchmark report includes a required average panel with at least 20 seeds, reports delta thresholds, and emits deterministic pass/fail outcome.

## Expected Evidence Artifacts
- synthetic benchmark report with multi-seed panel section
- panel threshold decision fields
- validation evidence log entries

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
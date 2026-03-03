# Overview
Generated: 2026-03-02
Phase: 4 (Bundle Files Materialized)
Input Doc: `docs/anchor_geometry_reliability_design.md`

## End Goal Summary
Make anchor geometry decisions deterministic, explicitly sourced, and fully traceable, while preserving current benchmark governance and extending it with bundle-intent-aware hard thresholds, multi-seed synthetic averaging, and rollback-on-regression evidence discipline.

## Current vs Target
Current:
- Anchor geometry can come from mixed run/hint paths with incomplete machine-readable decision provenance.
- `guesses.json` has partial anchor summaries but no canonical candidate-level decision schema with stable reason enums.
- `anchors.json` artifact does not exist as a first-class output for CLI/web parity.
- Benchmark hard gates are uneven across binaries, and synthetic random-seed averaging is not a binding thresholded panel.

Target:
- Canonical anchor decision schema with explicit per-side source label, per-candidate single reason code, and `was_selected`.
- Full candidate decision/rejection emission in `anchors.json`, with compact per-side anchor summary retained in guesses.
- Deterministic source semantics:
  - `run_exact` preserves punctuation/newlines and uses format-only normalization.
  - `run_prefix_projection` validity/provenance is explicit and confidence-aware.
- Bundle-intent-aware hard thresholds with 1% margin band, no-retry rollback-on-regression policy, and required multi-seed synthetic panel (`N>=20`) thresholds.

## Contract Catalog (Planning Scope)
- CON-001: `C-KNOWN-REDACTION-TARGETS-V1`
- CON-002: `C-SYNTHETIC-SEED-TIERS`
- CON-003: `C-INVESTIGATIVE-PROOF-FIRST`
- CON-004: `C-BEST-FEASIBLE-K-RULE`
- CON-005: `C-BASELINE-NO-REGRESSION`
- CON-006: `context_spans_json_v1`
- CON-007: current guess schema contract surface
- CON-008: current output artifact naming surface
- CON-011..CON-017: previously accepted user governance directives
- CON-018..CON-029: round-4/round-5 source, reason, confidence, multi-seed, and rollback directives

## Bundle Sequence Overview
1. `anchor-contract-schema-foundation`
- Purpose: define canonical anchor/source/reason/confidence schema and migration boundaries (breaking schema allowed).
- Purpose Demo: generated anchor decision record shows `was_selected`, single reason code, and per-side confidence fields in `0.0..1.0`.
- Why <=500 LOC: mostly type/schema surfaces and serialization wiring.
- Main risk: schema drift between anchor and guess artifacts.
- Contracts: consumes CON-007, CON-011, CON-019, CON-025, CON-026; introduces `anchor_decision_schema`.

2. `run-source-resolution-contract`
- Purpose: lock `run_exact` and `run_prefix_projection` semantics with explicit normalization/provenance and deterministic validity rules.
- Purpose Demo: same row emits identical source labels and x-provenance across repeated runs with clear offset source.
- Why <=500 LOC: concentrated to anchor resolver and associated typed metadata.
- Main risk: accidental metric movement from changed source precedence.
- Contracts: consumes CON-018, CON-023, CON-024; introduces `anchor_source_resolution_contract`.

3. `anchor-candidate-trace-completeness`
- Purpose: emit complete candidate set decisions/rejections into `anchors.json` with single terminal reason per candidate.
- Purpose Demo: for a row with fallback, artifact includes selected candidate plus all rejected alternatives and reason codes.
- Why <=500 LOC: focused emission path and reason-code mapping tables.
- Main risk: large artifact growth and serialization regressions.
- Contracts: consumes CON-012, CON-019, CON-024, CON-026; introduces `anchor_candidate_trace_contract`.

4. `artifact-surface-abstraction-cli-web`
- Purpose: add `anchors.json` as first-class output in both CLI and web paths via shared service/output abstraction.
- Purpose Demo: same input through CLI and web yields equivalent anchor artifact content and naming semantics.
- Why <=500 LOC: output path/type structs + publisher adaptation only.
- Main risk: parity mismatch between file and in-memory/web delivery surfaces.
- Contracts: consumes CON-008, CON-016, CON-027; introduces `anchor_artifact_delivery_parity`.

5. `guess-anchor-compact-linkage`
- Purpose: preserve quick-read anchor info in guesses using compact per-side summary linked to anchor identity.
- Purpose Demo: a guess row shows per-side selected source, type, confidence, and side-linked anchor context without reading full anchor artifact.
- Why <=500 LOC: guess schema/output mapper update plus compatibility checks.
- Main risk: duplicated fields drifting from canonical anchor artifact.
- Contracts: consumes CON-015, CON-020, CON-027; introduces `guess_anchor_compact_summary_contract`.

6. `benchmark-intent-threshold-engine`
- Purpose: make hard pass/fail machine-checkable by bundle intent (`improve` vs `neutral`) with 1% margin bands.
- Purpose Demo: neutral bundle gate rejects metric drop beyond allowed tolerance; improve bundle requires expected improvement where declared.
- Why <=500 LOC: benchmark gate evaluator and metadata interpretation updates.
- Main risk: false positives if tolerance application is inconsistent across metrics.
- Contracts: consumes CON-017, CON-022, CON-028; introduces `intent_aware_threshold_gate_contract`.

7. `synthetic-multiseed-panel-gate`
- Purpose: add required multi-seed average diagnostics and binding threshold evaluation at `N>=20`.
- Purpose Demo: benchmark report includes fixed-seed gate plus `N>=20` panel averages with threshold deltas and pass/fail.
- Why <=500 LOC: synthetic benchmark options/report section + gate checks.
- Main risk: runtime increase and threshold calibration instability.
- Contracts: consumes CON-002, CON-021, CON-028; introduces `synthetic_multiseed_panel_contract`.

8. `regression-response-rollback-protocol`
- Purpose: enforce no-retry regression response workflow: investigate, rollback, and emit root-cause evidence package.
- Purpose Demo: failing regression path produces explicit investigation/rollback evidence artifact with discovered root cause.
- Why <=500 LOC: orchestration/governance layer and evidence packaging hooks.
- Main risk: rollback detection boundaries and operational ergonomics.
- Contracts: consumes CON-003, CON-022, CON-029; introduces `regression_action_protocol`.

9. `baseline-refresh-and-governance-docs`
- Purpose: codify baseline refresh rules, accepted command matrix, and artifact interpretation guidance after contract changes.
- Purpose Demo: governance docs map each command to required artifact outputs and hard gate interpretation with margin semantics.
- Why <=500 LOC: documentation and benchmark metadata surfaces only.
- Main risk: documentation lag relative to implemented contract behavior.
- Contracts: consumes CON-013, CON-014, CON-017, CON-028, CON-029; introduces `baseline_governance_contract`.

## Purpose Demo One-Liners
- `anchor-contract-schema-foundation`: one row fully explains selected vs rejected candidates in machine-readable form.
- `run-source-resolution-contract`: source labels and coordinates are reproducible and provenance-tagged.
- `anchor-candidate-trace-completeness`: every discarded candidate has a terminal reason code.
- `artifact-surface-abstraction-cli-web`: CLI/web produce equivalent anchor artifact payload semantics.
- `guess-anchor-compact-linkage`: guesses remain quickly interpretable without opening full anchors artifact.
- `benchmark-intent-threshold-engine`: gate behavior reflects declared bundle intent and tolerance rules.
- `synthetic-multiseed-panel-gate`: synthetic report always includes `N>=20` average panel with threshold deltas.
- `regression-response-rollback-protocol`: regression handling is deterministic, evidence-backed, and no-retry.
- `baseline-refresh-and-governance-docs`: baseline update policy and pass/fail semantics are auditable and explicit.

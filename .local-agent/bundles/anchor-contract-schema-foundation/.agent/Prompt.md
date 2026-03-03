# Bundle Prompt

## Bundle
- Slug: anchor-contract-schema-foundation
- Intent: neutral

## Objective
Define the canonical anchor decision schema with deterministic source labels, one terminal decision reason code, and per-side plus row confidence values.

## Value
Makes anchor decisions machine-parseable and trace-complete without inferred behavior from free-form diagnostics.

## Scope In
- Define anchor decision record types and enums required by the canonical contract.
- Represent was_selected with one AnchorSelectionReasonCode per candidate.
- Represent left_anchor_confidence, right_anchor_confidence, and row_anchor_confidence in range 0.0..1.0.
- Add compatibility decision record for schema breakage policy and migration posture.

## Scope Out
- No resolver behavior changes.
- No benchmark gate threshold changes.
- No delivery-surface abstraction changes for CLI and web.

## Prerequisites
- Canonical files are current under .local-agent/series.
- Breaking schema policy remains accepted.
- Contract-change policy remains strict and explicit.

## Prerequisites And Contracts
### Consumed Contracts
- CON-007
- CON-011
- CON-019
- CON-025
- CON-026

### Introduced Contracts
- anchor_decision_schema

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
- No architecture boundary violations against service, logic, data, dependency doctrine.

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers and consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
A single row in the anchor artifact includes selected and rejected candidates, each with was_selected and one terminal reason code, and shows left/right/row confidence values in 0.0..1.0.

## Expected Evidence Artifacts
- anchors artifact schema output
- compatibility decision note in bundle documentation
- validation evidence log entries

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
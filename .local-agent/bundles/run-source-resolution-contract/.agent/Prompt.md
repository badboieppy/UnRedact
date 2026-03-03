# Bundle Prompt

## Bundle
- Slug: run-source-resolution-contract
- Intent: improve

## Objective
Lock deterministic source-resolution semantics for run_exact and run_prefix_projection with explicit provenance and newline and punctuation preserving exactness.

## Value
Eliminates silent coordinate-source ambiguity and aligns source labels with actual coordinate derivation behavior.

## Scope In
- Apply exact-match comparison policy that preserves punctuation and newline boundaries.
- Apply prefix-projection validity contract and offset provenance precedence.
- Emit deterministic source label and offset provenance required for confidence attribution.
- Translate errors at each layer boundary when introducing new resolver error surfaces.

## Scope Out
- No artifact delivery surface changes for CLI and web.
- No benchmark-threshold policy changes.
- No documentation-only governance updates.

## Prerequisites
- Canonical schema fields for source and provenance are available.
- Prefix-projection validity contract is fixed as delegated policy.
- Determinism requirement remains binding.

## Prerequisites And Contracts
### Consumed Contracts
- CON-018
- CON-023
- CON-024

### Introduced Contracts
- anchor_source_resolution_contract

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
- At least one declared accuracy metric improves for improve intent, with no out-of-band regression.
- Regression outside tolerance triggers immediate investigate-plus-rollback flow without retry.

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers and consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
Running the same input twice produces identical per-side source labels and coordinate provenance, and exact matches keep punctuation and newline sensitive text semantics.

## Expected Evidence Artifacts
- anchors artifact with explicit per-side source provenance
- row-level trace evidence for deterministic replay
- validation evidence log entries

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
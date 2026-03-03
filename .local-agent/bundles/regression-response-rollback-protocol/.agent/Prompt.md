# Bundle Prompt

## Bundle
- Slug: regression-response-rollback-protocol
- Intent: neutral

## Objective
Enforce deterministic no-retry regression handling with mandatory investigate, rollback, and evidence emission.

## Value
Prevents ambiguous failure handling and preserves a reliable audit trail for every hard regression event.

## Scope In
- Define hard-regression response flow with no retry path.
- Emit required investigation and root-cause evidence payload when rollback occurs.
- Ensure rollback boundaries are explicit and deterministic.
- Ensure error translation remains layer-local if rollback orchestration introduces new error paths.

## Scope Out
- No anchor source semantics changes.
- No schema redesign beyond response and evidence payload needs.
- No CLI and web artifact contract changes outside regression evidence outputs.

## Prerequisites
- Regression policy remains no-retry.
- Bundle-intent and threshold engine decisions are available for trigger conditions.
- Evidence-first governance remains required.

## Prerequisites And Contracts
### Consumed Contracts
- CON-003
- CON-022
- CON-029

### Introduced Contracts
- regression_action_protocol

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
- No-retry path enforced for hard regression outcomes.

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers and consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
An out-of-band regression event yields deterministic investigate-plus-rollback flow and writes a root-cause evidence artifact without retrying the failed run.

## Expected Evidence Artifacts
- regression investigation artifact
- rollback action record
- validation evidence log entries

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
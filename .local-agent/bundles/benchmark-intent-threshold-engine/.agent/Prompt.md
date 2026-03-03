# Bundle Prompt

## Bundle
- Slug: benchmark-intent-threshold-engine
- Intent: neutral

## Objective
Make benchmark hard-pass logic machine-checkable by declared bundle intent with 1 percent margin bands.

## Value
Ensures governance decisions are deterministic and auditable instead of interpretive.

## Scope In
- Implement intent-aware gate evaluation for improve and neutral expectations.
- Apply 1 percent margin-of-error bands to hard threshold boundaries.
- Preserve no-retry regression policy for out-of-band failures.
- Emit explicit gate decision traces with threshold math inputs and outputs.

## Scope Out
- No anchor resolver algorithm changes.
- No schema redesign outside gate metadata needed for evaluation.
- No output-surface publication abstraction changes.

## Prerequisites
- Bundle-intent governance contract is fixed.
- Accepted threshold values and margin policy are fixed.
- No-retry rollback policy remains binding.

## Prerequisites And Contracts
### Consumed Contracts
- CON-017
- CON-022
- CON-028

### Introduced Contracts
- intent_aware_threshold_gate_contract

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
- Gate decision includes explicit intent, threshold, margin, and outcome fields.

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers and consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
Gate output for a neutral task shows threshold comparison with margin band and fails deterministically when a metric crosses out of tolerated range.

## Expected Evidence Artifacts
- gate decision artifact with explicit threshold math
- bundle-intent metadata trace
- validation evidence log entries

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
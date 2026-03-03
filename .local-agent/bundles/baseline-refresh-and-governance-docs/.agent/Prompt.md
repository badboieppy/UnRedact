# Bundle Prompt

## Bundle
- Slug: baseline-refresh-and-governance-docs
- Intent: neutral

## Objective
Codify baseline governance, command matrix, compatibility decisions, and hard-gate interpretation rules in end-state documentation.

## Value
Keeps acceptance criteria and operational policy explicit, reproducible, and reviewable over time.

## Scope In
- Document required validation command matrix and artifact expectations.
- Document baseline lock and update policy and threshold interpretation with margin policy.
- Document compatibility decisions and removal posture for changed interfaces.
- Document waiver process expectations for doctrine exceptions.

## Scope Out
- No runtime algorithm changes.
- No schema behavior changes.
- No output publication behavior changes.

## Prerequisites
- Current baseline artifacts are available for reference.
- Current threshold and regression policy decisions are fixed.
- Doctrine path remains authoritative for engineering constraints.

## Prerequisites And Contracts
### Consumed Contracts
- CON-013
- CON-014
- CON-017
- CON-028
- CON-029

### Introduced Contracts
- baseline_governance_contract

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
- All required validation commands remain documented as mandatory.
- No policy contradiction with current contract ledger entries.
- No core metric regression beyond accepted tolerance bands for neutral intent.

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers and consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
Governance documentation maps every required command to expected outputs and pass/fail rules with margin and rollback semantics.

## Expected Evidence Artifacts
- governance documentation updates
- command-to-artifact mapping tables
- validation evidence log entries

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
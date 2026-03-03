# Bundle Prompt

## Bundle
- Slug: guess-anchor-compact-linkage
- Intent: neutral

## Objective
Retain compact per-side anchor context in guesses while preserving full trace detail in anchors artifact.

## Value
Keeps guess inspection fast while maintaining canonical trace depth in a dedicated artifact.

## Scope In
- Emit per-side compact fields in guesses: side-linked anchor identity context, anchor type, selected source, confidence.
- Keep compact fields aligned with canonical anchor artifact values.
- Preserve deterministic serialization order and stable field names.
- Record compatibility decision for guesses interaction-surface change.

## Scope Out
- No resolver decision-policy changes.
- No benchmark threshold changes.
- No multi-seed panel policy changes.

## Prerequisites
- Anchors artifact is available as canonical source of full decision records.
- Compact field set is fixed by contract.
- Anchor identity exposure contract remains active.

## Prerequisites And Contracts
### Consumed Contracts
- CON-015
- CON-020
- CON-027

### Introduced Contracts
- guess_anchor_compact_summary_contract

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
- Compact fields are present for all emitted guess rows with side anchors.

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers and consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
Each guess row contains the agreed compact per-side anchor fields and these values match the selected anchor records in anchors artifact for the same row.

## Expected Evidence Artifacts
- guesses artifact compact anchor fields
- alignment evidence between guesses and anchors artifact
- validation evidence log entries

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
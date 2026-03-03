# Bundle Prompt

## Bundle
- Slug: anchor-candidate-trace-completeness
- Intent: neutral

## Objective
Emit complete anchor candidate decision coverage so every selected and rejected candidate has a machine-readable terminal reason.

## Value
Turns row-level anchor transitions into auditable evidence instead of inferred behavior.

## Scope In
- Emit all candidate rows for each anchor decision context.
- Emit exactly one terminal AnchorSelectionReasonCode for each candidate.
- Ensure selected-vs-rejected state is explicit via was_selected.
- Keep trace records deterministic and diffable across identical runs.

## Scope Out
- No source resolution semantics changes.
- No benchmark threshold policy changes.
- No CLI and web output-surface abstraction changes.

## Prerequisites
- Canonical anchor decision schema is available.
- Terminal reason-code model is fixed.
- Trace completeness policy remains mandatory.

## Prerequisites And Contracts
### Consumed Contracts
- CON-012
- CON-019
- CON-024
- CON-026

### Introduced Contracts
- anchor_candidate_trace_contract

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
- No missing terminal reason codes in emitted candidate traces.

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers and consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
A row with fallback behavior contains one selected candidate and all rejected alternatives, each carrying one terminal reason code and deterministic ordering.

## Expected Evidence Artifacts
- anchors artifact with full candidate decision sets
- reason-code coverage evidence
- validation evidence log entries

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
# Bundle Prompt

## Bundle
- Slug: artifact-surface-abstraction-cli-web
- Intent: neutral

## Objective
Provide anchors artifact parity across CLI and web outputs using one shared service-layer delivery abstraction.

## Value
Prevents output contract divergence across ingress and egress surfaces and keeps artifact publication deterministic.

## Scope In
- Add anchors artifact to output surfaces exposed by service-level workflows.
- Preserve N-tier boundaries by keeping business routing in logic and publication translation in service.
- Keep output path and in-memory payload contracts explicit and layer-owned.
- Record compatibility decision for interaction-surface changes.

## Scope Out
- No anchor resolver behavior changes.
- No benchmark gate threshold policy changes.
- No synthetic multi-seed panel threshold changes.

## Prerequisites
- Anchors artifact schema is stable enough for service publication.
- CLI and web output parity requirement remains mandatory.
- Compatibility policy remains explicitly recorded.

## Prerequisites And Contracts
### Consumed Contracts
- CON-008
- CON-016
- CON-027

### Introduced Contracts
- anchor_artifact_delivery_parity

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
- CLI and web artifact field contracts are equivalent for canonical rows.

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers and consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
For the same input, CLI and web flows expose equivalent anchors artifact semantics and fields without layer-boundary leakage.

## Expected Evidence Artifacts
- CLI anchors artifact output
- web anchors payload output
- parity validation evidence log entries

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
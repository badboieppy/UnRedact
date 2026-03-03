# Implement Runbook

## Context Reload Protocol
1. Read .local-agent/series/Overview.md, .local-agent/series/ContextDigest.md, .local-agent/series/RequirementsLedger.md, and .local-agent/series/ContractLedger.md.
2. Read this bundle files .agent/Prompt.md and .agent/Plans.md.
3. Write an Execution Brief in .agent/Documentation.md listing REQ IDs, contract IDs, doctrine checks, and proof plan.

## Execution Workflow
1. Confirm scope boundaries and consumed and introduced contracts.
2. Apply only scoped changes while preserving layer boundaries and explicit boundary mappings.
3. Record compatibility decision details for any changed interaction surface.
4. Run required validation commands.
5. Record command outcomes, gate outcomes, and metric deltas.
6. Run the purpose demo and capture explicit evidence.

## Doctrine Enforcement Checklist
- File/module ownership aligns to one layer.
- Calls only move to the immediate lower layer.
- Boundary request/response/error types are translated at each boundary.
- No speculative configuration knobs are introduced.
- Public interfaces remain minimal and typed.
- Exception comments (if any) are narrow and justified locally.

## Stop-and-Fix Gates
Stop immediately and investigate if any condition occurs:
- Required command failure.
- Contract violation or missing evidence artifact.
- Doctrine boundary violation.
- Metric regression outside accepted tolerance for this bundle intent.

## Regression Policy
- No retry loop is allowed for hard regressions.
- On out-of-band regression: investigate root cause, rollback scoped change set, and emit evidence artifacts documenting findings.

## Required Commands
- cargo test
- cargo run --bin guess_accuracy_benchmark --release
- cargo run --bin synthetic_overfitting_benchmark --release
- cargo run --bin visual_score_impact_benchmark --release
- cargo run --bin evidence_first_change_gate --release

## Completion Conditions
- All scoped acceptance criteria are met.
- Validation and purpose demo evidence are recorded in documentation.
- Runtime execution state is updated with proof points and touched paths.
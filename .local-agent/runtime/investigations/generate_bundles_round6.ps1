$ErrorActionPreference = "Stop"
$root = ".local-agent/bundles"
$doctrinePath = "D:\Development\RandomProjects\Agents\doctrines\engineering_doctrine.md"
$requiredCommands = @(
  "cargo test",
  "cargo run --bin guess_accuracy_benchmark --release",
  "cargo run --bin synthetic_overfitting_benchmark --release",
  "cargo run --bin visual_score_impact_benchmark --release",
  "cargo run --bin evidence_first_change_gate --release"
)

$bundles = @(
  [ordered]@{
    slug="anchor-contract-schema-foundation"
    intent="neutral"
    purpose="Define the canonical anchor decision schema with deterministic source labels, single terminal decision reason code, and per-side plus row confidence values."
    value="Makes anchor decisions machine-parseable and trace-complete without relying on inferred behavior from free-form diagnostics."
    scope_in=@(
      "Define anchor decision record types and enums required by the canonical contract.",
      "Represent `was_selected` with one `AnchorSelectionReasonCode` per candidate.",
      "Represent `left_anchor_confidence`, `right_anchor_confidence`, and `row_anchor_confidence` in range `0.0..1.0`.",
      "Add compatibility decision record for schema breakage policy and migration posture."
    )
    scope_out=@(
      "No resolver behavior changes.",
      "No benchmark gate threshold changes.",
      "No delivery-surface abstraction changes for CLI/web."
    )
    prerequisites=@(
      "Canonical files are current under `.local-agent/series/`.",
      "Breaking schema policy remains accepted.",
      "Contract-change policy remains strict and explicit."
    )
    contracts_consumed=@("CON-007","CON-011","CON-019","CON-025","CON-026")
    contracts_introduced=@("anchor_decision_schema")
    purpose_demo="A single row in the anchor artifact includes selected and rejected candidates, each with `was_selected` and one terminal reason code, and shows left/right/row confidence values in `0.0..1.0`."
    risk="Schema drift between canonical anchor artifact and compact guess-level anchor summary fields."
    artifacts_expected=@("anchors artifact schema output","compatibility decision note in bundle documentation","validation evidence log entries")
    quality_gates=@(
      "All required validation commands complete successfully.",
      "No core metric regression beyond accepted tolerance bands for neutral intent.",
      "No architecture boundary violations against service/logic/data/dependency doctrine."
    )
  },
  [ordered]@{
    slug="run-source-resolution-contract"
    intent="improve"
    purpose="Lock deterministic source-resolution semantics for `run_exact` and `run_prefix_projection` with explicit provenance and newline/punctuation-preserving exactness."
    value="Eliminates silent coordinate-source ambiguity and aligns source labels with actual coordinate derivation behavior."
    scope_in=@(
      "Apply exact-match comparison policy that preserves punctuation and newline boundaries.",
      "Apply prefix-projection validity contract and offset provenance precedence.",
      "Emit deterministic source label and offset provenance required for confidence attribution.",
      "Translate errors at each layer boundary when introducing any new resolver error surfaces."
    )
    scope_out=@(
      "No artifact delivery surface changes for CLI/web.",
      "No benchmark-threshold policy changes.",
      "No documentation-only governance updates."
    )
    prerequisites=@(
      "Canonical schema fields for source/provenance are available.",
      "Prefix-projection validity contract is fixed as delegated policy.",
      "Determinism requirement remains binding."
    )
    contracts_consumed=@("CON-018","CON-023","CON-024")
    contracts_introduced=@("anchor_source_resolution_contract")
    purpose_demo="Running the same input twice produces identical per-side source labels and coordinate provenance, and exact matches keep punctuation/newline-sensitive text semantics."
    risk="Resolver precedence changes can shift ranking outcomes if any ambiguous branch remains."
    artifacts_expected=@("anchors artifact with explicit per-side source provenance","row-level trace evidence for deterministic replay","validation evidence log entries")
    quality_gates=@(
      "All required validation commands complete successfully.",
      "At least one declared accuracy metric improves for improve intent, with no out-of-band regression.",
      "Regression outside tolerance triggers immediate investigate-plus-rollback flow without retry."
    )
  },
  [ordered]@{
    slug="anchor-candidate-trace-completeness"
    intent="neutral"
    purpose="Emit complete anchor candidate decision coverage so every selected and rejected candidate has a machine-readable terminal reason."
    value="Turns row-level anchor transitions into auditable evidence instead of inferred behavior."
    scope_in=@(
      "Emit all candidate rows for each anchor decision context.",
      "Emit exactly one terminal `AnchorSelectionReasonCode` for each candidate.",
      "Ensure selected-vs-rejected state is explicit via `was_selected`.",
      "Keep trace records deterministic and diffable across identical runs."
    )
    scope_out=@(
      "No source resolution semantics changes.",
      "No benchmark threshold policy changes.",
      "No CLI/web output-surface abstraction changes."
    )
    prerequisites=@(
      "Canonical anchor decision schema is available.",
      "Terminal reason-code model is fixed.",
      "Trace completeness policy remains mandatory."
    )
    contracts_consumed=@("CON-012","CON-019","CON-024","CON-026")
    contracts_introduced=@("anchor_candidate_trace_contract")
    purpose_demo="A row with fallback behavior contains one selected candidate and all rejected alternatives, each carrying one terminal reason code and deterministic ordering."
    risk="Large trace payloads can increase artifact size and serialization pressure."
    artifacts_expected=@("anchors artifact with full candidate decision sets","reason-code coverage evidence","validation evidence log entries")
    quality_gates=@(
      "All required validation commands complete successfully.",
      "No core metric regression beyond accepted tolerance bands for neutral intent.",
      "No missing terminal reason codes in emitted candidate traces."
    )
  },
  [ordered]@{
    slug="artifact-surface-abstraction-cli-web"
    intent="neutral"
    purpose="Provide anchors artifact parity across CLI and web outputs using one shared service-layer delivery abstraction."
    value="Prevents output contract divergence across ingress/egress surfaces and keeps artifact publication deterministic."
    scope_in=@(
      "Add anchors artifact to output surfaces exposed by service-level workflows.",
      "Preserve N-tier boundaries by keeping business routing in logic and publication translation in service.",
      "Keep output path and in-memory payload contracts explicit and layer-owned.",
      "Record compatibility decision for interaction-surface changes."
    )
    scope_out=@(
      "No anchor resolver behavior changes.",
      "No benchmark gate threshold policy changes.",
      "No synthetic multi-seed panel threshold changes."
    )
    prerequisites=@(
      "Anchors artifact schema is stable enough for service publication.",
      "CLI and web output parity requirement remains mandatory.",
      "Compatibility policy remains explicitly recorded."
    )
    contracts_consumed=@("CON-008","CON-016","CON-027")
    contracts_introduced=@("anchor_artifact_delivery_parity")
    purpose_demo="For the same input, CLI and web flows expose equivalent anchors artifact semantics and fields without layer-boundary leakage."
    risk="One surface can accidentally omit or reshape fields relative to the other surface."
    artifacts_expected=@("CLI anchors artifact output","web anchors payload output","parity validation evidence log entries")
    quality_gates=@(
      "All required validation commands complete successfully.",
      "No core metric regression beyond accepted tolerance bands for neutral intent.",
      "CLI and web artifact field contracts are equivalent for canonical rows."
    )
  },
  [ordered]@{
    slug="guess-anchor-compact-linkage"
    intent="neutral"
    purpose="Retain compact per-side anchor context in guesses while preserving full trace detail in anchors artifact."
    value="Keeps guess inspection fast while maintaining canonical trace depth in a dedicated artifact."
    scope_in=@(
      "Emit per-side compact fields in guesses: side-linked anchor identity context, anchor type, selected source, confidence.",
      "Keep compact fields aligned with canonical anchor artifact values.",
      "Preserve deterministic serialization order and stable field names.",
      "Record compatibility decision for guesses interaction-surface change."
    )
    scope_out=@(
      "No resolver decision-policy changes.",
      "No benchmark threshold changes.",
      "No multi-seed panel policy changes."
    )
    prerequisites=@(
      "Anchors artifact is available as canonical source of full decision records.",
      "Compact field set is fixed by contract.",
      "Anchor identity exposure contract remains active."
    )
    contracts_consumed=@("CON-015","CON-020","CON-027")
    contracts_introduced=@("guess_anchor_compact_summary_contract")
    purpose_demo="Each guess row contains the agreed compact per-side anchor fields and these values match the selected anchor records in anchors artifact for the same row."
    risk="Compact and canonical values can diverge if mapping rules are inconsistent."
    artifacts_expected=@("guesses artifact compact anchor fields","alignment evidence between guesses and anchors artifact","validation evidence log entries")
    quality_gates=@(
      "All required validation commands complete successfully.",
      "No core metric regression beyond accepted tolerance bands for neutral intent.",
      "Compact fields are present for all emitted guess rows with side anchors."
    )
  },
  [ordered]@{
    slug="benchmark-intent-threshold-engine"
    intent="neutral"
    purpose="Make benchmark hard-pass logic machine-checkable by declared bundle intent with 1% margin bands."
    value="Ensures governance decisions are deterministic and auditable instead of interpretive."
    scope_in=@(
      "Implement intent-aware gate evaluation for `improve` and `neutral` expectations.",
      "Apply 1% margin-of-error bands to hard threshold boundaries.",
      "Preserve no-retry regression policy for out-of-band failures.",
      "Emit explicit gate decision traces with threshold math inputs and outputs."
    )
    scope_out=@(
      "No anchor resolver algorithm changes.",
      "No schema redesign outside gate metadata needed for evaluation.",
      "No output-surface publication abstraction changes."
    )
    prerequisites=@(
      "Bundle-intent governance contract is fixed.",
      "Accepted threshold values and margin policy are fixed.",
      "No-retry rollback policy remains binding."
    )
    contracts_consumed=@("CON-017","CON-022","CON-028")
    contracts_introduced=@("intent_aware_threshold_gate_contract")
    purpose_demo="Gate output for a neutral task shows threshold comparison with margin band and fails deterministically when a metric crosses out of tolerated range."
    risk="Incorrect threshold arithmetic or margin application can create false approvals or false failures."
    artifacts_expected=@("gate decision artifact with explicit threshold math","bundle-intent metadata trace","validation evidence log entries")
    quality_gates=@(
      "All required validation commands complete successfully.",
      "No core metric regression beyond accepted tolerance bands for neutral intent.",
      "Gate decision includes explicit intent, threshold, margin, and outcome fields."
    )
  },
  [ordered]@{
    slug="synthetic-multiseed-panel-gate"
    intent="neutral"
    purpose="Add required synthetic multi-seed average panel evaluation with `N>=20` and accepted tolerance deltas."
    value="Provides durable over-time signal while preserving deterministic binding checks and reducing overreaction to single-seed variance."
    scope_in=@(
      "Emit required multi-seed average panel diagnostics on every benchmark run.",
      "Evaluate panel deltas against accepted tolerances with margin policy.",
      "Keep fixed-seed determinism checks binding and explicit.",
      "Record sample-size and tolerance calculations used by gate outputs."
    )
    scope_out=@(
      "No anchor resolver policy changes.",
      "No output publication surface changes.",
      "No documentation-only governance updates outside panel behavior."
    )
    prerequisites=@(
      "Synthetic protocol root contract remains active.",
      "Minimum panel size accepted at `N>=20`.",
      "Tolerance policy for panel deltas is fixed."
    )
    contracts_consumed=@("CON-002","CON-021","CON-028")
    contracts_introduced=@("synthetic_multiseed_panel_contract")
    purpose_demo="Synthetic benchmark report includes a required average panel with at least 20 seeds, reports delta thresholds, and emits deterministic pass/fail outcome."
    risk="Panel runtime cost can rise and threshold tuning can destabilize if not encoded deterministically."
    artifacts_expected=@("synthetic benchmark report with multi-seed panel section","panel threshold decision fields","validation evidence log entries")
    quality_gates=@(
      "All required validation commands complete successfully.",
      "No core metric regression beyond accepted tolerance bands for neutral intent.",
      "Panel section must be present with `seed_count >= 20` and explicit threshold deltas."
    )
  },
  [ordered]@{
    slug="regression-response-rollback-protocol"
    intent="neutral"
    purpose="Enforce deterministic no-retry regression handling with mandatory investigate, rollback, and evidence emission."
    value="Prevents ambiguous failure handling and preserves a reliable audit trail for every hard regression event."
    scope_in=@(
      "Define hard-regression response flow with no retry path.",
      "Emit required investigation and root-cause evidence payload when rollback occurs.",
      "Ensure rollback boundaries are explicit and deterministic.",
      "Ensure error translation remains layer-local if rollback orchestration introduces new error paths."
    )
    scope_out=@(
      "No anchor source semantics changes.",
      "No schema redesign beyond response/evidence payload needs.",
      "No CLI/web artifact contract changes outside regression evidence outputs."
    )
    prerequisites=@(
      "Regression policy remains no-retry.",
      "Bundle-intent and threshold engine decisions are available for trigger conditions.",
      "Evidence-first governance remains required."
    )
    contracts_consumed=@("CON-003","CON-022","CON-029")
    contracts_introduced=@("regression_action_protocol")
    purpose_demo="An out-of-band regression event yields deterministic investigate-plus-rollback flow and writes a root-cause evidence artifact without retrying the failed run."
    risk="Rollback boundaries can be mis-scoped if trigger semantics are ambiguous."
    artifacts_expected=@("regression investigation artifact","rollback action record","validation evidence log entries")
    quality_gates=@(
      "All required validation commands complete successfully.",
      "No core metric regression beyond accepted tolerance bands for neutral intent.",
      "No-retry path enforced for hard regression outcomes."
    )
  },
  [ordered]@{
    slug="baseline-refresh-and-governance-docs"
    intent="neutral"
    purpose="Codify baseline governance, command matrix, compatibility decisions, and hard-gate interpretation rules in end-state documentation."
    value="Keeps acceptance criteria and operational policy explicit, reproducible, and reviewable over time."
    scope_in=@(
      "Document required validation command matrix and artifact expectations.",
      "Document baseline lock/update policy and threshold interpretation with margin policy.",
      "Document compatibility decisions and removal posture for changed interfaces.",
      "Document waiver process expectations for doctrine exceptions."
    )
    scope_out=@(
      "No runtime algorithm changes.",
      "No schema behavior changes.",
      "No output publication behavior changes."
    )
    prerequisites=@(
      "Current baseline artifacts are available for reference.",
      "Current threshold and regression policy decisions are fixed.",
      "Doctrine path remains authoritative for engineering constraints."
    )
    contracts_consumed=@("CON-013","CON-014","CON-017","CON-028","CON-029")
    contracts_introduced=@("baseline_governance_contract")
    purpose_demo="Governance documentation maps every required command to expected outputs and pass/fail rules with margin and rollback semantics."
    risk="Documentation can drift if operational rules change without synchronized updates."
    artifacts_expected=@("governance documentation updates","command-to-artifact mapping tables","validation evidence log entries")
    quality_gates=@(
      "All required validation commands remain documented as mandatory.",
      "No policy contradiction with current contract ledger entries.",
      "No core metric regression beyond accepted tolerance bands for neutral intent."
    )
  }
)

function ToBullets {
  param([string[]]$values)
  ($values | ForEach-Object { "- $_" }) -join [Environment]::NewLine
}

function ToCodeBullets {
  param([string[]]$values)
  ($values | ForEach-Object { "- `$_`" }) -join [Environment]::NewLine
}

foreach ($bundle in $bundles) {
  $bundleRoot = Join-Path $root $bundle.slug
  $agentDir = Join-Path $bundleRoot ".agent"
  New-Item -Path $agentDir -ItemType Directory -Force | Out-Null

  $contractsConsumedBullets = ToCodeBullets $bundle.contracts_consumed
  $contractsIntroducedBullets = ToCodeBullets $bundle.contracts_introduced
  $scopeInBullets = ToBullets $bundle.scope_in
  $scopeOutBullets = ToBullets $bundle.scope_out
  $prereqBullets = ToBullets $bundle.prerequisites
  $qualityGateBullets = ToBullets $bundle.quality_gates
  $requiredCommandsBullets = ToCodeBullets $requiredCommands
  $artifactBullets = ToBullets $bundle.artifacts_expected

  $prompt = @"
# Bundle Prompt

## Bundle
- Slug: `$($bundle.slug)`
- Intent: `$($bundle.intent)`

## Objective
$($bundle.purpose)

## Value
$($bundle.value)

## Scope In
$scopeInBullets

## Scope Out
$scopeOutBullets

## Prerequisites
$prereqBullets

## Prerequisites & Contracts
### Consumed Contracts
$contractsConsumedBullets

### Introduced Contracts
$contractsIntroducedBullets

## Doctrine Constraints
- Doctrine path: `$doctrinePath`
- Architecture boundaries must remain strict Service -> Logic -> Data -> Dependency.
- Boundary type and error translation must be explicit at every adjacent layer crossing.
- Public interfaces must stay minimal; compatibility decisions must be explicitly recorded.

## Validation Intent
### Required Commands
$requiredCommandsBullets

### Quality Gates
$qualityGateBullets

## Compatibility Decision Record
- Backwards compatibility required: No (breaking changes permitted for this effort).
- Dependents: internal project surfaces only.
- Migration plan: update all active producers/consumers in this repository within this change scope.
- Removal plan: remove superseded contract surfaces in the same change once replacement is active.

## Purpose Demo
$($bundle.purpose_demo)

## Expected Evidence Artifacts
$artifactBullets

## Async Execution Rule
No user interaction is allowed during execution. Resolve decisions from contracts and evidence artifacts only.
"@

  $plans = @"
# Execution Plan

## Milestones
1. Context alignment and execution brief
- Reload canonical series context and doctrine.
- Write execution brief in `.agent/Documentation.md` with relevant REQ and contract IDs.

2. Contract implementation scope
- Implement only the scoped contract changes described in Prompt.
- Keep interface and error translation explicit at every layer boundary.

3. Reconciliation checkpoint
- Verify all scoped acceptance conditions against emitted artifacts.
- Verify doctrine checklist and compatibility decision record are complete.
- Resolve any contract drift before validation runs.

4. Validation and evidence capture
- Run required validation command set.
- Capture gate outcomes and metric deltas in documentation.
- Enforce regression policy for this bundle intent.

5. Purpose demo completion
- Execute purpose demo scenario.
- Capture demo evidence in documentation with deterministic proof points.

## Acceptance Criteria
- Scoped contract changes are complete and deterministic.
- Doctrine constraints and compatibility record are satisfied.
- Validation evidence is recorded and passes intent-aligned gates.
- Purpose demo evidence is present and auditable.
"@

  $implement = @"
# Implement Runbook

## Context Reload Protocol
1. Read `.local-agent/series/Overview.md`, `.local-agent/series/ContextDigest.md`, `.local-agent/series/RequirementsLedger.md`, and `.local-agent/series/ContractLedger.md`.
2. Read this bundle's `.agent/Prompt.md` and `.agent/Plans.md`.
3. Write an Execution Brief in `.agent/Documentation.md` listing REQ IDs, contract IDs, doctrine checks, and proof plan.

## Execution Workflow
1. Confirm scope boundaries and consumed/introduced contracts.
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
$requiredCommandsBullets

## Completion Conditions
- All scoped acceptance criteria are met.
- Validation and purpose demo evidence are recorded in documentation.
- Runtime execution state is updated with proof points and touched paths.
"@

  $documentation = @"
# Documentation Log

## Execution Brief
- Status: pending
- REQ IDs:
- Contract IDs consumed:
- Contract IDs introduced:
- Doctrine checks:
- Proof plan:

## Compatibility Decision Record
- Backwards compatibility required:
- Dependents:
- Migration plan:
- Removal plan:

## Milestone Log
### Milestone 1
- Status:
- Evidence:

### Milestone 2
- Status:
- Evidence:

### Milestone 3
- Status:
- Evidence:

### Milestone 4
- Status:
- Evidence:

### Milestone 5
- Status:
- Evidence:

## Validation Evidence
- Command results:
- Gate outcomes:
- Metric deltas:

## Purpose Demo Evidence
- Scenario:
- Observed output:
- Determinism check:

## Waivers
- None.

## Final Outcome
- Status:
- Summary:
- Paths touched:
"@

  $manifest = [ordered]@{
    bundle_slug = $bundle.slug
    status = "planned"
    intent = $bundle.intent
    loc_budget_target = 500
    doctrine_path = $doctrinePath
    contracts_consumed = $bundle.contracts_consumed
    contracts_introduced = $bundle.contracts_introduced
    validation_intent = [ordered]@{
      required_commands = $requiredCommands
      quality_gates = $bundle.quality_gates
      regression_policy = "no_retry_investigate_then_rollback_on_out_of_band_regression"
    }
    purpose_demo = $bundle.purpose_demo
    artifacts_expected = $bundle.artifacts_expected
  }

  Set-Content -Path (Join-Path $agentDir "Prompt.md") -Value $prompt -NoNewline
  Set-Content -Path (Join-Path $agentDir "Plans.md") -Value $plans -NoNewline
  Set-Content -Path (Join-Path $agentDir "Implement.md") -Value $implement -NoNewline
  Set-Content -Path (Join-Path $agentDir "Documentation.md") -Value $documentation -NoNewline
  Set-Content -Path (Join-Path $bundleRoot "MANIFEST.json") -Value ($manifest | ConvertTo-Json -Depth 8) -NoNewline
}

Write-Output "created"
# Anchor Set Workflow Deep-Dive
Date: 2026-03-02

## Why this pass
User requested deeper validation of the assumption that current errors come from separate single-vs-multi workflows and asked to evaluate a simpler redaction-set model.

## Code-trace map
### Current anchor decision path
- Entry: `build_anchor_validated_guesses` in `src/logic/redaction_guessing_component/guess_logic.rs`.
- Per redaction:
  1) extract local context,
  2) gather shared cluster hints,
  3) `select_anchor_pair`:
     - same-run two-sided path (`try_same_run_hint_anchor`),
     - pair-candidate two-sided path (`build_pair_candidates` + `build_two_sided_anchor_from_pair`),
     - one-sided fallback (`recover_one_sided_anchor`).
- After initial per-row guesses:
  - `propagate_row_context_to_guesses`,
  - `apply_cluster_consensus`,
  - `apply_row_joint_assignment`,
  - `apply_row_sequence_consensus`.

### Current grouping behavior
- Redaction rows are linked into clusters using:
  - same-line x-gap <= 96 pt,
  - wrap-line y-gap <= 18 pt with forward/reset x rules.
- Shared hints are merged across every redaction in each cluster (`build_shared_context_hints`).
- Hint selection allows large edge gap (`CLUSTER_ANCHOR_HINT_MAX_GAP_PT = 240`).

## Evidence from required files
### Cluster spread evidence
Artifact: `.local-agent/runtime/investigations/determinism_probe/cluster_hint_pollution_report.json`

Observed:
- `EFTA00038617`: largest cluster on page index 1 has 10 redactions with merged span estimate 113.
- `EFTA00101126`: page index 1 has one 7-redaction cluster (the red UI button region).

Interpretation:
- In dense rows, the algorithm treats nearby redactions as one contextual set and shares hints broadly.
- This can cause non-local anchor reuse when local context is sparse/ambiguous.

### Alignment failure evidence
Artifact: `.local-agent/runtime/investigations/determinism_probe/anchor_reuse_anomaly_report.json`

Observed:
- `EFTA00038617`: 6/29 selected rows have far anchors (>120 pt on at least one side).
- `EFTA00101126`: 0/8 far-anchor rows for anchored targets, but has raster false positives on UI controls.

Interpretation:
- Main failure in `EFTA00038617` is locality drift from cluster-shared hinting and dense-row rendering behavior.
- Main failure in `EFTA00101126` is redaction detection quality (false positives), not anchor locality on true bars.

### Determinism scope evidence
Artifacts:
- `.local-agent/runtime/investigations/determinism_probe/repeatability/repeatability_hashes.json`
- normalized compare notes in `.local-agent/runtime/investigations/determinism_anchor_alignment_incident_probe.md`

Observed:
- Raw files differ run-to-run due diagnostics/timing values.
- Core decision payloads match when diagnostics/timing fields are normalized out.

Interpretation:
- Decision algorithm is deterministic for tested files.
- Full artifact determinism contract is currently violated by timing/diagnostic emission.

## Direct answer to user assumption
- Yes: current architecture has multiple decision layers that behave differently for sparse vs dense groups.
- The densest source of error is not random data; it is deterministic cross-row hint sharing plus permissive locality thresholds in dense clusters.
- A unified redaction-set workflow is compatible with the observed failure mode and likely simplifies contracts if it enforces strict local-left/local-right anchor selection first, with explicit no-anchor outputs when confidence is low.

## Remaining design unknowns to close
1. strict anchor locality limits by mode,
2. dense-row rendering policy (anchor text included vs selected-only),
3. redaction detection false-positive policy for colored/non-black fills,
4. full artifact determinism policy for diagnostics/timing fields,
5. machine-checkable success gate for "perfect anchored-side alignment".

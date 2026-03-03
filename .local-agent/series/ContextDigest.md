# ContextDigest
Generated: 2026-03-02
Phase: 1 (Incident investigation pass complete; question round pending)
Input Doc(s): docs/anchor_geometry_reliability_design.md + user incident intake (2026-03-02)
Status: Incident-cycle blockers are open and require clarification before planning output for this cycle.

## 1) InputDoc Anchor Map
- InputDoc: 1.1 Service description
- InputDoc: 1.2 Important service details
- InputDoc: 1.3 Problem description
- InputDoc: 1.4 Requirements governing the problem
- InputDoc: 2.1 Runtime architecture and call graph
- InputDoc: 2.2 Underlying-text context workflow (redaction-side geometry)
- InputDoc: 2.3 Font-run workflow (run-side geometry)
- InputDoc: 2.4 Anchoring workflow in guessing logic
- InputDoc: 2.5 Candidate scoring and ranking workflow
- InputDoc: 2.6 Evidence of current failure modes
- InputDoc: 2.7 Summary of current-state limitations
- InputDoc: 3.1 Recommended solution overview
- InputDoc: 3.2 Solution architecture and workflow
- InputDoc: 3.3 Anchor geometry source model
- InputDoc: 3.4 Anchor decision explainability model
- InputDoc: 3.5 Ranking and diagnostics behavior in solution state
- InputDoc: 3.6 Service-level quality behavior in solution state
- InputDoc: 3.7 Why this solution addresses the problem

## 2) Structured Facts Extracted
### Goals
- Recover hidden redacted words with measurable quality improvements (InputDoc: 1.1).
- Improve reliability of anchor geometry decisions and explainability of decision paths (InputDoc: 1.3).
- Keep behavior deterministic and evidence-governed (InputDoc: 1.2, 1.4).

### Current State (code-evidenced)
- Orchestration path is service -> component -> redaction scan + font runs + guessing (src/service/unredact_cli_entry.rs:68; src/logic/redaction_guessing_component/mod.rs:61,124,164).
- Output files are currently fixed to: `.redactions.json`, `.fonts.json`, `.guesses.json`, optional `.visualized.pdf` (src/logic/local_file_workflow_component.rs:27-37).
- Encoded pipeline currently has only redactions/fonts/guesses plus optional visualized pdf bytes (src/logic/types/mod.rs:58-63,66-78).
- Redaction-side context uses coarse width approximation `font_size * 0.6 * char_count` (src/dependency/pdf_redaction/text_parser.rs:177).
- Anchor selection path:
  1) collect hints,
  2) try same-run two-sided,
  3) else pair-based two-sided,
  4) else one-sided fallback (src/logic/redaction_guessing_component/guess_logic.rs:1807-1875,2136-2262).
- Pair candidates are already deterministically sorted by explicit key sequence (`font_penalty`, `hint_penalty`, `straddle_penalty`, `overlap_penalty`, `contains_center_penalty`, then distances/gap) (src/logic/redaction_guessing_component/guess_logic.rs:2032-2069).
- Resolver text/x rules:
  - exact text equality -> run x0,
  - hint found inside run text -> run x + projected prefix width,
  - hint contains run text with comma+prefix guard -> may return hint_x,
  - else fallback behaviors (src/logic/redaction_guessing_component/guess_logic.rs:2264-2330).
- Existing guess schema includes anchor fields but no structured decision/rejection reason enum arrays (src/types/guess_types.rs:61-103).

### Benchmark/Gating State (code-evidenced)
- `guess_accuracy_benchmark` hard-fails on runtime errors and determinism gate mismatch; no hard threshold gate yet for recall/mrr deltas (src/bin/guess_accuracy_benchmark.rs:2344-2350).
- `synthetic_overfitting_benchmark` has hard gate for run completeness + fixed-seed consistency (src/bin/synthetic_overfitting_benchmark.rs:273-275,489-491).
- `visual_score_impact_benchmark` reports metrics and fails on runtime/input errors, not on quality thresholds (src/bin/visual_score_impact_benchmark.rs:158-293).
- `evidence_first_change_gate` has pass/fail based on dossier validation schema (src/bin/evidence_first_change_gate.rs:55-95).

### Measured Failure Evidence (verified artifacts)
- Ranking bundle metrics confirmed from `benchmark/ranking_bundle5_*.json`:
  - before: r@1=0.090909, r@5=0.181818, r@20=0.363636, mrr=0.142650, best_k=128
  - after_pipe_norm: unchanged
  - after_hint_superset: r@1=0.000000, mrr=0.080269, best_k=133
  - after_hint_superset_threshold: r@1=0.000000, r@20=0.454545, mrr=0.081756, best_k=133
  - after_hint_superset_comma_guard: r@1=0.090909, r@20=0.454545, mrr=0.157513, best_k=133
- `EFTA00101126` probe shift verified:
  - current row 7/8: left `EPSTEIN, including`, left_x `71.86`/`137.11`, SARAH rank `4`/`4`
  - comma_guard row 7/8: left `including`, left_x `126.11`/`191.36`, SARAH rank `3`/`1`
- `EFTA00038617` row 13 flip verified:
  - before: `two_sided`, right `Maxwell,`, candidates 166
  - threshold: `left_only`, right `Ghislaine Maxwell,`, candidates 200

## 3) User Response Round 2 (2026-03-02)
Resolved decisions:
- Full anchor decision data should be emitted in a separate `anchors.json` artifact.
- Guess output should still include anchor information for quick visibility/reference.
- Contract-file change policy: only when strictly required (user chose conservative policy).
- “Pre-calculated baseline” should be derived by running the full benchmark set now and treating those outputs as baseline.
- Tests should be hard pass/fail; benchmarks should include both metrics and hard pass/fail thresholds.

Still unresolved (user requested deeper context first):
- Precise definition of `run_exact` matching semantics.
- Precise definition of what qualifies as `run_prefix_projection` validity.
- Detailed meaning of source tiers and same-tier tie-breaking.
- Clear naming of per-side trace fields.
- Scope and structure of reason-code enums.

## 4) Contradictions / Tensions
- InputDoc run-first requirement (3.3) vs current branch that can take hint_x on superset cases (guess_logic.rs:2311-2325).
- InputDoc requires complete machine-readable rejection reasons (3.4), but current output has only free-form diagnostics and summary fields.
- Benchmarks currently mix hard gates and non-threshold reporting; user now wants hard pass/fail thresholds across benchmarks.

## 5) Blockers (updated)
1. Formal source-tier definitions are not finalized (`run_exact`, `run_prefix_projection`, `hint_only_fallback` predicates).
2. Same-tier deterministic tie-break contract is not explicitly approved (despite existing deterministic code order).
3. Per-side trace field naming schema is not finalized.
4. Reason-code enum model boundaries are not finalized (selection vs rejection scope and naming).
5. Exact benchmark threshold policy is not finalized (which metrics, threshold values, comparison baseline semantics).
6. Full benchmark set command matrix is not fully enumerated for mandatory hard pass/fail.
7. `anchors.json` integration surface for both CLI and web outputs is not yet finalized.
8. “Reference in guesses doc” contract is not finalized (inline summary only vs stable pointer/id into anchors artifact).

## 6) Investigation Log (Phase 1 pass before next questions)
- Traced output encoding and file writer surfaces (`OutputFilePaths`, `EncodedPipelineOutputs`, publisher).
- Traced anchor selection internals and pair sorting keys.
- Traced resolver branch semantics and existing test coverage around superset handling.
- Traced benchmark/gate exit behavior for all bin entry points.

## 7) Baseline Run Evidence (user-requested full benchmark run)
Executed in release mode with outputs under `.local-agent/runtime/investigations/`.
- guess_accuracy overall: r@1=0.090909, r@5=0.181818, r@20=0.454545, mrr=0.157513, best_k=133; determinism_gate passed with mismatch_count=0.
- synthetic_overfitting: run_completeness_gate passed, fixed_seed_gate passed (mismatch_count=0).
- visual_score_impact: no_visual and visual summaries identical in this run (pairwise better=0, worse=0, tie=100).
- evidence_first_change_gate: approved=true with zero errors.
- cargo test: all targets passed (observed 76 passed, 0 failed).

## 8) User Response Round 3 (2026-03-02)
Resolved decisions:
- Guesses artifact should include anchor info relevant to each guess (for quick interpretation of guess impact).
- Both CLI and web outputs should produce anchor artifact outputs; artifact handling should be abstracted at service/output layer.
- Hard pass policy accepted with bundle-context interpretation:
  - Tests must pass.
  - Benchmarks must not fail runtime/gates.
  - Core metrics must not regress.
  - Metric improvement is required only for bundles expected to improve metrics; non-behavioral bundles are not expected to improve metrics.
- Mandatory validation command set confirmed:
  - `cargo test`
  - `cargo run --bin guess_accuracy_benchmark --release`
  - `cargo run --bin synthetic_overfitting_benchmark --release`
  - `cargo run --bin visual_score_impact_benchmark --release`
  - `cargo run --bin evidence_first_change_gate --release`

Open clarifications requested by user (needs deeper definitions first):
- Matching semantics options for `run_exact`.
- Validity semantics for `run_prefix_projection` offset derivation.
- Full meaning of source tiers, same-tier ties, and tie-break key order terms.
- Enum naming conventions for source and reason-code fields.
- Exact behavior/scope mapped by reason-code enums.

## 9) Blocker Resolution Delta (after round 3)
Resolved blockers:
- Full benchmark command set now specified.
- CLI+web artifact parity direction now specified.

Remaining blockers:
1. `run_exact` normalization/matching contract is not finalized.
2. `run_prefix_projection` validity contract is not finalized.
3. Same-tier tie-break contract terminology and final acceptance remain open.
4. Final enum naming/schema for source and reason-code fields remains open.
5. Exact guesses-to-anchors reference contract (row id/pointer model) remains open.
6. Bundle-intent-aware threshold policy needs explicit machine-checkable rule format.

## 10) User Response Round 4 (2026-03-02)
Resolved decisions:
- `run_exact` matching should use the loosest option: normalized punctuation/whitespace + case-insensitive comparison.
- Current tie-break ordering is acceptable if each decision is explicitly traced.
- Prefer a single candidate-level decision model using:
  - `was_selected` boolean
  - `AnchorSelectionReasonCode` for both selected and non-selected outcomes.
- Anchor ID is required; guess rows should include anchor type (`left`/`right` semantics available in guess output context).
- Bundle governance must encode expected outcome (`improve` vs `neutral`) and interpret regression accordingly.
- Regression response policy requested by user: investigate, rollback bundle changes, and output root cause + evidence data.

Partially resolved / requires further specification:
- User asks for an anchor-accuracy numerical confidence metric plus condition-level reason codes.
- User requests a new averaged synthetic random-seed non-regression gate (`10+` seeds), but asks for a statistically safe seed count after variance validation.
- User requested deeper plain-language definitions for:
  - why run/hint source logic exists if the PDF is text-based,
  - what source tiers/ties mean,
  - what each tie-break key means in operational terms.

## 11) Investigation Log Addendum (Round 4 pre-question pass)
- Verified root cause for non-trivial anchor source handling:
  - redaction-side underlying text stream uses approximated geometry (`font_size * 0.6 * char_count`) rather than per-glyph advances (`src/dependency/pdf_redaction/text_parser.rs:177`), so context text and run geometry can diverge.
  - run-side geometry has bbox + optional per-char advances and remains the higher-fidelity anchor coordinate source (`src/bin/synthetic_overfitting_benchmark.rs` evidence references unchanged; run logic at `guess_logic.rs:2264-2352`).
- Revalidated deterministic pair tie-break ordering and criteria terms from current code (`guess_logic.rs:2032-2069`).
- Added seed-variance investigation note:
  - `.local-agent/runtime/investigations/synthetic_seed_variance_round4.md`
  - exploratory seed sample (n=5) showed substantial variance (`sd_r20=0.03563`, `sd_mrr=0.006142`).
  - strict random-seed mean no-regression gating is statistically noisy without tolerance (empirical false-regression rate near 0.5 in bootstrap when comparing independently sampled means with equal true performance).

## 12) Blockers (current after round 4)
1. Exact normalization contract for `run_exact` is not fully machine-specified (character classes, punctuation mapping, whitespace collapsing rules, and final emitted normalized form behavior).
2. `run_prefix_projection` validity contract remains partially undefined:
   - what qualifies as "valid run mapping" for source labeling,
   - what minimum evidence/quality is required before preferring run-derived x.
3. Final source/tie terminology contract is not finalized in user-facing schema terms (code order accepted; naming/schema still open).
4. Final enum naming/schema for `AnchorSelectionReasonCode` and source labels is not finalized.
5. Anchor accuracy metric contract is not finalized (field names, scale/range, per-side vs per-row aggregation, and whether this metric is gate-binding or diagnostic).
6. Guesses-to-anchors reference contract is not finalized (which anchor identifier and which per-side fields are duplicated in guesses).
7. New multi-seed synthetic no-regression gate is not finalized:
   - strict/tolerant statistical rule,
   - statistically safe seed count,
   - deterministic reproducibility policy.
8. Bundle-intent-aware threshold policy is directionally set but not yet machine-checkable in explicit rule form.

## 13) User Response Round 5 (2026-03-02)
Resolved decisions:
- `run_exact` should preserve punctuation and newline characters from document text; comparison normalization should be limited to transport/text-format normalization (encoding/line-ending canonicalization), not punctuation removal.
- “Exact” means match the document text form, including newline boundaries.
- `run_prefix_projection` policy delegated to planner; user requested forward progress without further discussion.
- Confidence outputs should include all three levels:
  - `left_anchor_confidence`
  - `right_anchor_confidence`
  - `row_anchor_confidence`
- Confidence range accepted: `0.0..1.0`.
- Each candidate should emit one final decision reason code that explains why it was selected or rejected.
- Existing source enum names should remain unchanged for now.
- Guesses artifact should contain compact per-side anchor summary:
  - left/right identity context,
  - anchor type,
  - selected source label,
  - confidence.
- Multi-seed synthetic average output should be required and continuously reported over time (diagnostic visibility requirement).
- Seed-panel size and tolerance direction accepted:
  - at least `N=20`,
  - acceptable thresholds from prior proposal (`r@20` and `mrr` tolerance values) accepted.
- Hard thresholds should include a `1%` margin of error band.
- No retry loop on hard regression: investigate then rollback every regression outside tolerance.

Planner-selected clarifications (user-delegated, evidence-driven):
- `run_prefix_projection` is treated as valid when:
  - hint text is found as a contiguous substring in run text,
  - and a finite non-negative projected offset can be computed from run-side sources.
- Projection offset source precedence for validity/confidence:
  1) `char_advances_pt` sum (highest confidence),
  2) measured typography width from run font metadata (medium confidence),
  3) proportional run-bbox fallback (lowest confidence; still explicit as estimated).

## 14) Blocker Resolution (after round 5)
All Phase 1 blockers are now closed for planning:
1. `run_exact` normalization intent is now explicit (format normalization only; punctuation/newlines preserved).
2. `run_prefix_projection` validity semantics have a fixed planner contract.
3. Source/tie taxonomy naming accepted as current names.
4. Reason model direction fixed to `was_selected` + single `AnchorSelectionReasonCode`.
5. Confidence scope/range accepted (`left/right/row`, `0.0..1.0`).
6. Guesses-to-anchors compact reference fields are defined.
7. Multi-seed gate baseline direction fixed (`N>=20`, thresholded, always reported).
8. Bundle-intent + rollback governance now machine-checkable (with tolerance and no-retry rollback policy).

## 15) Bundle Materialization Status
- Approved bundle sequence has been materialized under `.local-agent/bundles/`.
- Each bundle now includes:
  - `.agent/Prompt.md`
  - `.agent/Plans.md`
  - `.agent/Implement.md`
  - `.agent/Documentation.md`
  - `MANIFEST.json`
- Implement runbooks include context reload protocol, doctrine enforcement checklist, stop-and-fix gates, no-retry regression policy, and mandatory command matrix.

## 16) Incident Intake Addendum (2026-03-02) — Determinism + Anchor Alignment
Input Doc for this cycle is user-provided incident text (no file), anchored as:
- InputDoc §A: "anchors seem to not 100% line up and accuracy is super low"
- InputDoc §B: "improve determinism ... find where error is coming from (anchor decisions, visuals, guessing, etc)"
- InputDoc §C: "multi-lines are still screwed up ... visualizing anchors multiple times ... overlap"
- InputDoc §D: explicit investigation instruction to render and inspect visualized outputs for:
  - `test_data/EFTA00038617.pdf`
  - `test_data/EFTA00101126.pdf`

### 16.1 Investigation Evidence (local-only)
Primary evidence file:
- `.local-agent/runtime/investigations/determinism_anchor_alignment_incident_probe.md`

Generated artifacts:
- `.local-agent/runtime/investigations/determinism_probe/EFTA00038617/*`
- `.local-agent/runtime/investigations/determinism_probe/EFTA00101126/*`
- `.local-agent/runtime/investigations/determinism_probe/repeatability/repeatability_hashes.json`
- `.local-agent/runtime/investigations/determinism_probe/anchor_reuse_anomaly_report.json`
- `.local-agent/runtime/investigations/determinism_probe/multiline_guess_report.json`
- `.local-agent/runtime/investigations/determinism_probe/current_guess_accuracy.json`

### 16.2 Structured Facts from This Pass
- Visual mismatch is reproducible on required files.
- `EFTA00038617` shows severe dense-row overlay overlap caused by distant/shared anchors reused across adjacent redactions.
- `EFTA00101126` has false-positive raster redactions on red UI buttons (`Print`, `Save As...`, `Reset`) causing obvious wrong overlays.
- Determinism check:
  - raw output file hashes differ run-to-run,
  - after removing diagnostics/timing fields, guess and anchor decision payloads match exactly.
- Current accuracy snapshot remains low:
  - overall `recall@1=0.0`, `recall@5=0.1818`, `recall@20=0.4545`, `mrr=0.0854`.

### 16.3 Code-Trace Findings (source-level)
- Anchor selection risk path:
  - shared hint merge across linked rows (`build_shared_context_hints`)
  - broad hint gap allowance (`CLUSTER_ANCHOR_HINT_MAX_GAP_PT=240`)
  - candidate selection can prefer non-local anchors in dense rows
  - file: `src/logic/redaction_guessing_component/guess_logic.rs`
- Visualization overlap path:
  - dense rows still render anchor-prefixed/suffixed text in selected-only overlay path
  - file: `src/data/visualization_data.rs`
- False-positive raster path:
  - raster dark-region detection treats non-black UI elements as redactions
  - file: `src/data/redaction_scan_data.rs` (plus downstream raster dependency)

### 16.4 Blockers (incident cycle, pre-question round)
1. Acceptance contract for raster false-positive suppression is undefined (what classes of dark regions are valid redactions vs UI/decorative elements).
2. Dense-row visualization contract is undefined (selected-text-only vs anchor+selected composite rendering behavior).
3. Locality constraint for anchor reuse is undefined (max acceptable left/right anchor gap for two-sided and one-sided modes).
4. Cross-row hint sharing policy is undefined for dense clusters (when to merge vs isolate row-local hints).
5. Artifact determinism scope is undefined (whether diagnostics/timing must be excluded or normalized for stable full-file hashes).
6. Multiline handling acceptance criteria are still not concrete for this incident (visual-only expectation vs anchor/guess contract requirement).

## 17) User Response Round 6 (2026-03-02)
Resolved directives from latest response:
- Raster validity policy preference: black-only is valid for target redaction class.
- Architecture direction: re-think current split behavior and move toward a simpler unified redaction-set workflow (discover neighboring redactions, then resolve anchors consistently across the set).
- Anchor locality direction: prioritize sentence-local anchors with minimal distance (current file examples are single-whitespace adjacency); focus this high-confidence case first.
- Determinism contract direction: strict same input + same dictionary + same tunables => exactly same output.
- Visualization quality direction:
  - anchored side should line up perfectly,
  - when two anchors exist, read/place from left-to-right,
  - visual output should make guess error explicit.
- Bundle process direction: include explicit before/after rendered-PDF visual inspection steps as part of implementation validation.
- Scope direction: focus on high-confidence text-based PDFs and high-reliability redaction detection; simplify service where possible.
- Success criteria confirmation: all three previously proposed incident criteria are accepted.

## 18) Investigation Addendum (Round 6 deep pass)
Deep trace evidence file:
- `.local-agent/runtime/investigations/determinism_anchor_set_workflow_trace.md`

Additional artifacts:
- `.local-agent/runtime/investigations/determinism_probe/cluster_hint_pollution_report.json`

Evidence-backed findings:
- Current anchor path includes multiple layered behaviors:
  - per-redaction anchor selection with two-sided + one-sided fallback,
  - cross-row cluster hint sharing,
  - post-selection row/cluster consensus and joint assignment.
- Largest observed cross-row cluster in required files:
  - `EFTA00038617` page index 1: 10 redactions sharing merged context pool.
- Cluster-level hint sharing and broad hint-gap acceptance are sufficient to explain deterministic but wrong local anchor picks in dense rows.
- This confirms user concern that current design complexity is a principal error source for alignment reliability.

## 19) Blockers (updated after round 6)
1. Unified redaction-set contract is not yet machine-specified:
   - exact neighbor discovery rules,
   - set boundary termination,
   - per-set ordering guarantees.
2. Sentence-local anchor adjacency contract is not fully specified numerically ("single whitespace" intent must become measurable geometry/text constraints).
3. No-anchor policy for low-confidence rows is not finalized (when to abstain vs when to fallback).
4. Dense-row visualization contract is not finalized (selected-only text vs anchor-composite rendering under high-density sets).
5. Black-only raster rule is directionally accepted, but exact threshold/definition is not finalized (color/luma/chroma/opacity handling).
6. Full-artifact determinism contract is not finalized (diagnostics/timing normalization and serialization ordering policy).
7. "Perfect anchored-side alignment" success metric is not yet machine-checkable (visual geometry assertions/gates not yet defined).

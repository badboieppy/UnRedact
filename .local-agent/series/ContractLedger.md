# ContractLedger
Generated: 2026-03-02
Only explicitly evidenced contracts are listed as concrete contracts. Unknowns are marked as unknown.

| Contract Ledger ID | Contract ID / Key | Surface | Invariants / Rules | Verification Evidence |
|---|---|---|---|---|
| CON-001 | C-KNOWN-REDACTION-TARGETS-V1 | Known-target benchmark contract | Dataset + target structure validated by code. | src/benchmarks/types/known_redaction_contract.rs |
| CON-002 | C-SYNTHETIC-SEED-TIERS | Synthetic anti-overfitting protocol | Fixed seed binding + exploratory diagnostic + run completeness gate. | src/bin/synthetic_overfitting_benchmark.rs |
| CON-003 | C-INVESTIGATIVE-PROOF-FIRST | Evidence dossier approval contract | Completeness + contract alignment + disconfirmation checks. | src/benchmarks/types/evidence_dossier_contract.rs |
| CON-004 | C-BEST-FEASIBLE-K-RULE | Consumed evidence contract | Required consumed contract ID. | src/benchmarks/types/evidence_dossier_contract.rs |
| CON-005 | C-BASELINE-NO-REGRESSION | Consumed evidence contract | Required consumed contract ID. | src/benchmarks/types/evidence_dossier_contract.rs |
| CON-006 | context_spans_json_v1 | Internal redaction->guess context metadata contract | Context spans encoded/parsed via redaction meta. | src/data/redaction_scan_data.rs + src/logic/redaction_guessing_component/guess_logic.rs |
| CON-007 | GuessReport / GuessContext schema (no external ID) | Current guess artifact schema | Anchor summary fields present; structured reason enums absent. | src/types/guess_types.rs |
| CON-008 | Output artifact naming contract (implicit code contract) | CLI file outputs | `.redactions.json`, `.fonts.json`, `.guesses.json`, optional `.visualized.pdf`. | src/logic/local_file_workflow_component.rs:27-37 |
| CON-009 | Unknown (required by InputDoc) | Canonical anchor source taxonomy contract | `run_exact`, `run_prefix_projection`, `hint_only_fallback` not formalized yet. | InputDoc: 3.3 |
| CON-010 | Unknown (required by InputDoc) | Structured reason-code enum contract | Full machine-readable rejection/selection reason enum set not formalized yet. | InputDoc: 3.4 |
| CON-011 | User directive | Schema migration policy | Breaking schema allowed for this effort. | User response round 1 |
| CON-012 | User directive | Trace completeness policy | Emit all anchor alternatives/rejections. | User response round 1 |
| CON-013 | User directive | Contract-change policy | Only change contracts when strictly required. | User response round 2 |
| CON-014 | User directive | Benchmark baseline policy | Run full benchmark set now and treat outputs as baseline. | User response round 2 |

| CON-015 | User directive | Guess artifact anchor visibility contract | Guess rows must retain anchor-relevant fields even with separate anchor artifact. | User response round 3 |
| CON-016 | User directive | Artifact parity contract (CLI + web) | Anchor artifact should be produced in both delivery modes with shared service abstraction. | User response round 3 |
| CON-017 | User directive | Validation command contract | Five-command validation set is mandatory. | User response round 3 |
| CON-018 | User directive | `run_exact` matching contract | Use normalized punctuation/whitespace + case-insensitive matching for loose run-exact resolution. | User response round 4 |
| CON-019 | User directive | Anchor selection reason schema direction | Use `wasSelected` + `AnchorSelectionReasonCode` across selected/non-selected candidates. | User response round 4 |
| CON-020 | User directive | Anchor identity exposure contract | Anchor ID should exist and anchor type context should appear in guesses. | User response round 4 |
| CON-021 | Unknown (user-directed but not yet formalized) | Multi-seed synthetic average non-regression gate | User wants 10+ random-seed average no-regression; statistical gate rule and safe N pending formalization. | User response round 4 + `.local-agent/runtime/investigations/synthetic_seed_variance_round4.md` |
| CON-022 | User directive | Bundle regression response contract | Bundle expectations must be tagged (`improve`/`neutral`); regressions trigger investigate + rollback + evidence output. | User response round 4 |
| CON-023 | User directive (updated) | `run_exact` text-normalization contract | Preserve punctuation/newlines; normalization limited to encoding/line-ending canonicalization for comparable text format. | User response round 5 (supersedes loose round-4 setting). |
| CON-024 | Planner-selected (user-delegated) | `run_prefix_projection` validity contract | Valid when hint is contiguous substring in run text and finite non-negative projected offset is derivable from run-side sources; projection provenance must be emitted. | User response round 5 + `guess_logic.rs:2285-2352`. |
| CON-025 | User directive | Confidence schema contract | Emit `left/right/row` anchor confidence in range `0.0..1.0`. | User response round 5 |
| CON-026 | User directive | Decision reason contract | One final `AnchorSelectionReasonCode` per candidate with `was_selected` boolean indicating selected vs rejected outcome. | User response round 5 |
| CON-027 | User directive | Guesses compact anchor summary contract | Guesses rows carry per-side compact fields (type, selected source, confidence, side-linked anchor context) while full detail remains in `anchors.json`. | User response round 5 |
| CON-028 | User directive | Multi-seed gate formalization contract | Required multi-seed reporting with `N>=20` and accepted tolerance thresholds (`r@20`/`mrr`), plus hard-threshold margin band. | User response round 5 + `.local-agent/runtime/investigations/synthetic_seed_variance_round4.md` |
| CON-029 | User directive | No-retry regression action contract | Regressions outside tolerance trigger investigate + rollback (no retry loop). | User response round 5 |
| CON-030 | User directive (incident cycle) | Incident evidence acquisition contract | Must render and inspect visualized PNG outputs for `EFTA00038617` and `EFTA00101126` during investigation. | User input §D + `.local-agent/runtime/investigations/determinism_anchor_alignment_incident_probe.md` |
| CON-031 | Observed runtime behavior | Decision-content determinism contract (current observed state) | Core anchor/guess decision payloads are deterministic across repeated runs when diagnostics/timing fields are normalized out. | `.local-agent/runtime/investigations/determinism_probe/repeatability/repeatability_hashes.json` + normalized compare in probe notes |
| CON-032 | Unknown (needs approval) | Full-artifact determinism contract | Whether diagnostics/timing fields must be normalized/removed to make full output file hashes stable is not yet decided. | Incident blocker list (ContextDigest §16.4) |
| CON-033 | Unknown (needs approval) | Anchor locality acceptance contract | Maximum acceptable left/right anchor gap per mode (`two_sided`, `left_only`, `right_only`) is not yet defined. | Incident blocker list + `anchor_reuse_anomaly_report.json` |
| CON-034 | Unknown (needs approval) | Dense-row visualization contract | For dense rows, whether overlays should render selected guess only or include anchor words is not yet fixed. | Incident blocker list + visual inspection evidence |
| CON-035 | Unknown (needs approval) | Raster false-positive suppression contract | Criteria for rejecting non-redaction dark regions (e.g., colored UI controls) are not yet defined. | Incident blocker list + EFTA00101126 p002 evidence |
| CON-036 | User directive (incident cycle) | Raster validity contract (high-confidence mode) | Black-only raster regions are the accepted reliable class for this incident-focused anchor reliability track. | User response round 6 (Q1) |
| CON-037 | User directive (incident cycle) | Unified redaction-set anchor workflow contract (directional) | Planning should collapse split single/multi behavior into a simpler set-based anchor workflow centered on neighboring-redaction grouping. | User response round 6 (Q2/Q4/Q5) |
| CON-038 | User directive (incident cycle) | Strict determinism contract | Same file + dictionary + tunables must produce identical outputs; random behavior is disallowed. | User response round 6 (Q7) |
| CON-039 | User directive (incident cycle) | Anchored-side visualization alignment contract | Anchored side must align perfectly; two-anchor rendering follows left-to-right reading direction to expose guess error visually. | User response round 6 (Q8) |
| CON-040 | User directive (incident cycle) | Visual validation process contract | Bundle runbooks must include before/after rendered-PDF inspection steps as required evidence. | User response round 6 (Q8) |
| CON-041 | User directive (incident cycle) | Reliability-first scope contract | Focus on high-confidence text PDFs; choose the highest reliability redaction detection method, favoring visual if measurably superior. | User response round 6 (Q9) |
| CON-042 | User directive (incident cycle) | Incident acceptance contract | Accepted success set: false-positive suppression on target case, dense-row overlap resolution on target pages, and benchmark non-regression. | User response round 6 (Q10) |
| CON-043 | Ledger clarification | Determinism-contract supersession note | CON-032 open question is superseded by CON-038 directive; implementation detail (how to normalize diagnostics/timing) remains a technical design choice under strict determinism. | ContextDigest §19 + user round 6 |

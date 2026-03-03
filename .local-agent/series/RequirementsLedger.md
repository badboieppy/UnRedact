# RequirementsLedger
Generated: 2026-03-02
Source of truth for this ledger: docs/anchor_geometry_reliability_design.md + user incident intake (2026-03-02) + explicit repository contracts and user-approved directives.

| REQ ID | Requirement (near-original wording) | Source Anchor | Evidence Snapshot |
|---|---|---|---|
| REQ-001 | Accuracy target: top-1 on canonical known redactions. | InputDoc: 1.4 | Known-target contract `C-KNOWN-REDACTION-TARGETS-V1`. |
| REQ-002 | Fallback objective: smallest K where recall@K = 1.0. | InputDoc: 1.4 | `best_feasible_k` reported by benchmark outputs. |
| REQ-003 | Determinism: same input + config + code must produce identical outputs. | InputDoc: 1.4 | Determinism gate exists in accuracy benchmark payload. |
| REQ-004 | Anti-overfitting: evaluate synthetic redactions across all PDFs in `test_data/`. | InputDoc: 1.4 | Synthetic protocol source pool policy is all test_data PDFs. |
| REQ-005 | No-regression policy against locked baselines. | InputDoc: 1.4 | Evidence contract consumes baseline no-regression contract. |
| REQ-006 | Evidence-first policy for algorithm behavior changes. | InputDoc: 1.4 | Evidence gate validates dossier schema and disconfirmation. |
| REQ-007 | Performance soft-gated until recall>=0.8, then hard-gated. | InputDoc: 1.4 | Explicit design requirement; gate wiring remains to be planned. |
| REQ-008 | Keep both extraction streams (underlying text + font runs). | InputDoc: 3.1, 3.2 | Both streams active in orchestrator. |
| REQ-009 | One authoritative anchor geometry decision per row with explicit source semantics. | InputDoc: 3.1, 3.2 | Target requirement. |
| REQ-010 | Preserve alternative candidates + rejection metadata. | InputDoc: 3.2 | Target requirement. |
| REQ-011 | Stable source taxonomy: `run_exact`, `run_prefix_projection`, `hint_only_fallback`. | InputDoc: 3.3 | Target requirement; predicates unresolved. |
| REQ-012 | If run mapping is valid, run-derived geometry is preferred. | InputDoc: 3.3 | Target requirement; current superset hint-x branch conflicts in some cases. |
| REQ-013 | Hint-derived geometry only as explicit fallback when run mapping unavailable/invalid. | InputDoc: 3.3 | Target requirement. |
| REQ-014 | Emit selected source label, coordinate, alternate coordinate, delta. | InputDoc: 3.3 | Target requirement. |
| REQ-015 | Emit machine-readable decision record with selected mode/text/coords/source + rejected reason codes. | InputDoc: 3.4 | Target requirement. |
| REQ-016 | Reason codes stable across runs and machine-parseable. | InputDoc: 3.4 | User selected enum-like model. |
| REQ-017 | Reason-code coverage must explain mode/source flips. | InputDoc: 3.4 | Target requirement. |
| REQ-018 | Ranking keeps effective-error ordering and raw error availability. | InputDoc: 3.5 | Current code behavior confirms this. |
| REQ-019 | Row-level traces first-class, deterministic, diffable. | InputDoc: 3.5 | Target requirement. |
| REQ-020 | Known-target objective remains top-1; fallback K minimized and non-regressed unless approved. | InputDoc: 3.6 | Target requirement. |
| REQ-021 | Fixed-seed synthetic tier binding; exploratory tier diagnostic. | InputDoc: 3.6 | Protocol currently enforces this split. |
| REQ-022 | Changes accepted only with measurable disconfirmable evidence vs locked baseline. | InputDoc: 3.6 | Evidence contract enforces this model. |
| REQ-023 | Breaking schema is acceptable for this effort. | User response round 1 | Explicit user directive. |
| REQ-024 | Emit full decision/rejection trace coverage (no partial omission). | User response round 1 | Explicit user directive. |
| REQ-025 | Mixed per-side source labels are valid and should be enum-backed. | User response round 1 | Explicit user directive. |
| REQ-026 | Full anchor decision data should be in separate `anchors.json` output. | User response round 2 | Explicit user directive. |
| REQ-027 | Guesses output must still include anchor info for quick human visibility/reference. | User response round 2 | Explicit user directive. |
| REQ-028 | Contract changes allowed only when strictly required. | User response round 2 | Explicit user directive. |
| REQ-029 | Run full benchmark set now and use outputs as current baseline for future thresholding. | User response round 2 | Explicit user directive. |
| REQ-030 | Tests must be hard pass/fail; benchmarks must provide both metrics and hard pass/fail thresholds. | User response round 2 | Explicit user directive; threshold definitions unresolved. |

| REQ-031 | Guesses output must retain anchor info relevant per guess to explain guess behavior impact quickly. | User response round 3 | Explicit user directive. |
| REQ-032 | Both CLI and web outputs should produce anchor artifact outputs with shared abstraction in service/output layer. | User response round 3 | Explicit user directive. |
| REQ-033 | Hard pass policy is bundle-intent-aware: no metric regressions allowed; metric improvement only required when bundle goal is behavioral improvement. | User response round 3 | Explicit user directive. |
| REQ-034 | Mandatory validation commands are fixed to the 5-command set confirmed by user. | User response round 3 | Explicit user directive. |
| REQ-035 | `run_exact` should use loose text matching with normalized punctuation/whitespace + case-insensitive comparison. | User response round 4 | Explicit user directive (normalization specifics still needs formal schema). |
| REQ-036 | Anchor decision outputs should include an anchor-accuracy numerical confidence metric and condition-level reason codes where possible. | User response round 4 | Explicit user directive (metric formula unresolved). |
| REQ-037 | Existing deterministic tie-break order is acceptable if all decisions are emitted and traceable. | User response round 4 | Explicit user directive. |
| REQ-038 | Candidate decision model should be represented with `wasSelected` + `AnchorSelectionReasonCode` rather than split happy/non-happy-path enums. | User response round 4 | Explicit user directive. |
| REQ-039 | Anchor records must include anchor ID; guesses output must include anchor type context (left/right relevance). | User response round 4 | Explicit user directive. |
| REQ-040 | Add an averaged synthetic random-seed non-regression check using 10+ seeds, with statistically safe sizing validated before binding. | User response round 4 | Explicit user directive; safe N and gate rule unresolved. |
| REQ-041 | Bundle plans must encode expected metric intent (`improve` or `neutral`) and evaluate results in that context. | User response round 4 | Explicit user directive. |
| REQ-042 | If a regression occurs, bundle workflow should investigate, rollback the bundle changes, and output discoveries/root causes/evidence data. | User response round 4 | Explicit user directive. |
| REQ-043 | `run_exact` should preserve punctuation and newline characters; normalization should only canonicalize text transport/encoding form rather than removing punctuation. | User response round 5 | Explicit user directive; supersedes round-4 loose normalization preference. |
| REQ-044 | “Exact match” semantics should include newline boundaries from document text (no newline collapsing/removal for exactness). | User response round 5 | Explicit user directive. |
| REQ-045 | `run_prefix_projection` validity policy is delegated; planner should choose and lock a deterministic policy to avoid additional back-and-forth. | User response round 5 | Explicit user delegation to planner. |
| REQ-046 | Confidence metrics should be emitted at three levels: left side, right side, and row aggregate. | User response round 5 | Explicit user directive. |
| REQ-047 | Confidence values should use range `0.0..1.0`. | User response round 5 | Explicit user directive. |
| REQ-048 | Each candidate should emit one final decision reason code representing the decision made (selected or rejected reason). | User response round 5 | Explicit user directive. |
| REQ-049 | Keep current source enum names for now. | User response round 5 | Explicit user directive. |
| REQ-050 | Guesses artifact should include compact per-side anchor summary fields: side context, anchor type, selected source, and confidence. | User response round 5 | Explicit user directive. |
| REQ-051 | Synthetic multi-seed average output should be required as ongoing diagnostics. | User response round 5 | Explicit user directive. |
| REQ-052 | Multi-seed evaluation should use at least `N=20` with accepted tolerance thresholds from proposal. | User response round 5 | Explicit user directive. |
| REQ-053 | Hard thresholds should include a `1%` margin-of-error band. | User response round 5 | Explicit user directive. |
| REQ-054 | No retry on hard regressions; regressions outside tolerance must trigger investigate + rollback. | User response round 5 | Explicit user directive. |
| REQ-055 | Improve system determinism for anchor/guess outputs; investigate and remove sources of unstable behavior. | InputDoc §A, §B | User explicitly requested determinism improvements and root-cause investigation. |
| REQ-056 | Investigate alignment failures end-to-end across anchor decisions, visualization, and guessing logic. | InputDoc §B | User explicitly requested root-cause localization by subsystem. |
| REQ-057 | Visualization anchors should line up with redaction geometry; significant overlap/misalignment is unacceptable. | InputDoc §A, §C | User reported severe overlap and misalignment in rendered outputs. |
| REQ-058 | Multi-line/dense-row rendering behavior must avoid duplicate/stacked overlays that obscure text and redactions. | InputDoc §C | User reported anchors visualized multiple times with text overlap. |
| REQ-059 | Required investigation evidence includes rendered PNG inspection of visualized outputs for `EFTA00038617` and `EFTA00101126`. | InputDoc §D | User explicitly requested rendering/inspection of those files. |
| REQ-060 | Accuracy should improve from current low state; incident planning must include measurable validation against benchmark metrics. | InputDoc §A | User explicitly stated current accuracy is super low and asked how to improve determinism/quality. |
| REQ-061 | False-positive raster detections (non-redaction UI/decorative regions) should be identified and addressed as part of alignment reliability. | Investigation evidence (EFTA00101126 p002) | Probe found red UI buttons detected as redactions, producing incorrect overlays. |
| REQ-062 | Decision-content determinism and full-artifact determinism scope must be explicitly defined (diagnostics/timing normalization policy). | Investigation evidence + InputDoc §B | Repeatability check showed payload stability but raw-file hash drift from diagnostics/timing fields. |
| REQ-063 | Raster redaction detection for this reliability track should treat black-only regions as valid high-confidence redactions. | User response round 6 (Q1) | User explicitly approved black-only validity direction. |
| REQ-064 | Simplify anchor architecture to a unified redaction-set workflow rather than separate implicit single-vs-multi handling paths. | User response round 6 (Q2/Q4/Q5) | User explicitly requested a simpler unified set-based design. |
| REQ-065 | Anchor selection should prioritize sentence-local anchors with minimal adjacency distance (high-confidence single-whitespace neighborhood case first). | User response round 6 (Q3) | User explicitly constrained focus to intuitive sentence-local anchors in current file class. |
| REQ-066 | Not all redactions require anchors; system should prioritize high-confidence anchor findings and avoid overfitting edge-case fallbacks. | User response round 6 (Q6/Q9) | User explicitly pushed back on excessive edge-case handling and requested high-confidence-only behavior focus. |
| REQ-067 | Determinism requirement is strict: same file + same dictionary + same tunables must produce exactly the same output artifacts. | User response round 6 (Q7) | User explicitly rejected any random/nondeterministic behavior. |
| REQ-068 | Visualization must align perfectly on anchored side; for two-anchor rows, placement order should follow left-to-right reading direction to make guess error visually explicit. | User response round 6 (Q8) | User explicitly defined desired visualization alignment behavior. |
| REQ-069 | Implementation bundles should include explicit before/after rendered-PDF visual inspection steps as validation evidence. | User response round 6 (Q8) | User explicitly requested visual double-check process in bundle runbooks. |
| REQ-070 | Scope priority is high-confidence text-based PDFs; use the highest reliability redaction-finding method and prefer visual detection when it proves more reliable than metadata dependency. | User response round 6 (Q9) | User explicitly directed reliability-first method choice and simplification. |
| REQ-071 | Incident success criteria are accepted as: no specified false positives, no specified dense-row overlap on target pages, and benchmark non-regression. | User response round 6 (Q10) | User accepted all three proposed incident criteria. |

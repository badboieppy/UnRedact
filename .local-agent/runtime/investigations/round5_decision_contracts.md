# Round 5 Decision Contracts (Machine-Checkable Draft)
Date: 2026-03-02
Basis: user response round 5 + prior variance evidence

## 1) `run_exact` text comparison contract
- Preserve punctuation characters.
- Preserve newline boundaries.
- Comparison normalization is limited to transport canonicalization:
  - decode to canonical UTF-8 representation,
  - canonicalize line endings to `\n` for internal compare,
  - no punctuation stripping,
  - no newline removal.

## 2) `run_prefix_projection` validity contract (planner-selected, user-delegated)
Projection is considered valid when:
1. `hint_text` occurs as a contiguous substring in `run_text`.
2. A finite non-negative x-offset can be derived from run-side geometry by precedence:
   - char-advance sum,
   - measured typography width,
   - proportional bbox fallback.
3. Offset provenance must be emitted for confidence attribution and reason coding.

## 3) Confidence contract
- Emit:
  - `left_anchor_confidence`
  - `right_anchor_confidence`
  - `row_anchor_confidence`
- Each value must be in `[0.0, 1.0]`.

## 4) Candidate decision-reason contract
- Emit one final `AnchorSelectionReasonCode` per candidate.
- Emit `was_selected` boolean.
- Selected rows: reason indicates selection path.
- Rejected rows: reason indicates terminal rejection reason.

## 5) Guesses compact anchor summary contract
Per guess row, include per-side compact anchor fields:
- side-linked anchor identity context,
- anchor type,
- selected source label,
- confidence value.
Full candidate/decision detail remains in `anchors.json`.

## 6) Synthetic multi-seed contract
- Always emit multi-seed diagnostics.
- Binding average panel uses at least `N=20` seeds.
- Accepted tolerance thresholds (from approved proposal):
  - `avg_recall_at_20_delta >= -0.01`
  - `avg_mrr_delta >= -0.002`
- Apply 1% margin band to hard threshold boundaries:
  - effective `avg_recall_at_20_delta >= -0.0101`
  - effective `avg_mrr_delta >= -0.00202`

## 7) Regression action contract
- If any hard gate fails outside allowed tolerance:
  - investigate immediately,
  - rollback bundle change set,
  - emit discoveries, root cause, and evidence artifacts.
- No retry loop for hard regression adjudication.

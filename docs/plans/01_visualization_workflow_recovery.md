# Problem 1 Plan: Visualization Workflow Recovery And Candidate Filtering

## Audience
New engineers joining the `UnRedact` project who need enough context to safely change visualization and guess validation flow.

## Date
2026-02-17

## Problem Statement
Visualization output regressed:
- guessed text is often drawn above redaction boxes instead of inside them,
- before and after anchor text is missing for rows where it should be shown,
- workflow order is wrong for quality and performance.

Expected workflow:
1. Render before and after context first, and measure alignment error in known text.
2. Use that geometry to filter dictionary candidates by width for the row font and shaping settings (`kern`, `liga`, `clig`).
3. Run expensive visual validation only on filtered candidates by rendering guess text inside the redaction box while keeping row alignment.

## Current System Behavior
### Current orchestration path
- `src/logic/orchestrator.rs` computes redactions, fonts, guesses, then optionally visualization PDF.
- `apply_visual_scores` is called after candidate generation and consensus.

### Current visualization behavior
- `src/data/visualization_data.rs` `build_overlays` has a hard branch:
  - for `RedactionKind::RasterDarkRegion`, it emits one overlay (guess text inside redaction box),
  - it immediately `continue`s and skips before and after overlays.
- This means raster rows with good anchors still do not render before and after context.
- In-box baseline placement uses fixed ratios in `raster_overlay_layout`:
  - `RASTER_MAX_FONT_TO_BOX_HEIGHT = 0.80`
  - `RASTER_BASELINE_ASCENT_RATIO = 0.75`
- This ratio-only baseline can place glyph ink above the box for real fonts with larger ascent.

### Observed evidence collected
- In a local run of `EFTA00101126.pdf`, page 8 redactions (indices 15 and 16) are `raster_dark_region`.
- Those rows have anchors in guess context (`left_text = "including"`, `right_text = "and"`, `guessable = true`), but current raster branch still hides before and after overlays.
- This behavior matches the reported regression.

## Expected Behavior After Fix
- If a row has valid anchors, visualization must render the full context triplet:
  - before text,
  - guess text,
  - after text.
- Guess text should be vertically placed so glyph ink stays inside redaction bounds (or within a very small tolerance).
- Candidate filtering must happen before expensive per-candidate image scoring:
  - width + anchor geometry prune first,
  - visual error scoring second.
- Rows without reliable anchors can still use in-box fallback, but must be labeled as fallback in diagnostics.

## Feasibility
High.
- Needed primitives already exist:
  - anchor extraction and font runs,
  - kerning and ligature-aware shaping in width measurement (`rustybuzz`),
  - visual scoring pipeline.
- Main work is re-ordering workflow and splitting current monolithic overlay logic into explicit stages.

## Decisions Made
1. Keep this in the normal orchestrator flow. No separate binary.
2. Replace raster-only overlay shortcut with strategy:
   - anchored row -> triplet overlay path,
   - unanchored row -> in-box fallback path.
3. Introduce explicit staged pipeline:
   - Stage A: context alignment score,
   - Stage B: width filter,
   - Stage C: expensive visual candidate scoring.
4. Preserve current JSON compatibility while adding optional diagnostics fields.

## Design
### New staged row evaluation model
Add a per-row internal structure:
- `RowEvalContext`
  - row identity: page index, redaction index, bbox
  - anchor text and positions
  - font config: font key, font size, h-scale
  - row alignment stats
- `RowWidthFilter`
  - candidate_count_in
  - candidate_count_out
  - retained_candidates (top N by width delta)
  - reject_reason stats
- `RowVisualScore`
  - candidate visual errors for retained set
  - winning candidate and score reason

### Stage A: Context-first alignment pass
For each guessable row:
1. Render only before and after overlays (no guess text yet).
2. Compute alignment error in non-redaction pixels.
3. Reject row from expensive visual scoring if context alignment is poor.
4. Keep diagnostics:
   - compared pixel count,
   - mean abs diff,
   - changed pixel ratio,
   - accept or reject reason.

Purpose:
- verify known context alignment before scoring unknown text,
- prevent noisy rows from poisoning ranking.

### Stage B: Width filtering
For rows passing Stage A:
1. Use row font config and shaping features (`kern`, `liga`, `clig`) to compute candidate width.
2. Compare candidate fit against measured anchor gap.
3. Keep only candidates within adaptive tolerance band.

Adaptive tolerance inputs:
- row residual spread (`epsilon_pt`),
- anchor confidence,
- punctuation context features.

Output:
- reduced candidate pool per row for Stage C.

### Stage C: Expensive visual candidate scoring
For each retained candidate:
1. Render triplet overlay (before + candidate + after) when anchors exist.
2. Compute pixel error in a row window, excluding redaction fill area.
3. Rank candidates by visual error + width residual.

Fallback:
- if row unanchored, use in-box guess-only scoring but mark diagnostics as fallback.

### Placement fix for text-above-box issue
Replace ratio-only baseline with font-aware baseline placement:
1. Use ascent and descent from `FontAsset` if available.
2. Solve baseline such that:
   - top ink <= box top minus padding,
   - bottom ink >= box bottom plus padding.
3. Clamp font size only after using ascent/descent metrics.

If font metrics unavailable:
- use conservative default ascent and descent with hard clip checks.

### Module boundaries
- `src/data/visualization_data.rs`
  - split into:
    - overlay builders (`build_context_overlays`, `build_triplet_overlays`, `build_fallback_overlays`)
    - layout helpers (`place_text_in_box`)
- `src/logic/visual_guess_score.rs`
  - add staged scoring entrypoints:
    - `score_context_alignment`
    - `filter_candidates_by_width`
    - `score_candidates_visually`
- `src/logic/orchestrator.rs`
  - call staged scoring in order and record diagnostics.

## Data To Collect For Better Future Accuracy
Per row:
- context alignment scores before candidate scoring,
- anchor confidence category,
- width filter retention ratio,
- candidate rank deltas before and after visual scoring,
- reason for fallback path.

Per run:
- count of rows by overlay mode (triplet vs fallback),
- count of rows where guess ink exceeds box bounds,
- histogram of width deltas.

## Testing And Benchmark Updates
### Unit tests
- `visualization_data`:
  - anchored raster row creates 3 overlays (before, guess, after),
  - fallback row creates 1 overlay,
  - baseline placement keeps glyph bounds in box.
- `visual_guess_score`:
  - context alignment rejects rows with bad known-text fit,
  - width filter reduces candidates and keeps true width-near options.

### Integration tests
- `tests/efta00101126_guessing.rs`
  - assert page 8 problematic rows use anchored triplet mode in diagnostics.
- `tests/efta00038617_guessing.rs`
  - ensure no regression in candidate pool behavior.

### Benchmark updates
- Extend `guess_accuracy_benchmark` output with:
  - `context_rows_scored`,
  - `context_rows_rejected`,
  - `width_filter_mean_retained`,
  - `triplet_overlay_rows`.

## Detailed TODO List
### Phase 0: Prepare safe refactor surface
- [ ] Add `OverlayMode` enum (`TripletAnchored`, `GuessOnlyFallback`).
- [ ] Add per-row diagnostic structs for context and width filter stages.
- [ ] Add serialization defaults so old JSON files still parse.

### Phase 1: Fix overlay strategy regression
- [ ] Refactor `build_overlays` to avoid early `continue` for all raster rows.
- [ ] Use anchored triplet path when `guess.context.has_anchor_pair == true`.
- [ ] Keep guess-only path for truly unanchored rows.
- [ ] Add explicit `overlay_mode` diagnostic per row.

### Phase 2: Fix vertical placement
- [ ] Add font metrics extraction helper for ascent and descent ratios.
- [ ] Implement `place_text_in_box_with_metrics`.
- [ ] Replace `RASTER_BASELINE_ASCENT_RATIO` usage in layout for anchored and fallback cases.
- [ ] Add clamp guard that detects and records if text still exceeds bounds.

### Phase 3: Stage A context alignment scoring
- [ ] Add context-only overlay generation function.
- [ ] Render base and context-overlay pages once per page.
- [ ] Compute context alignment error per row window.
- [ ] Add acceptance thresholds configurable in `GuessConfig`.
- [ ] Store stage results in guess diagnostics.

### Phase 4: Stage B width filtering
- [ ] Add width filter function that uses existing shaping path and row anchor gap.
- [ ] Add adaptive tolerance formula using row epsilon and anchor quality.
- [ ] Add candidate retention cap and minimum floor.
- [ ] Record filter statistics per row and per run.

### Phase 5: Stage C candidate visual scoring
- [ ] Render candidate triplet overlays only for retained candidates.
- [ ] Compute candidate-level visual score and combine with width residual.
- [ ] Re-rank candidates and update `exact_matches` order.
- [ ] Mark dropped candidates and reasons.

### Phase 6: Testing and validation
- [ ] Add unit tests for overlay mode selection.
- [ ] Add unit tests for metric-aware baseline placement.
- [ ] Add integration assertion for EFTA00101126 page 8 triplet mode behavior.
- [ ] Update benchmark JSON schema and output printer.
- [ ] Run `cargo test` and benchmark twice to verify stable behavior.

### Phase 7: Cleanup and docs
- [ ] Update README workflow section to document staged scoring order.
- [ ] Add developer note explaining fallback conditions.
- [ ] Add troubleshooting guide for rows that fail context alignment.

## Definition Of Done
- Before and after text renders for anchored raster rows.
- Guess text placement is inside redaction boxes in visualization.
- Candidate scoring follows the three-stage workflow.
- Benchmark output includes new stage metrics.
- Existing file-specific tests continue to pass.

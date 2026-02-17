# Guesser Accuracy Phase 2 Design (EFTA00038617 Focus)

## Context and Problem Summary

The current pipeline can surface candidate names for page 2 of `EFTA00038617.pdf`, but it still has three practical failures for the "Post-indictment of Epstein" first bullet list:

1. Nearby black bars are sometimes merged into fewer raster regions, reducing slot count for names.
2. Anchor text can be weak/noisy for raster regions, which causes unstable left/right context pairing.
3. Top guesses across adjacent bars can repeat or conflict, and rendered overlays can visually overlap.

This phase adds four concrete capabilities:

1. Local OCR fallback around raster bars when embedded anchors are weak.
2. Per-bar pixel profile data (column-run analysis) and split confidence, with region splitting.
3. Better raster-to-text row/baseline alignment in anchor selection.
4. Punctuation/list-aware candidate penalties for sequence-like name lists.

## Architecture: End-to-End Changes

### Redaction Detection Layer (`pdf_redaction_accessor`)

- Keep existing grid-based dark-region detection.
- Increase grid granularity to reduce false region merging.
- For each raster dark region, compute a column dark-run profile:
  - `dark_runs` (x-ranges of contiguous dark columns),
  - `max_gap_px`,
  - `split_confidence`,
  - `dark_pixel_ratio`.
- Split a wide merged dark region into multiple sub-regions when profile indicates multi-bar structure.
- Attach profile metadata into `RedactionOccurrence.meta`.

### Context Attachment Layer (`redaction_impl::attach_underlying_text`)

- Continue embedded-text context extraction first.
- If left/right anchors are weak (empty/noisy), call OCR fallback provider for local left/right windows.
- Merge OCR windows into `underlying_text` only when they improve weak anchors.

### Guesser Layer (`guess_impl`)

- Improve row run selection using stronger y+baseline+x proximity constraints, with fallback to broader mode.
- Add list/punctuation-aware penalties in candidate scoring (in addition to width error).
- Add row sequence consensus to reduce repeated names across adjacent same-row redactions.

### Visualization Layer (`visualization_data`)

- For raster dark-region redactions, place only guessed text inside bbox (centered fit).
- Do not emit left/right anchor overlays for raster bars to avoid visual overlap noise.

## Detailed Design by Requested Step

## Step 1: Local OCR Fallback Near Raster Bars

### Goal

Recover anchor text when embedded PDF text runs are weak/empty around raster regions.

### Design

- Extend `RedactionDataRetriever` with:
  - `ocr_context_hits(page_index, red_bbox, cfg) -> Result<Vec<UnderlyingTextHit>, String>`
  - Default implementation returns empty, preserving compatibility.
- Implement OCR provider in `PdfFileRetriever`:
  - Enabled only when `UNREDACT_ENABLE_LOCAL_OCR=1`.
  - Uses `PdfRenderer` page raster at `cfg.raster_dpi`.
  - Derives two windows:
    - left window before redaction,
    - right window after redaction.
  - Saves temporary crops and shells out to `tesseract` (or `UNREDACT_TESSERACT_CMD`).
  - Normalizes OCR text and returns two hits (left/right).
- In `attach_underlying_text`:
  - Build embedded context hits first.
  - If either side weak, request OCR hits and patch weak side(s).

### Failure/Compatibility

- If OCR disabled or tesseract missing/fails: return empty and continue with embedded path.
- No hard dependency on external OCR binary for tests.

## Step 2: Pixel Profile Metadata + Region Splitting

### Goal

Handle multiple redacted words in one merged raster stretch and expose richer confidence metadata.

### Design

- Compute per-region column dark ratio profile on grayscale pixels.
- Identify dark column runs and bright gaps.
- Derive:
  - `profile_dark_runs` (serialized run segments),
  - `profile_max_gap_px`,
  - `profile_split_confidence`,
  - `profile_dark_ratio`.
- Split region by dark runs when:
  - multiple runs exist,
  - confidence and dimensions indicate likely multiple bars.
- Emit each split as distinct `RedactionOccurrence`.

### Impact

- Expected increase in redaction slot count on list lines.
- Better 1:1 bar-to-name matching downstream.

## Step 3: Better Baseline/Row Alignment for Anchors

### Goal

Reduce noisy anchor pairing by selecting runs from the most plausible row near a raster bar.

### Design

- In `select_anchor_pair`:
  - Tight pass: require both y and x locality near redaction.
  - Include baseline-distance signal (`run.bbox.y1` vs `redaction.bbox.y1`).
  - If tight pass yields none, fallback to broader existing behavior.
- In pair ranking:
  - Add baseline-distance as explicit sort dimension ahead of weaker tie-breakers.

### Impact

- Fewer wrong rows selected (headers/footers/unrelated lines).
- More stable left/right anchors for list regions.

## Step 4: Punctuation/List Context Features

### Goal

Use punctuation/list cues to improve ranking when many names have similar widths.

### Design

- Add `punctuation_context_penalty(left_anchor, right_anchor, candidate)`:
  - penalize too-short/too-long candidate forms for list contexts,
  - penalize formats unlikely for surrounding punctuation (e.g., commas/parentheses mismatches),
  - bias toward full names in `included/among/served` contexts.
- Add penalty to effective candidate error used for sorting/exact extraction.
- Keep width error primary signal; punctuation is secondary bias.

### Impact

- Better ordering for list-style sequences.
- Less drift toward short tokens despite close width matches.

## Data Collection Additions

New collected fields (in `meta`) for raster regions:

- `profile_dark_runs`
- `profile_max_gap_px`
- `profile_split_confidence`
- `profile_dark_ratio`

New optional anchor source augmentation:

- OCR left/right context hits when weak embedded anchors.

## Granular TODO List

### Planning and docs

- [x] Add this design document to `docs/`.
- [x] Add explicit acceptance criteria for EFTA00038617 first-bullet list behavior.

### Step 1 OCR fallback

- [x] Extend dependency retriever trait with OCR fallback method and default no-op.
- [x] Extend data-layer wrapper trait and delegating implementation.
- [x] Add weak-anchor detection helper in redaction attach phase.
- [x] Add OCR window geometry helper (left/right windows).
- [x] Add PDF<->pixel coordinate helper used by OCR crops.
- [x] Add optional tesseract command execution helper.
- [x] Add OCR text normalization helper.
- [x] Merge OCR hits only into weak anchor sides.
- [x] Add diagnostics for OCR enabled/disabled/failure reasons.

### Step 2 pixel profile + splitting

- [x] Increase raster detection grid granularity.
- [x] Add dark column profile struct + calculator.
- [x] Add split confidence calculator.
- [x] Add split-by-profile function producing sub-regions.
- [x] Attach profile metadata into `RedactionOccurrence.meta`.
- [x] Ensure tiny/noisy splits are filtered.
- [x] Add unit tests for profile splitting on synthetic bars.

### Step 3 baseline alignment

- [x] Add tight x/y neighborhood pass for row run collection.
- [x] Add fallback broad pass for compatibility.
- [x] Add baseline distance to pair ranking data.
- [x] Incorporate baseline distance in pair sort ordering.
- [x] Add unit tests for preferring correct row when nearby text exists.

### Step 4 punctuation/list features

- [x] Add punctuation/list penalty function.
- [x] Integrate penalty into candidate error for sorting.
- [x] Keep existing context filter as hard filter; penalty is soft bias.
- [x] Add tests for list context favoring full names over short tokens.

### Sequence/no-overlap stabilization

- [x] Add row-sequence consensus pass to reduce repeated picks across adjacent bars.
- [x] Add normalization key for duplicate detection.
- [x] Add width compatibility penalty for row-sequence tie-breaking.
- [x] Ensure promotion updates both candidates and exact_matches ordering.

### Visualization improvements

- [x] Render raster-region overlays as in-box guess-only text.
- [x] Keep anchor triplet overlays for non-raster anchored rows.
- [x] Verify reduced visual overlap in list section.

### Tests and validation

- [x] Update `efta00038617_guessing.rs` expected name set to include `RICHARD BARNETT` and remove unrelated target.
- [x] Add assertions for first-bullet redaction count window.
- [x] Add assertions preventing same-row bbox overlap.
- [x] Keep `efta00101126_guessing.rs` passing as regression.
- [x] Run full `cargo test`.
- [x] Run `cargo fmt -- --check`.

## Acceptance Criteria

1. `tests/efta00038617_guessing.rs` validates the first bullet target pool:
   - `SARAH KELLEN, ADRIANA MUCINSKA, NADIA MARCINKOVA, LES WEXNER, LESLEY GROFF, JEAN LUC BRUNEL, HALEY ROBSON, WILLIAM HAMMOND, DAVID RODGERS, RICHARD BARNETT`
2. Same-row first-bullet redaction boxes are not overlapping.
3. `tests/efta00101126_guessing.rs` still passes.
4. Full suite passes and formatting check succeeds.

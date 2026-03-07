use std::collections::{BTreeMap, BTreeSet};

use crate::data::visual_score_data::{
    annotate_overlays, apply_page_crop_boxes, build_page_boxes, render_pages_to_rgba,
};
use crate::data::visualization_data::{VisualizationData, VisualizationInputs};
use crate::types::file_types::FontRunReport;
use crate::types::guess_types::{GuessReport, RedactionGuess};
use crate::types::redaction_types::{Rect, RedactionReport, RenderedPage};
use crate::types::text_overlay::TextOverlay;
use crate::types::time::Instant;

const BACKGROUND_LUMA_THRESHOLD: u8 = 245_u8;
const CHANGED_LUMA_DELTA: u8 = 24_u8;
const WINDOW_PADDING_PT: f32 = 1.0_f32;
const OVERLAY_TEXT_COLOR: [f32; 3] = [0.0_f32, 0.0_f32, 0.0_f32];
const OVERLAY_BORDER_WIDTH: f32 = 1.0_f32;
const CONTEXT_ALIGNMENT_MAX_DIFF: f32 = 0.22_f32;
const MAX_VISUAL_SCORE_DPI: f32 = 72.0_f32;
const VISUAL_TILE_PAGE_PADDING_PT: f32 = 8.0_f32;
const VISUAL_TILE_MAX_COVERAGE_FOR_CROP: f32 = 0.92_f32;
const VISUAL_RERANK_TOP_K: usize = 3;
const VISUAL_RERANK_MAX_EVAL_CANDIDATES: usize = 32;
const VISUAL_RERANK_BLEND_WEIGHT: f32 = 0.92_f32;
const VISUAL_RERANK_MAX_BASE_GAP: f32 = 0.10_f32;
const VISUAL_RERANK_MAX_TOP_SCORE: f32 = 0.90_f32;
const VISUAL_RERANK_MAX_GEOMETRIC_GAP_FOR_EVAL: f32 = 0.12_f32;
const VISUAL_RERANK_MAX_SCORE_GAP_FOR_EXPANSION: f32 = 0.18_f32;
const VISUAL_RERANK_MAX_WIDTH_DELTA_RATIO_FOR_EXPANSION: f32 = 0.08_f32;
const VISUAL_RERANK_MIN_SHIFT_PX: f32 = 0.5_f32;
const VISUAL_RERANK_MIN_GAIN_TO_REORDER: f32 = 0.02_f32;
const EDGE_BAND_PT: f32 = 1.5_f32;
const EDGE_BAND_WEIGHT: f32 = 1.8_f32;
const EDGE_INTERIOR_WEIGHT: f32 = 2.2_f32;
const REDACTION_INTERIOR_IGNORE_DARK_LUMA: u8 = 8_u8;
const EDGE_INK_LUMA_THRESHOLD: u8 = 104_u8;
const EDGE_INK_MATCH_BASE_LUMA_THRESHOLD: u8 = 176_u8;
const EDGE_INK_MISMATCH_BASE_LUMA_THRESHOLD: u8 = 236_u8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualGuessScoreConfig {
    pub enabled: bool,
    pub dpi: f32,
    pub min_ink_pixels: u32,
    pub drop_threshold: Option<f32>,
}

impl Default for VisualGuessScoreConfig {
    #[inline]
    fn default() -> Self {
        Self {
            enabled: true,
            dpi: 200.0_f32,
            min_ink_pixels: 64_u32,
            drop_threshold: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RowPixelScore {
    compared_pixels: u32,
    mean_abs_diff: f32,
    changed_pixel_ratio: f32,
    edge_ink_overlap_ratio: f32,
    edge_ink_mismatch_ratio: f32,
}

#[derive(Debug, Clone)]
struct CandidateVisualScore {
    text: String,
    score: RowPixelScore,
    blended_score: f32,
    combined_gain: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum VisualRerankGate {
    Eligible,
    InsufficientOverlays,
    InsufficientCandidates,
    MissingBoundaryText,
    InsufficientCandidateTexts,
    NonFiniteGeometric,
    TopScoreTooHigh,
    BaseGapTooLarge,
}

impl VisualRerankGate {
    #[inline]
    fn as_str(self) -> &'static str {
        match self {
            Self::Eligible => "eligible",
            Self::InsufficientOverlays => "insufficient_overlays",
            Self::InsufficientCandidates => "insufficient_candidates",
            Self::MissingBoundaryText => "missing_boundary_text",
            Self::InsufficientCandidateTexts => "insufficient_candidate_texts",
            Self::NonFiniteGeometric => "non_finite_geometric",
            Self::TopScoreTooHigh => "top_score_too_high",
            Self::BaseGapTooLarge => "base_gap_too_large",
        }
    }
}

struct RowCandidateScoringInput<'a> {
    pdf_bytes: &'a [u8],
    page_index: u32,
    base_page: &'a RenderedPage,
    page_box: Rect,
    redaction_bbox: Rect,
    template_overlays: &'a [TextOverlay],
    guess: &'a RedactionGuess,
    dpi: f32,
    min_ink_pixels: u32,
}

#[inline]
pub fn apply_visual_scores_from_bytes(
    pdf_bytes: &[u8],
    redactions: &RedactionReport,
    font_runs: &FontRunReport,
    guesses: &mut [RedactionGuess],
    cfg: VisualGuessScoreConfig,
) -> Result<Vec<String>, String> {
    if !cfg.enabled {
        return Ok(vec!["visual_score=disabled".to_owned()]);
    }
    if !cfg.dpi.is_finite() || cfg.dpi <= 0.0_f32 {
        return Err(format!("visual_score_invalid_dpi:{}", cfg.dpi));
    }
    if cfg.min_ink_pixels == 0 {
        return Err("visual_score_min_ink_pixels_must_be_positive".to_owned());
    }
    if let Some(threshold) = cfg.drop_threshold {
        if !threshold.is_finite() || threshold < 0.0_f32 {
            return Err(format!("visual_score_invalid_drop_threshold:{threshold}"));
        }
    }

    let max_items = redactions.redactions.len().min(guesses.len());
    if max_items == 0 {
        return Ok(vec!["visual_score=skipped_empty_input".to_owned()]);
    }

    let visualization = VisualizationData::new();
    let guess_report = GuessReport {
        input_redactions: String::new(),
        input_fonts: String::new(),
        guesses: guesses.to_vec(),
        anchors: Vec::new(),
        diagnostics: Vec::new(),
    };
    let inputs = visualization.load_inputs_from_bytes(
        pdf_bytes,
        redactions,
        Some(&guess_report),
        Some(font_runs),
    )?;
    apply_visual_scores_with_inputs(inputs, redactions, font_runs, guesses, cfg, max_items)
}

fn apply_visual_scores_with_inputs(
    inputs: VisualizationInputs,
    redactions: &RedactionReport,
    _font_runs: &FontRunReport,
    guesses: &mut [RedactionGuess],
    cfg: VisualGuessScoreConfig,
    max_items: usize,
) -> Result<Vec<String>, String> {
    let effective_dpi = cfg.dpi.min(MAX_VISUAL_SCORE_DPI);
    let dpi_ratio = if cfg.dpi <= 0.0_f32 {
        1.0_f32
    } else {
        (effective_dpi / cfg.dpi).clamp(0.1_f32, 1.0_f32)
    };
    let effective_min_ink_pixels =
        ((cfg.min_ink_pixels as f32 * dpi_ratio * dpi_ratio).round() as u32).max(8_u32);
    let overlays_by_redaction = group_overlays_by_redaction(&inputs.overlays);
    let page_boxes = build_page_boxes(&inputs.pdf_bytes)?;
    let rerank_enabled = cfg.enabled;

    let mut diagnostics = Vec::<String>::new();
    if overlays_by_redaction.is_empty() {
        for guess in guesses.iter_mut().take(max_items) {
            guess.visual_compared_pixels = None;
            guess.visual_mean_abs_diff = None;
            guess.visual_changed_pixel_ratio = None;
            guess.visual_reason = Some("no_overlay_for_top_guess".to_owned());
            guess.visual_dropped = false;
        }
        diagnostics.push("visual_score=scored_rows=0 dropped_rows=0 reason=no_overlays".to_owned());
        return Ok(diagnostics);
    }

    let context_overlays_by_redaction = build_context_overlays_by_redaction(&overlays_by_redaction);
    let (annotated_bytes, annotate_overlay_ms) = {
        let annotate_overlay_started = Instant::now();
        let bytes = annotate_overlays(
            &inputs.pdf_bytes,
            &inputs.overlays,
            OVERLAY_TEXT_COLOR,
            OVERLAY_BORDER_WIDTH,
        )?;
        (bytes, annotate_overlay_started.elapsed().as_millis())
    };
    let annotate_context_ms = 0_u128;
    let page_crop_boxes = build_visual_page_crop_boxes(&overlays_by_redaction, &page_boxes);
    let mut crop_fallback_reason: Option<String> = None;
    let (base_pdf_bytes_for_visual, overlay_pdf_bytes_for_visual, crop_apply_ms) =
        if page_crop_boxes.is_empty() {
            (inputs.pdf_bytes.clone(), annotated_bytes.clone(), 0_u128)
        } else {
            let crop_started = Instant::now();
            let base_cropped = apply_page_crop_boxes(&inputs.pdf_bytes, &page_crop_boxes);
            let overlay_cropped = apply_page_crop_boxes(&annotated_bytes, &page_crop_boxes);
            match (base_cropped, overlay_cropped) {
                (Ok(base_pdf), Ok(overlay_pdf)) => {
                    (base_pdf, overlay_pdf, crop_started.elapsed().as_millis())
                }
                (base_result, overlay_result) => {
                    let base_error = base_result.err().unwrap_or_else(|| "none".to_owned());
                    let overlay_error = overlay_result.err().unwrap_or_else(|| "none".to_owned());
                    crop_fallback_reason = Some(format!(
                        "visual_crop_fallback base_error={base_error} overlay_error={overlay_error}"
                    ));
                    (
                        inputs.pdf_bytes.clone(),
                        annotated_bytes.clone(),
                        crop_started.elapsed().as_millis(),
                    )
                }
            }
        };
    let mut scoring_page_boxes = page_boxes.clone();
    for (page_index, crop_box) in &page_crop_boxes {
        scoring_page_boxes.insert(*page_index, *crop_box);
    }

    let mut pages_to_render = BTreeSet::<u32>::new();
    for overlays in overlays_by_redaction.values() {
        if let Some(first) = overlays.first() {
            pages_to_render.insert(first.page_index);
        }
    }
    let renderer_init_ms = 0_u128;
    let pages_render_started = Instant::now();
    let base_pages =
        render_pages_to_rgba(&base_pdf_bytes_for_visual, &pages_to_render, effective_dpi)?;
    let overlay_pages = render_pages_to_rgba(
        &overlay_pdf_bytes_for_visual,
        &pages_to_render,
        effective_dpi,
    )?;
    let page_render_ms = pages_render_started.elapsed().as_millis();

    let mut rows_with_top_guess = 0_usize;
    let mut context_rows_scored = 0_usize;
    let mut context_rows_rejected = 0_usize;
    let mut rows_scored = 0_usize;
    let mut rows_dropped = 0_usize;
    let mut rerank_rows_considered = 0_usize;
    let mut rerank_rows_scored = 0_usize;
    let mut rerank_top1_changed = 0_usize;
    let mut rerank_gain_sum = 0.0_f64;
    let mut rerank_candidate_evals = 0_usize;
    let mut rerank_eval_ms = 0_u128;
    let mut rerank_gate_counts = BTreeMap::<&'static str, usize>::new();
    let row_scoring_started = Instant::now();
    for (index, (guess, redaction)) in guesses
        .iter_mut()
        .zip(redactions.redactions.iter())
        .enumerate()
        .take(max_items)
    {
        guess.visual_compared_pixels = None;
        guess.visual_mean_abs_diff = None;
        guess.visual_changed_pixel_ratio = None;
        guess.visual_reason = None;
        guess.visual_dropped = false;

        if top_guess_text(guess).is_none() {
            guess.visual_reason = Some("no_top_guess".to_owned());
            continue;
        }
        rows_with_top_guess += 1;

        let Some(overlays) = overlays_by_redaction.get(&index) else {
            guess.visual_reason = Some("no_overlay_for_top_guess".to_owned());
            continue;
        };
        if overlays.is_empty() {
            guess.visual_reason = Some("overlay_group_empty".to_owned());
            continue;
        }

        let Some(page_box) = scoring_page_boxes.get(&redaction.page_index).copied() else {
            guess.visual_reason = Some("page_box_missing".to_owned());
            continue;
        };
        let Some(base_page) = base_pages.get(&redaction.page_index) else {
            guess.visual_reason = Some("base_page_missing".to_owned());
            continue;
        };
        let Some(overlay_page) = overlay_pages.get(&redaction.page_index) else {
            guess.visual_reason = Some("overlay_page_missing".to_owned());
            continue;
        };

        let Some(window_bbox) = union_overlay_bbox(overlays).map(|bbox| pad_rect(bbox, page_box))
        else {
            guess.visual_reason = Some("overlay_bbox_missing".to_owned());
            continue;
        };

        if let Some(context_overlays) = context_overlays_by_redaction.get(&index) {
            if let Some(context_window_bbox) =
                union_overlay_bbox(context_overlays).map(|bbox| pad_rect(bbox, page_box))
            {
                if let Some(context_score) = score_row_overlay(
                    base_page,
                    overlay_page,
                    page_box,
                    context_window_bbox,
                    redaction.bbox,
                    effective_min_ink_pixels,
                ) {
                    context_rows_scored += 1;
                    if context_score.mean_abs_diff > CONTEXT_ALIGNMENT_MAX_DIFF {
                        context_rows_rejected += 1;
                        guess.visual_reason = Some("context_alignment_failed".to_owned());
                        continue;
                    }
                }
            }
        }

        let top_before = top_guess_text(guess).map(|value| value.to_owned());
        let mut row_scored = false;
        let rerank_gate = visual_rerank_gate(guess, overlays);
        *rerank_gate_counts
            .entry(rerank_gate.as_str())
            .or_insert(0_usize) += 1;
        if rerank_enabled && rerank_gate == VisualRerankGate::Eligible {
            rerank_rows_considered += 1;
            let rerank_eval_started = Instant::now();
            match score_top_k_candidates_for_row(RowCandidateScoringInput {
                pdf_bytes: &base_pdf_bytes_for_visual,
                page_index: redaction.page_index,
                base_page,
                page_box,
                redaction_bbox: redaction.bbox,
                template_overlays: overlays,
                guess,
                dpi: effective_dpi,
                min_ink_pixels: effective_min_ink_pixels,
            }) {
                Ok(mut candidate_scores) if !candidate_scores.is_empty() => {
                    rerank_eval_ms += rerank_eval_started.elapsed().as_millis();
                    rerank_rows_scored += 1;
                    rerank_candidate_evals += candidate_scores.len();
                    candidate_scores.sort_by(|left, right| {
                        right
                            .blended_score
                            .partial_cmp(&left.blended_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| {
                                left.score
                                    .mean_abs_diff
                                    .partial_cmp(&right.score.mean_abs_diff)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                    });
                    let mut chosen = candidate_scores.first().cloned();
                    let Some(mut chosen) = chosen.take() else {
                        guess.visual_reason = Some("visual_rerank_empty".to_owned());
                        continue;
                    };
                    if let Some(before) = top_before.as_deref() {
                        if !before.eq_ignore_ascii_case(&chosen.text)
                            && chosen.combined_gain < VISUAL_RERANK_MIN_GAIN_TO_REORDER
                        {
                            if let Some(original) = candidate_scores
                                .iter()
                                .find(|candidate| candidate.text.eq_ignore_ascii_case(before))
                                .cloned()
                            {
                                chosen = original;
                            }
                        }
                    }
                    rows_scored += 1;
                    row_scored = true;
                    guess.visual_compared_pixels = Some(chosen.score.compared_pixels);
                    guess.visual_mean_abs_diff = Some(chosen.score.mean_abs_diff);
                    guess.visual_changed_pixel_ratio = Some(chosen.score.changed_pixel_ratio);

                    if let Some(before) = top_before.as_deref() {
                        if !before.eq_ignore_ascii_case(&chosen.text) {
                            rerank_top1_changed += 1;
                            promote_guess_text_to_front(guess, &chosen.text);
                        }
                    }
                    rerank_gain_sum += chosen.combined_gain.max(0.0_f32) as f64;

                    if let Some(threshold) = cfg.drop_threshold {
                        if chosen.score.mean_abs_diff > threshold {
                            guess.candidates.clear();
                            guess.exact_matches.clear();
                            guess.visual_dropped = true;
                            rows_dropped += 1;
                        }
                    }
                }
                Ok(_) => {
                    rerank_eval_ms += rerank_eval_started.elapsed().as_millis();
                }
                Err(error) => {
                    rerank_eval_ms += rerank_eval_started.elapsed().as_millis();
                    guess.visual_reason = Some(format!("visual_rerank_failed:{error}"));
                }
            }
        }

        if row_scored {
            continue;
        }

        let score = score_row_overlay(
            base_page,
            overlay_page,
            page_box,
            window_bbox,
            redaction.bbox,
            effective_min_ink_pixels,
        );
        let Some(score) = score else {
            if guess.visual_reason.is_none() {
                guess.visual_reason = Some("insufficient_ink_pixels".to_owned());
            }
            continue;
        };

        rows_scored += 1;
        guess.visual_compared_pixels = Some(score.compared_pixels);
        guess.visual_mean_abs_diff = Some(score.mean_abs_diff);
        guess.visual_changed_pixel_ratio = Some(score.changed_pixel_ratio);

        if let Some(threshold) = cfg.drop_threshold {
            if score.mean_abs_diff > threshold {
                guess.candidates.clear();
                guess.exact_matches.clear();
                guess.visual_dropped = true;
                rows_dropped += 1;
            }
        }
    }

    diagnostics.push(format!(
        "visual_score=enabled rows_total={} rows_with_top_guess={} context_rows_scored={} context_rows_rejected={} rows_scored={} rows_dropped={} dpi_requested={} dpi_effective={} min_ink_pixels_requested={} min_ink_pixels_effective={} drop_threshold={} context_max_diff={}",
        max_items,
        rows_with_top_guess,
        context_rows_scored,
        context_rows_rejected,
        rows_scored,
        rows_dropped,
        cfg.dpi,
        effective_dpi,
        cfg.min_ink_pixels,
        effective_min_ink_pixels,
        cfg.drop_threshold
            .map(|value| format!("{value:.4}"))
            .unwrap_or_else(|| "none".to_owned()),
        CONTEXT_ALIGNMENT_MAX_DIFF
    ));
    if let Some(reason) = crop_fallback_reason {
        diagnostics.push(reason);
    }
    let rerank_changed_ratio = if rerank_rows_scored == 0 {
        0.0_f64
    } else {
        rerank_top1_changed as f64 / rerank_rows_scored as f64
    };
    let rerank_mean_gain = if rerank_rows_scored == 0 {
        0.0_f64
    } else {
        rerank_gain_sum / rerank_rows_scored as f64
    };
    diagnostics.push(format!(
        "visual_rerank=enabled={} rows_considered={} rows_scored={} top1_changed={} top1_changed_ratio={:.4} mean_gain={:.4} top_k={} eval_cap={} blend_weight={:.3}",
        rerank_enabled,
        rerank_rows_considered,
        rerank_rows_scored,
        rerank_top1_changed,
        rerank_changed_ratio,
        rerank_mean_gain,
        VISUAL_RERANK_TOP_K,
        VISUAL_RERANK_MAX_EVAL_CANDIDATES,
        VISUAL_RERANK_BLEND_WEIGHT
    ));
    let rerank_gate_summary = if rerank_gate_counts.is_empty() {
        "none".to_owned()
    } else {
        rerank_gate_counts
            .iter()
            .map(|(reason, count)| format!("{reason}={count}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    diagnostics.push(format!("visual_rerank_gates={rerank_gate_summary}"));
    let rerank_mean_eval_ms_per_candidate = if rerank_candidate_evals == 0 {
        0.0_f64
    } else {
        rerank_eval_ms as f64 / rerank_candidate_evals as f64
    };
    let rerank_mean_eval_ms_per_row = if rerank_rows_scored == 0 {
        0.0_f64
    } else {
        rerank_eval_ms as f64 / rerank_rows_scored as f64
    };
    diagnostics.push(format!(
        "visual_rerank_timing=candidate_evals={} eval_ms_total={} eval_ms_per_candidate={:.3} eval_ms_per_scored_row={:.3}",
        rerank_candidate_evals,
        rerank_eval_ms,
        rerank_mean_eval_ms_per_candidate,
        rerank_mean_eval_ms_per_row
    ));
    diagnostics.push(format!(
        "visual_score_timing=annotate_overlay_ms={} annotate_context_ms={} renderer_init_ms={} page_render_ms={} row_scoring_ms={} pages_rendered={}",
        annotate_overlay_ms,
        annotate_context_ms,
        renderer_init_ms,
        page_render_ms,
        row_scoring_started.elapsed().as_millis(),
        base_pages.len()
    ));
    diagnostics.push(format!(
        "visual_tile_rendering=pages_cropped={} crop_apply_ms={} tile_padding_pt={} max_crop_coverage={}",
        page_crop_boxes.len(),
        crop_apply_ms,
        VISUAL_TILE_PAGE_PADDING_PT,
        VISUAL_TILE_MAX_COVERAGE_FOR_CROP
    ));
    Ok(diagnostics)
}

fn group_overlays_by_redaction(overlays: &[TextOverlay]) -> BTreeMap<usize, Vec<TextOverlay>> {
    let mut by_index = BTreeMap::<usize, Vec<TextOverlay>>::new();
    for overlay in overlays {
        let Some(index) = overlay.redaction_index else {
            continue;
        };
        by_index.entry(index).or_default().push(overlay.clone());
    }
    by_index
}

fn build_context_overlays_by_redaction(
    overlays_by_redaction: &BTreeMap<usize, Vec<TextOverlay>>,
) -> BTreeMap<usize, Vec<TextOverlay>> {
    let mut out = BTreeMap::<usize, Vec<TextOverlay>>::new();
    for (index, overlays) in overlays_by_redaction {
        if overlays.len() < 3 {
            continue;
        }
        let Some(first) = overlays.first().cloned() else {
            continue;
        };
        let Some(last) = overlays.last().cloned() else {
            continue;
        };
        out.insert(*index, vec![first, last]);
    }
    out
}

fn top_guess_text(guess: &RedactionGuess) -> Option<&str> {
    guess
        .candidates
        .first()
        .map(|candidate| candidate.text.as_str())
}

fn promote_guess_text_to_front(guess: &mut RedactionGuess, selected_text: &str) {
    if let Some(pos) = guess
        .candidates
        .iter()
        .position(|candidate| candidate.text.eq_ignore_ascii_case(selected_text))
    {
        let chosen = guess.candidates.remove(pos);
        guess.candidates.insert(0, chosen);
    }
    if let Some(pos) = guess
        .exact_matches
        .iter()
        .position(|text| text.eq_ignore_ascii_case(selected_text))
    {
        let chosen = guess.exact_matches.remove(pos);
        guess.exact_matches.insert(0, chosen);
    }
}

fn ordered_candidate_texts_top_k(guess: &RedactionGuess, top_k: usize) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = BTreeSet::<String>::new();
    for candidate in &guess.candidates {
        let trimmed = candidate.text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_uppercase();
        if seen.insert(key) {
            out.push(trimmed.to_owned());
        }
        if out.len() >= top_k {
            return out;
        }
    }
    for text in &guess.exact_matches {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_uppercase();
        if seen.insert(key) {
            out.push(trimmed.to_owned());
        }
        if out.len() >= top_k {
            return out;
        }
    }
    out
}

fn rerank_candidate_texts(guess: &RedactionGuess) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = BTreeSet::<String>::new();

    for text in ordered_candidate_texts_top_k(guess, VISUAL_RERANK_TOP_K) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_uppercase();
        if seen.insert(key) {
            out.push(trimmed.to_owned());
        }
        if out.len() >= VISUAL_RERANK_MAX_EVAL_CANDIDATES {
            return out;
        }
    }
    let Some(top_text) = out.first().cloned() else {
        return out;
    };

    let top_geometric = geometric_score_for_text(guess, &top_text);
    let top_width = candidate_width_pt_for_text(guess, &top_text).max(0.1_f32);
    let mut expansion = Vec::<(f32, f32, String)>::new();

    for candidate in &guess.candidates {
        let trimmed = candidate.text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_uppercase();
        if seen.contains(&key) {
            continue;
        }
        let width = candidate
            .width_pt
            .filter(|value| value.is_finite() && *value > 0.0_f32)
            .unwrap_or_else(|| candidate_width_pt_for_text(guess, trimmed));
        let width_delta_ratio = ((width - top_width).abs() / top_width.max(1.0_f32)).max(0.0_f32);
        if width_delta_ratio > VISUAL_RERANK_MAX_WIDTH_DELTA_RATIO_FOR_EXPANSION {
            continue;
        }
        let score_gap = (top_geometric - candidate.score).max(0.0_f32);
        if score_gap > VISUAL_RERANK_MAX_SCORE_GAP_FOR_EXPANSION {
            continue;
        }
        expansion.push((width_delta_ratio, score_gap, trimmed.to_owned()));
    }
    expansion.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.1
                    .partial_cmp(&right.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.2.cmp(&right.2))
    });
    for (_, _, text) in expansion {
        let key = text.to_ascii_uppercase();
        if seen.insert(key) {
            out.push(text);
        }
        if out.len() >= VISUAL_RERANK_MAX_EVAL_CANDIDATES {
            break;
        }
    }
    out
}

fn visual_rerank_gate(guess: &RedactionGuess, overlays: &[TextOverlay]) -> VisualRerankGate {
    if overlays.len() < 3 {
        return VisualRerankGate::InsufficientOverlays;
    }
    if guess.candidates.len() < 2 {
        return VisualRerankGate::InsufficientCandidates;
    }
    let mut ordered = overlays.to_vec();
    ordered.sort_by(|left, right| {
        left.x
            .partial_cmp(&right.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let Some(left) = ordered.first() else {
        return VisualRerankGate::InsufficientOverlays;
    };
    let Some(right) = ordered.last() else {
        return VisualRerankGate::InsufficientOverlays;
    };
    if left.text.trim().is_empty() || right.text.trim().is_empty() {
        return VisualRerankGate::MissingBoundaryText;
    }

    let texts = rerank_candidate_texts(guess);
    if texts.len() < 2 {
        return VisualRerankGate::InsufficientCandidateTexts;
    }
    let top = geometric_score_for_text(guess, &texts[0]);
    let second = geometric_score_for_text(guess, &texts[1]);
    if !top.is_finite() || !second.is_finite() {
        return VisualRerankGate::NonFiniteGeometric;
    }
    if top > VISUAL_RERANK_MAX_TOP_SCORE {
        return VisualRerankGate::TopScoreTooHigh;
    }
    if (top - second).abs() <= VISUAL_RERANK_MAX_BASE_GAP {
        VisualRerankGate::Eligible
    } else {
        VisualRerankGate::BaseGapTooLarge
    }
}

fn visual_quality_from_score(score: &RowPixelScore) -> f32 {
    let diff_quality = (1.0_f32 - score.mean_abs_diff / 0.30_f32).clamp(0.0_f32, 1.0_f32);
    let changed_quality = (1.0_f32 - score.changed_pixel_ratio / 0.45_f32).clamp(0.0_f32, 1.0_f32);
    let edge_overlap_quality = if score.edge_ink_overlap_ratio.is_finite() {
        score.edge_ink_overlap_ratio.clamp(0.0_f32, 1.0_f32)
    } else {
        0.5_f32
    };
    let edge_clean_quality = if score.edge_ink_mismatch_ratio.is_finite() {
        (1.0_f32 - score.edge_ink_mismatch_ratio).clamp(0.0_f32, 1.0_f32)
    } else {
        0.5_f32
    };
    (diff_quality * 0.45_f32
        + changed_quality * 0.20_f32
        + edge_overlap_quality * 0.25_f32
        + edge_clean_quality * 0.10_f32)
        .clamp(0.0_f32, 1.0_f32)
}

fn geometric_score_for_text(guess: &RedactionGuess, text: &str) -> f32 {
    if let Some(candidate) = guess
        .candidates
        .iter()
        .find(|candidate| candidate.text.eq_ignore_ascii_case(text))
    {
        return candidate.score;
    }
    0.0_f32
}

fn candidate_width_pt_for_text(guess: &RedactionGuess, text: &str) -> f32 {
    if let Some(width) = guess
        .candidates
        .iter()
        .find(|candidate| candidate.text.eq_ignore_ascii_case(text))
        .and_then(|candidate| candidate.width_pt)
        .filter(|value| value.is_finite() && *value > 0.0_f32)
    {
        return width;
    }
    let char_width = guess.context.char_width_pt.max(0.1_f32);
    approximate_candidate_width_pt(text, char_width)
}

fn approximate_candidate_width_pt(text: &str, char_width_pt: f32) -> f32 {
    let glyph_count = text
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != ',')
        .count()
        .max(1) as f32;
    let spaces = text.chars().filter(|ch| ch.is_whitespace()).count() as f32;
    (glyph_count * char_width_pt + spaces * char_width_pt * 0.45_f32).max(0.1_f32)
}

fn build_candidate_overlays_from_template(
    template_overlays: &[TextOverlay],
    candidate_text: &str,
    candidate_width_pt: f32,
    top_width_pt: f32,
) -> Option<Vec<TextOverlay>> {
    if template_overlays.len() < 3 {
        return None;
    }
    let mut ordered = template_overlays.to_vec();
    ordered.sort_by(|left, right| {
        left.x
            .partial_cmp(&right.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut left = ordered.first()?.clone();
    let mut guess = ordered.get(1)?.clone();
    let mut right = ordered.get(2)?.clone();
    let template_bbox = union_overlay_bbox(&ordered)?;

    let top_width = top_width_pt.max(0.1_f32);
    let right_space = (right.x - (guess.x + top_width)).max(0.0_f32);
    let new_right_x = guess.x + candidate_width_pt.max(0.1_f32) + right_space;

    guess.text = candidate_text.to_owned();
    right.x = new_right_x;

    left.bbox = template_bbox;
    guess.bbox = template_bbox;
    right.bbox = template_bbox;
    Some(vec![left, guess, right])
}

fn score_top_k_candidates_for_row(
    input: RowCandidateScoringInput<'_>,
) -> Result<Vec<CandidateVisualScore>, String> {
    let RowCandidateScoringInput {
        pdf_bytes,
        page_index,
        base_page,
        page_box,
        redaction_bbox,
        template_overlays,
        guess,
        dpi,
        min_ink_pixels,
    } = input;
    let texts = rerank_candidate_texts(guess);
    if texts.len() < 2 {
        return Ok(Vec::new());
    }
    let Some(template_bbox) = union_overlay_bbox(template_overlays) else {
        return Ok(Vec::new());
    };
    let top_text = texts.first().cloned().unwrap_or_default();
    let top_width = candidate_width_pt_for_text(guess, &top_text);
    let max_positive_shift_pt = texts
        .iter()
        .map(|text| (candidate_width_pt_for_text(guess, text) - top_width).max(0.0_f32))
        .fold(0.0_f32, f32::max);
    let fixed_window_bbox = pad_rect(
        Rect::new(
            template_bbox.x0,
            template_bbox.y0,
            template_bbox.x1 + max_positive_shift_pt,
            template_bbox.y1,
        ),
        page_box,
    );
    let top_geometric = geometric_score_for_text(guess, &top_text);
    let min_shift_pt = ((72.0_f32 / dpi.max(1.0_f32)) * VISUAL_RERANK_MIN_SHIFT_PX).max(0.25_f32);
    let mut out = Vec::<CandidateVisualScore>::new();
    for text in texts {
        let candidate_width = candidate_width_pt_for_text(guess, &text);
        let geometric = geometric_score_for_text(guess, &text);
        if !text.eq_ignore_ascii_case(&top_text) {
            let geometric_gap = (top_geometric - geometric).max(0.0_f32);
            if geometric_gap > VISUAL_RERANK_MAX_GEOMETRIC_GAP_FOR_EVAL {
                continue;
            }
            if (candidate_width - top_width).abs() < min_shift_pt {
                continue;
            }
        }
        let Some(overlays) = build_candidate_overlays_from_template(
            template_overlays,
            &text,
            candidate_width,
            top_width,
        ) else {
            continue;
        };
        let annotated = annotate_overlays(
            pdf_bytes,
            &overlays,
            OVERLAY_TEXT_COLOR,
            OVERLAY_BORDER_WIDTH,
        )?;
        let page_indices = BTreeSet::from([page_index]);
        let overlaid_pages = render_pages_to_rgba(&annotated, &page_indices, dpi)?;
        let Some(overlaid_page) = overlaid_pages.get(&page_index) else {
            continue;
        };
        let Some(score) = score_row_overlay(
            base_page,
            overlaid_page,
            page_box,
            fixed_window_bbox,
            redaction_bbox,
            min_ink_pixels,
        ) else {
            continue;
        };
        let visual = visual_quality_from_score(&score);
        let blended = geometric * (1.0_f32 - VISUAL_RERANK_BLEND_WEIGHT)
            + visual * VISUAL_RERANK_BLEND_WEIGHT;
        out.push(CandidateVisualScore {
            text,
            score,
            blended_score: blended,
            combined_gain: 0.0_f32,
        });
    }
    if out.is_empty() {
        return Ok(out);
    }
    let top_blended = out
        .iter()
        .find(|candidate| candidate.text.eq_ignore_ascii_case(&top_text))
        .map(|candidate| candidate.blended_score)
        .unwrap_or_else(|| out[0].blended_score);
    for candidate in &mut out {
        candidate.combined_gain = candidate.blended_score - top_blended;
    }
    Ok(out)
}

fn union_overlay_bbox(overlays: &[TextOverlay]) -> Option<Rect> {
    let first = overlays.first()?;
    let mut x0 = first.bbox.x0;
    let mut y0 = first.bbox.y0;
    let mut x1 = first.bbox.x1;
    let mut y1 = first.bbox.y1;
    for overlay in overlays.iter().skip(1) {
        x0 = x0.min(overlay.bbox.x0);
        y0 = y0.min(overlay.bbox.y0);
        x1 = x1.max(overlay.bbox.x1);
        y1 = y1.max(overlay.bbox.y1);
    }
    Some(Rect::new(x0, y0, x1, y1))
}

fn pad_rect(rect: Rect, page_box: Rect) -> Rect {
    Rect::new(
        (rect.x0 - WINDOW_PADDING_PT).max(page_box.x0),
        (rect.y0 - WINDOW_PADDING_PT).max(page_box.y0),
        (rect.x1 + WINDOW_PADDING_PT).min(page_box.x1),
        (rect.y1 + WINDOW_PADDING_PT).min(page_box.y1),
    )
}

fn pad_rect_with_padding(rect: Rect, page_box: Rect, padding_pt: f32) -> Rect {
    Rect::new(
        (rect.x0 - padding_pt).max(page_box.x0),
        (rect.y0 - padding_pt).max(page_box.y0),
        (rect.x1 + padding_pt).min(page_box.x1),
        (rect.y1 + padding_pt).min(page_box.y1),
    )
}

fn merge_rect(left: Rect, right: Rect) -> Rect {
    Rect::new(
        left.x0.min(right.x0),
        left.y0.min(right.y0),
        left.x1.max(right.x1),
        left.y1.max(right.y1),
    )
}

fn build_visual_page_crop_boxes(
    overlays_by_redaction: &BTreeMap<usize, Vec<TextOverlay>>,
    page_boxes: &BTreeMap<u32, Rect>,
) -> BTreeMap<u32, Rect> {
    let mut out = BTreeMap::<u32, Rect>::new();
    for overlays in overlays_by_redaction.values() {
        let Some(first) = overlays.first() else {
            continue;
        };
        let page_index = first.page_index;
        let Some(page_box) = page_boxes.get(&page_index).copied() else {
            continue;
        };
        let Some(row_bbox) = union_overlay_bbox(overlays) else {
            continue;
        };
        let padded = pad_rect_with_padding(row_bbox, page_box, VISUAL_TILE_PAGE_PADDING_PT);
        out.entry(page_index)
            .and_modify(|existing| *existing = merge_rect(*existing, padded))
            .or_insert(padded);
    }
    for (page_index, crop_box) in out.iter_mut() {
        if let Some(page_box) = page_boxes.get(page_index).copied() {
            let coverage = crop_box.area() / page_box.area().max(0.0001_f32);
            if coverage >= VISUAL_TILE_MAX_COVERAGE_FOR_CROP {
                *crop_box = page_box;
            }
        }
    }
    out
}

fn score_row_overlay(
    base: &RenderedPage,
    overlaid: &RenderedPage,
    page_box: Rect,
    window_bbox: Rect,
    redaction_bbox: Rect,
    min_ink_pixels: u32,
) -> Option<RowPixelScore> {
    if base.width_px != overlaid.width_px || base.height_px != overlaid.height_px {
        return None;
    }
    if base.pixels.len() != overlaid.pixels.len() || base.pixels.is_empty() {
        return None;
    }

    let window = rect_pdf_to_pixels(
        &window_bbox,
        page_box,
        base.dpi,
        base.width_px,
        base.height_px,
    )?;
    let redaction = rect_pdf_to_pixels(
        &redaction_bbox,
        page_box,
        base.dpi,
        base.width_px,
        base.height_px,
    );

    let width = base.width_px as usize;
    let mut compared_pixels = 0_u32;
    let mut changed_weight = 0.0_f32;
    let mut compared_weight = 0.0_f32;
    let mut diff_sum = 0.0_f32;
    let mut edge_ink_weight = 0.0_f32;
    let mut edge_ink_overlap_weight = 0.0_f32;
    let mut edge_ink_mismatch_weight = 0.0_f32;
    let edge_band_px = ((EDGE_BAND_PT / 72.0_f32) * base.dpi).ceil().max(0.0_f32) as u32;

    for y in window.1..window.3 {
        for x in window.0..window.2 {
            let mut inside_redaction_edge_band = false;
            let mut outside_redaction_edge_band = false;
            if let Some(red_box) = redaction {
                if point_in_rect_px(x, y, red_box) {
                    if !point_in_inner_edge_band_px(x, y, red_box, edge_band_px) {
                        continue;
                    }
                    inside_redaction_edge_band = true;
                } else if point_in_edge_band_px(x, y, red_box, edge_band_px) {
                    outside_redaction_edge_band = true;
                }
            }
            let index = ((y as usize * width) + x as usize) * 4;
            if index + 2 >= base.pixels.len() {
                continue;
            }
            let base_luma = luma_u8(&base.pixels[index..index + 4]);
            let over_luma = luma_u8(&overlaid.pixels[index..index + 4]);
            if base_luma >= BACKGROUND_LUMA_THRESHOLD && over_luma >= BACKGROUND_LUMA_THRESHOLD {
                continue;
            }
            if inside_redaction_edge_band
                && base_luma <= REDACTION_INTERIOR_IGNORE_DARK_LUMA
                && over_luma <= REDACTION_INTERIOR_IGNORE_DARK_LUMA
            {
                continue;
            }

            compared_pixels = compared_pixels.saturating_add(1);
            let edge_weight = if inside_redaction_edge_band {
                EDGE_INTERIOR_WEIGHT
            } else if outside_redaction_edge_band {
                EDGE_BAND_WEIGHT
            } else {
                1.0_f32
            };
            if (inside_redaction_edge_band || outside_redaction_edge_band)
                && over_luma <= EDGE_INK_LUMA_THRESHOLD
            {
                edge_ink_weight += edge_weight;
                if base_luma <= EDGE_INK_MATCH_BASE_LUMA_THRESHOLD {
                    edge_ink_overlap_weight += edge_weight;
                } else if base_luma >= EDGE_INK_MISMATCH_BASE_LUMA_THRESHOLD {
                    edge_ink_mismatch_weight += edge_weight;
                }
            }
            compared_weight += edge_weight;
            let delta = base_luma.abs_diff(over_luma);
            diff_sum += (delta as f32 / 255.0_f32) * edge_weight;
            if delta >= CHANGED_LUMA_DELTA {
                changed_weight += edge_weight;
            }
        }
    }

    if compared_pixels < min_ink_pixels {
        return None;
    }
    let denom = compared_weight.max(0.0001_f32);
    let (edge_ink_overlap_ratio, edge_ink_mismatch_ratio) = if edge_ink_weight <= 0.0_f32 {
        (0.5_f32, 0.5_f32)
    } else {
        let edge_denom = edge_ink_weight.max(0.0001_f32);
        (
            edge_ink_overlap_weight / edge_denom,
            edge_ink_mismatch_weight / edge_denom,
        )
    };
    Some(RowPixelScore {
        compared_pixels,
        mean_abs_diff: diff_sum / denom,
        changed_pixel_ratio: changed_weight / denom,
        edge_ink_overlap_ratio,
        edge_ink_mismatch_ratio,
    })
}

fn luma_u8(rgba: &[u8]) -> u8 {
    if rgba.len() < 3 {
        return 255;
    }
    let r = rgba[0] as f32;
    let g = rgba[1] as f32;
    let b = rgba[2] as f32;
    (0.299_f32 * r + 0.587_f32 * g + 0.114_f32 * b)
        .round()
        .clamp(0.0_f32, 255.0_f32) as u8
}

fn point_in_rect_px(x: u32, y: u32, rect: (u32, u32, u32, u32)) -> bool {
    x >= rect.0 && x < rect.2 && y >= rect.1 && y < rect.3
}

fn point_in_edge_band_px(x: u32, y: u32, rect: (u32, u32, u32, u32), band_px: u32) -> bool {
    if band_px == 0 {
        return false;
    }
    let y_min = rect.1.saturating_sub(band_px);
    let y_max = rect.3.saturating_add(band_px);
    if y < y_min || y >= y_max {
        return false;
    }
    let left_min = rect.0.saturating_sub(band_px);
    let left_band = x >= left_min && x < rect.0;
    let right_max = rect.2.saturating_add(band_px);
    let right_band = x >= rect.2 && x < right_max;
    left_band || right_band
}

fn point_in_inner_edge_band_px(x: u32, y: u32, rect: (u32, u32, u32, u32), band_px: u32) -> bool {
    if band_px == 0 {
        return false;
    }
    if !point_in_rect_px(x, y, rect) {
        return false;
    }
    let near_left = x < rect.0.saturating_add(band_px);
    let near_right = x >= rect.2.saturating_sub(band_px);
    let near_top = y < rect.1.saturating_add(band_px);
    let near_bottom = y >= rect.3.saturating_sub(band_px);
    near_left || near_right || near_top || near_bottom
}

fn rect_pdf_to_pixels(
    rect: &Rect,
    page_box: Rect,
    dpi: f32,
    width_px: u32,
    height_px: u32,
) -> Option<(u32, u32, u32, u32)> {
    if dpi <= 0.0_f32 || width_px == 0 || height_px == 0 {
        return None;
    }

    let x0 = (((rect.x0 - page_box.x0) / 72.0_f32) * dpi).floor();
    let x1 = (((rect.x1 - page_box.x0) / 72.0_f32) * dpi).ceil();
    let y0 = (((page_box.y1 - rect.y1) / 72.0_f32) * dpi).floor();
    let y1 = (((page_box.y1 - rect.y0) / 72.0_f32) * dpi).ceil();

    let x0_px = x0.clamp(0.0_f32, width_px as f32) as u32;
    let x1_px = x1.clamp(0.0_f32, width_px as f32) as u32;
    let y0_px = y0.clamp(0.0_f32, height_px as f32) as u32;
    let y1_px = y1.clamp(0.0_f32, height_px as f32) as u32;

    if x1_px <= x0_px || y1_px <= y0_px {
        return None;
    }
    Some((x0_px, y0_px, x1_px, y1_px))
}

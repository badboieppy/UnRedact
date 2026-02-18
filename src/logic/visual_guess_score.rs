use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use lopdf::{Document, Object, ObjectId};

use crate::data::visualization_data::{VisualizationData, VisualizationDataSource as _};
use crate::dependency::hayro_renderer::HayroRenderer;
use crate::dependency::pdf_annotator::PdfAnnotator;
use crate::types::file_types::FontRunReport;
use crate::types::guess_types::{GuessReport, RedactionGuess};
use crate::types::redaction_types::{PdfRenderer as _, Rect, RedactionReport, RenderedPage};
use crate::types::text_overlay::TextOverlay;

const BACKGROUND_LUMA_THRESHOLD: u8 = 245_u8;
const CHANGED_LUMA_DELTA: u8 = 24_u8;
const WINDOW_PADDING_PT: f32 = 1.0_f32;
const OVERLAY_TEXT_COLOR: [f32; 3] = [0.0_f32, 0.0_f32, 0.0_f32];
const OVERLAY_BORDER_WIDTH: f32 = 1.0_f32;
const CONTEXT_ALIGNMENT_MAX_DIFF: f32 = 0.22_f32;

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
}

#[inline]
pub fn apply_visual_scores(
    pdf_path: &Path,
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
        diagnostics: Vec::new(),
    };
    let inputs =
        visualization.load_inputs(pdf_path, redactions, Some(&guess_report), Some(font_runs))?;
    let overlays_by_redaction = group_overlays_by_redaction(&inputs.overlays);
    let page_boxes = build_page_boxes(&inputs.pdf_bytes)?;

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

    let annotator = PdfAnnotator;
    let context_overlays_by_redaction = build_context_overlays_by_redaction(&overlays_by_redaction);
    let context_overlays = context_overlays_by_redaction
        .values()
        .flat_map(|items| items.iter().cloned())
        .collect::<Vec<_>>();
    let annotated_bytes = annotator.annotate(
        &inputs.pdf_bytes,
        &[],
        &inputs.overlays,
        OVERLAY_TEXT_COLOR,
        OVERLAY_TEXT_COLOR,
        OVERLAY_BORDER_WIDTH,
    )?;
    let context_annotated_bytes = if context_overlays.is_empty() {
        None
    } else {
        Some(annotator.annotate(
            &inputs.pdf_bytes,
            &[],
            &context_overlays,
            OVERLAY_TEXT_COLOR,
            OVERLAY_TEXT_COLOR,
            OVERLAY_BORDER_WIDTH,
        )?)
    };

    let base_renderer = HayroRenderer::new_from_bytes(&inputs.pdf_bytes)?;
    let overlay_renderer = HayroRenderer::new_from_bytes(&annotated_bytes)?;
    let context_renderer = match context_annotated_bytes.as_deref() {
        Some(bytes) => Some(HayroRenderer::new_from_bytes(bytes)?),
        None => None,
    };
    let mut pages_to_render = BTreeSet::<u32>::new();
    for overlays in overlays_by_redaction.values() {
        if let Some(first) = overlays.first() {
            pages_to_render.insert(first.page_index);
        }
    }
    let mut base_pages = BTreeMap::<u32, RenderedPage>::new();
    let mut overlay_pages = BTreeMap::<u32, RenderedPage>::new();
    let mut context_pages = BTreeMap::<u32, RenderedPage>::new();
    for page_index in pages_to_render {
        let base = base_renderer.render_page_to_rgba(page_index as usize, cfg.dpi)?;
        let overlay = overlay_renderer.render_page_to_rgba(page_index as usize, cfg.dpi)?;
        base_pages.insert(page_index, base);
        overlay_pages.insert(page_index, overlay);
        if let Some(renderer) = &context_renderer {
            let context = renderer.render_page_to_rgba(page_index as usize, cfg.dpi)?;
            context_pages.insert(page_index, context);
        }
    }

    let mut rows_with_top_guess = 0_usize;
    let mut context_rows_scored = 0_usize;
    let mut context_rows_rejected = 0_usize;
    let mut rows_scored = 0_usize;
    let mut rows_dropped = 0_usize;
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

        let Some(page_box) = page_boxes.get(&redaction.page_index).copied() else {
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
            if let Some(context_page) = context_pages.get(&redaction.page_index) {
                if let Some(context_window_bbox) =
                    union_overlay_bbox(context_overlays).map(|bbox| pad_rect(bbox, page_box))
                {
                    if let Some(context_score) = score_row_overlay(
                        base_page,
                        context_page,
                        page_box,
                        context_window_bbox,
                        redaction.bbox,
                        cfg.min_ink_pixels,
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
        }

        let score = score_row_overlay(
            base_page,
            overlay_page,
            page_box,
            window_bbox,
            redaction.bbox,
            cfg.min_ink_pixels,
        );
        let Some(score) = score else {
            guess.visual_reason = Some("insufficient_ink_pixels".to_owned());
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
        "visual_score=enabled rows_total={} rows_with_top_guess={} context_rows_scored={} context_rows_rejected={} rows_scored={} rows_dropped={} dpi={} min_ink_pixels={} drop_threshold={} context_max_diff={}",
        max_items,
        rows_with_top_guess,
        context_rows_scored,
        context_rows_rejected,
        rows_scored,
        rows_dropped,
        cfg.dpi,
        cfg.min_ink_pixels,
        cfg.drop_threshold
            .map(|value| format!("{value:.4}"))
            .unwrap_or_else(|| "none".to_owned()),
        CONTEXT_ALIGNMENT_MAX_DIFF
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
    if let Some(exact) = guess.exact_matches.first() {
        return Some(exact.as_str());
    }
    guess
        .candidates
        .first()
        .map(|candidate| candidate.text.as_str())
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
    let mut changed_pixels = 0_u32;
    let mut diff_sum = 0.0_f32;

    for y in window.1..window.3 {
        for x in window.0..window.2 {
            if let Some(red_box) = redaction {
                if point_in_rect_px(x, y, red_box) {
                    continue;
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

            compared_pixels = compared_pixels.saturating_add(1);
            let delta = base_luma.abs_diff(over_luma);
            diff_sum += delta as f32 / 255.0_f32;
            if delta >= CHANGED_LUMA_DELTA {
                changed_pixels = changed_pixels.saturating_add(1);
            }
        }
    }

    if compared_pixels < min_ink_pixels {
        return None;
    }
    let denom = compared_pixels as f32;
    Some(RowPixelScore {
        compared_pixels,
        mean_abs_diff: diff_sum / denom,
        changed_pixel_ratio: changed_pixels as f32 / denom,
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

fn build_page_boxes(pdf_bytes: &[u8]) -> Result<BTreeMap<u32, Rect>, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|error| error.to_string())?;
    let mut boxes = BTreeMap::<u32, Rect>::new();
    for (page_no, page_id) in doc.get_pages() {
        let page_index = page_no.saturating_sub(1);
        let page_box = page_render_box_from_page(&doc, page_id)
            .unwrap_or(Rect::new(0.0_f32, 0.0_f32, 612.0_f32, 792.0_f32));
        boxes.insert(page_index, page_box);
    }
    Ok(boxes)
}

fn page_render_box_from_page(doc: &Document, page_id: ObjectId) -> Option<Rect> {
    inherited_page_rect(doc, page_id, b"CropBox")
        .or_else(|| inherited_page_rect(doc, page_id, b"MediaBox"))
}

fn inherited_page_rect(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Rect> {
    let mut current_id = page_id;
    let mut depth = 0_usize;
    loop {
        if depth > 32 {
            return None;
        }
        depth += 1;
        let object = doc.get_object(current_id).ok()?;
        let dict = match object {
            Object::Dictionary(value) => value,
            _ => return None,
        };

        if let Ok(value) = dict.get(key) {
            if let Some(rect) = object_to_rect_resolved(doc, value) {
                return Some(rect);
            }
        }

        let parent = match dict.get(b"Parent").ok()? {
            Object::Reference(parent_id) => *parent_id,
            _ => return None,
        };
        current_id = parent;
    }
}

fn object_to_rect_resolved(doc: &Document, object: &Object) -> Option<Rect> {
    match object {
        Object::Reference(object_id) => doc.get_object(*object_id).ok().and_then(object_to_rect),
        _ => object_to_rect(object),
    }
}

fn object_to_rect(object: &Object) -> Option<Rect> {
    let values = match object {
        Object::Array(items) => items,
        _ => return None,
    };
    if values.len() < 4 {
        return None;
    }
    let x0 = object_to_f32(values.first()?)?;
    let y0 = object_to_f32(values.get(1)?)?;
    let x1 = object_to_f32(values.get(2)?)?;
    let y1 = object_to_f32(values.get(3)?)?;
    Some(Rect::new(x0, y0, x1, y1))
}

fn object_to_f32(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_row_overlay_detects_difference() {
        let base = RenderedPage {
            width_px: 4,
            height_px: 4,
            dpi: 200.0_f32,
            pixels: vec![255_u8; 4 * 4 * 4],
        };
        let mut overlaid = base.clone();
        for i in 0..4_usize {
            let idx = (4_usize + i) * 4_usize;
            overlaid.pixels[idx] = 0_u8;
            overlaid.pixels[idx + 1] = 0_u8;
            overlaid.pixels[idx + 2] = 0_u8;
        }

        let page_box = Rect::new(0.0_f32, 0.0_f32, 72.0_f32, 72.0_f32);
        let window = Rect::new(0.0_f32, 0.0_f32, 72.0_f32, 72.0_f32);
        let redaction = Rect::new(1000.0_f32, 1000.0_f32, 1001.0_f32, 1001.0_f32);
        let score = score_row_overlay(&base, &overlaid, page_box, window, redaction, 1_u32)
            .expect("score should be present");
        assert!(score.compared_pixels > 0);
        assert!(score.mean_abs_diff > 0.0_f32);
        assert!(score.changed_pixel_ratio > 0.0_f32);
    }
}

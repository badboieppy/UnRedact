use lopdf::{Dictionary, Document, Object};
use serde::Deserialize;

use crate::data::redaction_scan_data::CONTEXT_SPANS_META_KEY;
use crate::dependency::pdf_annotator::PdfAnnotator;
use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect as FontRect};
use crate::types::guess_types::{AnchorDecisionRecord, GuessReport, RedactionGuess};
use crate::types::redaction_types::{Rect, RedactionKind, RedactionReport};
use crate::types::runtime_defaults::GLYPH_UNITS_SCALE;
use crate::types::text_overlay::TextOverlay;
use crate::types::typography_shaping::shaping_features;
use crate::types::visualizer_config::VisualizerConfig;

const RASTER_TEXT_PADDING_PT: f32 = 1.0_f32;
const RASTER_MAX_FONT_TO_BOX_HEIGHT: f32 = 0.72_f32;
const RASTER_MIN_FONT_SIZE_PT: f32 = 4.5_f32;
const RASTER_BASELINE_ASCENT_RATIO: f32 = 0.70_f32;
const OVERLAY_MULTILINE_LEADING_RATIO: f32 = 1.15_f32;
const OVERLAY_CONTEXT_LINE_BUCKET_PT: f32 = 2.0_f32;
const OVERLAY_ANCHOR_LINE_TOLERANCE_PT: f32 = 8.0_f32;
const OVERLAY_ANCHOR_MAX_GAP_PT: f32 = 260.0_f32;
const OVERLAY_ANCHOR_EDGE_WINDOW_PT: f32 = 24.0_f32;
const OVERLAY_STYLE_FONT_SIZE_TOLERANCE_PT: f32 = 0.5_f32;
const OVERLAY_STYLE_H_SCALE_TOLERANCE_PCT: f32 = 5.0_f32;

#[derive(Debug, Clone)]
pub struct VisualizationInputs {
    pub pdf_bytes: Vec<u8>,
    pub rects: Vec<(u32, Rect)>,
    pub overlays: Vec<TextOverlay>,
}

#[derive(Debug, Clone, Copy)]
pub struct VisualizationData;

#[derive(Debug, Clone, Copy)]
struct RasterOverlayLayout {
    x: f32,
    y: f32,
    font_size_pt: f32,
}

struct AnchorPairOverlayInput<'a> {
    redaction_index: usize,
    redaction: &'a crate::types::redaction_types::RedactionOccurrence,
    guess: &'a RedactionGuess,
    anchor: Option<&'a AnchorDecisionRecord>,
    selected_text: &'a str,
    left_bbox: Option<Rect>,
    right_bbox: Option<Rect>,
    runs: &'a [FontTextRun],
    assets: &'a std::collections::BTreeMap<String, FontAsset>,
    width_map: &'a std::collections::BTreeMap<FontWidthKey, FontWidthTable>,
    force_selected_only: bool,
}

struct AnchorSelectedOverlayInput<'a> {
    redaction_index: usize,
    redaction: &'a crate::types::redaction_types::RedactionOccurrence,
    selected_text: &'a str,
    font_key: &'a str,
    font_size_pt: f32,
    h_scale_pct: f32,
    y0: f32,
    y1: f32,
    anchor_mode: Option<&'a str>,
    anchor_left_x: Option<f32>,
    anchor_right_x: Option<f32>,
    left_anchor_text: &'a str,
    right_anchor_text: &'a str,
    prefer_anchor_placement: bool,
    assets: &'a std::collections::BTreeMap<String, crate::types::file_types::FontAsset>,
    width_map: &'a std::collections::BTreeMap<FontWidthKey, FontWidthTable>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnchorRenderDirection {
    Prefix,
    Suffix,
}

#[derive(Debug, Clone)]
struct OverlayAnchorCandidate {
    text: String,
    bbox: Rect,
    line_bucket: i32,
    role_hint: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OverlayContextSpanRecord {
    text: String,
    bbox: Rect,
    #[serde(default)]
    line_bucket: i32,
    #[serde(default)]
    role_hint: String,
}

impl VisualizationData {
    #[inline]
    pub fn new() -> Self {
        Self
    }

    #[inline]
    pub fn load_inputs_from_bytes(
        &self,
        pdf_bytes: &[u8],
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
    ) -> Result<VisualizationInputs, String> {
        let width_map = build_font_width_map(pdf_bytes)?;
        let mut rects = Vec::with_capacity(report.redactions.len());
        for redaction in &report.redactions {
            rects.push((redaction.page_index, redaction.bbox));
        }
        let overlays = build_overlays(report, guesses, font_runs, &width_map);
        Ok(VisualizationInputs {
            pdf_bytes: pdf_bytes.to_vec(),
            rects,
            overlays,
        })
    }

    #[inline]
    pub fn render_visualized_pdf_from_bytes(
        &self,
        pdf_bytes: &[u8],
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
        cfg: VisualizerConfig,
    ) -> Result<Vec<u8>, String> {
        let inputs = self.load_inputs_from_bytes(pdf_bytes, report, guesses, font_runs)?;
        let annotator = PdfAnnotator;
        annotator.annotate(
            &inputs.pdf_bytes,
            &inputs.rects,
            &inputs.overlays,
            cfg.color,
            cfg.text_color,
            cfg.border_width,
        )
    }
}

impl Default for VisualizationData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

fn build_overlays(
    report: &RedactionReport,
    guesses: Option<&GuessReport>,
    font_runs: Option<&FontRunReport>,
    width_map: &std::collections::BTreeMap<FontWidthKey, FontWidthTable>,
) -> Vec<TextOverlay> {
    let mut out = Vec::new();
    let guesses = match guesses {
        Some(g) => g,
        None => return out,
    };
    let font_runs = match font_runs {
        Some(runs) => runs,
        None => return out,
    };

    let max = report.redactions.len().min(guesses.guesses.len());
    let assets = font_runs
        .assets
        .iter()
        .map(|a| (a.font_key.clone(), a.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let dense_anchor_rows = build_dense_anchor_row_flags(report);
    let dense_row_max_heights = build_dense_row_max_heights(report);
    for idx in 0..max {
        let redaction = &report.redactions[idx];
        let guess = &guesses.guesses[idx];
        let anchor = guesses.anchors.get(idx);
        let selected = pick_best_guess(guess);
        let selected = match selected {
            Some(text) => text,
            None => continue,
        };
        let selected_text = normalize_overlay_line_breaks(selected.trim());
        if selected_text.is_empty() {
            continue;
        }
        let left_hit = redaction.underlying_text.first();
        let right_hit = redaction.underlying_text.get(1);
        let context_left = anchor_left_text(anchor);
        let context_right = anchor_right_text(anchor);
        let anchor_left_x = anchor.and_then(|value| value.usable_left_edge_x_pt);
        let anchor_right_x = anchor.and_then(|value| value.usable_right_edge_x_pt);

        let has_any_anchor_hint = !context_left.is_empty()
            || !context_right.is_empty()
            || anchor_left_x.is_some()
            || anchor_right_x.is_some();
        if guess.context.anchor_mode.is_some()
            && has_any_anchor_hint
            && push_anchor_pair_overlays(
                &mut out,
                AnchorPairOverlayInput {
                    redaction_index: idx,
                    redaction,
                    guess,
                    anchor,
                    selected_text: selected_text.as_str(),
                    left_bbox: left_hit.map(|hit| hit.bbox),
                    right_bbox: right_hit.map(|hit| hit.bbox),
                    runs: &font_runs.runs,
                    assets: &assets,
                    width_map,
                    force_selected_only: dense_anchor_rows.get(idx).copied().unwrap_or(false),
                },
            )
        {
            continue;
        }

        if matches!(redaction.kind, RedactionKind::RasterDarkRegion) {
            let dense_row = dense_anchor_rows.get(idx).copied().unwrap_or(false);
            let nearby_run =
                select_run_by_bbox(&font_runs.runs, redaction.page_index, Some(redaction.bbox));
            let (font_key, requested_font_size_pt, h_scale_pct) = if let Some(run) = nearby_run {
                (
                    run.font_key.clone(),
                    guess.context.font_size_pt.unwrap_or(run.font_size_pt),
                    guess.context.h_scale_pct.unwrap_or(run.h_scale_pct),
                )
            } else {
                ("F1".to_owned(), 11.0_f32, 100.0_f32)
            };
            let box_height = redaction.bbox.height().abs().max(1.0_f32);
            let effective_box_height = if dense_row {
                dense_row_max_heights
                    .get(idx)
                    .copied()
                    .unwrap_or(box_height)
                    .max(box_height)
            } else {
                box_height
            };
            let fitted_font_size_pt = if dense_row {
                requested_font_size_pt.max(RASTER_MIN_FONT_SIZE_PT)
            } else {
                requested_font_size_pt
                    .min(effective_box_height * RASTER_MAX_FONT_TO_BOX_HEIGHT)
                    .max(RASTER_MIN_FONT_SIZE_PT)
            };
            let guess_line_count = overlay_line_count(&selected_text);
            let guess_width = multiline_text_width_pt(
                redaction.page_index,
                &font_key,
                fitted_font_size_pt,
                h_scale_pct,
                &selected_text,
                &assets,
                width_map,
            );
            let layout = raster_overlay_layout(
                redaction.bbox,
                fitted_font_size_pt,
                guess_width,
                guess_line_count,
            );
            let line_drop_pt = multiline_extra_line_drop_pt(layout.font_size_pt, guess_line_count);
            out.push(TextOverlay {
                redaction_index: Some(idx),
                page_index: redaction.page_index,
                text: selected_text.clone(),
                font_key,
                font_size_pt: layout.font_size_pt,
                h_scale_pct,
                x: layout.x,
                y: layout.y,
                bbox: Rect::new(
                    redaction.bbox.x0,
                    (redaction.bbox.y0 - line_drop_pt).min(redaction.bbox.y0),
                    redaction.bbox.x1,
                    redaction.bbox.y1,
                ),
            });
            continue;
        }

        if context_left.is_empty() || context_right.is_empty() {
            continue;
        }

        let left_text = left_hit.map(|h| h.text.trim());
        let right_text = right_hit.map(|h| h.text.trim());
        let left_text = match left_text {
            Some(text) if !text.is_empty() => text,
            _ => context_left,
        };
        let right_text = match right_text {
            Some(text) if !text.is_empty() => text,
            _ => context_right,
        };

        let left_bbox = left_hit.map(|h| h.bbox);
        let right_bbox = right_hit.map(|h| h.bbox);
        let left_run =
            select_run_by_text(&font_runs.runs, redaction.page_index, left_text, left_bbox)
                .or_else(|| select_run_by_bbox(&font_runs.runs, redaction.page_index, left_bbox));
        let right_run = select_run_by_text(
            &font_runs.runs,
            redaction.page_index,
            right_text,
            right_bbox,
        )
        .or_else(|| select_run_by_bbox(&font_runs.runs, redaction.page_index, right_bbox));
        let (left_run, right_run) = match (left_run, right_run) {
            (Some(l), Some(r)) => (l, r),
            _ => continue,
        };

        let left_font = &left_run.font_key;
        let right_font = &right_run.font_key;
        let left_size = left_run.font_size_pt;
        let right_size = right_run.font_size_pt;

        let left_width = text_width_pt(
            redaction.page_index,
            left_font,
            left_size,
            left_run.h_scale_pct,
            left_text,
            &assets,
            width_map,
        );
        let left_space = text_width_pt(
            redaction.page_index,
            left_font,
            left_size,
            left_run.h_scale_pct,
            " ",
            &assets,
            width_map,
        );
        let right_space = text_width_pt(
            redaction.page_index,
            right_font,
            right_size,
            right_run.h_scale_pct,
            " ",
            &assets,
            width_map,
        );

        let guess_font = if left_font == right_font && (left_size - right_size).abs() <= 0.01 {
            left_run
        } else {
            right_run
        };
        let guess_line_count = overlay_line_count(&selected_text);
        let guess_width = multiline_text_width_pt(
            redaction.page_index,
            &guess_font.font_key,
            guess_font.font_size_pt,
            guess_font.h_scale_pct,
            &selected_text,
            &assets,
            width_map,
        );

        let left_x = left_run.bbox.x0;
        let guess_x = left_x + left_width + left_space;
        let right_x = guess_x + guess_width + right_space;
        let guess_line_drop_pt =
            multiline_extra_line_drop_pt(guess_font.font_size_pt, guess_line_count);

        let overlay_bbox = Rect::new(
            left_x.min(redaction.bbox.x0),
            left_run
                .bbox
                .y0
                .min(right_run.bbox.y0)
                .min(redaction.bbox.y0),
            right_x.max(redaction.bbox.x1),
            left_run
                .bbox
                .y1
                .max(right_run.bbox.y1)
                .max(redaction.bbox.y1),
        );
        let overlay_bbox = Rect::new(
            overlay_bbox.x0,
            (overlay_bbox.y0 - guess_line_drop_pt).min(overlay_bbox.y0),
            right_x.max(redaction.bbox.x1),
            overlay_bbox.y1,
        );

        out.push(TextOverlay {
            redaction_index: Some(idx),
            page_index: redaction.page_index,
            text: left_text.to_owned(),
            font_key: left_run.font_key.clone(),
            font_size_pt: left_size,
            h_scale_pct: left_run.h_scale_pct,
            x: left_x,
            y: left_run.bbox.y1,
            bbox: overlay_bbox,
        });
        out.push(TextOverlay {
            redaction_index: Some(idx),
            page_index: redaction.page_index,
            text: selected_text.clone(),
            font_key: guess_font.font_key.clone(),
            font_size_pt: guess_font.font_size_pt,
            h_scale_pct: guess_font.h_scale_pct,
            x: guess_x,
            y: guess_font.bbox.y1,
            bbox: overlay_bbox,
        });
        out.push(TextOverlay {
            redaction_index: Some(idx),
            page_index: redaction.page_index,
            text: right_text.to_owned(),
            font_key: right_run.font_key.clone(),
            font_size_pt: right_size,
            h_scale_pct: right_run.h_scale_pct,
            x: right_x,
            y: right_run.bbox.y1,
            bbox: overlay_bbox,
        });
    }
    out
}

fn push_anchor_pair_overlays(
    overlays: &mut Vec<TextOverlay>,
    input: AnchorPairOverlayInput<'_>,
) -> bool {
    let AnchorPairOverlayInput {
        redaction_index,
        redaction,
        guess,
        anchor,
        selected_text,
        left_bbox,
        right_bbox,
        runs,
        assets,
        width_map,
        force_selected_only,
    } = input;
    let context_left = anchor_left_text(anchor);
    let context_right = anchor_right_text(anchor);
    let anchor_left_x = anchor.and_then(|value| value.usable_left_edge_x_pt);
    let anchor_right_x = anchor.and_then(|value| value.usable_right_edge_x_pt);
    let selected_text = normalize_overlay_line_breaks(selected_text.trim());
    if selected_text.is_empty() {
        return false;
    }
    let left_text = redaction
        .underlying_text
        .first()
        .map(|hit| hit.text.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(context_left);
    let right_text = redaction
        .underlying_text
        .get(1)
        .map(|hit| hit.text.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(context_right);
    if left_text.is_empty() && right_text.is_empty() {
        return false;
    }

    let left_anchor_run = if left_text.is_empty() {
        None
    } else {
        select_run_by_text(runs, redaction.page_index, left_text, left_bbox)
            .or_else(|| select_run_by_bbox(runs, redaction.page_index, left_bbox))
    };
    let right_anchor_run = if right_text.is_empty() {
        None
    } else {
        select_run_by_text(runs, redaction.page_index, right_text, right_bbox)
            .or_else(|| select_run_by_bbox(runs, redaction.page_index, right_bbox))
    };

    let run_style = left_anchor_run
        .or(right_anchor_run)
        .map(|run| (run.font_key.clone(), run.font_size_pt, run.h_scale_pct));
    let nearby_run = select_run_by_bbox(runs, redaction.page_index, Some(redaction.bbox))
        .map(|run| (run.font_key.clone(), run.font_size_pt, run.h_scale_pct));
    let (font_key, font_size_pt, h_scale_pct) =
        if let Some((font_key, run_font_size_pt, run_h_scale_pct)) = run_style.or(nearby_run) {
            let context_font_size_pt = guess.context.font_size_pt;
            let context_h_scale_pct = guess.context.h_scale_pct;
            let context_style_compatible = context_font_size_pt
                .map(|value| {
                    (value - run_font_size_pt).abs() <= OVERLAY_STYLE_FONT_SIZE_TOLERANCE_PT
                })
                .unwrap_or(true)
                && context_h_scale_pct
                    .map(|value| {
                        (value - run_h_scale_pct).abs() <= OVERLAY_STYLE_H_SCALE_TOLERANCE_PCT
                    })
                    .unwrap_or(true);
            let font_size_pt = if context_style_compatible {
                context_font_size_pt.unwrap_or(run_font_size_pt)
            } else {
                run_font_size_pt
            };
            let h_scale_pct = if context_style_compatible {
                context_h_scale_pct.unwrap_or(run_h_scale_pct)
            } else {
                run_h_scale_pct
            };
            (font_key, font_size_pt, h_scale_pct)
        } else {
            (
                "F1".to_owned(),
                guess.context.font_size_pt.unwrap_or(11.0_f32),
                guess.context.h_scale_pct.unwrap_or(100.0_f32),
            )
        };
    let y0 = left_anchor_run
        .map(|run| run.bbox.y0)
        .or_else(|| right_anchor_run.map(|run| run.bbox.y0))
        .or_else(|| left_bbox.map(|bbox| bbox.y0))
        .or_else(|| right_bbox.map(|bbox| bbox.y0))
        .unwrap_or(redaction.bbox.y0);
    let y1 = left_anchor_run
        .map(|run| run.bbox.y1)
        .or_else(|| right_anchor_run.map(|run| run.bbox.y1))
        .or_else(|| left_bbox.map(|bbox| bbox.y1))
        .or_else(|| right_bbox.map(|bbox| bbox.y1))
        .unwrap_or(redaction.bbox.y1);

    let use_joined_pair_overlay = !force_selected_only
        && !selected_text.contains('\n')
        && !left_text.is_empty()
        && !right_text.is_empty()
        && anchor_context_is_joinable(left_text, right_text);
    if !use_joined_pair_overlay {
        return push_anchor_selected_only_overlay(
            overlays,
            AnchorSelectedOverlayInput {
                redaction_index,
                redaction,
                selected_text: &selected_text,
                font_key: &font_key,
                font_size_pt,
                h_scale_pct,
                y0,
                y1,
                anchor_mode: guess.context.anchor_mode.as_deref(),
                anchor_left_x,
                anchor_right_x,
                left_anchor_text: left_text,
                right_anchor_text: right_text,
                prefer_anchor_placement: true,
                assets,
                width_map,
            },
        );
    }

    let left_x = left_anchor_run
        .map(|run| run.bbox.x0)
        .or(anchor_left_x)
        .or(anchor_right_x);
    let Some(left_x) = left_x else {
        return push_anchor_selected_only_overlay(
            overlays,
            AnchorSelectedOverlayInput {
                redaction_index,
                redaction,
                selected_text: &selected_text,
                font_key: &font_key,
                font_size_pt,
                h_scale_pct,
                y0,
                y1,
                anchor_mode: guess.context.anchor_mode.as_deref(),
                anchor_left_x,
                anchor_right_x,
                left_anchor_text: left_text,
                right_anchor_text: right_text,
                prefer_anchor_placement: true,
                assets,
                width_map,
            },
        );
    };

    let joined_text =
        normalize_overlay_line_breaks(&format!("{left_text} {selected_text} {right_text}"));
    let joined_width = multiline_text_width_pt(
        redaction.page_index,
        &font_key,
        font_size_pt,
        h_scale_pct,
        &joined_text,
        assets,
        width_map,
    )
    .max(0.1_f32);

    let joined_line_count = overlay_line_count(&joined_text);
    let first_line_bbox = Rect::new(left_x, y0, left_x + joined_width, y1);
    let row_right = max_row_right_edge(overlays, redaction.page_index, first_line_bbox);
    let overlap_guard_pt = 0.25_f32;
    let joined_overlaps_row = row_right
        .map(|right_edge| left_x + overlap_guard_pt < right_edge)
        .unwrap_or(false);
    if joined_overlaps_row {
        return push_anchor_selected_only_overlay(
            overlays,
            AnchorSelectedOverlayInput {
                redaction_index,
                redaction,
                selected_text: &selected_text,
                font_key: &font_key,
                font_size_pt,
                h_scale_pct,
                y0,
                y1,
                anchor_mode: guess.context.anchor_mode.as_deref(),
                anchor_left_x,
                anchor_right_x,
                left_anchor_text: left_text,
                right_anchor_text: right_text,
                prefer_anchor_placement: true,
                assets,
                width_map,
            },
        );
    }
    let (text, x, width, line_count) = (joined_text, left_x, joined_width, joined_line_count);
    let line_drop_pt = multiline_extra_line_drop_pt(font_size_pt, line_count);

    let overlay_bbox = Rect::new(
        x,
        (y0 - line_drop_pt).min(redaction.bbox.y0),
        x + width,
        y1.max(redaction.bbox.y1),
    );
    overlays.push(TextOverlay {
        redaction_index: Some(redaction_index),
        page_index: redaction.page_index,
        text,
        font_key,
        font_size_pt,
        h_scale_pct,
        x,
        y: y1,
        bbox: overlay_bbox,
    });
    true
}

fn push_anchor_selected_only_overlay(
    overlays: &mut Vec<TextOverlay>,
    input: AnchorSelectedOverlayInput<'_>,
) -> bool {
    let AnchorSelectedOverlayInput {
        redaction_index,
        redaction,
        selected_text,
        font_key,
        font_size_pt,
        h_scale_pct,
        y0,
        y1,
        anchor_mode,
        anchor_left_x,
        anchor_right_x,
        left_anchor_text,
        right_anchor_text,
        prefer_anchor_placement,
        assets,
        width_map,
    } = input;
    let selected_text = normalize_overlay_line_breaks(selected_text.trim());
    if selected_text.is_empty() {
        return false;
    }
    let anchor_mode = anchor_mode.unwrap_or_default();
    let direction = if anchor_mode == "right_only" {
        AnchorRenderDirection::Suffix
    } else {
        AnchorRenderDirection::Prefix
    };
    let left_anchor_text = left_anchor_text.trim();
    let right_anchor_text = right_anchor_text.trim();
    let space_width = text_width_pt(
        redaction.page_index,
        font_key,
        font_size_pt,
        h_scale_pct,
        " ",
        assets,
        width_map,
    )
    .max(0.1_f32);
    let line_step_pt = font_size_pt.max(1.0_f32) * OVERLAY_MULTILINE_LEADING_RATIO;
    let anchor_candidates = if prefer_anchor_placement {
        collect_overlay_anchor_candidates(redaction)
    } else {
        Vec::new()
    };
    let one_anchor_mode = matches!(anchor_mode, "left_only" | "right_only");
    let fallback_anchor = match direction {
        AnchorRenderDirection::Prefix => (
            anchor_left_x.or(anchor_right_x),
            if !left_anchor_text.is_empty() {
                left_anchor_text
            } else {
                right_anchor_text
            },
        ),
        AnchorRenderDirection::Suffix => (
            anchor_right_x.or(anchor_left_x),
            if !right_anchor_text.is_empty() {
                right_anchor_text
            } else {
                left_anchor_text
            },
        ),
    };
    let mut previous_anchor_x = fallback_anchor
        .0
        .unwrap_or_else(|| redaction.bbox.x0.max(0.0_f32));
    let mut previous_anchor_center_y: Option<f32> = None;
    let mut wrote_overlay = false;

    for (line_index, raw_line) in selected_text.split('\n').enumerate() {
        let guess_line = raw_line.trim();
        if guess_line.is_empty() {
            continue;
        }
        let line_index_f = line_index as f32;
        let target_y = y1 - (line_index_f * line_step_pt);
        let mut chosen_anchor_x: Option<f32> = None;
        let mut chosen_anchor_text = String::new();
        let mut chosen_anchor_bbox: Option<Rect> = None;
        let mut chosen_anchor_center_y: Option<f32> = None;

        if one_anchor_mode {
            if line_index == 0 {
                chosen_anchor_x = fallback_anchor.0;
                chosen_anchor_text = fallback_anchor.1.to_owned();
            }
        } else if prefer_anchor_placement {
            if let Some(candidate) =
                select_anchor_candidate_for_line(&anchor_candidates, redaction, target_y, direction)
            {
                chosen_anchor_x = Some(candidate.bbox.x0);
                chosen_anchor_text = candidate.text;
                chosen_anchor_bbox = Some(candidate.bbox);
                chosen_anchor_center_y = Some((candidate.bbox.y0 + candidate.bbox.y1) * 0.5_f32);
            } else if line_index == 0 {
                chosen_anchor_x = fallback_anchor.0;
                chosen_anchor_text = fallback_anchor.1.to_owned();
            }
        }
        if line_index > 0 {
            if let (Some(prev_center_y), Some(candidate_center_y)) =
                (previous_anchor_center_y, chosen_anchor_center_y)
            {
                let min_drop = line_step_pt * 0.70_f32;
                if candidate_center_y > prev_center_y - min_drop {
                    chosen_anchor_x = None;
                    chosen_anchor_text.clear();
                    chosen_anchor_bbox = None;
                    chosen_anchor_center_y = None;
                }
            }
        }

        if chosen_anchor_x.is_none() && line_index > 0 {
            chosen_anchor_x = Some(previous_anchor_x);
        }
        let guess_width = text_width_pt(
            redaction.page_index,
            font_key,
            font_size_pt,
            h_scale_pct,
            guess_line,
            assets,
            width_map,
        )
        .max(0.1_f32);
        if chosen_anchor_x.is_none() {
            let box_width = redaction.bbox.width().abs().max(1.0_f32);
            chosen_anchor_x = Some(redaction.bbox.x0 + ((box_width - guess_width) * 0.5_f32));
        }
        let anchor_x = chosen_anchor_x.unwrap_or(redaction.bbox.x0);
        let anchor_text = chosen_anchor_text.trim();
        let use_anchor_text = !anchor_text.is_empty();

        let (line_text, mut x) = match direction {
            AnchorRenderDirection::Prefix => {
                if use_anchor_text {
                    (format!("{anchor_text} {guess_line}"), anchor_x)
                } else {
                    (guess_line.to_owned(), anchor_x)
                }
            }
            AnchorRenderDirection::Suffix => {
                if use_anchor_text {
                    let guess_with_space_width = guess_width + space_width;
                    (
                        format!("{guess_line} {anchor_text}"),
                        anchor_x - guess_with_space_width,
                    )
                } else {
                    (guess_line.to_owned(), anchor_x)
                }
            }
        };
        let rendered_width = text_width_pt(
            redaction.page_index,
            font_key,
            font_size_pt,
            h_scale_pct,
            &line_text,
            assets,
            width_map,
        )
        .max(0.1_f32);
        let mut line_y0 = y0 - (line_index_f * line_step_pt);
        let mut line_y1 = y1 - (line_index_f * line_step_pt);
        if let Some(anchor_bbox) = chosen_anchor_bbox {
            line_y0 = anchor_bbox.y0;
            line_y1 = anchor_bbox.y1;
        }

        if !use_anchor_text && line_index == 0 {
            let candidate_bbox = Rect::new(x, line_y0, x + rendered_width, line_y1);
            let row_right = max_row_right_edge(overlays, redaction.page_index, candidate_bbox);
            let overlap_guard_pt = 0.25_f32;
            if let Some(right_edge) = row_right {
                if x + overlap_guard_pt < right_edge {
                    x = right_edge + space_width;
                }
            }
        }

        let mut bbox_left = x.min(redaction.bbox.x0);
        let mut bbox_right = (x + rendered_width).max(redaction.bbox.x1);
        if let Some(anchor_bbox) = chosen_anchor_bbox {
            bbox_left = bbox_left.min(anchor_bbox.x0);
            bbox_right = bbox_right.max(anchor_bbox.x1);
        }
        let overlay_bbox = Rect::new(
            bbox_left,
            line_y0.min(redaction.bbox.y0),
            bbox_right,
            line_y1.max(redaction.bbox.y1),
        );
        overlays.push(TextOverlay {
            redaction_index: Some(redaction_index),
            page_index: redaction.page_index,
            text: line_text,
            font_key: font_key.to_owned(),
            font_size_pt,
            h_scale_pct,
            x,
            y: line_y1,
            bbox: overlay_bbox,
        });
        previous_anchor_x = anchor_x;
        previous_anchor_center_y = Some(chosen_anchor_center_y.unwrap_or(target_y));
        wrote_overlay = true;
    }
    wrote_overlay
}

fn collect_overlay_anchor_candidates(
    redaction: &crate::types::redaction_types::RedactionOccurrence,
) -> Vec<OverlayAnchorCandidate> {
    let mut out = Vec::<OverlayAnchorCandidate>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();

    for span in parse_overlay_context_spans(redaction) {
        let text = span.text.trim();
        if text.is_empty() {
            continue;
        }
        let line_bucket = if span.line_bucket == 0_i32 {
            overlay_context_line_bucket(span.bbox)
        } else {
            span.line_bucket
        };
        let key = format!(
            "{line_bucket}:{:.1}:{:.1}:{:.1}:{:.1}:{}",
            span.bbox.x0, span.bbox.y0, span.bbox.x1, span.bbox.y1, text
        );
        if !seen.insert(key) {
            continue;
        }
        out.push(OverlayAnchorCandidate {
            text: text.to_owned(),
            bbox: span.bbox,
            line_bucket,
            role_hint: span.role_hint,
        });
    }

    for hit in &redaction.underlying_text {
        let text = hit.text.trim();
        if text.is_empty() {
            continue;
        }
        let line_bucket = overlay_context_line_bucket(hit.bbox);
        let role_hint = if hit.bbox.x1 <= redaction.bbox.x0 + 0.5_f32 {
            "left".to_owned()
        } else if hit.bbox.x0 >= redaction.bbox.x1 - 0.5_f32 {
            "right".to_owned()
        } else {
            "center".to_owned()
        };
        let key = format!(
            "{line_bucket}:{:.1}:{:.1}:{:.1}:{:.1}:{}",
            hit.bbox.x0, hit.bbox.y0, hit.bbox.x1, hit.bbox.y1, text
        );
        if !seen.insert(key) {
            continue;
        }
        out.push(OverlayAnchorCandidate {
            text: text.to_owned(),
            bbox: hit.bbox,
            line_bucket,
            role_hint,
        });
    }

    out.sort_by(|left, right| {
        left.line_bucket
            .cmp(&right.line_bucket)
            .then_with(|| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.text.cmp(&right.text))
    });
    out
}

fn parse_overlay_context_spans(
    redaction: &crate::types::redaction_types::RedactionOccurrence,
) -> Vec<OverlayContextSpanRecord> {
    let Some(raw) = redaction.meta.get(CONTEXT_SPANS_META_KEY) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<OverlayContextSpanRecord>>(raw).unwrap_or_default()
}

fn overlay_context_line_bucket(rect: Rect) -> i32 {
    let center_y = (rect.y0 + rect.y1) * 0.5_f32;
    (center_y / OVERLAY_CONTEXT_LINE_BUCKET_PT).round() as i32
}

fn select_anchor_candidate_for_line(
    candidates: &[OverlayAnchorCandidate],
    redaction: &crate::types::redaction_types::RedactionOccurrence,
    target_y: f32,
    direction: AnchorRenderDirection,
) -> Option<OverlayAnchorCandidate> {
    let target_bucket = (target_y / OVERLAY_CONTEXT_LINE_BUCKET_PT).round() as i32;
    let mut filtered = candidates
        .iter()
        .filter(|candidate| {
            let text = candidate.text.trim();
            if text.is_empty() {
                return false;
            }
            let center_y = (candidate.bbox.y0 + candidate.bbox.y1) * 0.5_f32;
            let y_delta = (center_y - target_y).abs();
            let bucket_delta = (candidate.line_bucket - target_bucket).abs();
            if bucket_delta > 1_i32 && y_delta > OVERLAY_ANCHOR_LINE_TOLERANCE_PT {
                return false;
            }
            match direction {
                AnchorRenderDirection::Prefix => {
                    if candidate.role_hint == "right" {
                        return false;
                    }
                    let gap = redaction.bbox.x0 - candidate.bbox.x1;
                    if gap > OVERLAY_ANCHOR_MAX_GAP_PT {
                        return false;
                    }
                    if gap > OVERLAY_ANCHOR_EDGE_WINDOW_PT {
                        return false;
                    }
                    candidate.bbox.x0 <= redaction.bbox.x1 + OVERLAY_ANCHOR_MAX_GAP_PT * 0.2_f32
                }
                AnchorRenderDirection::Suffix => {
                    if candidate.role_hint == "left" {
                        return false;
                    }
                    let gap = candidate.bbox.x0 - redaction.bbox.x1;
                    if gap > OVERLAY_ANCHOR_MAX_GAP_PT {
                        return false;
                    }
                    if gap > OVERLAY_ANCHOR_EDGE_WINDOW_PT {
                        return false;
                    }
                    candidate.bbox.x1 + OVERLAY_ANCHOR_MAX_GAP_PT * 0.2_f32 >= redaction.bbox.x0
                }
            }
        })
        .cloned()
        .collect::<Vec<_>>();

    filtered.sort_by(|left, right| {
        let left_center_y = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
        let right_center_y = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
        let left_y_delta = (left_center_y - target_y).abs();
        let right_y_delta = (right_center_y - target_y).abs();
        match direction {
            AnchorRenderDirection::Prefix => left
                .bbox
                .x0
                .partial_cmp(&right.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left_y_delta
                        .partial_cmp(&right_y_delta)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }),
            AnchorRenderDirection::Suffix => right
                .bbox
                .x0
                .partial_cmp(&left.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left_y_delta
                        .partial_cmp(&right_y_delta)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }),
        }
    });
    filtered.into_iter().next()
}

fn raster_overlay_layout(
    rect: Rect,
    requested_font_size_pt: f32,
    text_width_pt: f32,
    line_count: usize,
) -> RasterOverlayLayout {
    let box_width = rect.width().abs().max(1.0_f32);
    let box_height = rect.height().abs().max(1.0_f32);
    let font_size_pt = requested_font_size_pt.max(RASTER_MIN_FONT_SIZE_PT);

    let x_min = rect.x0 + RASTER_TEXT_PADDING_PT;
    let x_max = (rect.x1 - RASTER_TEXT_PADDING_PT - text_width_pt).max(x_min);
    let centered_x = rect.x0 + ((box_width - text_width_pt) * 0.5_f32);
    let x = centered_x.clamp(x_min, x_max);

    let line_drop_pt = multiline_extra_line_drop_pt(font_size_pt, line_count);
    let text_bottom = rect.y0 + ((box_height - font_size_pt) * 0.5_f32);
    let baseline =
        text_bottom + (font_size_pt * RASTER_BASELINE_ASCENT_RATIO) + (line_drop_pt * 0.5_f32);
    let y_min = rect.y0 + RASTER_TEXT_PADDING_PT + line_drop_pt;
    let y_max = rect.y1 - RASTER_TEXT_PADDING_PT;
    let y = baseline.clamp(y_min.min(y_max), y_max);

    RasterOverlayLayout { x, y, font_size_pt }
}

fn pick_best_guess(guess: &crate::types::guess_types::RedactionGuess) -> Option<&str> {
    guess.candidates.first().map(|c| c.text.as_str())
}

fn anchor_left_text(anchor: Option<&AnchorDecisionRecord>) -> &str {
    anchor
        .and_then(|value| value.left.as_ref())
        .map(|side| side.text.trim())
        .unwrap_or_default()
}

fn anchor_right_text(anchor: Option<&AnchorDecisionRecord>) -> &str {
    anchor
        .and_then(|value| value.right.as_ref())
        .map(|side| side.text.trim())
        .unwrap_or_default()
}

fn build_dense_anchor_row_flags(report: &RedactionReport) -> Vec<bool> {
    let redactions = &report.redactions;
    let mut out = vec![false; redactions.len()];
    for (index, current) in redactions.iter().enumerate() {
        out[index] = redactions
            .iter()
            .enumerate()
            .any(|(other_index, other)| is_dense_row_neighbor(index, current, other_index, other));
    }
    out
}

fn build_dense_row_max_heights(report: &RedactionReport) -> Vec<f32> {
    let redactions = &report.redactions;
    let mut out = vec![0.0_f32; redactions.len()];
    for (index, current) in redactions.iter().enumerate() {
        let mut max_height = current.bbox.height().abs().max(1.0_f32);
        for (other_index, other) in redactions.iter().enumerate() {
            if !is_dense_row_neighbor(index, current, other_index, other) {
                continue;
            }
            max_height = max_height.max(other.bbox.height().abs().max(1.0_f32));
        }
        out[index] = max_height;
    }
    out
}

fn is_dense_row_neighbor(
    index: usize,
    current: &crate::types::redaction_types::RedactionOccurrence,
    other_index: usize,
    other: &crate::types::redaction_types::RedactionOccurrence,
) -> bool {
    if index == other_index {
        return false;
    }
    if current.page_index != other.page_index {
        return false;
    }
    if !same_visual_row(current.bbox, other.bbox) {
        return false;
    }
    horizontal_gap_between_rects(current.bbox, other.bbox) <= 96.0_f32
}

fn horizontal_gap_between_rects(left: Rect, right: Rect) -> f32 {
    if left.x1 < right.x0 {
        return right.x0 - left.x1;
    }
    if right.x1 < left.x0 {
        return left.x0 - right.x1;
    }
    0.0_f32
}

fn anchor_context_is_joinable(left_text: &str, right_text: &str) -> bool {
    let left_words = anchor_words(left_text);
    let right_words = anchor_words(right_text);
    if left_words.is_empty() || right_words.is_empty() {
        return false;
    }
    if left_words.len() > 3 || right_words.len() > 3 {
        return false;
    }
    let left_has_marker = left_words
        .iter()
        .any(|word| matches!(word.as_str(), "including" | "included" | "among" | "served"));
    let right_has_and = right_words.iter().any(|word| word == "and");
    left_has_marker && right_has_and
}

fn anchor_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|ch| ch.is_ascii_alphabetic())
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
}

fn normalize_overlay_line_breaks(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn overlay_line_count(text: &str) -> usize {
    text.split('\n').count().max(1)
}

fn multiline_extra_line_drop_pt(font_size_pt: f32, line_count: usize) -> f32 {
    if line_count <= 1 {
        return 0.0_f32;
    }
    let line_step_pt = font_size_pt.max(1.0_f32) * OVERLAY_MULTILINE_LEADING_RATIO;
    (line_count.saturating_sub(1) as f32) * line_step_pt
}

fn multiline_text_width_pt(
    page_index: u32,
    font_key: &str,
    font_size_pt: f32,
    h_scale_pct: f32,
    text: &str,
    assets: &std::collections::BTreeMap<String, crate::types::file_types::FontAsset>,
    width_map: &std::collections::BTreeMap<FontWidthKey, FontWidthTable>,
) -> f32 {
    let mut best = 0.0_f32;
    for line in text.split('\n') {
        if line.is_empty() {
            continue;
        }
        let width = text_width_pt(
            page_index,
            font_key,
            font_size_pt,
            h_scale_pct,
            line,
            assets,
            width_map,
        );
        if width.is_finite() && width > best {
            best = width;
        }
    }
    if best > 0.0_f32 {
        best
    } else {
        0.1_f32
    }
}

fn max_row_right_edge(
    overlays: &[TextOverlay],
    page_index: u32,
    candidate_bbox: Rect,
) -> Option<f32> {
    overlays
        .iter()
        .filter(|overlay| overlay.page_index == page_index)
        .filter(|overlay| same_visual_row(overlay.bbox, candidate_bbox))
        .map(|overlay| overlay.bbox.x1)
        .max_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
}

fn same_visual_row(left: Rect, right: Rect) -> bool {
    let overlap = vertical_overlap_rect(left, right);
    if overlap <= 0.0_f32 {
        return false;
    }
    let left_height = left.height().abs().max(1.0_f32);
    let right_height = right.height().abs().max(1.0_f32);
    let min_height = left_height.min(right_height).max(1.0_f32);
    overlap / min_height >= 0.40_f32
}

fn vertical_overlap_rect(left: Rect, right: Rect) -> f32 {
    (left.y1.min(right.y1) - left.y0.max(right.y0)).max(0.0_f32)
}

fn select_run_by_text<'a>(
    runs: &'a [FontTextRun],
    page_index: u32,
    text: &str,
    bbox: Option<Rect>,
) -> Option<&'a FontTextRun> {
    let mut best: Option<(&FontTextRun, f32)> = None;
    for run in runs {
        if run.page_index != page_index {
            continue;
        }
        if run.text.trim() != text {
            continue;
        }
        if let Some(b) = bbox {
            if vertical_overlap_run(&run.bbox, &b) <= 0.0 {
                continue;
            }
        }
        let dist = bbox.map(|b| (run.bbox.x0 - b.x0).abs()).unwrap_or(0.0);
        match best {
            None => best = Some((run, dist)),
            Some((_, best_score)) if dist < best_score => best = Some((run, dist)),
            _ => {}
        }
    }
    best.map(|(r, _)| r)
}

fn select_run_by_bbox(
    runs: &[FontTextRun],
    page_index: u32,
    bbox: Option<Rect>,
) -> Option<&FontTextRun> {
    let bbox = bbox?;
    let mut best: Option<(&FontTextRun, f32, f32)> = None;
    for run in runs {
        if run.page_index != page_index {
            continue;
        }
        let overlap = vertical_overlap_run(&run.bbox, &bbox);
        if overlap <= 0.0 {
            continue;
        }
        let dist = (run.bbox.x0 - bbox.x0).abs();
        match best {
            None => best = Some((run, overlap, dist)),
            Some((_, best_overlap, best_dist)) => {
                if overlap > best_overlap || (overlap == best_overlap && dist < best_dist) {
                    best = Some((run, overlap, dist));
                }
            }
        }
    }
    best.map(|(r, _, _)| r)
}

fn vertical_overlap_run(a: &FontRect, b: &Rect) -> f32 {
    (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FontWidthKey {
    page_index: u32,
    font_key: String,
}

#[derive(Debug, Clone)]
struct FontWidthTable {
    first_char: u16,
    widths: Vec<f32>,
}

fn text_width_pt(
    page_index: u32,
    font_key: &str,
    font_size_pt: f32,
    h_scale_pct: f32,
    text: &str,
    assets: &std::collections::BTreeMap<String, crate::types::file_types::FontAsset>,
    width_map: &std::collections::BTreeMap<FontWidthKey, FontWidthTable>,
) -> f32 {
    let scale = (h_scale_pct / 100.0_f32).max(0.01_f32);
    if let Some(asset) = assets.get(font_key) {
        if let Some(width) = advance_pt(asset, text, font_size_pt) {
            if width.is_finite() && width > 0.0 {
                return width * scale;
            }
        }
    }
    let key = FontWidthKey {
        page_index,
        font_key: font_key.to_owned(),
    };
    if let Some(table) = width_map.get(&key) {
        if let Some(width) = width_from_table(table, text, font_size_pt) {
            if width.is_finite() && width > 0.0 {
                return width * scale;
            }
        }
    }
    fallback_width_pt(text, font_size_pt) * scale
}

fn advance_pt(
    asset: &crate::types::file_types::FontAsset,
    text: &str,
    font_size_pt: f32,
) -> Option<f32> {
    let face = rustybuzz::Face::from_slice(&asset.bytes, 0)?;
    let units_per_em = asset.units_per_em.max(1) as f32;
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(text);
    let out = rustybuzz::shape(&face, shaping_features(), buf);
    let units = out
        .glyph_positions()
        .iter()
        .map(|p| p.x_advance as f32)
        .sum::<f32>()
        / GLYPH_UNITS_SCALE as f32;
    Some(units * (font_size_pt / units_per_em))
}

fn width_from_table(table: &FontWidthTable, text: &str, font_size_pt: f32) -> Option<f32> {
    let mut sum = 0.0_f32;
    let mut any = false;
    for ch in text.chars() {
        let code = ch as u32;
        if code > u16::MAX as u32 {
            continue;
        }
        let code = code as u16;
        if code < table.first_char {
            continue;
        }
        let idx = (code - table.first_char) as usize;
        if idx >= table.widths.len() {
            continue;
        }
        sum += table.widths[idx] * (font_size_pt / 1000.0);
        any = true;
    }
    any.then_some(sum)
}

fn fallback_width_pt(text: &str, font_size_pt: f32) -> f32 {
    let count = text.chars().count().max(1) as f32;
    font_size_pt.abs().max(1.0) * 0.6 * count
}

fn build_font_width_map(
    pdf_bytes: &[u8],
) -> Result<std::collections::BTreeMap<FontWidthKey, FontWidthTable>, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|e| e.to_string())?;
    let pages = doc.get_pages();
    let mut map = std::collections::BTreeMap::new();

    for (page_no, page_id) in pages {
        let page_index = page_no.saturating_sub(1);
        let (res_opt, _unused_pages) =
            doc.get_page_resources(page_id).map_err(|e| e.to_string())?;
        let resources = match res_opt {
            Some(r) => r,
            None => continue,
        };
        let font_obj = match resources.get(b"Font").ok() {
            Some(o) => o,
            None => continue,
        };
        let font_dict = match deref_to_dict(&doc, font_obj).or_else(|| object_to_dict(font_obj)) {
            Some(d) => d,
            None => continue,
        };
        for (key_bytes, value_object) in font_dict.iter() {
            let key = String::from_utf8_lossy(key_bytes).to_string();
            let dict =
                match deref_to_dict(&doc, value_object).or_else(|| object_to_dict(value_object)) {
                    Some(d) => d,
                    None => continue,
                };
            let target = resolve_width_dict(&doc, dict);
            let target = match target {
                Some(d) => d,
                None => continue,
            };
            let first_char = target.get(b"FirstChar").ok().and_then(object_to_u16);
            let widths = target.get(b"Widths").ok().and_then(object_to_f32_array);
            let (first_char, widths) = match (first_char, widths) {
                (Some(f), Some(w)) if !w.is_empty() => (f, w),
                _ => continue,
            };
            let table = FontWidthTable { first_char, widths };
            map.insert(
                FontWidthKey {
                    page_index,
                    font_key: key,
                },
                table,
            );
        }
    }

    Ok(map)
}

fn resolve_width_dict<'a>(doc: &'a Document, dict: &'a Dictionary) -> Option<&'a Dictionary> {
    if dict.has(b"Widths") {
        return Some(dict);
    }
    let subtype = dict.get(b"Subtype").ok().and_then(object_to_name_string);
    if subtype.as_deref() == Some("Type0") {
        let descendants = dict.get(b"DescendantFonts").ok().and_then(object_to_array);
        let first = descendants
            .and_then(|arr| arr.first())
            .and_then(|obj| deref_to_dict(doc, obj));
        if let Some(desc) = first {
            if desc.has(b"Widths") {
                return Some(desc);
            }
        }
    }
    None
}

fn object_to_u16(object: &Object) -> Option<u16> {
    match object {
        Object::Integer(v) => (*v).try_into().ok(),
        Object::Real(v) => (*v as i64).try_into().ok(),
        _ => None,
    }
}

fn object_to_f32(object: &Object) -> Option<f32> {
    match object {
        Object::Real(real_value) => Some(*real_value),
        Object::Integer(integer_value) => Some(*integer_value as f32),
        _ => None,
    }
}

fn object_to_f32_array(object: &Object) -> Option<Vec<f32>> {
    match object {
        Object::Array(values) => {
            let mut out = Vec::with_capacity(values.len());
            for item in values {
                if let Some(v) = object_to_f32(item) {
                    out.push(v);
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn object_to_array(object: &Object) -> Option<&Vec<Object>> {
    match object {
        Object::Array(values) => Some(values),
        _ => None,
    }
}

fn object_to_name_string(object: &Object) -> Option<String> {
    match object {
        Object::Name(name_bytes) => Some(String::from_utf8_lossy(name_bytes).to_string()),
        _ => None,
    }
}

fn object_to_dict(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        _ => None,
    }
}

fn deref_to_dict<'doc>(doc: &'doc Document, object: &'doc Object) -> Option<&'doc Dictionary> {
    match object {
        Object::Reference(object_id) => match doc.get_object(*object_id).ok()? {
            Object::Dictionary(d) => Some(d),
            _ => None,
        },
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_font_width_map, text_width_pt, VisualizationData};
    use crate::types::file_types::{FontRunReport, FontTextRun, Rect as FontRect};
    use crate::types::guess_types::{
        AnchorDecisionRecord, AnchorSideDecision, AnchorType, GuessCandidate, GuessContext,
        GuessReport, RedactionGuess,
    };
    use crate::types::redaction_types::{
        Rect, RedactionKind, RedactionOccurrence, RedactionReport, UnderlyingTextHit,
    };
    use crate::types::visualizer_config::VisualizerConfig;

    fn sample_report() -> RedactionReport {
        RedactionReport {
            input: "x.pdf".to_owned(),
            redactions: vec![RedactionOccurrence {
                page_index: 0_u32,
                bbox: Rect::new(100.0_f32, 200.0_f32, 170.0_f32, 214.0_f32),
                kind: RedactionKind::RasterDarkRegion,
                score: 1.0_f32,
                meta: std::collections::BTreeMap::new(),
                underlying_text: vec![],
            }],
            count: 1_u32,
            page_counts: std::collections::BTreeMap::from([(0_u32, 1_u32)]),
            diagnostics: vec![],
        }
    }

    fn sample_anchor_pair_report() -> RedactionReport {
        RedactionReport {
            input: "x.pdf".to_owned(),
            redactions: vec![RedactionOccurrence {
                page_index: 0_u32,
                bbox: Rect::new(100.0_f32, 200.0_f32, 170.0_f32, 214.0_f32),
                kind: RedactionKind::RasterDarkRegion,
                score: 1.0_f32,
                meta: std::collections::BTreeMap::new(),
                underlying_text: vec![
                    UnderlyingTextHit {
                        page_index: 0_u32,
                        bbox: Rect::new(104.0_f32, 208.0_f32, 150.0_f32, 219.0_f32),
                        text: "including".to_owned(),
                    },
                    UnderlyingTextHit {
                        page_index: 0_u32,
                        bbox: Rect::new(214.0_f32, 208.0_f32, 232.0_f32, 219.0_f32),
                        text: "and".to_owned(),
                    },
                ],
            }],
            count: 1_u32,
            page_counts: std::collections::BTreeMap::from([(0_u32, 1_u32)]),
            diagnostics: vec![],
        }
    }

    fn sample_multi_anchor_pair_report() -> RedactionReport {
        RedactionReport {
            input: "x.pdf".to_owned(),
            redactions: vec![
                RedactionOccurrence {
                    page_index: 0_u32,
                    bbox: Rect::new(100.0_f32, 200.0_f32, 170.0_f32, 214.0_f32),
                    kind: RedactionKind::RasterDarkRegion,
                    score: 1.0_f32,
                    meta: std::collections::BTreeMap::new(),
                    underlying_text: vec![
                        UnderlyingTextHit {
                            page_index: 0_u32,
                            bbox: Rect::new(104.0_f32, 208.0_f32, 150.0_f32, 219.0_f32),
                            text: "including".to_owned(),
                        },
                        UnderlyingTextHit {
                            page_index: 0_u32,
                            bbox: Rect::new(214.0_f32, 208.0_f32, 232.0_f32, 219.0_f32),
                            text: "and".to_owned(),
                        },
                    ],
                },
                RedactionOccurrence {
                    page_index: 0_u32,
                    bbox: Rect::new(176.0_f32, 200.0_f32, 246.0_f32, 214.0_f32),
                    kind: RedactionKind::RasterDarkRegion,
                    score: 1.0_f32,
                    meta: std::collections::BTreeMap::new(),
                    underlying_text: vec![
                        UnderlyingTextHit {
                            page_index: 0_u32,
                            bbox: Rect::new(150.0_f32, 208.0_f32, 196.0_f32, 219.0_f32),
                            text: "including".to_owned(),
                        },
                        UnderlyingTextHit {
                            page_index: 0_u32,
                            bbox: Rect::new(260.0_f32, 208.0_f32, 278.0_f32, 219.0_f32),
                            text: "and".to_owned(),
                        },
                    ],
                },
            ],
            count: 2_u32,
            page_counts: std::collections::BTreeMap::from([(0_u32, 2_u32)]),
            diagnostics: vec![],
        }
    }

    fn sample_guesses() -> GuessReport {
        sample_guesses_with_candidate("SARAH KELLEN")
    }

    fn sample_guesses_with_candidate(top_text: &str) -> GuessReport {
        let bbox = Rect::new(100.0_f32, 200.0_f32, 170.0_f32, 214.0_f32);
        GuessReport {
            input_redactions: String::new(),
            input_fonts: String::new(),
            guesses: vec![RedactionGuess {
                page_index: 0_u32,
                bbox,
                candidates: vec![GuessCandidate {
                    text: top_text.to_owned(),
                    width_pt: 70.0_f32,
                    glyph_width_sum_pt: 70.0_f32,
                    char_spacing_total_pt: 0.0_f32,
                    word_spacing_total_pt: 0.0_f32,
                    predicted_left_edge_x_pt: Some(80.0_f32),
                    predicted_right_edge_x_pt: Some(150.0_f32),
                    actual_right_edge_x_pt: Some(150.0_f32),
                    target_width_pt: 70.0_f32,
                    error_pt: 0.0_f32,
                    normalized_error: Some(0.0_f32),
                }],
                context: GuessContext {
                    anchor_mode: Some("two_sided".to_owned()),
                    usable_left_edge_x_pt: Some(80.0_f32),
                    usable_right_edge_x_pt: Some(190.0_f32),
                    target_width_pt: 110.0_f32,
                    font_key: Some("F_anchor".to_owned()),
                    font_name: Some("Times-Roman".to_owned()),
                    base_font: Some("Times-Roman".to_owned()),
                    font_size_pt: Some(11.0_f32),
                    h_scale_pct: Some(100.0_f32),
                    char_spacing_pt: Some(0.0_f32),
                    word_spacing_pt: Some(0.0_f32),
                    width_source: Some("standard_14_font".to_owned()),
                    encoding_source: Some("named_encoding".to_owned()),
                },
            }],
            anchors: vec![AnchorDecisionRecord {
                anchor_row_id: "page0_row0".to_owned(),
                page_index: 0_u32,
                bbox,
                anchor_mode: "two_sided".to_owned(),
                left: Some(AnchorSideDecision {
                    anchor_id: "left0".to_owned(),
                    anchor_type: AnchorType::Left,
                    text: "including".to_owned(),
                    bbox: Rect::new(104.0_f32, 208.0_f32, 150.0_f32, 219.0_f32),
                    x: 80.0_f32,
                }),
                right: Some(AnchorSideDecision {
                    anchor_id: "right0".to_owned(),
                    anchor_type: AnchorType::Right,
                    text: "and".to_owned(),
                    bbox: Rect::new(214.0_f32, 208.0_f32, 232.0_f32, 219.0_f32),
                    x: 190.0_f32,
                }),
                usable_left_edge_x_pt: Some(80.0_f32),
                usable_right_edge_x_pt: Some(190.0_f32),
                target_width_pt: 110.0_f32,
                font_key: "F_anchor".to_owned(),
                font_name: "Times-Roman".to_owned(),
                base_font: Some("Times-Roman".to_owned()),
                font_size_pt: 11.0_f32,
                h_scale_pct: 100.0_f32,
                char_spacing_pt: 0.0_f32,
                word_spacing_pt: 0.0_f32,
                width_source: Some("standard_14_font".to_owned()),
                encoding_source: Some("named_encoding".to_owned()),
            }],
            stage_timings: vec![],
        }
    }

    fn sample_multi_anchor_pair_guesses() -> GuessReport {
        let mut report = sample_guesses_with_candidate("SARAH KELLEN");
        report.anchors[0].usable_left_edge_x_pt = Some(104.0_f32);
        report.anchors[0].usable_right_edge_x_pt = Some(214.0_f32);
        if let Some(left) = report.anchors[0].left.as_mut() {
            left.x = 104.0_f32;
        }
        if let Some(right) = report.anchors[0].right.as_mut() {
            right.x = 214.0_f32;
        }

        let first = report
            .guesses
            .first()
            .cloned()
            .expect("sample guess should exist");
        let mut second = first.clone();
        second.bbox = Rect::new(176.0_f32, 200.0_f32, 246.0_f32, 214.0_f32);
        second.candidates = vec![GuessCandidate {
            text: "ADRIANA MUCINSKA".to_owned(),
            width_pt: 75.0_f32,
            glyph_width_sum_pt: 75.0_f32,
            char_spacing_total_pt: 0.0_f32,
            word_spacing_total_pt: 0.0_f32,
            predicted_left_edge_x_pt: Some(150.0_f32),
            predicted_right_edge_x_pt: Some(225.0_f32),
            actual_right_edge_x_pt: Some(225.0_f32),
            target_width_pt: 75.0_f32,
            error_pt: 0.1_f32,
            normalized_error: Some(0.1_f32),
        }];

        let second_anchor = AnchorDecisionRecord {
            anchor_row_id: "page0_row1".to_owned(),
            page_index: 0_u32,
            bbox: second.bbox,
            anchor_mode: "two_sided".to_owned(),
            left: Some(AnchorSideDecision {
                anchor_id: "left1".to_owned(),
                anchor_type: AnchorType::Left,
                text: "including".to_owned(),
                bbox: Rect::new(150.0_f32, 208.0_f32, 196.0_f32, 219.0_f32),
                x: 150.0_f32,
            }),
            right: Some(AnchorSideDecision {
                anchor_id: "right1".to_owned(),
                anchor_type: AnchorType::Right,
                text: "and".to_owned(),
                bbox: Rect::new(260.0_f32, 208.0_f32, 278.0_f32, 219.0_f32),
                x: 260.0_f32,
            }),
            usable_left_edge_x_pt: Some(150.0_f32),
            usable_right_edge_x_pt: Some(260.0_f32),
            target_width_pt: 110.0_f32,
            font_key: "F_anchor".to_owned(),
            font_name: "Times-Roman".to_owned(),
            base_font: Some("Times-Roman".to_owned()),
            font_size_pt: 11.0_f32,
            h_scale_pct: 100.0_f32,
            char_spacing_pt: 0.0_f32,
            word_spacing_pt: 0.0_f32,
            width_source: Some("standard_14_font".to_owned()),
            encoding_source: Some("named_encoding".to_owned()),
        };

        GuessReport {
            input_redactions: String::new(),
            input_fonts: String::new(),
            guesses: vec![first, second],
            anchors: vec![report.anchors.remove(0), second_anchor],
            stage_timings: vec![],
        }
    }

    fn sample_font_runs() -> FontRunReport {
        FontRunReport {
            input: "x.pdf".to_owned(),
            runs: vec![],
            assets: vec![],
        }
    }

    fn sample_font_runs_with_anchor_row() -> FontRunReport {
        FontRunReport {
            input: "x.pdf".to_owned(),
            runs: vec![
                FontTextRun {
                    page_index: 0_u32,
                    text: "including".to_owned(),
                    bbox: FontRect::new(104.0_f32, 208.0_f32, 150.0_f32, 219.0_f32),
                    font_key: "F_anchor".to_owned(),
                    font_name: "AnchorFont".to_owned(),
                    font_size_pt: 11.0_f32,
                    h_scale_pct: 96.0_f32,
                    measured_width_pt: None,
                    measured_width_px: None,
                    measured_dpi: None,
                    char_advances_pt: vec![],
                    char_advances_px: vec![],
                },
                FontTextRun {
                    page_index: 0_u32,
                    text: "and".to_owned(),
                    bbox: FontRect::new(214.0_f32, 208.0_f32, 232.0_f32, 219.0_f32),
                    font_key: "F_anchor".to_owned(),
                    font_name: "AnchorFont".to_owned(),
                    font_size_pt: 11.0_f32,
                    h_scale_pct: 96.0_f32,
                    measured_width_pt: None,
                    measured_width_px: None,
                    measured_dpi: None,
                    char_advances_pt: vec![],
                    char_advances_px: vec![],
                },
            ],
            assets: vec![],
        }
    }

    fn sample_pdf_bytes() -> Vec<u8> {
        let input = std::path::Path::new("test_data/EFTA00101126.pdf");
        std::fs::read(input)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()))
    }

    #[test]
    fn load_inputs_from_bytes_builds_rects_and_anchor_triplet_overlays() {
        let data = VisualizationData::new();
        let report = sample_report();
        let guesses = sample_guesses();
        let font_runs = sample_font_runs();

        let inputs = data
            .load_inputs_from_bytes(
                &sample_pdf_bytes(),
                &report,
                Some(&guesses),
                Some(&font_runs),
            )
            .expect("visualization inputs should load");

        assert_eq!(inputs.rects.len(), 1_usize);
        assert_eq!(inputs.overlays.len(), 1_usize);
        let text = inputs
            .overlays
            .iter()
            .map(|overlay| overlay.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(text, vec!["including SARAH KELLEN and"]);
    }

    #[test]
    fn render_visualized_pdf_from_bytes_produces_pdf_output() {
        let data = VisualizationData::new();
        let report = sample_report();
        let guesses = sample_guesses();
        let font_runs = sample_font_runs();

        let out = data
            .render_visualized_pdf_from_bytes(
                &sample_pdf_bytes(),
                &report,
                Some(&guesses),
                Some(&font_runs),
                VisualizerConfig::default(),
            )
            .expect("render should succeed");
        assert!(!out.is_empty());
        assert!(out.starts_with(b"%PDF"));
    }

    #[test]
    fn load_inputs_from_bytes_without_guess_data_has_no_overlays() {
        let data = VisualizationData::new();
        let report = sample_report();

        let inputs = data
            .load_inputs_from_bytes(&sample_pdf_bytes(), &report, None, None)
            .expect("visualization inputs should load");
        assert_eq!(inputs.rects.len(), 1_usize);
        assert!(inputs.overlays.is_empty());
    }

    #[test]
    fn anchor_pair_overlay_uses_joined_text_with_anchor_typography() {
        let data = VisualizationData::new();
        let report = sample_anchor_pair_report();
        let guesses = sample_guesses_with_candidate("EDWARD JAY EPSTEIN EDWARD JAY EPSTEIN");
        let font_runs = sample_font_runs_with_anchor_row();
        let pdf_bytes = sample_pdf_bytes();

        let inputs = data
            .load_inputs_from_bytes(&pdf_bytes, &report, Some(&guesses), Some(&font_runs))
            .expect("visualization inputs should load");
        assert_eq!(inputs.overlays.len(), 1_usize);
        let overlay = &inputs.overlays[0];
        assert_eq!(
            overlay.text,
            "including EDWARD JAY EPSTEIN EDWARD JAY EPSTEIN and"
        );

        let width_map = build_font_width_map(&pdf_bytes).expect("width map should load");
        let assets =
            std::collections::BTreeMap::<String, crate::types::file_types::FontAsset>::new();
        let joined_width = text_width_pt(
            overlay.page_index,
            &overlay.font_key,
            overlay.font_size_pt,
            overlay.h_scale_pct,
            &overlay.text,
            &assets,
            &width_map,
        );
        assert!(
            joined_width > 0.0_f32,
            "joined anchor overlay width should be measurable"
        );
        assert!(
            (overlay.h_scale_pct - 100.0_f32).abs() <= 0.001_f32,
            "joined anchor overlay should preserve anchor h-scale"
        );
    }

    #[test]
    fn anchor_pair_overlay_prefers_left_anchor_run_origin_and_style() {
        let data = VisualizationData::new();
        let report = sample_anchor_pair_report();
        let mut guesses = sample_guesses();
        guesses.guesses[0].context.font_name = Some("Wrong-Font".to_owned());
        guesses.guesses[0].context.font_size_pt = Some(17.0_f32);
        guesses.guesses[0].context.h_scale_pct = Some(82.0_f32);
        let font_runs = sample_font_runs_with_anchor_row();

        let inputs = data
            .load_inputs_from_bytes(
                &sample_pdf_bytes(),
                &report,
                Some(&guesses),
                Some(&font_runs),
            )
            .expect("visualization inputs should load");

        assert_eq!(inputs.overlays.len(), 1_usize);
        let overlay = &inputs.overlays[0];
        assert_eq!(overlay.text, "including SARAH KELLEN and");
        assert!((overlay.x - 104.0_f32).abs() <= 0.001_f32);
        assert!((overlay.y - 219.0_f32).abs() <= 0.001_f32);
        assert_eq!(overlay.font_key, "F_anchor");
        assert!((overlay.font_size_pt - 11.0_f32).abs() <= 0.001_f32);
        assert!((overlay.h_scale_pct - 96.0_f32).abs() <= 0.001_f32);
    }

    #[test]
    fn anchor_pair_overlay_preserves_multiline_guess_text() {
        let data = VisualizationData::new();
        let report = sample_anchor_pair_report();
        let guesses = sample_guesses_with_candidate("NADIA\nMARCINKOVA");
        let font_runs = sample_font_runs_with_anchor_row();

        let inputs = data
            .load_inputs_from_bytes(
                &sample_pdf_bytes(),
                &report,
                Some(&guesses),
                Some(&font_runs),
            )
            .expect("visualization inputs should load");
        assert_eq!(inputs.overlays.len(), 2_usize);
        let first = &inputs.overlays[0];
        let second = &inputs.overlays[1];
        assert_eq!(first.text, "including NADIA");
        assert_eq!(second.text, "MARCINKOVA");
        assert!(
            second.y < first.y,
            "wrapped line should be emitted below first line"
        );
    }

    #[test]
    fn anchor_pair_overlay_uses_selected_only_for_non_joinable_context() {
        let data = VisualizationData::new();
        let mut report = sample_anchor_pair_report();
        report.redactions[0].underlying_text[0].text = "(pilot),".to_owned();
        report.redactions[0].underlying_text[1].text = "(pilot),".to_owned();
        let guesses = sample_guesses();
        let font_runs = sample_font_runs_with_anchor_row();

        let inputs = data
            .load_inputs_from_bytes(
                &sample_pdf_bytes(),
                &report,
                Some(&guesses),
                Some(&font_runs),
            )
            .expect("visualization inputs should load");

        assert_eq!(inputs.overlays.len(), 1_usize);
        let overlay = &inputs.overlays[0];
        assert_eq!(overlay.text, "(pilot), SARAH KELLEN");
        assert!((overlay.x - 104.0_f32).abs() <= 0.001_f32);
        assert!(overlay.bbox.x0 <= report.redactions[0].underlying_text[0].bbox.x0);
    }

    #[test]
    fn raster_overlay_preserves_multiline_guess_text() {
        let data = VisualizationData::new();
        let report = sample_report();
        let mut guesses = sample_guesses_with_candidate("NADIA\nMARCINKOVA");
        guesses.guesses[0].context.anchor_mode = None;
        guesses.anchors.clear();
        let font_runs = sample_font_runs();

        let inputs = data
            .load_inputs_from_bytes(
                &sample_pdf_bytes(),
                &report,
                Some(&guesses),
                Some(&font_runs),
            )
            .expect("visualization inputs should load");

        assert_eq!(inputs.overlays.len(), 1_usize);
        let overlay = &inputs.overlays[0];
        assert_eq!(overlay.text, "NADIA\nMARCINKOVA");
        assert!(
            overlay.bbox.y0 < report.redactions[0].bbox.y0,
            "multiline raster overlay bbox should include wrapped lines"
        );
        assert!((overlay.bbox.x0 - report.redactions[0].bbox.x0).abs() <= 0.001_f32);
        assert!((overlay.bbox.x1 - report.redactions[0].bbox.x1).abs() <= 0.001_f32);
    }

    #[test]
    fn anchor_pair_multi_redaction_rows_use_leftmost_line_anchor_prefix() {
        let data = VisualizationData::new();
        let report = sample_multi_anchor_pair_report();
        let guesses = sample_multi_anchor_pair_guesses();
        let font_runs = sample_font_runs();

        let inputs = data
            .load_inputs_from_bytes(
                &sample_pdf_bytes(),
                &report,
                Some(&guesses),
                Some(&font_runs),
            )
            .expect("visualization inputs should load");

        assert_eq!(inputs.overlays.len(), 2_usize);
        assert_eq!(inputs.overlays[0].text, "including SARAH KELLEN");
        assert_eq!(inputs.overlays[1].text, "including ADRIANA MUCINSKA");
        assert!((inputs.overlays[0].x - 104.0_f32).abs() <= 0.001_f32);
        assert!((inputs.overlays[1].x - 150.0_f32).abs() <= 0.001_f32);
    }
}

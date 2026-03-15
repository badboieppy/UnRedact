use std::collections::{BTreeMap, BTreeSet};

use crate::data::fonts_data::FontsData;
use crate::data::helpers::character_measurement::measure_text;
use crate::data::helpers::normalized_measurement::{
    normalize_font_size_pt, normalize_h_scale_pct, normalize_spacing_pt, normalized_font_name,
    width_profile_from_run,
};
use crate::data::helpers::text_runs::{join_adjacent_run_text, normalize_transport_text};
use crate::data::page_boxes_data::build_page_boxes;
use crate::data::types::redaction_evidence_types::{
    AnchorMode, AnchorSet, AnchorSide, CandidateWidthModel, CollectRedactionEvidenceRequest,
    GuessGeometry, MeasurementFont, NeighborFacts, NeighborRef, RedactionEvidenceDiagnostic,
    RedactionEvidenceRow, RedactionEvidenceSet, TrustedRedaction,
};
use crate::dependency::pdf_font_run_types::PdfFontTextRun;
use crate::dependency::pdf_font_truth_accessor::{FontWidthSource, PdfFontTruthCatalog};
use crate::types::diagnostic_types::DiagnosticValue;
use crate::types::redaction_types::{Rect, RedactionOccurrence};

const MIN_REDACTION_EDGE_PT: f32 = 1.0_f32;
const MIN_REDACTION_AREA_PT2: f32 = 4.0_f32;
const MAX_PAGE_COVERAGE_RATIO: f32 = 0.90_f32;
const SAME_LINE_BASELINE_TOLERANCE_PT: f32 = 3.0_f32;
const ROW_EPSILON_MIN_PT: f64 = 3.5_f64;
const ROW_EPSILON_MAX_PT: f64 = 8.0_f64;

#[derive(Clone)]
struct LineBucket<'a> {
    line_id: String,
    baseline_y1: f32,
    y0: f32,
    y1: f32,
    runs: Vec<&'a PdfFontTextRun>,
}

struct ResolvedAnchorMeasurement {
    mode: AnchorMode,
    left_anchor: Option<AnchorSide>,
    right_anchor: Option<AnchorSide>,
    measurement_model: CandidateWidthModel,
}

struct EvidenceBuildError {
    reason_code: &'static str,
    message: String,
    metrics: BTreeMap<String, DiagnosticValue>,
}

struct DiagnosticLocation {
    row_id: Option<String>,
    redaction_id: Option<String>,
    page_index: u32,
    bbox: Rect,
}

pub fn collect_redaction_evidence(
    req: CollectRedactionEvidenceRequest<'_>,
) -> Result<RedactionEvidenceSet, String> {
    let page_boxes = build_page_boxes(req.pdf_bytes)?;
    let fonts_data = FontsData::new();
    let font_runs = fonts_data.load_font_runs_from_bytes(req.input_name, req.pdf_bytes)?;
    let font_truth = fonts_data.load_font_truth_from_bytes(req.input_name, req.pdf_bytes)?;
    let runs_by_page = build_runs_by_page(&font_runs.pdf_report.runs);
    let line_buckets_by_page = build_line_buckets_by_page(&runs_by_page);

    let mut rows = Vec::<RedactionEvidenceRow>::new();
    let mut diagnostics = Vec::<RedactionEvidenceDiagnostic>::new();
    let mut seen_redactions = BTreeSet::<String>::new();

    for (index, redaction) in req.redactions.redactions.iter().enumerate() {
        let redaction_id = format!("page{}_redaction{index:03}", redaction.page_index);
        let key = normalized_redaction_key(redaction);
        if !seen_redactions.insert(key) {
            if req.collect_diagnostics {
                diagnostics.push(build_diagnostic(
                    DiagnosticLocation {
                        row_id: None,
                        redaction_id: Some(redaction_id),
                        page_index: redaction.page_index,
                        bbox: redaction.bbox,
                    },
                    "redaction_evidence",
                    "redaction_duplicate",
                    "duplicate redaction geometry",
                    BTreeMap::new(),
                ));
            }
            continue;
        }
        let Some(page_box) = page_boxes.get(&redaction.page_index).copied() else {
            if req.collect_diagnostics {
                diagnostics.push(build_diagnostic(
                    DiagnosticLocation {
                        row_id: None,
                        redaction_id: Some(redaction_id),
                        page_index: redaction.page_index,
                        bbox: redaction.bbox,
                    },
                    "redaction_evidence",
                    "redaction_out_of_page_bounds",
                    "page box missing for redaction page",
                    BTreeMap::new(),
                ));
            }
            continue;
        };
        if let Err(reason_code) = validate_redaction(redaction, page_box) {
            if req.collect_diagnostics {
                diagnostics.push(build_diagnostic(
                    DiagnosticLocation {
                        row_id: None,
                        redaction_id: Some(redaction_id),
                        page_index: redaction.page_index,
                        bbox: redaction.bbox,
                    },
                    "redaction_evidence",
                    reason_code,
                    "redaction failed trusted geometry checks",
                    BTreeMap::new(),
                ));
            }
            continue;
        }
        let Some(line_bucket) = select_line_bucket(
            redaction,
            line_buckets_by_page
                .get(&redaction.page_index)
                .map(Vec::as_slice),
        ) else {
            if req.collect_diagnostics {
                diagnostics.push(build_diagnostic(
                    DiagnosticLocation {
                        row_id: None,
                        redaction_id: Some(redaction_id),
                        page_index: redaction.page_index,
                        bbox: redaction.bbox,
                    },
                    "redaction_evidence",
                    "line_bucket_not_found",
                    "no same-line visible text bucket for redaction",
                    BTreeMap::new(),
                ));
            }
            continue;
        };
        let row_id = format!("page{}_row{index}", redaction.page_index);
        match build_row(redaction, &redaction_id, &row_id, line_bucket, &font_truth) {
            Ok(row) => {
                if req.collect_diagnostics {
                    diagnostics.extend(build_backend_diagnostics(&row));
                }
                rows.push(row);
            }
            Err(error) => {
                if req.collect_diagnostics {
                    diagnostics.push(build_diagnostic(
                        DiagnosticLocation {
                            row_id: Some(row_id),
                            redaction_id: Some(redaction_id),
                            page_index: redaction.page_index,
                            bbox: redaction.bbox,
                        },
                        "redaction_evidence",
                        error.reason_code,
                        &error.message,
                        error.metrics,
                    ));
                }
            }
        }
    }

    populate_neighbor_facts(&mut rows);
    diagnostics.sort_by(|left, right| {
        left.page_index
            .cmp(&right.page_index)
            .then_with(|| left.row_id.cmp(&right.row_id))
            .then_with(|| left.reason_code.cmp(&right.reason_code))
            .then_with(|| left.stage.cmp(&right.stage))
    });

    Ok(RedactionEvidenceSet {
        input: req.input_name.to_owned(),
        rows,
        diagnostics,
    })
}

fn build_runs_by_page(runs: &[PdfFontTextRun]) -> BTreeMap<u32, Vec<&PdfFontTextRun>> {
    let mut out = BTreeMap::<u32, Vec<&PdfFontTextRun>>::new();
    for run in runs {
        out.entry(run.page_index).or_default().push(run);
    }
    for page_runs in out.values_mut() {
        page_runs.sort_by(|left, right| {
            left.bbox
                .y1
                .partial_cmp(&right.bbox.y1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.bbox
                        .x0
                        .partial_cmp(&right.bbox.x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.text.cmp(&right.text))
        });
    }
    out
}

fn build_line_buckets_by_page<'a>(
    runs_by_page: &BTreeMap<u32, Vec<&'a PdfFontTextRun>>,
) -> BTreeMap<u32, Vec<LineBucket<'a>>> {
    let mut out = BTreeMap::<u32, Vec<LineBucket<'a>>>::new();
    for (page_index, runs) in runs_by_page {
        let mut lines = Vec::<LineBucket<'a>>::new();
        for run in runs {
            if run.text.trim().is_empty() {
                continue;
            }
            if let Some(existing) = lines.iter_mut().find(|line| {
                (run.bbox.y1 - line.baseline_y1).abs() <= SAME_LINE_BASELINE_TOLERANCE_PT
            }) {
                existing.y0 = existing.y0.min(run.bbox.y0);
                existing.y1 = existing.y1.max(run.bbox.y1);
                existing.runs.push(*run);
            } else {
                let line_id = format!("page{page_index}_line{:03}", lines.len());
                lines.push(LineBucket {
                    line_id,
                    baseline_y1: run.bbox.y1,
                    y0: run.bbox.y0,
                    y1: run.bbox.y1,
                    runs: vec![*run],
                });
            }
        }
        for line in &mut lines {
            line.runs.sort_by(|left, right| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.text.cmp(&right.text))
            });
        }
        out.insert(*page_index, lines);
    }
    out
}

fn normalized_redaction_key(redaction: &RedactionOccurrence) -> String {
    format!(
        "{}:{:.4}:{:.4}:{:.4}:{:.4}:{:?}",
        redaction.page_index,
        redaction.bbox.x0,
        redaction.bbox.y0,
        redaction.bbox.x1,
        redaction.bbox.y1,
        redaction.kind
    )
}

fn validate_redaction(redaction: &RedactionOccurrence, page_box: Rect) -> Result<(), &'static str> {
    let bbox = Rect::new(
        redaction.bbox.x0,
        redaction.bbox.y0,
        redaction.bbox.x1,
        redaction.bbox.y1,
    );
    if !bbox.x0.is_finite() || !bbox.y0.is_finite() || !bbox.x1.is_finite() || !bbox.y1.is_finite()
    {
        return Err("invalid_redaction_geometry");
    }
    if bbox.width().abs() < MIN_REDACTION_EDGE_PT || bbox.height().abs() < MIN_REDACTION_EDGE_PT {
        return Err("invalid_redaction_geometry");
    }
    if bbox.area() < MIN_REDACTION_AREA_PT2 {
        return Err("invalid_redaction_geometry");
    }
    if bbox.x0 < page_box.x0
        || bbox.y0 < page_box.y0
        || bbox.x1 > page_box.x1
        || bbox.y1 > page_box.y1
    {
        return Err("redaction_out_of_page_bounds");
    }
    let page_area = page_box.area().max(0.0001_f32);
    if bbox.area() / page_area >= MAX_PAGE_COVERAGE_RATIO {
        return Err("redaction_out_of_page_bounds");
    }
    if !redaction.score.is_finite() {
        return Err("invalid_redaction_geometry");
    }
    Ok(())
}

fn select_line_bucket<'a>(
    redaction: &RedactionOccurrence,
    line_buckets: Option<&'a [LineBucket<'a>]>,
) -> Option<&'a LineBucket<'a>> {
    let buckets = line_buckets?;
    buckets
        .iter()
        .filter(|line| {
            vertical_overlap_pt(line.y0, line.y1, redaction.bbox.y0, redaction.bbox.y1) > 0.0_f32
                || (line.baseline_y1 - redaction.bbox.y1).abs() <= SAME_LINE_BASELINE_TOLERANCE_PT
        })
        .min_by(|left, right| {
            let left_overlap =
                vertical_overlap_pt(left.y0, left.y1, redaction.bbox.y0, redaction.bbox.y1);
            let right_overlap =
                vertical_overlap_pt(right.y0, right.y1, redaction.bbox.y0, redaction.bbox.y1);
            let left_baseline_delta = (left.baseline_y1 - redaction.bbox.y1).abs();
            let right_baseline_delta = (right.baseline_y1 - redaction.bbox.y1).abs();
            right_overlap
                .partial_cmp(&left_overlap)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left_baseline_delta
                        .partial_cmp(&right_baseline_delta)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.line_id.cmp(&right.line_id))
        })
}

fn vertical_overlap_pt(y0_a: f32, y1_a: f32, y0_b: f32, y1_b: f32) -> f32 {
    (y1_a.min(y1_b) - y0_a.max(y0_b)).max(0.0_f32)
}

fn build_row(
    redaction: &RedactionOccurrence,
    redaction_id: &str,
    row_id: &str,
    line_bucket: &LineBucket<'_>,
    font_truth: &PdfFontTruthCatalog,
) -> Result<RedactionEvidenceRow, EvidenceBuildError> {
    let left_run = select_anchor_run(redaction, &line_bucket.runs, true);
    let right_run = select_anchor_run(redaction, &line_bucket.runs, false);
    if left_run.is_none() && right_run.is_none() {
        return Err(build_error(
            "same_line_anchor_missing",
            "no same-line visible anchor spans available",
        ));
    }

    let left_anchor = left_run.map(|run| build_anchor_side(row_id, run, &line_bucket.runs, true));
    let right_anchor =
        right_run.map(|run| build_anchor_side(row_id, run, &line_bucket.runs, false));
    let resolved =
        resolve_anchor_measurement(left_run, right_run, left_anchor, right_anchor, font_truth)?;
    let ResolvedAnchorMeasurement {
        mode,
        left_anchor,
        right_anchor,
        measurement_model,
    } = resolved;
    let boundary_space_width_pt = boundary_space_width_pt(&measurement_model);
    let left_anchor_width_pt = left_anchor
        .as_ref()
        .map(|anchor| measure_text(&measurement_model, &anchor.text))
        .transpose()
        .ok()
        .flatten();
    let (line_bias_pt, tolerance_pt) =
        estimate_row_geometry(line_bucket, left_run.or(right_run), &measurement_model);

    let usable_left_edge_x_pt = left_anchor.as_ref().and_then(|anchor| {
        left_anchor_width_pt
            .map(|anchor_width| anchor.text_edge_x_pt + anchor_width + boundary_space_width_pt)
    });
    let usable_right_edge_x_pt = right_anchor
        .as_ref()
        .map(|anchor| anchor.text_edge_x_pt - boundary_space_width_pt);
    let target_width_pt = match (usable_left_edge_x_pt, usable_right_edge_x_pt) {
        (Some(left_edge), Some(right_edge)) if right_edge > left_edge => right_edge - left_edge,
        _ => redaction.bbox.width().abs(),
    };

    Ok(RedactionEvidenceRow {
        row_id: row_id.to_owned(),
        page_index: redaction.page_index,
        redaction: TrustedRedaction {
            redaction_id: redaction_id.to_owned(),
            page_index: redaction.page_index,
            bbox: redaction.bbox,
            kind: redaction.kind.clone(),
            score: redaction.score,
        },
        anchor_set: AnchorSet {
            mode,
            left: left_anchor,
            right: right_anchor,
            geometry: GuessGeometry {
                redaction_left_x_pt: redaction.bbox.x0,
                redaction_right_x_pt: redaction.bbox.x1,
                redaction_width_pt: redaction.bbox.width().abs(),
                usable_left_edge_x_pt,
                usable_right_edge_x_pt,
                target_width_pt,
                line_bias_pt,
                tolerance_pt,
            },
        },
        font: MeasurementFont {
            font_key: measurement_model.font_key.clone(),
            font_name: measurement_model.font_name.clone(),
            base_font: measurement_model.base_font.clone(),
            font_size_pt: measurement_model.font_size_pt,
            h_scale_pct: measurement_model.h_scale_pct,
            char_spacing_pt: measurement_model.char_spacing_pt,
            word_spacing_pt: measurement_model.word_spacing_pt,
            width_source: Some(measurement_model.width_source.as_str().to_owned()),
            encoding_source: Some(measurement_model.encoding_source.as_str().to_owned()),
        },
        neighbor_facts: NeighborFacts {
            line_id: line_bucket.line_id.clone(),
            ..NeighborFacts::default()
        },
        measurement_model,
    })
}

fn resolve_anchor_measurement(
    left_run: Option<&PdfFontTextRun>,
    right_run: Option<&PdfFontTextRun>,
    left_anchor: Option<AnchorSide>,
    right_anchor: Option<AnchorSide>,
    font_truth: &PdfFontTruthCatalog,
) -> Result<ResolvedAnchorMeasurement, EvidenceBuildError> {
    let mut last_error = None::<EvidenceBuildError>;

    if let (Some(left_run), Some(right_run), Some(left_anchor), Some(right_anchor)) = (
        left_run,
        right_run,
        left_anchor.clone(),
        right_anchor.clone(),
    ) {
        match build_measurement_model(Some(left_run), Some(right_run), font_truth) {
            Ok(measurement_model) => {
                return Ok(ResolvedAnchorMeasurement {
                    mode: AnchorMode::TwoSided,
                    left_anchor: Some(left_anchor),
                    right_anchor: Some(right_anchor),
                    measurement_model,
                });
            }
            Err(error) => last_error = Some(error),
        }
    }

    if let (Some(left_run), Some(left_anchor)) = (left_run, left_anchor) {
        match build_measurement_model(Some(left_run), None, font_truth) {
            Ok(measurement_model) => {
                return Ok(ResolvedAnchorMeasurement {
                    mode: AnchorMode::LeftOnly,
                    left_anchor: Some(left_anchor),
                    right_anchor: None,
                    measurement_model,
                });
            }
            Err(error) => last_error = Some(error),
        }
    }

    if let (Some(right_run), Some(right_anchor)) = (right_run, right_anchor) {
        match build_measurement_model(None, Some(right_run), font_truth) {
            Ok(measurement_model) => {
                return Ok(ResolvedAnchorMeasurement {
                    mode: AnchorMode::RightOnly,
                    left_anchor: None,
                    right_anchor: Some(right_anchor),
                    measurement_model,
                });
            }
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        build_error(
            "character_model_unbuildable",
            "failed to resolve a same-line measurement model",
        )
    }))
}

fn select_anchor_run<'a>(
    redaction: &RedactionOccurrence,
    line_runs: &[&'a PdfFontTextRun],
    left_side: bool,
) -> Option<&'a PdfFontTextRun> {
    line_runs
        .iter()
        .copied()
        .filter(|run| {
            let trimmed = run.text.trim();
            if trimmed.is_empty() {
                return false;
            }
            if left_side {
                run.bbox.x1 <= redaction.bbox.x0
            } else {
                run.bbox.x0 >= redaction.bbox.x1
            }
        })
        .min_by(|left, right| {
            let left_visibility = visibility_rank(left);
            let right_visibility = visibility_rank(right);
            let left_gap = if left_side {
                (redaction.bbox.x0 - left.bbox.x1).abs()
            } else {
                (left.bbox.x0 - redaction.bbox.x1).abs()
            };
            let right_gap = if left_side {
                (redaction.bbox.x0 - right.bbox.x1).abs()
            } else {
                (right.bbox.x0 - redaction.bbox.x1).abs()
            };
            left_visibility
                .cmp(&right_visibility)
                .then_with(|| {
                    left_gap
                        .partial_cmp(&right_gap)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    let left_width = (left.bbox.x1 - left.bbox.x0).abs();
                    let right_width = (right.bbox.x1 - right.bbox.x0).abs();
                    left_width
                        .partial_cmp(&right_width)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    let left_id = format!("{:.4}:{:.4}:{}", left.bbox.x0, left.bbox.y0, left.text);
                    let right_id =
                        format!("{:.4}:{:.4}:{}", right.bbox.x0, right.bbox.y0, right.text);
                    left_id.cmp(&right_id)
                })
        })
}

fn build_anchor_side(
    row_id: &str,
    run: &PdfFontTextRun,
    line_runs: &[&PdfFontTextRun],
    left_side: bool,
) -> AnchorSide {
    let (text, x) = enrich_anchor_text_and_edge(run, line_runs, left_side);
    AnchorSide {
        anchor_id: if left_side {
            format!("{row_id}_left")
        } else {
            format!("{row_id}_right")
        },
        text,
        bbox: Rect::new(run.bbox.x0, run.bbox.y0, run.bbox.x1, run.bbox.y1),
        text_edge_x_pt: x as f32,
    }
}

fn enrich_anchor_text_and_edge(
    run: &PdfFontTextRun,
    line_runs: &[&PdfFontTextRun],
    left_side: bool,
) -> (String, f64) {
    let mut text = normalize_transport_text(&run.text);
    let mut x = run.bbox.x0 as f64;
    let Some(run_index) = line_runs
        .iter()
        .position(|candidate| std::ptr::eq(*candidate, run))
    else {
        return (text, x);
    };
    if left_side {
        let mut cursor = run_index;
        while cursor > 0 {
            let previous = line_runs[cursor - 1];
            let current = line_runs[cursor];
            if !runs_are_neighbors(previous, current) {
                break;
            }
            text = join_adjacent_run_text(
                &previous.text,
                &text,
                (current.bbox.x0 - previous.bbox.x1).max(0.0_f32) as f64,
            );
            x = previous.bbox.x0 as f64;
            cursor -= 1;
        }
        return (text, x);
    }
    let mut cursor = run_index;
    while (cursor + 1) < line_runs.len() {
        let current = line_runs[cursor];
        let next = line_runs[cursor + 1];
        if !runs_are_neighbors(current, next) {
            break;
        }
        text = join_adjacent_run_text(
            &text,
            &next.text,
            (next.bbox.x0 - current.bbox.x1).max(0.0_f32) as f64,
        );
        cursor += 1;
    }
    (text, x)
}

fn runs_are_neighbors(left: &PdfFontTextRun, right: &PdfFontTextRun) -> bool {
    let left_identity = width_profile_from_run(left);
    let right_identity = width_profile_from_run(right);
    left.page_index == right.page_index
        && left_identity == right_identity
        && (left.bbox.y1 - right.bbox.y1).abs() <= SAME_LINE_BASELINE_TOLERANCE_PT
        && (right.bbox.x0 - left.bbox.x1) >= -0.5_f32
        && (right.bbox.x0 - left.bbox.x1) <= 18.0_f32
}

fn build_measurement_model(
    left_run: Option<&PdfFontTextRun>,
    right_run: Option<&PdfFontTextRun>,
    font_truth: &PdfFontTruthCatalog,
) -> Result<CandidateWidthModel, EvidenceBuildError> {
    let seed_run = left_run.or(right_run).ok_or_else(|| {
        build_error(
            "same_line_anchor_missing",
            "no anchor run available for measurement model",
        )
    })?;
    let seed_profile = width_profile_from_run(seed_run);
    if let Some(other) = right_run.or(left_run) {
        if width_profile_from_run(other) != seed_profile {
            let mut metrics = width_profile_metrics("left_or_seed", seed_run);
            extend_metrics(&mut metrics, width_profile_metrics("right_or_other", other));
            return Err(build_error_with_metrics(
                "anchor_measurement_mismatch",
                "anchor sides do not share the same measurement model",
                metrics,
            ));
        }
    }
    let font_name = normalized_font_name(&seed_run.font_name);
    if font_name.is_empty() {
        return Err(build_error_with_metrics(
            "character_model_unbuildable",
            "anchor font name is empty",
            width_profile_metrics("seed", seed_run),
        ));
    }
    let resource_key = crate::data::types::redaction_evidence_types::MeasurementFontKey {
        page_index: seed_run.page_index,
        font_key: seed_run.font_key.clone(),
    };
    let backend = font_truth.resources.get(
        &crate::dependency::pdf_font_truth_accessor::FontResourceKey {
            page_index: resource_key.page_index,
            font_key: resource_key.font_key.clone(),
        },
    );
    Ok(CandidateWidthModel {
        resource_key,
        font_key: seed_run.font_key.clone(),
        font_name,
        base_font: backend.and_then(|entry| entry.base_font.clone()),
        subtype: backend.and_then(|entry| entry.subtype.clone()),
        font_size_pt: normalize_font_size_pt(seed_run.font_size_pt),
        h_scale_pct: normalize_h_scale_pct(seed_run.h_scale_pct),
        char_spacing_pt: normalize_spacing_pt(seed_run.width_metrics.char_spacing_pt),
        word_spacing_pt: normalize_spacing_pt(seed_run.width_metrics.word_spacing_pt),
        width_source: backend
            .map(|entry| match entry.width_source {
                FontWidthSource::PdfWidthTable => {
                    crate::data::types::redaction_evidence_types::MeasurementWidthSource::PdfWidthTable
                }
                FontWidthSource::Standard14Font => {
                    crate::data::types::redaction_evidence_types::MeasurementWidthSource::Standard14Font
                }
                FontWidthSource::None => {
                    crate::data::types::redaction_evidence_types::MeasurementWidthSource::None
                }
            })
            .unwrap_or_default(),
        encoding_source: backend
            .map(|entry| match entry.encoding_source {
                crate::dependency::pdf_font_truth_accessor::FontUnicodeSource::ToUnicode => {
                    crate::data::types::redaction_evidence_types::MeasurementEncodingSource::ToUnicode
                }
                crate::dependency::pdf_font_truth_accessor::FontUnicodeSource::EncodingDictionary => {
                    crate::data::types::redaction_evidence_types::MeasurementEncodingSource::EncodingDictionary
                }
                crate::dependency::pdf_font_truth_accessor::FontUnicodeSource::NamedEncoding => {
                    crate::data::types::redaction_evidence_types::MeasurementEncodingSource::NamedEncoding
                }
                crate::dependency::pdf_font_truth_accessor::FontUnicodeSource::StandardDefaultEncoding => {
                    crate::data::types::redaction_evidence_types::MeasurementEncodingSource::StandardDefaultEncoding
                }
                crate::dependency::pdf_font_truth_accessor::FontUnicodeSource::None => {
                    crate::data::types::redaction_evidence_types::MeasurementEncodingSource::None
                }
            })
            .unwrap_or_default(),
        has_to_unicode: backend.map(|entry| entry.has_to_unicode).unwrap_or(false),
        has_encoding_dictionary: backend
            .map(|entry| entry.has_encoding_dictionary)
            .unwrap_or(false),
        has_named_encoding: backend.map(|entry| entry.has_named_encoding).unwrap_or(false),
        has_explicit_widths: backend.map(|entry| entry.has_explicit_widths).unwrap_or(false),
        unicode_to_codes: backend
            .map(|entry| entry.unicode_to_codes.clone())
            .unwrap_or_default(),
        code_to_width_units: backend
            .map(|entry| entry.code_to_width_units.clone())
            .unwrap_or_default(),
    })
}

fn same_width_profile(run: &PdfFontTextRun, seed_run: &PdfFontTextRun) -> bool {
    width_profile_from_run(run) == width_profile_from_run(seed_run)
}

fn estimate_row_geometry(
    line_bucket: &LineBucket<'_>,
    seed_run: Option<&PdfFontTextRun>,
    model: &CandidateWidthModel,
) -> (f32, f32) {
    let Some(seed_run) = seed_run else {
        return (0.0_f32, ROW_EPSILON_MIN_PT as f32);
    };
    let mut row = line_bucket
        .runs
        .iter()
        .copied()
        .filter(|run| {
            same_width_profile(run, seed_run)
                && (run.bbox.y1 - seed_run.bbox.y1).abs() <= SAME_LINE_BASELINE_TOLERANCE_PT
        })
        .collect::<Vec<_>>();
    row.sort_by(|left, right| {
        left.bbox
            .x0
            .partial_cmp(&right.bbox.x0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.text.cmp(&right.text))
    });
    let mut residuals = Vec::<f64>::new();
    for window in row.windows(2) {
        let current = window[0];
        let next = window[1];
        let Ok(current_width) = measure_text(model, current.text.trim()) else {
            continue;
        };
        let predicted_next =
            current.bbox.x0 as f64 + current_width as f64 + boundary_space_width_pt(model) as f64;
        let residual = next.bbox.x0 as f64 - predicted_next;
        if residual.is_finite() {
            residuals.push(residual);
        }
    }
    residuals.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    if residuals.is_empty() {
        return (0.0_f32, ROW_EPSILON_MIN_PT as f32);
    }
    let median_index = ((residuals.len() as f64) * 0.5_f64).floor() as usize;
    let bias = residuals[median_index.min(residuals.len().saturating_sub(1))];
    let mut centered = residuals
        .iter()
        .map(|value| (value - bias).abs())
        .collect::<Vec<_>>();
    centered.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let epsilon = if centered.is_empty() {
        ROW_EPSILON_MIN_PT
    } else {
        let idx = ((centered.len() as f64) * 0.75_f64).floor() as usize;
        centered[idx.min(centered.len().saturating_sub(1))]
    };
    (
        bias as f32,
        epsilon.clamp(ROW_EPSILON_MIN_PT, ROW_EPSILON_MAX_PT) as f32,
    )
}

fn visibility_rank(run: &PdfFontTextRun) -> i32 {
    if run.width_metrics.render_mode == 3 {
        1
    } else {
        0
    }
}

fn build_error(reason_code: &'static str, message: &str) -> EvidenceBuildError {
    EvidenceBuildError {
        reason_code,
        message: message.to_owned(),
        metrics: BTreeMap::new(),
    }
}

fn build_error_with_metrics(
    reason_code: &'static str,
    message: &str,
    metrics: BTreeMap<String, DiagnosticValue>,
) -> EvidenceBuildError {
    EvidenceBuildError {
        reason_code,
        message: message.to_owned(),
        metrics,
    }
}

fn width_profile_metrics(prefix: &str, run: &PdfFontTextRun) -> BTreeMap<String, DiagnosticValue> {
    let mut metrics = BTreeMap::new();
    metrics.insert(
        format!("{prefix}_font_name"),
        DiagnosticValue::Text(normalized_font_name(&run.font_name)),
    );
    metrics.insert(
        format!("{prefix}_font_size_pt"),
        DiagnosticValue::Float(normalize_font_size_pt(run.font_size_pt) as f64),
    );
    metrics.insert(
        format!("{prefix}_h_scale_pct"),
        DiagnosticValue::Float(normalize_h_scale_pct(run.h_scale_pct) as f64),
    );
    metrics.insert(
        format!("{prefix}_char_spacing_pt"),
        DiagnosticValue::Float(normalize_spacing_pt(run.width_metrics.char_spacing_pt) as f64),
    );
    metrics.insert(
        format!("{prefix}_word_spacing_pt"),
        DiagnosticValue::Float(normalize_spacing_pt(run.width_metrics.word_spacing_pt) as f64),
    );
    metrics.insert(
        format!("{prefix}_render_mode"),
        DiagnosticValue::Integer(run.width_metrics.render_mode as i64),
    );
    metrics
}

fn boundary_space_width_pt(model: &CandidateWidthModel) -> f32 {
    let scale = (model.h_scale_pct / 100.0_f32).max(0.01_f32);
    let space_width_units = model
        .unicode_to_codes
        .get(&' ')
        .into_iter()
        .flat_map(|codes| codes.iter())
        .find_map(|code| model.code_to_width_units.get(code))
        .copied()
        .unwrap_or_default();
    space_width_units as f32 * (model.font_size_pt / 1000.0_f32) * scale
        + model.char_spacing_pt
        + model.word_spacing_pt
}

fn extend_metrics(
    target: &mut BTreeMap<String, DiagnosticValue>,
    source: BTreeMap<String, DiagnosticValue>,
) {
    target.extend(source);
}

fn populate_neighbor_facts(rows: &mut [RedactionEvidenceRow]) {
    let mut line_index = BTreeMap::<(u32, String), Vec<usize>>::new();
    for (index, row) in rows.iter().enumerate() {
        line_index
            .entry((row.page_index, row.neighbor_facts.line_id.clone()))
            .or_default()
            .push(index);
    }
    for ((_, _), row_indices) in line_index {
        let mut row_indices = row_indices;
        row_indices.sort_by(|left_index, right_index| {
            let left = &rows[*left_index];
            let right = &rows[*right_index];
            left.redaction
                .bbox
                .x0
                .partial_cmp(&right.redaction.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.redaction
                        .bbox
                        .x1
                        .partial_cmp(&right.redaction.bbox.x1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.row_id.cmp(&right.row_id))
        });
        let count = row_indices.len();
        for (line_order, row_index) in row_indices.iter().copied().enumerate() {
            let previous_same_line = (line_order > 0).then(|| {
                let previous = &rows[row_indices[line_order - 1]];
                NeighborRef {
                    row_id: previous.row_id.clone(),
                    redaction_id: previous.redaction.redaction_id.clone(),
                    bbox: previous.redaction.bbox,
                    gap_pt: rows[row_index].redaction.bbox.x0 - previous.redaction.bbox.x1,
                }
            });
            let next_same_line = (line_order + 1 < count).then(|| {
                let next = &rows[row_indices[line_order + 1]];
                NeighborRef {
                    row_id: next.row_id.clone(),
                    redaction_id: next.redaction.redaction_id.clone(),
                    bbox: next.redaction.bbox,
                    gap_pt: next.redaction.bbox.x0 - rows[row_index].redaction.bbox.x1,
                }
            });
            rows[row_index].neighbor_facts.line_row_count = count;
            rows[row_index].neighbor_facts.line_order = line_order;
            rows[row_index].neighbor_facts.previous_same_line = previous_same_line;
            rows[row_index].neighbor_facts.next_same_line = next_same_line;
        }
    }
}

fn build_diagnostic(
    location: DiagnosticLocation,
    stage: &str,
    reason_code: &str,
    message: &str,
    metrics: BTreeMap<String, DiagnosticValue>,
) -> RedactionEvidenceDiagnostic {
    RedactionEvidenceDiagnostic {
        row_id: location.row_id,
        redaction_id: location.redaction_id,
        page_index: location.page_index,
        bbox: location.bbox,
        stage: stage.to_owned(),
        reason_code: reason_code.to_owned(),
        message: message.to_owned(),
        metrics,
    }
}

fn build_backend_diagnostics(row: &RedactionEvidenceRow) -> Vec<RedactionEvidenceDiagnostic> {
    let location = DiagnosticLocation {
        row_id: Some(row.row_id.clone()),
        redaction_id: Some(row.redaction.redaction_id.clone()),
        page_index: row.page_index,
        bbox: row.redaction.bbox,
    };
    let mut diagnostics = Vec::with_capacity(3);

    let mut font_metrics = BTreeMap::new();
    font_metrics.insert(
        "font_key".to_owned(),
        DiagnosticValue::Text(row.font.font_key.clone()),
    );
    font_metrics.insert(
        "base_font".to_owned(),
        DiagnosticValue::Text(row.font.base_font.clone().unwrap_or_default()),
    );
    font_metrics.insert(
        "encoding_source".to_owned(),
        DiagnosticValue::Text(
            row.font
                .encoding_source
                .clone()
                .unwrap_or_else(|| "none".to_owned()),
        ),
    );
    font_metrics.insert(
        "width_source".to_owned(),
        DiagnosticValue::Text(
            row.font
                .width_source
                .clone()
                .unwrap_or_else(|| "none".to_owned()),
        ),
    );
    font_metrics.insert(
        "has_to_unicode".to_owned(),
        DiagnosticValue::Bool(row.measurement_model.has_to_unicode),
    );
    font_metrics.insert(
        "has_encoding_dictionary".to_owned(),
        DiagnosticValue::Bool(row.measurement_model.has_encoding_dictionary),
    );
    font_metrics.insert(
        "has_named_encoding".to_owned(),
        DiagnosticValue::Bool(row.measurement_model.has_named_encoding),
    );
    font_metrics.insert(
        "has_explicit_widths".to_owned(),
        DiagnosticValue::Bool(row.measurement_model.has_explicit_widths),
    );

    diagnostics.push(build_diagnostic(
        DiagnosticLocation {
            row_id: location.row_id.clone(),
            redaction_id: location.redaction_id.clone(),
            page_index: location.page_index,
            bbox: location.bbox,
        },
        "redaction_evidence",
        "font_width_backend_selected",
        "selected width backend for row font",
        font_metrics.clone(),
    ));
    diagnostics.push(build_diagnostic(
        DiagnosticLocation {
            row_id: location.row_id.clone(),
            redaction_id: location.redaction_id.clone(),
            page_index: location.page_index,
            bbox: location.bbox,
        },
        "redaction_evidence",
        "font_unicode_backend_selected",
        "selected unicode backend for row font",
        font_metrics.clone(),
    ));

    let mut row_metrics = font_metrics;
    row_metrics.insert(
        "anchor_mode".to_owned(),
        DiagnosticValue::Text(row.anchor_set.mode.as_str().to_owned()),
    );
    row_metrics.insert(
        "font_size_pt".to_owned(),
        DiagnosticValue::Float(row.font.font_size_pt as f64),
    );
    row_metrics.insert(
        "h_scale_pct".to_owned(),
        DiagnosticValue::Float(row.font.h_scale_pct as f64),
    );
    row_metrics.insert(
        "char_spacing_pt".to_owned(),
        DiagnosticValue::Float(row.font.char_spacing_pt as f64),
    );
    row_metrics.insert(
        "word_spacing_pt".to_owned(),
        DiagnosticValue::Float(row.font.word_spacing_pt as f64),
    );
    row_metrics.insert(
        "tolerance_pt".to_owned(),
        DiagnosticValue::Float(row.anchor_set.geometry.tolerance_pt as f64),
    );
    row_metrics.insert(
        "supported_unicode_count".to_owned(),
        DiagnosticValue::Integer(row.measurement_model.unicode_to_codes.len() as i64),
    );
    row_metrics.insert(
        "code_width_count".to_owned(),
        DiagnosticValue::Integer(row.measurement_model.code_to_width_units.len() as i64),
    );

    let row_reason = match (
        row.measurement_model.encoding_source,
        row.measurement_model.width_source,
    ) {
        (crate::data::types::redaction_evidence_types::MeasurementEncodingSource::None, _) => {
            "row_unicode_backend_missing"
        }
        (_, crate::data::types::redaction_evidence_types::MeasurementWidthSource::None) => {
            "row_width_backend_missing"
        }
        _ => "row_backend_ready",
    };
    let row_message = match row_reason {
        "row_unicode_backend_missing" => "row font is missing a unicode backend",
        "row_width_backend_missing" => "row font is missing a width backend",
        _ => "row font backends are ready",
    };
    diagnostics.push(build_diagnostic(
        location,
        "redaction_evidence",
        row_reason,
        row_message,
        row_metrics,
    ));

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::{runs_are_neighbors, select_anchor_run, select_line_bucket, LineBucket};
    use crate::dependency::pdf_font_run_types::{PdfFontTextRun, PdfWidthMetrics};
    use crate::types::file_types::{FontTextRun, Rect};
    use crate::types::redaction_types::{RedactionKind, RedactionOccurrence};
    use std::collections::BTreeMap;

    fn sample_run(y1: f32) -> PdfFontTextRun {
        sample_run_with_metrics(y1, 0, 0.0_f32, 0.0_f32)
    }

    fn sample_run_with_metrics(
        y1: f32,
        render_mode: u8,
        char_spacing_pt: f32,
        word_spacing_pt: f32,
    ) -> PdfFontTextRun {
        PdfFontTextRun {
            run: FontTextRun {
                page_index: 0,
                text: "anchor".to_owned(),
                bbox: Rect::new(10.0, y1 - 10.0, 20.0, y1),
                font_key: "F1".to_owned(),
                font_name: "Times-Roman".to_owned(),
                font_size_pt: 12.0,
                h_scale_pct: 100.0,
                measured_width_pt: None,
                measured_width_px: None,
                measured_dpi: None,
                char_advances_pt: vec![2.0; 6],
                char_advances_px: Vec::new(),
            },
            width_metrics: PdfWidthMetrics {
                render_mode,
                char_spacing_pt,
                word_spacing_pt,
                ..PdfWidthMetrics::default()
            },
        }
    }

    fn sample_redaction(y1: f32) -> RedactionOccurrence {
        RedactionOccurrence {
            page_index: 0,
            bbox: crate::types::redaction_types::Rect::new(30.0, y1 - 10.0, 40.0, y1),
            kind: RedactionKind::DrawnRect,
            score: 1.0,
            meta: BTreeMap::new(),
            underlying_text: Vec::new(),
        }
    }

    #[test]
    fn select_line_bucket_requires_overlap_or_baseline_proximity() {
        let run = sample_run(100.0);
        let lines = vec![LineBucket {
            line_id: "page0_line000".to_owned(),
            baseline_y1: 100.0,
            y0: 90.0,
            y1: 100.0,
            runs: vec![&run],
        }];
        assert!(select_line_bucket(&sample_redaction(114.0), Some(lines.as_slice())).is_none());
    }

    #[test]
    fn select_line_bucket_prefers_overlap_before_baseline_delta() {
        let overlap_run = sample_run(108.0);
        let baseline_run = sample_run(105.0);
        let lines = vec![
            LineBucket {
                line_id: "page0_line000".to_owned(),
                baseline_y1: baseline_run.bbox.y1,
                y0: 105.0,
                y1: 106.0,
                runs: vec![&baseline_run],
            },
            LineBucket {
                line_id: "page0_line001".to_owned(),
                baseline_y1: overlap_run.bbox.y1,
                y0: overlap_run.bbox.y0,
                y1: overlap_run.bbox.y1,
                runs: vec![&overlap_run],
            },
        ];
        let selected = select_line_bucket(&sample_redaction(104.0), Some(lines.as_slice()))
            .expect("expected admissible line");
        assert_eq!(selected.line_id, "page0_line001");
    }

    #[test]
    fn runs_are_neighbors_rejects_different_spacing_profiles() {
        let left = sample_run_with_metrics(100.0, 0, 0.0_f32, 0.0_f32);
        let mut right = sample_run_with_metrics(100.0, 0, 0.5_f32, 0.0_f32);
        right.run.bbox = Rect::new(20.5, 90.0, 30.0, 100.0);
        assert!(!runs_are_neighbors(&left, &right));
    }

    #[test]
    fn select_anchor_run_prefers_visible_text_over_invisible_text() {
        let redaction = sample_redaction(100.0);
        let invisible = sample_run_with_metrics(100.0, 3, 0.0_f32, 0.0_f32);
        let mut visible = sample_run_with_metrics(100.0, 0, 0.0_f32, 0.0_f32);
        visible.run.bbox = Rect::new(9.0, 90.0, 19.0, 100.0);
        let line_runs = vec![&invisible, &visible];

        let selected =
            select_anchor_run(&redaction, &line_runs, true).expect("expected a left-side anchor");
        assert_eq!(selected.width_metrics.render_mode, 0);
    }
}

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
    GuessGeometry, MeasurementFont, MeasurementSeedSide, NeighborFacts, NeighborRef,
    RedactionEvidenceDiagnostic, RedactionEvidenceRow, RedactionEvidenceSet, TrustedRedaction,
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

struct LineBucketCandidate<'a> {
    line_bucket: &'a LineBucket<'a>,
    vertical_overlap_pt: f32,
    baseline_delta_pt: f32,
}

struct AnchorRunCandidate<'a> {
    run: &'a PdfFontTextRun,
    visibility_rank: i32,
    gap_pt: f32,
    width_pt: f32,
}

#[derive(Clone)]
struct AnchorSpanCandidate<'a> {
    candidate_id: String,
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    run: &'a PdfFontTextRun,
    left_side: bool,
    anchor: AnchorSide,
    visibility_rank: i32,
    gap_pt: f32,
    width_pt: f32,
}

#[derive(Clone)]
struct AnchorPairCandidate<'a> {
    pair_id: String,
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    left: AnchorSpanCandidate<'a>,
    right: AnchorSpanCandidate<'a>,
}

struct BucketAnchorCandidates<'a> {
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    left_candidates: Vec<AnchorSpanCandidate<'a>>,
    right_candidates: Vec<AnchorSpanCandidate<'a>>,
}

struct RowBuildOutput {
    row: RedactionEvidenceRow,
    diagnostics: Vec<RedactionEvidenceDiagnostic>,
}

struct AnchorResolutionDecision<'a> {
    mode: AnchorMode,
    selected_line_bucket: Option<&'a LineBucket<'a>>,
    left: Option<AnchorSpanCandidate<'a>>,
    right: Option<AnchorSpanCandidate<'a>>,
    measurement_seed_side: Option<MeasurementSeedSide>,
    selection_reason: String,
}

#[derive(Clone)]
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
        let row_id = format!("page{}_row{index}", redaction.page_index);
        let key = normalized_redaction_key(redaction);
        if !seen_redactions.insert(key) {
            if req.collect_diagnostics {
                diagnostics.push(build_diagnostic(
                    DiagnosticLocation {
                        row_id: Some(row_id.clone()),
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
                        row_id: Some(row_id.clone()),
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
                        row_id: Some(row_id.clone()),
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
        let output = build_row(
            redaction,
            &redaction_id,
            &row_id,
            line_buckets_by_page
                .get(&redaction.page_index)
                .map(Vec::as_slice),
            &font_truth,
            req.collect_diagnostics,
        );
        if req.collect_diagnostics {
            diagnostics.extend(output.diagnostics);
            diagnostics.extend(build_backend_diagnostics(&output.row));
        }
        rows.push(output.row);
    }

    populate_neighbor_facts(&mut rows);
    diagnostics.sort_by(|left, right| {
        left.page_index
            .cmp(&right.page_index)
            .then_with(|| left.row_id.cmp(&right.row_id))
            .then_with(|| left.stage.cmp(&right.stage))
            .then_with(|| left.reason_code.cmp(&right.reason_code))
            .then_with(|| {
                diagnostic_stable_candidate_id(left).cmp(&diagnostic_stable_candidate_id(right))
            })
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

#[cfg(test)]
fn select_line_bucket<'a>(
    redaction: &RedactionOccurrence,
    line_buckets: Option<&'a [LineBucket<'a>]>,
) -> Option<&'a LineBucket<'a>> {
    ranked_line_bucket_candidates(redaction, line_buckets)
        .first()
        .map(|candidate| candidate.line_bucket)
}

fn vertical_overlap_pt(y0_a: f32, y1_a: f32, y0_b: f32, y1_b: f32) -> f32 {
    (y1_a.min(y1_b) - y0_a.max(y0_b)).max(0.0_f32)
}

fn ranked_line_bucket_candidates<'a>(
    redaction: &RedactionOccurrence,
    line_buckets: Option<&'a [LineBucket<'a>]>,
) -> Vec<LineBucketCandidate<'a>> {
    let Some(buckets) = line_buckets else {
        return Vec::new();
    };
    let mut candidates = buckets
        .iter()
        .filter_map(|line_bucket| {
            let vertical_overlap = vertical_overlap_pt(
                line_bucket.y0,
                line_bucket.y1,
                redaction.bbox.y0,
                redaction.bbox.y1,
            );
            let baseline_delta = (line_bucket.baseline_y1 - redaction.bbox.y1).abs();
            ((vertical_overlap > 0.0_f32) || (baseline_delta <= SAME_LINE_BASELINE_TOLERANCE_PT))
                .then_some(LineBucketCandidate {
                    line_bucket,
                    vertical_overlap_pt: vertical_overlap,
                    baseline_delta_pt: baseline_delta,
                })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .vertical_overlap_pt
            .partial_cmp(&left.vertical_overlap_pt)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.baseline_delta_pt
                    .partial_cmp(&right.baseline_delta_pt)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.line_bucket.line_id.cmp(&right.line_bucket.line_id))
    });
    candidates
}

fn ranked_anchor_run_candidates<'a>(
    redaction: &RedactionOccurrence,
    line_runs: &[&'a PdfFontTextRun],
    left_side: bool,
) -> Vec<AnchorRunCandidate<'a>> {
    let mut candidates = line_runs
        .iter()
        .copied()
        .filter_map(|run| {
            let trimmed = run.text.trim();
            if trimmed.is_empty() {
                return None;
            }
            let allowed = if left_side {
                run.bbox.x1 <= redaction.bbox.x0
            } else {
                run.bbox.x0 >= redaction.bbox.x1
            };
            if !allowed {
                return None;
            }
            let gap_pt = if left_side {
                (redaction.bbox.x0 - run.bbox.x1).abs()
            } else {
                (run.bbox.x0 - redaction.bbox.x1).abs()
            };
            Some(AnchorRunCandidate {
                run,
                visibility_rank: visibility_rank(run),
                gap_pt,
                width_pt: (run.bbox.x1 - run.bbox.x0).abs(),
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.visibility_rank
            .cmp(&right.visibility_rank)
            .then_with(|| {
                left.gap_pt
                    .partial_cmp(&right.gap_pt)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.width_pt
                    .partial_cmp(&right.width_pt)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| anchor_run_sort_key(left.run).cmp(&anchor_run_sort_key(right.run)))
    });
    candidates
}

fn build_row(
    redaction: &RedactionOccurrence,
    redaction_id: &str,
    row_id: &str,
    line_buckets: Option<&[LineBucket<'_>]>,
    font_truth: &PdfFontTruthCatalog,
    collect_diagnostics: bool,
) -> RowBuildOutput {
    let location = DiagnosticLocation {
        row_id: Some(row_id.to_owned()),
        redaction_id: Some(redaction_id.to_owned()),
        page_index: redaction.page_index,
        bbox: redaction.bbox,
    };
    let line_bucket_candidates = ranked_line_bucket_candidates(redaction, line_buckets);
    let (resolution, diagnostics) = resolve_anchor_set_for_redaction(
        redaction,
        row_id,
        location.clone(),
        &line_bucket_candidates,
        collect_diagnostics,
    );
    let measurement_seed_run = measurement_seed_run(&resolution);
    let measurement_model = measurement_seed_run
        .map(|seed_run| build_measurement_model(seed_run, font_truth))
        .unwrap_or_default();
    let boundary_space = resolution
        .measurement_seed_side
        .map(|_| boundary_space_width_pt(&measurement_model))
        .unwrap_or(0.0_f32);
    let left_anchor = resolution
        .left
        .as_ref()
        .map(|candidate| candidate.anchor.clone());
    let right_anchor = resolution
        .right
        .as_ref()
        .map(|candidate| candidate.anchor.clone());
    let left_anchor_width_pt = left_anchor
        .as_ref()
        .map(|anchor| measure_text(&measurement_model, &anchor.text))
        .transpose()
        .ok()
        .flatten();
    let geometry_line_bucket = resolution.selected_line_bucket.or_else(|| {
        line_bucket_candidates
            .first()
            .map(|candidate| candidate.line_bucket)
    });
    let (line_bias_pt, tolerance_pt) = geometry_line_bucket
        .map(|line_bucket| {
            estimate_row_geometry(line_bucket, measurement_seed_run, &measurement_model)
        })
        .unwrap_or((0.0_f32, ROW_EPSILON_MIN_PT as f32));
    let usable_left_edge_x_pt = left_anchor.as_ref().and_then(|anchor| {
        left_anchor_width_pt
            .map(|anchor_width| anchor.text_edge_x_pt + anchor_width + boundary_space)
    });
    let usable_right_edge_x_pt = right_anchor
        .as_ref()
        .map(|anchor| anchor.text_edge_x_pt - boundary_space);
    let target_width_pt = match (usable_left_edge_x_pt, usable_right_edge_x_pt) {
        (Some(left_edge), Some(right_edge)) if right_edge > left_edge => right_edge - left_edge,
        _ => redaction.bbox.width().abs(),
    };

    RowBuildOutput {
        row: RedactionEvidenceRow {
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
                mode: resolution.mode,
                left: left_anchor,
                right: right_anchor,
                measurement_seed_side: resolution.measurement_seed_side,
                selected_line_id: resolution
                    .selected_line_bucket
                    .map(|line_bucket| line_bucket.line_id.clone()),
                selection_reason: Some(resolution.selection_reason),
                selected_left_gap_pt: resolution.left.as_ref().map(|candidate| candidate.gap_pt),
                selected_right_gap_pt: resolution.right.as_ref().map(|candidate| candidate.gap_pt),
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
                line_id: resolution
                    .selected_line_bucket
                    .map(|line_bucket| line_bucket.line_id.clone())
                    .unwrap_or_else(|| format!("{row_id}_unresolved")),
                ..NeighborFacts::default()
            },
            measurement_model,
        },
        diagnostics,
    }
}

#[cfg(test)]
fn select_anchor_run<'a>(
    redaction: &RedactionOccurrence,
    line_runs: &[&'a PdfFontTextRun],
    left_side: bool,
) -> Option<&'a PdfFontTextRun> {
    ranked_anchor_run_candidates(redaction, line_runs, left_side)
        .first()
        .map(|candidate| candidate.run)
}

fn resolve_anchor_set_for_redaction<'a>(
    redaction: &RedactionOccurrence,
    row_id: &str,
    location: DiagnosticLocation,
    line_bucket_candidates: &'a [LineBucketCandidate<'a>],
    collect_diagnostics: bool,
) -> (
    AnchorResolutionDecision<'a>,
    Vec<RedactionEvidenceDiagnostic>,
) {
    let mut diagnostics = Vec::new();
    let mut bucket_candidates = Vec::<BucketAnchorCandidates<'a>>::new();

    for (line_bucket_rank, candidate) in line_bucket_candidates.iter().enumerate() {
        let bucket = build_bucket_anchor_candidates(
            redaction,
            row_id,
            candidate.line_bucket,
            line_bucket_rank,
            location.clone(),
            collect_diagnostics,
        );
        diagnostics.extend(bucket.1);
        bucket_candidates.push(bucket.0);
    }

    let mut selected_line_id = None::<String>;
    let mut valid_pairs = Vec::<AnchorPairCandidate<'a>>::new();
    for bucket in &bucket_candidates {
        for left in &bucket.left_candidates {
            for right in &bucket.right_candidates {
                let pair_candidate = AnchorPairCandidate {
                    pair_id: format!("{}|{}", left.candidate_id, right.candidate_id),
                    line_bucket: bucket.line_bucket,
                    line_bucket_rank: bucket.line_bucket_rank,
                    left: left.clone(),
                    right: right.clone(),
                };
                if collect_diagnostics {
                    diagnostics.push(build_diagnostic(
                        location.clone(),
                        "redaction_evidence",
                        "anchor_pair_candidate_considered",
                        "considered anchor pair candidate",
                        pair_candidate_metrics(&pair_candidate, line_bucket_candidates.len()),
                    ));
                }
                if let Some((reason_code, message)) =
                    pair_rejection_reason(&pair_candidate.left, &pair_candidate.right)
                {
                    if collect_diagnostics {
                        diagnostics.push(build_diagnostic(
                            location.clone(),
                            "redaction_evidence",
                            reason_code,
                            message,
                            pair_candidate_metrics(&pair_candidate, line_bucket_candidates.len()),
                        ));
                    }
                    continue;
                }
                valid_pairs.push(pair_candidate);
            }
        }
    }

    valid_pairs.sort_by(compare_pair_candidates);
    if let Some(selected_pair) = valid_pairs.first().cloned() {
        if collect_diagnostics {
            diagnostics.extend(build_line_bucket_diagnostics(
                location.clone(),
                redaction,
                line_bucket_candidates,
                Some(selected_pair.line_bucket.line_id.as_str()),
            ));
            diagnostics.push(build_diagnostic(
                location.clone(),
                "redaction_evidence",
                "anchor_pair_selected",
                "selected anchor pair candidate",
                pair_candidate_metrics(&selected_pair, line_bucket_candidates.len()),
            ));
        }
        let measurement_seed_side = select_measurement_seed_side(
            AnchorMode::TwoSided,
            Some(selected_pair.left.gap_pt),
            Some(selected_pair.right.gap_pt),
        );
        if collect_diagnostics {
            diagnostics.push(build_anchor_resolution_final_diagnostic(
                location.clone(),
                selected_pair.line_bucket.line_id.as_str(),
                AnchorMode::TwoSided,
                Some(&selected_pair.left),
                Some(&selected_pair.right),
                measurement_seed_side,
                "pair_candidate_selected",
            ));
            diagnostics.push(build_measurement_seed_diagnostic(
                location.clone(),
                selected_pair.line_bucket.line_id.as_str(),
                measurement_seed_side,
                Some(&selected_pair.left),
                Some(&selected_pair.right),
            ));
        }
        return (
            AnchorResolutionDecision {
                mode: AnchorMode::TwoSided,
                selected_line_bucket: Some(selected_pair.line_bucket),
                left: Some(selected_pair.left),
                right: Some(selected_pair.right),
                measurement_seed_side,
                selection_reason: "pair_candidate_selected".to_owned(),
            },
            diagnostics,
        );
    }

    let mut all_left_candidates = bucket_candidates
        .iter()
        .flat_map(|bucket| bucket.left_candidates.iter().cloned())
        .collect::<Vec<_>>();
    let mut all_right_candidates = bucket_candidates
        .iter()
        .flat_map(|bucket| bucket.right_candidates.iter().cloned())
        .collect::<Vec<_>>();
    all_left_candidates.sort_by(compare_side_candidates);
    all_right_candidates.sort_by(compare_side_candidates);

    let decision = match (
        all_left_candidates.first().cloned(),
        all_right_candidates.first().cloned(),
    ) {
        (Some(left), Some(right)) => {
            let left_key = (
                ordered_f32(left.gap_pt),
                left.line_bucket_rank,
                left.candidate_id.clone(),
            );
            let right_key = (
                ordered_f32(right.gap_pt),
                right.line_bucket_rank,
                right.candidate_id.clone(),
            );
            if left_key <= right_key {
                AnchorResolutionDecision {
                    mode: AnchorMode::LeftOnly,
                    selected_line_bucket: Some(left.line_bucket),
                    left: Some(left),
                    right: None,
                    measurement_seed_side: Some(MeasurementSeedSide::Left),
                    selection_reason: "one_sided_no_valid_pair_left_gap_selected".to_owned(),
                }
            } else {
                AnchorResolutionDecision {
                    mode: AnchorMode::RightOnly,
                    selected_line_bucket: Some(right.line_bucket),
                    left: None,
                    right: Some(right),
                    measurement_seed_side: Some(MeasurementSeedSide::Right),
                    selection_reason: "one_sided_no_valid_pair_right_gap_selected".to_owned(),
                }
            }
        }
        (Some(left), None) => AnchorResolutionDecision {
            mode: AnchorMode::LeftOnly,
            selected_line_bucket: Some(left.line_bucket),
            left: Some(left),
            right: None,
            measurement_seed_side: Some(MeasurementSeedSide::Left),
            selection_reason: "one_sided_only_left_candidate_available".to_owned(),
        },
        (None, Some(right)) => AnchorResolutionDecision {
            mode: AnchorMode::RightOnly,
            selected_line_bucket: Some(right.line_bucket),
            left: None,
            right: Some(right),
            measurement_seed_side: Some(MeasurementSeedSide::Right),
            selection_reason: "one_sided_only_right_candidate_available".to_owned(),
        },
        (None, None) => AnchorResolutionDecision {
            mode: AnchorMode::Unresolved,
            selected_line_bucket: line_bucket_candidates
                .first()
                .map(|candidate| candidate.line_bucket),
            left: None,
            right: None,
            measurement_seed_side: None,
            selection_reason: if line_bucket_candidates.is_empty() {
                "unresolved_no_eligible_line_bucket".to_owned()
            } else {
                "unresolved_no_valid_anchor_spans".to_owned()
            },
        },
    };

    if let Some(line_bucket) = decision.selected_line_bucket {
        selected_line_id = Some(line_bucket.line_id.clone());
    }
    if collect_diagnostics {
        diagnostics.extend(build_line_bucket_diagnostics(
            location.clone(),
            redaction,
            line_bucket_candidates,
            selected_line_id.as_deref(),
        ));
        match decision.mode {
            AnchorMode::LeftOnly | AnchorMode::RightOnly => {
                diagnostics.push(build_diagnostic(
                    location.clone(),
                    "redaction_evidence",
                    "anchor_one_sided_selected",
                    "selected one-sided anchor candidate",
                    side_selection_metrics(&decision, line_bucket_candidates.len()),
                ));
            }
            AnchorMode::Unresolved => {}
            AnchorMode::TwoSided => {}
        }
        diagnostics.push(build_anchor_resolution_final_diagnostic(
            location.clone(),
            selected_line_id.as_deref().unwrap_or_default(),
            decision.mode,
            decision.left.as_ref(),
            decision.right.as_ref(),
            decision.measurement_seed_side,
            &decision.selection_reason,
        ));
        if decision.measurement_seed_side.is_some() {
            diagnostics.push(build_measurement_seed_diagnostic(
                location,
                selected_line_id.as_deref().unwrap_or_default(),
                decision.measurement_seed_side,
                decision.left.as_ref(),
                decision.right.as_ref(),
            ));
        }
    }

    (decision, diagnostics)
}

fn build_bucket_anchor_candidates<'a>(
    redaction: &RedactionOccurrence,
    row_id: &str,
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    location: DiagnosticLocation,
    collect_diagnostics: bool,
) -> (BucketAnchorCandidates<'a>, Vec<RedactionEvidenceDiagnostic>) {
    let (left_candidates, mut diagnostics) = build_anchor_span_candidates(
        redaction,
        row_id,
        line_bucket,
        line_bucket_rank,
        true,
        location.clone(),
        collect_diagnostics,
    );
    let (right_candidates, right_diagnostics) = build_anchor_span_candidates(
        redaction,
        row_id,
        line_bucket,
        line_bucket_rank,
        false,
        location,
        collect_diagnostics,
    );
    diagnostics.extend(right_diagnostics);
    (
        BucketAnchorCandidates {
            line_bucket,
            line_bucket_rank,
            left_candidates,
            right_candidates,
        },
        diagnostics,
    )
}

fn build_anchor_span_candidates<'a>(
    redaction: &RedactionOccurrence,
    row_id: &str,
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    left_side: bool,
    location: DiagnosticLocation,
    collect_diagnostics: bool,
) -> (
    Vec<AnchorSpanCandidate<'a>>,
    Vec<RedactionEvidenceDiagnostic>,
) {
    let run_candidates = ranked_anchor_run_candidates(redaction, &line_bucket.runs, left_side);
    let mut candidates = Vec::<AnchorSpanCandidate<'a>>::new();
    let mut diagnostics = Vec::<RedactionEvidenceDiagnostic>::new();

    for (candidate_rank, run_candidate) in run_candidates.iter().enumerate() {
        let span_candidate = build_anchor_span_candidate(
            row_id,
            line_bucket,
            line_bucket_rank,
            candidate_rank,
            run_candidate,
            left_side,
        );
        if collect_diagnostics {
            diagnostics.push(build_diagnostic(
                location.clone(),
                "redaction_evidence",
                "anchor_span_candidate_considered",
                "considered anchor span candidate",
                anchor_span_metrics(
                    &span_candidate,
                    run_candidates.len(),
                    candidate_rank,
                    redaction,
                ),
            ));
        }
        if !anchor_text_has_alnum(&span_candidate.anchor.text) {
            if collect_diagnostics {
                diagnostics.push(build_diagnostic(
                    location.clone(),
                    "redaction_evidence",
                    "anchor_span_rejected_non_alnum",
                    "anchor span text contains no unicode letters or digits",
                    anchor_span_metrics(
                        &span_candidate,
                        run_candidates.len(),
                        candidate_rank,
                        redaction,
                    ),
                ));
            }
            continue;
        }
        candidates.push(span_candidate);
    }

    (candidates, diagnostics)
}

fn build_line_bucket_diagnostics(
    location: DiagnosticLocation,
    redaction: &RedactionOccurrence,
    candidates: &[LineBucketCandidate<'_>],
    selected_line_id: Option<&str>,
) -> Vec<RedactionEvidenceDiagnostic> {
    let mut diagnostics = Vec::new();
    for (candidate_rank, candidate) in candidates.iter().enumerate() {
        diagnostics.push(build_diagnostic(
            location.clone(),
            "redaction_evidence",
            "line_bucket_candidate_considered",
            "considered same-line text bucket for redaction",
            line_bucket_metrics(
                candidate.line_bucket,
                redaction,
                candidates.len(),
                candidate_rank,
            ),
        ));
    }
    if let Some(selected_line_id) = selected_line_id {
        if let Some((selected_rank, selected)) = candidates
            .iter()
            .enumerate()
            .find(|(_, candidate)| candidate.line_bucket.line_id == selected_line_id)
        {
            diagnostics.push(build_diagnostic(
                location,
                "redaction_evidence",
                "line_bucket_selected",
                "selected same-line text bucket for redaction",
                line_bucket_metrics(
                    selected.line_bucket,
                    redaction,
                    candidates.len(),
                    selected_rank,
                ),
            ));
        }
    }
    diagnostics
}

fn build_anchor_span_candidate<'a>(
    row_id: &str,
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    candidate_rank: usize,
    run_candidate: &AnchorRunCandidate<'a>,
    left_side: bool,
) -> AnchorSpanCandidate<'a> {
    let side = if left_side { "left" } else { "right" };
    let anchor = build_anchor_side(row_id, run_candidate.run, &line_bucket.runs, left_side);
    let width_pt = anchor.bbox.width().abs();
    AnchorSpanCandidate {
        candidate_id: format!(
            "{}:{}:{candidate_rank:03}:{}",
            line_bucket.line_id,
            side,
            anchor_run_sort_key(run_candidate.run)
        ),
        line_bucket,
        line_bucket_rank,
        run: run_candidate.run,
        left_side,
        anchor,
        visibility_rank: run_candidate.visibility_rank,
        gap_pt: run_candidate.gap_pt,
        width_pt,
    }
}

fn compare_pair_candidates(
    left: &AnchorPairCandidate<'_>,
    right: &AnchorPairCandidate<'_>,
) -> std::cmp::Ordering {
    (
        left.line_bucket_rank,
        ordered_f32(left.left.gap_pt + left.right.gap_pt),
        ordered_f32(left.left.gap_pt.max(left.right.gap_pt)),
        ordered_f32(left.left.gap_pt),
        ordered_f32(left.right.gap_pt),
        left.pair_id.as_str(),
    )
        .cmp(&(
            right.line_bucket_rank,
            ordered_f32(right.left.gap_pt + right.right.gap_pt),
            ordered_f32(right.left.gap_pt.max(right.right.gap_pt)),
            ordered_f32(right.left.gap_pt),
            ordered_f32(right.right.gap_pt),
            right.pair_id.as_str(),
        ))
}

fn compare_side_candidates(
    left: &AnchorSpanCandidate<'_>,
    right: &AnchorSpanCandidate<'_>,
) -> std::cmp::Ordering {
    (
        left.line_bucket_rank,
        left.visibility_rank,
        ordered_f32(left.gap_pt),
        ordered_f32(left.width_pt),
        left.candidate_id.as_str(),
    )
        .cmp(&(
            right.line_bucket_rank,
            right.visibility_rank,
            ordered_f32(right.gap_pt),
            ordered_f32(right.width_pt),
            right.candidate_id.as_str(),
        ))
}

fn pair_rejection_reason(
    left: &AnchorSpanCandidate<'_>,
    right: &AnchorSpanCandidate<'_>,
) -> Option<(&'static str, &'static str)> {
    if left.run.font_key != right.run.font_key {
        return Some((
            "anchor_pair_rejected_font_key_mismatch",
            "anchor pair candidates use different font resources",
        ));
    }
    if normalized_font_name(&left.run.font_name) != normalized_font_name(&right.run.font_name) {
        return Some((
            "anchor_pair_rejected_font_name_mismatch",
            "anchor pair candidates use different normalized font names",
        ));
    }
    if normalize_font_size_pt(left.run.font_size_pt)
        != normalize_font_size_pt(right.run.font_size_pt)
    {
        return Some((
            "anchor_pair_rejected_font_size_mismatch",
            "anchor pair candidates use different normalized font sizes",
        ));
    }
    if normalize_spacing_pt(left.run.width_metrics.char_spacing_pt)
        != normalize_spacing_pt(right.run.width_metrics.char_spacing_pt)
    {
        return Some((
            "anchor_pair_rejected_char_spacing_mismatch",
            "anchor pair candidates use different normalized char spacing",
        ));
    }
    if normalize_spacing_pt(left.run.width_metrics.word_spacing_pt)
        != normalize_spacing_pt(right.run.width_metrics.word_spacing_pt)
    {
        return Some((
            "anchor_pair_rejected_word_spacing_mismatch",
            "anchor pair candidates use different normalized word spacing",
        ));
    }
    if left.run.width_metrics.render_mode != right.run.width_metrics.render_mode {
        return Some((
            "anchor_pair_rejected_render_mode_mismatch",
            "anchor pair candidates use different render modes",
        ));
    }
    None
}

fn select_measurement_seed_side(
    mode: AnchorMode,
    left_gap_pt: Option<f32>,
    right_gap_pt: Option<f32>,
) -> Option<MeasurementSeedSide> {
    match mode {
        AnchorMode::TwoSided => match (left_gap_pt, right_gap_pt) {
            (Some(left_gap_pt), Some(right_gap_pt)) if left_gap_pt <= right_gap_pt => {
                Some(MeasurementSeedSide::Left)
            }
            (Some(_), Some(_)) => Some(MeasurementSeedSide::Right),
            (Some(_), None) => Some(MeasurementSeedSide::Left),
            (None, Some(_)) => Some(MeasurementSeedSide::Right),
            (None, None) => None,
        },
        AnchorMode::LeftOnly => Some(MeasurementSeedSide::Left),
        AnchorMode::RightOnly => Some(MeasurementSeedSide::Right),
        AnchorMode::Unresolved => None,
    }
}

fn measurement_seed_run<'a>(
    resolution: &'a AnchorResolutionDecision<'a>,
) -> Option<&'a PdfFontTextRun> {
    match resolution.measurement_seed_side {
        Some(MeasurementSeedSide::Left) => resolution.left.as_ref().map(|candidate| candidate.run),
        Some(MeasurementSeedSide::Right) => {
            resolution.right.as_ref().map(|candidate| candidate.run)
        }
        None => None,
    }
}

fn build_anchor_resolution_final_diagnostic(
    location: DiagnosticLocation,
    line_id: &str,
    mode: AnchorMode,
    left: Option<&AnchorSpanCandidate<'_>>,
    right: Option<&AnchorSpanCandidate<'_>>,
    measurement_seed_side: Option<MeasurementSeedSide>,
    selection_reason: &str,
) -> RedactionEvidenceDiagnostic {
    let mut metrics = resolution_metrics(
        line_id,
        mode,
        left,
        right,
        measurement_seed_side,
        selection_reason,
    );
    metrics.insert(
        "stable_candidate_id".to_owned(),
        DiagnosticValue::Text(stable_resolution_candidate_id(left, right)),
    );
    build_diagnostic(
        location,
        "redaction_evidence",
        "anchor_resolution_final",
        "resolved final anchor decision for row",
        metrics,
    )
}

fn build_measurement_seed_diagnostic(
    location: DiagnosticLocation,
    line_id: &str,
    measurement_seed_side: Option<MeasurementSeedSide>,
    left: Option<&AnchorSpanCandidate<'_>>,
    right: Option<&AnchorSpanCandidate<'_>>,
) -> RedactionEvidenceDiagnostic {
    let mut metrics = resolution_metrics(
        line_id,
        left.and(right)
            .map(|_| AnchorMode::TwoSided)
            .unwrap_or_else(|| {
                if left.is_some() {
                    AnchorMode::LeftOnly
                } else if right.is_some() {
                    AnchorMode::RightOnly
                } else {
                    AnchorMode::Unresolved
                }
            }),
        left,
        right,
        measurement_seed_side,
        "measurement_seed_selected",
    );
    metrics.insert(
        "stable_candidate_id".to_owned(),
        DiagnosticValue::Text(stable_resolution_candidate_id(left, right)),
    );
    build_diagnostic(
        location,
        "redaction_evidence",
        "measurement_seed_selected",
        "selected measurement seed side for anchor resolution",
        metrics,
    )
}

fn resolution_metrics(
    line_id: &str,
    mode: AnchorMode,
    left: Option<&AnchorSpanCandidate<'_>>,
    right: Option<&AnchorSpanCandidate<'_>>,
    measurement_seed_side: Option<MeasurementSeedSide>,
    selection_reason: &str,
) -> BTreeMap<String, DiagnosticValue> {
    let mut metrics = BTreeMap::new();
    metrics.insert(
        "line_id".to_owned(),
        DiagnosticValue::Text(line_id.to_owned()),
    );
    metrics.insert(
        "final_mode".to_owned(),
        DiagnosticValue::Text(mode.as_str().to_owned()),
    );
    metrics.insert(
        "selection_reason".to_owned(),
        DiagnosticValue::Text(selection_reason.to_owned()),
    );
    if let Some(measurement_seed_side) = measurement_seed_side {
        metrics.insert(
            "measurement_seed_side".to_owned(),
            DiagnosticValue::Text(measurement_seed_side.as_str().to_owned()),
        );
    }
    if let Some(left) = left {
        metrics.insert(
            "selected_left_text".to_owned(),
            DiagnosticValue::Text(left.anchor.text.clone()),
        );
        metrics.insert(
            "selected_left_gap_pt".to_owned(),
            DiagnosticValue::Float(left.gap_pt as f64),
        );
        metrics.insert(
            "selected_left_candidate_id".to_owned(),
            DiagnosticValue::Text(left.candidate_id.clone()),
        );
    }
    if let Some(right) = right {
        metrics.insert(
            "selected_right_text".to_owned(),
            DiagnosticValue::Text(right.anchor.text.clone()),
        );
        metrics.insert(
            "selected_right_gap_pt".to_owned(),
            DiagnosticValue::Float(right.gap_pt as f64),
        );
        metrics.insert(
            "selected_right_candidate_id".to_owned(),
            DiagnosticValue::Text(right.candidate_id.clone()),
        );
    }
    metrics
}

fn side_selection_metrics(
    decision: &AnchorResolutionDecision<'_>,
    line_bucket_count: usize,
) -> BTreeMap<String, DiagnosticValue> {
    let mut metrics = resolution_metrics(
        decision
            .selected_line_bucket
            .map(|line_bucket| line_bucket.line_id.as_str())
            .unwrap_or_default(),
        decision.mode,
        decision.left.as_ref(),
        decision.right.as_ref(),
        decision.measurement_seed_side,
        &decision.selection_reason,
    );
    metrics.insert(
        "line_bucket_count".to_owned(),
        DiagnosticValue::Integer(line_bucket_count as i64),
    );
    metrics
}

fn pair_candidate_metrics(
    pair: &AnchorPairCandidate<'_>,
    line_bucket_count: usize,
) -> BTreeMap<String, DiagnosticValue> {
    let mut metrics = BTreeMap::new();
    metrics.insert(
        "stable_candidate_id".to_owned(),
        DiagnosticValue::Text(pair.pair_id.clone()),
    );
    metrics.insert(
        "line_bucket_count".to_owned(),
        DiagnosticValue::Integer(line_bucket_count as i64),
    );
    metrics.insert(
        "line_bucket_rank".to_owned(),
        DiagnosticValue::Integer(pair.line_bucket_rank as i64),
    );
    metrics.insert(
        "line_id".to_owned(),
        DiagnosticValue::Text(pair.line_bucket.line_id.clone()),
    );
    metrics.insert(
        "left_candidate_id".to_owned(),
        DiagnosticValue::Text(pair.left.candidate_id.clone()),
    );
    metrics.insert(
        "right_candidate_id".to_owned(),
        DiagnosticValue::Text(pair.right.candidate_id.clone()),
    );
    metrics.insert(
        "left_gap_pt".to_owned(),
        DiagnosticValue::Float(pair.left.gap_pt as f64),
    );
    metrics.insert(
        "right_gap_pt".to_owned(),
        DiagnosticValue::Float(pair.right.gap_pt as f64),
    );
    metrics.insert(
        "pair_gap_sum_pt".to_owned(),
        DiagnosticValue::Float((pair.left.gap_pt + pair.right.gap_pt) as f64),
    );
    metrics.insert(
        "pair_gap_max_pt".to_owned(),
        DiagnosticValue::Float(pair.left.gap_pt.max(pair.right.gap_pt) as f64),
    );
    metrics.insert(
        "left_text".to_owned(),
        DiagnosticValue::Text(pair.left.anchor.text.clone()),
    );
    metrics.insert(
        "right_text".to_owned(),
        DiagnosticValue::Text(pair.right.anchor.text.clone()),
    );
    extend_metrics(&mut metrics, width_profile_metrics("left", pair.left.run));
    extend_metrics(&mut metrics, width_profile_metrics("right", pair.right.run));
    metrics
}

fn anchor_span_metrics(
    candidate: &AnchorSpanCandidate<'_>,
    candidate_count: usize,
    candidate_rank: usize,
    redaction: &RedactionOccurrence,
) -> BTreeMap<String, DiagnosticValue> {
    let mut metrics = BTreeMap::new();
    metrics.insert(
        "stable_candidate_id".to_owned(),
        DiagnosticValue::Text(candidate.candidate_id.clone()),
    );
    metrics.insert(
        "anchor_side".to_owned(),
        DiagnosticValue::Text(if candidate.left_side {
            "left".to_owned()
        } else {
            "right".to_owned()
        }),
    );
    metrics.insert(
        "candidate_count".to_owned(),
        DiagnosticValue::Integer(candidate_count as i64),
    );
    metrics.insert(
        "candidate_rank".to_owned(),
        DiagnosticValue::Integer(candidate_rank as i64),
    );
    metrics.insert(
        "line_bucket_rank".to_owned(),
        DiagnosticValue::Integer(candidate.line_bucket_rank as i64),
    );
    metrics.insert(
        "line_id".to_owned(),
        DiagnosticValue::Text(candidate.line_bucket.line_id.clone()),
    );
    metrics.insert(
        "anchor_text".to_owned(),
        DiagnosticValue::Text(candidate.anchor.text.clone()),
    );
    metrics.insert(
        "gap_pt".to_owned(),
        DiagnosticValue::Float(candidate.gap_pt as f64),
    );
    metrics.insert(
        "visibility_rank".to_owned(),
        DiagnosticValue::Integer(candidate.visibility_rank as i64),
    );
    metrics.insert(
        "span_x0".to_owned(),
        DiagnosticValue::Float(candidate.anchor.bbox.x0 as f64),
    );
    metrics.insert(
        "span_x1".to_owned(),
        DiagnosticValue::Float(candidate.anchor.bbox.x1 as f64),
    );
    metrics.insert(
        "span_y0".to_owned(),
        DiagnosticValue::Float(candidate.anchor.bbox.y0 as f64),
    );
    metrics.insert(
        "span_y1".to_owned(),
        DiagnosticValue::Float(candidate.anchor.bbox.y1 as f64),
    );
    metrics.insert(
        "span_width_pt".to_owned(),
        DiagnosticValue::Float(candidate.width_pt as f64),
    );
    metrics.insert(
        "text_edge_x_pt".to_owned(),
        DiagnosticValue::Float(candidate.anchor.text_edge_x_pt as f64),
    );
    metrics.insert(
        "redaction_left_x_pt".to_owned(),
        DiagnosticValue::Float(redaction.bbox.x0 as f64),
    );
    metrics.insert(
        "redaction_right_x_pt".to_owned(),
        DiagnosticValue::Float(redaction.bbox.x1 as f64),
    );
    metrics.insert(
        "font_key".to_owned(),
        DiagnosticValue::Text(candidate.run.font_key.clone()),
    );
    metrics.insert(
        "font_name".to_owned(),
        DiagnosticValue::Text(normalized_font_name(&candidate.run.font_name)),
    );
    extend_metrics(&mut metrics, width_profile_metrics("run", candidate.run));
    metrics
}

fn anchor_text_has_alnum(text: &str) -> bool {
    normalize_transport_text(text)
        .chars()
        .any(char::is_alphanumeric)
}

fn ordered_f32(value: f32) -> i32 {
    ((value as f64) * 10_000.0_f64).round() as i32
}

fn stable_resolution_candidate_id(
    left: Option<&AnchorSpanCandidate<'_>>,
    right: Option<&AnchorSpanCandidate<'_>>,
) -> String {
    match (left, right) {
        (Some(left), Some(right)) => format!("{}|{}", left.candidate_id, right.candidate_id),
        (Some(left), None) => left.candidate_id.clone(),
        (None, Some(right)) => right.candidate_id.clone(),
        (None, None) => String::new(),
    }
}

fn diagnostic_stable_candidate_id(diagnostic: &RedactionEvidenceDiagnostic) -> String {
    diagnostic
        .metrics
        .get("stable_candidate_id")
        .and_then(|value| match value {
            DiagnosticValue::Text(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn line_bucket_metrics(
    line_bucket: &LineBucket<'_>,
    redaction: &RedactionOccurrence,
    candidate_count: usize,
    candidate_rank: usize,
) -> BTreeMap<String, DiagnosticValue> {
    let (bucket_left_x_pt, bucket_right_x_pt) = line_bucket_span(line_bucket);
    let left_candidates = ranked_anchor_run_candidates(redaction, &line_bucket.runs, true);
    let right_candidates = ranked_anchor_run_candidates(redaction, &line_bucket.runs, false);
    let mut metrics = BTreeMap::new();
    metrics.insert(
        "candidate_count".to_owned(),
        DiagnosticValue::Integer(candidate_count as i64),
    );
    metrics.insert(
        "candidate_rank".to_owned(),
        DiagnosticValue::Integer(candidate_rank as i64),
    );
    metrics.insert(
        "line_id".to_owned(),
        DiagnosticValue::Text(line_bucket.line_id.clone()),
    );
    metrics.insert(
        "bucket_baseline_y1".to_owned(),
        DiagnosticValue::Float(line_bucket.baseline_y1 as f64),
    );
    metrics.insert(
        "bucket_y0".to_owned(),
        DiagnosticValue::Float(line_bucket.y0 as f64),
    );
    metrics.insert(
        "bucket_y1".to_owned(),
        DiagnosticValue::Float(line_bucket.y1 as f64),
    );
    metrics.insert(
        "bucket_left_x_pt".to_owned(),
        DiagnosticValue::Float(bucket_left_x_pt as f64),
    );
    metrics.insert(
        "bucket_right_x_pt".to_owned(),
        DiagnosticValue::Float(bucket_right_x_pt as f64),
    );
    metrics.insert(
        "bucket_vertical_overlap_pt".to_owned(),
        DiagnosticValue::Float(vertical_overlap_pt(
            line_bucket.y0,
            line_bucket.y1,
            redaction.bbox.y0,
            redaction.bbox.y1,
        ) as f64),
    );
    metrics.insert(
        "bucket_baseline_delta_pt".to_owned(),
        DiagnosticValue::Float((line_bucket.baseline_y1 - redaction.bbox.y1).abs() as f64),
    );
    metrics.insert(
        "bucket_run_count".to_owned(),
        DiagnosticValue::Integer(line_bucket.runs.len() as i64),
    );
    metrics.insert(
        "bucket_text_preview".to_owned(),
        DiagnosticValue::Text(line_bucket_text_preview(line_bucket)),
    );
    metrics.insert(
        "bucket_left_candidate_count".to_owned(),
        DiagnosticValue::Integer(left_candidates.len() as i64),
    );
    metrics.insert(
        "bucket_right_candidate_count".to_owned(),
        DiagnosticValue::Integer(right_candidates.len() as i64),
    );
    metrics
}

fn line_bucket_span(line_bucket: &LineBucket<'_>) -> (f32, f32) {
    let left_x = line_bucket
        .runs
        .iter()
        .map(|run| run.bbox.x0)
        .fold(f32::INFINITY, f32::min);
    let right_x = line_bucket
        .runs
        .iter()
        .map(|run| run.bbox.x1)
        .fold(f32::NEG_INFINITY, f32::max);
    let left_x = if left_x.is_finite() { left_x } else { 0.0_f32 };
    let right_x = if right_x.is_finite() {
        right_x
    } else {
        0.0_f32
    };
    (left_x, right_x)
}

fn line_bucket_text_preview(line_bucket: &LineBucket<'_>) -> String {
    line_bucket
        .runs
        .iter()
        .map(|run| normalize_transport_text(&run.text))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn anchor_run_sort_key(run: &PdfFontTextRun) -> String {
    format!("{:.4}:{:.4}:{}", run.bbox.x0, run.bbox.y0, run.text)
}

fn build_anchor_side(
    row_id: &str,
    run: &PdfFontTextRun,
    line_runs: &[&PdfFontTextRun],
    left_side: bool,
) -> AnchorSide {
    let (text, text_edge_x_pt, bbox) = enrich_anchor_text_and_edge(run, line_runs, left_side);
    AnchorSide {
        anchor_id: if left_side {
            format!("{row_id}_left")
        } else {
            format!("{row_id}_right")
        },
        text,
        bbox,
        text_edge_x_pt,
    }
}

fn enrich_anchor_text_and_edge(
    run: &PdfFontTextRun,
    line_runs: &[&PdfFontTextRun],
    left_side: bool,
) -> (String, f32, Rect) {
    let mut text = normalize_transport_text(&run.text);
    let mut text_edge_x_pt = run.bbox.x0;
    let mut bbox = Rect::new(run.bbox.x0, run.bbox.y0, run.bbox.x1, run.bbox.y1);
    let Some(run_index) = line_runs
        .iter()
        .position(|candidate| std::ptr::eq(*candidate, run))
    else {
        return (text, text_edge_x_pt, bbox);
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
            text_edge_x_pt = previous.bbox.x0;
            bbox = Rect::new(
                previous.bbox.x0.min(bbox.x0),
                previous.bbox.y0.min(bbox.y0),
                bbox.x1.max(previous.bbox.x1),
                previous.bbox.y1.max(bbox.y1),
            );
            cursor -= 1;
        }
        return (text, text_edge_x_pt, bbox);
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
        bbox = Rect::new(
            bbox.x0.min(next.bbox.x0),
            bbox.y0.min(next.bbox.y0),
            bbox.x1.max(next.bbox.x1),
            bbox.y1.max(next.bbox.y1),
        );
        cursor += 1;
    }
    (text, text_edge_x_pt, bbox)
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
    seed_run: &PdfFontTextRun,
    font_truth: &PdfFontTruthCatalog,
) -> CandidateWidthModel {
    let font_name = normalized_font_name(&seed_run.font_name);
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
    CandidateWidthModel {
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
    }
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
    if row.anchor_set.mode == AnchorMode::Unresolved
        || row.anchor_set.measurement_seed_side.is_none()
    {
        return Vec::new();
    }
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

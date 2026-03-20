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
const LINE_BUCKET_HORIZONTAL_GAP_MAX_PT: f32 = 250.0_f32;
const ANCHOR_BOUNDARY_SMALL_OVERLAP_PT: f32 = 3.0_f32;
const ANCHOR_BOUNDARY_PUNCTUATION_GAP_MAX_PT: f32 = 0.25_f32;
const ANCHOR_NONLOCAL_PAIR_BOX_DELTA_PT: f32 = 10.0_f32;
const ANCHOR_NONLOCAL_PAIR_BOX_DELTA_RATIO: f32 = 0.15_f32;
const ANCHOR_NONLOCAL_PAIR_MAX_GAP_PT: f32 = 20.0_f32;
const ANCHOR_NONLOCAL_PAIR_GAP_DIFF_PT: f32 = 10.0_f32;

#[derive(Clone)]
struct LineBucket<'a> {
    line_id: String,
    baseline_y1: f32,
    y0: f32,
    y1: f32,
    rightmost_x1: f32,
    split_from_previous_horizontal_gap_pt: Option<f32>,
    runs: Vec<&'a PdfFontTextRun>,
}

struct LineBucketCandidate<'a> {
    line_bucket: &'a LineBucket<'a>,
    vertical_overlap_pt: f32,
    baseline_delta_pt: f32,
    baseline_eligible: bool,
    overlap_eligible: bool,
}

#[derive(Clone, Copy)]
struct AnchorRunCandidate<'a> {
    run: &'a PdfFontTextRun,
    relation: AnchorRunRelation,
    visibility_rank: i32,
    gap_pt: f32,
    overlap_depth_pt: f32,
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
    relation: AnchorRunRelation,
    visibility_rank: i32,
    gap_pt: f32,
    overlap_depth_pt: f32,
    width_pt: f32,
}

#[derive(Clone)]
struct AnchorPairCandidate<'a> {
    pair_id: String,
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    baseline_eligible: bool,
    overlap_eligible: bool,
    left_candidate_count: usize,
    right_candidate_count: usize,
    left: AnchorSpanCandidate<'a>,
    right: AnchorSpanCandidate<'a>,
}

struct BucketAnchorCandidates<'a> {
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    baseline_eligible: bool,
    overlap_eligible: bool,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum BoundaryGapSource {
    ExplicitTrailingWhitespace,
    ExplicitLeadingWhitespace,
    None,
}

impl BoundaryGapSource {
    #[inline]
    fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitTrailingWhitespace => "explicit_trailing_whitespace",
            Self::ExplicitLeadingWhitespace => "explicit_leading_whitespace",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Copy)]
struct BoundaryGapDecision {
    gap_pt: f32,
    explicit_gap_pt: f32,
    inferred_gap_pt: f32,
    source: BoundaryGapSource,
}

#[derive(Clone, Copy)]
struct BoundaryRunGeometry {
    inner_left_edge_x_pt: f32,
    inner_right_edge_x_pt: f32,
    leading_whitespace_width_pt: f32,
    trailing_whitespace_width_pt: f32,
}

#[derive(Clone, Copy)]
struct AnchorGeometryComputation {
    usable_left_edge_x_pt: Option<f32>,
    usable_right_edge_x_pt: Option<f32>,
    target_width_pt: f32,
    left_boundary_gap: Option<BoundaryGapDecision>,
    right_boundary_gap: Option<BoundaryGapDecision>,
}

struct AnchorGeometryDiagnosticsInput<'a> {
    left_anchor: Option<&'a AnchorSide>,
    right_anchor: Option<&'a AnchorSide>,
    left_boundary_gap: Option<BoundaryGapDecision>,
    right_boundary_gap: Option<BoundaryGapDecision>,
    usable_left_edge_x_pt: Option<f32>,
    usable_right_edge_x_pt: Option<f32>,
    target_width_pt: f32,
    redaction_width_pt: f32,
}

#[derive(Clone)]
struct DiagnosticLocation {
    row_id: Option<String>,
    redaction_id: Option<String>,
    page_index: u32,
    bbox: Rect,
}

struct AnchorSpanBuildRequest<'a, 'b> {
    redaction: &'b RedactionOccurrence,
    row_id: &'b str,
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    left_side: bool,
    location: DiagnosticLocation,
    collect_diagnostics: bool,
    overlap_tolerance_pt: f32,
}

struct BucketCandidateSummary {
    left_run_candidate_count: usize,
    right_run_candidate_count: usize,
    left_span_candidate_count: usize,
    right_span_candidate_count: usize,
}

struct AnchorRunMetricsInput<'a> {
    run: &'a PdfFontTextRun,
    left_side: bool,
    line_bucket: &'a LineBucket<'a>,
    line_bucket_rank: usize,
    redaction: &'a RedactionOccurrence,
    candidate_count: usize,
    candidate_rank: Option<usize>,
    assessment: AnchorRunBoundaryAssessment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum AnchorRunRelation {
    FullyOutside,
    SmallOverlap,
    DeepOverlap,
    OppositeSide,
}

impl AnchorRunRelation {
    #[inline]
    fn as_str(self) -> &'static str {
        match self {
            Self::FullyOutside => "fully_outside",
            Self::SmallOverlap => "small_overlap",
            Self::DeepOverlap => "deep_overlap",
            Self::OppositeSide => "opposite_side",
        }
    }

    #[inline]
    fn sort_rank(self) -> i32 {
        match self {
            Self::FullyOutside => 0,
            Self::SmallOverlap => 1,
            Self::DeepOverlap => 2,
            Self::OppositeSide => 3,
        }
    }

    #[inline]
    fn is_allowed(self) -> bool {
        matches!(self, Self::FullyOutside | Self::SmallOverlap)
    }
}

#[derive(Clone, Copy)]
struct AnchorRunBoundaryAssessment {
    relation: AnchorRunRelation,
    boundary_distance_pt: f32,
    overlap_depth_pt: f32,
    overlap_tolerance_pt: f32,
    boundary_geometry: BoundaryRunGeometry,
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
            let mut target_index = None::<usize>;
            let mut split_gap_pt = None::<f32>;
            for index in (0..lines.len()).rev() {
                let line = &lines[index];
                if (run.bbox.y1 - line.baseline_y1).abs() > SAME_LINE_BASELINE_TOLERANCE_PT {
                    continue;
                }
                let horizontal_gap_pt = (run.bbox.x0 - line.rightmost_x1).max(0.0_f32);
                if horizontal_gap_pt <= LINE_BUCKET_HORIZONTAL_GAP_MAX_PT {
                    target_index = Some(index);
                    break;
                }
                if split_gap_pt.is_none() {
                    split_gap_pt = Some(horizontal_gap_pt);
                }
            }
            if let Some(target_index) = target_index {
                let existing = &mut lines[target_index];
                existing.y0 = existing.y0.min(run.bbox.y0);
                existing.y1 = existing.y1.max(run.bbox.y1);
                existing.rightmost_x1 = existing.rightmost_x1.max(run.bbox.x1.max(run.bbox.x0));
                existing.runs.push(*run);
            } else {
                let line_id = format!("page{page_index}_line{:03}", lines.len());
                lines.push(LineBucket {
                    line_id,
                    baseline_y1: run.bbox.y1,
                    y0: run.bbox.y0,
                    y1: run.bbox.y1,
                    rightmost_x1: run.bbox.x1.max(run.bbox.x0),
                    split_from_previous_horizontal_gap_pt: split_gap_pt,
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
            let overlap_eligible = vertical_overlap > 0.0_f32;
            let baseline_eligible = baseline_delta <= SAME_LINE_BASELINE_TOLERANCE_PT;
            (overlap_eligible || baseline_eligible).then_some(LineBucketCandidate {
                line_bucket,
                vertical_overlap_pt: vertical_overlap,
                baseline_delta_pt: baseline_delta,
                baseline_eligible,
                overlap_eligible,
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
    overlap_tolerance_pt: f32,
) -> Vec<AnchorRunCandidate<'a>> {
    let mut candidates = line_runs
        .iter()
        .copied()
        .filter_map(|run| {
            let trimmed = run.text.trim();
            if trimmed.is_empty() {
                return None;
            }
            let assessment =
                assess_anchor_run_boundary(redaction, run, left_side, overlap_tolerance_pt);
            if !assessment.relation.is_allowed() {
                return None;
            }
            Some(AnchorRunCandidate {
                run,
                relation: assessment.relation,
                visibility_rank: visibility_rank(run),
                gap_pt: assessment.boundary_distance_pt,
                overlap_depth_pt: assessment.overlap_depth_pt,
                width_pt: (run.bbox.x1 - run.bbox.x0).abs(),
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.relation
            .sort_rank()
            .cmp(&right.relation.sort_rank())
            .then_with(|| {
                left.gap_pt
                    .partial_cmp(&right.gap_pt)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.visibility_rank.cmp(&right.visibility_rank))
            .then_with(|| {
                left.overlap_depth_pt
                    .partial_cmp(&right.overlap_depth_pt)
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

fn assess_anchor_run_boundary(
    redaction: &RedactionOccurrence,
    run: &PdfFontTextRun,
    left_side: bool,
    overlap_tolerance_pt: f32,
) -> AnchorRunBoundaryAssessment {
    let overlap_tolerance_pt = overlap_tolerance_pt.max(0.0_f32);
    let boundary_geometry = boundary_run_geometry(run);
    if left_side {
        let boundary_edge_x_pt = boundary_geometry.inner_right_edge_x_pt;
        let far_edge_x_pt = boundary_geometry.inner_left_edge_x_pt;
        let redaction_edge_x_pt = redaction.bbox.x0;
        let boundary_distance_pt = (boundary_edge_x_pt - redaction_edge_x_pt).abs();
        let overlap_depth_pt = (boundary_edge_x_pt - redaction_edge_x_pt).max(0.0_f32);
        let relation = if boundary_edge_x_pt <= redaction_edge_x_pt {
            AnchorRunRelation::FullyOutside
        } else if far_edge_x_pt < redaction_edge_x_pt {
            if overlap_depth_pt <= overlap_tolerance_pt {
                AnchorRunRelation::SmallOverlap
            } else {
                AnchorRunRelation::DeepOverlap
            }
        } else if far_edge_x_pt < redaction.bbox.x1 {
            AnchorRunRelation::DeepOverlap
        } else {
            AnchorRunRelation::OppositeSide
        };
        return AnchorRunBoundaryAssessment {
            relation,
            boundary_distance_pt,
            overlap_depth_pt,
            overlap_tolerance_pt,
            boundary_geometry,
        };
    }
    let boundary_edge_x_pt = boundary_geometry.inner_left_edge_x_pt;
    let far_edge_x_pt = boundary_geometry.inner_right_edge_x_pt;
    let redaction_edge_x_pt = redaction.bbox.x1;
    let boundary_distance_pt = (boundary_edge_x_pt - redaction_edge_x_pt).abs();
    let overlap_depth_pt = (redaction_edge_x_pt - boundary_edge_x_pt).max(0.0_f32);
    let relation = if boundary_edge_x_pt >= redaction_edge_x_pt {
        AnchorRunRelation::FullyOutside
    } else if far_edge_x_pt > redaction_edge_x_pt {
        if overlap_depth_pt <= overlap_tolerance_pt {
            AnchorRunRelation::SmallOverlap
        } else {
            AnchorRunRelation::DeepOverlap
        }
    } else if far_edge_x_pt > redaction.bbox.x0 {
        AnchorRunRelation::DeepOverlap
    } else {
        AnchorRunRelation::OppositeSide
    };
    AnchorRunBoundaryAssessment {
        relation,
        boundary_distance_pt,
        overlap_depth_pt,
        overlap_tolerance_pt,
        boundary_geometry,
    }
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
    let left_anchor = resolution
        .left
        .as_ref()
        .map(|candidate| candidate.anchor.clone());
    let right_anchor = resolution
        .right
        .as_ref()
        .map(|candidate| candidate.anchor.clone());
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
    let geometry = compute_anchor_geometry(
        redaction.bbox,
        left_anchor.as_ref(),
        right_anchor.as_ref(),
        &measurement_model,
        line_bias_pt,
    );
    let usable_left_edge_x_pt = geometry.usable_left_edge_x_pt;
    let usable_right_edge_x_pt = geometry.usable_right_edge_x_pt;
    let target_width_pt = geometry.target_width_pt;
    let mut diagnostics = diagnostics;
    if collect_diagnostics {
        diagnostics.extend(build_line_bucket_diagnostics(
            location.clone(),
            redaction,
            line_buckets,
            &line_bucket_candidates,
            resolution
                .selected_line_bucket
                .map(|line_bucket| line_bucket.line_id.as_str()),
        ));
        diagnostics.extend(build_anchor_geometry_diagnostics(
            location.clone(),
            AnchorGeometryDiagnosticsInput {
                left_anchor: left_anchor.as_ref(),
                right_anchor: right_anchor.as_ref(),
                left_boundary_gap: geometry.left_boundary_gap,
                right_boundary_gap: geometry.right_boundary_gap,
                usable_left_edge_x_pt,
                usable_right_edge_x_pt,
                target_width_pt,
                redaction_width_pt: redaction.bbox.width().abs(),
            },
        ));
    }

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

fn compute_anchor_geometry(
    redaction_bbox: Rect,
    left_anchor: Option<&AnchorSide>,
    right_anchor: Option<&AnchorSide>,
    measurement_model: &CandidateWidthModel,
    line_bias_pt: f32,
) -> AnchorGeometryComputation {
    let left_boundary_gap = left_anchor.map(|anchor| {
        select_boundary_gap(
            anchor.trailing_whitespace_width_pt,
            true,
            measurement_model,
            line_bias_pt,
        )
    });
    let right_boundary_gap = right_anchor.map(|anchor| {
        select_boundary_gap(
            anchor.leading_whitespace_width_pt,
            false,
            measurement_model,
            line_bias_pt,
        )
    });
    let usable_left_edge_x_pt = left_anchor
        .zip(left_boundary_gap)
        .map(|(anchor, gap)| anchor.inner_right_edge_x_pt + gap.gap_pt);
    let usable_right_edge_x_pt = right_anchor
        .zip(right_boundary_gap)
        .map(|(anchor, gap)| anchor.inner_left_edge_x_pt - gap.gap_pt);
    let target_width_pt = match (usable_left_edge_x_pt, usable_right_edge_x_pt) {
        (Some(left_edge), Some(right_edge)) if right_edge > left_edge => right_edge - left_edge,
        _ => redaction_bbox.width().abs(),
    };
    AnchorGeometryComputation {
        usable_left_edge_x_pt,
        usable_right_edge_x_pt,
        target_width_pt,
        left_boundary_gap,
        right_boundary_gap,
    }
}

fn select_boundary_gap(
    explicit_gap_pt: f32,
    left_side: bool,
    _measurement_model: &CandidateWidthModel,
    _line_bias_pt: f32,
) -> BoundaryGapDecision {
    let explicit_gap_pt = explicit_gap_pt.max(0.0_f32);
    if explicit_gap_pt > f32::EPSILON {
        return BoundaryGapDecision {
            gap_pt: explicit_gap_pt,
            explicit_gap_pt,
            inferred_gap_pt: 0.0_f32,
            source: if left_side {
                BoundaryGapSource::ExplicitTrailingWhitespace
            } else {
                BoundaryGapSource::ExplicitLeadingWhitespace
            },
        };
    }
    BoundaryGapDecision {
        gap_pt: 0.0_f32,
        explicit_gap_pt: 0.0_f32,
        inferred_gap_pt: 0.0_f32,
        source: BoundaryGapSource::None,
    }
}

fn build_anchor_geometry_diagnostics(
    location: DiagnosticLocation,
    input: AnchorGeometryDiagnosticsInput<'_>,
) -> Vec<RedactionEvidenceDiagnostic> {
    let mut diagnostics = Vec::new();
    if let Some(anchor) = input.left_anchor {
        diagnostics.push(build_anchor_inner_edge_diagnostic(
            location.clone(),
            anchor,
            "left",
        ));
    }
    if let Some(anchor) = input.right_anchor {
        diagnostics.push(build_anchor_inner_edge_diagnostic(
            location.clone(),
            anchor,
            "right",
        ));
    }
    if let Some(gap) = input.left_boundary_gap {
        diagnostics.push(build_anchor_boundary_gap_diagnostic(
            location.clone(),
            "left",
            gap,
            input.left_anchor.map(|anchor| anchor.inner_right_edge_x_pt),
            input.usable_left_edge_x_pt,
        ));
    }
    if let Some(gap) = input.right_boundary_gap {
        diagnostics.push(build_anchor_boundary_gap_diagnostic(
            location.clone(),
            "right",
            gap,
            input.right_anchor.map(|anchor| anchor.inner_left_edge_x_pt),
            input.usable_right_edge_x_pt,
        ));
    }
    let mut metrics = BTreeMap::new();
    if let Some(usable_left_edge_x_pt) = input.usable_left_edge_x_pt {
        metrics.insert(
            "usable_left_edge_x_pt".to_owned(),
            DiagnosticValue::Float(usable_left_edge_x_pt as f64),
        );
    }
    if let Some(usable_right_edge_x_pt) = input.usable_right_edge_x_pt {
        metrics.insert(
            "usable_right_edge_x_pt".to_owned(),
            DiagnosticValue::Float(usable_right_edge_x_pt as f64),
        );
    }
    metrics.insert(
        "target_width_pt".to_owned(),
        DiagnosticValue::Float(input.target_width_pt as f64),
    );
    metrics.insert(
        "redaction_width_pt".to_owned(),
        DiagnosticValue::Float(input.redaction_width_pt as f64),
    );
    diagnostics.push(
        if matches!(
            (input.usable_left_edge_x_pt, input.usable_right_edge_x_pt),
            (Some(left), Some(right)) if right > left
        ) {
            build_diagnostic(
                location,
                "redaction_evidence",
                "anchor_target_width_computed",
                "computed target width from anchor inner edges",
                metrics,
            )
        } else {
            build_diagnostic(
                location,
                "redaction_evidence",
                "anchor_target_width_invalid",
                "anchor span was incomplete or non-positive; fell back to redaction box width",
                metrics,
            )
        },
    );
    diagnostics
}

fn build_anchor_inner_edge_diagnostic(
    location: DiagnosticLocation,
    anchor: &AnchorSide,
    side: &str,
) -> RedactionEvidenceDiagnostic {
    let mut metrics = BTreeMap::new();
    metrics.insert("side".to_owned(), DiagnosticValue::Text(side.to_owned()));
    metrics.insert(
        "anchor_text".to_owned(),
        DiagnosticValue::Text(anchor.text.clone()),
    );
    metrics.insert(
        "span_x0".to_owned(),
        DiagnosticValue::Float(anchor.bbox.x0 as f64),
    );
    metrics.insert(
        "span_x1".to_owned(),
        DiagnosticValue::Float(anchor.bbox.x1 as f64),
    );
    metrics.insert(
        "text_edge_x_pt".to_owned(),
        DiagnosticValue::Float(anchor.text_edge_x_pt as f64),
    );
    metrics.insert(
        "inner_left_edge_x_pt".to_owned(),
        DiagnosticValue::Float(anchor.inner_left_edge_x_pt as f64),
    );
    metrics.insert(
        "inner_right_edge_x_pt".to_owned(),
        DiagnosticValue::Float(anchor.inner_right_edge_x_pt as f64),
    );
    metrics.insert(
        "leading_whitespace_width_pt".to_owned(),
        DiagnosticValue::Float(anchor.leading_whitespace_width_pt as f64),
    );
    metrics.insert(
        "trailing_whitespace_width_pt".to_owned(),
        DiagnosticValue::Float(anchor.trailing_whitespace_width_pt as f64),
    );
    build_diagnostic(
        location,
        "redaction_evidence",
        "anchor_inner_edges_selected",
        "computed inner anchor edges from boundary-adjacent run geometry",
        metrics,
    )
}

fn build_anchor_boundary_gap_diagnostic(
    location: DiagnosticLocation,
    side: &str,
    gap: BoundaryGapDecision,
    anchor_inner_edge_x_pt: Option<f32>,
    usable_edge_x_pt: Option<f32>,
) -> RedactionEvidenceDiagnostic {
    let mut metrics = BTreeMap::new();
    metrics.insert("side".to_owned(), DiagnosticValue::Text(side.to_owned()));
    metrics.insert(
        "boundary_gap_pt".to_owned(),
        DiagnosticValue::Float(gap.gap_pt as f64),
    );
    metrics.insert(
        "explicit_boundary_gap_pt".to_owned(),
        DiagnosticValue::Float(gap.explicit_gap_pt as f64),
    );
    metrics.insert(
        "inferred_boundary_gap_pt".to_owned(),
        DiagnosticValue::Float(gap.inferred_gap_pt as f64),
    );
    metrics.insert(
        "boundary_gap_source".to_owned(),
        DiagnosticValue::Text(gap.source.as_str().to_owned()),
    );
    if let Some(anchor_inner_edge_x_pt) = anchor_inner_edge_x_pt {
        metrics.insert(
            "anchor_inner_edge_x_pt".to_owned(),
            DiagnosticValue::Float(anchor_inner_edge_x_pt as f64),
        );
    }
    if let Some(usable_edge_x_pt) = usable_edge_x_pt {
        metrics.insert(
            "usable_edge_x_pt".to_owned(),
            DiagnosticValue::Float(usable_edge_x_pt as f64),
        );
    }
    build_diagnostic(
        location,
        "redaction_evidence",
        "anchor_boundary_gap_selected",
        "selected boundary gap between anchor inner edge and hidden text span",
        metrics,
    )
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
    let mut pair_candidate_count = 0_usize;

    for (line_bucket_rank, candidate) in line_bucket_candidates.iter().enumerate() {
        let bucket = build_bucket_anchor_candidates(
            redaction,
            row_id,
            candidate,
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
                pair_candidate_count += 1;
                let pair_candidate = AnchorPairCandidate {
                    pair_id: format!("{}|{}", left.candidate_id, right.candidate_id),
                    line_bucket: bucket.line_bucket,
                    line_bucket_rank: bucket.line_bucket_rank,
                    baseline_eligible: bucket.baseline_eligible,
                    overlap_eligible: bucket.overlap_eligible,
                    left_candidate_count: bucket.left_candidates.len(),
                    right_candidate_count: bucket.right_candidates.len(),
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
                if collect_diagnostics {
                    diagnostics.push(build_diagnostic(
                        location.clone(),
                        "redaction_evidence",
                        "anchor_pair_candidate_valid",
                        "anchor pair candidate passed pair validity checks",
                        pair_candidate_metrics(&pair_candidate, line_bucket_candidates.len()),
                    ));
                }
                valid_pairs.push(pair_candidate);
            }
        }
    }

    if !valid_pairs.is_empty() {
        let mut filtered_pairs = Vec::<AnchorPairCandidate<'a>>::new();
        let mut rejected_nonlocal_pair_count = 0_usize;
        for pair_candidate in valid_pairs {
            if pair_is_nonlocal_overlap_only_bucket(&pair_candidate, redaction) {
                rejected_nonlocal_pair_count += 1;
                if collect_diagnostics {
                    diagnostics.push(build_diagnostic(
                        location.clone(),
                        "redaction_evidence",
                        "anchor_pair_rejected_overlap_only_nonlocal_bucket",
                        "rejected two-sided pair because it came from an overlap-only bucket with a nonlocal gap profile",
                        pair_box_metrics(&pair_candidate, redaction, line_bucket_candidates.len()),
                    ));
                }
                continue;
            }
            filtered_pairs.push(pair_candidate);
        }
        if collect_diagnostics && rejected_nonlocal_pair_count > 0 {
            let mut metrics = anchor_pair_pool_metrics(
                line_bucket_candidates.len(),
                pair_candidate_count,
                filtered_pairs.len(),
            );
            metrics.insert(
                "rejected_nonlocal_pair_count".to_owned(),
                DiagnosticValue::Integer(rejected_nonlocal_pair_count as i64),
            );
            diagnostics.push(build_diagnostic(
                location.clone(),
                "redaction_evidence",
                "anchor_pair_pool_post_nonlocal_filter_summary",
                "summarized pair-candidate availability after nonlocal overlap-bucket rejection",
                metrics,
            ));
        }
        valid_pairs = filtered_pairs;
    }

    if collect_diagnostics {
        diagnostics.push(build_anchor_pair_pool_summary_diagnostic(
            location.clone(),
            line_bucket_candidates.len(),
            pair_candidate_count,
            valid_pairs.len(),
        ));
        if pair_candidate_count == 0 || valid_pairs.is_empty() {
            diagnostics.push(build_diagnostic(
                location.clone(),
                "redaction_evidence",
                "anchor_pair_pool_empty",
                "no valid two-sided anchor pair was available for the row",
                anchor_pair_pool_metrics(
                    line_bucket_candidates.len(),
                    pair_candidate_count,
                    valid_pairs.len(),
                ),
            ));
        }
    }

    valid_pairs.sort_by(compare_pair_candidates);
    if let Some(initial_selected_pair) = valid_pairs.first().cloned() {
        let (selected_pair, box_override_applied) = select_box_sane_pair(
            redaction,
            &valid_pairs,
            &initial_selected_pair,
            location.clone(),
            line_bucket_candidates.len(),
            collect_diagnostics,
            &mut diagnostics,
        );
        if collect_diagnostics {
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
                if box_override_applied {
                    "pair_candidate_selected_box_sanity_override"
                } else {
                    "pair_candidate_selected"
                },
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
                selection_reason: if box_override_applied {
                    "pair_candidate_selected_box_sanity_override".to_owned()
                } else {
                    "pair_candidate_selected".to_owned()
                },
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
    line_bucket_candidate: &'a LineBucketCandidate<'a>,
    line_bucket_rank: usize,
    location: DiagnosticLocation,
    collect_diagnostics: bool,
) -> (BucketAnchorCandidates<'a>, Vec<RedactionEvidenceDiagnostic>) {
    let line_bucket = line_bucket_candidate.line_bucket;
    let (left_candidates, left_run_candidate_count, mut diagnostics) =
        build_anchor_span_candidates(AnchorSpanBuildRequest {
            redaction,
            row_id,
            line_bucket,
            line_bucket_rank,
            left_side: true,
            location: location.clone(),
            collect_diagnostics,
            overlap_tolerance_pt: ANCHOR_BOUNDARY_SMALL_OVERLAP_PT,
        });
    let (right_candidates, right_run_candidate_count, right_diagnostics) =
        build_anchor_span_candidates(AnchorSpanBuildRequest {
            redaction,
            row_id,
            line_bucket,
            line_bucket_rank,
            left_side: false,
            location: location.clone(),
            collect_diagnostics,
            overlap_tolerance_pt: ANCHOR_BOUNDARY_SMALL_OVERLAP_PT,
        });
    diagnostics.extend(right_diagnostics);
    let summary = BucketCandidateSummary {
        left_run_candidate_count,
        right_run_candidate_count,
        left_span_candidate_count: left_candidates.len(),
        right_span_candidate_count: right_candidates.len(),
    };
    if collect_diagnostics {
        diagnostics.extend(build_bucket_candidate_diagnostics(
            location.clone(),
            redaction,
            line_bucket,
            line_bucket_rank,
            &summary,
        ));
    }
    (
        BucketAnchorCandidates {
            line_bucket,
            line_bucket_rank,
            baseline_eligible: line_bucket_candidate.baseline_eligible,
            overlap_eligible: line_bucket_candidate.overlap_eligible,
            left_candidates,
            right_candidates,
        },
        diagnostics,
    )
}

fn build_anchor_span_candidates<'a, 'b>(
    req: AnchorSpanBuildRequest<'a, 'b>,
) -> (
    Vec<AnchorSpanCandidate<'a>>,
    usize,
    Vec<RedactionEvidenceDiagnostic>,
) {
    let run_candidates = ranked_anchor_run_candidates(
        req.redaction,
        &req.line_bucket.runs,
        req.left_side,
        req.overlap_tolerance_pt,
    );
    let mut candidates = Vec::<AnchorSpanCandidate<'a>>::new();
    let mut diagnostics = build_anchor_run_diagnostics(&req, &run_candidates);

    for (candidate_rank, run_candidate) in run_candidates.iter().enumerate() {
        let span_candidate = build_anchor_span_candidate(
            req.row_id,
            req.line_bucket,
            req.line_bucket_rank,
            candidate_rank,
            run_candidate,
            req.left_side,
        );
        if req.collect_diagnostics {
            diagnostics.push(build_diagnostic(
                req.location.clone(),
                "redaction_evidence",
                "anchor_span_candidate_considered",
                "considered anchor span candidate",
                anchor_span_metrics(
                    &span_candidate,
                    run_candidates.len(),
                    candidate_rank,
                    req.redaction,
                ),
            ));
        }
        let allow_boundary_punctuation =
            anchor_text_is_boundary_punctuation(&span_candidate.anchor.text)
                && candidate_rank == 0
                && span_candidate.relation == AnchorRunRelation::FullyOutside
                && span_candidate.gap_pt <= ANCHOR_BOUNDARY_PUNCTUATION_GAP_MAX_PT;
        if !anchor_text_has_alnum(&span_candidate.anchor.text) && !allow_boundary_punctuation {
            if req.collect_diagnostics {
                diagnostics.push(build_diagnostic(
                    req.location.clone(),
                    "redaction_evidence",
                    "anchor_span_rejected_non_alnum",
                    "anchor span text contains no unicode letters or digits",
                    anchor_span_metrics(
                        &span_candidate,
                        run_candidates.len(),
                        candidate_rank,
                        req.redaction,
                    ),
                ));
            }
            continue;
        }
        if allow_boundary_punctuation && req.collect_diagnostics {
            diagnostics.push(build_diagnostic(
                req.location.clone(),
                "redaction_evidence",
                "anchor_span_allowed_boundary_punctuation",
                "allowed a punctuation-only boundary token because it was the nearest fully-outside candidate and almost flush with the redaction edge",
                anchor_span_metrics(
                    &span_candidate,
                    run_candidates.len(),
                    candidate_rank,
                    req.redaction,
                ),
            ));
        }
        candidates.push(span_candidate);
    }

    (candidates, run_candidates.len(), diagnostics)
}

fn build_anchor_run_diagnostics(
    req: &AnchorSpanBuildRequest<'_, '_>,
    run_candidates: &[AnchorRunCandidate<'_>],
) -> Vec<RedactionEvidenceDiagnostic> {
    if !req.collect_diagnostics {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    for run in &req.line_bucket.runs {
        let trimmed = run.text.trim();
        let assessment =
            assess_anchor_run_boundary(req.redaction, run, req.left_side, req.overlap_tolerance_pt);
        if trimmed.is_empty() {
            diagnostics.push(build_diagnostic(
                req.location.clone(),
                "redaction_evidence",
                "anchor_run_rejected_empty_text",
                "anchor run was rejected because trimmed text is empty",
                anchor_run_metrics(AnchorRunMetricsInput {
                    run,
                    left_side: req.left_side,
                    line_bucket: req.line_bucket,
                    line_bucket_rank: req.line_bucket_rank,
                    redaction: req.redaction,
                    candidate_count: run_candidates.len(),
                    candidate_rank: None,
                    assessment,
                }),
            ));
            continue;
        }
        if matches!(assessment.relation, AnchorRunRelation::DeepOverlap) {
            diagnostics.push(build_diagnostic(
                req.location.clone(),
                "redaction_evidence",
                "anchor_run_rejected_deep_overlap",
                "anchor run was rejected because it overlaps too deeply into the redaction",
                anchor_run_metrics(AnchorRunMetricsInput {
                    run,
                    left_side: req.left_side,
                    line_bucket: req.line_bucket,
                    line_bucket_rank: req.line_bucket_rank,
                    redaction: req.redaction,
                    candidate_count: run_candidates.len(),
                    candidate_rank: None,
                    assessment,
                }),
            ));
        }
        if matches!(assessment.relation, AnchorRunRelation::OppositeSide) {
            diagnostics.push(build_diagnostic(
                req.location.clone(),
                "redaction_evidence",
                "anchor_run_rejected_opposite_side",
                "anchor run was rejected because its inner boundary edge is on the opposite side of the redaction",
                anchor_run_metrics(AnchorRunMetricsInput {
                    run,
                    left_side: req.left_side,
                    line_bucket: req.line_bucket,
                    line_bucket_rank: req.line_bucket_rank,
                    redaction: req.redaction,
                    candidate_count: run_candidates.len(),
                    candidate_rank: None,
                    assessment,
                }),
            ));
        }
        if !assessment.relation.is_allowed() {
            diagnostics.push(build_diagnostic(
                req.location.clone(),
                "redaction_evidence",
                "anchor_run_rejected_wrong_side_of_redaction",
                "anchor run was rejected because it was not admissible for the requested side",
                anchor_run_metrics(AnchorRunMetricsInput {
                    run,
                    left_side: req.left_side,
                    line_bucket: req.line_bucket,
                    line_bucket_rank: req.line_bucket_rank,
                    redaction: req.redaction,
                    candidate_count: run_candidates.len(),
                    candidate_rank: None,
                    assessment,
                }),
            ));
        }
    }
    for (candidate_rank, candidate) in run_candidates.iter().enumerate() {
        let metrics = anchor_run_metrics(AnchorRunMetricsInput {
            run: candidate.run,
            left_side: req.left_side,
            line_bucket: req.line_bucket,
            line_bucket_rank: req.line_bucket_rank,
            redaction: req.redaction,
            candidate_count: run_candidates.len(),
            candidate_rank: Some(candidate_rank),
            assessment: assess_anchor_run_boundary(
                req.redaction,
                candidate.run,
                req.left_side,
                req.overlap_tolerance_pt,
            ),
        });
        diagnostics.push(build_diagnostic(
            req.location.clone(),
            "redaction_evidence",
            "anchor_run_candidate_considered",
            "considered boundary-adjacent text run for anchor construction",
            metrics.clone(),
        ));
        if candidate.relation == AnchorRunRelation::SmallOverlap {
            diagnostics.push(build_diagnostic(
                req.location.clone(),
                "redaction_evidence",
                "anchor_run_selected_small_overlap",
                "selected a boundary-local run that slightly overlaps the redaction",
                metrics,
            ));
        }
    }
    diagnostics
}

fn build_bucket_candidate_diagnostics(
    location: DiagnosticLocation,
    redaction: &RedactionOccurrence,
    line_bucket: &LineBucket<'_>,
    line_bucket_rank: usize,
    summary: &BucketCandidateSummary,
) -> Vec<RedactionEvidenceDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut metrics = line_bucket_metrics(line_bucket, redaction, 0, line_bucket_rank as i64);
    metrics.insert(
        "left_run_candidate_count".to_owned(),
        DiagnosticValue::Integer(summary.left_run_candidate_count as i64),
    );
    metrics.insert(
        "right_run_candidate_count".to_owned(),
        DiagnosticValue::Integer(summary.right_run_candidate_count as i64),
    );
    metrics.insert(
        "left_span_candidate_count".to_owned(),
        DiagnosticValue::Integer(summary.left_span_candidate_count as i64),
    );
    metrics.insert(
        "right_span_candidate_count".to_owned(),
        DiagnosticValue::Integer(summary.right_span_candidate_count as i64),
    );
    diagnostics.push(build_diagnostic(
        DiagnosticLocation {
            row_id: location.row_id.clone(),
            redaction_id: location.redaction_id.clone(),
            page_index: location.page_index,
            bbox: location.bbox,
        },
        "redaction_evidence",
        "anchor_bucket_candidate_summary",
        "summarized anchor candidates available in this line bucket",
        metrics.clone(),
    ));
    if summary.left_span_candidate_count == 0 {
        diagnostics.push(build_diagnostic(
            DiagnosticLocation {
                row_id: location.row_id.clone(),
                redaction_id: location.redaction_id.clone(),
                page_index: location.page_index,
                bbox: location.bbox,
            },
            "redaction_evidence",
            "anchor_bucket_missing_left_candidates",
            "line bucket produced no valid left-side anchor span candidates",
            metrics.clone(),
        ));
    }
    if summary.right_span_candidate_count == 0 {
        diagnostics.push(build_diagnostic(
            location,
            "redaction_evidence",
            "anchor_bucket_missing_right_candidates",
            "line bucket produced no valid right-side anchor span candidates",
            metrics,
        ));
    }
    diagnostics
}

fn build_line_bucket_diagnostics(
    location: DiagnosticLocation,
    redaction: &RedactionOccurrence,
    line_buckets: Option<&[LineBucket<'_>]>,
    candidates: &[LineBucketCandidate<'_>],
    selected_line_id: Option<&str>,
) -> Vec<RedactionEvidenceDiagnostic> {
    let Some(line_buckets) = line_buckets else {
        return vec![build_diagnostic(
            location,
            "redaction_evidence",
            "line_bucket_pool_empty",
            "page had no non-empty text buckets available for anchor resolution",
            BTreeMap::new(),
        )];
    };
    let mut diagnostics = Vec::new();
    let eligible_candidates = candidates
        .iter()
        .map(|candidate| (candidate.line_bucket.line_id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    for line_bucket in line_buckets {
        if let Some(candidate) = eligible_candidates
            .get(line_bucket.line_id.as_str())
            .copied()
        {
            let candidate_rank = candidates
                .iter()
                .position(|entry| entry.line_bucket.line_id == line_bucket.line_id)
                .unwrap_or_default();
            let mut metrics = line_bucket_metrics(
                line_bucket,
                redaction,
                candidates.len() as i64,
                candidate_rank as i64,
            );
            metrics.insert(
                "bucket_baseline_eligible".to_owned(),
                DiagnosticValue::Bool(candidate.baseline_eligible),
            );
            metrics.insert(
                "bucket_overlap_eligible".to_owned(),
                DiagnosticValue::Bool(candidate.overlap_eligible),
            );
            metrics.insert(
                "bucket_overlap_only".to_owned(),
                DiagnosticValue::Bool(candidate.overlap_eligible && !candidate.baseline_eligible),
            );
            diagnostics.push(build_diagnostic(
                location.clone(),
                "redaction_evidence",
                "line_bucket_candidate_considered",
                "considered same-line text bucket for redaction",
                metrics.clone(),
            ));
            if let Some(split_gap_pt) = line_bucket.split_from_previous_horizontal_gap_pt {
                let mut split_metrics = metrics;
                split_metrics.insert(
                    "split_horizontal_gap_pt".to_owned(),
                    DiagnosticValue::Float(split_gap_pt as f64),
                );
                diagnostics.push(build_diagnostic(
                    location.clone(),
                    "redaction_evidence",
                    "line_bucket_split_horizontal_gap",
                    "started a new same-baseline line bucket because the horizontal gap from the previous cluster was too large",
                    split_metrics,
                ));
            }
        } else {
            let metrics = line_bucket_metrics(line_bucket, redaction, candidates.len() as i64, -1);
            diagnostics.push(build_diagnostic(
                location.clone(),
                "redaction_evidence",
                "line_bucket_rejected_not_same_line",
                "line bucket was rejected because it was neither overlapping nor baseline-close",
                metrics.clone(),
            ));
            if let Some(split_gap_pt) = line_bucket.split_from_previous_horizontal_gap_pt {
                let mut split_metrics = metrics;
                split_metrics.insert(
                    "split_horizontal_gap_pt".to_owned(),
                    DiagnosticValue::Float(split_gap_pt as f64),
                );
                diagnostics.push(build_diagnostic(
                    location.clone(),
                    "redaction_evidence",
                    "line_bucket_split_horizontal_gap",
                    "started a new same-baseline line bucket because the horizontal gap from the previous cluster was too large",
                    split_metrics,
                ));
            }
        }
    }
    if candidates.is_empty() {
        diagnostics.push(build_diagnostic(
            DiagnosticLocation {
                row_id: location.row_id.clone(),
                redaction_id: location.redaction_id.clone(),
                page_index: location.page_index,
                bbox: location.bbox,
            },
            "redaction_evidence",
            "line_bucket_pool_no_eligible_candidates",
            "no line bucket satisfied the anchor-resolution same-line eligibility rule",
            BTreeMap::new(),
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
                    candidates.len() as i64,
                    selected_rank as i64,
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
        relation: run_candidate.relation,
        visibility_rank: run_candidate.visibility_rank,
        gap_pt: run_candidate.gap_pt,
        overlap_depth_pt: run_candidate.overlap_depth_pt,
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
        left.relation.sort_rank(),
        ordered_f32(left.gap_pt),
        left.visibility_rank,
        ordered_f32(left.overlap_depth_pt),
        ordered_f32(left.width_pt),
        left.candidate_id.as_str(),
    )
        .cmp(&(
            right.line_bucket_rank,
            right.relation.sort_rank(),
            ordered_f32(right.gap_pt),
            right.visibility_rank,
            ordered_f32(right.overlap_depth_pt),
            ordered_f32(right.width_pt),
            right.candidate_id.as_str(),
        ))
}

fn pair_target_width_pt(pair: &AnchorPairCandidate<'_>) -> Option<f32> {
    let left_gap_pt = pair.left.anchor.trailing_whitespace_width_pt.max(0.0_f32);
    let right_gap_pt = pair.right.anchor.leading_whitespace_width_pt.max(0.0_f32);
    let usable_left_edge_x_pt = pair.left.anchor.inner_right_edge_x_pt + left_gap_pt;
    let usable_right_edge_x_pt = pair.right.anchor.inner_left_edge_x_pt - right_gap_pt;
    (usable_right_edge_x_pt > usable_left_edge_x_pt)
        .then_some(usable_right_edge_x_pt - usable_left_edge_x_pt)
}

fn pair_is_nonlocal_overlap_only_bucket(
    pair: &AnchorPairCandidate<'_>,
    redaction: &RedactionOccurrence,
) -> bool {
    if !pair.overlap_eligible || pair.baseline_eligible {
        return false;
    }
    let Some(pair_target_width_pt) = pair_target_width_pt(pair) else {
        return false;
    };
    let redaction_width_pt = redaction.bbox.width().abs();
    if pair_target_width_pt <= redaction_width_pt {
        return false;
    }
    let pair_box_delta_pt = (pair_target_width_pt - redaction_width_pt).abs();
    let min_box_delta_pt = ANCHOR_NONLOCAL_PAIR_BOX_DELTA_PT
        .max(redaction_width_pt * ANCHOR_NONLOCAL_PAIR_BOX_DELTA_RATIO);
    if pair_box_delta_pt < min_box_delta_pt {
        return false;
    }
    let pair_gap_max_pt = pair.left.gap_pt.max(pair.right.gap_pt);
    let pair_gap_diff_pt = (pair.left.gap_pt - pair.right.gap_pt).abs();
    pair_gap_max_pt > ANCHOR_NONLOCAL_PAIR_MAX_GAP_PT
        || pair_gap_diff_pt > ANCHOR_NONLOCAL_PAIR_GAP_DIFF_PT
}

fn pair_box_metrics(
    pair: &AnchorPairCandidate<'_>,
    redaction: &RedactionOccurrence,
    line_bucket_count: usize,
) -> BTreeMap<String, DiagnosticValue> {
    let mut metrics = pair_candidate_metrics(pair, line_bucket_count);
    let redaction_width_pt = redaction.bbox.width().abs();
    metrics.insert(
        "bucket_baseline_eligible".to_owned(),
        DiagnosticValue::Bool(pair.baseline_eligible),
    );
    metrics.insert(
        "bucket_overlap_eligible".to_owned(),
        DiagnosticValue::Bool(pair.overlap_eligible),
    );
    metrics.insert(
        "bucket_overlap_only".to_owned(),
        DiagnosticValue::Bool(pair.overlap_eligible && !pair.baseline_eligible),
    );
    metrics.insert(
        "left_candidate_count".to_owned(),
        DiagnosticValue::Integer(pair.left_candidate_count as i64),
    );
    metrics.insert(
        "right_candidate_count".to_owned(),
        DiagnosticValue::Integer(pair.right_candidate_count as i64),
    );
    metrics.insert(
        "pair_gap_diff_pt".to_owned(),
        DiagnosticValue::Float((pair.left.gap_pt - pair.right.gap_pt).abs() as f64),
    );
    if let Some(pair_target_width_pt) = pair_target_width_pt(pair) {
        metrics.insert(
            "pair_target_width_pt".to_owned(),
            DiagnosticValue::Float(pair_target_width_pt as f64),
        );
        metrics.insert(
            "pair_box_delta_pt".to_owned(),
            DiagnosticValue::Float((pair_target_width_pt - redaction_width_pt).abs() as f64),
        );
    }
    metrics.insert(
        "redaction_width_pt".to_owned(),
        DiagnosticValue::Float(redaction_width_pt as f64),
    );
    metrics
}

fn select_box_sane_pair<'a>(
    redaction: &RedactionOccurrence,
    valid_pairs: &[AnchorPairCandidate<'a>],
    initial_selected_pair: &AnchorPairCandidate<'a>,
    location: DiagnosticLocation,
    line_bucket_count: usize,
    collect_diagnostics: bool,
    diagnostics: &mut Vec<RedactionEvidenceDiagnostic>,
) -> (AnchorPairCandidate<'a>, bool) {
    let Some(initial_target_width_pt) = pair_target_width_pt(initial_selected_pair) else {
        if collect_diagnostics {
            diagnostics.push(build_diagnostic(
                location,
                "redaction_evidence",
                "anchor_pair_selected_without_box_override",
                "selected anchor pair without box-sanity override because pair target width was unavailable",
                pair_box_metrics(initial_selected_pair, redaction, line_bucket_count),
            ));
        }
        return (initial_selected_pair.clone(), false);
    };
    let redaction_width_pt = redaction.bbox.width().abs();
    let initial_delta_pt = (initial_target_width_pt - redaction_width_pt).abs();
    if collect_diagnostics {
        diagnostics.push(build_diagnostic(
            location.clone(),
            "redaction_evidence",
            "anchor_pair_span_box_delta_computed",
            "computed selected pair delta against the redaction box width",
            pair_box_metrics(initial_selected_pair, redaction, line_bucket_count),
        ));
    }
    if initial_target_width_pt <= redaction_width_pt {
        if collect_diagnostics {
            diagnostics.push(build_diagnostic(
                location,
                "redaction_evidence",
                "anchor_pair_selected_without_box_override",
                "selected anchor pair without box-sanity override because the span was not suspiciously wide",
                pair_box_metrics(initial_selected_pair, redaction, line_bucket_count),
            ));
        }
        return (initial_selected_pair.clone(), false);
    }
    if collect_diagnostics {
        diagnostics.push(build_diagnostic(
            location.clone(),
            "redaction_evidence",
            "anchor_pair_span_box_delta_suspect",
            "selected anchor pair span was suspiciously wider than the redaction box",
            pair_box_metrics(initial_selected_pair, redaction, line_bucket_count),
        ));
    }
    let replacement = valid_pairs
        .iter()
        .filter_map(|candidate| {
            let candidate_target_width_pt = pair_target_width_pt(candidate)?;
            let candidate_delta_pt = (candidate_target_width_pt - redaction_width_pt).abs();
            (candidate_delta_pt < initial_delta_pt).then_some((
                candidate.clone(),
                candidate_delta_pt,
                initial_delta_pt - candidate_delta_pt,
            ))
        })
        .min_by(|left, right| {
            (
                ordered_f32(left.1),
                left.0.line_bucket_rank,
                left.0.pair_id.as_str(),
            )
                .cmp(&(
                    ordered_f32(right.1),
                    right.0.line_bucket_rank,
                    right.0.pair_id.as_str(),
                ))
        });
    if let Some((replacement_pair, replacement_delta_pt, improvement_pt)) = replacement {
        if collect_diagnostics {
            let mut metrics = pair_box_metrics(&replacement_pair, redaction, line_bucket_count);
            metrics.insert(
                "replaced_pair_id".to_owned(),
                DiagnosticValue::Text(initial_selected_pair.pair_id.clone()),
            );
            metrics.insert(
                "replaced_pair_box_delta_pt".to_owned(),
                DiagnosticValue::Float(initial_delta_pt as f64),
            );
            metrics.insert(
                "replacement_improvement_pt".to_owned(),
                DiagnosticValue::Float(improvement_pt as f64),
            );
            metrics.insert(
                "replacement_pair_box_delta_pt".to_owned(),
                DiagnosticValue::Float(replacement_delta_pt as f64),
            );
            diagnostics.push(build_diagnostic(
                location,
                "redaction_evidence",
                "anchor_pair_selected_box_sanity_override",
                "selected a different valid pair because the original pair was suspiciously wider than the redaction box",
                metrics,
            ));
        }
        return (replacement_pair, true);
    }
    if collect_diagnostics {
        diagnostics.push(build_diagnostic(
            location,
            "redaction_evidence",
            "anchor_pair_selected_without_box_override",
            "selected anchor pair without box-sanity override because no replacement improved the box delta enough",
            pair_box_metrics(initial_selected_pair, redaction, line_bucket_count),
        ));
    }
    (initial_selected_pair.clone(), false)
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

fn anchor_pair_pool_metrics(
    line_bucket_count: usize,
    pair_candidate_count: usize,
    valid_pair_count: usize,
) -> BTreeMap<String, DiagnosticValue> {
    let mut metrics = BTreeMap::new();
    metrics.insert(
        "line_bucket_count".to_owned(),
        DiagnosticValue::Integer(line_bucket_count as i64),
    );
    metrics.insert(
        "pair_candidate_count".to_owned(),
        DiagnosticValue::Integer(pair_candidate_count as i64),
    );
    metrics.insert(
        "valid_pair_count".to_owned(),
        DiagnosticValue::Integer(valid_pair_count as i64),
    );
    metrics
}

fn build_anchor_pair_pool_summary_diagnostic(
    location: DiagnosticLocation,
    line_bucket_count: usize,
    pair_candidate_count: usize,
    valid_pair_count: usize,
) -> RedactionEvidenceDiagnostic {
    build_diagnostic(
        location,
        "redaction_evidence",
        "anchor_pair_pool_summary",
        "summarized pair-candidate availability for the row",
        anchor_pair_pool_metrics(line_bucket_count, pair_candidate_count, valid_pair_count),
    )
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
        "bucket_baseline_eligible".to_owned(),
        DiagnosticValue::Bool(pair.baseline_eligible),
    );
    metrics.insert(
        "bucket_overlap_eligible".to_owned(),
        DiagnosticValue::Bool(pair.overlap_eligible),
    );
    metrics.insert(
        "bucket_overlap_only".to_owned(),
        DiagnosticValue::Bool(pair.overlap_eligible && !pair.baseline_eligible),
    );
    metrics.insert(
        "left_candidate_count".to_owned(),
        DiagnosticValue::Integer(pair.left_candidate_count as i64),
    );
    metrics.insert(
        "right_candidate_count".to_owned(),
        DiagnosticValue::Integer(pair.right_candidate_count as i64),
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
        "relation".to_owned(),
        DiagnosticValue::Text(candidate.relation.as_str().to_owned()),
    );
    metrics.insert(
        "relation_rank".to_owned(),
        DiagnosticValue::Integer(candidate.relation.sort_rank() as i64),
    );
    metrics.insert(
        "visibility_rank".to_owned(),
        DiagnosticValue::Integer(candidate.visibility_rank as i64),
    );
    metrics.insert(
        "overlap_depth_pt".to_owned(),
        DiagnosticValue::Float(candidate.overlap_depth_pt as f64),
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
        "boundary_run_x0".to_owned(),
        DiagnosticValue::Float(candidate.run.bbox.x0 as f64),
    );
    metrics.insert(
        "boundary_run_x1".to_owned(),
        DiagnosticValue::Float(candidate.run.bbox.x1 as f64),
    );
    metrics.insert(
        "boundary_run_width_pt".to_owned(),
        DiagnosticValue::Float((candidate.run.bbox.x1 - candidate.run.bbox.x0).abs() as f64),
    );
    metrics.insert(
        "text_edge_x_pt".to_owned(),
        DiagnosticValue::Float(candidate.anchor.text_edge_x_pt as f64),
    );
    metrics.insert(
        "inner_left_edge_x_pt".to_owned(),
        DiagnosticValue::Float(candidate.anchor.inner_left_edge_x_pt as f64),
    );
    metrics.insert(
        "inner_right_edge_x_pt".to_owned(),
        DiagnosticValue::Float(candidate.anchor.inner_right_edge_x_pt as f64),
    );
    metrics.insert(
        "leading_whitespace_width_pt".to_owned(),
        DiagnosticValue::Float(candidate.anchor.leading_whitespace_width_pt as f64),
    );
    metrics.insert(
        "trailing_whitespace_width_pt".to_owned(),
        DiagnosticValue::Float(candidate.anchor.trailing_whitespace_width_pt as f64),
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

fn anchor_text_is_boundary_punctuation(text: &str) -> bool {
    let normalized = normalize_transport_text(text);
    let trimmed = normalized.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_punctuation() && !ch.is_alphanumeric())
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
    candidate_count: i64,
    candidate_rank: i64,
) -> BTreeMap<String, DiagnosticValue> {
    let (bucket_left_x_pt, bucket_right_x_pt) = line_bucket_span(line_bucket);
    let left_candidates = ranked_anchor_run_candidates(
        redaction,
        &line_bucket.runs,
        true,
        ANCHOR_BOUNDARY_SMALL_OVERLAP_PT,
    );
    let right_candidates = ranked_anchor_run_candidates(
        redaction,
        &line_bucket.runs,
        false,
        ANCHOR_BOUNDARY_SMALL_OVERLAP_PT,
    );
    let mut metrics = BTreeMap::new();
    metrics.insert(
        "candidate_count".to_owned(),
        DiagnosticValue::Integer(candidate_count),
    );
    metrics.insert(
        "candidate_rank".to_owned(),
        DiagnosticValue::Integer(candidate_rank),
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
        "bucket_rightmost_x1".to_owned(),
        DiagnosticValue::Float(line_bucket.rightmost_x1 as f64),
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
    if let Some(split_gap_pt) = line_bucket.split_from_previous_horizontal_gap_pt {
        metrics.insert(
            "split_horizontal_gap_pt".to_owned(),
            DiagnosticValue::Float(split_gap_pt as f64),
        );
    }
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

fn anchor_run_metrics(input: AnchorRunMetricsInput<'_>) -> BTreeMap<String, DiagnosticValue> {
    let mut metrics = BTreeMap::new();
    let side = if input.left_side { "left" } else { "right" };
    metrics.insert(
        "stable_candidate_id".to_owned(),
        DiagnosticValue::Text(format!(
            "{}:{}:{}",
            input.line_bucket.line_id,
            side,
            anchor_run_sort_key(input.run)
        )),
    );
    metrics.insert(
        "anchor_side".to_owned(),
        DiagnosticValue::Text(side.to_owned()),
    );
    metrics.insert(
        "line_id".to_owned(),
        DiagnosticValue::Text(input.line_bucket.line_id.clone()),
    );
    metrics.insert(
        "line_bucket_rank".to_owned(),
        DiagnosticValue::Integer(input.line_bucket_rank as i64),
    );
    metrics.insert(
        "candidate_count".to_owned(),
        DiagnosticValue::Integer(input.candidate_count as i64),
    );
    if let Some(candidate_rank) = input.candidate_rank {
        metrics.insert(
            "candidate_rank".to_owned(),
            DiagnosticValue::Integer(candidate_rank as i64),
        );
    }
    metrics.insert(
        "run_text".to_owned(),
        DiagnosticValue::Text(input.run.text.clone()),
    );
    metrics.insert(
        "run_text_trimmed".to_owned(),
        DiagnosticValue::Text(input.run.text.trim().to_owned()),
    );
    metrics.insert(
        "run_allowed_for_side".to_owned(),
        DiagnosticValue::Bool(input.assessment.relation.is_allowed()),
    );
    metrics.insert(
        "run_gap_pt".to_owned(),
        DiagnosticValue::Float(input.assessment.boundary_distance_pt as f64),
    );
    metrics.insert(
        "run_relation".to_owned(),
        DiagnosticValue::Text(input.assessment.relation.as_str().to_owned()),
    );
    metrics.insert(
        "run_relation_rank".to_owned(),
        DiagnosticValue::Integer(input.assessment.relation.sort_rank() as i64),
    );
    metrics.insert(
        "run_overlap_depth_pt".to_owned(),
        DiagnosticValue::Float(input.assessment.overlap_depth_pt as f64),
    );
    metrics.insert(
        "run_overlap_tolerance_pt".to_owned(),
        DiagnosticValue::Float(input.assessment.overlap_tolerance_pt as f64),
    );
    metrics.insert(
        "run_boundary_inner_left_edge_x_pt".to_owned(),
        DiagnosticValue::Float(input.assessment.boundary_geometry.inner_left_edge_x_pt as f64),
    );
    metrics.insert(
        "run_boundary_inner_right_edge_x_pt".to_owned(),
        DiagnosticValue::Float(input.assessment.boundary_geometry.inner_right_edge_x_pt as f64),
    );
    metrics.insert(
        "run_visibility_rank".to_owned(),
        DiagnosticValue::Integer(visibility_rank(input.run) as i64),
    );
    metrics.insert(
        "run_x0".to_owned(),
        DiagnosticValue::Float(input.run.bbox.x0 as f64),
    );
    metrics.insert(
        "run_x1".to_owned(),
        DiagnosticValue::Float(input.run.bbox.x1 as f64),
    );
    metrics.insert(
        "run_y0".to_owned(),
        DiagnosticValue::Float(input.run.bbox.y0 as f64),
    );
    metrics.insert(
        "run_y1".to_owned(),
        DiagnosticValue::Float(input.run.bbox.y1 as f64),
    );
    metrics.insert(
        "run_width_pt".to_owned(),
        DiagnosticValue::Float((input.run.bbox.x1 - input.run.bbox.x0).abs() as f64),
    );
    metrics.insert(
        "redaction_left_x_pt".to_owned(),
        DiagnosticValue::Float(input.redaction.bbox.x0 as f64),
    );
    metrics.insert(
        "redaction_right_x_pt".to_owned(),
        DiagnosticValue::Float(input.redaction.bbox.x1 as f64),
    );
    extend_metrics(&mut metrics, width_profile_metrics("run", input.run));
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
    let boundary_geometry = boundary_run_geometry(run);
    AnchorSide {
        anchor_id: if left_side {
            format!("{row_id}_left")
        } else {
            format!("{row_id}_right")
        },
        text,
        bbox,
        text_edge_x_pt,
        inner_left_edge_x_pt: boundary_geometry.inner_left_edge_x_pt,
        inner_right_edge_x_pt: boundary_geometry.inner_right_edge_x_pt,
        leading_whitespace_width_pt: boundary_geometry.leading_whitespace_width_pt,
        trailing_whitespace_width_pt: boundary_geometry.trailing_whitespace_width_pt,
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

fn boundary_run_geometry(run: &PdfFontTextRun) -> BoundaryRunGeometry {
    let run_left_x_pt = run.bbox.x0.min(run.bbox.x1);
    let run_right_x_pt = run.bbox.x0.max(run.bbox.x1);
    let chars = run.text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return BoundaryRunGeometry {
            inner_left_edge_x_pt: run_left_x_pt,
            inner_right_edge_x_pt: run_right_x_pt,
            leading_whitespace_width_pt: 0.0_f32,
            trailing_whitespace_width_pt: 0.0_f32,
        };
    }
    let advances =
        normalized_run_char_advances_pt(run, chars.len(), run_right_x_pt - run_left_x_pt);
    let leading_whitespace_width_pt = chars
        .iter()
        .zip(advances.iter())
        .take_while(|(ch, _)| ch.is_whitespace())
        .map(|(_, advance)| *advance)
        .sum::<f32>()
        .max(0.0_f32);
    let trailing_whitespace_width_pt = chars
        .iter()
        .rev()
        .zip(advances.iter().rev())
        .take_while(|(ch, _)| ch.is_whitespace())
        .map(|(_, advance)| *advance)
        .sum::<f32>()
        .max(0.0_f32);
    let mut inner_left_edge_x_pt = run_left_x_pt + leading_whitespace_width_pt;
    let mut inner_right_edge_x_pt = run_right_x_pt - trailing_whitespace_width_pt;
    if inner_right_edge_x_pt < inner_left_edge_x_pt {
        inner_left_edge_x_pt = run_left_x_pt;
        inner_right_edge_x_pt = run_right_x_pt;
    }
    BoundaryRunGeometry {
        inner_left_edge_x_pt,
        inner_right_edge_x_pt,
        leading_whitespace_width_pt,
        trailing_whitespace_width_pt,
    }
}

fn normalized_run_char_advances_pt(
    run: &PdfFontTextRun,
    char_count: usize,
    width_pt: f32,
) -> Vec<f32> {
    if char_count == 0 {
        return Vec::new();
    }
    let width_pt = width_pt.max(0.0_f32);
    if run.char_advances_pt.len() != char_count
        || run
            .char_advances_pt
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0_f32)
    {
        return vec![width_pt / char_count as f32; char_count];
    }
    let sum = run.char_advances_pt.iter().sum::<f32>();
    if !sum.is_finite() || sum <= 0.0_f32 || !width_pt.is_finite() {
        return vec![width_pt / char_count as f32; char_count];
    }
    let factor = width_pt / sum;
    run.char_advances_pt
        .iter()
        .map(|value| value * factor)
        .collect::<Vec<_>>()
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

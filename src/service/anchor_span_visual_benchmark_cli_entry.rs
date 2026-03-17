use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::benchmarks::types::known_redaction_contract::{
    canonical_known_redaction_contract, KnownRedactionDataset, KnownRedactionRowSelector,
    KnownRedactionTargetSelector,
};
use crate::logic::visual_anchor_research_component::{
    run_visual_anchor_research, RunVisualAnchorResearchRequest,
};
use crate::logic::{
    discover_pdf_inputs, read_input_pdf_bytes, validate_batch_input_directory, write_output_bytes,
    PipelineConfig,
};
use crate::service::tooling_entry::default_name_dictionary_entries;
use crate::types::diagnostic_types::{DiagnosticRecord, DiagnosticValue};
use crate::types::guess_types::{
    AnchorDecisionRecord, GuessCandidate, GuessReport, RedactionGuess,
};
use crate::types::visual_anchor_metric_types::{VisualAnchorMetricRow, VisualAnchorMetricsReport};

const TEST_DATA_DIR: &str = "test_data";
const DELTA_ALIGNMENT_MIN_PT: f32 = 5.0_f32;
const DELTA_ALIGNMENT_RATIO: f32 = 0.10_f32;
const DELTA_INFLATED_MIN_PT: f32 = 10.0_f32;
const DELTA_INFLATED_RATIO: f32 = 0.15_f32;
const FAR_PAIR_REDUCTION_MIN_PT: f32 = 8.0_f32;
const FAR_PAIR_REDUCTION_RATIO: f32 = 0.50_f32;
const BOUNDARY_GAP_REDUCTION_MIN_PT: f32 = 5.0_f32;
const BOUNDARY_GAP_REDUCTION_RATIO: f32 = 0.30_f32;
const TIE_ZONE_MAX_ERROR_GAP_PT: f32 = 1.0_f32;
const RESCORE_TOP_K: usize = 20;

const EXPERIMENT_CURRENT_VS_VISUAL_DELTA: &str = "current_vs_visual_delta";
const EXPERIMENT_BOX_VS_VISUAL_DELTA: &str = "box_vs_visual_delta";
const EXPERIMENT_BOUNDARY_GAP_BREAKDOWN: &str = "boundary_gap_breakdown";
const EXPERIMENT_BOUNDARY_GAP_ZEROED: &str = "boundary_gap_zeroed";
const EXPERIMENT_BOUNDARY_RUN_ONLY: &str = "boundary_run_only";
const EXPERIMENT_JOIN_GROWTH_DELTA: &str = "join_growth_delta";
const EXPERIMENT_NEAREST_TEXT_PAIR: &str = "nearest_text_pair";
const EXPERIMENT_MINIMUM_GAP_VALID_PAIR: &str = "minimum_gap_valid_pair";
const EXPERIMENT_PER_SIDE_GAP_MISMATCH: &str = "per_side_gap_mismatch";
const EXPERIMENT_VISUAL_ALIGNED_RESCORE: &str = "visual_aligned_rescore";
const EXPERIMENT_ONE_SIDED_COUNTERFACTUAL: &str = "one_sided_counterfactual";
const EXPERIMENT_TIE_ZONE_AFTER_VISUAL_ALIGNMENT: &str = "tie_zone_after_visual_alignment";

const PRIMARY_REASON_SPAN_ALIGNED: &str = "span_aligned";
const PRIMARY_REASON_FAR_PAIR: &str = "span_inflated_by_far_anchor_pair";
const PRIMARY_REASON_JOIN_GROWTH: &str = "span_inflated_by_join_growth";
const PRIMARY_REASON_BOUNDARY_GAP: &str = "span_inflated_by_boundary_gap";
const PRIMARY_REASON_BOX_UNRELIABLE: &str = "redaction_box_unreliable";
const PRIMARY_REASON_ONE_SIDED_REFERENCE_MISSING: &str = "one_sided_reference_missing";
const PRIMARY_REASON_RANKING_TIE: &str = "ranking_tie_after_visual_alignment";
const PRIMARY_REASON_UNEXPLAINED: &str = "unexplained_span_mismatch";

const REFERENCE_GROUPED_VISUAL_SPAN: &str = "grouped_visual_span";
const REFERENCE_NEAREST_VISUAL_SPAN: &str = "nearest_visual_span";
const REFERENCE_REDACTION_DARK_COMPONENT: &str = "redaction_dark_component";
const REFERENCE_REDACTION_BOX: &str = "redaction_box";

const REFERENCE_COMPLETE: &str = "reference_complete";
const REFERENCE_REDACTION_ONLY: &str = "reference_redaction_only";

const ALIGNMENT_ALIGNED: &str = "aligned";
const ALIGNMENT_INFLATED: &str = "inflated";
const ALIGNMENT_COMPRESSED: &str = "compressed";

#[derive(Debug, Clone, PartialEq)]
pub struct AnchorSpanVisualBenchmarkRequest {
    pub output_dir: PathBuf,
    pub compact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnchorSpanVisualBenchmarkOutputs {
    pub summary_path: PathBuf,
    pub rows_path: PathBuf,
    pub experiments_dir: PathBuf,
    pub crops_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct BenchmarkInputSpec {
    input: PathBuf,
    contract_dataset: Option<KnownRedactionDataset>,
    panel_only: bool,
}

#[derive(Debug, Clone)]
struct RowDiagnosticFacts {
    selected_left_candidate_id: Option<String>,
    selected_right_candidate_id: Option<String>,
    span_candidates: BTreeMap<String, SpanCandidateFacts>,
    pair_candidates: BTreeMap<String, PairCandidateFacts>,
    left_boundary_gap: Option<BoundaryGapFacts>,
    right_boundary_gap: Option<BoundaryGapFacts>,
}

#[derive(Debug, Clone)]
struct SpanCandidateFacts {
    candidate_id: String,
    side: String,
    line_id: String,
    candidate_rank: usize,
    span_x0: f32,
    span_x1: f32,
    inner_left_edge_x_pt: f32,
    inner_right_edge_x_pt: f32,
    leading_whitespace_width_pt: f32,
    trailing_whitespace_width_pt: f32,
    boundary_run_x0: f32,
    boundary_run_x1: f32,
}

#[derive(Debug, Clone)]
struct PairCandidateFacts {
    pair_id: String,
    line_id: String,
    left_candidate_id: String,
    right_candidate_id: String,
    pair_gap_sum_pt: f32,
    rejection_code: Option<String>,
}

#[derive(Debug, Clone)]
struct BoundaryGapFacts {
    gap_pt: f32,
    explicit_gap_pt: f32,
    inferred_gap_pt: f32,
    source: String,
}

#[derive(Debug, Clone)]
struct VisualReference {
    kind: &'static str,
    class: &'static str,
    width_pt: f32,
    left_gap_pt: Option<f32>,
    right_gap_pt: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
struct SummaryFile {
    dataset: DatasetSummary,
    row_alignment_counts: BTreeMap<String, usize>,
    primary_reason_counts: BTreeMap<String, usize>,
    experiment_improvement_counts: BTreeMap<String, usize>,
    visual_aligned_target_rank_summary: TargetRankImprovementSummary,
}

#[derive(Debug, Clone, Serialize)]
struct DatasetSummary {
    file_count: usize,
    benchmark_file_count: usize,
    sanity_panel_file_count: usize,
    row_count: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
struct TargetRankImprovementSummary {
    evaluated_targets: usize,
    improved_targets: usize,
    unchanged_targets: usize,
    worsened_targets: usize,
}

#[derive(Debug, Clone, Serialize)]
struct RowSummaryRecord {
    row_key: String,
    input_pdf: String,
    page_index: u32,
    row_id: String,
    current_anchor_mode: String,
    current_left_text: Option<String>,
    current_right_text: Option<String>,
    current_span_width_pt: f32,
    visual_reference_kind: String,
    visual_reference_class: String,
    visual_reference_width_pt: Option<f32>,
    redaction_box_width_pt: f32,
    redaction_dark_component_width_pt: Option<f32>,
    selected_left_gap_pt: Option<f32>,
    selected_right_gap_pt: Option<f32>,
    top1_candidate_text: Option<String>,
    top1_candidate_width_pt: Option<f32>,
    top1_candidate_error_pt: Option<f32>,
    benchmark_target_label: Option<String>,
    benchmark_target_text: Option<String>,
    benchmark_target_rank: Option<usize>,
    benchmark_target_error_pt: Option<f32>,
    primary_reason_code: String,
    current_alignment: String,
    crop_path: String,
}

#[derive(Debug, Clone, Serialize)]
struct ExperimentFile {
    name: String,
    improved_row_count: usize,
    rows: Vec<ExperimentRowRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct ExperimentRowRecord {
    row_key: String,
    counterfactual_span_width_pt: Option<f32>,
    absolute_visual_delta_pt: Option<f32>,
    delta_improvement_pt: Option<f32>,
    metrics: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
struct RowExperimentSet {
    current_vs_visual_delta: ExperimentRowRecord,
    box_vs_visual_delta: ExperimentRowRecord,
    boundary_gap_breakdown: ExperimentRowRecord,
    boundary_gap_zeroed: ExperimentRowRecord,
    boundary_run_only: ExperimentRowRecord,
    join_growth_delta: ExperimentRowRecord,
    nearest_text_pair: ExperimentRowRecord,
    minimum_gap_valid_pair: ExperimentRowRecord,
    per_side_gap_mismatch: ExperimentRowRecord,
    visual_aligned_rescore: ExperimentRowRecord,
    one_sided_counterfactual: ExperimentRowRecord,
    tie_zone_after_visual_alignment: ExperimentRowRecord,
}

#[derive(Debug, Clone)]
struct RescoredCandidate {
    normalized_text: String,
    text: String,
    error_pt: f32,
}

#[derive(Debug, Clone)]
struct RescoreResult {
    top1_text: Option<String>,
    top1_error_pt: Option<f32>,
    target_rank: Option<usize>,
    target_error_pt: Option<f32>,
}

#[derive(Debug, Clone)]
struct RowBenchmarkTarget {
    label: String,
    text: String,
    rank: Option<usize>,
    error_pt: Option<f32>,
}

#[derive(Debug, Clone)]
struct RowAnalysis {
    row_record: RowSummaryRecord,
    experiments: RowExperimentSet,
    crop_file_name: String,
    crop_bytes: Vec<u8>,
}

#[inline]
pub fn run(
    req: AnchorSpanVisualBenchmarkRequest,
) -> Result<AnchorSpanVisualBenchmarkOutputs, String> {
    let inputs = discover_benchmark_inputs()?;
    let mut rows = Vec::<RowAnalysis>::new();

    for input in &inputs {
        let pdf_bytes = read_input_pdf_bytes(&input.input)?;
        let dictionary_bytes = dictionary_bytes_for_dataset(input.contract_dataset.as_ref());
        let input_name = input.input.to_string_lossy().to_string();
        let research = run_visual_anchor_research(RunVisualAnchorResearchRequest {
            input_name: &input_name,
            pdf_bytes: &pdf_bytes,
            dictionary_bytes: dictionary_bytes.as_deref(),
            cfg: &benchmark_config(),
            collect_diagnostics: true,
        })?;
        rows.extend(analyze_rows(
            input,
            &research.guesses,
            research.diagnostics.as_deref().unwrap_or(&[]),
            &research.visual_report,
            &research.visual_crops,
        )?);
    }

    rows.sort_by(|left, right| left.row_record.row_key.cmp(&right.row_record.row_key));

    let rows_path = req.output_dir.join("rows.json");
    let summary_path = req.output_dir.join("summary.json");
    let experiments_dir = req.output_dir.join("experiments");
    let crops_dir = req.output_dir.join("crops");

    let row_records = rows
        .iter()
        .map(|analysis| analysis.row_record.clone())
        .collect::<Vec<_>>();
    let summary = build_summary(&inputs, &rows);
    write_output_bytes(
        &rows_path,
        encode_json(&row_records, req.compact)?.as_slice(),
    )?;
    write_output_bytes(
        &summary_path,
        encode_json(&summary, req.compact)?.as_slice(),
    )?;
    write_experiment_files(&experiments_dir, &rows, req.compact)?;
    for row in &rows {
        let path = crops_dir.join(&row.crop_file_name);
        write_output_bytes(path.as_path(), row.crop_bytes.as_slice())?;
    }

    Ok(AnchorSpanVisualBenchmarkOutputs {
        summary_path,
        rows_path,
        experiments_dir,
        crops_dir,
    })
}

fn discover_benchmark_inputs() -> Result<Vec<BenchmarkInputSpec>, String> {
    let contract = canonical_known_redaction_contract()?;
    let test_data_root = Path::new(TEST_DATA_DIR);
    validate_batch_input_directory(test_data_root)?;
    let discovered = discover_pdf_inputs(test_data_root)?;
    let mut out = Vec::<BenchmarkInputSpec>::new();
    let mut seen = BTreeSet::<PathBuf>::new();

    for dataset in &contract.datasets {
        let path = PathBuf::from(&dataset.input_pdf);
        if seen.insert(path.clone()) {
            out.push(BenchmarkInputSpec {
                input: path,
                contract_dataset: Some(dataset.clone()),
                panel_only: false,
            });
        }
    }
    for input in discovered {
        if seen.contains(&input) {
            continue;
        }
        out.push(BenchmarkInputSpec {
            input,
            contract_dataset: None,
            panel_only: true,
        });
    }
    out.sort_by(|left, right| left.input.cmp(&right.input));
    Ok(out)
}

fn analyze_rows(
    input: &BenchmarkInputSpec,
    guesses: &GuessReport,
    diagnostics: &[DiagnosticRecord],
    visual_report: &VisualAnchorMetricsReport,
    visual_crops: &[crate::logic::types::NamedBinaryArtifact],
) -> Result<Vec<RowAnalysis>, String> {
    let facts_by_row = collect_row_diagnostic_facts(diagnostics);
    let visual_by_row = visual_report
        .rows
        .iter()
        .map(|row| (row.row_id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let crop_by_row = visual_crops
        .iter()
        .map(|crop| {
            (
                crop.file_name
                    .strip_suffix(".png")
                    .unwrap_or(crop.file_name.as_str())
                    .to_owned(),
                crop,
            )
        })
        .collect::<BTreeMap<_, _>>();

    let row_targets = row_targets_for_dataset(input.contract_dataset.as_ref(), guesses)?;
    let stem = input
        .input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("input");
    let mut rows = Vec::<RowAnalysis>::new();

    for (guess, anchor) in guesses.guesses.iter().zip(guesses.anchors.iter()) {
        let visual = visual_by_row
            .get(&anchor.anchor_row_id)
            .ok_or_else(|| format!("missing visual row {}", anchor.anchor_row_id))?;
        let facts = facts_by_row
            .get(&anchor.anchor_row_id)
            .cloned()
            .unwrap_or_else(empty_row_diagnostic_facts);
        let visual_reference = select_visual_reference(visual);
        let current_span_width_pt = anchor.target_width_pt;
        let current_abs_delta_pt = Some((current_span_width_pt - visual_reference.width_pt).abs());
        let boundary_gap_zeroed =
            build_boundary_gap_zeroed_row(stem, anchor, &visual_reference, &facts);
        let nearest_text_pair =
            build_nearest_text_pair_row(stem, anchor, &visual_reference, &facts);
        let minimum_gap_valid_pair =
            build_minimum_gap_valid_pair_row(stem, anchor, &visual_reference, &facts);
        let join_growth_delta = build_join_growth_row(stem, anchor, &visual_reference, &facts);
        let per_side_gap_mismatch =
            build_per_side_gap_mismatch_row(stem, anchor, visual, &visual_reference);
        let row_target = row_targets.get(&anchor.anchor_row_id).cloned().flatten();
        let visual_aligned_rescore = build_visual_aligned_rescore_row(
            stem,
            anchor,
            guess,
            &visual_reference,
            row_target.as_ref(),
        );
        let one_sided_counterfactual =
            build_one_sided_counterfactual_row(stem, anchor, &visual_reference);
        let tie_zone_after_visual_alignment =
            build_tie_zone_row(stem, anchor, row_target.as_ref(), &visual_aligned_rescore);

        let experiments = RowExperimentSet {
            current_vs_visual_delta: build_simple_width_experiment(
                EXPERIMENT_CURRENT_VS_VISUAL_DELTA,
                stem,
                anchor,
                visual_reference.width_pt,
                current_span_width_pt,
            ),
            box_vs_visual_delta: build_simple_width_experiment(
                EXPERIMENT_BOX_VS_VISUAL_DELTA,
                stem,
                anchor,
                visual_reference.width_pt,
                guess.bbox.width().abs(),
            ),
            boundary_gap_breakdown: build_boundary_gap_breakdown_row(
                stem,
                anchor,
                visual,
                &visual_reference,
                &facts,
            ),
            boundary_gap_zeroed,
            boundary_run_only: build_simple_width_experiment(
                EXPERIMENT_BOUNDARY_RUN_ONLY,
                stem,
                anchor,
                visual_reference.width_pt,
                current_span_width_pt,
            ),
            join_growth_delta,
            nearest_text_pair,
            minimum_gap_valid_pair,
            per_side_gap_mismatch,
            visual_aligned_rescore,
            one_sided_counterfactual,
            tie_zone_after_visual_alignment,
        };
        let primary_reason_code = classify_primary_reason(
            anchor,
            &visual_reference,
            current_abs_delta_pt,
            &experiments,
            row_target.as_ref(),
        );
        let crop = crop_by_row
            .get(&anchor.anchor_row_id)
            .ok_or_else(|| format!("missing crop {}", anchor.anchor_row_id))?;
        let crop_file_name = format!("{stem}__{}.png", anchor.anchor_row_id);
        rows.push(RowAnalysis {
            row_record: RowSummaryRecord {
                row_key: row_key(stem, &anchor.anchor_row_id),
                input_pdf: input.input.display().to_string(),
                page_index: anchor.page_index,
                row_id: anchor.anchor_row_id.clone(),
                current_anchor_mode: anchor.anchor_mode.clone(),
                current_left_text: anchor.left.as_ref().map(|side| side.text.clone()),
                current_right_text: anchor.right.as_ref().map(|side| side.text.clone()),
                current_span_width_pt,
                visual_reference_kind: visual_reference.kind.to_owned(),
                visual_reference_class: visual_reference.class.to_owned(),
                visual_reference_width_pt: Some(visual_reference.width_pt),
                redaction_box_width_pt: guess.bbox.width().abs(),
                redaction_dark_component_width_pt: visual
                    .redaction_dark_component
                    .as_ref()
                    .map(|component| component.width_pt),
                selected_left_gap_pt: anchor.selected_left_gap_pt,
                selected_right_gap_pt: anchor.selected_right_gap_pt,
                top1_candidate_text: guess
                    .candidates
                    .first()
                    .map(|candidate| candidate.text.clone()),
                top1_candidate_width_pt: guess
                    .candidates
                    .first()
                    .map(|candidate| candidate.width_pt),
                top1_candidate_error_pt: guess
                    .candidates
                    .first()
                    .map(|candidate| candidate.error_pt),
                benchmark_target_label: row_target.as_ref().map(|target| target.label.clone()),
                benchmark_target_text: row_target.as_ref().map(|target| target.text.clone()),
                benchmark_target_rank: row_target.as_ref().and_then(|target| target.rank),
                benchmark_target_error_pt: row_target.as_ref().and_then(|target| target.error_pt),
                primary_reason_code: primary_reason_code.to_owned(),
                current_alignment: classify_alignment(
                    current_span_width_pt,
                    visual_reference.width_pt,
                )
                .to_owned(),
                crop_path: format!("crops/{crop_file_name}"),
            },
            experiments,
            crop_file_name,
            crop_bytes: crop.bytes.clone(),
        });
    }

    rows.sort_by(|left, right| left.row_record.row_key.cmp(&right.row_record.row_key));
    Ok(rows)
}

fn build_summary(inputs: &[BenchmarkInputSpec], rows: &[RowAnalysis]) -> SummaryFile {
    let mut row_alignment_counts = BTreeMap::<String, usize>::new();
    let mut primary_reason_counts = BTreeMap::<String, usize>::new();
    let mut experiment_improvement_counts = BTreeMap::<String, usize>::new();
    for row in rows {
        *row_alignment_counts
            .entry(row.row_record.current_alignment.clone())
            .or_default() += 1;
        *primary_reason_counts
            .entry(row.row_record.primary_reason_code.clone())
            .or_default() += 1;
    }
    for (name, rows) in experiment_rows_by_name(rows) {
        let improved = rows
            .iter()
            .filter(|row| {
                row.delta_improvement_pt
                    .is_some_and(|value| value > 0.01_f32)
            })
            .count();
        experiment_improvement_counts.insert(name.to_owned(), improved);
    }
    SummaryFile {
        dataset: DatasetSummary {
            file_count: inputs.len(),
            benchmark_file_count: inputs
                .iter()
                .filter(|input| input.contract_dataset.is_some())
                .count(),
            sanity_panel_file_count: inputs.iter().filter(|input| input.panel_only).count(),
            row_count: rows.len(),
        },
        row_alignment_counts,
        primary_reason_counts,
        experiment_improvement_counts,
        visual_aligned_target_rank_summary: build_target_rank_summary_from_rows(rows),
    }
}

fn build_target_rank_summary_from_rows(rows: &[RowAnalysis]) -> TargetRankImprovementSummary {
    let mut out = TargetRankImprovementSummary::default();
    let mut current_by_label = BTreeMap::<String, Option<usize>>::new();
    let mut visual_by_label = BTreeMap::<String, Option<usize>>::new();
    for row in rows {
        let Some(label) = row.row_record.benchmark_target_label.as_ref() else {
            continue;
        };
        let visual_rank = row
            .experiments
            .visual_aligned_rescore
            .metrics
            .get("target_rank_after")
            .and_then(value_as_usize);
        merge_best_rank(
            &mut current_by_label,
            label.clone(),
            row.row_record.benchmark_target_rank,
        );
        merge_best_rank(&mut visual_by_label, label.clone(), visual_rank);
    }
    for label in current_by_label.keys().chain(visual_by_label.keys()) {
        let current = current_by_label.get(label).copied().flatten();
        let visual = visual_by_label.get(label).copied().flatten();
        out.evaluated_targets += 1;
        match (current, visual) {
            (Some(current), Some(visual)) if visual < current => out.improved_targets += 1,
            (Some(current), Some(visual)) if visual > current => out.worsened_targets += 1,
            (None, Some(_)) => out.improved_targets += 1,
            (Some(_), None) => out.worsened_targets += 1,
            _ => out.unchanged_targets += 1,
        }
    }
    out
}

fn merge_best_rank(
    target: &mut BTreeMap<String, Option<usize>>,
    label: String,
    rank: Option<usize>,
) {
    target
        .entry(label)
        .and_modify(|current| {
            *current = match (*current, rank) {
                (Some(current), Some(rank)) => Some(current.min(rank)),
                (None, Some(rank)) => Some(rank),
                (current, None) => current,
            };
        })
        .or_insert(rank);
}

fn experiment_rows_by_name(
    rows: &[RowAnalysis],
) -> BTreeMap<&'static str, Vec<ExperimentRowRecord>> {
    let mut out = BTreeMap::<&'static str, Vec<ExperimentRowRecord>>::new();
    for row in rows {
        out.entry(EXPERIMENT_CURRENT_VS_VISUAL_DELTA)
            .or_default()
            .push(row.experiments.current_vs_visual_delta.clone());
        out.entry(EXPERIMENT_BOX_VS_VISUAL_DELTA)
            .or_default()
            .push(row.experiments.box_vs_visual_delta.clone());
        out.entry(EXPERIMENT_BOUNDARY_GAP_BREAKDOWN)
            .or_default()
            .push(row.experiments.boundary_gap_breakdown.clone());
        out.entry(EXPERIMENT_BOUNDARY_GAP_ZEROED)
            .or_default()
            .push(row.experiments.boundary_gap_zeroed.clone());
        out.entry(EXPERIMENT_BOUNDARY_RUN_ONLY)
            .or_default()
            .push(row.experiments.boundary_run_only.clone());
        out.entry(EXPERIMENT_JOIN_GROWTH_DELTA)
            .or_default()
            .push(row.experiments.join_growth_delta.clone());
        out.entry(EXPERIMENT_NEAREST_TEXT_PAIR)
            .or_default()
            .push(row.experiments.nearest_text_pair.clone());
        out.entry(EXPERIMENT_MINIMUM_GAP_VALID_PAIR)
            .or_default()
            .push(row.experiments.minimum_gap_valid_pair.clone());
        out.entry(EXPERIMENT_PER_SIDE_GAP_MISMATCH)
            .or_default()
            .push(row.experiments.per_side_gap_mismatch.clone());
        out.entry(EXPERIMENT_VISUAL_ALIGNED_RESCORE)
            .or_default()
            .push(row.experiments.visual_aligned_rescore.clone());
        out.entry(EXPERIMENT_ONE_SIDED_COUNTERFACTUAL)
            .or_default()
            .push(row.experiments.one_sided_counterfactual.clone());
        out.entry(EXPERIMENT_TIE_ZONE_AFTER_VISUAL_ALIGNMENT)
            .or_default()
            .push(row.experiments.tie_zone_after_visual_alignment.clone());
    }
    out
}

fn write_experiment_files(
    experiments_dir: &Path,
    rows: &[RowAnalysis],
    compact: bool,
) -> Result<(), String> {
    for (name, mut experiment_rows) in experiment_rows_by_name(rows) {
        experiment_rows.sort_by(|left, right| left.row_key.cmp(&right.row_key));
        let improved_row_count = experiment_rows
            .iter()
            .filter(|row| {
                row.delta_improvement_pt
                    .is_some_and(|value| value > 0.01_f32)
            })
            .count();
        let payload = ExperimentFile {
            name: name.to_owned(),
            improved_row_count,
            rows: experiment_rows,
        };
        let path = experiments_dir.join(format!("{name}.json"));
        write_output_bytes(path.as_path(), encode_json(&payload, compact)?.as_slice())?;
    }
    Ok(())
}

fn select_visual_reference(row: &VisualAnchorMetricRow) -> VisualReference {
    if let (Some(left), Some(right), Some(width_pt)) = (
        row.grouped_left.as_ref(),
        row.grouped_right.as_ref(),
        row.width_comparison.grouped_visual_span_width_pt,
    ) {
        return VisualReference {
            kind: REFERENCE_GROUPED_VISUAL_SPAN,
            class: REFERENCE_COMPLETE,
            width_pt,
            left_gap_pt: Some(left.gap_pt),
            right_gap_pt: Some(right.gap_pt),
        };
    }
    if let (Some(left), Some(right), Some(width_pt)) = (
        row.nearest_left.as_ref(),
        row.nearest_right.as_ref(),
        row.width_comparison.nearest_visual_span_width_pt,
    ) {
        return VisualReference {
            kind: REFERENCE_NEAREST_VISUAL_SPAN,
            class: REFERENCE_COMPLETE,
            width_pt,
            left_gap_pt: Some(left.gap_pt),
            right_gap_pt: Some(right.gap_pt),
        };
    }
    if let Some(width_pt) = row
        .redaction_dark_component
        .as_ref()
        .map(|component| component.width_pt)
    {
        return VisualReference {
            kind: REFERENCE_REDACTION_DARK_COMPONENT,
            class: REFERENCE_REDACTION_ONLY,
            width_pt,
            left_gap_pt: None,
            right_gap_pt: None,
        };
    }
    VisualReference {
        kind: REFERENCE_REDACTION_BOX,
        class: REFERENCE_REDACTION_ONLY,
        width_pt: row.width_comparison.redaction_box_width_pt,
        left_gap_pt: None,
        right_gap_pt: None,
    }
}

fn build_simple_width_experiment(
    name: &str,
    stem: &str,
    anchor: &AnchorDecisionRecord,
    reference_width_pt: f32,
    counterfactual_span_width_pt: f32,
) -> ExperimentRowRecord {
    let absolute_visual_delta_pt = (counterfactual_span_width_pt - reference_width_pt).abs();
    let current_absolute_visual_delta_pt = (anchor.target_width_pt - reference_width_pt).abs();
    let mut metrics = BTreeMap::<String, Value>::new();
    metrics.insert("experiment".to_owned(), json!(name));
    ExperimentRowRecord {
        row_key: row_key(stem, &anchor.anchor_row_id),
        counterfactual_span_width_pt: Some(counterfactual_span_width_pt),
        absolute_visual_delta_pt: Some(absolute_visual_delta_pt),
        delta_improvement_pt: Some(current_absolute_visual_delta_pt - absolute_visual_delta_pt),
        metrics,
    }
}

fn build_boundary_gap_breakdown_row(
    stem: &str,
    anchor: &AnchorDecisionRecord,
    visual: &VisualAnchorMetricRow,
    reference: &VisualReference,
    facts: &RowDiagnosticFacts,
) -> ExperimentRowRecord {
    let mut metrics = BTreeMap::<String, Value>::new();
    metrics.insert(
        "left_boundary_gap".to_owned(),
        boundary_gap_json(facts.left_boundary_gap.as_ref()),
    );
    metrics.insert(
        "right_boundary_gap".to_owned(),
        boundary_gap_json(facts.right_boundary_gap.as_ref()),
    );
    metrics.insert(
        "left_visual_gap_pt".to_owned(),
        json!(visual_gap_for_side("left", visual, reference)),
    );
    metrics.insert(
        "right_visual_gap_pt".to_owned(),
        json!(visual_gap_for_side("right", visual, reference)),
    );
    build_simple_width_experiment(
        EXPERIMENT_BOUNDARY_GAP_BREAKDOWN,
        stem,
        anchor,
        reference.width_pt,
        anchor.target_width_pt,
    )
    .with_metrics(metrics)
}

fn build_boundary_gap_zeroed_row(
    stem: &str,
    anchor: &AnchorDecisionRecord,
    reference: &VisualReference,
    facts: &RowDiagnosticFacts,
) -> ExperimentRowRecord {
    let counterfactual_span_width_pt = facts
        .selected_left_candidate_id
        .as_ref()
        .and_then(|id| facts.span_candidates.get(id))
        .zip(
            facts
                .selected_right_candidate_id
                .as_ref()
                .and_then(|id| facts.span_candidates.get(id)),
        )
        .map(|(left, right)| {
            (right.inner_left_edge_x_pt - left.inner_right_edge_x_pt).max(0.0_f32)
        });
    build_counterfactual_width_experiment(
        EXPERIMENT_BOUNDARY_GAP_ZEROED,
        stem,
        anchor,
        reference.width_pt,
        counterfactual_span_width_pt,
        BTreeMap::new(),
    )
}

fn build_join_growth_row(
    stem: &str,
    anchor: &AnchorDecisionRecord,
    reference: &VisualReference,
    facts: &RowDiagnosticFacts,
) -> ExperimentRowRecord {
    let left_join_growth_pt = facts
        .selected_left_candidate_id
        .as_ref()
        .and_then(|id| facts.span_candidates.get(id))
        .map(|candidate| (candidate.boundary_run_x0 - candidate.span_x0).max(0.0_f32));
    let right_join_growth_pt = facts
        .selected_right_candidate_id
        .as_ref()
        .and_then(|id| facts.span_candidates.get(id))
        .map(|candidate| (candidate.span_x1 - candidate.boundary_run_x1).max(0.0_f32));
    let mut metrics = BTreeMap::<String, Value>::new();
    metrics.insert("left_join_growth_pt".to_owned(), json!(left_join_growth_pt));
    metrics.insert(
        "right_join_growth_pt".to_owned(),
        json!(right_join_growth_pt),
    );
    metrics.insert(
        "total_join_growth_pt".to_owned(),
        json!(left_join_growth_pt.unwrap_or(0.0_f32) + right_join_growth_pt.unwrap_or(0.0_f32)),
    );
    build_counterfactual_width_experiment(
        EXPERIMENT_JOIN_GROWTH_DELTA,
        stem,
        anchor,
        reference.width_pt,
        Some(anchor.target_width_pt),
        metrics,
    )
}

fn build_nearest_text_pair_row(
    stem: &str,
    anchor: &AnchorDecisionRecord,
    reference: &VisualReference,
    facts: &RowDiagnosticFacts,
) -> ExperimentRowRecord {
    let counterfactual = anchor.selected_line_id.as_ref().and_then(|line_id| {
        let left = facts
            .span_candidates
            .values()
            .filter(|candidate| candidate.line_id == *line_id && candidate.side == "left")
            .min_by(|left, right| {
                left.candidate_rank
                    .cmp(&right.candidate_rank)
                    .then_with(|| left.candidate_id.cmp(&right.candidate_id))
            });
        let right = facts
            .span_candidates
            .values()
            .filter(|candidate| candidate.line_id == *line_id && candidate.side == "right")
            .min_by(|left, right| {
                left.candidate_rank
                    .cmp(&right.candidate_rank)
                    .then_with(|| left.candidate_id.cmp(&right.candidate_id))
            });
        left.zip(right)
            .map(|(left, right)| pair_width_from_explicit_whitespace(left, right))
    });
    build_counterfactual_width_experiment(
        EXPERIMENT_NEAREST_TEXT_PAIR,
        stem,
        anchor,
        reference.width_pt,
        counterfactual,
        BTreeMap::new(),
    )
}

fn build_minimum_gap_valid_pair_row(
    stem: &str,
    anchor: &AnchorDecisionRecord,
    reference: &VisualReference,
    facts: &RowDiagnosticFacts,
) -> ExperimentRowRecord {
    let counterfactual = anchor.selected_line_id.as_ref().and_then(|line_id| {
        facts
            .pair_candidates
            .values()
            .filter(|pair| pair.line_id == *line_id && pair.rejection_code.is_none())
            .min_by(|left, right| {
                ordered_f32(left.pair_gap_sum_pt)
                    .cmp(&ordered_f32(right.pair_gap_sum_pt))
                    .then_with(|| left.pair_id.cmp(&right.pair_id))
            })
            .and_then(|pair| {
                let left = facts.span_candidates.get(&pair.left_candidate_id)?;
                let right = facts.span_candidates.get(&pair.right_candidate_id)?;
                Some(pair_width_from_explicit_whitespace(left, right))
            })
    });
    build_counterfactual_width_experiment(
        EXPERIMENT_MINIMUM_GAP_VALID_PAIR,
        stem,
        anchor,
        reference.width_pt,
        counterfactual,
        BTreeMap::new(),
    )
}

fn build_per_side_gap_mismatch_row(
    stem: &str,
    anchor: &AnchorDecisionRecord,
    visual: &VisualAnchorMetricRow,
    reference: &VisualReference,
) -> ExperimentRowRecord {
    let mut metrics = BTreeMap::<String, Value>::new();
    let left_visual_gap_pt = visual_gap_for_side("left", visual, reference);
    let right_visual_gap_pt = visual_gap_for_side("right", visual, reference);
    metrics.insert(
        "left_gap_mismatch_pt".to_owned(),
        json!(anchor
            .selected_left_gap_pt
            .zip(left_visual_gap_pt)
            .map(|(current, visual)| current - visual)),
    );
    metrics.insert(
        "right_gap_mismatch_pt".to_owned(),
        json!(anchor
            .selected_right_gap_pt
            .zip(right_visual_gap_pt)
            .map(|(current, visual)| current - visual)),
    );
    build_counterfactual_width_experiment(
        EXPERIMENT_PER_SIDE_GAP_MISMATCH,
        stem,
        anchor,
        reference.width_pt,
        Some(anchor.target_width_pt),
        metrics,
    )
}

fn build_visual_aligned_rescore_row(
    stem: &str,
    anchor: &AnchorDecisionRecord,
    guess: &RedactionGuess,
    reference: &VisualReference,
    row_target: Option<&RowBenchmarkTarget>,
) -> ExperimentRowRecord {
    let rescore = rescore_candidates_to_width(&guess.candidates, reference.width_pt, row_target);
    let mut metrics = BTreeMap::<String, Value>::new();
    metrics.insert("top1_text".to_owned(), json!(rescore.top1_text));
    metrics.insert("top1_error_pt".to_owned(), json!(rescore.top1_error_pt));
    metrics.insert(
        "target_rank_before".to_owned(),
        json!(row_target.and_then(|target| target.rank)),
    );
    metrics.insert("target_rank_after".to_owned(), json!(rescore.target_rank));
    metrics.insert(
        "target_error_before".to_owned(),
        json!(row_target.and_then(|target| target.error_pt)),
    );
    metrics.insert(
        "target_error_after".to_owned(),
        json!(rescore.target_error_pt),
    );
    build_counterfactual_width_experiment(
        EXPERIMENT_VISUAL_ALIGNED_RESCORE,
        stem,
        anchor,
        reference.width_pt,
        Some(reference.width_pt),
        metrics,
    )
}

fn build_one_sided_counterfactual_row(
    stem: &str,
    anchor: &AnchorDecisionRecord,
    reference: &VisualReference,
) -> ExperimentRowRecord {
    let mut metrics = BTreeMap::<String, Value>::new();
    metrics.insert("current_mode".to_owned(), json!(anchor.anchor_mode));
    metrics.insert(
        "current_span_width_pt".to_owned(),
        json!(anchor.target_width_pt),
    );
    metrics.insert(
        "visual_reference_width_pt".to_owned(),
        json!(reference.width_pt),
    );
    build_counterfactual_width_experiment(
        EXPERIMENT_ONE_SIDED_COUNTERFACTUAL,
        stem,
        anchor,
        reference.width_pt,
        matches!(anchor.anchor_mode.as_str(), "left_only" | "right_only")
            .then_some(reference.width_pt),
        metrics,
    )
}

fn build_tie_zone_row(
    stem: &str,
    anchor: &AnchorDecisionRecord,
    row_target: Option<&RowBenchmarkTarget>,
    visual_aligned_rescore: &ExperimentRowRecord,
) -> ExperimentRowRecord {
    let mut metrics = visual_aligned_rescore.metrics.clone();
    let target_error_after = metrics.get("target_error_after").and_then(value_as_f32);
    let top1_error_pt = metrics.get("top1_error_pt").and_then(value_as_f32);
    metrics.insert(
        "target_vs_top1_error_gap_pt".to_owned(),
        json!(target_error_after
            .zip(top1_error_pt)
            .map(|(target, top1)| target - top1)),
    );
    metrics.insert(
        "target_rank_before".to_owned(),
        json!(row_target.and_then(|target| target.rank)),
    );
    build_counterfactual_width_experiment(
        EXPERIMENT_TIE_ZONE_AFTER_VISUAL_ALIGNMENT,
        stem,
        anchor,
        visual_aligned_rescore
            .counterfactual_span_width_pt
            .unwrap_or(anchor.target_width_pt),
        visual_aligned_rescore.counterfactual_span_width_pt,
        metrics,
    )
}

fn build_counterfactual_width_experiment(
    _name: &str,
    stem: &str,
    anchor: &AnchorDecisionRecord,
    reference_width_pt: f32,
    counterfactual_span_width_pt: Option<f32>,
    metrics: BTreeMap<String, Value>,
) -> ExperimentRowRecord {
    let absolute_visual_delta_pt =
        counterfactual_span_width_pt.map(|width_pt| (width_pt - reference_width_pt).abs());
    let delta_improvement_pt = absolute_visual_delta_pt
        .map(|delta| (anchor.target_width_pt - reference_width_pt).abs() - delta);
    ExperimentRowRecord {
        row_key: row_key(stem, &anchor.anchor_row_id),
        counterfactual_span_width_pt,
        absolute_visual_delta_pt,
        delta_improvement_pt,
        metrics,
    }
}

fn classify_primary_reason(
    anchor: &AnchorDecisionRecord,
    reference: &VisualReference,
    current_abs_delta_pt: Option<f32>,
    experiments: &RowExperimentSet,
    row_target: Option<&RowBenchmarkTarget>,
) -> &'static str {
    let current_span_width_pt = anchor.target_width_pt;
    if classify_alignment(current_span_width_pt, reference.width_pt) == ALIGNMENT_ALIGNED {
        return PRIMARY_REASON_SPAN_ALIGNED;
    }
    let current_abs_delta_pt =
        current_abs_delta_pt.unwrap_or((current_span_width_pt - reference.width_pt).abs());
    let far_pair_reduction_threshold =
        FAR_PAIR_REDUCTION_MIN_PT.max(current_abs_delta_pt * FAR_PAIR_REDUCTION_RATIO);
    if experiment_improves_by(&experiments.nearest_text_pair, far_pair_reduction_threshold)
        || experiment_improves_by(
            &experiments.minimum_gap_valid_pair,
            far_pair_reduction_threshold,
        )
    {
        return PRIMARY_REASON_FAR_PAIR;
    }
    if experiment_improves_by(&experiments.boundary_run_only, far_pair_reduction_threshold) {
        return PRIMARY_REASON_JOIN_GROWTH;
    }
    let boundary_gap_threshold =
        BOUNDARY_GAP_REDUCTION_MIN_PT.max(current_abs_delta_pt * BOUNDARY_GAP_REDUCTION_RATIO);
    if experiment_improves_by(&experiments.boundary_gap_zeroed, boundary_gap_threshold) {
        return PRIMARY_REASON_BOUNDARY_GAP;
    }
    if classify_alignment(anchor.bbox.width().abs(), reference.width_pt) != ALIGNMENT_ALIGNED {
        return PRIMARY_REASON_BOX_UNRELIABLE;
    }
    if matches!(anchor.anchor_mode.as_str(), "left_only" | "right_only")
        && reference.class != REFERENCE_COMPLETE
    {
        return PRIMARY_REASON_ONE_SIDED_REFERENCE_MISSING;
    }
    if let Some(row_target) = row_target {
        let target_rank_after = experiments
            .visual_aligned_rescore
            .metrics
            .get("target_rank_after")
            .and_then(value_as_usize);
        let target_error_after = experiments
            .visual_aligned_rescore
            .metrics
            .get("target_error_after")
            .and_then(value_as_f32);
        let top1_error_after = experiments
            .visual_aligned_rescore
            .metrics
            .get("top1_error_pt")
            .and_then(value_as_f32);
        if target_rank_after.unwrap_or(usize::MAX) > RESCORE_TOP_K
            && target_error_after
                .zip(top1_error_after)
                .is_some_and(|(target, top1)| (target - top1) <= TIE_ZONE_MAX_ERROR_GAP_PT)
            && !row_target.text.trim().is_empty()
        {
            return PRIMARY_REASON_RANKING_TIE;
        }
    }
    PRIMARY_REASON_UNEXPLAINED
}

fn experiment_improves_by(experiment: &ExperimentRowRecord, threshold: f32) -> bool {
    experiment
        .delta_improvement_pt
        .is_some_and(|value| value >= threshold)
}

fn row_targets_for_dataset(
    dataset: Option<&KnownRedactionDataset>,
    guesses: &GuessReport,
) -> Result<BTreeMap<String, Option<RowBenchmarkTarget>>, String> {
    let mut out = guesses
        .anchors
        .iter()
        .map(|anchor| (anchor.anchor_row_id.clone(), None))
        .collect::<BTreeMap<_, _>>();
    let Some(dataset) = dataset else {
        return Ok(out);
    };
    match &dataset.row_selector {
        KnownRedactionRowSelector::PositionFromEnd {} => {
            for target in &dataset.targets {
                let KnownRedactionTargetSelector::IndexFromEnd { index_from_end } = target.selector
                else {
                    return Err(format!(
                        "dataset '{}' uses unsupported selector for row target mapping",
                        dataset.name
                    ));
                };
                if guesses.guesses.len() < index_from_end || guesses.anchors.len() < index_from_end
                {
                    continue;
                }
                let row_index = guesses.guesses.len() - index_from_end;
                if let Some(anchor) = guesses.anchors.get(row_index) {
                    let guess = &guesses.guesses[row_index];
                    let rank = rank_in_guess(guess, &target.target);
                    let error_pt = rank.and_then(|rank| candidate_error_for_rank(guess, rank));
                    out.insert(
                        anchor.anchor_row_id.clone(),
                        Some(RowBenchmarkTarget {
                            label: target.label.clone(),
                            text: target.target.clone(),
                            rank,
                            error_pt,
                        }),
                    );
                }
            }
        }
        KnownRedactionRowSelector::PageYRange {
            page_index,
            y0_min,
            y1_max,
        } => {
            for (guess, anchor) in guesses.guesses.iter().zip(guesses.anchors.iter()) {
                if guess.page_index != *page_index
                    || guess.bbox.y0 < *y0_min
                    || guess.bbox.y1 > *y1_max
                {
                    continue;
                }
                let best = dataset
                    .targets
                    .iter()
                    .filter_map(|target| {
                        let rank = rank_in_guess(guess, &target.target)?;
                        Some(RowBenchmarkTarget {
                            label: target.label.clone(),
                            text: target.target.clone(),
                            rank: Some(rank),
                            error_pt: candidate_error_for_rank(guess, rank),
                        })
                    })
                    .min_by(|left, right| {
                        left.rank
                            .cmp(&right.rank)
                            .then_with(|| {
                                ordered_f32(left.error_pt.unwrap_or(f32::MAX))
                                    .cmp(&ordered_f32(right.error_pt.unwrap_or(f32::MAX)))
                            })
                            .then_with(|| left.label.cmp(&right.label))
                    });
                out.insert(anchor.anchor_row_id.clone(), best);
            }
        }
    }
    Ok(out)
}

fn collect_row_diagnostic_facts(
    diagnostics: &[DiagnosticRecord],
) -> BTreeMap<String, RowDiagnosticFacts> {
    let mut out = BTreeMap::<String, RowDiagnosticFacts>::new();
    for diagnostic in diagnostics {
        let Some(row_id) = diagnostic.row_id.clone() else {
            continue;
        };
        let facts = out.entry(row_id).or_insert_with(empty_row_diagnostic_facts);
        match diagnostic.code.as_str() {
            "anchor_resolution_final" => {
                facts.selected_left_candidate_id =
                    metric_text(diagnostic, "selected_left_candidate_id");
                facts.selected_right_candidate_id =
                    metric_text(diagnostic, "selected_right_candidate_id");
            }
            "anchor_boundary_gap_selected" => {
                let side = metric_text(diagnostic, "side").unwrap_or_default();
                let gap = BoundaryGapFacts {
                    gap_pt: metric_f32(diagnostic, "boundary_gap_pt").unwrap_or_default(),
                    explicit_gap_pt: metric_f32(diagnostic, "explicit_boundary_gap_pt")
                        .unwrap_or_default(),
                    inferred_gap_pt: metric_f32(diagnostic, "inferred_boundary_gap_pt")
                        .unwrap_or_default(),
                    source: metric_text(diagnostic, "boundary_gap_source")
                        .unwrap_or_else(|| "none".to_owned()),
                };
                if side == "left" {
                    facts.left_boundary_gap = Some(gap);
                } else if side == "right" {
                    facts.right_boundary_gap = Some(gap);
                }
            }
            "anchor_span_candidate_considered" => {
                if let Some(candidate) = span_candidate_from_diagnostic(diagnostic) {
                    facts
                        .span_candidates
                        .insert(candidate.candidate_id.clone(), candidate);
                }
            }
            "anchor_pair_candidate_considered" => {
                if let Some(pair) = pair_candidate_from_diagnostic(diagnostic) {
                    facts.pair_candidates.insert(pair.pair_id.clone(), pair);
                }
            }
            code if code.starts_with("anchor_pair_rejected_") => {
                if let Some(pair) = pair_candidate_from_diagnostic(diagnostic) {
                    facts
                        .pair_candidates
                        .entry(pair.pair_id.clone())
                        .and_modify(|existing| existing.rejection_code = Some(code.to_owned()))
                        .or_insert(PairCandidateFacts {
                            rejection_code: Some(code.to_owned()),
                            ..pair
                        });
                }
            }
            _ => {}
        }
    }
    out
}

fn span_candidate_from_diagnostic(diagnostic: &DiagnosticRecord) -> Option<SpanCandidateFacts> {
    Some(SpanCandidateFacts {
        candidate_id: metric_text(diagnostic, "stable_candidate_id")?,
        side: metric_text(diagnostic, "anchor_side")?,
        line_id: metric_text(diagnostic, "line_id")?,
        candidate_rank: metric_usize(diagnostic, "candidate_rank")?,
        span_x0: metric_f32(diagnostic, "span_x0")?,
        span_x1: metric_f32(diagnostic, "span_x1")?,
        inner_left_edge_x_pt: metric_f32(diagnostic, "inner_left_edge_x_pt")?,
        inner_right_edge_x_pt: metric_f32(diagnostic, "inner_right_edge_x_pt")?,
        leading_whitespace_width_pt: metric_f32(diagnostic, "leading_whitespace_width_pt")?,
        trailing_whitespace_width_pt: metric_f32(diagnostic, "trailing_whitespace_width_pt")?,
        boundary_run_x0: metric_f32(diagnostic, "boundary_run_x0")?,
        boundary_run_x1: metric_f32(diagnostic, "boundary_run_x1")?,
    })
}

fn pair_candidate_from_diagnostic(diagnostic: &DiagnosticRecord) -> Option<PairCandidateFacts> {
    Some(PairCandidateFacts {
        pair_id: metric_text(diagnostic, "stable_candidate_id")?,
        line_id: metric_text(diagnostic, "line_id")?,
        left_candidate_id: metric_text(diagnostic, "left_candidate_id")?,
        right_candidate_id: metric_text(diagnostic, "right_candidate_id")?,
        pair_gap_sum_pt: metric_f32(diagnostic, "pair_gap_sum_pt")?,
        rejection_code: None,
    })
}

fn metric_text(diagnostic: &DiagnosticRecord, key: &str) -> Option<String> {
    diagnostic.metrics.get(key).and_then(|value| match value {
        DiagnosticValue::Text(value) => Some(value.clone()),
        _ => None,
    })
}

fn metric_f32(diagnostic: &DiagnosticRecord, key: &str) -> Option<f32> {
    diagnostic.metrics.get(key).and_then(|value| match value {
        DiagnosticValue::Float(value) => Some(*value as f32),
        DiagnosticValue::Integer(value) => Some(*value as f32),
        _ => None,
    })
}

fn metric_usize(diagnostic: &DiagnosticRecord, key: &str) -> Option<usize> {
    diagnostic.metrics.get(key).and_then(|value| match value {
        DiagnosticValue::Integer(value) => usize::try_from(*value).ok(),
        _ => None,
    })
}

fn rank_in_guess(guess: &RedactionGuess, target: &str) -> Option<usize> {
    let target_upper = normalize_candidate_text(target);
    if target_upper.is_empty() {
        return None;
    }
    ordered_guess_texts_upper(guess)
        .iter()
        .position(|value| value == &target_upper)
        .map(|index| index + 1)
}

fn ordered_guess_texts_upper(guess: &RedactionGuess) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = BTreeSet::<String>::new();
    for candidate in &guess.candidates {
        let normalized = normalize_candidate_text(&candidate.text);
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

fn rescore_candidates_to_width(
    candidates: &[GuessCandidate],
    target_width_pt: f32,
    row_target: Option<&RowBenchmarkTarget>,
) -> RescoreResult {
    let mut rescored = candidates
        .iter()
        .map(|candidate| RescoredCandidate {
            normalized_text: normalize_candidate_text(&candidate.text),
            text: candidate.text.clone(),
            error_pt: (candidate.width_pt - target_width_pt).abs(),
        })
        .filter(|candidate| !candidate.normalized_text.is_empty())
        .collect::<Vec<_>>();
    rescored.sort_by(|left, right| {
        ordered_f32(left.error_pt)
            .cmp(&ordered_f32(right.error_pt))
            .then_with(|| left.normalized_text.cmp(&right.normalized_text))
            .then_with(|| left.text.cmp(&right.text))
    });
    let top1 = rescored.first().cloned();
    let target_upper = row_target.map(|target| normalize_candidate_text(&target.text));
    let target_rank = target_upper.as_ref().and_then(|target| {
        rescored
            .iter()
            .position(|candidate| &candidate.normalized_text == target)
            .map(|index| index + 1)
    });
    let target_error_pt = target_rank
        .and_then(|rank| rescored.get(rank - 1))
        .map(|candidate| candidate.error_pt);
    RescoreResult {
        top1_text: top1.as_ref().map(|candidate| candidate.text.clone()),
        top1_error_pt: top1.as_ref().map(|candidate| candidate.error_pt),
        target_rank,
        target_error_pt,
    }
}

fn candidate_error_for_rank(guess: &RedactionGuess, rank: usize) -> Option<f32> {
    let target = ordered_guess_texts_upper(guess)
        .get(rank.saturating_sub(1))?
        .clone();
    let mut seen = BTreeSet::<String>::new();
    for candidate in &guess.candidates {
        let normalized = normalize_candidate_text(&candidate.text);
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        if normalized == target {
            return Some(candidate.error_pt);
        }
    }
    None
}

fn dictionary_bytes_for_dataset(dataset: Option<&KnownRedactionDataset>) -> Option<Vec<u8>> {
    let dataset = dataset?;
    if dataset.name != "EFTA00038617" {
        return None;
    }
    let targets = dataset
        .targets
        .iter()
        .map(|target| normalize_candidate_text(&target.target))
        .filter(|target| !target.is_empty())
        .collect::<BTreeSet<_>>();
    let mut lines = targets.iter().cloned().collect::<Vec<_>>();
    for value in default_name_dictionary_entries() {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = normalize_candidate_text(trimmed);
        if normalized.is_empty() || targets.contains(&normalized) {
            continue;
        }
        lines.push(trimmed.to_owned());
        if lines.len() >= 1_200 {
            break;
        }
    }
    Some(lines.join("\n").into_bytes())
}

fn benchmark_config() -> PipelineConfig {
    PipelineConfig {
        visualize: false,
        ..PipelineConfig::default()
    }
}

fn pair_width_from_explicit_whitespace(
    left: &SpanCandidateFacts,
    right: &SpanCandidateFacts,
) -> f32 {
    let left_gap_pt = left.trailing_whitespace_width_pt.max(0.0_f32);
    let right_gap_pt = right.leading_whitespace_width_pt.max(0.0_f32);
    ((right.inner_left_edge_x_pt - right_gap_pt) - (left.inner_right_edge_x_pt + left_gap_pt))
        .max(0.0_f32)
}

fn normalize_candidate_text(text: &str) -> String {
    text.trim().to_ascii_uppercase()
}

fn row_key(stem: &str, row_id: &str) -> String {
    format!("{stem}:{row_id}")
}

fn empty_row_diagnostic_facts() -> RowDiagnosticFacts {
    RowDiagnosticFacts {
        selected_left_candidate_id: None,
        selected_right_candidate_id: None,
        span_candidates: BTreeMap::new(),
        pair_candidates: BTreeMap::new(),
        left_boundary_gap: None,
        right_boundary_gap: None,
    }
}

fn classify_alignment(current_span_width_pt: f32, visual_reference_width_pt: f32) -> &'static str {
    let abs_delta = (current_span_width_pt - visual_reference_width_pt).abs();
    let aligned_threshold =
        DELTA_ALIGNMENT_MIN_PT.max(visual_reference_width_pt.abs() * DELTA_ALIGNMENT_RATIO);
    if abs_delta <= aligned_threshold {
        return ALIGNMENT_ALIGNED;
    }
    let inflation_threshold =
        DELTA_INFLATED_MIN_PT.max(visual_reference_width_pt.abs() * DELTA_INFLATED_RATIO);
    if current_span_width_pt - visual_reference_width_pt >= inflation_threshold {
        ALIGNMENT_INFLATED
    } else {
        ALIGNMENT_COMPRESSED
    }
}

fn boundary_gap_json(gap: Option<&BoundaryGapFacts>) -> Value {
    match gap {
        Some(gap) => json!({
            "gap_pt": gap.gap_pt,
            "explicit_gap_pt": gap.explicit_gap_pt,
            "inferred_gap_pt": gap.inferred_gap_pt,
            "source": gap.source,
        }),
        None => Value::Null,
    }
}

fn visual_gap_for_side(
    side: &str,
    row: &VisualAnchorMetricRow,
    reference: &VisualReference,
) -> Option<f32> {
    match reference.kind {
        REFERENCE_GROUPED_VISUAL_SPAN => {
            if side == "left" {
                row.grouped_left.as_ref().map(|span| span.gap_pt)
            } else {
                row.grouped_right.as_ref().map(|span| span.gap_pt)
            }
        }
        REFERENCE_NEAREST_VISUAL_SPAN => {
            if side == "left" {
                row.nearest_left.as_ref().map(|span| span.gap_pt)
            } else {
                row.nearest_right.as_ref().map(|span| span.gap_pt)
            }
        }
        _ => {
            if side == "left" {
                reference.left_gap_pt
            } else {
                reference.right_gap_pt
            }
        }
    }
}

fn ordered_f32(value: f32) -> i32 {
    ((value as f64) * 10_000.0_f64).round() as i32
}

fn value_as_f32(value: &Value) -> Option<f32> {
    value.as_f64().map(|value| value as f32)
}

fn value_as_usize(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|value| usize::try_from(value).ok())
}

fn encode_json<T: Serialize>(value: &T, compact: bool) -> Result<Vec<u8>, String> {
    if compact {
        serde_json::to_vec(value)
    } else {
        serde_json::to_vec_pretty(value)
    }
    .map_err(|error| format!("failed to encode benchmark json: {error}"))
}

trait ExperimentRowRecordExt {
    fn with_metrics(self, metrics: BTreeMap<String, Value>) -> Self;
}

impl ExperimentRowRecordExt for ExperimentRowRecord {
    fn with_metrics(mut self, metrics: BTreeMap<String, Value>) -> Self {
        self.metrics = metrics;
        self
    }
}

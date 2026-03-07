use clap::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use unredact::benchmarks::types::anchor_synthetic_contract::{
    canonical_anchor_synthetic_contract, AnchorSyntheticContract,
};
use unredact::benchmarks::types::redaction_detection_contract::{
    canonical_redaction_detection_contract, RedactionDetectionContract,
};
use unredact::service::tooling_entry::{
    collect_underlying_text_hits_by_page, run_anchor_from_redactions, ToolingAnchorRequest,
};
use unredact::types::guess_types::{
    AnchorCandidateDecision, AnchorDecisionRecord, AnchorReport, AnchorSideDecision, AnchorType,
};
use unredact::types::redaction_types::{
    Rect, RedactionKind, RedactionOccurrence, RedactionReport, UnderlyingTextHit,
};

const TREND_EPSILON: f64 = 1e-9_f64;
const POSITION_TOLERANCE_PT: f64 = 1.0_f64;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContractSummary {
    contract_id: String,
    schema_version: usize,
    dataset_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct SyntheticConfigSummary {
    contract_id: String,
    schema_version: usize,
    seeds: Vec<u64>,
    samples_per_pdf: usize,
    max_pdfs: usize,
    min_gap_pt: f32,
    max_gap_pt: f32,
    max_center_y_delta_pt: f32,
}

#[derive(Debug, Clone, Serialize)]
struct MetricDefinitions {
    row_selected_ratio: &'static str,
    row_mode_match_ratio: &'static str,
    text_exact_recall: &'static str,
    text_soft_recall: &'static str,
    x_error_mae_pt: &'static str,
    x_error_p90_pt: &'static str,
    x_within_tol_recall: &'static str,
    anchor_score: &'static str,
    #[serde(rename = "curated_headline_score")]
    headline: &'static str,
    position_tolerance_pt: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SideAccuracySummary {
    #[serde(rename = "expected_anchor_count", alias = "expected")]
    expected: usize,
    #[serde(rename = "resolved_anchor_count", alias = "resolved")]
    resolved: usize,
    #[serde(rename = "resolved_anchor_recall", alias = "resolved_recall")]
    resolved_recall: f64,
    #[serde(rename = "exact_text_match_count", alias = "text_exact")]
    text_exact: usize,
    #[serde(rename = "exact_text_recall", alias = "text_exact_recall")]
    text_exact_recall: f64,
    #[serde(rename = "soft_text_match_count", alias = "text_soft")]
    text_soft: usize,
    #[serde(rename = "soft_text_recall", alias = "text_soft_recall")]
    text_soft_recall: f64,
    x_error_mae_pt: Option<f64>,
    x_error_p90_pt: Option<f64>,
    #[serde(rename = "x_within_tolerance_count", alias = "x_within_tol_count")]
    x_within_tol_count: usize,
    #[serde(rename = "x_within_tolerance_recall", alias = "x_within_tol_recall")]
    x_within_tol_recall: f64,
    source_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DatasetSummary {
    #[serde(rename = "dataset_name")]
    name: String,
    #[serde(rename = "input_pdf_path")]
    input_pdf: String,
    #[serde(rename = "redaction_input_path")]
    redaction_input: String,
    #[serde(rename = "redaction_rows_evaluated", alias = "rows_total")]
    rows_total: usize,
    #[serde(
        rename = "rows_with_selected_anchor_candidate",
        alias = "rows_with_selected_candidate"
    )]
    rows_with_selected_candidate: usize,
    #[serde(rename = "row_selection_rate", alias = "row_selected_ratio")]
    row_selected_ratio: f64,
    #[serde(rename = "row_mode_match_rate", alias = "row_mode_match_ratio")]
    row_mode_match_ratio: f64,
    anchor_mode_counts: BTreeMap<String, usize>,
    #[serde(rename = "left_side_metrics", alias = "left")]
    left: SideAccuracySummary,
    #[serde(rename = "right_side_metrics", alias = "right")]
    right: SideAccuracySummary,
    #[serde(rename = "expected_anchor_side_count", alias = "side_expected_total")]
    side_expected_total: usize,
    #[serde(rename = "resolved_anchor_side_count", alias = "side_resolved_total")]
    side_resolved_total: usize,
    #[serde(
        rename = "overall_exact_text_recall",
        alias = "text_exact_recall_overall"
    )]
    text_exact_recall_overall: f64,
    #[serde(
        rename = "overall_soft_text_recall",
        alias = "text_soft_recall_overall"
    )]
    text_soft_recall_overall: f64,
    #[serde(
        rename = "overall_x_within_tolerance_recall",
        alias = "x_within_tol_recall_overall"
    )]
    x_within_tol_recall_overall: f64,
    #[serde(
        rename = "overall_x_error_mean_absolute_pt",
        alias = "x_error_mae_overall_pt"
    )]
    x_error_mae_overall_pt: Option<f64>,
    #[serde(rename = "anchor_quality_score", alias = "anchor_score")]
    anchor_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModeSummary {
    dataset_count: usize,
    #[serde(rename = "redaction_rows_evaluated", alias = "rows_total")]
    rows_total: usize,
    #[serde(
        rename = "rows_with_selected_anchor_candidate",
        alias = "rows_with_selected_candidate"
    )]
    rows_with_selected_candidate: usize,
    #[serde(rename = "row_selection_rate", alias = "row_selected_ratio")]
    row_selected_ratio: f64,
    #[serde(rename = "row_mode_match_rate", alias = "row_mode_match_ratio")]
    row_mode_match_ratio: f64,
    anchor_mode_counts: BTreeMap<String, usize>,
    #[serde(rename = "left_side_metrics", alias = "left")]
    left: SideAccuracySummary,
    #[serde(rename = "right_side_metrics", alias = "right")]
    right: SideAccuracySummary,
    #[serde(default)]
    #[serde(rename = "expected_anchor_side_count", alias = "side_expected_total")]
    side_expected_total: usize,
    #[serde(default)]
    #[serde(rename = "resolved_anchor_side_count", alias = "side_resolved_total")]
    side_resolved_total: usize,
    #[serde(default)]
    #[serde(
        rename = "overall_exact_text_recall",
        alias = "text_exact_recall_overall"
    )]
    text_exact_recall_overall: f64,
    #[serde(default)]
    #[serde(
        rename = "overall_soft_text_recall",
        alias = "text_soft_recall_overall"
    )]
    text_soft_recall_overall: f64,
    #[serde(default)]
    #[serde(
        rename = "overall_x_within_tolerance_recall",
        alias = "x_within_tol_recall_overall"
    )]
    x_within_tol_recall_overall: f64,
    #[serde(default)]
    #[serde(
        rename = "overall_x_error_mean_absolute_pt",
        alias = "x_error_mae_overall_pt"
    )]
    x_error_mae_overall_pt: Option<f64>,
    #[serde(rename = "anchor_quality_score", alias = "anchor_score")]
    anchor_score: f64,
}

#[derive(Debug, Clone, Serialize)]
struct CuratedSummary {
    #[serde(rename = "dataset_results")]
    datasets: Vec<DatasetSummary>,
    #[serde(rename = "aggregate_metrics")]
    overall: ModeSummary,
}

#[derive(Debug, Clone, Serialize)]
struct SyntheticSeedDatasetSummary {
    #[serde(rename = "source_dataset_name")]
    dataset: String,
    #[serde(rename = "input_pdf_path")]
    input_pdf: String,
    seed: u64,
    candidate_pool_count: usize,
    #[serde(rename = "sampled_redaction_rows")]
    sampled_rows: usize,
    #[serde(rename = "dataset_result")]
    summary: DatasetSummary,
}

#[derive(Debug, Clone, Serialize)]
struct SyntheticSummary {
    #[serde(rename = "synthetic_input_config")]
    config: SyntheticConfigSummary,
    #[serde(rename = "seed_dataset_results")]
    seed_datasets: Vec<SyntheticSeedDatasetSummary>,
    #[serde(rename = "aggregate_metrics")]
    overall: ModeSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeadlineSummary {
    #[serde(rename = "score_source", alias = "source")]
    source: String,
    #[serde(rename = "score", alias = "value")]
    value: f64,
    #[serde(rename = "score_formula", alias = "formula")]
    formula: String,
}

#[derive(Debug, Clone, Serialize)]
struct MetricDelta {
    #[serde(rename = "metric_name")]
    metric: String,
    #[serde(rename = "goal_direction")]
    goal: String,
    #[serde(rename = "baseline_value")]
    baseline: f64,
    #[serde(rename = "current_value")]
    current: f64,
    #[serde(rename = "absolute_delta")]
    delta_abs: f64,
    #[serde(rename = "percent_delta")]
    delta_pct: Option<f64>,
    #[serde(rename = "trend_direction")]
    trend: String,
}

#[derive(Debug, Clone, Serialize)]
struct BaselineCompare {
    #[serde(rename = "baseline_report_path")]
    baseline_path: String,
    #[serde(rename = "metric_deltas")]
    metrics: Vec<MetricDelta>,
}

#[derive(Debug, Clone, Serialize)]
struct ConsistencySummary {
    #[serde(rename = "repeated_run_count")]
    repeats: usize,
    #[serde(rename = "all_run_hashes_identical")]
    all_hashes_identical: bool,
    #[serde(rename = "hash_match_ratio_against_first_run")]
    hash_match_ratio: f64,
    run_hashes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AnchorAccuracyBenchmark {
    #[serde(rename = "benchmark_contract")]
    contract: ContractSummary,
    #[serde(rename = "metric_definitions")]
    definitions: MetricDefinitions,
    #[serde(rename = "curated_anchor_results")]
    curated: CuratedSummary,
    #[serde(rename = "synthetic_anchor_results")]
    synthetic: SyntheticSummary,
    #[serde(rename = "curated_headline_score")]
    headline: HeadlineSummary,
    #[serde(rename = "repeat_consistency")]
    consistency: ConsistencySummary,
    #[serde(rename = "baseline_comparison")]
    baseline_compare: Option<BaselineCompare>,
    #[serde(rename = "created_new_baseline")]
    baseline_bootstrapped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BaselineSnapshot {
    contract: ContractSummary,
    #[serde(rename = "curated_aggregate_metrics", alias = "curated_overall")]
    curated_overall: ModeSummary,
    #[serde(rename = "synthetic_aggregate_metrics", alias = "synthetic_overall")]
    synthetic_overall: ModeSummary,
    #[serde(rename = "curated_headline_score", alias = "headline")]
    headline: HeadlineSummary,
}

#[derive(Debug, Clone, Parser)]
#[command(
    name = "anchor_accuracy_benchmark",
    about = "Measure anchor resolver quality with curated and deterministic synthetic inputs."
)]
struct CliOptions {
    #[arg(long = "out", default_value = "benchmark/anchor_benchmark_report.json")]
    out_path: PathBuf,
    #[arg(long = "baseline-out")]
    baseline_out_path: Option<PathBuf>,
    #[arg(long, default_value_t = 2_usize, value_parser = parse_positive_usize)]
    repeats: usize,
}

#[derive(Debug, Clone, Default)]
struct SideAccumulator {
    expected: usize,
    resolved: usize,
    text_exact: usize,
    text_soft: usize,
    x_errors: Vec<f64>,
    x_within_tol_count: usize,
    source_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default)]
struct DatasetAccumulator {
    rows_total: usize,
    rows_with_selected_candidate: usize,
    rows_with_expected_mode: usize,
    rows_mode_matched: usize,
    anchor_mode_counts: BTreeMap<String, usize>,
    left: SideAccumulator,
    right: SideAccumulator,
}

#[derive(Debug, Clone)]
struct DatasetEvalDetail {
    summary: DatasetSummary,
    acc: DatasetAccumulator,
}

#[derive(Debug, Clone)]
struct RunSnapshot {
    hash: String,
}

#[derive(Debug, Clone, Copy)]
struct LcgRng {
    state: u64,
}

impl LcgRng {
    #[inline]
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15_u64),
        }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005_u64)
            .wrapping_add(1_u64);
        self.state ^ (self.state >> 33)
    }

    #[inline]
    fn shuffle<T>(&mut self, values: &mut [T]) {
        if values.len() <= 1 {
            return;
        }
        for idx in (1..values.len()).rev() {
            let swap_idx = (self.next_u64() as usize) % (idx + 1);
            values.swap(idx, swap_idx);
        }
    }
}

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|error| format!("invalid usize value '{value}': {error}"))?;
    if parsed == 0 {
        return Err("value must be > 0".to_owned());
    }
    Ok(parsed)
}

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create output directory {}: {error}",
            parent.display()
        )
    })
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    ensure_parent_dir(path)?;
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode json for {}: {error}", path.display()))?;
    std::fs::write(path, encoded)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn write_text_file(path: &Path, value: &str) -> Result<(), String> {
    ensure_parent_dir(path)?;
    std::fs::write(path, value)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn default_baseline_out_path(out_path: &Path) -> PathBuf {
    let parent = out_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = out_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("anchor_benchmark_report");
    parent.join(format!("{stem}.baseline.json"))
}

fn default_summary_out_path(out_path: &Path) -> PathBuf {
    let parent = out_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = out_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("anchor_benchmark_report");
    parent.join(format!("{stem}.summary.md"))
}

fn load_redaction_report(path: &Path) -> Result<RedactionReport, String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "failed to read redaction report {}: {error}",
            path.display()
        )
    })?;
    serde_json::from_slice::<RedactionReport>(&bytes).map_err(|error| {
        format!(
            "failed to parse redaction report {}: {error}",
            path.display()
        )
    })
}

fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_ascii_uppercase()
}

fn text_soft_match(left: &str, right: &str) -> bool {
    let left_norm = normalize_text(left);
    let right_norm = normalize_text(right);
    if left_norm.is_empty() || right_norm.is_empty() {
        return false;
    }
    left_norm == right_norm || left_norm.contains(&right_norm) || right_norm.contains(&left_norm)
}

fn safe_ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return if numerator == 0 { 1.0_f64 } else { 0.0_f64 };
    }
    numerator as f64 / denominator as f64
}

fn percentile_sorted(values: &[f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let quantile = q.clamp(0.0_f64, 1.0_f64);
    let idx = ((values.len().saturating_sub(1) as f64) * quantile).round() as usize;
    values.get(idx).copied()
}

fn parse_row_index(anchor_row_id: &str) -> Option<usize> {
    let marker = "_row";
    let marker_index = anchor_row_id.rfind(marker)?;
    let value = anchor_row_id.get(marker_index + marker.len()..)?;
    value.parse::<usize>().ok()
}

fn selected_candidate(decision: &AnchorDecisionRecord) -> Option<&AnchorCandidateDecision> {
    if let Some(candidate_id) = decision.selected_candidate_id.as_deref() {
        if let Some(candidate) = decision
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id == candidate_id)
        {
            return Some(candidate);
        }
    }
    decision
        .candidates
        .iter()
        .find(|candidate| candidate.was_selected)
}

fn selected_side(
    candidate: &AnchorCandidateDecision,
    anchor_type: AnchorType,
) -> Option<&AnchorSideDecision> {
    match anchor_type {
        AnchorType::Left => candidate.left.as_ref(),
        AnchorType::Right => candidate.right.as_ref(),
    }
}

fn expected_side(
    redaction: &RedactionOccurrence,
    anchor_type: AnchorType,
) -> Option<UnderlyingTextHit> {
    let hit = match anchor_type {
        AnchorType::Left => redaction.underlying_text.first(),
        AnchorType::Right => redaction.underlying_text.get(1),
    }?;
    if hit.text.trim().is_empty() {
        return None;
    }
    Some(hit.clone())
}

fn expected_mode(left_expected: bool, right_expected: bool) -> Option<&'static str> {
    match (left_expected, right_expected) {
        (true, true) => Some("two_sided"),
        (true, false) => Some("left_only"),
        (false, true) => Some("right_only"),
        (false, false) => None,
    }
}

fn accumulate_side(
    acc: &mut SideAccumulator,
    expected: Option<&UnderlyingTextHit>,
    observed: Option<&AnchorSideDecision>,
) {
    let Some(expected_hit) = expected else {
        return;
    };
    acc.expected += 1;
    let Some(observed_side) = observed else {
        return;
    };
    acc.resolved += 1;
    if normalize_text(&observed_side.text) == normalize_text(&expected_hit.text) {
        acc.text_exact += 1;
    }
    if text_soft_match(&observed_side.text, &expected_hit.text) {
        acc.text_soft += 1;
    }
    let error = (observed_side.x as f64 - expected_hit.bbox.x0 as f64).abs();
    if error.is_finite() {
        acc.x_errors.push(error);
        if error <= POSITION_TOLERANCE_PT {
            acc.x_within_tol_count += 1;
        }
    }
    let source_key = format!("{:?}", observed_side.selected_source).to_ascii_lowercase();
    *acc.source_counts.entry(source_key).or_insert(0_usize) += 1;
}

fn finalize_side(mut acc: SideAccumulator) -> SideAccuracySummary {
    acc.x_errors
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let x_error_mae_pt = if acc.x_errors.is_empty() {
        None
    } else {
        Some(acc.x_errors.iter().sum::<f64>() / acc.x_errors.len() as f64)
    };
    SideAccuracySummary {
        expected: acc.expected,
        resolved: acc.resolved,
        resolved_recall: safe_ratio(acc.resolved, acc.expected),
        text_exact: acc.text_exact,
        text_exact_recall: safe_ratio(acc.text_exact, acc.expected),
        text_soft: acc.text_soft,
        text_soft_recall: safe_ratio(acc.text_soft, acc.expected),
        x_error_mae_pt,
        x_error_p90_pt: percentile_sorted(&acc.x_errors, 0.9_f64),
        x_within_tol_count: acc.x_within_tol_count,
        x_within_tol_recall: safe_ratio(acc.x_within_tol_count, acc.expected),
        source_counts: acc.source_counts,
    }
}

fn score_components(summary: &DatasetAccumulator) -> (f64, f64, f64, f64) {
    let side_expected_total = summary.left.expected + summary.right.expected;
    let side_text_exact_total = summary.left.text_exact + summary.right.text_exact;
    let side_within_tol_total = summary.left.x_within_tol_count + summary.right.x_within_tol_count;
    let text_exact_recall = safe_ratio(side_text_exact_total, side_expected_total);
    let x_within_tol_recall = safe_ratio(side_within_tol_total, side_expected_total);
    let row_selected_ratio = safe_ratio(summary.rows_with_selected_candidate, summary.rows_total);
    let row_mode_match_ratio =
        safe_ratio(summary.rows_mode_matched, summary.rows_with_expected_mode);
    (
        row_selected_ratio,
        row_mode_match_ratio,
        text_exact_recall,
        x_within_tol_recall,
    )
}

fn anchor_score(summary: &DatasetAccumulator) -> f64 {
    let (row_selected_ratio, row_mode_match_ratio, text_exact_recall, x_within_tol_recall) =
        score_components(summary);
    (0.10_f64 * row_selected_ratio
        + 0.10_f64 * row_mode_match_ratio
        + 0.45_f64 * text_exact_recall
        + 0.35_f64 * x_within_tol_recall)
        .clamp(0.0_f64, 1.0_f64)
}

fn map_decisions_by_row<'a>(
    redactions: &[RedactionOccurrence],
    decisions: &'a [AnchorDecisionRecord],
) -> Vec<Option<&'a AnchorDecisionRecord>> {
    let mut by_index = vec![None; redactions.len()];
    for decision in decisions {
        if let Some(index) = parse_row_index(&decision.anchor_row_id) {
            if index < by_index.len() && by_index[index].is_none() {
                by_index[index] = Some(decision);
            }
        }
    }
    for (index, decision) in decisions.iter().enumerate() {
        if index < by_index.len() && by_index[index].is_none() {
            by_index[index] = Some(decision);
        }
    }
    by_index
}

fn evaluate_anchor_report(
    dataset_name: &str,
    input_pdf: &str,
    redactions: &RedactionReport,
    report: &AnchorReport,
) -> DatasetEvalDetail {
    let mut acc = DatasetAccumulator {
        rows_total: redactions.redactions.len(),
        ..DatasetAccumulator::default()
    };
    let decisions = map_decisions_by_row(&redactions.redactions, &report.decisions);
    for (index, redaction) in redactions.redactions.iter().enumerate() {
        let left_expected = expected_side(redaction, AnchorType::Left);
        let right_expected = expected_side(redaction, AnchorType::Right);
        let expected_mode = expected_mode(left_expected.is_some(), right_expected.is_some());
        if expected_mode.is_some() {
            acc.rows_with_expected_mode += 1;
        }

        let decision = decisions.get(index).and_then(|value| *value);
        let selected = decision.and_then(selected_candidate);
        if selected.is_some() {
            acc.rows_with_selected_candidate += 1;
        }
        if let Some(selected_candidate) = selected {
            let selected_mode = decision
                .and_then(|row| row.selected_mode.as_deref())
                .unwrap_or(selected_candidate.anchor_mode.as_str());
            *acc.anchor_mode_counts
                .entry(selected_mode.to_owned())
                .or_insert(0_usize) += 1;
            if expected_mode
                .map(|mode| mode == selected_mode)
                .unwrap_or(false)
            {
                acc.rows_mode_matched += 1;
            }
            accumulate_side(
                &mut acc.left,
                left_expected.as_ref(),
                selected_side(selected_candidate, AnchorType::Left),
            );
            accumulate_side(
                &mut acc.right,
                right_expected.as_ref(),
                selected_side(selected_candidate, AnchorType::Right),
            );
        } else {
            accumulate_side(&mut acc.left, left_expected.as_ref(), None);
            accumulate_side(&mut acc.right, right_expected.as_ref(), None);
        }
    }
    let side_expected_total = acc.left.expected + acc.right.expected;
    let side_resolved_total = acc.left.resolved + acc.right.resolved;
    let text_exact_recall_overall = safe_ratio(
        acc.left.text_exact + acc.right.text_exact,
        side_expected_total,
    );
    let text_soft_recall_overall = safe_ratio(
        acc.left.text_soft + acc.right.text_soft,
        side_expected_total,
    );
    let x_within_tol_recall_overall = safe_ratio(
        acc.left.x_within_tol_count + acc.right.x_within_tol_count,
        side_expected_total,
    );
    let mut all_errors = acc.left.x_errors.clone();
    all_errors.extend(acc.right.x_errors.iter().copied());
    let x_error_mae_overall_pt = if all_errors.is_empty() {
        None
    } else {
        Some(all_errors.iter().sum::<f64>() / all_errors.len() as f64)
    };
    let summary = DatasetSummary {
        name: dataset_name.to_owned(),
        input_pdf: input_pdf.to_owned(),
        redaction_input: redactions.input.clone(),
        rows_total: acc.rows_total,
        rows_with_selected_candidate: acc.rows_with_selected_candidate,
        row_selected_ratio: safe_ratio(acc.rows_with_selected_candidate, acc.rows_total),
        row_mode_match_ratio: safe_ratio(acc.rows_mode_matched, acc.rows_with_expected_mode),
        anchor_mode_counts: acc.anchor_mode_counts.clone(),
        left: finalize_side(acc.left.clone()),
        right: finalize_side(acc.right.clone()),
        side_expected_total,
        side_resolved_total,
        text_exact_recall_overall,
        text_soft_recall_overall,
        x_within_tol_recall_overall,
        x_error_mae_overall_pt,
        anchor_score: anchor_score(&acc),
    };
    DatasetEvalDetail { summary, acc }
}

fn merge_mode_details(details: &[DatasetEvalDetail]) -> ModeSummary {
    let mut merged = DatasetAccumulator::default();
    for detail in details {
        merged.rows_total += detail.acc.rows_total;
        merged.rows_with_selected_candidate += detail.acc.rows_with_selected_candidate;
        merged.rows_with_expected_mode += detail.acc.rows_with_expected_mode;
        merged.rows_mode_matched += detail.acc.rows_mode_matched;
        for (key, value) in &detail.acc.anchor_mode_counts {
            *merged
                .anchor_mode_counts
                .entry(key.clone())
                .or_insert(0_usize) += *value;
        }
        merged.left.expected += detail.acc.left.expected;
        merged.left.resolved += detail.acc.left.resolved;
        merged.left.text_exact += detail.acc.left.text_exact;
        merged.left.text_soft += detail.acc.left.text_soft;
        merged.left.x_within_tol_count += detail.acc.left.x_within_tol_count;
        merged
            .left
            .x_errors
            .extend(detail.acc.left.x_errors.iter().copied());
        for (key, value) in &detail.acc.left.source_counts {
            *merged
                .left
                .source_counts
                .entry(key.clone())
                .or_insert(0_usize) += *value;
        }
        merged.right.expected += detail.acc.right.expected;
        merged.right.resolved += detail.acc.right.resolved;
        merged.right.text_exact += detail.acc.right.text_exact;
        merged.right.text_soft += detail.acc.right.text_soft;
        merged.right.x_within_tol_count += detail.acc.right.x_within_tol_count;
        merged
            .right
            .x_errors
            .extend(detail.acc.right.x_errors.iter().copied());
        for (key, value) in &detail.acc.right.source_counts {
            *merged
                .right
                .source_counts
                .entry(key.clone())
                .or_insert(0_usize) += *value;
        }
    }
    let left = finalize_side(merged.left);
    let right = finalize_side(merged.right);
    let side_expected_total = left.expected + right.expected;
    let side_resolved_total = left.resolved + right.resolved;
    let side_text_exact_total = left.text_exact + right.text_exact;
    let side_text_soft_total = left.text_soft + right.text_soft;
    let side_within_tol_total = left.x_within_tol_count + right.x_within_tol_count;
    let row_selected_ratio = safe_ratio(merged.rows_with_selected_candidate, merged.rows_total);
    let row_mode_match_ratio = safe_ratio(merged.rows_mode_matched, merged.rows_with_expected_mode);
    let text_exact_recall = safe_ratio(side_text_exact_total, side_expected_total);
    let text_soft_recall = safe_ratio(side_text_soft_total, side_expected_total);
    let x_within_tol_recall = safe_ratio(side_within_tol_total, side_expected_total);
    let x_error_sum = left.x_error_mae_pt.unwrap_or(0.0_f64) * left.resolved as f64
        + right.x_error_mae_pt.unwrap_or(0.0_f64) * right.resolved as f64;
    let x_error_mae_overall_pt = if side_resolved_total == 0 {
        None
    } else {
        Some(x_error_sum / side_resolved_total as f64)
    };
    let anchor_score = (0.10_f64 * row_selected_ratio
        + 0.10_f64 * row_mode_match_ratio
        + 0.45_f64 * text_exact_recall
        + 0.35_f64 * x_within_tol_recall)
        .clamp(0.0_f64, 1.0_f64);
    ModeSummary {
        dataset_count: details.len(),
        rows_total: merged.rows_total,
        rows_with_selected_candidate: merged.rows_with_selected_candidate,
        row_selected_ratio,
        row_mode_match_ratio,
        anchor_mode_counts: merged.anchor_mode_counts,
        left,
        right,
        side_expected_total,
        side_resolved_total,
        text_exact_recall_overall: text_exact_recall,
        text_soft_recall_overall: text_soft_recall,
        x_within_tol_recall_overall: x_within_tol_recall,
        x_error_mae_overall_pt,
        anchor_score,
    }
}

fn source_hits_for_synthetic(
    hits_by_page: &BTreeMap<u32, Vec<UnderlyingTextHit>>,
    cfg: &AnchorSyntheticContract,
) -> Vec<RedactionOccurrence> {
    let mut out = Vec::<RedactionOccurrence>::new();
    for (page_index, page_hits) in hits_by_page {
        let mut hits = page_hits
            .iter()
            .filter(|hit| !hit.text.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            let left_center_y = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
            let right_center_y = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
            left_center_y
                .partial_cmp(&right_center_y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.bbox
                        .x0
                        .partial_cmp(&right.bbox.x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.text.cmp(&right.text))
        });
        for left_idx in 0..hits.len() {
            let left = &hits[left_idx];
            for right in hits
                .iter()
                .take((left_idx + 6).min(hits.len()))
                .skip(left_idx + 1)
            {
                let left_center_y = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
                let right_center_y = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
                if (left_center_y - right_center_y).abs() > cfg.max_center_y_delta_pt {
                    continue;
                }
                let gap = right.bbox.x0 - left.bbox.x1;
                if gap < cfg.min_gap_pt || gap > cfg.max_gap_pt {
                    continue;
                }
                let x0 = left.bbox.x1 + 0.5_f32;
                let x1 = right.bbox.x0 - 0.5_f32;
                if x1 <= x0 {
                    continue;
                }
                let y0 = left.bbox.y0.min(right.bbox.y0);
                let y1 = left.bbox.y1.max(right.bbox.y1);
                let bbox = Rect::new(x0, y0, x1, y1);
                if bbox.area() <= 0.0_f32 {
                    continue;
                }
                out.push(RedactionOccurrence {
                    page_index: *page_index,
                    bbox,
                    kind: RedactionKind::Unknown,
                    score: 1.0_f32,
                    meta: BTreeMap::new(),
                    underlying_text: vec![left.clone(), right.clone()],
                });
            }
        }
    }
    out.sort_by(|left, right| {
        left.page_index
            .cmp(&right.page_index)
            .then_with(|| {
                left.bbox
                    .y0
                    .partial_cmp(&right.bbox.y0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    out
}

fn sample_redactions(
    seed: u64,
    source: &[RedactionOccurrence],
    count: usize,
) -> Vec<RedactionOccurrence> {
    if source.is_empty() || count == 0 {
        return Vec::new();
    }
    let mut values = source.to_vec();
    let mut rng = LcgRng::new(seed);
    rng.shuffle(values.as_mut_slice());
    values.truncate(count.min(values.len()));
    values.sort_by(|left, right| {
        left.page_index
            .cmp(&right.page_index)
            .then_with(|| {
                left.bbox
                    .y0
                    .partial_cmp(&right.bbox.y0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    values
}

fn synthetic_report(
    input_pdf: &str,
    redactions: Vec<RedactionOccurrence>,
    seed: u64,
) -> RedactionReport {
    let mut page_counts = BTreeMap::<u32, u32>::new();
    for redaction in &redactions {
        *page_counts.entry(redaction.page_index).or_insert(0_u32) += 1_u32;
    }
    RedactionReport {
        input: format!("synthetic://{input_pdf}#seed={seed}"),
        count: redactions.len() as u32,
        redactions,
        page_counts,
        diagnostics: vec![format!("synthetic_seed={seed}")],
    }
}

fn hash_json<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(bytes.as_slice());
    Ok(format!("{:x}", hasher.finalize()))
}

fn metric_definitions() -> MetricDefinitions {
    MetricDefinitions {
        row_selected_ratio: "rows_with_selected_candidate / rows_total. Higher is better.",
        row_mode_match_ratio:
            "rows where selected anchor mode matches expected mode from known left/right availability. Higher is better.",
        text_exact_recall:
            "expected anchor sides resolved with normalized exact text match. Higher is better.",
        text_soft_recall:
            "expected anchor sides resolved with normalized containment text match. Higher is better.",
        x_error_mae_pt:
            "mean absolute error in points between selected anchor x and expected anchor x. Lower is better.",
        x_error_p90_pt: "90th percentile of absolute anchor x error in points. Lower is better.",
        x_within_tol_recall:
            "expected anchor sides resolved with absolute x error <= position tolerance. Higher is better.",
        anchor_score:
            "composite score: 0.45*text_exact_recall + 0.35*x_within_tol_recall + 0.10*row_selected_ratio + 0.10*row_mode_match_ratio.",
        headline:
            "headline score from curated mode only (no synthetic contribution): curated_headline_score.score = curated_anchor_results.aggregate_metrics.anchor_quality_score.",
        position_tolerance_pt: "absolute x error tolerance in points for x_within_tol_recall.",
    }
}

fn trend_label(delta_abs: f64) -> &'static str {
    if delta_abs > TREND_EPSILON {
        "up"
    } else if delta_abs < -TREND_EPSILON {
        "down"
    } else {
        "flat"
    }
}

fn metric_delta(metric: &str, goal: &str, current: f64, baseline: f64) -> MetricDelta {
    let delta_abs = current - baseline;
    let delta_pct = if baseline.abs() <= TREND_EPSILON {
        None
    } else {
        Some((delta_abs / baseline.abs()) * 100.0_f64)
    };
    MetricDelta {
        metric: metric.to_owned(),
        goal: goal.to_owned(),
        baseline,
        current,
        delta_abs,
        delta_pct,
        trend: trend_label(delta_abs).to_owned(),
    }
}

fn build_baseline_compare(
    current_curated: &ModeSummary,
    current_synthetic: &ModeSummary,
    current_headline: &HeadlineSummary,
    baseline: &BaselineSnapshot,
    baseline_path: &Path,
) -> BaselineCompare {
    let mut metrics = vec![
        metric_delta(
            "curated_headline_score.score",
            "higher_is_better",
            current_headline.value,
            baseline.headline.value,
        ),
        metric_delta(
            "curated_anchor_results.aggregate_metrics.anchor_quality_score",
            "higher_is_better",
            current_curated.anchor_score,
            baseline.curated_overall.anchor_score,
        ),
        metric_delta(
            "curated_anchor_results.aggregate_metrics.row_selection_rate",
            "higher_is_better",
            current_curated.row_selected_ratio,
            baseline.curated_overall.row_selected_ratio,
        ),
        metric_delta(
            "curated_anchor_results.aggregate_metrics.left_side_metrics.exact_text_recall",
            "higher_is_better",
            current_curated.left.text_exact_recall,
            baseline.curated_overall.left.text_exact_recall,
        ),
        metric_delta(
            "curated_anchor_results.aggregate_metrics.right_side_metrics.exact_text_recall",
            "higher_is_better",
            current_curated.right.text_exact_recall,
            baseline.curated_overall.right.text_exact_recall,
        ),
        metric_delta(
            "curated_anchor_results.aggregate_metrics.left_side_metrics.x_within_tolerance_recall",
            "higher_is_better",
            current_curated.left.x_within_tol_recall,
            baseline.curated_overall.left.x_within_tol_recall,
        ),
        metric_delta(
            "curated_anchor_results.aggregate_metrics.right_side_metrics.x_within_tolerance_recall",
            "higher_is_better",
            current_curated.right.x_within_tol_recall,
            baseline.curated_overall.right.x_within_tol_recall,
        ),
        metric_delta(
            "curated_anchor_results.aggregate_metrics.overall_soft_text_recall",
            "higher_is_better",
            current_curated.text_soft_recall_overall,
            baseline.curated_overall.text_soft_recall_overall,
        ),
        metric_delta(
            "synthetic_anchor_results.aggregate_metrics.anchor_quality_score",
            "higher_is_better",
            current_synthetic.anchor_score,
            baseline.synthetic_overall.anchor_score,
        ),
    ];
    if let (Some(current_value), Some(baseline_value)) = (
        current_curated.x_error_mae_overall_pt,
        baseline.curated_overall.x_error_mae_overall_pt,
    ) {
        metrics.push(metric_delta(
            "curated_anchor_results.aggregate_metrics.overall_x_error_mean_absolute_pt",
            "lower_is_better",
            current_value,
            baseline_value,
        ));
    }
    BaselineCompare {
        baseline_path: baseline_path.display().to_string(),
        metrics,
    }
}

fn consistency_summary(run_snapshots: &[RunSnapshot]) -> ConsistencySummary {
    if run_snapshots.is_empty() {
        return ConsistencySummary {
            repeats: 0,
            all_hashes_identical: true,
            hash_match_ratio: 1.0_f64,
            run_hashes: Vec::new(),
        };
    }
    let baseline_hash = &run_snapshots[0].hash;
    let matched = run_snapshots
        .iter()
        .filter(|snapshot| &snapshot.hash == baseline_hash)
        .count();
    ConsistencySummary {
        repeats: run_snapshots.len(),
        all_hashes_identical: matched == run_snapshots.len(),
        hash_match_ratio: matched as f64 / run_snapshots.len() as f64,
        run_hashes: run_snapshots
            .iter()
            .map(|snapshot| snapshot.hash.clone())
            .collect::<Vec<_>>(),
    }
}

fn format_optional(value: Option<f64>) -> String {
    value
        .map(|item| format!("{item:.4}"))
        .unwrap_or_else(|| "-".to_owned())
}

fn append_mode_console_table(out: &mut String, label: &str, summary: &ModeSummary) {
    writeln!(out, "{label}").unwrap();
    writeln!(out, "  {:<34} {:>10}", "Datasets", summary.dataset_count).unwrap();
    writeln!(out, "  {:<34} {:>10}", "Redaction rows", summary.rows_total).unwrap();
    writeln!(
        out,
        "  {:<34} {:>10.4}",
        "Row selection rate", summary.row_selected_ratio
    )
    .unwrap();
    writeln!(
        out,
        "  {:<34} {:>10.4}",
        "Row mode match rate", summary.row_mode_match_ratio
    )
    .unwrap();
    writeln!(
        out,
        "  {:<34} {:>10.4}",
        "Anchor quality score", summary.anchor_score
    )
    .unwrap();
    writeln!(
        out,
        "  {:<34} {:>10.4}",
        "Overall exact text recall", summary.text_exact_recall_overall
    )
    .unwrap();
    writeln!(
        out,
        "  {:<34} {:>10.4}",
        "Overall soft text recall", summary.text_soft_recall_overall
    )
    .unwrap();
    writeln!(
        out,
        "  {:<34} {:>10.4}",
        "Overall x within tolerance", summary.x_within_tol_recall_overall
    )
    .unwrap();
    writeln!(
        out,
        "  {:<34} {:>10}",
        "Overall x error MAE (pt)",
        format_optional(summary.x_error_mae_overall_pt)
    )
    .unwrap();
}

fn render_console_summary(
    payload: &AnchorAccuracyBenchmark,
    json_out_path: &Path,
    summary_out_path: &Path,
    baseline_out_path: &Path,
) -> String {
    let mut out = String::new();
    writeln!(&mut out, "Anchor Benchmark Report").unwrap();
    writeln!(&mut out, "=======================").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "Files").unwrap();
    writeln!(&mut out, "  JSON report: {}", json_out_path.display()).unwrap();
    writeln!(
        &mut out,
        "  Markdown summary: {}",
        summary_out_path.display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  Baseline snapshot: {}",
        baseline_out_path.display()
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Curated Headline Score").unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10}",
        "Score source", payload.headline.source
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10.4}",
        "Headline score", payload.headline.value
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Curated Dataset Results").unwrap();
    writeln!(
        &mut out,
        "  {:<16} {:>6} {:>8} {:>8} {:>8} {:>10} {:>10}",
        "Dataset", "Rows", "Select", "Mode", "Score", "Soft text", "X MAE pt"
    )
    .unwrap();
    for dataset in &payload.curated.datasets {
        writeln!(
            &mut out,
            "  {:<16} {:>6} {:>8.4} {:>8.4} {:>8.4} {:>10.4} {:>10}",
            dataset.name,
            dataset.rows_total,
            dataset.row_selected_ratio,
            dataset.row_mode_match_ratio,
            dataset.anchor_score,
            dataset.text_soft_recall_overall,
            format_optional(dataset.x_error_mae_overall_pt)
        )
        .unwrap();
    }
    writeln!(&mut out).unwrap();

    append_mode_console_table(
        &mut out,
        "Curated Aggregate Metrics",
        &payload.curated.overall,
    );
    writeln!(&mut out).unwrap();
    append_mode_console_table(
        &mut out,
        "Synthetic Aggregate Metrics",
        &payload.synthetic.overall,
    );
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Repeat Consistency").unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10}",
        "Repeated runs", payload.consistency.repeats
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10}",
        "All hashes identical", payload.consistency.all_hashes_identical
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10.4}",
        "First-run match ratio", payload.consistency.hash_match_ratio
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Baseline Comparison").unwrap();
    if let Some(compare) = &payload.baseline_compare {
        writeln!(&mut out, "  Baseline report: {}", compare.baseline_path).unwrap();
        writeln!(
            &mut out,
            "  {:<64} {:>10} {:>10} {:>10} {:>8}",
            "Metric", "Baseline", "Current", "Delta", "Trend"
        )
        .unwrap();
        for metric in &compare.metrics {
            writeln!(
                &mut out,
                "  {:<64} {:>10.4} {:>10.4} {:>10.4} {:>8}",
                metric.metric, metric.baseline, metric.current, metric.delta_abs, metric.trend
            )
            .unwrap();
        }
    } else {
        writeln!(
            &mut out,
            "  No previous baseline report found. This run created the baseline snapshot."
        )
        .unwrap();
    }
    out
}

fn append_mode_markdown_table(out: &mut String, label: &str, summary: &ModeSummary) {
    writeln!(out, "## {label}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "| Metric | Value |").unwrap();
    writeln!(out, "| --- | ---: |").unwrap();
    writeln!(out, "| Datasets | {} |", summary.dataset_count).unwrap();
    writeln!(out, "| Redaction rows | {} |", summary.rows_total).unwrap();
    writeln!(
        out,
        "| Row selection rate | {:.4} |",
        summary.row_selected_ratio
    )
    .unwrap();
    writeln!(
        out,
        "| Row mode match rate | {:.4} |",
        summary.row_mode_match_ratio
    )
    .unwrap();
    writeln!(
        out,
        "| Anchor quality score | {:.4} |",
        summary.anchor_score
    )
    .unwrap();
    writeln!(
        out,
        "| Overall exact text recall | {:.4} |",
        summary.text_exact_recall_overall
    )
    .unwrap();
    writeln!(
        out,
        "| Overall soft text recall | {:.4} |",
        summary.text_soft_recall_overall
    )
    .unwrap();
    writeln!(
        out,
        "| Overall x within tolerance recall | {:.4} |",
        summary.x_within_tol_recall_overall
    )
    .unwrap();
    writeln!(
        out,
        "| Overall x error mean absolute (pt) | {} |",
        format_optional(summary.x_error_mae_overall_pt)
    )
    .unwrap();
    writeln!(out).unwrap();
}

fn render_markdown_summary(
    payload: &AnchorAccuracyBenchmark,
    json_out_path: &Path,
    summary_out_path: &Path,
    baseline_out_path: &Path,
) -> String {
    let mut out = String::new();
    writeln!(&mut out, "# Anchor Benchmark Report").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "## Files").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "- JSON report: `{}`", json_out_path.display()).unwrap();
    writeln!(
        &mut out,
        "- Markdown summary: `{}`",
        summary_out_path.display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Baseline snapshot: `{}`",
        baseline_out_path.display()
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "## Curated Headline Score").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "| Metric | Value |").unwrap();
    writeln!(&mut out, "| --- | ---: |").unwrap();
    writeln!(&mut out, "| Score source | {} |", payload.headline.source).unwrap();
    writeln!(
        &mut out,
        "| Headline score | {:.4} |",
        payload.headline.value
    )
    .unwrap();
    writeln!(&mut out, "| Formula | `{}` |", payload.headline.formula).unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "## Curated Dataset Results").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(
        &mut out,
        "| Dataset | Rows | Selection rate | Mode match rate | Anchor score | Soft text recall | X error MAE (pt) |"
    )
    .unwrap();
    writeln!(
        &mut out,
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: |"
    )
    .unwrap();
    for dataset in &payload.curated.datasets {
        writeln!(
            &mut out,
            "| {} | {} | {:.4} | {:.4} | {:.4} | {:.4} | {} |",
            dataset.name,
            dataset.rows_total,
            dataset.row_selected_ratio,
            dataset.row_mode_match_ratio,
            dataset.anchor_score,
            dataset.text_soft_recall_overall,
            format_optional(dataset.x_error_mae_overall_pt)
        )
        .unwrap();
    }
    writeln!(&mut out).unwrap();

    append_mode_markdown_table(
        &mut out,
        "Curated Aggregate Metrics",
        &payload.curated.overall,
    );
    append_mode_markdown_table(
        &mut out,
        "Synthetic Aggregate Metrics",
        &payload.synthetic.overall,
    );

    writeln!(&mut out, "## Repeat Consistency").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "| Metric | Value |").unwrap();
    writeln!(&mut out, "| --- | ---: |").unwrap();
    writeln!(
        &mut out,
        "| Repeated runs | {} |",
        payload.consistency.repeats
    )
    .unwrap();
    writeln!(
        &mut out,
        "| All hashes identical | {} |",
        payload.consistency.all_hashes_identical
    )
    .unwrap();
    writeln!(
        &mut out,
        "| First-run match ratio | {:.4} |",
        payload.consistency.hash_match_ratio
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "## Baseline Comparison").unwrap();
    writeln!(&mut out).unwrap();
    if let Some(compare) = &payload.baseline_compare {
        writeln!(&mut out, "Baseline report: `{}`", compare.baseline_path).unwrap();
        writeln!(&mut out).unwrap();
        writeln!(&mut out, "| Metric | Baseline | Current | Delta | Trend |").unwrap();
        writeln!(&mut out, "| --- | ---: | ---: | ---: | --- |").unwrap();
        for metric in &compare.metrics {
            writeln!(
                &mut out,
                "| {} | {:.4} | {:.4} | {:.4} | {} |",
                metric.metric, metric.baseline, metric.current, metric.delta_abs, metric.trend
            )
            .unwrap();
        }
    } else {
        writeln!(
            &mut out,
            "No previous baseline report found. This run created the baseline snapshot."
        )
        .unwrap();
    }
    out
}

fn run_curated(
    contract: &RedactionDetectionContract,
) -> Result<(Vec<DatasetSummary>, ModeSummary), String> {
    let mut details = Vec::<DatasetEvalDetail>::new();
    for dataset in &contract.datasets {
        let input_path = Path::new(&dataset.input_pdf);
        if !input_path.exists() {
            return Err(format!(
                "missing curated dataset input {}",
                input_path.display()
            ));
        }
        let redaction_path = Path::new(&dataset.ground_truth_redactions);
        if !redaction_path.exists() {
            return Err(format!(
                "missing curated redaction report {}",
                redaction_path.display()
            ));
        }
        let pdf_bytes = std::fs::read(input_path)
            .map_err(|error| format!("failed to read {}: {error}", input_path.display()))?;
        let redactions = load_redaction_report(redaction_path)?;
        let diagnostics = ["benchmark_mode=curated".to_owned()];
        let anchor_report = run_anchor_from_redactions(ToolingAnchorRequest {
            input_name: dataset.name.as_str(),
            pdf_bytes: &pdf_bytes,
            redactions: &redactions,
            diagnostics: &diagnostics,
        })?;
        details.push(evaluate_anchor_report(
            &dataset.name,
            &dataset.input_pdf,
            &redactions,
            &anchor_report,
        ));
    }
    let datasets = details
        .iter()
        .map(|detail| detail.summary.clone())
        .collect::<Vec<_>>();
    Ok((datasets, merge_mode_details(details.as_slice())))
}

fn run_synthetic(
    contract: &RedactionDetectionContract,
    synthetic_cfg: &AnchorSyntheticContract,
) -> Result<(Vec<SyntheticSeedDatasetSummary>, ModeSummary), String> {
    let mut seed_datasets = Vec::<SyntheticSeedDatasetSummary>::new();
    let mut details = Vec::<DatasetEvalDetail>::new();
    for (dataset_index, dataset) in contract.datasets.iter().enumerate() {
        if synthetic_cfg.max_pdfs > 0 && dataset_index >= synthetic_cfg.max_pdfs {
            break;
        }
        let input_path = Path::new(&dataset.input_pdf);
        if !input_path.exists() {
            return Err(format!(
                "missing synthetic dataset input {}",
                input_path.display()
            ));
        }
        let pdf_bytes = std::fs::read(input_path)
            .map_err(|error| format!("failed to read {}: {error}", input_path.display()))?;
        let hits_by_page = collect_underlying_text_hits_by_page(&pdf_bytes)?;
        let candidate_pool = source_hits_for_synthetic(&hits_by_page, synthetic_cfg);
        for seed in &synthetic_cfg.seeds {
            let sampled = sample_redactions(
                *seed,
                candidate_pool.as_slice(),
                synthetic_cfg.samples_per_pdf,
            );
            let sampled_rows = sampled.len();
            let redaction_report = synthetic_report(dataset.input_pdf.as_str(), sampled, *seed);
            let diagnostics = [format!("benchmark_mode=synthetic seed={seed}")];
            let anchor_report = run_anchor_from_redactions(ToolingAnchorRequest {
                input_name: dataset.name.as_str(),
                pdf_bytes: &pdf_bytes,
                redactions: &redaction_report,
                diagnostics: &diagnostics,
            })?;
            let detail = evaluate_anchor_report(
                &format!("{}#seed={seed}", dataset.name),
                &dataset.input_pdf,
                &redaction_report,
                &anchor_report,
            );
            seed_datasets.push(SyntheticSeedDatasetSummary {
                dataset: dataset.name.clone(),
                input_pdf: dataset.input_pdf.clone(),
                seed: *seed,
                candidate_pool_count: candidate_pool.len(),
                sampled_rows,
                summary: detail.summary.clone(),
            });
            details.push(detail);
        }
    }
    Ok((seed_datasets, merge_mode_details(details.as_slice())))
}

fn headline_from_curated(curated: &ModeSummary) -> HeadlineSummary {
    HeadlineSummary {
        source: "curated_only".to_owned(),
        value: curated.anchor_score,
        formula: "curated_headline_score.score = curated_anchor_results.aggregate_metrics.anchor_quality_score".to_owned(),
    }
}

fn run_once(
    redaction_contract: &RedactionDetectionContract,
    synthetic_cfg: &AnchorSyntheticContract,
) -> Result<(CuratedSummary, SyntheticSummary, HeadlineSummary, String), String> {
    let (curated_datasets, curated_overall) = run_curated(redaction_contract)?;
    let (synthetic_seed_datasets, synthetic_overall) =
        run_synthetic(redaction_contract, synthetic_cfg)?;
    let headline = headline_from_curated(&curated_overall);
    let curated = CuratedSummary {
        datasets: curated_datasets,
        overall: curated_overall,
    };
    let synthetic = SyntheticSummary {
        config: SyntheticConfigSummary {
            contract_id: synthetic_cfg.contract_id.clone(),
            schema_version: synthetic_cfg.schema_version,
            seeds: synthetic_cfg.seeds.clone(),
            samples_per_pdf: synthetic_cfg.samples_per_pdf,
            max_pdfs: synthetic_cfg.max_pdfs,
            min_gap_pt: synthetic_cfg.min_gap_pt,
            max_gap_pt: synthetic_cfg.max_gap_pt,
            max_center_y_delta_pt: synthetic_cfg.max_center_y_delta_pt,
        },
        seed_datasets: synthetic_seed_datasets,
        overall: synthetic_overall,
    };
    let hash = hash_json(&(&curated, &synthetic, &headline))?;
    Ok((curated, synthetic, headline, hash))
}

fn main() {
    let options = CliOptions::parse();
    let redaction_contract = match canonical_redaction_detection_contract() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("failed to load redaction detection contract: {error}");
            std::process::exit(1);
        }
    };
    let synthetic_cfg = match canonical_anchor_synthetic_contract() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("failed to load anchor synthetic config: {error}");
            std::process::exit(1);
        }
    };
    let contract_summary = ContractSummary {
        contract_id: redaction_contract.contract_id.clone(),
        schema_version: redaction_contract.schema_version,
        dataset_count: redaction_contract.datasets.len(),
    };
    let baseline_out_path = options
        .baseline_out_path
        .clone()
        .unwrap_or_else(|| default_baseline_out_path(options.out_path.as_path()));
    let existing_baseline = if baseline_out_path.exists() {
        let bytes = match std::fs::read(&baseline_out_path) {
            Ok(value) => value,
            Err(error) => {
                eprintln!(
                    "failed to read baseline {}: {error}",
                    baseline_out_path.display()
                );
                std::process::exit(1);
            }
        };
        match serde_json::from_slice::<BaselineSnapshot>(&bytes) {
            Ok(value) => Some(value),
            Err(error) => {
                eprintln!(
                    "failed to parse baseline {}: {error}",
                    baseline_out_path.display()
                );
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    let mut selected_curated = None::<CuratedSummary>;
    let mut selected_synthetic = None::<SyntheticSummary>;
    let mut selected_headline = None::<HeadlineSummary>;
    let mut run_snapshots = Vec::<RunSnapshot>::new();
    for repeat in 0..options.repeats {
        let result = run_once(redaction_contract, synthetic_cfg);
        let (curated, synthetic, headline, hash) = match result {
            Ok(value) => value,
            Err(error) => {
                eprintln!("anchor benchmark failed on repeat {}: {error}", repeat + 1);
                std::process::exit(1);
            }
        };
        run_snapshots.push(RunSnapshot { hash });
        if selected_curated.is_none() {
            selected_curated = Some(curated);
            selected_synthetic = Some(synthetic);
            selected_headline = Some(headline);
        }
    }

    let curated = match selected_curated {
        Some(value) => value,
        None => {
            eprintln!("no anchor curated payload generated");
            std::process::exit(1);
        }
    };
    let synthetic = match selected_synthetic {
        Some(value) => value,
        None => {
            eprintln!("no anchor synthetic payload generated");
            std::process::exit(1);
        }
    };
    let headline = match selected_headline {
        Some(value) => value,
        None => {
            eprintln!("no anchor headline payload generated");
            std::process::exit(1);
        }
    };

    let baseline_compare = existing_baseline.as_ref().map(|baseline| {
        build_baseline_compare(
            &curated.overall,
            &synthetic.overall,
            &headline,
            baseline,
            &baseline_out_path,
        )
    });

    let payload = AnchorAccuracyBenchmark {
        contract: contract_summary.clone(),
        definitions: metric_definitions(),
        curated: CuratedSummary {
            datasets: curated.datasets.clone(),
            overall: curated.overall.clone(),
        },
        synthetic: SyntheticSummary {
            config: synthetic.config.clone(),
            seed_datasets: synthetic.seed_datasets.clone(),
            overall: synthetic.overall.clone(),
        },
        headline: headline.clone(),
        consistency: consistency_summary(run_snapshots.as_slice()),
        baseline_compare,
        baseline_bootstrapped: existing_baseline.is_none(),
    };
    let summary_out_path = default_summary_out_path(options.out_path.as_path());
    let console_summary = render_console_summary(
        &payload,
        options.out_path.as_path(),
        &summary_out_path,
        &baseline_out_path,
    );
    println!("{console_summary}");
    if let Err(error) = write_json_file(options.out_path.as_path(), &payload) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    let markdown_summary = render_markdown_summary(
        &payload,
        options.out_path.as_path(),
        &summary_out_path,
        &baseline_out_path,
    );
    if let Err(error) = write_text_file(&summary_out_path, &markdown_summary) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    println!("JSON report file: {}", options.out_path.display());
    println!("Markdown summary file: {}", summary_out_path.display());

    let baseline_snapshot = BaselineSnapshot {
        contract: contract_summary,
        curated_overall: curated.overall,
        synthetic_overall: synthetic.overall,
        headline,
    };
    if let Err(error) = write_json_file(&baseline_out_path, &baseline_snapshot) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    println!("Baseline snapshot file: {}", baseline_out_path.display());
}

#[cfg(test)]
mod tests {
    use super::{parse_row_index, sample_redactions, trend_label, LcgRng};
    use std::collections::BTreeMap;
    use unredact::types::redaction_types::{Rect, RedactionKind, RedactionOccurrence};

    fn synthetic_row(id: usize) -> RedactionOccurrence {
        let x0 = 10.0_f32 + id as f32;
        RedactionOccurrence {
            page_index: 0,
            bbox: Rect::new(x0, 20.0_f32, x0 + 8.0_f32, 30.0_f32),
            kind: RedactionKind::Unknown,
            score: 1.0_f32,
            meta: BTreeMap::new(),
            underlying_text: Vec::new(),
        }
    }

    #[test]
    fn parse_row_index_reads_anchor_row_suffix() {
        assert_eq!(parse_row_index("page1_row37"), Some(37));
        assert_eq!(parse_row_index("page9_row0"), Some(0));
        assert_eq!(parse_row_index("page9-row0"), None);
    }

    #[test]
    fn seeded_sampling_is_reproducible() {
        let source = (0..32).map(synthetic_row).collect::<Vec<_>>();
        let first = sample_redactions(12345_u64, source.as_slice(), 10);
        let second = sample_redactions(12345_u64, source.as_slice(), 10);
        let third = sample_redactions(98765_u64, source.as_slice(), 10);
        assert_eq!(first, second);
        assert_ne!(first, third);
    }

    #[test]
    fn lcg_shuffle_is_stable_for_same_seed() {
        let mut first = vec![1, 2, 3, 4, 5, 6, 7];
        let mut second = first.clone();
        let mut rng_a = LcgRng::new(424242_u64);
        let mut rng_b = LcgRng::new(424242_u64);
        rng_a.shuffle(first.as_mut_slice());
        rng_b.shuffle(second.as_mut_slice());
        assert_eq!(first, second);
    }

    #[test]
    fn trend_label_handles_signs() {
        assert_eq!(trend_label(0.0_f64), "flat");
        assert_eq!(trend_label(0.5_f64), "up");
        assert_eq!(trend_label(-0.5_f64), "down");
    }
}

use clap::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use unredact::benchmarks::types::redaction_detection_contract::{
    canonical_redaction_detection_contract, RedactionDetectionContract, RedactionDetectionDataset,
};
use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::GuessConfig;
use unredact::types::redaction_types::{Rect, RedactionReport};
use unredact::types::visualizer_config::VisualizerConfig;

const IOU_MATCH_THRESHOLD: f32 = 0.20_f32;
const TREND_EPSILON: f64 = 1e-9_f64;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContractSummary {
    contract_id: String,
    schema_version: usize,
    dataset_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct DatasetSummary {
    #[serde(rename = "dataset_name")]
    name: String,
    #[serde(rename = "input_pdf_path")]
    input_pdf: String,
    #[serde(rename = "ground_truth_redaction_report_path")]
    ground_truth_redactions: String,
    #[serde(rename = "predicted_redaction_count")]
    predicted_count: usize,
    #[serde(rename = "ground_truth_redaction_count")]
    ground_truth_count: usize,
    #[serde(rename = "matched_redaction_count")]
    matched_count: usize,
    #[serde(rename = "unmatched_predicted_redaction_count")]
    unmatched_predicted_count: usize,
    #[serde(rename = "unmatched_ground_truth_redaction_count")]
    unmatched_ground_truth_count: usize,
    #[serde(rename = "detection_precision")]
    precision: f64,
    #[serde(rename = "detection_recall")]
    recall: f64,
    #[serde(rename = "detection_f1")]
    f1: f64,
    matched_iou_median: Option<f64>,
    matched_iou_p90: Option<f64>,
    matched_center_error_median_pt: Option<f64>,
    matched_center_error_p90_pt: Option<f64>,
    matched_area_ratio_median: Option<f64>,
    matched_area_ratio_p90: Option<f64>,
    matched_kind_agreement_ratio: Option<f64>,
    #[serde(rename = "predicted_redaction_page_count")]
    predicted_page_count: usize,
    #[serde(rename = "ground_truth_redaction_page_count")]
    ground_truth_page_count: usize,
    #[serde(rename = "matched_redaction_page_count")]
    matched_page_count: usize,
    #[serde(rename = "page_detection_precision")]
    page_precision: f64,
    #[serde(rename = "page_detection_recall")]
    page_recall: f64,
    #[serde(rename = "page_count_absolute_error_sum")]
    page_count_error_abs_sum: u64,
    #[serde(rename = "page_count_mean_absolute_error")]
    page_count_error_mean_abs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OverallSummary {
    #[serde(rename = "predicted_redaction_count", alias = "predicted_count")]
    predicted_count: usize,
    #[serde(rename = "ground_truth_redaction_count", alias = "ground_truth_count")]
    ground_truth_count: usize,
    #[serde(rename = "matched_redaction_count", alias = "matched_count")]
    matched_count: usize,
    #[serde(default)]
    #[serde(
        rename = "unmatched_predicted_redaction_count",
        alias = "unmatched_predicted_count"
    )]
    unmatched_predicted_count: usize,
    #[serde(default)]
    #[serde(
        rename = "unmatched_ground_truth_redaction_count",
        alias = "unmatched_ground_truth_count"
    )]
    unmatched_ground_truth_count: usize,
    #[serde(rename = "detection_precision", alias = "precision")]
    precision: f64,
    #[serde(rename = "detection_recall", alias = "recall")]
    recall: f64,
    #[serde(rename = "detection_f1", alias = "f1")]
    f1: f64,
    matched_iou_median: Option<f64>,
    matched_iou_p90: Option<f64>,
    #[serde(default)]
    matched_center_error_median_pt: Option<f64>,
    #[serde(default)]
    matched_center_error_p90_pt: Option<f64>,
    #[serde(default)]
    matched_area_ratio_median: Option<f64>,
    #[serde(default)]
    matched_area_ratio_p90: Option<f64>,
    #[serde(default)]
    matched_kind_agreement_ratio: Option<f64>,
    #[serde(default)]
    #[serde(
        rename = "predicted_redaction_page_count",
        alias = "predicted_page_count"
    )]
    predicted_page_count: usize,
    #[serde(default)]
    #[serde(
        rename = "ground_truth_redaction_page_count",
        alias = "ground_truth_page_count"
    )]
    ground_truth_page_count: usize,
    #[serde(default)]
    #[serde(rename = "matched_redaction_page_count", alias = "matched_page_count")]
    matched_page_count: usize,
    #[serde(default)]
    #[serde(rename = "page_detection_precision", alias = "page_precision")]
    page_precision: f64,
    #[serde(default)]
    #[serde(rename = "page_detection_recall", alias = "page_recall")]
    page_recall: f64,
    #[serde(
        rename = "page_count_absolute_error_sum",
        alias = "page_count_error_abs_sum"
    )]
    page_count_error_abs_sum: u64,
    #[serde(
        rename = "page_count_mean_absolute_error",
        alias = "page_count_error_mean_abs"
    )]
    page_count_error_mean_abs: f64,
}

#[derive(Debug, Clone, Serialize)]
struct MetricDefinitions {
    precision: &'static str,
    recall: &'static str,
    f1: &'static str,
    matched_iou_median: &'static str,
    matched_iou_p90: &'static str,
    matched_center_error_median_pt: &'static str,
    matched_center_error_p90_pt: &'static str,
    matched_area_ratio_median: &'static str,
    matched_area_ratio_p90: &'static str,
    matched_kind_agreement_ratio: &'static str,
    page_precision: &'static str,
    page_recall: &'static str,
    page_count_error_mean_abs: &'static str,
    iou_match_threshold: &'static str,
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
struct RedactionAccuracyBenchmark {
    #[serde(rename = "benchmark_contract")]
    contract: ContractSummary,
    #[serde(rename = "metric_definitions")]
    definitions: MetricDefinitions,
    #[serde(rename = "dataset_results")]
    datasets: Vec<DatasetSummary>,
    #[serde(rename = "overall_detection_metrics")]
    overall: OverallSummary,
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
    #[serde(rename = "overall_detection_metrics", alias = "overall")]
    overall: OverallSummary,
}

#[derive(Debug, Clone, Parser)]
#[command(
    name = "redaction_accuracy_benchmark",
    about = "Measure redaction detector quality against curated ground truth reports."
)]
struct CliOptions {
    #[arg(
        long = "out",
        default_value = "benchmark/redaction_benchmark_report.json"
    )]
    out_path: PathBuf,
    #[arg(long = "baseline-out")]
    baseline_out_path: Option<PathBuf>,
    #[arg(long, default_value_t = 2_usize, value_parser = parse_positive_usize)]
    repeats: usize,
}

#[derive(Debug, Clone)]
struct DatasetEvalDetail {
    summary: DatasetSummary,
    matched_ious: Vec<f64>,
    matched_center_errors: Vec<f64>,
    matched_area_ratios: Vec<f64>,
    matched_kind_agree_count: usize,
    predicted_pages_count: usize,
    ground_truth_pages_count: usize,
    matched_pages_count: usize,
    page_error_samples: usize,
}

#[derive(Debug, Clone)]
struct RunSnapshot {
    hash: String,
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

fn benchmark_config() -> UnredactServiceConfig {
    UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig {
            visual_score: false,
            ..GuessConfig::default()
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    }
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
        .unwrap_or("redaction_benchmark_report");
    parent.join(format!("{stem}.baseline.json"))
}

fn default_summary_out_path(out_path: &Path) -> PathBuf {
    let parent = out_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = out_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("redaction_benchmark_report");
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

fn rect_iou(left: Rect, right: Rect) -> f32 {
    let x0 = left.x0.max(right.x0);
    let y0 = left.y0.max(right.y0);
    let x1 = left.x1.min(right.x1);
    let y1 = left.y1.min(right.y1);
    let intersection_w = (x1 - x0).max(0.0_f32);
    let intersection_h = (y1 - y0).max(0.0_f32);
    let intersection = intersection_w * intersection_h;
    if intersection <= 0.0_f32 {
        return 0.0_f32;
    }
    let left_area = left.width().max(0.0_f32) * left.height().max(0.0_f32);
    let right_area = right.width().max(0.0_f32) * right.height().max(0.0_f32);
    let union = (left_area + right_area - intersection).max(0.0_f32);
    if union <= 0.0_f32 {
        return 0.0_f32;
    }
    intersection / union
}

fn rect_center_distance_pt(left: Rect, right: Rect) -> f64 {
    let left_x = (left.x0 as f64 + left.x1 as f64) * 0.5_f64;
    let left_y = (left.y0 as f64 + left.y1 as f64) * 0.5_f64;
    let right_x = (right.x0 as f64 + right.x1 as f64) * 0.5_f64;
    let right_y = (right.y0 as f64 + right.y1 as f64) * 0.5_f64;
    let dx = left_x - right_x;
    let dy = left_y - right_y;
    (dx * dx + dy * dy).sqrt()
}

fn rect_area_ratio(left: Rect, right: Rect) -> Option<f64> {
    let left_area = left.area() as f64;
    let right_area = right.area() as f64;
    if left_area <= 0.0_f64 || right_area <= 0.0_f64 {
        return None;
    }
    Some(left_area / right_area)
}

fn percentile_sorted(values: &[f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let quantile = q.clamp(0.0_f64, 1.0_f64);
    let idx = ((values.len().saturating_sub(1) as f64) * quantile).round() as usize;
    values.get(idx).copied()
}

fn safe_ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return if numerator == 0 { 1.0_f64 } else { 0.0_f64 };
    }
    numerator as f64 / denominator as f64
}

fn evaluate_dataset(
    root: &Path,
    dataset: &RedactionDetectionDataset,
) -> Result<DatasetEvalDetail, String> {
    let input = Path::new(&dataset.input_pdf);
    if !input.exists() {
        return Err(format!("missing dataset input {}", input.display()));
    }
    let ground_truth_path = Path::new(&dataset.ground_truth_redactions);
    if !ground_truth_path.exists() {
        return Err(format!(
            "missing dataset ground truth {}",
            ground_truth_path.display()
        ));
    }

    let output_dir = root.join(&dataset.name);
    std::fs::create_dir_all(&output_dir)
        .map_err(|error| format!("failed to create {}: {error}", output_dir.display()))?;
    let outputs = run_from_paths(input, &output_dir, None, benchmark_config())?;
    let predicted = load_redaction_report(&outputs.redactions_path)?;
    let ground_truth = load_redaction_report(ground_truth_path)?;

    let mut candidate_pairs = Vec::<(usize, usize, f64)>::new();
    for (pred_idx, pred) in predicted.redactions.iter().enumerate() {
        for (gt_idx, gt) in ground_truth.redactions.iter().enumerate() {
            if pred.page_index != gt.page_index {
                continue;
            }
            let iou = rect_iou(pred.bbox, gt.bbox) as f64;
            if iou >= IOU_MATCH_THRESHOLD as f64 {
                candidate_pairs.push((pred_idx, gt_idx, iou));
            }
        }
    }
    candidate_pairs.sort_by(|left, right| {
        right
            .2
            .partial_cmp(&left.2)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut used_pred = BTreeSet::<usize>::new();
    let mut used_gt = BTreeSet::<usize>::new();
    let mut matched_ious = Vec::<f64>::new();
    let mut matched_center_errors = Vec::<f64>::new();
    let mut matched_area_ratios = Vec::<f64>::new();
    let mut matched_kind_agree_count = 0_usize;
    for (pred_idx, gt_idx, iou) in candidate_pairs {
        if used_pred.contains(&pred_idx) || used_gt.contains(&gt_idx) {
            continue;
        }
        used_pred.insert(pred_idx);
        used_gt.insert(gt_idx);
        matched_ious.push(iou);
        let pred = &predicted.redactions[pred_idx];
        let gt = &ground_truth.redactions[gt_idx];
        matched_center_errors.push(rect_center_distance_pt(pred.bbox, gt.bbox));
        if let Some(area_ratio) = rect_area_ratio(pred.bbox, gt.bbox) {
            matched_area_ratios.push(area_ratio);
        }
        if pred.kind == gt.kind {
            matched_kind_agree_count += 1;
        }
    }
    matched_ious
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    matched_center_errors
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    matched_area_ratios
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    let predicted_count = predicted.redactions.len();
    let ground_truth_count = ground_truth.redactions.len();
    let matched_count = matched_ious.len();
    let unmatched_predicted_count = predicted_count.saturating_sub(matched_count);
    let unmatched_ground_truth_count = ground_truth_count.saturating_sub(matched_count);
    let precision = safe_ratio(matched_count, predicted_count);
    let recall = safe_ratio(matched_count, ground_truth_count);
    let f1 = if precision + recall <= 0.0_f64 {
        0.0_f64
    } else {
        2.0_f64 * precision * recall / (precision + recall)
    };

    let mut page_indices = BTreeSet::<u32>::new();
    page_indices.extend(predicted.page_counts.keys().copied());
    page_indices.extend(ground_truth.page_counts.keys().copied());
    let predicted_pages = predicted
        .page_counts
        .iter()
        .filter(|(_key, value)| **value > 0_u32)
        .map(|(key, _value)| *key)
        .collect::<BTreeSet<_>>();
    let ground_truth_pages = ground_truth
        .page_counts
        .iter()
        .filter(|(_key, value)| **value > 0_u32)
        .map(|(key, _value)| *key)
        .collect::<BTreeSet<_>>();
    let matched_pages = predicted_pages
        .intersection(&ground_truth_pages)
        .copied()
        .collect::<BTreeSet<_>>();
    let mut page_error_abs_sum = 0_u64;
    for page_index in &page_indices {
        let pred = predicted
            .page_counts
            .get(page_index)
            .copied()
            .unwrap_or(0_u32);
        let gt = ground_truth
            .page_counts
            .get(page_index)
            .copied()
            .unwrap_or(0_u32);
        page_error_abs_sum += pred.abs_diff(gt) as u64;
    }
    let page_count_error_mean_abs = if page_indices.is_empty() {
        0.0_f64
    } else {
        page_error_abs_sum as f64 / page_indices.len() as f64
    };

    Ok(DatasetEvalDetail {
        summary: DatasetSummary {
            name: dataset.name.clone(),
            input_pdf: dataset.input_pdf.clone(),
            ground_truth_redactions: dataset.ground_truth_redactions.clone(),
            predicted_count,
            ground_truth_count,
            matched_count,
            unmatched_predicted_count,
            unmatched_ground_truth_count,
            precision,
            recall,
            f1,
            matched_iou_median: percentile_sorted(&matched_ious, 0.5_f64),
            matched_iou_p90: percentile_sorted(&matched_ious, 0.9_f64),
            matched_center_error_median_pt: percentile_sorted(&matched_center_errors, 0.5_f64),
            matched_center_error_p90_pt: percentile_sorted(&matched_center_errors, 0.9_f64),
            matched_area_ratio_median: percentile_sorted(&matched_area_ratios, 0.5_f64),
            matched_area_ratio_p90: percentile_sorted(&matched_area_ratios, 0.9_f64),
            matched_kind_agreement_ratio: if matched_count == 0 {
                None
            } else {
                Some(matched_kind_agree_count as f64 / matched_count as f64)
            },
            predicted_page_count: predicted_pages.len(),
            ground_truth_page_count: ground_truth_pages.len(),
            matched_page_count: matched_pages.len(),
            page_precision: safe_ratio(matched_pages.len(), predicted_pages.len()),
            page_recall: safe_ratio(matched_pages.len(), ground_truth_pages.len()),
            page_count_error_abs_sum: page_error_abs_sum,
            page_count_error_mean_abs,
        },
        matched_ious,
        matched_center_errors,
        matched_area_ratios,
        matched_kind_agree_count,
        predicted_pages_count: predicted_pages.len(),
        ground_truth_pages_count: ground_truth_pages.len(),
        matched_pages_count: matched_pages.len(),
        page_error_samples: page_indices.len(),
    })
}

fn summarize_overall(details: &[DatasetEvalDetail]) -> OverallSummary {
    let predicted_count = details
        .iter()
        .map(|detail| detail.summary.predicted_count)
        .sum::<usize>();
    let ground_truth_count = details
        .iter()
        .map(|detail| detail.summary.ground_truth_count)
        .sum::<usize>();
    let matched_count = details
        .iter()
        .map(|detail| detail.summary.matched_count)
        .sum::<usize>();
    let unmatched_predicted_count = details
        .iter()
        .map(|detail| detail.summary.unmatched_predicted_count)
        .sum::<usize>();
    let unmatched_ground_truth_count = details
        .iter()
        .map(|detail| detail.summary.unmatched_ground_truth_count)
        .sum::<usize>();
    let precision = safe_ratio(matched_count, predicted_count);
    let recall = safe_ratio(matched_count, ground_truth_count);
    let f1 = if precision + recall <= 0.0_f64 {
        0.0_f64
    } else {
        2.0_f64 * precision * recall / (precision + recall)
    };
    let mut all_ious = details
        .iter()
        .flat_map(|detail| detail.matched_ious.iter().copied())
        .collect::<Vec<_>>();
    all_ious.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mut all_center_errors = details
        .iter()
        .flat_map(|detail| detail.matched_center_errors.iter().copied())
        .collect::<Vec<_>>();
    all_center_errors
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mut all_area_ratios = details
        .iter()
        .flat_map(|detail| detail.matched_area_ratios.iter().copied())
        .collect::<Vec<_>>();
    all_area_ratios
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let matched_kind_agree_count = details
        .iter()
        .map(|detail| detail.matched_kind_agree_count)
        .sum::<usize>();
    let predicted_page_count = details
        .iter()
        .map(|detail| detail.predicted_pages_count)
        .sum::<usize>();
    let ground_truth_page_count = details
        .iter()
        .map(|detail| detail.ground_truth_pages_count)
        .sum::<usize>();
    let matched_page_count = details
        .iter()
        .map(|detail| detail.matched_pages_count)
        .sum::<usize>();
    let page_count_error_abs_sum = details
        .iter()
        .map(|detail| detail.summary.page_count_error_abs_sum)
        .sum::<u64>();
    let page_error_samples = details
        .iter()
        .map(|detail| detail.page_error_samples)
        .sum::<usize>();
    let page_count_error_mean_abs = if page_error_samples == 0 {
        0.0_f64
    } else {
        page_count_error_abs_sum as f64 / page_error_samples as f64
    };

    OverallSummary {
        predicted_count,
        ground_truth_count,
        matched_count,
        unmatched_predicted_count,
        unmatched_ground_truth_count,
        precision,
        recall,
        f1,
        matched_iou_median: percentile_sorted(&all_ious, 0.5_f64),
        matched_iou_p90: percentile_sorted(&all_ious, 0.9_f64),
        matched_center_error_median_pt: percentile_sorted(&all_center_errors, 0.5_f64),
        matched_center_error_p90_pt: percentile_sorted(&all_center_errors, 0.9_f64),
        matched_area_ratio_median: percentile_sorted(&all_area_ratios, 0.5_f64),
        matched_area_ratio_p90: percentile_sorted(&all_area_ratios, 0.9_f64),
        matched_kind_agreement_ratio: if matched_count == 0 {
            None
        } else {
            Some(matched_kind_agree_count as f64 / matched_count as f64)
        },
        predicted_page_count,
        ground_truth_page_count,
        matched_page_count,
        page_precision: safe_ratio(matched_page_count, predicted_page_count),
        page_recall: safe_ratio(matched_page_count, ground_truth_page_count),
        page_count_error_abs_sum,
        page_count_error_mean_abs,
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
        precision: "matched_predictions / total_predictions. Higher is better.",
        recall: "matched_ground_truth / total_ground_truth. Higher is better.",
        f1: "Harmonic mean of precision and recall. Higher is better.",
        matched_iou_median: "Median IoU of matched prediction-ground truth pairs. Higher is better.",
        matched_iou_p90: "90th percentile IoU of matched pairs. Higher is better.",
        matched_center_error_median_pt:
            "Median center-point distance (pt) between matched prediction-ground truth boxes. Lower is better.",
        matched_center_error_p90_pt:
            "90th percentile center-point distance (pt) for matched boxes. Lower is better.",
        matched_area_ratio_median:
            "Median area ratio (predicted/ground-truth) on matched boxes. Closer to 1 is better.",
        matched_area_ratio_p90:
            "90th percentile area ratio (predicted/ground-truth) on matched boxes. Closer to 1 is better.",
        matched_kind_agreement_ratio:
            "Share of matched boxes where predicted redaction kind equals ground-truth kind. Higher is better.",
        page_precision:
            "Pages with any predicted redaction that also have ground-truth redactions divided by predicted redaction pages. Higher is better.",
        page_recall:
            "Ground-truth redaction pages that also have predicted redactions divided by ground-truth redaction pages. Higher is better.",
        page_count_error_mean_abs:
            "Mean absolute page-level redaction count error across all evaluated pages. Lower is better.",
        iou_match_threshold:
            "IoU threshold used when matching predicted and ground-truth redaction boxes.",
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
    current: &OverallSummary,
    baseline: &OverallSummary,
    baseline_path: &Path,
) -> BaselineCompare {
    let mut metrics = vec![
        metric_delta(
            "overall_detection_metrics.detection_precision",
            "higher_is_better",
            current.precision,
            baseline.precision,
        ),
        metric_delta(
            "overall_detection_metrics.detection_recall",
            "higher_is_better",
            current.recall,
            baseline.recall,
        ),
        metric_delta(
            "overall_detection_metrics.detection_f1",
            "higher_is_better",
            current.f1,
            baseline.f1,
        ),
        metric_delta(
            "overall_detection_metrics.page_count_mean_absolute_error",
            "lower_is_better",
            current.page_count_error_mean_abs,
            baseline.page_count_error_mean_abs,
        ),
        metric_delta(
            "overall_detection_metrics.page_detection_precision",
            "higher_is_better",
            current.page_precision,
            baseline.page_precision,
        ),
        metric_delta(
            "overall_detection_metrics.page_detection_recall",
            "higher_is_better",
            current.page_recall,
            baseline.page_recall,
        ),
    ];
    if let (Some(current_iou), Some(baseline_iou)) =
        (current.matched_iou_median, baseline.matched_iou_median)
    {
        metrics.push(metric_delta(
            "overall_detection_metrics.matched_iou_median",
            "higher_is_better",
            current_iou,
            baseline_iou,
        ));
    }
    if let (Some(current_iou), Some(baseline_iou)) =
        (current.matched_iou_p90, baseline.matched_iou_p90)
    {
        metrics.push(metric_delta(
            "overall_detection_metrics.matched_iou_p90",
            "higher_is_better",
            current_iou,
            baseline_iou,
        ));
    }
    if let (Some(current_value), Some(baseline_value)) = (
        current.matched_center_error_median_pt,
        baseline.matched_center_error_median_pt,
    ) {
        metrics.push(metric_delta(
            "overall_detection_metrics.matched_center_error_median_pt",
            "lower_is_better",
            current_value,
            baseline_value,
        ));
    }
    if let (Some(current_value), Some(baseline_value)) = (
        current.matched_center_error_p90_pt,
        baseline.matched_center_error_p90_pt,
    ) {
        metrics.push(metric_delta(
            "overall_detection_metrics.matched_center_error_p90_pt",
            "lower_is_better",
            current_value,
            baseline_value,
        ));
    }
    if let (Some(current_value), Some(baseline_value)) = (
        current.matched_kind_agreement_ratio,
        baseline.matched_kind_agreement_ratio,
    ) {
        metrics.push(metric_delta(
            "overall_detection_metrics.matched_kind_agreement_ratio",
            "higher_is_better",
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
    let run_hashes = run_snapshots
        .iter()
        .map(|snapshot| snapshot.hash.clone())
        .collect::<Vec<_>>();
    ConsistencySummary {
        repeats: run_snapshots.len(),
        all_hashes_identical: matched == run_snapshots.len(),
        hash_match_ratio: matched as f64 / run_snapshots.len() as f64,
        run_hashes,
    }
}

fn format_optional(value: Option<f64>) -> String {
    value
        .map(|item| format!("{item:.4}"))
        .unwrap_or_else(|| "-".to_owned())
}

fn render_console_summary(
    payload: &RedactionAccuracyBenchmark,
    json_out_path: &Path,
    summary_out_path: &Path,
    baseline_out_path: &Path,
) -> String {
    let mut out = String::new();
    let overall = &payload.overall;
    writeln!(&mut out, "Redaction Benchmark Report").unwrap();
    writeln!(&mut out, "==========================").unwrap();
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

    writeln!(&mut out, "Overall Detection Metrics").unwrap();
    writeln!(&mut out, "  {:<30} {:>10.4}", "Detection F1", overall.f1).unwrap();
    writeln!(
        &mut out,
        "  {:<30} {:>10.4}",
        "Detection precision", overall.precision
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<30} {:>10.4}",
        "Detection recall", overall.recall
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<30} {:>10.4}",
        "Page detection precision", overall.page_precision
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<30} {:>10.4}",
        "Page detection recall", overall.page_recall
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<30} {:>10}",
        "Matched redactions", overall.matched_count
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Dataset Results").unwrap();
    writeln!(
        &mut out,
        "  {:<16} {:>6} {:>6} {:>8} {:>8} {:>8} {:>10} {:>10}",
        "Dataset", "Pred", "GT", "Matched", "F1", "Recall", "IoU med", "Page rec",
    )
    .unwrap();
    for dataset in &payload.datasets {
        writeln!(
            &mut out,
            "  {:<16} {:>6} {:>6} {:>8} {:>8.4} {:>8.4} {:>10} {:>10.4}",
            dataset.name,
            dataset.predicted_count,
            dataset.ground_truth_count,
            dataset.matched_count,
            dataset.f1,
            dataset.recall,
            format_optional(dataset.matched_iou_median),
            dataset.page_recall,
        )
        .unwrap();
    }
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Repeat Consistency").unwrap();
    writeln!(
        &mut out,
        "  {:<30} {:>10}",
        "Repeated runs", payload.consistency.repeats
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<30} {:>10}",
        "All hashes identical", payload.consistency.all_hashes_identical
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<30} {:>10.4}",
        "First-run match ratio", payload.consistency.hash_match_ratio
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Baseline Comparison").unwrap();
    if let Some(compare) = &payload.baseline_compare {
        writeln!(&mut out, "  Baseline report: {}", compare.baseline_path).unwrap();
        writeln!(
            &mut out,
            "  {:<54} {:>10} {:>10} {:>10} {:>8}",
            "Metric", "Baseline", "Current", "Delta", "Trend"
        )
        .unwrap();
        for metric in &compare.metrics {
            writeln!(
                &mut out,
                "  {:<54} {:>10.4} {:>10.4} {:>10.4} {:>8}",
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

fn render_markdown_summary(
    payload: &RedactionAccuracyBenchmark,
    json_out_path: &Path,
    summary_out_path: &Path,
    baseline_out_path: &Path,
) -> String {
    let mut out = String::new();
    let overall = &payload.overall;
    writeln!(&mut out, "# Redaction Benchmark Report").unwrap();
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

    writeln!(&mut out, "## Overall Detection Metrics").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "| Metric | Value |").unwrap();
    writeln!(&mut out, "| --- | ---: |").unwrap();
    writeln!(&mut out, "| Detection F1 | {:.4} |", overall.f1).unwrap();
    writeln!(
        &mut out,
        "| Detection precision | {:.4} |",
        overall.precision
    )
    .unwrap();
    writeln!(&mut out, "| Detection recall | {:.4} |", overall.recall).unwrap();
    writeln!(
        &mut out,
        "| Page detection precision | {:.4} |",
        overall.page_precision
    )
    .unwrap();
    writeln!(
        &mut out,
        "| Page detection recall | {:.4} |",
        overall.page_recall
    )
    .unwrap();
    writeln!(
        &mut out,
        "| Matched redactions | {} |",
        overall.matched_count
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "## Dataset Results").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(
        &mut out,
        "| Dataset | Predicted | Ground truth | Matched | F1 | Recall | Median IoU | Median center error (pt) | Page recall |"
    )
    .unwrap();
    writeln!(
        &mut out,
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
    )
    .unwrap();
    for dataset in &payload.datasets {
        writeln!(
            &mut out,
            "| {} | {} | {} | {} | {:.4} | {:.4} | {} | {} | {:.4} |",
            dataset.name,
            dataset.predicted_count,
            dataset.ground_truth_count,
            dataset.matched_count,
            dataset.f1,
            dataset.recall,
            format_optional(dataset.matched_iou_median),
            format_optional(dataset.matched_center_error_median_pt),
            dataset.page_recall
        )
        .unwrap();
    }
    writeln!(&mut out).unwrap();

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

fn run_once(
    root: &Path,
    contract: &RedactionDetectionContract,
) -> Result<(Vec<DatasetSummary>, OverallSummary), String> {
    let mut details = Vec::<DatasetEvalDetail>::new();
    for dataset in &contract.datasets {
        details.push(evaluate_dataset(root, dataset)?);
    }
    let datasets = details
        .iter()
        .map(|detail| detail.summary.clone())
        .collect::<Vec<_>>();
    let overall = summarize_overall(details.as_slice());
    Ok((datasets, overall))
}

fn main() {
    let options = CliOptions::parse();
    let contract = match canonical_redaction_detection_contract() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("failed to load redaction detection contract: {error}");
            std::process::exit(1);
        }
    };
    let contract_summary = ContractSummary {
        contract_id: contract.contract_id.clone(),
        schema_version: contract.schema_version,
        dataset_count: contract.datasets.len(),
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

    let mut selected_datasets = None::<Vec<DatasetSummary>>;
    let mut selected_overall = None::<OverallSummary>;
    let mut run_snapshots = Vec::<RunSnapshot>::new();
    for repeat in 0..options.repeats {
        let benchmark_root = std::env::temp_dir().join(format!(
            "unredact_redaction_accuracy_benchmark_{}_{}",
            std::process::id(),
            repeat
        ));
        if benchmark_root.exists() {
            let remove_result = std::fs::remove_dir_all(&benchmark_root);
            if let Err(error) = remove_result {
                eprintln!("failed to clean benchmark temp dir: {error}");
                std::process::exit(1);
            }
        }
        if let Err(error) = std::fs::create_dir_all(&benchmark_root) {
            eprintln!("failed to create benchmark temp dir: {error}");
            std::process::exit(1);
        }

        let (datasets, overall) = match run_once(&benchmark_root, contract) {
            Ok(value) => value,
            Err(error) => {
                eprintln!(
                    "redaction benchmark failed on repeat {}: {error}",
                    repeat + 1
                );
                std::process::exit(1);
            }
        };
        let run_hash = match hash_json(&(datasets.clone(), overall.clone())) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("failed to hash benchmark payload: {error}");
                std::process::exit(1);
            }
        };
        run_snapshots.push(RunSnapshot { hash: run_hash });
        if selected_datasets.is_none() {
            selected_datasets = Some(datasets);
            selected_overall = Some(overall);
        }
    }

    let datasets = match selected_datasets {
        Some(value) => value,
        None => {
            eprintln!("no redaction benchmark datasets were produced");
            std::process::exit(1);
        }
    };
    let overall = match selected_overall {
        Some(value) => value,
        None => {
            eprintln!("no redaction benchmark overall summary was produced");
            std::process::exit(1);
        }
    };
    let baseline_compare = existing_baseline
        .as_ref()
        .map(|baseline| build_baseline_compare(&overall, &baseline.overall, &baseline_out_path));
    let payload = RedactionAccuracyBenchmark {
        contract: contract_summary.clone(),
        definitions: metric_definitions(),
        datasets,
        overall: overall.clone(),
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
        overall,
    };
    if let Err(error) = write_json_file(&baseline_out_path, &baseline_snapshot) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    println!("Baseline snapshot file: {}", baseline_out_path.display());
}

#[cfg(test)]
mod tests {
    use super::{rect_iou, safe_ratio, trend_label};
    use unredact::types::redaction_types::Rect;

    #[test]
    fn rect_iou_returns_expected_value_for_overlap() {
        let left = Rect::new(0.0_f32, 0.0_f32, 10.0_f32, 10.0_f32);
        let right = Rect::new(5.0_f32, 0.0_f32, 15.0_f32, 10.0_f32);
        let iou = rect_iou(left, right);
        assert!((iou - 0.33333334_f32).abs() <= 0.0001_f32);
    }

    #[test]
    fn safe_ratio_handles_empty_denominator() {
        assert!((safe_ratio(0, 0) - 1.0_f64).abs() <= 0.0000001_f64);
        assert!((safe_ratio(1, 0) - 0.0_f64).abs() <= 0.0000001_f64);
    }

    #[test]
    fn trend_label_handles_small_deltas() {
        assert_eq!(trend_label(0.0_f64), "flat");
        assert_eq!(trend_label(1.0_f64), "up");
        assert_eq!(trend_label(-1.0_f64), "down");
    }
}

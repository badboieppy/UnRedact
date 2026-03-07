use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

const TREND_EPSILON: f64 = 1e-9_f64;
const DEFAULT_OUT_PATH: &str = "benchmark/combined_benchmark_report.json";
const REDACTION_STAGE_BIN: &str = "redaction_accuracy_benchmark";
const ANCHOR_STAGE_BIN: &str = "anchor_accuracy_benchmark";
const REDACTION_STAGE_FILE: &str = "redaction_benchmark_report.json";
const ANCHOR_STAGE_FILE: &str = "anchor_benchmark_report.json";
const REDACTION_WEIGHT: f64 = 0.60_f64;
const ANCHOR_WEIGHT: f64 = 0.40_f64;

#[derive(Debug, Clone, Deserialize)]
struct RedactionBenchmarkInput {
    #[serde(rename = "overall_detection_metrics", alias = "overall")]
    overall: RedactionOverallInput,
}

#[derive(Debug, Clone, Deserialize)]
struct RedactionOverallInput {
    #[serde(rename = "detection_f1", alias = "f1")]
    f1: f64,
    #[serde(rename = "detection_precision", alias = "precision")]
    precision: f64,
    #[serde(rename = "detection_recall", alias = "recall")]
    recall: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct AnchorBenchmarkInput {
    #[serde(rename = "curated_headline_score", alias = "headline")]
    headline: AnchorHeadlineInput,
    #[serde(rename = "curated_anchor_results", alias = "curated")]
    curated: AnchorCuratedInput,
    #[serde(rename = "synthetic_anchor_results", alias = "synthetic")]
    synthetic: AnchorSyntheticInput,
}

#[derive(Debug, Clone, Deserialize)]
struct AnchorHeadlineInput {
    #[serde(rename = "score_source", alias = "source")]
    source: String,
    #[serde(rename = "score", alias = "value")]
    value: f64,
    #[serde(rename = "score_formula", alias = "formula")]
    formula: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AnchorCuratedInput {
    #[serde(rename = "aggregate_metrics", alias = "overall")]
    overall: AnchorOverallInput,
}

#[derive(Debug, Clone, Deserialize)]
struct AnchorSyntheticInput {
    #[serde(rename = "aggregate_metrics", alias = "overall")]
    overall: AnchorOverallInput,
}

#[derive(Debug, Clone, Deserialize)]
struct AnchorOverallInput {
    #[serde(rename = "anchor_quality_score", alias = "anchor_score")]
    anchor_score: f64,
    #[serde(rename = "row_selection_rate", alias = "row_selected_ratio")]
    row_selected_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
struct CombinedDefinitions {
    #[serde(rename = "redaction_stage_score_definition")]
    redaction_stage_score: &'static str,
    #[serde(rename = "anchor_stage_score_definition")]
    anchor_stage_score: &'static str,
    #[serde(rename = "combined_score_definition")]
    total_score: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ArtifactPaths {
    #[serde(rename = "redaction_benchmark_report_path")]
    redaction_json: String,
    #[serde(rename = "anchor_benchmark_report_path")]
    anchor_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StageSummary {
    #[serde(rename = "stage_name", alias = "stage")]
    stage: String,
    #[serde(rename = "score_source_metric", alias = "source_metric")]
    source_metric: String,
    #[serde(rename = "stage_score", alias = "score")]
    score: f64,
    #[serde(rename = "stage_weight", alias = "weight")]
    weight: f64,
    #[serde(rename = "weighted_stage_score", alias = "weighted_score")]
    weighted_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CombinedSummary {
    #[serde(rename = "redaction_stage_weight", alias = "redaction_weight")]
    redaction_weight: f64,
    #[serde(rename = "anchor_stage_weight", alias = "anchor_weight")]
    anchor_weight: f64,
    #[serde(rename = "combined_weighted_score", alias = "total_score")]
    total_score: f64,
    #[serde(rename = "stage_scores", alias = "stages")]
    stages: Vec<StageSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticSummary {
    #[serde(rename = "redaction_detection_precision")]
    redaction_precision: f64,
    #[serde(rename = "redaction_detection_recall")]
    redaction_recall: f64,
    #[serde(rename = "anchor_headline_score_source")]
    anchor_headline_source: String,
    #[serde(rename = "anchor_headline_score_formula")]
    anchor_headline_formula: String,
    #[serde(rename = "curated_anchor_quality_score")]
    anchor_curated_score: f64,
    #[serde(rename = "synthetic_anchor_quality_score")]
    anchor_synthetic_score: f64,
    #[serde(rename = "curated_row_selection_rate")]
    anchor_curated_row_selected_ratio: f64,
    #[serde(rename = "synthetic_row_selection_rate")]
    anchor_synthetic_row_selected_ratio: f64,
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
struct CombinedAccuracyBenchmark {
    #[serde(rename = "stage_report_paths")]
    artifacts: ArtifactPaths,
    #[serde(rename = "score_definitions")]
    definitions: CombinedDefinitions,
    #[serde(rename = "combined_score_summary")]
    summary: CombinedSummary,
    #[serde(rename = "stage_diagnostics")]
    diagnostics: DiagnosticSummary,
    #[serde(rename = "baseline_comparison")]
    baseline_compare: Option<BaselineCompare>,
    #[serde(rename = "created_new_baseline")]
    baseline_bootstrapped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BaselineSnapshot {
    #[serde(rename = "combined_score_summary", alias = "summary")]
    summary: CombinedSummary,
}

#[derive(Debug, Clone, Parser)]
#[command(
    name = "combined_accuracy_benchmark",
    about = "Run redaction and anchor benchmarks, then publish a combined summary."
)]
struct CliOptions {
    #[arg(long = "out", default_value = DEFAULT_OUT_PATH)]
    out_path: PathBuf,
    #[arg(long, default_value_t = 2_usize, value_parser = parse_positive_usize)]
    repeats: usize,
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
        .unwrap_or("combined_benchmark_report");
    parent.join(format!("{stem}.baseline.json"))
}

fn default_summary_out_path(out_path: &Path) -> PathBuf {
    let parent = out_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = out_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("combined_benchmark_report");
    parent.join(format!("{stem}.summary.md"))
}

fn artifact_root(out_path: &Path) -> &Path {
    out_path.parent().unwrap_or_else(|| Path::new(""))
}

fn redaction_stage_out_path(out_path: &Path) -> PathBuf {
    artifact_root(out_path).join(REDACTION_STAGE_FILE)
}

fn anchor_stage_out_path(out_path: &Path) -> PathBuf {
    artifact_root(out_path).join(ANCHOR_STAGE_FILE)
}

fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_slice::<T>(&bytes)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn cargo_binary() -> String {
    std::env::var("CARGO").unwrap_or_else(|_error| "cargo".to_owned())
}

fn run_stage_benchmark(bin_name: &str, out_path: &Path, repeats: usize) -> Result<(), String> {
    ensure_parent_dir(out_path)?;
    let mut cmd = Command::new(cargo_binary());
    cmd.arg("run");
    if !cfg!(debug_assertions) {
        cmd.arg("--release");
    }
    cmd.arg("--quiet");
    cmd.arg("--bin");
    cmd.arg(bin_name);
    cmd.arg("--");
    cmd.arg("--out");
    cmd.arg(out_path);
    cmd.arg("--repeats");
    cmd.arg(repeats.to_string());
    let status = cmd
        .status()
        .map_err(|error| format!("failed to launch {bin_name}: {error}"))?;
    if !status.success() {
        return Err(format!("{bin_name} exited with status {status}"));
    }
    Ok(())
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
    current: &CombinedSummary,
    baseline: &CombinedSummary,
    baseline_path: &Path,
) -> BaselineCompare {
    let current_redaction = current
        .stages
        .iter()
        .find(|stage| stage.stage == "redaction")
        .map(|stage| stage.score)
        .unwrap_or(0.0_f64);
    let baseline_redaction = baseline
        .stages
        .iter()
        .find(|stage| stage.stage == "redaction")
        .map(|stage| stage.score)
        .unwrap_or(0.0_f64);
    let current_anchor = current
        .stages
        .iter()
        .find(|stage| stage.stage == "anchor_curated")
        .map(|stage| stage.score)
        .unwrap_or(0.0_f64);
    let baseline_anchor = baseline
        .stages
        .iter()
        .find(|stage| stage.stage == "anchor_curated")
        .map(|stage| stage.score)
        .unwrap_or(0.0_f64);
    BaselineCompare {
        baseline_path: baseline_path.display().to_string(),
        metrics: vec![
            metric_delta(
                "combined_score_summary.combined_weighted_score",
                "higher_is_better",
                current.total_score,
                baseline.total_score,
            ),
            metric_delta(
                "combined_score_summary.stage_scores.redaction.stage_score",
                "higher_is_better",
                current_redaction,
                baseline_redaction,
            ),
            metric_delta(
                "combined_score_summary.stage_scores.anchor_curated.stage_score",
                "higher_is_better",
                current_anchor,
                baseline_anchor,
            ),
        ],
    }
}

fn render_console_summary(
    payload: &CombinedAccuracyBenchmark,
    json_out_path: &Path,
    summary_out_path: &Path,
    baseline_out_path: &Path,
) -> String {
    let mut out = String::new();
    writeln!(&mut out, "Combined Benchmark Report").unwrap();
    writeln!(&mut out, "=========================").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "Files").unwrap();
    writeln!(
        &mut out,
        "  Combined JSON report: {}",
        json_out_path.display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  Combined markdown summary: {}",
        summary_out_path.display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  Combined baseline snapshot: {}",
        baseline_out_path.display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  Redaction stage JSON report: {}",
        payload.artifacts.redaction_json
    )
    .unwrap();
    writeln!(
        &mut out,
        "  Redaction stage markdown summary: {}",
        default_summary_out_path(Path::new(payload.artifacts.redaction_json.as_str())).display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  Anchor stage JSON report: {}",
        payload.artifacts.anchor_json
    )
    .unwrap();
    writeln!(
        &mut out,
        "  Anchor stage markdown summary: {}",
        default_summary_out_path(Path::new(payload.artifacts.anchor_json.as_str())).display()
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Combined Score Summary").unwrap();
    writeln!(
        &mut out,
        "  {:<30} {:>10.4}",
        "Combined weighted score", payload.summary.total_score
    )
    .unwrap();
    for stage in &payload.summary.stages {
        writeln!(
            &mut out,
            "  {:<30} {:>10.4}",
            format!("{} stage score", stage.stage),
            stage.score
        )
        .unwrap();
    }
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Stage Score Breakdown").unwrap();
    writeln!(
        &mut out,
        "  {:<18} {:<48} {:>10} {:>10} {:>10}",
        "Stage", "Source metric", "Weight", "Score", "Weighted"
    )
    .unwrap();
    for stage in &payload.summary.stages {
        writeln!(
            &mut out,
            "  {:<18} {:<48} {:>10.2} {:>10.4} {:>10.4}",
            stage.stage, stage.source_metric, stage.weight, stage.score, stage.weighted_score
        )
        .unwrap();
    }
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Stage Diagnostics").unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10.4}",
        "Redaction detection precision", payload.diagnostics.redaction_precision
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10.4}",
        "Redaction detection recall", payload.diagnostics.redaction_recall
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10.4}",
        "Curated anchor quality score", payload.diagnostics.anchor_curated_score
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10.4}",
        "Synthetic anchor quality score", payload.diagnostics.anchor_synthetic_score
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10.4}",
        "Curated row selection rate", payload.diagnostics.anchor_curated_row_selected_ratio
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {:<34} {:>10.4}",
        "Synthetic row selection rate", payload.diagnostics.anchor_synthetic_row_selected_ratio
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Baseline Comparison").unwrap();
    if let Some(compare) = &payload.baseline_compare {
        writeln!(&mut out, "  Baseline report: {}", compare.baseline_path).unwrap();
        writeln!(
            &mut out,
            "  {:<60} {:>10} {:>10} {:>10} {:>8}",
            "Metric", "Baseline", "Current", "Delta", "Trend"
        )
        .unwrap();
        for metric in &compare.metrics {
            writeln!(
                &mut out,
                "  {:<60} {:>10.4} {:>10.4} {:>10.4} {:>8}",
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
    payload: &CombinedAccuracyBenchmark,
    json_out_path: &Path,
    summary_out_path: &Path,
    baseline_out_path: &Path,
) -> String {
    let mut out = String::new();
    writeln!(&mut out, "# Combined Benchmark Report").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "## Files").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(
        &mut out,
        "- Combined JSON report: `{}`",
        json_out_path.display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Combined markdown summary: `{}`",
        summary_out_path.display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Combined baseline snapshot: `{}`",
        baseline_out_path.display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Redaction stage JSON report: `{}`",
        payload.artifacts.redaction_json
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Redaction stage markdown summary: `{}`",
        default_summary_out_path(Path::new(payload.artifacts.redaction_json.as_str())).display()
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Anchor stage JSON report: `{}`",
        payload.artifacts.anchor_json
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Anchor stage markdown summary: `{}`",
        default_summary_out_path(Path::new(payload.artifacts.anchor_json.as_str())).display()
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "## Combined Score Summary").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "| Metric | Value |").unwrap();
    writeln!(&mut out, "| --- | ---: |").unwrap();
    writeln!(
        &mut out,
        "| Combined weighted score | {:.4} |",
        payload.summary.total_score
    )
    .unwrap();
    for stage in &payload.summary.stages {
        writeln!(
            &mut out,
            "| {} stage score | {:.4} |",
            stage.stage, stage.score
        )
        .unwrap();
    }
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "## Stage Score Breakdown").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(
        &mut out,
        "| Stage | Source metric | Weight | Score | Weighted score |"
    )
    .unwrap();
    writeln!(&mut out, "| --- | --- | ---: | ---: | ---: |").unwrap();
    for stage in &payload.summary.stages {
        writeln!(
            &mut out,
            "| {} | {} | {:.2} | {:.4} | {:.4} |",
            stage.stage, stage.source_metric, stage.weight, stage.score, stage.weighted_score
        )
        .unwrap();
    }
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "## Stage Diagnostics").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "| Metric | Value |").unwrap();
    writeln!(&mut out, "| --- | ---: |").unwrap();
    writeln!(
        &mut out,
        "| Redaction detection precision | {:.4} |",
        payload.diagnostics.redaction_precision
    )
    .unwrap();
    writeln!(
        &mut out,
        "| Redaction detection recall | {:.4} |",
        payload.diagnostics.redaction_recall
    )
    .unwrap();
    writeln!(
        &mut out,
        "| Curated anchor quality score | {:.4} |",
        payload.diagnostics.anchor_curated_score
    )
    .unwrap();
    writeln!(
        &mut out,
        "| Synthetic anchor quality score | {:.4} |",
        payload.diagnostics.anchor_synthetic_score
    )
    .unwrap();
    writeln!(
        &mut out,
        "| Curated row selection rate | {:.4} |",
        payload.diagnostics.anchor_curated_row_selected_ratio
    )
    .unwrap();
    writeln!(
        &mut out,
        "| Synthetic row selection rate | {:.4} |",
        payload.diagnostics.anchor_synthetic_row_selected_ratio
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

fn main() {
    let options = CliOptions::parse();
    let redaction_out_path = redaction_stage_out_path(options.out_path.as_path());
    let anchor_out_path = anchor_stage_out_path(options.out_path.as_path());
    let summary_out_path = default_summary_out_path(options.out_path.as_path());

    println!();
    println!("==============================");
    println!("Running redaction benchmark stage");
    println!("==============================");
    if let Err(error) =
        run_stage_benchmark(REDACTION_STAGE_BIN, &redaction_out_path, options.repeats)
    {
        eprintln!("{error}");
        std::process::exit(1);
    }
    println!();
    println!("============================");
    println!("Running anchor benchmark stage");
    println!("============================");
    if let Err(error) = run_stage_benchmark(ANCHOR_STAGE_BIN, &anchor_out_path, options.repeats) {
        eprintln!("{error}");
        std::process::exit(1);
    }

    let redaction = match load_json::<RedactionBenchmarkInput>(&redaction_out_path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let anchor = match load_json::<AnchorBenchmarkInput>(&anchor_out_path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    let summary = CombinedSummary {
        redaction_weight: REDACTION_WEIGHT,
        anchor_weight: ANCHOR_WEIGHT,
        total_score: redaction.overall.f1 * REDACTION_WEIGHT
            + anchor.headline.value * ANCHOR_WEIGHT,
        stages: vec![
            StageSummary {
                stage: "redaction".to_owned(),
                source_metric: "overall_detection_metrics.detection_f1".to_owned(),
                score: redaction.overall.f1,
                weight: REDACTION_WEIGHT,
                weighted_score: redaction.overall.f1 * REDACTION_WEIGHT,
            },
            StageSummary {
                stage: "anchor_curated".to_owned(),
                source_metric: "curated_headline_score.score".to_owned(),
                score: anchor.headline.value,
                weight: ANCHOR_WEIGHT,
                weighted_score: anchor.headline.value * ANCHOR_WEIGHT,
            },
        ],
    };

    let baseline_out_path = default_baseline_out_path(options.out_path.as_path());
    let existing_baseline = if baseline_out_path.exists() {
        match load_json::<BaselineSnapshot>(&baseline_out_path) {
            Ok(value) => Some(value),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    let baseline_compare = existing_baseline
        .as_ref()
        .map(|baseline| build_baseline_compare(&summary, &baseline.summary, &baseline_out_path));

    let payload = CombinedAccuracyBenchmark {
        artifacts: ArtifactPaths {
            redaction_json: redaction_out_path.display().to_string(),
            anchor_json: anchor_out_path.display().to_string(),
        },
        definitions: CombinedDefinitions {
            redaction_stage_score:
                "Redaction stage score from redaction benchmark overall_detection_metrics.detection_f1. Higher is better.",
            anchor_stage_score:
                "Anchor stage score from anchor benchmark curated_headline_score.score (curated-only). Higher is better.",
            total_score:
                "Weighted sum of stage scores. Default weights: redaction=0.60 and anchor_curated=0.40. Synthetic anchor metrics are diagnostic-only and excluded.",
        },
        diagnostics: DiagnosticSummary {
            redaction_precision: redaction.overall.precision,
            redaction_recall: redaction.overall.recall,
            anchor_headline_source: anchor.headline.source,
            anchor_headline_formula: anchor.headline.formula,
            anchor_curated_score: anchor.curated.overall.anchor_score,
            anchor_synthetic_score: anchor.synthetic.overall.anchor_score,
            anchor_curated_row_selected_ratio: anchor.curated.overall.row_selected_ratio,
            anchor_synthetic_row_selected_ratio: anchor.synthetic.overall.row_selected_ratio,
        },
        summary: summary.clone(),
        baseline_compare,
        baseline_bootstrapped: existing_baseline.is_none(),
    };
    let console_summary = render_console_summary(
        &payload,
        options.out_path.as_path(),
        &summary_out_path,
        &baseline_out_path,
    );
    println!();
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
    println!("Combined JSON report file: {}", options.out_path.display());
    println!(
        "Combined markdown summary file: {}",
        summary_out_path.display()
    );

    let baseline_snapshot = BaselineSnapshot { summary };
    if let Err(error) = write_json_file(&baseline_out_path, &baseline_snapshot) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    println!("Baseline snapshot file: {}", baseline_out_path.display());
}

#[cfg(test)]
mod tests {
    use super::{
        anchor_stage_out_path, default_baseline_out_path, parse_positive_usize,
        redaction_stage_out_path, trend_label,
    };
    use std::path::Path;

    #[test]
    fn default_artifact_paths_follow_out_directory() {
        let out = Path::new("benchmark/tmp/combined_benchmark_report.json");
        assert_eq!(
            redaction_stage_out_path(out),
            Path::new("benchmark/tmp/redaction_benchmark_report.json")
        );
        assert_eq!(
            anchor_stage_out_path(out),
            Path::new("benchmark/tmp/anchor_benchmark_report.json")
        );
        assert_eq!(
            default_baseline_out_path(out),
            Path::new("benchmark/tmp/combined_benchmark_report.baseline.json")
        );
    }

    #[test]
    fn parse_positive_usize_rejects_zero() {
        assert!(parse_positive_usize("1").is_ok());
        assert!(parse_positive_usize("0").is_err());
    }

    #[test]
    fn trend_label_handles_signs() {
        assert_eq!(trend_label(0.0_f64), "flat");
        assert_eq!(trend_label(0.2_f64), "up");
        assert_eq!(trend_label(-0.2_f64), "down");
    }
}

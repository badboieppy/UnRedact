use clap::Parser;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use unredact::service::tooling_entry::default_name_dictionary_entries;
use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::{GuessConfig, GuessReport, RedactionGuess};
use unredact::types::visualizer_config::VisualizerConfig;

const EFTA00038617_TARGETS: [&str; 10] = [
    "SARAH KELLEN",
    "ADRIANA MUCINSKA",
    "NADIA MARCINKOVA",
    "LES WEXNER",
    "LESLEY GROFF",
    "JEAN LUC BRUNEL",
    "HALEY ROBSON",
    "WILLIAM HAMMOND",
    "DAVID RODGERS",
    "RICHARD BARNETT",
];

const NOISE_WORDS: [&str; 24] = [
    "ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT", "GOLF", "HOTEL", "INDIA", "JULIET",
    "KILO", "LIMA", "MIKE", "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO", "SIERRA", "TANGO",
    "UNIFORM", "VICTOR", "WHISKEY", "XRAY",
];

const MULTI_SPAN_GAP_RATIO_THRESHOLD: f64 = 2.0_f64;

#[derive(Debug, Clone, Serialize)]
struct RankedTarget {
    label: String,
    target: String,
    best_rank: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkSummary {
    evaluated_items: usize,
    found_items: usize,
    recall_at_1: f64,
    recall_at_5: f64,
    recall_at_20: f64,
    mrr: f64,
    mean_rank_found: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct DatasetResult {
    name: String,
    summary: BenchmarkSummary,
    visual_summary: VisualSummary,
    visual_rerank_summary: VisualRerankSummary,
    timing_summary: TimingSummary,
    candidate_summary: CandidateSummary,
    quality_summary: QualitySummary,
    targets: Vec<RankedTarget>,
}

#[derive(Debug, Clone, Serialize)]
struct AccuracyBenchmark {
    definitions: MetricDefinitions,
    datasets: Vec<DatasetResult>,
    overall: BenchmarkSummary,
    overall_visual: VisualSummary,
    overall_visual_rerank: VisualRerankSummary,
    overall_timing: TimingSummary,
    overall_candidates: CandidateSummary,
    overall_quality: QualitySummary,
    consistency: ConsistencySummary,
}

#[derive(Debug, Clone, Serialize)]
struct MetricDefinitions {
    evaluated_items: &'static str,
    found_items: &'static str,
    recall_at_1: &'static str,
    recall_at_5: &'static str,
    recall_at_20: &'static str,
    mrr: &'static str,
    mean_rank_found: &'static str,
    best_rank: &'static str,
    visual_rows_total: &'static str,
    visual_rows_with_top_guess: &'static str,
    visual_rows_scored: &'static str,
    visual_rows_dropped: &'static str,
    visual_mean_abs_diff: &'static str,
    visual_median_abs_diff: &'static str,
    visual_p90_abs_diff: &'static str,
    visual_mean_changed_pixel_ratio: &'static str,
    visual_mean_compared_pixels: &'static str,
    visual_rerank_rows_considered: &'static str,
    visual_rerank_rows_scored: &'static str,
    visual_rerank_top1_changed: &'static str,
    visual_rerank_top1_changed_ratio: &'static str,
    visual_rerank_mean_gain: &'static str,
    timing_redactions_ms: &'static str,
    timing_fonts_ms: &'static str,
    timing_guess_ms: &'static str,
    timing_visualize_ms: &'static str,
    timing_orchestrator_total_ms: &'static str,
    candidate_rows_total: &'static str,
    candidate_rows_with_candidates: &'static str,
    candidate_mean_count: &'static str,
    candidate_median_count: &'static str,
    candidate_p90_count: &'static str,
    candidate_multi_span_rows: &'static str,
    candidate_multi_span_mean_count: &'static str,
    candidate_multi_span_p90_count: &'static str,
    candidate_single_span_rows: &'static str,
    candidate_single_span_mean_count: &'static str,
    quality_rows_total: &'static str,
    quality_anchored_rows: &'static str,
    quality_anchor_two_sided_rows: &'static str,
    quality_anchor_one_sided_rows: &'static str,
    quality_width_asset_rows: &'static str,
    quality_width_table_rows: &'static str,
    quality_width_core_rows: &'static str,
    quality_width_fallback_rows: &'static str,
    quality_width_fallback_reason_rows: &'static str,
    consistency_repeats: &'static str,
    consistency_all_hashes_identical: &'static str,
    consistency_hash_match_ratio: &'static str,
    consistency_top1_agreement_ratio: &'static str,
    consistency_top5_jaccard_mean: &'static str,
    consistency_mean_rank_stddev: &'static str,
    consistency_unstable_rows_count: &'static str,
    consistency_unstable_rows_ratio: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct VisualSummary {
    rows_total: usize,
    rows_with_top_guess: usize,
    rows_scored: usize,
    rows_dropped: usize,
    mean_abs_diff: Option<f64>,
    median_abs_diff: Option<f64>,
    p90_abs_diff: Option<f64>,
    mean_changed_pixel_ratio: Option<f64>,
    mean_compared_pixels: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct VisualRerankSummary {
    rows_considered: usize,
    rows_scored: usize,
    top1_changed: usize,
    top1_changed_ratio: Option<f64>,
    mean_gain: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct TimingSummary {
    redactions_ms: Option<f64>,
    fonts_ms: Option<f64>,
    guess_ms: Option<f64>,
    visualize_ms: Option<f64>,
    orchestrator_total_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct CandidateSummary {
    rows_total: usize,
    rows_with_candidates: usize,
    mean_count: Option<f64>,
    median_count: Option<f64>,
    p90_count: Option<f64>,
    multi_span_rows: usize,
    multi_span_mean_count: Option<f64>,
    multi_span_p90_count: Option<f64>,
    single_span_rows: usize,
    single_span_mean_count: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Default)]
struct QualitySummary {
    rows_total: usize,
    anchored_rows: usize,
    anchor_two_sided_rows: usize,
    anchor_one_sided_rows: usize,
    width_asset_rows: usize,
    width_table_rows: usize,
    width_core_rows: usize,
    width_fallback_rows: usize,
    width_fallback_reason_rows: usize,
}

#[derive(Debug, Clone, Default)]
struct VisualAccumulator {
    rows_total: usize,
    rows_with_top_guess: usize,
    rows_dropped: usize,
    abs_diff: Vec<f64>,
    changed_ratio: Vec<f64>,
    compared_pixels: Vec<f64>,
}

#[derive(Debug, Clone, Default)]
struct VisualRerankAccumulator {
    rows_considered: usize,
    rows_scored: usize,
    top1_changed: usize,
    weighted_gain_sum: f64,
}

#[derive(Debug, Clone, Default)]
struct CandidateAccumulator {
    rows_total: usize,
    rows_with_candidates: usize,
    counts: Vec<f64>,
    multi_span_counts: Vec<f64>,
    single_span_counts: Vec<f64>,
}

#[derive(Debug, Clone, Default)]
struct TimingAccumulator {
    redactions_ms: Vec<f64>,
    fonts_ms: Vec<f64>,
    guess_ms: Vec<f64>,
    visualize_ms: Vec<f64>,
    orchestrator_total_ms: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct DatasetConsistencySummary {
    dataset: String,
    repeats: usize,
    all_hashes_identical: bool,
    hash_match_ratio: f64,
    top1_agreement_ratio: f64,
    top5_jaccard_mean: f64,
    mean_rank_stddev: Option<f64>,
    unstable_rows_count: usize,
    unstable_rows_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ConsistencySummary {
    repeats: usize,
    all_hashes_identical: bool,
    hash_match_ratio: f64,
    top1_agreement_ratio: f64,
    top5_jaccard_mean: f64,
    mean_rank_stddev: Option<f64>,
    unstable_rows_count: usize,
    unstable_rows_ratio: f64,
    run_hashes: Vec<String>,
    per_dataset: Vec<DatasetConsistencySummary>,
}

#[derive(Debug, Clone, Serialize)]
struct RowSnapshot {
    key: String,
    top1: Option<String>,
    top5: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DatasetRunSnapshot {
    name: String,
    dataset_hash: String,
    rows: Vec<RowSnapshot>,
    target_ranks: Vec<(String, Option<usize>)>,
}

#[derive(Debug, Clone)]
struct BenchmarkRunSnapshot {
    hash: String,
    dataset_runs: Vec<DatasetRunSnapshot>,
}

#[derive(Debug, Clone)]
struct EvaluatedDataset {
    dataset: DatasetResult,
    visual_accumulator: VisualAccumulator,
    visual_rerank_accumulator: VisualRerankAccumulator,
    timing_accumulator: TimingAccumulator,
    candidate_accumulator: CandidateAccumulator,
    run_snapshot: DatasetRunSnapshot,
}

#[derive(Debug, Clone, Parser)]
#[command(
    name = "guess_accuracy_benchmark",
    about = "Run accuracy, performance, and consistency benchmark suites for known test PDFs."
)]
struct CliOptions {
    #[arg(long = "out", default_value = "benchmark/guess_accuracy.json")]
    out_path: PathBuf,
    #[arg(
        long,
        default_value_t = 2_usize,
        value_parser = parse_positive_usize
    )]
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

fn benchmark_config() -> UnredactServiceConfig {
    UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        raster_dpi: 200.0_f32,
        guess: GuessConfig {
            visual_score: true,
            visual_score_dpi: 200.0_f32,
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    }
}

fn load_report(path: &Path) -> Result<GuessReport, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read report {}: {error}", path.display()))?;
    serde_json::from_slice::<GuessReport>(&bytes)
        .map_err(|error| format!("failed to parse report {}: {error}", path.display()))
}

fn run_report(
    input: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
) -> Result<GuessReport, String> {
    let cfg = benchmark_config();
    let outputs = run_from_paths(input, output_dir, dictionary_path, cfg)?;
    load_report(&outputs.guesses_path)
}

fn ordered_guess_texts_upper(guess: &RedactionGuess) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    for text in &guess.exact_matches {
        let normalized = text.trim().to_ascii_uppercase();
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    for candidate in &guess.candidates {
        let normalized = candidate.text.trim().to_ascii_uppercase();
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

fn top1_guess_text(guess: &RedactionGuess) -> Option<String> {
    ordered_guess_texts_upper(guess).into_iter().next()
}

fn top5_guess_texts(guess: &RedactionGuess) -> Vec<String> {
    ordered_guess_texts_upper(guess)
        .into_iter()
        .take(5)
        .collect::<Vec<_>>()
}

fn rank_in_guess(guess: &RedactionGuess, target: &str) -> Option<usize> {
    let target_upper = target.trim().to_ascii_uppercase();
    if target_upper.is_empty() {
        return None;
    }
    let ordered = ordered_guess_texts_upper(guess);
    ordered
        .iter()
        .position(|value| value == &target_upper)
        .map(|index| index + 1)
}

fn best_rank_in_guesses(guesses: &[&RedactionGuess], target: &str) -> Option<usize> {
    guesses
        .iter()
        .filter_map(|guess| rank_in_guess(guess, target))
        .min()
}

fn summarize_ranks(ranks: &[Option<usize>]) -> BenchmarkSummary {
    let evaluated_items = ranks.len();
    let found = ranks.iter().filter_map(|rank| *rank).collect::<Vec<_>>();
    let found_items = found.len();
    let recall_at = |k: usize| -> f64 {
        if evaluated_items == 0 {
            return 0.0_f64;
        }
        let hits = ranks
            .iter()
            .filter_map(|rank| *rank)
            .filter(|rank| *rank <= k)
            .count();
        hits as f64 / evaluated_items as f64
    };
    let mrr = if evaluated_items == 0 {
        0.0_f64
    } else {
        let reciprocal_sum = ranks
            .iter()
            .map(|rank| rank.map_or(0.0_f64, |value| 1.0_f64 / value as f64))
            .sum::<f64>();
        reciprocal_sum / evaluated_items as f64
    };
    let mean_rank_found = if found.is_empty() {
        None
    } else {
        Some(found.iter().map(|value| *value as f64).sum::<f64>() / found.len() as f64)
    };

    BenchmarkSummary {
        evaluated_items,
        found_items,
        recall_at_1: recall_at(1),
        recall_at_5: recall_at(5),
        recall_at_20: recall_at(20),
        mrr,
        mean_rank_found,
    }
}

fn has_top_guess(guess: &RedactionGuess) -> bool {
    !guess.exact_matches.is_empty() || !guess.candidates.is_empty()
}

fn percentile_sorted(values: &[f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let quantile = q.clamp(0.0_f64, 1.0_f64);
    let idx = ((values.len().saturating_sub(1) as f64) * quantile).round() as usize;
    values.get(idx).copied()
}

fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn stddev(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let center = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| {
            let delta = *value - center;
            delta * delta
        })
        .sum::<f64>()
        / values.len() as f64;
    Some(variance.sqrt())
}

fn visual_accumulator_from_guesses(guesses: &[RedactionGuess]) -> VisualAccumulator {
    let mut acc = VisualAccumulator {
        rows_total: guesses.len(),
        ..VisualAccumulator::default()
    };
    for guess in guesses {
        if has_top_guess(guess) {
            acc.rows_with_top_guess += 1;
        }
        if guess.visual_dropped {
            acc.rows_dropped += 1;
        }
        if let Some(value) = guess.visual_mean_abs_diff {
            acc.abs_diff.push(value as f64);
        }
        if let Some(value) = guess.visual_changed_pixel_ratio {
            acc.changed_ratio.push(value as f64);
        }
        if let Some(value) = guess.visual_compared_pixels {
            acc.compared_pixels.push(value as f64);
        }
    }
    acc
}

fn summarize_visual_accumulator(mut acc: VisualAccumulator) -> VisualSummary {
    acc.abs_diff
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    VisualSummary {
        rows_total: acc.rows_total,
        rows_with_top_guess: acc.rows_with_top_guess,
        rows_scored: acc.abs_diff.len(),
        rows_dropped: acc.rows_dropped,
        mean_abs_diff: mean(&acc.abs_diff),
        median_abs_diff: percentile_sorted(&acc.abs_diff, 0.5_f64),
        p90_abs_diff: percentile_sorted(&acc.abs_diff, 0.9_f64),
        mean_changed_pixel_ratio: mean(&acc.changed_ratio),
        mean_compared_pixels: mean(&acc.compared_pixels),
    }
}

fn merge_visual_accumulators(accumulators: &[VisualAccumulator]) -> VisualAccumulator {
    let mut merged = VisualAccumulator::default();
    for acc in accumulators {
        merged.rows_total += acc.rows_total;
        merged.rows_with_top_guess += acc.rows_with_top_guess;
        merged.rows_dropped += acc.rows_dropped;
        merged.abs_diff.extend_from_slice(&acc.abs_diff);
        merged.changed_ratio.extend_from_slice(&acc.changed_ratio);
        merged
            .compared_pixels
            .extend_from_slice(&acc.compared_pixels);
    }
    merged
}

fn visual_rerank_accumulator_from_diagnostics(diagnostics: &[String]) -> VisualRerankAccumulator {
    let mut acc = VisualRerankAccumulator::default();
    for line in diagnostics {
        let Some(rest) = line.strip_prefix("visual_rerank=") else {
            continue;
        };
        let mut rows_considered = None::<usize>;
        let mut rows_scored = None::<usize>;
        let mut top1_changed = None::<usize>;
        let mut mean_gain = None::<f64>;
        for token in rest.split_whitespace() {
            if let Some(value) = token.strip_prefix("rows_considered=") {
                rows_considered = value.parse::<usize>().ok();
            } else if let Some(value) = token.strip_prefix("rows_scored=") {
                rows_scored = value.parse::<usize>().ok();
            } else if let Some(value) = token.strip_prefix("top1_changed=") {
                top1_changed = value.parse::<usize>().ok();
            } else if let Some(value) = token.strip_prefix("mean_gain=") {
                mean_gain = value.parse::<f64>().ok();
            }
        }
        let scored = rows_scored.unwrap_or(0_usize);
        acc.rows_considered += rows_considered.unwrap_or(0_usize);
        acc.rows_scored += scored;
        acc.top1_changed += top1_changed.unwrap_or(0_usize);
        if let Some(gain) = mean_gain {
            acc.weighted_gain_sum += gain * scored as f64;
        }
    }
    acc
}

fn summarize_visual_rerank_accumulator(acc: &VisualRerankAccumulator) -> VisualRerankSummary {
    VisualRerankSummary {
        rows_considered: acc.rows_considered,
        rows_scored: acc.rows_scored,
        top1_changed: acc.top1_changed,
        top1_changed_ratio: if acc.rows_scored == 0 {
            None
        } else {
            Some(acc.top1_changed as f64 / acc.rows_scored as f64)
        },
        mean_gain: if acc.rows_scored == 0 {
            None
        } else {
            Some(acc.weighted_gain_sum / acc.rows_scored as f64)
        },
    }
}

fn merge_visual_rerank_accumulators(
    accumulators: &[VisualRerankAccumulator],
) -> VisualRerankAccumulator {
    let mut merged = VisualRerankAccumulator::default();
    for acc in accumulators {
        merged.rows_considered += acc.rows_considered;
        merged.rows_scored += acc.rows_scored;
        merged.top1_changed += acc.top1_changed;
        merged.weighted_gain_sum += acc.weighted_gain_sum;
    }
    merged
}

fn is_multi_span_guess(guess: &RedactionGuess) -> bool {
    if !guess.context.has_anchor_pair {
        return false;
    }
    let width = guess.bbox.width().abs() as f64;
    if width <= 0.0_f64 {
        return false;
    }
    (guess.context.gap_pt as f64).abs() / width >= MULTI_SPAN_GAP_RATIO_THRESHOLD
}

fn candidate_accumulator_from_guesses(guesses: &[RedactionGuess]) -> CandidateAccumulator {
    let mut acc = CandidateAccumulator {
        rows_total: guesses.len(),
        ..CandidateAccumulator::default()
    };
    for guess in guesses {
        let count = guess.candidates.len() as f64;
        if count > 0.0_f64 {
            acc.rows_with_candidates += 1;
        }
        acc.counts.push(count);
        if is_multi_span_guess(guess) {
            acc.multi_span_counts.push(count);
        } else {
            acc.single_span_counts.push(count);
        }
    }
    acc
}

fn summarize_candidate_accumulator(mut acc: CandidateAccumulator) -> CandidateSummary {
    acc.counts
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    acc.multi_span_counts
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    acc.single_span_counts
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    CandidateSummary {
        rows_total: acc.rows_total,
        rows_with_candidates: acc.rows_with_candidates,
        mean_count: mean(&acc.counts),
        median_count: percentile_sorted(&acc.counts, 0.5_f64),
        p90_count: percentile_sorted(&acc.counts, 0.9_f64),
        multi_span_rows: acc.multi_span_counts.len(),
        multi_span_mean_count: mean(&acc.multi_span_counts),
        multi_span_p90_count: percentile_sorted(&acc.multi_span_counts, 0.9_f64),
        single_span_rows: acc.single_span_counts.len(),
        single_span_mean_count: mean(&acc.single_span_counts),
    }
}

fn merge_candidate_accumulators(accumulators: &[CandidateAccumulator]) -> CandidateAccumulator {
    let mut merged = CandidateAccumulator::default();
    for acc in accumulators {
        merged.rows_total += acc.rows_total;
        merged.rows_with_candidates += acc.rows_with_candidates;
        merged.counts.extend_from_slice(&acc.counts);
        merged
            .multi_span_counts
            .extend_from_slice(&acc.multi_span_counts);
        merged
            .single_span_counts
            .extend_from_slice(&acc.single_span_counts);
    }
    merged
}

fn quality_summary_from_guesses(guesses: &[RedactionGuess]) -> QualitySummary {
    let mut out = QualitySummary {
        rows_total: guesses.len(),
        ..QualitySummary::default()
    };
    for guess in guesses {
        if guess.context.has_anchor_pair {
            out.anchored_rows += 1;
        }
        match guess.context.anchor_mode.as_deref() {
            Some("two_sided") => out.anchor_two_sided_rows += 1,
            Some("left_only") | Some("right_only") => out.anchor_one_sided_rows += 1,
            _ => {}
        }
        let width_source = guess
            .context
            .candidate_width_source
            .as_deref()
            .or(guess.context.anchor_width_source.as_deref());
        match width_source {
            Some("asset") => out.width_asset_rows += 1,
            Some("pdf_width_table") => out.width_table_rows += 1,
            Some("core_font") => out.width_core_rows += 1,
            Some("fallback") => out.width_fallback_rows += 1,
            _ => {}
        }
        if guess.context.width_fallback_reason.is_some() {
            out.width_fallback_reason_rows += 1;
        }
    }
    out
}

fn merge_quality_summaries(summaries: &[QualitySummary]) -> QualitySummary {
    let mut merged = QualitySummary::default();
    for summary in summaries {
        merged.rows_total += summary.rows_total;
        merged.anchored_rows += summary.anchored_rows;
        merged.anchor_two_sided_rows += summary.anchor_two_sided_rows;
        merged.anchor_one_sided_rows += summary.anchor_one_sided_rows;
        merged.width_asset_rows += summary.width_asset_rows;
        merged.width_table_rows += summary.width_table_rows;
        merged.width_core_rows += summary.width_core_rows;
        merged.width_fallback_rows += summary.width_fallback_rows;
        merged.width_fallback_reason_rows += summary.width_fallback_reason_rows;
    }
    merged
}

fn timing_accumulator_from_diagnostics(diagnostics: &[String]) -> TimingAccumulator {
    let mut acc = TimingAccumulator::default();
    for line in diagnostics {
        if !line.starts_with("timing_ms stage=") {
            continue;
        }
        let mut stage = None::<String>;
        let mut value = None::<f64>;
        for token in line.split_whitespace() {
            if let Some(rest) = token.strip_prefix("stage=") {
                stage = Some(rest.to_owned());
            } else if let Some(rest) = token.strip_prefix("value=") {
                value = rest.parse::<f64>().ok();
            }
        }
        let (Some(stage), Some(value)) = (stage, value) else {
            continue;
        };
        match stage.as_str() {
            "redactions" => acc.redactions_ms.push(value),
            "fonts" => acc.fonts_ms.push(value),
            "guess" => acc.guess_ms.push(value),
            "visualize" => acc.visualize_ms.push(value),
            "orchestrator_total" => acc.orchestrator_total_ms.push(value),
            _ => {}
        }
    }
    acc
}

fn summarize_timing_accumulator(acc: &TimingAccumulator) -> TimingSummary {
    TimingSummary {
        redactions_ms: mean(&acc.redactions_ms),
        fonts_ms: mean(&acc.fonts_ms),
        guess_ms: mean(&acc.guess_ms),
        visualize_ms: mean(&acc.visualize_ms),
        orchestrator_total_ms: mean(&acc.orchestrator_total_ms),
    }
}

fn merge_timing_accumulators(accumulators: &[TimingAccumulator]) -> TimingAccumulator {
    let mut merged = TimingAccumulator::default();
    for acc in accumulators {
        merged.redactions_ms.extend_from_slice(&acc.redactions_ms);
        merged.fonts_ms.extend_from_slice(&acc.fonts_ms);
        merged.guess_ms.extend_from_slice(&acc.guess_ms);
        merged.visualize_ms.extend_from_slice(&acc.visualize_ms);
        merged
            .orchestrator_total_ms
            .extend_from_slice(&acc.orchestrator_total_ms);
    }
    merged
}

fn write_noisy_dictionary(path: &Path, targets: &[&str]) -> Result<(), String> {
    let mut lines = targets
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let target_set = targets
        .iter()
        .map(|value| value.to_ascii_uppercase())
        .collect::<std::collections::BTreeSet<_>>();

    for value in default_name_dictionary_entries() {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if target_set.contains(&trimmed.to_ascii_uppercase()) {
            continue;
        }
        lines.push(trimmed.to_owned());
        if lines.len() >= 1_200 {
            break;
        }
    }
    lines.extend(NOISE_WORDS.into_iter().map(str::to_owned));
    std::fs::write(path, lines.join("\n"))
        .map_err(|error| format!("failed to write dictionary {}: {error}", path.display()))
}

fn hash_json<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hasher.finalize();
    Ok(format!("{hash:x}"))
}

fn build_row_snapshots(dataset: &str, guesses: &[RedactionGuess]) -> Vec<RowSnapshot> {
    guesses
        .iter()
        .enumerate()
        .map(|(index, guess)| RowSnapshot {
            key: format!(
                "{}:{}:{}:{:.2}:{:.2}:{:.2}:{:.2}",
                dataset,
                index,
                guess.page_index,
                guess.bbox.x0,
                guess.bbox.y0,
                guess.bbox.x1,
                guess.bbox.y1
            ),
            top1: top1_guess_text(guess),
            top5: top5_guess_texts(guess),
        })
        .collect::<Vec<_>>()
}

fn jaccard(left: &[String], right: &[String]) -> f64 {
    let left_set = left.iter().cloned().collect::<BTreeSet<_>>();
    let right_set = right.iter().cloned().collect::<BTreeSet<_>>();
    if left_set.is_empty() && right_set.is_empty() {
        return 1.0_f64;
    }
    let inter = left_set.intersection(&right_set).count() as f64;
    let union = left_set.union(&right_set).count() as f64;
    if union <= 0.0_f64 {
        0.0_f64
    } else {
        inter / union
    }
}

fn compute_row_consistency(row_sets: &[Vec<RowSnapshot>]) -> (f64, f64, usize, f64) {
    if row_sets.is_empty() || row_sets[0].is_empty() {
        return (1.0_f64, 1.0_f64, 0_usize, 0.0_f64);
    }
    let base = &row_sets[0];
    let maps = row_sets
        .iter()
        .map(|rows| {
            rows.iter()
                .map(|row| (row.key.clone(), row.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .collect::<Vec<_>>();

    let mut top1_same = 0_usize;
    let mut unstable = 0_usize;
    let mut jaccards = Vec::<f64>::new();

    for row in base {
        let aligned = maps
            .iter()
            .map(|map| map.get(&row.key).cloned())
            .collect::<Vec<_>>();
        let first_top1 = aligned
            .first()
            .and_then(|value| value.as_ref())
            .and_then(|row| row.top1.clone());
        let same_top1 = aligned
            .iter()
            .all(|entry| entry.as_ref().and_then(|row| row.top1.clone()) == first_top1);
        if same_top1 {
            top1_same += 1;
        } else {
            unstable += 1;
        }
        for left_idx in 0..aligned.len() {
            for right_idx in (left_idx + 1)..aligned.len() {
                let left = aligned[left_idx]
                    .as_ref()
                    .map(|value| value.top5.clone())
                    .unwrap_or_default();
                let right = aligned[right_idx]
                    .as_ref()
                    .map(|value| value.top5.clone())
                    .unwrap_or_default();
                jaccards.push(jaccard(&left, &right));
            }
        }
    }

    let row_count = base.len() as f64;
    (
        top1_same as f64 / row_count,
        mean(&jaccards).unwrap_or(1.0_f64),
        unstable,
        unstable as f64 / row_count,
    )
}

fn compute_rank_stddev(rank_sets: &[Vec<(String, Option<usize>)>]) -> Option<f64> {
    if rank_sets.is_empty() || rank_sets[0].is_empty() {
        return None;
    }
    let maps = rank_sets
        .iter()
        .map(|values| values.iter().cloned().collect::<BTreeMap<_, _>>())
        .collect::<Vec<_>>();
    let mut deviations = Vec::<f64>::new();
    for (label, _) in &rank_sets[0] {
        let series = maps
            .iter()
            .map(|map| map.get(label).copied().flatten().unwrap_or(10_000_usize) as f64)
            .collect::<Vec<_>>();
        if let Some(value) = stddev(&series) {
            deviations.push(value);
        }
    }
    mean(&deviations)
}

fn evaluate_efta00101126(root: &Path) -> Result<EvaluatedDataset, String> {
    let input = Path::new("test_data/EFTA00101126.pdf");
    if !input.exists() {
        return Err(format!("missing dataset input {}", input.display()));
    }
    let output_dir = root.join("efta00101126");
    std::fs::create_dir_all(&output_dir)
        .map_err(|error| format!("failed to create {}: {error}", output_dir.display()))?;
    let report = run_report(input, &output_dir, None)?;

    let mut targets = Vec::<RankedTarget>::new();
    let target_text = "SARAH KELLEN";
    if report.guesses.len() >= 2 {
        let second_last = &report.guesses[report.guesses.len() - 2];
        let last = &report.guesses[report.guesses.len() - 1];
        targets.push(RankedTarget {
            label: "second_last".to_owned(),
            target: target_text.to_owned(),
            best_rank: rank_in_guess(second_last, target_text),
        });
        targets.push(RankedTarget {
            label: "last".to_owned(),
            target: target_text.to_owned(),
            best_rank: rank_in_guess(last, target_text),
        });
    } else {
        targets.push(RankedTarget {
            label: "second_last".to_owned(),
            target: target_text.to_owned(),
            best_rank: None,
        });
        targets.push(RankedTarget {
            label: "last".to_owned(),
            target: target_text.to_owned(),
            best_rank: None,
        });
    }

    let ranks = targets
        .iter()
        .map(|target| target.best_rank)
        .collect::<Vec<_>>();
    let visual_accumulator = visual_accumulator_from_guesses(&report.guesses);
    let visual_rerank_accumulator = visual_rerank_accumulator_from_diagnostics(&report.diagnostics);
    let visual_rerank_summary = summarize_visual_rerank_accumulator(&visual_rerank_accumulator);
    let timing_accumulator = timing_accumulator_from_diagnostics(&report.diagnostics);
    let candidate_accumulator = candidate_accumulator_from_guesses(&report.guesses);
    let quality_summary = quality_summary_from_guesses(&report.guesses);
    let timing_summary = summarize_timing_accumulator(&timing_accumulator);
    let visual_summary = summarize_visual_accumulator(visual_accumulator.clone());
    let candidate_summary = summarize_candidate_accumulator(candidate_accumulator.clone());
    let dataset = DatasetResult {
        name: "EFTA00101126".to_owned(),
        summary: summarize_ranks(&ranks),
        visual_summary,
        visual_rerank_summary,
        timing_summary,
        candidate_summary,
        quality_summary: quality_summary.clone(),
        targets: targets.clone(),
    };
    let run_snapshot = DatasetRunSnapshot {
        name: "EFTA00101126".to_owned(),
        dataset_hash: hash_json(&(
            dataset.name.clone(),
            dataset.summary.clone(),
            dataset.visual_summary.clone(),
            dataset.visual_rerank_summary.clone(),
            dataset.candidate_summary.clone(),
            dataset.quality_summary.clone(),
            dataset.targets.clone(),
        ))?,
        rows: build_row_snapshots("EFTA00101126", &report.guesses),
        target_ranks: targets
            .iter()
            .map(|target| (target.label.clone(), target.best_rank))
            .collect::<Vec<_>>(),
    };
    Ok(EvaluatedDataset {
        dataset,
        visual_accumulator,
        visual_rerank_accumulator,
        timing_accumulator,
        candidate_accumulator,
        run_snapshot,
    })
}

fn evaluate_efta00038617(root: &Path) -> Result<EvaluatedDataset, String> {
    let input = Path::new("test_data/EFTA00038617.pdf");
    if !input.exists() {
        return Err(format!("missing dataset input {}", input.display()));
    }
    let output_dir = root.join("efta00038617");
    std::fs::create_dir_all(&output_dir)
        .map_err(|error| format!("failed to create {}: {error}", output_dir.display()))?;
    let dictionary_path = output_dir.join("benchmark_dictionary.txt");
    write_noisy_dictionary(&dictionary_path, &EFTA00038617_TARGETS)?;
    let report = run_report(input, &output_dir, Some(&dictionary_path))?;

    let first_bullet = report
        .guesses
        .iter()
        .filter(|guess| {
            guess.page_index == 1 && guess.bbox.y0 >= 440.0_f32 && guess.bbox.y1 <= 505.0_f32
        })
        .collect::<Vec<_>>();

    let targets = EFTA00038617_TARGETS
        .iter()
        .map(|target| RankedTarget {
            label: (*target).to_owned(),
            target: (*target).to_owned(),
            best_rank: best_rank_in_guesses(&first_bullet, target),
        })
        .collect::<Vec<_>>();

    let ranks = targets
        .iter()
        .map(|target| target.best_rank)
        .collect::<Vec<_>>();
    let visual_accumulator = visual_accumulator_from_guesses(&report.guesses);
    let visual_rerank_accumulator = visual_rerank_accumulator_from_diagnostics(&report.diagnostics);
    let visual_rerank_summary = summarize_visual_rerank_accumulator(&visual_rerank_accumulator);
    let timing_accumulator = timing_accumulator_from_diagnostics(&report.diagnostics);
    let candidate_accumulator = candidate_accumulator_from_guesses(&report.guesses);
    let quality_summary = quality_summary_from_guesses(&report.guesses);
    let timing_summary = summarize_timing_accumulator(&timing_accumulator);
    let visual_summary = summarize_visual_accumulator(visual_accumulator.clone());
    let candidate_summary = summarize_candidate_accumulator(candidate_accumulator.clone());
    let dataset = DatasetResult {
        name: "EFTA00038617".to_owned(),
        summary: summarize_ranks(&ranks),
        visual_summary,
        visual_rerank_summary,
        timing_summary,
        candidate_summary,
        quality_summary: quality_summary.clone(),
        targets: targets.clone(),
    };
    let run_snapshot = DatasetRunSnapshot {
        name: "EFTA00038617".to_owned(),
        dataset_hash: hash_json(&(
            dataset.name.clone(),
            dataset.summary.clone(),
            dataset.visual_summary.clone(),
            dataset.visual_rerank_summary.clone(),
            dataset.candidate_summary.clone(),
            dataset.quality_summary.clone(),
            dataset.targets.clone(),
        ))?,
        rows: build_row_snapshots("EFTA00038617", &report.guesses),
        target_ranks: targets
            .iter()
            .map(|target| (target.label.clone(), target.best_rank))
            .collect::<Vec<_>>(),
    };
    Ok(EvaluatedDataset {
        dataset,
        visual_accumulator,
        visual_rerank_accumulator,
        timing_accumulator,
        candidate_accumulator,
        run_snapshot,
    })
}

fn print_summary(label: &str, summary: &BenchmarkSummary) {
    println!(
        "{label:16} items={:>2} found={:>2} r@1={:>5.1}% r@5={:>5.1}% r@20={:>5.1}% mrr={:.3} mean_rank={}",
        summary.evaluated_items,
        summary.found_items,
        summary.recall_at_1 * 100.0_f64,
        summary.recall_at_5 * 100.0_f64,
        summary.recall_at_20 * 100.0_f64,
        summary.mrr,
        summary
            .mean_rank_found
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_owned())
    );
}

fn print_visual_summary(label: &str, summary: &VisualSummary) {
    println!(
        "{label:16} rows={:>3} top={:>3} scored={:>3} dropped={:>3} mean_abs_diff={} median_abs_diff={} p90_abs_diff={} mean_changed={} mean_pixels={}",
        summary.rows_total,
        summary.rows_with_top_guess,
        summary.rows_scored,
        summary.rows_dropped,
        summary
            .mean_abs_diff
            .map(|value| format!("{value:.4}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .median_abs_diff
            .map(|value| format!("{value:.4}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .p90_abs_diff
            .map(|value| format!("{value:.4}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .mean_changed_pixel_ratio
            .map(|value| format!("{:.2}%", value * 100.0_f64))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .mean_compared_pixels
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned())
    );
}

fn print_visual_rerank_summary(label: &str, summary: &VisualRerankSummary) {
    println!(
        "{label:16} considered={:>3} scored={:>3} top1_changed={:>3} top1_changed_ratio={} mean_gain={}",
        summary.rows_considered,
        summary.rows_scored,
        summary.top1_changed,
        summary
            .top1_changed_ratio
            .map(|value| format!("{:.2}%", value * 100.0_f64))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .mean_gain
            .map(|value| format!("{value:.4}"))
            .unwrap_or_else(|| "-".to_owned())
    );
}

fn print_timing_summary(label: &str, summary: &TimingSummary) {
    println!(
        "{label:16} redactions_ms={} fonts_ms={} guess_ms={} visualize_ms={} total_ms={}",
        summary
            .redactions_ms
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .fonts_ms
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .guess_ms
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .visualize_ms
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .orchestrator_total_ms
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned())
    );
}

fn print_candidate_summary(label: &str, summary: &CandidateSummary) {
    println!(
        "{label:16} rows={} with_candidates={} mean={} median={} p90={} multi_rows={} multi_mean={} multi_p90={} single_rows={} single_mean={}",
        summary.rows_total,
        summary.rows_with_candidates,
        summary
            .mean_count
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .median_count
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .p90_count
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary.multi_span_rows,
        summary
            .multi_span_mean_count
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary
            .multi_span_p90_count
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
        summary.single_span_rows,
        summary
            .single_span_mean_count
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "-".to_owned()),
    );
}

fn print_quality_summary(label: &str, summary: &QualitySummary) {
    println!(
        "{label:16} rows={} anchored={} two_sided={} one_sided={} width_asset={} width_table={} width_core={} width_fallback={} fallback_reason_rows={}",
        summary.rows_total,
        summary.anchored_rows,
        summary.anchor_two_sided_rows,
        summary.anchor_one_sided_rows,
        summary.width_asset_rows,
        summary.width_table_rows,
        summary.width_core_rows,
        summary.width_fallback_rows,
        summary.width_fallback_reason_rows
    );
}

fn metric_definitions() -> MetricDefinitions {
    MetricDefinitions {
        evaluated_items: "Number of target strings evaluated in this dataset.",
        found_items:
            "How many targets appeared anywhere in ranked guesses (exact matches + candidate list).",
        recall_at_1: "Fraction of targets with best_rank <= 1. Higher is better.",
        recall_at_5: "Fraction of targets with best_rank <= 5. Higher is better.",
        recall_at_20: "Fraction of targets with best_rank <= 20. Higher is better.",
        mrr: "Mean reciprocal rank across all targets: avg(1/rank), with 0 for not-found.",
        mean_rank_found: "Average rank among found targets only. Lower is better.",
        best_rank:
            "Per-target best observed rank (1 is top candidate). Null means the target was not found.",
        visual_rows_total: "Total redaction rows in guesses for the dataset.",
        visual_rows_with_top_guess:
            "Rows where a top guess exists (either first exact_match or first candidate).",
        visual_rows_scored:
            "Rows with computed visual score (visual_mean_abs_diff present).",
        visual_rows_dropped:
            "Rows removed by visual thresholding (visual_dropped=true).",
        visual_mean_abs_diff:
            "Mean absolute grayscale delta in the non-redaction overlay window; lower is better.",
        visual_median_abs_diff:
            "Median of visual_mean_abs_diff across scored rows; lower is better.",
        visual_p90_abs_diff:
            "90th percentile of visual_mean_abs_diff across scored rows; lower is better.",
        visual_mean_changed_pixel_ratio:
            "Average fraction of significantly changed pixels in scored rows; lower is better.",
        visual_mean_compared_pixels:
            "Average non-background pixel count used per scored row.",
        visual_rerank_rows_considered:
            "Rows eligible for visual rerank candidate scoring (top-K evaluation path).",
        visual_rerank_rows_scored:
            "Rows where visual rerank actually scored candidate alternatives.",
        visual_rerank_top1_changed:
            "Rows where rerank changed top-1 guess from geometric ranking.",
        visual_rerank_top1_changed_ratio:
            "top1_changed / rows_scored for visual rerank; higher means more rerank impact.",
        visual_rerank_mean_gain:
            "Average blended-score gain of chosen rerank candidate over baseline top candidate.",
        timing_redactions_ms: "Average redaction stage runtime in milliseconds.",
        timing_fonts_ms: "Average font extraction stage runtime in milliseconds.",
        timing_guess_ms: "Average guessing stage runtime in milliseconds.",
        timing_visualize_ms: "Average visualization stage runtime in milliseconds.",
        timing_orchestrator_total_ms: "Average total orchestrator runtime in milliseconds.",
        candidate_rows_total: "Total redaction rows considered for candidate-count statistics.",
        candidate_rows_with_candidates: "Rows where the candidate list is non-empty.",
        candidate_mean_count: "Mean number of candidates per row.",
        candidate_median_count: "Median number of candidates per row.",
        candidate_p90_count: "90th percentile candidate count per row.",
        candidate_multi_span_rows: "Rows classified as multi-span by anchor-gap ratio.",
        candidate_multi_span_mean_count: "Mean candidate count for multi-span rows.",
        candidate_multi_span_p90_count: "90th percentile candidate count for multi-span rows.",
        candidate_single_span_rows: "Rows classified as non-multi-span rows.",
        candidate_single_span_mean_count: "Mean candidate count for non-multi-span rows.",
        quality_rows_total: "Total rows in the guess report for anchor/width quality accounting.",
        quality_anchored_rows: "Rows that have an anchor pair or one-sided recovered anchor.",
        quality_anchor_two_sided_rows:
            "Rows where anchor_mode is two_sided (full left/right anchor).",
        quality_anchor_one_sided_rows:
            "Rows where anchor_mode is left_only or right_only.",
        quality_width_asset_rows:
            "Rows whose primary candidate width source is embedded font asset shaping.",
        quality_width_table_rows:
            "Rows whose primary candidate width source is PDF width table lookup.",
        quality_width_core_rows:
            "Rows whose primary candidate width source is core-font width table fallback.",
        quality_width_fallback_rows:
            "Rows whose primary candidate width source is heuristic fallback.",
        quality_width_fallback_reason_rows:
            "Rows carrying explicit width_fallback_reason diagnostics.",
        consistency_repeats: "Number of repeated benchmark runs with the same code/config.",
        consistency_all_hashes_identical:
            "True when every repeated run produced the same benchmark hash.",
        consistency_hash_match_ratio: "Fraction of runs whose hash matches run #1.",
        consistency_top1_agreement_ratio:
            "Fraction of rows whose top1 guess is identical across repeated runs.",
        consistency_top5_jaccard_mean:
            "Mean pairwise Jaccard similarity of top5 guess sets across repeated runs.",
        consistency_mean_rank_stddev:
            "Average per-target standard deviation of rank across repeats.",
        consistency_unstable_rows_count:
            "Number of rows that changed top1 guess across repeated runs.",
        consistency_unstable_rows_ratio:
            "Unstable row count divided by total compared rows.",
    }
}

fn compute_consistency(run_snapshots: &[BenchmarkRunSnapshot]) -> ConsistencySummary {
    if run_snapshots.is_empty() {
        return ConsistencySummary {
            repeats: 0,
            all_hashes_identical: true,
            hash_match_ratio: 1.0_f64,
            top1_agreement_ratio: 1.0_f64,
            top5_jaccard_mean: 1.0_f64,
            mean_rank_stddev: None,
            unstable_rows_count: 0,
            unstable_rows_ratio: 0.0_f64,
            run_hashes: Vec::new(),
            per_dataset: Vec::new(),
        };
    }

    let run_hashes = run_snapshots
        .iter()
        .map(|snapshot| snapshot.hash.clone())
        .collect::<Vec<_>>();
    let first_hash = run_hashes.first().cloned().unwrap_or_default();
    let hash_matches = run_hashes
        .iter()
        .filter(|value| *value == &first_hash)
        .count();
    let repeats = run_snapshots.len();
    let hash_match_ratio = hash_matches as f64 / repeats as f64;
    let all_hashes_identical = hash_matches == repeats;

    let dataset_names = run_snapshots
        .iter()
        .flat_map(|snapshot| {
            snapshot
                .dataset_runs
                .iter()
                .map(|dataset| dataset.name.clone())
        })
        .collect::<BTreeSet<_>>();

    let mut per_dataset = Vec::<DatasetConsistencySummary>::new();
    for dataset_name in dataset_names {
        let dataset_runs = run_snapshots
            .iter()
            .map(|snapshot| {
                snapshot
                    .dataset_runs
                    .iter()
                    .find(|dataset| dataset.name == dataset_name)
                    .cloned()
            })
            .collect::<Vec<_>>();
        let dataset_hashes = dataset_runs
            .iter()
            .map(|entry| entry.as_ref().map(|value| value.dataset_hash.clone()))
            .collect::<Vec<_>>();
        let dataset_first_hash = dataset_hashes.first().cloned().unwrap_or(None);
        let dataset_hash_matches = dataset_hashes
            .iter()
            .filter(|value| **value == dataset_first_hash)
            .count();
        let dataset_rows = dataset_runs
            .iter()
            .map(|entry| {
                entry
                    .as_ref()
                    .map(|value| value.rows.clone())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let (top1, top5, unstable_count, unstable_ratio) = compute_row_consistency(&dataset_rows);
        let rank_sets = dataset_runs
            .iter()
            .map(|entry| {
                entry
                    .as_ref()
                    .map(|value| value.target_ranks.clone())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        per_dataset.push(DatasetConsistencySummary {
            dataset: dataset_name,
            repeats,
            all_hashes_identical: dataset_hash_matches == repeats,
            hash_match_ratio: dataset_hash_matches as f64 / repeats as f64,
            top1_agreement_ratio: top1,
            top5_jaccard_mean: top5,
            mean_rank_stddev: compute_rank_stddev(&rank_sets),
            unstable_rows_count: unstable_count,
            unstable_rows_ratio: unstable_ratio,
        });
    }

    let overall_rows = run_snapshots
        .iter()
        .map(|snapshot| {
            snapshot
                .dataset_runs
                .iter()
                .flat_map(|dataset| {
                    dataset.rows.iter().map(|row| RowSnapshot {
                        key: format!("{}::{}", dataset.name, row.key),
                        top1: row.top1.clone(),
                        top5: row.top5.clone(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let (top1_agreement_ratio, top5_jaccard_mean, unstable_rows_count, unstable_rows_ratio) =
        compute_row_consistency(&overall_rows);

    let overall_ranks = run_snapshots
        .iter()
        .map(|snapshot| {
            snapshot
                .dataset_runs
                .iter()
                .flat_map(|dataset| {
                    dataset
                        .target_ranks
                        .iter()
                        .map(|(label, rank)| (format!("{}::{}", dataset.name, label), *rank))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    ConsistencySummary {
        repeats,
        all_hashes_identical,
        hash_match_ratio,
        top1_agreement_ratio,
        top5_jaccard_mean,
        mean_rank_stddev: compute_rank_stddev(&overall_ranks),
        unstable_rows_count,
        unstable_rows_ratio,
        run_hashes,
        per_dataset,
    }
}

fn main() {
    let options = CliOptions::parse();
    let mut run_snapshots = Vec::<BenchmarkRunSnapshot>::new();
    let mut selected_payload = None::<AccuracyBenchmark>;

    for repeat in 0..options.repeats {
        let benchmark_root = std::env::temp_dir().join(format!(
            "unredact_accuracy_benchmark_{}_{}",
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

        let efta00101126 = match evaluate_efta00101126(&benchmark_root) {
            Ok(result) => result,
            Err(error) => {
                eprintln!(
                    "benchmark failed for EFTA00101126 on repeat {}: {error}",
                    repeat + 1
                );
                std::process::exit(1);
            }
        };
        let efta00038617 = match evaluate_efta00038617(&benchmark_root) {
            Ok(result) => result,
            Err(error) => {
                eprintln!(
                    "benchmark failed for EFTA00038617 on repeat {}: {error}",
                    repeat + 1
                );
                std::process::exit(1);
            }
        };

        let evaluated = [efta00101126, efta00038617];
        let datasets = evaluated
            .iter()
            .map(|item| item.dataset.clone())
            .collect::<Vec<_>>();
        let overall_ranks = datasets
            .iter()
            .flat_map(|dataset| dataset.targets.iter().map(|target| target.best_rank))
            .collect::<Vec<_>>();
        let overall = summarize_ranks(&overall_ranks);
        let visual_accumulators = evaluated
            .iter()
            .map(|item| item.visual_accumulator.clone())
            .collect::<Vec<_>>();
        let overall_visual =
            summarize_visual_accumulator(merge_visual_accumulators(&visual_accumulators));
        let visual_rerank_accumulators = evaluated
            .iter()
            .map(|item| item.visual_rerank_accumulator.clone())
            .collect::<Vec<_>>();
        let overall_visual_rerank = summarize_visual_rerank_accumulator(
            &merge_visual_rerank_accumulators(&visual_rerank_accumulators),
        );
        let timing_accumulators = evaluated
            .iter()
            .map(|item| item.timing_accumulator.clone())
            .collect::<Vec<_>>();
        let overall_timing =
            summarize_timing_accumulator(&merge_timing_accumulators(&timing_accumulators));
        let candidate_accumulators = evaluated
            .iter()
            .map(|item| item.candidate_accumulator.clone())
            .collect::<Vec<_>>();
        let overall_candidates =
            summarize_candidate_accumulator(merge_candidate_accumulators(&candidate_accumulators));
        let quality_summaries = evaluated
            .iter()
            .map(|item| item.dataset.quality_summary.clone())
            .collect::<Vec<_>>();
        let overall_quality = merge_quality_summaries(&quality_summaries);
        let definitions = metric_definitions();

        let provisional = AccuracyBenchmark {
            definitions,
            datasets,
            overall,
            overall_visual,
            overall_visual_rerank,
            overall_timing,
            overall_candidates,
            overall_quality,
            consistency: ConsistencySummary {
                repeats: 1,
                all_hashes_identical: true,
                hash_match_ratio: 1.0_f64,
                top1_agreement_ratio: 1.0_f64,
                top5_jaccard_mean: 1.0_f64,
                mean_rank_stddev: None,
                unstable_rows_count: 0,
                unstable_rows_ratio: 0.0_f64,
                run_hashes: Vec::new(),
                per_dataset: Vec::new(),
            },
        };
        let dataset_runs = evaluated
            .iter()
            .map(|item| item.run_snapshot.clone())
            .collect::<Vec<_>>();
        let hash = match hash_json(&dataset_runs) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("failed to hash benchmark payload: {error}");
                std::process::exit(1);
            }
        };
        run_snapshots.push(BenchmarkRunSnapshot { hash, dataset_runs });

        if selected_payload.is_none() {
            selected_payload = Some(provisional);
        }
    }

    let consistency = compute_consistency(&run_snapshots);
    let mut payload = match selected_payload {
        Some(value) => value,
        None => {
            eprintln!("no benchmark run payload generated");
            std::process::exit(1);
        }
    };
    payload.consistency = consistency.clone();
    println!("Guess Accuracy Benchmark");
    println!("Metric definitions:");
    println!("  evaluated_items: {}", payload.definitions.evaluated_items);
    println!("  found_items: {}", payload.definitions.found_items);
    println!("  recall_at_1: {}", payload.definitions.recall_at_1);
    println!("  recall_at_5: {}", payload.definitions.recall_at_5);
    println!("  recall_at_20: {}", payload.definitions.recall_at_20);
    println!("  mrr: {}", payload.definitions.mrr);
    println!("  mean_rank_found: {}", payload.definitions.mean_rank_found);
    println!("  best_rank: {}", payload.definitions.best_rank);
    println!(
        "  visual_rows_total: {}",
        payload.definitions.visual_rows_total
    );
    println!(
        "  visual_rows_with_top_guess: {}",
        payload.definitions.visual_rows_with_top_guess
    );
    println!(
        "  visual_rows_scored: {}",
        payload.definitions.visual_rows_scored
    );
    println!(
        "  visual_rows_dropped: {}",
        payload.definitions.visual_rows_dropped
    );
    println!(
        "  visual_mean_abs_diff: {}",
        payload.definitions.visual_mean_abs_diff
    );
    println!(
        "  visual_median_abs_diff: {}",
        payload.definitions.visual_median_abs_diff
    );
    println!(
        "  visual_p90_abs_diff: {}",
        payload.definitions.visual_p90_abs_diff
    );
    println!(
        "  visual_mean_changed_pixel_ratio: {}",
        payload.definitions.visual_mean_changed_pixel_ratio
    );
    println!(
        "  visual_mean_compared_pixels: {}",
        payload.definitions.visual_mean_compared_pixels
    );
    println!(
        "  visual_rerank_rows_considered: {}",
        payload.definitions.visual_rerank_rows_considered
    );
    println!(
        "  visual_rerank_rows_scored: {}",
        payload.definitions.visual_rerank_rows_scored
    );
    println!(
        "  visual_rerank_top1_changed: {}",
        payload.definitions.visual_rerank_top1_changed
    );
    println!(
        "  visual_rerank_top1_changed_ratio: {}",
        payload.definitions.visual_rerank_top1_changed_ratio
    );
    println!(
        "  visual_rerank_mean_gain: {}",
        payload.definitions.visual_rerank_mean_gain
    );
    println!(
        "  timing_redactions_ms: {}",
        payload.definitions.timing_redactions_ms
    );
    println!("  timing_fonts_ms: {}", payload.definitions.timing_fonts_ms);
    println!("  timing_guess_ms: {}", payload.definitions.timing_guess_ms);
    println!(
        "  timing_visualize_ms: {}",
        payload.definitions.timing_visualize_ms
    );
    println!(
        "  timing_orchestrator_total_ms: {}",
        payload.definitions.timing_orchestrator_total_ms
    );
    println!(
        "  candidate_rows_total: {}",
        payload.definitions.candidate_rows_total
    );
    println!(
        "  candidate_rows_with_candidates: {}",
        payload.definitions.candidate_rows_with_candidates
    );
    println!(
        "  candidate_mean_count: {}",
        payload.definitions.candidate_mean_count
    );
    println!(
        "  candidate_median_count: {}",
        payload.definitions.candidate_median_count
    );
    println!(
        "  candidate_p90_count: {}",
        payload.definitions.candidate_p90_count
    );
    println!(
        "  candidate_multi_span_rows: {}",
        payload.definitions.candidate_multi_span_rows
    );
    println!(
        "  candidate_multi_span_mean_count: {}",
        payload.definitions.candidate_multi_span_mean_count
    );
    println!(
        "  candidate_multi_span_p90_count: {}",
        payload.definitions.candidate_multi_span_p90_count
    );
    println!(
        "  candidate_single_span_rows: {}",
        payload.definitions.candidate_single_span_rows
    );
    println!(
        "  candidate_single_span_mean_count: {}",
        payload.definitions.candidate_single_span_mean_count
    );
    println!(
        "  quality_rows_total: {}",
        payload.definitions.quality_rows_total
    );
    println!(
        "  quality_anchored_rows: {}",
        payload.definitions.quality_anchored_rows
    );
    println!(
        "  quality_anchor_two_sided_rows: {}",
        payload.definitions.quality_anchor_two_sided_rows
    );
    println!(
        "  quality_anchor_one_sided_rows: {}",
        payload.definitions.quality_anchor_one_sided_rows
    );
    println!(
        "  quality_width_asset_rows: {}",
        payload.definitions.quality_width_asset_rows
    );
    println!(
        "  quality_width_table_rows: {}",
        payload.definitions.quality_width_table_rows
    );
    println!(
        "  quality_width_core_rows: {}",
        payload.definitions.quality_width_core_rows
    );
    println!(
        "  quality_width_fallback_rows: {}",
        payload.definitions.quality_width_fallback_rows
    );
    println!(
        "  quality_width_fallback_reason_rows: {}",
        payload.definitions.quality_width_fallback_reason_rows
    );
    println!(
        "  consistency_repeats: {}",
        payload.definitions.consistency_repeats
    );
    println!(
        "  consistency_all_hashes_identical: {}",
        payload.definitions.consistency_all_hashes_identical
    );
    println!(
        "  consistency_hash_match_ratio: {}",
        payload.definitions.consistency_hash_match_ratio
    );
    println!(
        "  consistency_top1_agreement_ratio: {}",
        payload.definitions.consistency_top1_agreement_ratio
    );
    println!(
        "  consistency_top5_jaccard_mean: {}",
        payload.definitions.consistency_top5_jaccard_mean
    );
    println!(
        "  consistency_mean_rank_stddev: {}",
        payload.definitions.consistency_mean_rank_stddev
    );
    println!(
        "  consistency_unstable_rows_count: {}",
        payload.definitions.consistency_unstable_rows_count
    );
    println!(
        "  consistency_unstable_rows_ratio: {}",
        payload.definitions.consistency_unstable_rows_ratio
    );
    for dataset in &payload.datasets {
        print_summary(&dataset.name, &dataset.summary);
        print_visual_summary(&format!("{} visual", dataset.name), &dataset.visual_summary);
        print_visual_rerank_summary(
            &format!("{} rerank", dataset.name),
            &dataset.visual_rerank_summary,
        );
        print_timing_summary(&format!("{} timing", dataset.name), &dataset.timing_summary);
        print_candidate_summary(
            &format!("{} candidates", dataset.name),
            &dataset.candidate_summary,
        );
        print_quality_summary(
            &format!("{} quality", dataset.name),
            &dataset.quality_summary,
        );
    }
    print_summary("OVERALL", &payload.overall);
    print_visual_summary("OVERALL visual", &payload.overall_visual);
    print_visual_rerank_summary("OVERALL rerank", &payload.overall_visual_rerank);
    print_timing_summary("OVERALL timing", &payload.overall_timing);
    print_candidate_summary("OVERALL candidates", &payload.overall_candidates);
    print_quality_summary("OVERALL quality", &payload.overall_quality);
    println!(
        "CONSISTENCY      repeats={} hashes_identical={} hash_match={:.3} top1_agree={:.3} top5_jaccard={:.3} unstable_rows={} unstable_ratio={:.3}",
        payload.consistency.repeats,
        payload.consistency.all_hashes_identical,
        payload.consistency.hash_match_ratio,
        payload.consistency.top1_agreement_ratio,
        payload.consistency.top5_jaccard_mean,
        payload.consistency.unstable_rows_count,
        payload.consistency.unstable_rows_ratio
    );

    if let Some(parent) = options.out_path.parent() {
        if !parent.as_os_str().is_empty() {
            let create_result = std::fs::create_dir_all(parent);
            if let Err(error) = create_result {
                eprintln!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                );
                std::process::exit(1);
            }
        }
    }
    let encoded = match serde_json::to_vec_pretty(&payload) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("failed to encode benchmark json: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = std::fs::write(&options.out_path, encoded) {
        eprintln!("failed to write {}: {error}", options.out_path.display());
        std::process::exit(1);
    }
    println!("wrote {}", options.out_path.display());
}

use clap::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use unredact::service::tooling_entry::{
    collect_underlying_text_hits_by_page, load_dictionary_from_bytes, run_guess_from_redactions,
    ToolingGuessRequest,
};
use unredact::types::guess_types::{GuessConfig, RedactionGuess};
use unredact::types::redaction_types::{
    Rect, RedactionKind, RedactionOccurrence, RedactionReport, UnderlyingTextHit,
};

const PROTOCOL_ID: &str = "C-SYNTHETIC-SEED-TIERS";
const PROTOCOL_VERSION: usize = 1;
const SOURCE_POOL_POLICY: &str = "all_test_data_pdfs";
const FIXED_SEED_POLICY: &str = "binding";
const EXPLORATORY_SEED_POLICY: &str = "diagnostic_only";
const EXPLORATORY_SEED_STRATEGY: &str = "pseudo_random_from_fixed_base_seed";
const RUN_COMPLETENESS_POLICY: &str = "binding_full_coverage";
const REQUIRED_PROFILE_COMMAND: &str = "cargo run --bin synthetic_overfitting_benchmark --release";
const MULTI_SEED_PANEL_POLICY: &str = "binding_average_non_regression";

const FIXED_SEEDS: [u64; 3] = [12_345_u64, 424_242_u64, 98_765_u64];
const FIXED_RUNS_PER_SEED: usize = 2;
const EXPLORATORY_SEED_COUNT: usize = 20;
const EXPLORATORY_SEED_BASE: u64 = 0x5EED_5EED_1337_1337_u64;
const MULTI_SEED_PANEL_MIN_COUNT: usize = 20;
const MULTI_SEED_PANEL_R20_TOLERANCE: f64 = 0.01_f64;
const MULTI_SEED_PANEL_MRR_TOLERANCE: f64 = 0.002_f64;
const HARD_THRESHOLD_MARGIN_RATIO: f64 = 0.01_f64;

const SAME_LINE_DELTA_PT: f32 = 3.0_f32;
const MIN_TARGET_WORD_LEN: usize = 4;
const MAX_TARGET_WORD_LEN: usize = 20;
const TARGET_OVERLAP_RATIO_MIN: f32 = 0.20_f32;
const MAX_TARGETS_PER_FILE: usize = 40;
const MAX_REPORTED_MISMATCHES: usize = 128;

#[derive(Debug, Parser)]
#[command(
    name = "synthetic_overfitting_benchmark",
    about = "Evaluate synthetic anti-overfitting performance across all test_data PDFs."
)]
struct CliOptions {
    #[arg(
        long,
        default_value = "test_data",
        help = "Root directory containing PDFs to include in the synthetic source pool."
    )]
    root: PathBuf,
    #[arg(
        long,
        default_value_t = 8_usize,
        help = "Synthetic targets sampled per PDF (clamped to available candidates)."
    )]
    targets_per_file: usize,
    #[arg(
        long,
        default_value_t = 8_000_usize,
        help = "Maximum synthetic dictionary size."
    )]
    dictionary_size: usize,
    #[arg(
        long,
        default_value = "benchmark/synthetic_overfitting_evaluation.json",
        help = "Output path for protocol report."
    )]
    out: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RankSummary {
    evaluated_items: usize,
    found_items: usize,
    recall_at_1: f64,
    recall_at_5: f64,
    recall_at_20: f64,
    mrr: f64,
    mean_rank_found: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct ProtocolConfiguration {
    protocol_id: String,
    version: usize,
    source_pool_policy: String,
    fixed_seed_policy: String,
    fixed_seeds: Vec<u64>,
    fixed_runs_per_seed: usize,
    exploratory_seed_policy: String,
    exploratory_seed_strategy: String,
    exploratory_seed_count: usize,
    exploratory_seed_base: u64,
}

#[derive(Debug, Clone, Serialize)]
struct SourcePoolInventory {
    policy: String,
    root: String,
    discovered_pdf_count: usize,
    discovered_pdfs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FileEvaluationReport {
    pdf_path: String,
    candidate_pool_size: usize,
    sampled_targets: usize,
    skipped: bool,
    skip_reason: Option<String>,
    summary: Option<RankSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct SeedEvaluationReport {
    seed: u64,
    evaluated_files: usize,
    skipped_files: usize,
    sampled_targets_total: usize,
    summary: RankSummary,
    comparator_hash: String,
    files: Vec<FileEvaluationReport>,
}

#[derive(Debug, Clone, Serialize)]
struct FixedSeedRunReport {
    seed: u64,
    run_hashes: Vec<String>,
    baseline: SeedEvaluationReport,
}

#[derive(Debug, Clone, Serialize)]
struct DeterminismMismatch {
    class: String,
    seed: u64,
    run_index: usize,
    pdf_path: Option<String>,
    target: Option<String>,
    baseline: Option<String>,
    observed: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FixedSeedGateSection {
    policy: String,
    fixed_seeds: Vec<u64>,
    runs_per_seed: usize,
    passed: bool,
    mismatch_count: usize,
    mismatches: Vec<DeterminismMismatch>,
    seed_reports: Vec<FixedSeedRunReport>,
}

#[derive(Debug, Clone, Serialize)]
struct ExploratorySeedDiagnosticsSection {
    policy: String,
    strategy: String,
    seeds: Vec<u64>,
    summary: RankSummary,
    seed_reports: Vec<SeedEvaluationReport>,
}

#[derive(Debug, Clone, Serialize)]
struct MultiSeedPanelSection {
    policy: String,
    seed_count: usize,
    required_min_seed_count: usize,
    tolerance_recall_at_20: f64,
    tolerance_mrr: f64,
    margin_ratio: f64,
    baseline_summary: RankSummary,
    current_summary: RankSummary,
    delta_recall_at_20: f64,
    delta_mrr: f64,
    allowed_negative_delta_recall_at_20: f64,
    allowed_negative_delta_mrr: f64,
    passed_seed_count: bool,
    passed_thresholds: bool,
    passed: bool,
    failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiSeedPanelBaseline {
    summary: RankSummary,
}

#[derive(Debug, Clone, Serialize)]
struct SyntheticOverfittingReport {
    protocol: ProtocolConfiguration,
    source_pool: SourcePoolInventory,
    run_completeness_gate: RunCompletenessGateSection,
    fixed_seed_gate: FixedSeedGateSection,
    exploratory_seed_diagnostics: ExploratorySeedDiagnosticsSection,
    multi_seed_panel: MultiSeedPanelSection,
}

#[derive(Debug, Clone, Serialize)]
struct RunCompletenessGateSection {
    policy: String,
    required_profile: String,
    debug_build_detected: bool,
    discovered_pdf_count: usize,
    expected_seed_runs: usize,
    observed_seed_runs: usize,
    expected_seed_file_evaluations: usize,
    observed_seed_file_evaluations: usize,
    skipped_seed_file_evaluations: usize,
    failed_seeds: Vec<u64>,
    failures: Vec<String>,
    passed: bool,
}

#[derive(Debug, Clone)]
struct SeedSnapshot {
    report: SeedEvaluationReport,
    rows: Vec<FileSeedRows>,
    completeness_failure: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FileSeedRows {
    pdf_path: String,
    target_rows: Vec<TargetRow>,
}

#[derive(Debug, Clone, Serialize)]
struct TargetRow {
    target: String,
    best_rank: Option<usize>,
    top1: Option<String>,
}

#[derive(Debug, Clone)]
struct TargetCandidate {
    page_index: u32,
    text: String,
    bbox: Rect,
    left_context: UnderlyingTextHit,
    right_context: UnderlyingTextHit,
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

fn main() {
    let options = CliOptions::parse();
    if let Err(error) = run(options) {
        eprintln!("synthetic overfitting benchmark failed: {error}");
        std::process::exit(1);
    }
}

fn protocol_configuration() -> ProtocolConfiguration {
    ProtocolConfiguration {
        protocol_id: PROTOCOL_ID.to_owned(),
        version: PROTOCOL_VERSION,
        source_pool_policy: SOURCE_POOL_POLICY.to_owned(),
        fixed_seed_policy: FIXED_SEED_POLICY.to_owned(),
        fixed_seeds: FIXED_SEEDS.into_iter().collect::<Vec<_>>(),
        fixed_runs_per_seed: FIXED_RUNS_PER_SEED,
        exploratory_seed_policy: EXPLORATORY_SEED_POLICY.to_owned(),
        exploratory_seed_strategy: EXPLORATORY_SEED_STRATEGY.to_owned(),
        exploratory_seed_count: EXPLORATORY_SEED_COUNT,
        exploratory_seed_base: EXPLORATORY_SEED_BASE,
    }
}

fn hard_gate_passed(report: &SyntheticOverfittingReport) -> bool {
    report.run_completeness_gate.passed
        && report.fixed_seed_gate.passed
        && report.multi_seed_panel.passed
}

fn default_panel_baseline_out_path(out_path: &Path) -> PathBuf {
    let parent = out_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = out_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("synthetic_overfitting_evaluation");
    parent.join(format!("{stem}.baseline.json"))
}

fn load_panel_baseline(path: &Path) -> Result<Option<MultiSeedPanelBaseline>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read panel baseline {}: {error}", path.display()))?;
    let parsed = serde_json::from_slice::<MultiSeedPanelBaseline>(&bytes)
        .map_err(|error| format!("failed to parse panel baseline {}: {error}", path.display()))?;
    Ok(Some(parsed))
}

fn write_panel_baseline(path: &Path, summary: &RankSummary) -> Result<(), String> {
    let snapshot = MultiSeedPanelBaseline {
        summary: summary.clone(),
    };
    write_json(path, &snapshot)
}

fn evaluate_multi_seed_panel(
    current_summary: &RankSummary,
    seed_count: usize,
    baseline_summary: &RankSummary,
) -> MultiSeedPanelSection {
    let allowed_negative_delta_recall_at_20 =
        MULTI_SEED_PANEL_R20_TOLERANCE * (1.0_f64 + HARD_THRESHOLD_MARGIN_RATIO);
    let allowed_negative_delta_mrr =
        MULTI_SEED_PANEL_MRR_TOLERANCE * (1.0_f64 + HARD_THRESHOLD_MARGIN_RATIO);
    let delta_recall_at_20 = current_summary.recall_at_20 - baseline_summary.recall_at_20;
    let delta_mrr = current_summary.mrr - baseline_summary.mrr;
    let passed_seed_count = seed_count >= MULTI_SEED_PANEL_MIN_COUNT;
    let passed_thresholds = delta_recall_at_20 >= -allowed_negative_delta_recall_at_20
        && delta_mrr >= -allowed_negative_delta_mrr;
    let mut failures = Vec::<String>::new();
    if !passed_seed_count {
        failures.push(format!(
            "seed_count_below_min observed={} required={}",
            seed_count, MULTI_SEED_PANEL_MIN_COUNT
        ));
    }
    if delta_recall_at_20 < -allowed_negative_delta_recall_at_20 {
        failures.push(format!(
            "recall_at_20_regression delta={delta_recall_at_20:.6} allowed_negative_delta={allowed_negative_delta_recall_at_20:.6}"
        ));
    }
    if delta_mrr < -allowed_negative_delta_mrr {
        failures.push(format!(
            "mrr_regression delta={delta_mrr:.6} allowed_negative_delta={allowed_negative_delta_mrr:.6}"
        ));
    }
    MultiSeedPanelSection {
        policy: MULTI_SEED_PANEL_POLICY.to_owned(),
        seed_count,
        required_min_seed_count: MULTI_SEED_PANEL_MIN_COUNT,
        tolerance_recall_at_20: MULTI_SEED_PANEL_R20_TOLERANCE,
        tolerance_mrr: MULTI_SEED_PANEL_MRR_TOLERANCE,
        margin_ratio: HARD_THRESHOLD_MARGIN_RATIO,
        baseline_summary: baseline_summary.clone(),
        current_summary: current_summary.clone(),
        delta_recall_at_20,
        delta_mrr,
        allowed_negative_delta_recall_at_20,
        allowed_negative_delta_mrr,
        passed_seed_count,
        passed_thresholds,
        passed: passed_seed_count && passed_thresholds,
        failures,
    }
}

fn require_release_build() -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err(format!(
            "release build is required for benchmark gate; run `{REQUIRED_PROFILE_COMMAND}`"
        ));
    }
    Ok(())
}

fn build_run_completeness_gate(
    discovered_pdf_count: usize,
    expected_seed_runs: usize,
    snapshots: &[SeedSnapshot],
) -> RunCompletenessGateSection {
    let observed_seed_runs = snapshots.len();
    let observed_seed_file_evaluations = snapshots
        .iter()
        .map(|snapshot| snapshot.report.files.len())
        .sum::<usize>();
    let skipped_seed_file_evaluations = snapshots
        .iter()
        .map(|snapshot| snapshot.report.skipped_files)
        .sum::<usize>();
    let expected_seed_file_evaluations = discovered_pdf_count.saturating_mul(expected_seed_runs);
    let mut failures = Vec::<String>::new();
    if observed_seed_runs != expected_seed_runs {
        failures.push(format!(
            "seed_run_count_mismatch expected={} observed={}",
            expected_seed_runs, observed_seed_runs
        ));
    }
    if observed_seed_file_evaluations != expected_seed_file_evaluations {
        failures.push(format!(
            "seed_file_evaluation_count_mismatch expected={} observed={}",
            expected_seed_file_evaluations, observed_seed_file_evaluations
        ));
    }
    if skipped_seed_file_evaluations > 0 {
        failures.push(format!(
            "skipped_seed_file_evaluations={} (expected 0)",
            skipped_seed_file_evaluations
        ));
    }
    let mut failed_seeds = snapshots
        .iter()
        .filter_map(|snapshot| {
            snapshot
                .completeness_failure
                .as_ref()
                .map(|_| snapshot.report.seed)
        })
        .collect::<Vec<_>>();
    failed_seeds.sort_unstable();
    failed_seeds.dedup();
    failures.extend(
        snapshots
            .iter()
            .filter_map(|snapshot| {
                snapshot
                    .completeness_failure
                    .as_ref()
                    .map(|failure| format!("seed={} {failure}", snapshot.report.seed))
            })
            .collect::<Vec<_>>(),
    );
    let debug_build_detected = cfg!(debug_assertions);
    if debug_build_detected {
        failures.push(format!(
            "debug build detected; required command: {REQUIRED_PROFILE_COMMAND}"
        ));
    }
    RunCompletenessGateSection {
        policy: RUN_COMPLETENESS_POLICY.to_owned(),
        required_profile: REQUIRED_PROFILE_COMMAND.to_owned(),
        debug_build_detected,
        discovered_pdf_count,
        expected_seed_runs,
        observed_seed_runs,
        expected_seed_file_evaluations,
        observed_seed_file_evaluations,
        skipped_seed_file_evaluations,
        failed_seeds,
        passed: failures.is_empty(),
        failures,
    }
}

fn run(options: CliOptions) -> Result<(), String> {
    require_release_build()?;
    let protocol = protocol_configuration();
    if !options.root.exists() {
        return Err(format!(
            "missing source pool root {}",
            options.root.display()
        ));
    }
    let pdfs = discover_pdfs(&options.root)?;
    if pdfs.is_empty() {
        return Err(format!(
            "no PDF files discovered under source pool root {}",
            options.root.display()
        ));
    }
    let inventory = SourcePoolInventory {
        policy: protocol.source_pool_policy.clone(),
        root: options.root.display().to_string(),
        discovered_pdf_count: pdfs.len(),
        discovered_pdfs: pdfs
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
    };

    let mut fixed_seed_reports = Vec::<FixedSeedRunReport>::new();
    let mut mismatch_count = 0_usize;
    let mut mismatches = Vec::<DeterminismMismatch>::new();
    let mut executed_seed_snapshots = Vec::<SeedSnapshot>::new();
    for seed in &protocol.fixed_seeds {
        let runs = (0..protocol.fixed_runs_per_seed)
            .map(|_| {
                evaluate_seed(
                    &pdfs,
                    *seed,
                    options.targets_per_file,
                    options.dictionary_size,
                )
            })
            .collect::<Result<Vec<_>, String>>()?;
        let run_hashes = runs
            .iter()
            .map(|run| run.report.comparator_hash.clone())
            .collect::<Vec<_>>();
        compare_seed_runs(*seed, &runs, &mut mismatch_count, &mut mismatches);
        executed_seed_snapshots.extend(runs.iter().cloned());
        fixed_seed_reports.push(FixedSeedRunReport {
            seed: *seed,
            run_hashes,
            baseline: runs[0].report.clone(),
        });
    }

    let mut exploratory_rng = LcgRng::new(protocol.exploratory_seed_base);
    let exploratory_seeds = (0..protocol.exploratory_seed_count)
        .map(|_| exploratory_rng.next_u64())
        .collect::<Vec<_>>();
    let exploratory_snapshots = exploratory_seeds
        .iter()
        .map(|seed| {
            evaluate_seed(
                &pdfs,
                *seed,
                options.targets_per_file,
                options.dictionary_size,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    executed_seed_snapshots.extend(exploratory_snapshots.iter().cloned());
    let exploratory_reports = exploratory_snapshots
        .iter()
        .map(|snapshot| snapshot.report.clone())
        .collect::<Vec<_>>();
    let exploratory_summary = summarize_ranks(
        &exploratory_snapshots
            .iter()
            .flat_map(|snapshot| {
                snapshot
                    .rows
                    .iter()
                    .flat_map(|file| file.target_rows.iter().map(|row| row.best_rank))
            })
            .collect::<Vec<_>>(),
    );

    let fixed_seed_policy = protocol.fixed_seed_policy.clone();
    let fixed_seeds = protocol.fixed_seeds.clone();
    let fixed_runs_per_seed = protocol.fixed_runs_per_seed;
    let exploratory_seed_policy = protocol.exploratory_seed_policy.clone();
    let exploratory_seed_strategy = protocol.exploratory_seed_strategy.clone();
    let panel_baseline_path = default_panel_baseline_out_path(&options.out);
    let panel_baseline = load_panel_baseline(&panel_baseline_path)?;
    let baseline_summary = panel_baseline
        .as_ref()
        .map(|snapshot| snapshot.summary.clone())
        .unwrap_or_else(|| exploratory_summary.clone());
    let multi_seed_panel = evaluate_multi_seed_panel(
        &exploratory_summary,
        exploratory_seeds.len(),
        &baseline_summary,
    );
    let run_completeness_gate = build_run_completeness_gate(
        pdfs.len(),
        protocol
            .fixed_seeds
            .len()
            .saturating_mul(protocol.fixed_runs_per_seed)
            .saturating_add(protocol.exploratory_seed_count),
        &executed_seed_snapshots,
    );
    let report = SyntheticOverfittingReport {
        protocol,
        source_pool: inventory,
        run_completeness_gate,
        fixed_seed_gate: FixedSeedGateSection {
            policy: fixed_seed_policy,
            fixed_seeds,
            runs_per_seed: fixed_runs_per_seed,
            passed: mismatch_count == 0,
            mismatch_count,
            mismatches,
            seed_reports: fixed_seed_reports,
        },
        exploratory_seed_diagnostics: ExploratorySeedDiagnosticsSection {
            policy: exploratory_seed_policy,
            strategy: exploratory_seed_strategy,
            seeds: exploratory_seeds,
            summary: exploratory_summary,
            seed_reports: exploratory_reports,
        },
        multi_seed_panel,
    };

    print_report_summary(&report);
    write_json(&options.out, &report)?;
    println!("wrote {}", options.out.display());
    if panel_baseline.is_none() {
        write_panel_baseline(
            &panel_baseline_path,
            &report.multi_seed_panel.current_summary,
        )?;
        println!(
            "panel_baseline_bootstrapped_path={}",
            panel_baseline_path.display()
        );
    }
    if !hard_gate_passed(&report) {
        return Err(format_hard_gate_failure(&report));
    }
    Ok(())
}

fn format_hard_gate_failure(report: &SyntheticOverfittingReport) -> String {
    let mut failures = Vec::<String>::new();
    if !report.run_completeness_gate.passed {
        failures.push(format!(
            "run_completeness_gate_failed policy={} failures={}",
            report.run_completeness_gate.policy,
            report.run_completeness_gate.failures.join(" | ")
        ));
    }
    if !report.fixed_seed_gate.passed {
        failures.push(format!(
            "fixed_seed_gate_failed mismatches={}",
            report.fixed_seed_gate.mismatch_count
        ));
    }
    if !report.multi_seed_panel.passed {
        failures.push(format!(
            "multi_seed_panel_failed policy={} failures={}",
            report.multi_seed_panel.policy,
            report.multi_seed_panel.failures.join(" | ")
        ));
    }
    if failures.is_empty() {
        return "unknown hard gate failure".to_owned();
    }
    failures.join(" ; ")
}

fn discover_pdfs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::<PathBuf>::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|error| format!("failed to read directory {}: {error}", dir.display()))?;
        let mut paths = entries
            .map(|entry| {
                entry.map(|value| value.path()).map_err(|error| {
                    format!("failed to read entry under {}: {error}", dir.display())
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        paths.sort();
        for path in paths {
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let is_pdf = path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false);
            if is_pdf {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn evaluate_seed(
    pdfs: &[PathBuf],
    seed: u64,
    targets_per_file: usize,
    dictionary_size: usize,
) -> Result<SeedSnapshot, String> {
    let mut file_reports = Vec::<FileEvaluationReport>::new();
    let mut file_rows = Vec::<FileSeedRows>::new();
    let mut all_ranks = Vec::<Option<usize>>::new();

    for pdf in pdfs {
        let pdf_bytes = std::fs::read(pdf)
            .map_err(|error| format!("failed to read {}: {error}", pdf.display()))?;
        let hits_by_page = collect_underlying_text_hits_by_page(&pdf_bytes)?;
        let mut candidates = Vec::<TargetCandidate>::new();
        for (page_index, page_hits) in &hits_by_page {
            let word_hits = extract_word_hits(*page_index, page_hits);
            candidates.extend(build_target_candidates(*page_index, &word_hits));
        }

        if candidates.is_empty() {
            file_reports.push(FileEvaluationReport {
                pdf_path: pdf.display().to_string(),
                candidate_pool_size: 0,
                sampled_targets: 0,
                skipped: true,
                skip_reason: Some("no_synthetic_candidates".to_owned()),
                summary: None,
            });
            continue;
        }

        let file_seed = seed ^ stable_string_hash(&pdf.display().to_string());
        let mut rng = LcgRng::new(file_seed);
        let target_count = targets_per_file.clamp(1, MAX_TARGETS_PER_FILE);
        let sampled = sample_targets(&candidates, target_count, &mut rng);
        if sampled.is_empty() {
            file_reports.push(FileEvaluationReport {
                pdf_path: pdf.display().to_string(),
                candidate_pool_size: candidates.len(),
                sampled_targets: 0,
                skipped: true,
                skip_reason: Some("sampling_exhausted".to_owned()),
                summary: None,
            });
            continue;
        }

        let dictionary = build_dictionary(&sampled, &hits_by_page, dictionary_size, file_seed);
        let dictionary_raw = dictionary.join("\n");
        let dictionary_inputs = load_dictionary_from_bytes(Some(dictionary_raw.as_bytes()))?;
        let redaction_report = build_synthetic_redaction_report(pdf, &sampled);
        let guess_report = run_guess_from_redactions(ToolingGuessRequest {
            input_name: &pdf.to_string_lossy(),
            pdf_bytes: &pdf_bytes,
            redactions: &redaction_report,
            dictionary: &dictionary_inputs.dictionary,
            dictionary_diagnostics: &dictionary_inputs.diagnostics,
            guess: &GuessConfig {
                visual_score: true,
                visual_score_dpi: 200.0_f32,
            },
        })?;

        let target_rows = sampled
            .iter()
            .map(|target| {
                let (best_rank, top1) = evaluate_target(&guess_report.guesses, target);
                all_ranks.push(best_rank);
                TargetRow {
                    target: target.text.clone(),
                    best_rank,
                    top1,
                }
            })
            .collect::<Vec<_>>();
        let summary = summarize_ranks(
            &target_rows
                .iter()
                .map(|row| row.best_rank)
                .collect::<Vec<_>>(),
        );
        file_reports.push(FileEvaluationReport {
            pdf_path: pdf.display().to_string(),
            candidate_pool_size: candidates.len(),
            sampled_targets: target_rows.len(),
            skipped: false,
            skip_reason: None,
            summary: Some(summary),
        });
        file_rows.push(FileSeedRows {
            pdf_path: pdf.display().to_string(),
            target_rows,
        });
    }

    file_reports.sort_by(|left, right| left.pdf_path.cmp(&right.pdf_path));
    file_rows.sort_by(|left, right| left.pdf_path.cmp(&right.pdf_path));
    let summary = summarize_ranks(&all_ranks);
    let evaluated_files = file_reports.iter().filter(|file| !file.skipped).count();
    let skipped_files = file_reports.len().saturating_sub(evaluated_files);
    let sampled_targets_total = file_reports
        .iter()
        .map(|file| file.sampled_targets)
        .sum::<usize>();

    let report = SeedEvaluationReport {
        seed,
        evaluated_files,
        skipped_files,
        sampled_targets_total,
        summary,
        comparator_hash: hash_json(&file_rows)?,
        files: file_reports,
    };
    let mut seed_failures = Vec::<String>::new();
    if report.files.len() != pdfs.len() {
        seed_failures.push(format!(
            "pdf_count_mismatch expected={} observed={}",
            pdfs.len(),
            report.files.len()
        ));
    }
    if report.skipped_files > 0 {
        seed_failures.push(format!(
            "skipped_files={} (expected 0)",
            report.skipped_files
        ));
    }
    if report.evaluated_files != pdfs.len() {
        seed_failures.push(format!(
            "evaluated_files_mismatch expected={} observed={}",
            pdfs.len(),
            report.evaluated_files
        ));
    }
    Ok(SeedSnapshot {
        report,
        rows: file_rows,
        completeness_failure: if seed_failures.is_empty() {
            None
        } else {
            Some(seed_failures.join(" | "))
        },
    })
}

fn compare_seed_runs(
    seed: u64,
    runs: &[SeedSnapshot],
    mismatch_count: &mut usize,
    mismatches: &mut Vec<DeterminismMismatch>,
) {
    if runs.is_empty() {
        return;
    }
    let baseline = &runs[0];
    for (run_index, observed) in runs.iter().enumerate().skip(1) {
        if observed.report.comparator_hash != baseline.report.comparator_hash {
            record_mismatch(
                mismatch_count,
                mismatches,
                DeterminismMismatch {
                    class: "seed_hash_mismatch".to_owned(),
                    seed,
                    run_index,
                    pdf_path: None,
                    target: None,
                    baseline: Some(baseline.report.comparator_hash.clone()),
                    observed: Some(observed.report.comparator_hash.clone()),
                },
            );
        }
        let baseline_files = baseline
            .rows
            .iter()
            .map(|file| (file.pdf_path.clone(), file))
            .collect::<BTreeMap<_, _>>();
        let observed_files = observed
            .rows
            .iter()
            .map(|file| (file.pdf_path.clone(), file))
            .collect::<BTreeMap<_, _>>();
        for (pdf_path, baseline_rows) in &baseline_files {
            let Some(observed_rows) = observed_files.get(pdf_path) else {
                record_mismatch(
                    mismatch_count,
                    mismatches,
                    DeterminismMismatch {
                        class: "file_missing".to_owned(),
                        seed,
                        run_index,
                        pdf_path: Some(pdf_path.clone()),
                        target: None,
                        baseline: Some("present".to_owned()),
                        observed: Some("missing".to_owned()),
                    },
                );
                continue;
            };
            let baseline_targets = baseline_rows
                .target_rows
                .iter()
                .map(|row| (row.target.clone(), row))
                .collect::<BTreeMap<_, _>>();
            let observed_targets = observed_rows
                .target_rows
                .iter()
                .map(|row| (row.target.clone(), row))
                .collect::<BTreeMap<_, _>>();
            for (target, baseline_row) in &baseline_targets {
                let Some(observed_row) = observed_targets.get(target) else {
                    record_mismatch(
                        mismatch_count,
                        mismatches,
                        DeterminismMismatch {
                            class: "target_missing".to_owned(),
                            seed,
                            run_index,
                            pdf_path: Some(pdf_path.clone()),
                            target: Some(target.clone()),
                            baseline: Some("present".to_owned()),
                            observed: Some("missing".to_owned()),
                        },
                    );
                    continue;
                };
                if observed_row.best_rank != baseline_row.best_rank {
                    record_mismatch(
                        mismatch_count,
                        mismatches,
                        DeterminismMismatch {
                            class: "rank_set_divergence".to_owned(),
                            seed,
                            run_index,
                            pdf_path: Some(pdf_path.clone()),
                            target: Some(target.clone()),
                            baseline: Some(format_rank(baseline_row.best_rank)),
                            observed: Some(format_rank(observed_row.best_rank)),
                        },
                    );
                }
                if observed_row.top1 != baseline_row.top1 {
                    record_mismatch(
                        mismatch_count,
                        mismatches,
                        DeterminismMismatch {
                            class: "top1_divergence".to_owned(),
                            seed,
                            run_index,
                            pdf_path: Some(pdf_path.clone()),
                            target: Some(target.clone()),
                            baseline: baseline_row.top1.clone(),
                            observed: observed_row.top1.clone(),
                        },
                    );
                }
            }
        }
    }
}

fn record_mismatch(
    mismatch_count: &mut usize,
    mismatches: &mut Vec<DeterminismMismatch>,
    mismatch: DeterminismMismatch,
) {
    *mismatch_count += 1;
    if mismatches.len() >= MAX_REPORTED_MISMATCHES {
        return;
    }
    mismatches.push(mismatch);
}

fn format_rank(rank: Option<usize>) -> String {
    rank.map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_owned())
}

fn stable_string_hash(value: &str) -> u64 {
    let mut hash = 14695981039346656037_u64;
    for byte in value.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211_u64);
    }
    hash
}

fn hash_json<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                )
            })?;
        }
    }
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode report json: {error}"))?;
    std::fs::write(path, encoded)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn center_y(rect: &Rect) -> f32 {
    (rect.y0 + rect.y1) * 0.5_f32
}

fn extract_word_hits(page_index: u32, hits: &[UnderlyingTextHit]) -> Vec<UnderlyingTextHit> {
    let mut out = Vec::<UnderlyingTextHit>::new();
    for hit in hits {
        let full_text = hit.text.trim();
        if full_text.is_empty() {
            continue;
        }
        let total_chars = full_text.chars().count().max(1) as f32;
        let width = hit.bbox.width().abs().max(0.0001_f32);
        let tokens = tokenize_word_ranges(full_text);
        for (token, start_char, end_char) in tokens {
            if canonical_word(&token).is_none() {
                continue;
            }
            let start_ratio = (start_char as f32 / total_chars).clamp(0.0_f32, 1.0_f32);
            let end_ratio = (end_char as f32 / total_chars).clamp(0.0_f32, 1.0_f32);
            let x0 = hit.bbox.x0 + width * start_ratio;
            let x1 = hit.bbox.x0 + width * end_ratio;
            if x1 <= x0 {
                continue;
            }
            out.push(UnderlyingTextHit {
                page_index,
                bbox: Rect::new(x0, hit.bbox.y0, x1, hit.bbox.y1),
                text: token,
            });
        }
    }
    out
}

fn tokenize_word_ranges(text: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::<(String, usize, usize)>::new();
    let mut token = String::new();
    let mut start = 0_usize;
    for (idx, ch) in text.chars().enumerate() {
        if ch.is_ascii_alphabetic() || ch == '\'' || ch == '-' {
            if token.is_empty() {
                start = idx;
            }
            token.push(ch);
            continue;
        }
        if token.is_empty() {
            continue;
        }
        out.push((token.clone(), start, idx));
        token.clear();
    }
    if !token.is_empty() {
        out.push((token, start, text.chars().count()));
    }
    out
}

fn canonical_word(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_alphabetic() || ch == '\'' || ch == '-' {
            out.push(ch);
        }
    }
    if out.is_empty() {
        return None;
    }
    let alpha_len = out.chars().filter(|ch| ch.is_ascii_alphabetic()).count();
    if !(MIN_TARGET_WORD_LEN..=MAX_TARGET_WORD_LEN).contains(&alpha_len) {
        return None;
    }
    Some(out.to_ascii_uppercase())
}

fn build_target_candidates(page_index: u32, hits: &[UnderlyingTextHit]) -> Vec<TargetCandidate> {
    let mut ordered = hits.to_vec();
    ordered.sort_by(|left, right| {
        center_y(&left.bbox)
            .partial_cmp(&center_y(&right.bbox))
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| left.text.cmp(&right.text))
    });
    let mut frequencies = BTreeMap::<String, usize>::new();
    for hit in &ordered {
        if let Some(text) = canonical_word(&hit.text) {
            *frequencies.entry(text).or_insert(0_usize) += 1_usize;
        }
    }

    let mut out = Vec::<TargetCandidate>::new();
    for idx in 1..ordered.len().saturating_sub(1) {
        let hit = &ordered[idx];
        let Some(text) = canonical_word(&hit.text) else {
            continue;
        };
        if frequencies.get(&text).copied().unwrap_or(0) != 1 {
            continue;
        }
        let left = &ordered[idx - 1];
        let right = &ordered[idx + 1];
        if canonical_word(&left.text).is_none() || canonical_word(&right.text).is_none() {
            continue;
        }
        if (center_y(&left.bbox) - center_y(&hit.bbox)).abs() > SAME_LINE_DELTA_PT {
            continue;
        }
        if (center_y(&right.bbox) - center_y(&hit.bbox)).abs() > SAME_LINE_DELTA_PT {
            continue;
        }
        if left.bbox.x1 > hit.bbox.x0 || right.bbox.x0 < hit.bbox.x1 {
            continue;
        }
        let width = hit.bbox.width().abs();
        let height = hit.bbox.height().abs();
        if width < 6.0_f32 || height < 5.0_f32 {
            continue;
        }
        out.push(TargetCandidate {
            page_index,
            text,
            bbox: hit.bbox,
            left_context: left.clone(),
            right_context: right.clone(),
        });
    }
    out
}

fn build_dictionary(
    candidates: &[TargetCandidate],
    hits_by_page: &BTreeMap<u32, Vec<UnderlyingTextHit>>,
    dictionary_size: usize,
    seed: u64,
) -> Vec<String> {
    let mut set = BTreeSet::<String>::new();
    for candidate in candidates {
        set.insert(candidate.text.clone());
    }
    for hits in hits_by_page.values() {
        for hit in hits {
            if let Some(word) = canonical_word(&hit.text) {
                set.insert(word);
                if set.len() >= dictionary_size {
                    return set.into_iter().collect::<Vec<_>>();
                }
            }
        }
    }
    let mut rng = LcgRng::new(seed ^ 0xA5A5_5A5A_1234_4321_u64);
    while set.len() < dictionary_size {
        let mut token = String::new();
        for _ in 0..8_usize {
            token.push((b'A' + (rng.next_u64() % 26_u64) as u8) as char);
        }
        set.insert(token);
    }
    set.into_iter().collect::<Vec<_>>()
}

fn sample_targets(
    pool: &[TargetCandidate],
    desired: usize,
    rng: &mut LcgRng,
) -> Vec<TargetCandidate> {
    let mut indices = (0..pool.len()).collect::<Vec<_>>();
    rng.shuffle(&mut indices);
    let mut selected = Vec::<TargetCandidate>::new();
    let mut used_text = BTreeSet::<String>::new();
    for idx in indices {
        let candidate = &pool[idx];
        if !used_text.insert(candidate.text.clone()) {
            continue;
        }
        if selected
            .iter()
            .any(|existing| rects_overlap(existing.bbox, candidate.bbox))
        {
            continue;
        }
        selected.push(candidate.clone());
        if selected.len() >= desired {
            break;
        }
    }
    selected
}

fn rects_overlap(left: Rect, right: Rect) -> bool {
    let overlap_w = (left.x1.min(right.x1) - left.x0.max(right.x0)).max(0.0_f32);
    let overlap_h = (left.y1.min(right.y1) - left.y0.max(right.y0)).max(0.0_f32);
    overlap_w > 0.0_f32 && overlap_h > 0.0_f32
}

fn build_synthetic_redaction_report(input: &Path, targets: &[TargetCandidate]) -> RedactionReport {
    let mut redactions = Vec::<RedactionOccurrence>::with_capacity(targets.len());
    let mut page_counts = BTreeMap::<u32, u32>::new();
    for target in targets {
        redactions.push(RedactionOccurrence {
            page_index: target.page_index,
            bbox: target.bbox,
            kind: RedactionKind::DrawnRect,
            score: 1.0_f32,
            meta: BTreeMap::new(),
            underlying_text: vec![target.left_context.clone(), target.right_context.clone()],
        });
        *page_counts.entry(target.page_index).or_insert(0_u32) += 1_u32;
    }
    RedactionReport {
        input: input.display().to_string(),
        redactions: redactions.clone(),
        count: redactions.len() as u32,
        page_counts,
        diagnostics: vec!["synthetic_random_word_redactions=true".to_owned()],
    }
}

fn overlap_ratio(a: Rect, b: Rect) -> f32 {
    let overlap_w = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0_f32);
    let overlap_h = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0_f32);
    let overlap_area = overlap_w * overlap_h;
    let target_area = b.area().max(0.0001_f32);
    overlap_area / target_area
}

fn evaluate_target(
    guesses: &[RedactionGuess],
    target: &TargetCandidate,
) -> (Option<usize>, Option<String>) {
    let mut best_rank = None::<usize>;
    let mut best_overlap = 0.0_f32;
    let mut best_top1 = None::<String>;
    for guess in guesses {
        if guess.page_index != target.page_index {
            continue;
        }
        let overlap = overlap_ratio(guess.bbox, target.bbox);
        if overlap < TARGET_OVERLAP_RATIO_MIN {
            continue;
        }
        if let Some(rank) = rank_in_guess(guess, &target.text) {
            best_rank = Some(best_rank.map_or(rank, |current| current.min(rank)));
        }
        if overlap > best_overlap {
            best_overlap = overlap;
            best_top1 = guess
                .candidates
                .first()
                .map(|candidate| candidate.text.trim().to_ascii_uppercase())
                .filter(|value| !value.is_empty());
        }
    }
    (best_rank, best_top1)
}

fn rank_in_guess(guess: &RedactionGuess, target: &str) -> Option<usize> {
    let target_upper = target.trim().to_ascii_uppercase();
    if target_upper.is_empty() {
        return None;
    }
    let mut rank = 1_usize;
    let mut seen = BTreeSet::<String>::new();
    for candidate in &guess.candidates {
        let normalized = candidate.text.trim().to_ascii_uppercase();
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        if normalized == target_upper {
            return Some(rank);
        }
        rank += 1;
    }
    for exact in &guess.exact_matches {
        let normalized = exact.trim().to_ascii_uppercase();
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        if normalized == target_upper {
            return Some(rank);
        }
        rank += 1;
    }
    None
}

fn summarize_ranks(ranks: &[Option<usize>]) -> RankSummary {
    let evaluated_items = ranks.len();
    let found = ranks.iter().flatten().copied().collect::<Vec<_>>();
    let found_items = found.len();
    let recall_at = |k: usize| -> f64 {
        if evaluated_items == 0 {
            return 0.0_f64;
        }
        found.iter().filter(|rank| **rank <= k).count() as f64 / evaluated_items as f64
    };
    let mrr = if evaluated_items == 0 {
        0.0_f64
    } else {
        ranks
            .iter()
            .map(|rank| rank.map(|value| 1.0_f64 / value as f64).unwrap_or(0.0_f64))
            .sum::<f64>()
            / evaluated_items as f64
    };
    let mean_rank_found = if found.is_empty() {
        None
    } else {
        Some(found.iter().map(|value| *value as f64).sum::<f64>() / found.len() as f64)
    };
    RankSummary {
        evaluated_items,
        found_items,
        recall_at_1: recall_at(1),
        recall_at_5: recall_at(5),
        recall_at_20: recall_at(20),
        mrr,
        mean_rank_found,
    }
}

fn format_optional_f64(value: Option<f64>) -> String {
    value
        .map(|item| format!("{item:.3}"))
        .unwrap_or_else(|| "-".to_owned())
}

fn print_rank_summary(label: &str, summary: &RankSummary) {
    println!(
        "{} eval={} found={} r@1={:.3} r@5={:.3} r@20={:.3} mrr={:.3} mean_rank={}",
        label,
        summary.evaluated_items,
        summary.found_items,
        summary.recall_at_1,
        summary.recall_at_5,
        summary.recall_at_20,
        summary.mrr,
        format_optional_f64(summary.mean_rank_found)
    );
}

fn merge_rank_summaries<'a, I>(summaries: I) -> RankSummary
where
    I: Iterator<Item = &'a RankSummary>,
{
    let mut evaluated_items = 0_usize;
    let mut found_items = 0_usize;
    let mut recall_at_1_sum = 0.0_f64;
    let mut recall_at_5_sum = 0.0_f64;
    let mut recall_at_20_sum = 0.0_f64;
    let mut mrr_sum = 0.0_f64;
    let mut found_rank_sum = 0.0_f64;
    for summary in summaries {
        evaluated_items += summary.evaluated_items;
        found_items += summary.found_items;
        let weight = summary.evaluated_items as f64;
        recall_at_1_sum += summary.recall_at_1 * weight;
        recall_at_5_sum += summary.recall_at_5 * weight;
        recall_at_20_sum += summary.recall_at_20 * weight;
        mrr_sum += summary.mrr * weight;
        found_rank_sum += summary.mean_rank_found.unwrap_or(0.0_f64) * summary.found_items as f64;
    }
    let evaluation_weight = evaluated_items as f64;
    RankSummary {
        evaluated_items,
        found_items,
        recall_at_1: if evaluated_items == 0 {
            0.0_f64
        } else {
            recall_at_1_sum / evaluation_weight
        },
        recall_at_5: if evaluated_items == 0 {
            0.0_f64
        } else {
            recall_at_5_sum / evaluation_weight
        },
        recall_at_20: if evaluated_items == 0 {
            0.0_f64
        } else {
            recall_at_20_sum / evaluation_weight
        },
        mrr: if evaluated_items == 0 {
            0.0_f64
        } else {
            mrr_sum / evaluation_weight
        },
        mean_rank_found: if found_items == 0 {
            None
        } else {
            Some(found_rank_sum / found_items as f64)
        },
    }
}

fn print_report_summary(report: &SyntheticOverfittingReport) {
    println!("Synthetic Overfitting Benchmark");
    println!(
        "protocol id={} version={}",
        report.protocol.protocol_id, report.protocol.version
    );
    println!(
        "source_pool policy={} root={} discovered_pdf_count={}",
        report.source_pool.policy, report.source_pool.root, report.source_pool.discovered_pdf_count
    );
    println!(
        "run_completeness_gate policy={} passed={} expected_seed_runs={} observed_seed_runs={} expected_seed_file_evaluations={} observed_seed_file_evaluations={} skipped_seed_file_evaluations={} debug_build_detected={} required_profile=\"{}\"",
        report.run_completeness_gate.policy,
        report.run_completeness_gate.passed,
        report.run_completeness_gate.expected_seed_runs,
        report.run_completeness_gate.observed_seed_runs,
        report.run_completeness_gate.expected_seed_file_evaluations,
        report.run_completeness_gate.observed_seed_file_evaluations,
        report.run_completeness_gate.skipped_seed_file_evaluations,
        report.run_completeness_gate.debug_build_detected,
        report.run_completeness_gate.required_profile
    );
    if !report.run_completeness_gate.failures.is_empty() {
        for failure in &report.run_completeness_gate.failures {
            println!("RUN_COMPLETENESS_FAILURE {failure}");
        }
    }
    println!(
        "fixed_seed_gate policy={} passed={} mismatch_count={} seeds={:?}",
        report.fixed_seed_gate.policy,
        report.fixed_seed_gate.passed,
        report.fixed_seed_gate.mismatch_count,
        report.fixed_seed_gate.fixed_seeds
    );
    let fixed_overall = merge_rank_summaries(
        report
            .fixed_seed_gate
            .seed_reports
            .iter()
            .map(|seed| &seed.baseline.summary),
    );
    print_rank_summary("fixed_seed_gate_overall", &fixed_overall);
    for seed in &report.fixed_seed_gate.seed_reports {
        print_rank_summary(
            &format!(
                "fixed_seed seed={} files={} skipped={} targets={}",
                seed.seed,
                seed.baseline.evaluated_files,
                seed.baseline.skipped_files,
                seed.baseline.sampled_targets_total
            ),
            &seed.baseline.summary,
        );
    }
    println!(
        "exploratory_seed_diagnostics policy={} strategy={} seeds={}",
        report.exploratory_seed_diagnostics.policy,
        report.exploratory_seed_diagnostics.strategy,
        report.exploratory_seed_diagnostics.seeds.len()
    );
    print_rank_summary(
        "exploratory_seed_overall",
        &report.exploratory_seed_diagnostics.summary,
    );
    for seed in &report.exploratory_seed_diagnostics.seed_reports {
        print_rank_summary(
            &format!(
                "exploratory_seed seed={} files={} skipped={} targets={}",
                seed.seed, seed.evaluated_files, seed.skipped_files, seed.sampled_targets_total
            ),
            &seed.summary,
        );
    }
    println!(
        "multi_seed_panel policy={} passed={} seed_count={} required_min_seed_count={} delta_r20={:.6} delta_mrr={:.6} allowed_negative_delta_r20={:.6} allowed_negative_delta_mrr={:.6}",
        report.multi_seed_panel.policy,
        report.multi_seed_panel.passed,
        report.multi_seed_panel.seed_count,
        report.multi_seed_panel.required_min_seed_count,
        report.multi_seed_panel.delta_recall_at_20,
        report.multi_seed_panel.delta_mrr,
        report.multi_seed_panel.allowed_negative_delta_recall_at_20,
        report.multi_seed_panel.allowed_negative_delta_mrr
    );
    print_rank_summary(
        "multi_seed_panel_baseline_overall",
        &report.multi_seed_panel.baseline_summary,
    );
    print_rank_summary(
        "multi_seed_panel_current_overall",
        &report.multi_seed_panel.current_summary,
    );
    if !report.multi_seed_panel.failures.is_empty() {
        for failure in &report.multi_seed_panel.failures {
            println!("MULTI_SEED_PANEL_FAILURE {failure}");
        }
    }
    if report.fixed_seed_gate.mismatch_count > 0 {
        for mismatch in &report.fixed_seed_gate.mismatches {
            println!(
                "FIXED_SEED_MISMATCH class={} seed={} run={} pdf={} target={} baseline={} observed={}",
                mismatch.class,
                mismatch.seed,
                mismatch.run_index,
                mismatch.pdf_path.as_deref().unwrap_or("-"),
                mismatch.target.as_deref().unwrap_or("-"),
                mismatch.baseline.as_deref().unwrap_or("-"),
                mismatch.observed.as_deref().unwrap_or("-")
            );
        }
        if report.fixed_seed_gate.mismatch_count > report.fixed_seed_gate.mismatches.len() {
            println!(
                "FIXED_SEED_MISMATCH truncated={} total={}",
                report.fixed_seed_gate.mismatch_count - report.fixed_seed_gate.mismatches.len(),
                report.fixed_seed_gate.mismatch_count
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_run_completeness_gate, compare_seed_runs, discover_pdfs, hard_gate_passed,
        protocol_configuration, DeterminismMismatch, ExploratorySeedDiagnosticsSection,
        FileSeedRows, FixedSeedGateSection, MultiSeedPanelSection, RankSummary,
        RunCompletenessGateSection, SeedEvaluationReport, SeedSnapshot, SourcePoolInventory,
        SyntheticOverfittingReport, TargetRow, HARD_THRESHOLD_MARGIN_RATIO,
        MULTI_SEED_PANEL_MIN_COUNT, MULTI_SEED_PANEL_MRR_TOLERANCE, MULTI_SEED_PANEL_R20_TOLERANCE,
    };
    use std::path::PathBuf;

    fn seed_snapshot(
        seed: u64,
        hash: &str,
        pdf_path: &str,
        target: &str,
        rank: Option<usize>,
        top1: Option<&str>,
    ) -> SeedSnapshot {
        SeedSnapshot {
            report: SeedEvaluationReport {
                seed,
                evaluated_files: 1,
                skipped_files: 0,
                sampled_targets_total: 1,
                summary: RankSummary {
                    evaluated_items: 1,
                    found_items: usize::from(rank.is_some()),
                    recall_at_1: 0.0_f64,
                    recall_at_5: 0.0_f64,
                    recall_at_20: 0.0_f64,
                    mrr: 0.0_f64,
                    mean_rank_found: None,
                },
                comparator_hash: hash.to_owned(),
                files: Vec::new(),
            },
            rows: vec![FileSeedRows {
                pdf_path: pdf_path.to_owned(),
                target_rows: vec![TargetRow {
                    target: target.to_owned(),
                    best_rank: rank,
                    top1: top1.map(str::to_owned),
                }],
            }],
            completeness_failure: None,
        }
    }

    fn empty_summary() -> RankSummary {
        RankSummary {
            evaluated_items: 0,
            found_items: 0,
            recall_at_1: 0.0_f64,
            recall_at_5: 0.0_f64,
            recall_at_20: 0.0_f64,
            mrr: 0.0_f64,
            mean_rank_found: None,
        }
    }

    fn minimal_report(fixed_passed: bool) -> SyntheticOverfittingReport {
        let protocol = protocol_configuration();
        SyntheticOverfittingReport {
            protocol: protocol.clone(),
            source_pool: SourcePoolInventory {
                policy: protocol.source_pool_policy.clone(),
                root: "test_data".to_owned(),
                discovered_pdf_count: 0,
                discovered_pdfs: Vec::new(),
            },
            run_completeness_gate: RunCompletenessGateSection {
                policy: "binding_full_coverage".to_owned(),
                required_profile: "cargo run --bin synthetic_overfitting_benchmark --release"
                    .to_owned(),
                debug_build_detected: false,
                discovered_pdf_count: 0,
                expected_seed_runs: 0,
                observed_seed_runs: 0,
                expected_seed_file_evaluations: 0,
                observed_seed_file_evaluations: 0,
                skipped_seed_file_evaluations: 0,
                failed_seeds: Vec::new(),
                failures: Vec::new(),
                passed: true,
            },
            fixed_seed_gate: FixedSeedGateSection {
                policy: protocol.fixed_seed_policy.clone(),
                fixed_seeds: protocol.fixed_seeds.clone(),
                runs_per_seed: protocol.fixed_runs_per_seed,
                passed: fixed_passed,
                mismatch_count: usize::from(!fixed_passed),
                mismatches: Vec::new(),
                seed_reports: Vec::new(),
            },
            exploratory_seed_diagnostics: ExploratorySeedDiagnosticsSection {
                policy: protocol.exploratory_seed_policy.clone(),
                strategy: protocol.exploratory_seed_strategy.clone(),
                seeds: Vec::new(),
                summary: empty_summary(),
                seed_reports: Vec::new(),
            },
            multi_seed_panel: MultiSeedPanelSection {
                policy: "binding_average_non_regression".to_owned(),
                seed_count: MULTI_SEED_PANEL_MIN_COUNT,
                required_min_seed_count: MULTI_SEED_PANEL_MIN_COUNT,
                tolerance_recall_at_20: MULTI_SEED_PANEL_R20_TOLERANCE,
                tolerance_mrr: MULTI_SEED_PANEL_MRR_TOLERANCE,
                margin_ratio: HARD_THRESHOLD_MARGIN_RATIO,
                baseline_summary: empty_summary(),
                current_summary: empty_summary(),
                delta_recall_at_20: 0.0_f64,
                delta_mrr: 0.0_f64,
                allowed_negative_delta_recall_at_20: MULTI_SEED_PANEL_R20_TOLERANCE
                    * (1.0_f64 + HARD_THRESHOLD_MARGIN_RATIO),
                allowed_negative_delta_mrr: MULTI_SEED_PANEL_MRR_TOLERANCE
                    * (1.0_f64 + HARD_THRESHOLD_MARGIN_RATIO),
                passed_seed_count: true,
                passed_thresholds: true,
                passed: true,
                failures: Vec::new(),
            },
        }
    }

    fn temp_test_root(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("unredact_{name}_{}_{}", std::process::id(), unique))
    }

    #[test]
    fn fixed_seed_comparator_accepts_identical_runs() {
        let baseline = seed_snapshot(
            123,
            "hash_a",
            "test_data/a.pdf",
            "ALPHA",
            Some(1),
            Some("ALPHA"),
        );
        let observed = seed_snapshot(
            123,
            "hash_a",
            "test_data/a.pdf",
            "ALPHA",
            Some(1),
            Some("ALPHA"),
        );
        let mut mismatch_count = 0_usize;
        let mut mismatches = Vec::<DeterminismMismatch>::new();
        compare_seed_runs(
            123,
            &[baseline, observed],
            &mut mismatch_count,
            &mut mismatches,
        );
        assert_eq!(mismatch_count, 0);
        assert!(mismatches.is_empty());
    }

    #[test]
    fn fixed_seed_comparator_reports_hash_rank_and_top1_divergence() {
        let baseline = seed_snapshot(
            123,
            "hash_a",
            "test_data/a.pdf",
            "ALPHA",
            Some(1),
            Some("ALPHA"),
        );
        let observed = seed_snapshot(
            123,
            "hash_b",
            "test_data/a.pdf",
            "ALPHA",
            Some(2),
            Some("OMEGA"),
        );
        let mut mismatch_count = 0_usize;
        let mut mismatches = Vec::<DeterminismMismatch>::new();
        compare_seed_runs(
            123,
            &[baseline, observed],
            &mut mismatch_count,
            &mut mismatches,
        );
        assert!(mismatch_count >= 3);
        let classes = mismatches
            .iter()
            .map(|item| item.class.clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(classes.contains("seed_hash_mismatch"));
        assert!(classes.contains("rank_set_divergence"));
        assert!(classes.contains("top1_divergence"));
    }

    #[test]
    fn report_has_policy_labeled_tier_separation() {
        let report = minimal_report(true);
        assert_eq!(report.fixed_seed_gate.policy, "binding");
        assert_eq!(
            report.exploratory_seed_diagnostics.policy,
            "diagnostic_only"
        );
        assert_ne!(
            report.fixed_seed_gate.policy,
            report.exploratory_seed_diagnostics.policy
        );
        assert_eq!(report.protocol.protocol_id, "C-SYNTHETIC-SEED-TIERS");
        assert_eq!(report.protocol.version, 1);
    }

    #[test]
    fn hard_gate_fails_when_fixed_seed_gate_fails() {
        let passing = minimal_report(true);
        let failing = minimal_report(false);
        assert!(hard_gate_passed(&passing));
        assert!(!hard_gate_passed(&failing));
    }

    #[test]
    fn hard_gate_fails_when_run_completeness_gate_fails() {
        let mut report = minimal_report(true);
        report.run_completeness_gate.passed = false;
        report
            .run_completeness_gate
            .failures
            .push("seed_run_count_mismatch expected=11 observed=10".to_owned());
        assert!(!hard_gate_passed(&report));
    }

    #[test]
    fn hard_gate_fails_when_multi_seed_panel_gate_fails() {
        let mut report = minimal_report(true);
        report.multi_seed_panel.passed = false;
        report
            .multi_seed_panel
            .failures
            .push("mrr_regression delta=-0.020000 allowed_negative_delta=0.002020".to_owned());
        assert!(!hard_gate_passed(&report));
    }

    #[test]
    fn run_completeness_gate_fails_when_seed_coverage_is_partial() {
        let snapshot = seed_snapshot(
            123,
            "hash_a",
            "test_data/a.pdf",
            "ALPHA",
            Some(1),
            Some("ALPHA"),
        );
        let gate = build_run_completeness_gate(2, 2, &[snapshot]);
        assert!(!gate.passed);
        assert!(!gate.failures.is_empty());
    }

    #[test]
    fn discover_pdfs_recurses_and_sorts() {
        let root = temp_test_root("synthetic_pdf_discovery");
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).expect("temporary nested directory should be created");
        std::fs::write(root.join("b.pdf"), b"").expect("temporary test file should be created");
        std::fs::write(root.join("a.PDF"), b"").expect("temporary test file should be created");
        std::fs::write(nested.join("c.pdf"), b"").expect("temporary test file should be created");
        std::fs::write(nested.join("ignore.txt"), b"")
            .expect("temporary test file should be created");

        let discovered = discover_pdfs(&root).expect("pdf discovery should succeed");
        let expected = vec![root.join("a.PDF"), root.join("b.pdf"), nested.join("c.pdf")];
        assert_eq!(discovered, expected);

        std::fs::remove_dir_all(&root).expect("temporary test directory should be removed");
    }
}

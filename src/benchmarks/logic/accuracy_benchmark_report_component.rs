use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::benchmarks::data::accuracy_benchmark_report_data::{
    build_best_possible_rank, build_candidate_pool_quality, build_dictionary_ablation_summary,
    build_family_composition, build_hard_negative_full_name_dictionary,
    build_pairwise_winner_explanations, build_perturbation_robustness, build_stability_summary,
    build_tie_density, evaluate_dataset, filter_full_name_only, filter_no_comma_single,
    render_definitions_markdown, render_summary_markdown, summarize_variant,
    DatasetEvaluationInput,
};
use crate::benchmarks::types::accuracy_benchmark_report_types::{
    AccuracyBenchmarkReportManifest, AccuracyBenchmarkSummary, VariantSummary,
    VisualBenchmarkStageSummary,
};
use crate::benchmarks::types::known_redaction_contract::{
    canonical_known_redaction_contract, KnownRedactionContract, KnownRedactionDataset,
    KnownRedactionTarget, KnownRedactionTargetSelector,
};
use crate::logic::write_output_bytes;
use crate::service::anchor_span_visual_benchmark_cli_entry::{
    run as run_anchor_span_visual_benchmark, AnchorSpanVisualBenchmarkRequest,
};
use crate::service::tooling_entry::default_name_dictionary_entries;
use crate::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use crate::types::guess_types::{GuessConfig, GuessReport};
use crate::types::visualizer_config::VisualizerConfig;

const NOISE_WORDS: [&str; 24] = [
    "ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT", "GOLF", "HOTEL", "INDIA", "JULIET",
    "KILO", "LIMA", "MIKE", "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO", "SIERRA", "TANGO",
    "UNIFORM", "VICTOR", "WHISKEY", "XRAY",
];

pub const DICTIONARY_VARIANT_BASELINE: &str = "baseline";
pub const DICTIONARY_VARIANT_DEFAULT: &str = "default_dictionary";
pub const DICTIONARY_VARIANT_FULL_NAME_ONLY: &str = "full_name_only";
pub const DICTIONARY_VARIANT_NO_COMMA_SINGLE: &str = "no_comma_single";
pub const DICTIONARY_VARIANT_HARD_NEGATIVE_W2: &str = "hard_negative_full_name_w2";
pub const DICTIONARY_VARIANT_HARD_NEGATIVE_W5: &str = "hard_negative_full_name_w5";

#[derive(Debug, Clone, PartialEq)]
pub struct AccuracyBenchmarkReportRequest {
    pub output_dir: PathBuf,
    pub repeats: usize,
    pub compact: bool,
}

#[derive(Debug, Clone)]
pub struct AccuracyBenchmarkReportRun {
    pub manifest: AccuracyBenchmarkReportManifest,
    pub summary: AccuracyBenchmarkSummary,
    pub summary_markdown: String,
    pub definitions_markdown: String,
    pub baseline_stage: VariantSummary,
    pub dictionary_ablation:
        crate::benchmarks::types::accuracy_benchmark_report_types::DictionaryAblationSummary,
    pub candidate_pool_quality:
        crate::benchmarks::types::accuracy_benchmark_report_types::CandidatePoolQualitySummary,
    pub family_composition:
        crate::benchmarks::types::accuracy_benchmark_report_types::FamilyCompositionSummary,
    pub best_possible_rank:
        crate::benchmarks::types::accuracy_benchmark_report_types::BestPossibleRankSummary,
    pub pairwise_winner_explanations:
        crate::benchmarks::types::accuracy_benchmark_report_types::PairwiseWinnerSummary,
    pub tie_density: crate::benchmarks::types::accuracy_benchmark_report_types::TieDensitySummary,
    pub perturbation_robustness:
        crate::benchmarks::types::accuracy_benchmark_report_types::PerturbationRobustnessSummary,
    pub stability: crate::benchmarks::types::accuracy_benchmark_report_types::StabilitySummary,
    pub visual_stage: Option<VisualBenchmarkStageSummary>,
}

type DictionaryBuilder = fn(&KnownRedactionDataset, &VariantSummary) -> Vec<String>;

#[inline]
pub fn run_accuracy_benchmark_report(
    req: AccuracyBenchmarkReportRequest,
) -> Result<AccuracyBenchmarkReportRun, String> {
    let contract = canonical_known_redaction_contract()?.clone();
    let baseline_repeats = (0..req.repeats.max(1))
        .map(|repeat_index| {
            run_variant(
                &contract,
                DICTIONARY_VARIANT_BASELINE,
                &req.output_dir
                    .join("runs")
                    .join(DICTIONARY_VARIANT_BASELINE)
                    .join(format!("repeat{repeat_index:02}")),
                None,
                None,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let baseline_stage = baseline_repeats
        .first()
        .cloned()
        .ok_or_else(|| "baseline benchmark did not produce a run".to_owned())?;

    let mut variant_runs = vec![baseline_stage.clone()];
    for (variant_name, builder) in build_dictionary_variants() {
        variant_runs.push(run_variant(
            &contract,
            variant_name,
            &req.output_dir.join("runs").join(variant_name),
            Some(&baseline_stage),
            Some(builder),
        )?);
    }

    let dictionary_ablation = build_dictionary_ablation_summary(&variant_runs);
    let candidate_pool_quality = build_candidate_pool_quality(&baseline_stage);
    let family_composition = build_family_composition(&baseline_stage);
    let best_possible_rank = build_best_possible_rank(&baseline_stage);
    let pairwise_winner_explanations = build_pairwise_winner_explanations(&baseline_stage);
    let tie_density = build_tie_density(&baseline_stage);
    let perturbation_robustness = build_perturbation_robustness(&baseline_stage);
    let stability = build_stability_summary(&baseline_repeats)?;
    let visual_stage = Some(run_visual_stage(
        &req.output_dir.join("stages").join("anchor_span_visual"),
    )?);

    let manifest = AccuracyBenchmarkReportManifest {
        contract_id: contract.contract_id.clone(),
        schema_version: contract.schema_version,
        canonical_target_count: contract.canonical_target_count(),
        repeats: req.repeats.max(1),
        dictionary_variants: variant_runs
            .iter()
            .map(|variant| variant.name.clone())
            .collect(),
        executed_stages: vec![
            "guess_baseline".to_owned(),
            "dictionary_ablation".to_owned(),
            "candidate_pool_quality".to_owned(),
            "family_composition".to_owned(),
            "best_possible_rank".to_owned(),
            "pairwise_winner_explanations".to_owned(),
            "tie_density".to_owned(),
            "perturbation_robustness".to_owned(),
            "stability".to_owned(),
            "anchor_span_visual".to_owned(),
        ],
    };
    let summary = AccuracyBenchmarkSummary {
        manifest: manifest.clone(),
        baseline: baseline_stage.clone(),
        dictionary_ablation: dictionary_ablation.clone(),
        candidate_pool_quality: candidate_pool_quality.clone(),
        family_composition: family_composition.clone(),
        best_possible_rank: best_possible_rank.clone(),
        pairwise_winner_explanations: pairwise_winner_explanations.clone(),
        tie_density: tie_density.clone(),
        perturbation_robustness: perturbation_robustness.clone(),
        stability: stability.clone(),
        anchor_span_visual_summary_path: visual_stage
            .as_ref()
            .map(|stage| stage.summary_path.clone()),
    };
    let summary_markdown = render_summary_markdown(&summary);
    let definitions_markdown = render_definitions_markdown();

    Ok(AccuracyBenchmarkReportRun {
        manifest,
        summary,
        summary_markdown,
        definitions_markdown,
        baseline_stage,
        dictionary_ablation,
        candidate_pool_quality,
        family_composition,
        best_possible_rank,
        pairwise_winner_explanations,
        tie_density,
        perturbation_robustness,
        stability,
        visual_stage,
    })
}

#[inline]
pub fn encode_json<T: serde::Serialize>(value: &T, compact: bool) -> Result<Vec<u8>, String> {
    if compact {
        serde_json::to_vec(value).map_err(|error| format!("failed to encode json: {error}"))
    } else {
        serde_json::to_vec_pretty(value).map_err(|error| format!("failed to encode json: {error}"))
    }
}

#[inline]
pub fn write_report_artifact(path: &Path, payload: &[u8]) -> Result<(), String> {
    write_output_bytes(path, payload)
}

fn benchmark_config() -> UnredactServiceConfig {
    UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig::default(),
        visualize: false,
        visualizer: VisualizerConfig::default(),
    }
}

fn run_variant(
    contract: &KnownRedactionContract,
    variant_name: &str,
    output_root: &Path,
    baseline: Option<&VariantSummary>,
    dictionary_builder: Option<DictionaryBuilder>,
) -> Result<VariantSummary, String> {
    let mut datasets = Vec::<
        crate::benchmarks::types::accuracy_benchmark_report_types::VariantDatasetResult,
    >::new();
    for dataset in &contract.datasets {
        let dictionary_entries = match (baseline, dictionary_builder) {
            (Some(baseline), Some(builder)) => Some(builder(dataset, baseline)),
            (None, None) => baseline_dictionary_entries(dataset),
            _ => {
                return Err(format!(
                    "variant '{variant_name}' received an incomplete dictionary builder configuration"
                ));
            }
        };
        let report = run_report(dataset, output_root, variant_name, dictionary_entries)?;
        datasets.push(evaluate_dataset(DatasetEvaluationInput {
            dataset,
            report: &report,
        })?);
    }
    Ok(summarize_variant(variant_name, datasets))
}

fn run_report(
    dataset: &KnownRedactionDataset,
    output_root: &Path,
    variant_name: &str,
    dictionary_entries: Option<Vec<String>>,
) -> Result<GuessReport, String> {
    let input = Path::new(&dataset.input_pdf);
    let dataset_dir = output_root.join(&dataset.name);
    std::fs::create_dir_all(&dataset_dir)
        .map_err(|error| format!("failed to create {}: {error}", dataset_dir.display()))?;
    let dictionary_path = if let Some(entries) = dictionary_entries {
        let path = dataset_dir.join(format!("{variant_name}.dictionary.txt"));
        std::fs::write(&path, entries.join("\n"))
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        Some(path)
    } else {
        None
    };
    let outputs = run_from_paths(
        input,
        &dataset_dir,
        dictionary_path.as_deref(),
        benchmark_config(),
    )?;
    let bytes = std::fs::read(&outputs.guesses_path)
        .map_err(|error| format!("failed to read {}: {error}", outputs.guesses_path.display()))?;
    serde_json::from_slice::<GuessReport>(&bytes).map_err(|error| {
        format!(
            "failed to parse guess report {}: {error}",
            outputs.guesses_path.display()
        )
    })
}

fn run_visual_stage(output_dir: &Path) -> Result<VisualBenchmarkStageSummary, String> {
    let outputs = run_anchor_span_visual_benchmark(AnchorSpanVisualBenchmarkRequest {
        output_dir: output_dir.to_path_buf(),
        compact: true,
    })?;
    Ok(VisualBenchmarkStageSummary {
        summary_path: outputs.summary_path,
        rows_path: outputs.rows_path,
        experiments_dir: outputs.experiments_dir,
        crops_dir: outputs.crops_dir,
    })
}

fn build_dictionary_variants() -> Vec<(&'static str, DictionaryBuilder)> {
    vec![
        (DICTIONARY_VARIANT_DEFAULT, dictionary_default),
        (DICTIONARY_VARIANT_FULL_NAME_ONLY, dictionary_full_name_only),
        (
            DICTIONARY_VARIANT_NO_COMMA_SINGLE,
            dictionary_no_comma_single,
        ),
        (
            DICTIONARY_VARIANT_HARD_NEGATIVE_W2,
            dictionary_hard_negative_w2,
        ),
        (
            DICTIONARY_VARIANT_HARD_NEGATIVE_W5,
            dictionary_hard_negative_w5,
        ),
    ]
}

fn dictionary_default(dataset: &KnownRedactionDataset, _baseline: &VariantSummary) -> Vec<String> {
    merge_with_targets(
        default_name_dictionary_entries()
            .iter()
            .map(|entry| (*entry).to_owned())
            .collect::<Vec<_>>(),
        &dataset.targets,
    )
}

fn dictionary_full_name_only(
    dataset: &KnownRedactionDataset,
    _baseline: &VariantSummary,
) -> Vec<String> {
    merge_with_targets(
        filter_full_name_only(default_name_dictionary_entries()),
        &dataset.targets,
    )
}

fn dictionary_no_comma_single(
    dataset: &KnownRedactionDataset,
    _baseline: &VariantSummary,
) -> Vec<String> {
    merge_with_targets(
        filter_no_comma_single(default_name_dictionary_entries()),
        &dataset.targets,
    )
}

fn dictionary_hard_negative_w2(
    dataset: &KnownRedactionDataset,
    baseline: &VariantSummary,
) -> Vec<String> {
    merge_with_targets(
        build_hard_negative_full_name_dictionary(
            &single_dataset_variant(baseline, &dataset.name),
            2.0_f32,
            200,
        ),
        &dataset.targets,
    )
}

fn dictionary_hard_negative_w5(
    dataset: &KnownRedactionDataset,
    baseline: &VariantSummary,
) -> Vec<String> {
    merge_with_targets(
        build_hard_negative_full_name_dictionary(
            &single_dataset_variant(baseline, &dataset.name),
            5.0_f32,
            200,
        ),
        &dataset.targets,
    )
}

fn single_dataset_variant(baseline: &VariantSummary, dataset_name: &str) -> VariantSummary {
    VariantSummary {
        name: baseline.name.clone(),
        overall: baseline.overall.clone(),
        datasets: baseline
            .datasets
            .iter()
            .filter(|dataset| dataset.name == dataset_name)
            .cloned()
            .collect::<Vec<_>>(),
    }
}

fn merge_with_targets(
    entries: impl IntoIterator<Item = String>,
    targets: &[KnownRedactionTarget],
) -> Vec<String> {
    let mut out = BTreeSet::<String>::new();
    for target in targets {
        let trimmed = target.target.trim();
        if !trimmed.is_empty() {
            out.insert(trimmed.to_ascii_uppercase());
        }
    }
    for entry in entries {
        let trimmed = entry.trim();
        if !trimmed.is_empty() {
            out.insert(trimmed.to_ascii_uppercase());
        }
    }
    for noise in NOISE_WORDS {
        out.insert(noise.to_owned());
    }
    out.into_iter().collect::<Vec<_>>()
}

fn baseline_dictionary_entries(dataset: &KnownRedactionDataset) -> Option<Vec<String>> {
    if !dataset
        .targets
        .iter()
        .all(|target| matches!(target.selector, KnownRedactionTargetSelector::InPool {}))
    {
        return None;
    }
    let mut out = BTreeSet::<String>::new();
    for target in &dataset.targets {
        let trimmed = target.target.trim();
        if !trimmed.is_empty() {
            out.insert(trimmed.to_ascii_uppercase());
        }
    }
    for entry in default_name_dictionary_entries() {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.insert(trimmed.to_ascii_uppercase());
        if out.len() >= 1_200 {
            break;
        }
    }
    for noise in NOISE_WORDS {
        out.insert(noise.to_owned());
    }
    Some(out.into_iter().collect::<Vec<_>>())
}

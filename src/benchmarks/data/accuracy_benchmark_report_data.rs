use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest as _, Sha256};

use crate::benchmarks::types::accuracy_benchmark_report_types::{
    BenchmarkSummary, BestPossibleRankRow, BestPossibleRankSummary, CandidatePoolQualityRow,
    CandidatePoolQualitySummary, CandidateSummary, DictionaryAblationSummary,
    DictionaryVariantResult, FamilyCompositionSummary, FamilyCount, PairwiseWinnerExplanation,
    PairwiseWinnerSummary, PerturbationRobustnessRow, PerturbationRobustnessSummary,
    QualitySummary, SelectedGuessRow, StabilityDatasetSummary, StabilitySummary, TargetResult,
    TieDensityRow, TieDensitySummary, VariantDatasetResult, VariantSummary,
};
use crate::benchmarks::types::known_redaction_contract::{
    KnownRedactionDataset, KnownRedactionRowSelector, KnownRedactionTargetSelector,
};
use crate::types::guess_types::{GuessCandidate, GuessReport, RedactionGuess};

const TIE_THRESHOLDS_PT: [f32; 5] = [0.05_f32, 0.10_f32, 0.25_f32, 0.50_f32, 1.0_f32];
const MISSING_RANK_SENTINEL: f64 = 10_000.0_f64;

#[derive(Debug, Clone)]
pub struct DatasetEvaluationInput<'a> {
    pub dataset: &'a KnownRedactionDataset,
    pub report: &'a GuessReport,
}

#[inline]
pub fn evaluate_dataset(input: DatasetEvaluationInput<'_>) -> Result<VariantDatasetResult, String> {
    let selected_rows = selected_rows_for_dataset(input.dataset, input.report)?;
    let targets = evaluate_targets(input.dataset, &selected_rows)?;
    let ranks = targets
        .iter()
        .map(|target| target.best_rank)
        .collect::<Vec<_>>();
    Ok(VariantDatasetResult {
        name: input.dataset.name.clone(),
        summary: summarize_ranks(&ranks),
        candidate_summary: summarize_candidate_rows(&selected_rows),
        quality_summary: summarize_quality(&selected_rows),
        targets,
        selected_rows,
    })
}

#[inline]
pub fn summarize_variant(name: &str, datasets: Vec<VariantDatasetResult>) -> VariantSummary {
    let mut all_ranks = Vec::<Option<usize>>::new();
    for dataset in &datasets {
        all_ranks.extend(dataset.targets.iter().map(|target| target.best_rank));
    }
    VariantSummary {
        name: name.to_owned(),
        overall: summarize_ranks(&all_ranks),
        datasets,
    }
}

#[inline]
pub fn build_candidate_pool_quality(variant: &VariantSummary) -> CandidatePoolQualitySummary {
    let rows = variant
        .datasets
        .iter()
        .flat_map(|dataset| {
            dataset
                .targets
                .iter()
                .map(|target| CandidatePoolQualityRow {
                    dataset: target.dataset.clone(),
                    label: target.label.clone(),
                    target: target.target.clone(),
                    best_rank: target.best_rank,
                    present_in_pool: target.present_in_pool,
                    best_row_key: target.best_row_key.clone(),
                    candidate_count: target.candidate_count,
                    better_than_target_count: target.best_rank.map(|rank| rank.saturating_sub(1)),
                    same_family_better_count: target.best_row_key.as_ref().and_then(|row_key| {
                        dataset
                            .selected_rows
                            .iter()
                            .find(|row| &row.row_key == row_key)
                            .and_then(|row| same_family_better_count(row, &target.target))
                    }),
                    top1_text: target.top1_text.clone(),
                    target_error_pt: target.target_error_pt,
                    top1_error_pt: target.top1_error_pt,
                })
        })
        .collect::<Vec<_>>();
    CandidatePoolQualitySummary {
        targets_total: rows.len(),
        targets_present_in_pool: rows.iter().filter(|row| row.present_in_pool).count(),
        targets_missing_from_pool: rows.iter().filter(|row| !row.present_in_pool).count(),
        targets_ranked_top_20: rows
            .iter()
            .filter_map(|row| row.best_rank)
            .filter(|rank| *rank <= 20)
            .count(),
        rows,
    }
}

#[inline]
pub fn build_family_composition(variant: &VariantSummary) -> FamilyCompositionSummary {
    let mut target_counts = BTreeMap::<String, usize>::new();
    let mut top1_counts = BTreeMap::<String, usize>::new();
    let mut candidate_counts = BTreeMap::<String, usize>::new();

    for dataset in &variant.datasets {
        for target in &dataset.targets {
            increment_count(&mut target_counts, classify_name_family(&target.target));
            if let Some(top1) = target.top1_text.as_deref() {
                increment_count(&mut top1_counts, classify_name_family(top1));
            }
        }
        for row in &dataset.selected_rows {
            for candidate in &row.candidates {
                increment_count(&mut candidate_counts, classify_name_family(&candidate.text));
            }
        }
    }

    FamilyCompositionSummary {
        target_families: counts_to_sorted_vec(target_counts),
        top1_families: counts_to_sorted_vec(top1_counts),
        candidate_families: counts_to_sorted_vec(candidate_counts),
    }
}

#[inline]
pub fn build_best_possible_rank(variant: &VariantSummary) -> BestPossibleRankSummary {
    let mut rows = Vec::<BestPossibleRankRow>::new();
    let mut same_family = 0_usize;
    let mut plain_multi = 0_usize;
    let mut no_comma_single = 0_usize;

    for dataset in &variant.datasets {
        for target in &dataset.targets {
            let ranks = dataset
                .selected_rows
                .iter()
                .find(|row| target.best_row_key.as_deref() == Some(row.row_key.as_str()))
                .map(|row| {
                    let exact = rank_in_row(row, &target.target, |_| true);
                    let family = rank_in_row(row, &target.target, |candidate| {
                        classify_name_family(&candidate.text)
                            == classify_name_family(&target.target)
                    });
                    let plain = rank_in_row(row, &target.target, |candidate| {
                        classify_name_family(&candidate.text) == "plain_multi_token"
                    });
                    let pruned = rank_in_row(row, &target.target, |candidate| {
                        !matches!(
                            classify_name_family(&candidate.text).as_str(),
                            "comma" | "single_token"
                        )
                    });
                    (exact, family, plain, pruned)
                })
                .unwrap_or((None, None, None, None));
            if improves_rank(target.best_rank, ranks.1) {
                same_family += 1;
            }
            if improves_rank(target.best_rank, ranks.2) {
                plain_multi += 1;
            }
            if improves_rank(target.best_rank, ranks.3) {
                no_comma_single += 1;
            }
            rows.push(BestPossibleRankRow {
                dataset: target.dataset.clone(),
                label: target.label.clone(),
                target: target.target.clone(),
                current_rank: target.best_rank,
                exact_oracle_rank: ranks.0.map(|_| 1),
                same_family_rank: ranks.1,
                plain_multi_token_rank: ranks.2,
                no_comma_single_rank: ranks.3,
            });
        }
    }

    BestPossibleRankSummary {
        rows,
        improvable_by_same_family: same_family,
        improvable_by_plain_multi_token: plain_multi,
        improvable_by_no_comma_single: no_comma_single,
    }
}

#[inline]
pub fn build_pairwise_winner_explanations(variant: &VariantSummary) -> PairwiseWinnerSummary {
    let mut rows = Vec::<PairwiseWinnerExplanation>::new();
    let mut reason_counts = BTreeMap::<String, usize>::new();
    for dataset in &variant.datasets {
        for target in &dataset.targets {
            let reason = build_pairwise_reason(dataset, target);
            increment_count(&mut reason_counts, reason.reason.clone());
            rows.push(reason);
        }
    }
    PairwiseWinnerSummary {
        reasons: counts_to_sorted_vec(reason_counts),
        rows,
    }
}

#[inline]
pub fn build_tie_density(variant: &VariantSummary) -> TieDensitySummary {
    let mut rows = Vec::<TieDensityRow>::new();
    let mut within_target_050 = Vec::<f64>::new();
    let mut within_top1_050 = Vec::<f64>::new();
    for dataset in &variant.datasets {
        for target in &dataset.targets {
            let row = target.best_row_key.as_ref().and_then(|row_key| {
                dataset
                    .selected_rows
                    .iter()
                    .find(|row| &row.row_key == row_key)
            });
            let density = build_tie_density_row(target, row);
            if let Some(value) = density.within_target_050 {
                within_target_050.push(value as f64);
            }
            if let Some(value) = density.within_top1_050 {
                within_top1_050.push(value as f64);
            }
            rows.push(density);
        }
    }
    TieDensitySummary {
        rows,
        mean_within_target_050: mean(&within_target_050),
        mean_within_top1_050: mean(&within_top1_050),
    }
}

#[inline]
pub fn build_perturbation_robustness(variant: &VariantSummary) -> PerturbationRobustnessSummary {
    let mut rows = Vec::<PerturbationRobustnessRow>::new();
    let mut changed_025 = 0_usize;
    let mut changed_050 = 0_usize;
    let mut changed_100 = 0_usize;
    for dataset in &variant.datasets {
        for target in &dataset.targets {
            let row = target.best_row_key.as_ref().and_then(|row_key| {
                dataset
                    .selected_rows
                    .iter()
                    .find(|row| &row.row_key == row_key)
            });
            let robustness = build_perturbation_row(target, row);
            if robustness.changed_at_025 {
                changed_025 += 1;
            }
            if robustness.changed_at_050 {
                changed_050 += 1;
            }
            if robustness.changed_at_100 {
                changed_100 += 1;
            }
            rows.push(robustness);
        }
    }
    PerturbationRobustnessSummary {
        rows,
        changed_at_025: changed_025,
        changed_at_050: changed_050,
        changed_at_100: changed_100,
    }
}

#[inline]
pub fn build_dictionary_ablation_summary(variants: &[VariantSummary]) -> DictionaryAblationSummary {
    let best_variant_by_mrr = variants
        .iter()
        .max_by(|left, right| {
            left.overall
                .mrr
                .partial_cmp(&right.overall.mrr)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|variant| variant.name.clone());
    let best_variant_by_mean_rank = variants
        .iter()
        .min_by(|left, right| {
            option_f64_for_min(left.overall.mean_rank_found)
                .partial_cmp(&option_f64_for_min(right.overall.mean_rank_found))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|variant| variant.name.clone());
    DictionaryAblationSummary {
        baseline_variant: variants
            .first()
            .map(|variant| variant.name.clone())
            .unwrap_or_else(|| "baseline".to_owned()),
        best_variant_by_mrr,
        best_variant_by_mean_rank,
        variants: variants
            .iter()
            .map(|variant| DictionaryVariantResult {
                variant: variant.name.clone(),
                overall: variant.overall.clone(),
                datasets: variant.datasets.clone(),
            })
            .collect::<Vec<_>>(),
    }
}

#[inline]
pub fn build_stability_summary(repeats: &[VariantSummary]) -> Result<StabilitySummary, String> {
    if repeats.is_empty() {
        return Ok(StabilitySummary {
            repeats: 0,
            all_hashes_identical: true,
            per_dataset: Vec::new(),
        });
    }

    let mut per_dataset = Vec::<StabilityDatasetSummary>::new();
    let dataset_names = repeats[0]
        .datasets
        .iter()
        .map(|dataset| dataset.name.clone())
        .collect::<Vec<_>>();
    let mut all_hashes_identical = true;

    for dataset_name in dataset_names {
        let dataset_runs = repeats
            .iter()
            .filter_map(|variant| {
                variant
                    .datasets
                    .iter()
                    .find(|dataset| dataset.name == dataset_name)
            })
            .collect::<Vec<_>>();
        let hashes = dataset_runs
            .iter()
            .map(|dataset| hash_dataset(dataset))
            .collect::<Result<Vec<_>, String>>()?;
        let identical = hashes.windows(2).all(|pair| pair[0] == pair[1]);
        all_hashes_identical &= identical;

        let target_labels = dataset_runs[0]
            .targets
            .iter()
            .map(|target| target.label.clone())
            .collect::<Vec<_>>();
        let mut top1_same = 0_usize;
        let mut unstable = 0_usize;
        let mut rank_stddevs = Vec::<f64>::new();
        for label in &target_labels {
            let series = dataset_runs
                .iter()
                .filter_map(|dataset| dataset.targets.iter().find(|target| &target.label == label))
                .collect::<Vec<_>>();
            let first_top1 = series.first().and_then(|target| target.top1_text.clone());
            let same_top1 = series.iter().all(|target| target.top1_text == first_top1);
            if same_top1 {
                top1_same += 1;
            } else {
                unstable += 1;
            }
            let ranks = series
                .iter()
                .map(|target| {
                    target
                        .best_rank
                        .map_or(MISSING_RANK_SENTINEL, |rank| rank as f64)
                })
                .collect::<Vec<_>>();
            if let Some(value) = stddev(&ranks) {
                rank_stddevs.push(value);
            }
        }

        let target_count = target_labels.len().max(1);
        per_dataset.push(StabilityDatasetSummary {
            dataset: dataset_name,
            repeats: dataset_runs.len(),
            all_hashes_identical: identical,
            top1_agreement_ratio: top1_same as f64 / target_count as f64,
            mean_rank_stddev: mean(&rank_stddevs),
            unstable_targets: unstable,
        });
    }

    Ok(StabilitySummary {
        repeats: repeats.len(),
        all_hashes_identical,
        per_dataset,
    })
}

#[inline]
pub fn filter_full_name_only(entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .filter(|entry| classify_name_family(entry) == "plain_multi_token")
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
}

#[inline]
pub fn filter_no_comma_single(entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .filter(|entry| {
            !matches!(
                classify_name_family(entry).as_str(),
                "comma" | "single_token"
            )
        })
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
}

#[inline]
pub fn build_hard_negative_full_name_dictionary(
    variant: &VariantSummary,
    error_limit_pt: f32,
    entry_limit: usize,
) -> Vec<String> {
    let mut scored = BTreeMap::<String, f32>::new();
    let mut targets = BTreeSet::<String>::new();
    for dataset in &variant.datasets {
        for target in &dataset.targets {
            targets.insert(target.target.trim().to_ascii_uppercase());
        }
        for row in &dataset.selected_rows {
            for candidate in &row.candidates {
                let text = candidate.text.trim();
                if text.is_empty() {
                    continue;
                }
                if classify_name_family(text) != "plain_multi_token" {
                    continue;
                }
                if candidate.error_pt > error_limit_pt {
                    continue;
                }
                let key = text.to_ascii_uppercase();
                let existing = scored.get(&key).copied().unwrap_or(f32::MAX);
                if candidate.error_pt < existing {
                    scored.insert(key, candidate.error_pt);
                }
            }
        }
    }

    let mut entries = scored.into_iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.1
            .partial_cmp(&right.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut out = targets.into_iter().collect::<Vec<_>>();
    for (text, _) in entries {
        if out.len() >= entry_limit {
            break;
        }
        if !out.contains(&text) {
            out.push(text);
        }
    }
    out
}

#[inline]
pub fn render_summary_markdown(
    summary: &crate::benchmarks::types::accuracy_benchmark_report_types::AccuracyBenchmarkSummary,
) -> String {
    let baseline = &summary.baseline;
    let mut out = String::new();
    out.push_str("# Accuracy Benchmark Report\n\n");
    out.push_str(
        "See `definitions.md` in the same directory for benchmark and signal definitions.\n\n",
    );
    out.push_str("## Baseline\n\n");
    out.push_str(&format!(
        "- MRR: {:.5}\n- Mean rank found: {}\n- Recall@20: {:.5}\n\n",
        baseline.overall.mrr,
        render_opt_f64(baseline.overall.mean_rank_found),
        baseline.overall.recall_at_20
    ));
    out.push_str("## Dictionary Ablation\n\n");
    out.push_str(&format!(
        "- Best by MRR: {}\n- Best by mean rank: {}\n\n",
        summary
            .dictionary_ablation
            .best_variant_by_mrr
            .as_deref()
            .unwrap_or("n/a"),
        summary
            .dictionary_ablation
            .best_variant_by_mean_rank
            .as_deref()
            .unwrap_or("n/a")
    ));
    for variant in &summary.dictionary_ablation.variants {
        out.push_str(&format!(
            "- `{}`: MRR {:.5}, mean rank {}\n",
            variant.variant,
            variant.overall.mrr,
            render_opt_f64(variant.overall.mean_rank_found)
        ));
    }
    out.push_str("\n## Signals\n\n");
    out.push_str(&format!(
        "- Candidate pool present: {}/{}\n- Top-20 reachable: {}\n- Perturbation changes at ±0.50pt: {}\n",
        summary.candidate_pool_quality.targets_present_in_pool,
        summary.candidate_pool_quality.targets_total,
        summary.candidate_pool_quality.targets_ranked_top_20,
        summary.perturbation_robustness.changed_at_050
    ));
    out.push_str(&format!(
        "- Same-family counterfactual improvements: {}\n- No-comma-single counterfactual improvements: {}\n",
        summary.best_possible_rank.improvable_by_same_family,
        summary.best_possible_rank.improvable_by_no_comma_single
    ));
    out.push_str("\n## Stability\n\n");
    out.push_str(&format!(
        "- Repeats: {}\n- All hashes identical: {}\n",
        summary.stability.repeats, summary.stability.all_hashes_identical
    ));
    for dataset in &summary.stability.per_dataset {
        out.push_str(&format!(
            "- `{}`: top1 agreement {:.3}, unstable targets {}, mean rank stddev {}\n",
            dataset.dataset,
            dataset.top1_agreement_ratio,
            dataset.unstable_targets,
            render_opt_f64(dataset.mean_rank_stddev)
        ));
    }
    if let Some(path) = &summary.anchor_span_visual_summary_path {
        out.push_str("\n## Visual Span Benchmark\n\n");
        out.push_str(&format!("- Summary: `{}`\n", path.display()));
    }
    out
}

#[inline]
pub fn render_definitions_markdown() -> String {
    let mut out = String::new();
    out.push_str("# Benchmark Definitions\n\n");
    out.push_str("This page defines each stage and signal emitted by the accuracy benchmark report. For every metric below, “better” means the direction that improves true guess ranking rather than just moving a number.\n\n");

    out.push_str("## Core Metrics\n\n");
    out.push_str("- `MRR`: Mean reciprocal rank across benchmark targets. Higher is better. `1.0` means every target is rank 1.\n");
    out.push_str("- `mean_rank_found`: Mean rank among found targets only. Lower is better.\n");
    out.push_str("- `recall@1`, `recall@5`, `recall@20`: Fraction of benchmark targets found within the top K final deduped candidates. Higher is better.\n");
    out.push_str("- `found_items`: Number of targets present in the final candidate pools. If this drops, ranking metrics become harder to interpret because generation may be failing.\n\n");

    out.push_str("## Stages\n\n");
    out.push_str("- `baseline`: The current real guesser behavior on the canonical known-redaction benchmark. This is the primary reference point for any change.\n");
    out.push_str("- `default_dictionary`: Reruns the benchmark with the repo default dictionary for all datasets. This checks whether any special benchmark dictionary is masking normal behavior.\n");
    out.push_str("- `full_name_only`: Uses a dictionary pruned to plain multi-token names. This is a diagnostic stage, not a product recommendation by itself. A large gain here means non-full-name candidate families are hurting ranking.\n");
    out.push_str("- `no_comma_single`: Uses a dictionary with comma-form and single-token candidates removed. This isolates the impact of shortened/compressed candidate families.\n");
    out.push_str("- `hard_negative_full_name_w2`: Uses an adversarial dictionary of plausible full-name distractors that were near the target width in the baseline pool, roughly within 2 pt. This is an overfitting detector.\n");
    out.push_str("- `hard_negative_full_name_w5`: Same as above, but with a broader 5 pt band. This is a looser stress test for ranking among plausible full names.\n");
    out.push_str("- `anchor_span_visual`: Runs the visual span benchmark and persists its own summary, rows, experiments, and crops. This diagnoses whether guess ranking is still being poisoned by anchor geometry.\n\n");

    out.push_str("## Signals\n\n");
    out.push_str("- `candidate_pool_quality`: Answers whether each benchmark target is present in the final pool, where it ranks, how many candidates beat it, and how many same-family candidates beat it. If a target is absent, ranking is not the first problem to solve.\n");
    out.push_str("- `family_composition`: Counts target, top-1, and candidate families using heuristic classes such as `plain_multi_token`, `comma`, `single_token`, `initial`, and `punctuation_heavy`. This is useful for spotting biased candidate pools.\n");
    out.push_str("- `best_possible_rank`: Counterfactual ceilings on the existing final pool. It reports how much improvement is theoretically available if ranking preferred same-family candidates, plain multi-token candidates, or a pool with comma/single removed. This does not prove a change is safe; it only shows headroom.\n");
    out.push_str("- `pairwise_winner_explanations`: For each target, explains why top-1 beat it using the final visible evidence. Reasons currently include `top1_lower_width_error`, `lexical_tiebreak`, `family_variant_width_tie`, `target_already_top1`, and `target_missing_from_pool`.\n");
    out.push_str("- `tie_density`: Counts how many candidates sit within fixed error thresholds of the target and top-1 (`0.05`, `0.10`, `0.25`, `0.50`, `1.00` pt). High tie density means ranking is fragile.\n");
    out.push_str("- `perturbation_robustness`: Re-scores the final pool after small target-width perturbations (`±0.25`, `±0.50`, `±1.00` pt). If top-1 flips easily, the ranker is unstable even if aggregate metrics look good.\n");
    out.push_str("- `stability`: Repeats the exact same benchmark run and checks whether hashes, top-1 answers, and per-target ranks are stable. If this is unstable, benchmark conclusions are weak.\n\n");

    out.push_str("## Interpretation Rules\n\n");
    out.push_str("- A gain in `baseline` is the strongest signal.\n");
    out.push_str("- A gain only in `full_name_only` or `no_comma_single` is evidence that candidate-family composition matters, but it does not automatically justify a runtime dictionary policy.\n");
    out.push_str("- A gain on normal baseline but not on `hard_negative_full_name_w2` or `w5` is a warning sign for overfitting to easy dictionary composition.\n");
    out.push_str("- If `candidate_pool_quality` shows the target missing, fix generation before tuning ranking.\n");
    out.push_str("- If targets are present but `pairwise_winner_explanations` and `tie_density` show dense near-ties, fix ranking rather than generation.\n");
    out.push_str("- If guess metrics are bad while `anchor_span_visual` is also bad, geometry may still be contaminating guess evaluation.\n");
    out.push_str("- If `perturbation_robustness` is weak, small measurement noise can flip winners; treat marginal gains carefully.\n");
    out.push_str("- If `stability` is not deterministic, do not trust small benchmark deltas.\n\n");

    out.push_str("## Caveats\n\n");
    out.push_str("- These signals are computed from the final visible guess pools unless explicitly noted otherwise. They do not fully explain candidates that were never generated or were removed before final output.\n");
    out.push_str("- Family labels are heuristic text-shape labels, not exact provenance from dictionary expansion.\n");
    out.push_str("- Counterfactual signals show possible ranking headroom, not guaranteed safe runtime changes.\n");
    out.push_str("- Adversarial dictionary stages are intended to flag overfitting, not to replace the canonical benchmark.\n");
    out
}

#[inline]
pub fn classify_name_family(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "empty".to_owned();
    }
    if trimmed.contains(',') {
        return "comma".to_owned();
    }
    let tokens = trimmed
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return "empty".to_owned();
    }
    let alpha_count = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .count();
    let punct_count = trimmed
        .chars()
        .filter(|ch| !ch.is_ascii_alphanumeric() && !ch.is_whitespace())
        .count();
    if punct_count > alpha_count {
        return "punctuation_heavy".to_owned();
    }
    if tokens.len() == 1 {
        let token = tokens[0];
        let letters = token.chars().filter(|ch| ch.is_ascii_alphabetic()).count();
        if letters <= 2 || token.ends_with('.') {
            return "initial".to_owned();
        }
        return "single_token".to_owned();
    }
    if tokens.iter().any(|token| {
        let letters = token.chars().filter(|ch| ch.is_ascii_alphabetic()).count();
        letters <= 1 || token.ends_with('.')
    }) {
        return "initial".to_owned();
    }
    "plain_multi_token".to_owned()
}

fn selected_rows_for_dataset(
    dataset: &KnownRedactionDataset,
    report: &GuessReport,
) -> Result<Vec<SelectedGuessRow>, String> {
    match dataset.row_selector {
        KnownRedactionRowSelector::PositionFromEnd {} => dataset
            .targets
            .iter()
            .map(|target| match target.selector {
                KnownRedactionTargetSelector::IndexFromEnd { index_from_end } => {
                    let guess = report
                        .guesses
                        .get(
                            report
                                .guesses
                                .len()
                                .checked_sub(index_from_end)
                                .ok_or_else(|| {
                                    format!(
                                        "dataset '{}' index_from_end={} is out of range",
                                        dataset.name, index_from_end
                                    )
                                })?,
                        )
                        .ok_or_else(|| {
                            format!(
                                "dataset '{}' target '{}' could not resolve row from end",
                                dataset.name, target.label
                            )
                        })?;
                    Ok(selected_guess_row(
                        &dataset.name,
                        format!("{}#{}", target_row_key(&dataset.name, guess), target.label),
                        guess,
                    ))
                }
                KnownRedactionTargetSelector::InPool {} => Err(format!(
                    "dataset '{}' uses InPool target with PositionFromEnd row selector",
                    dataset.name
                )),
            })
            .collect::<Result<Vec<_>, String>>(),
        KnownRedactionRowSelector::PageYRange {
            page_index,
            y0_min,
            y1_max,
        } => Ok(report
            .guesses
            .iter()
            .filter(|guess| {
                guess.page_index == page_index && guess.bbox.y0 >= y0_min && guess.bbox.y1 <= y1_max
            })
            .map(|guess| {
                selected_guess_row(&dataset.name, target_row_key(&dataset.name, guess), guess)
            })
            .collect::<Vec<_>>()),
    }
}

fn selected_guess_row(
    dataset_name: &str,
    row_key: String,
    guess: &RedactionGuess,
) -> SelectedGuessRow {
    SelectedGuessRow {
        row_key,
        dataset: dataset_name.to_owned(),
        page_index: guess.page_index,
        bbox: guess.bbox,
        candidates: guess.candidates.clone(),
        anchor_mode: guess.context.anchor_mode.clone(),
        target_width_pt: guess.context.target_width_pt,
        top1_text: guess
            .candidates
            .first()
            .map(|candidate| candidate.text.clone()),
    }
}

fn evaluate_targets(
    dataset: &KnownRedactionDataset,
    selected_rows: &[SelectedGuessRow],
) -> Result<Vec<TargetResult>, String> {
    dataset
        .targets
        .iter()
        .map(|target| match target.selector {
            KnownRedactionTargetSelector::IndexFromEnd { .. } => {
                let row = selected_rows
                    .iter()
                    .find(|row| row.row_key.ends_with(&format!("#{}", target.label)))
                    .or_else(|| {
                        selected_rows
                            .iter()
                            .find(|row| rank_in_selected_row(row, &target.target).is_some())
                    })
                    .or_else(|| selected_rows.first());
                Ok(target_result_from_row(
                    dataset.name.as_str(),
                    target,
                    row,
                    usize::from(row.is_some()),
                ))
            }
            KnownRedactionTargetSelector::InPool {} => {
                let mut best_rank = None::<usize>;
                let mut best_row = None::<&SelectedGuessRow>;
                for row in selected_rows {
                    let rank = rank_in_selected_row(row, &target.target);
                    if rank.is_some() && (best_rank.is_none() || rank < best_rank) {
                        best_rank = rank;
                        best_row = Some(row);
                    }
                }
                Ok(target_result_from_row(
                    dataset.name.as_str(),
                    target,
                    best_row,
                    selected_rows.len(),
                ))
            }
        })
        .collect::<Result<Vec<_>, String>>()
}

fn target_result_from_row(
    dataset_name: &str,
    target: &crate::benchmarks::types::known_redaction_contract::KnownRedactionTarget,
    row: Option<&SelectedGuessRow>,
    eligible_row_count: usize,
) -> TargetResult {
    let target_upper = normalized_text(&target.target);
    let present = row
        .map(|row| {
            row.candidates
                .iter()
                .any(|candidate| normalized_text(&candidate.text) == target_upper)
        })
        .unwrap_or(false);
    let candidate_count = row.map(|row| row.candidates.len());
    let best_rank = row.and_then(|row| rank_in_selected_row(row, &target.target));
    let (target_error_pt, top1_error_pt, top1_text, top1_family) = match row {
        Some(row) => {
            let target_candidate = row
                .candidates
                .iter()
                .find(|candidate| normalized_text(&candidate.text) == target_upper);
            let top1 = row.candidates.first();
            (
                target_candidate.map(|candidate| candidate.error_pt),
                top1.map(|candidate| candidate.error_pt),
                top1.map(|candidate| candidate.text.clone()),
                top1.map(|candidate| classify_name_family(&candidate.text)),
            )
        }
        None => (None, None, None, None),
    };
    TargetResult {
        dataset: dataset_name.to_owned(),
        label: target.label.clone(),
        target: target.target.clone(),
        best_rank,
        best_row_key: row.map(|row| row.row_key.clone()),
        eligible_row_count,
        present_in_pool: present,
        top1_text,
        top1_family,
        target_family: Some(classify_name_family(&target.target)),
        candidate_count,
        target_error_pt,
        top1_error_pt,
        anchor_mode: row.and_then(|row| row.anchor_mode.clone()),
    }
}

fn target_row_key(dataset_name: &str, guess: &RedactionGuess) -> String {
    format!(
        "{dataset_name}:page{}:{:.2}:{:.2}:{:.2}:{:.2}",
        guess.page_index, guess.bbox.x0, guess.bbox.y0, guess.bbox.x1, guess.bbox.y1
    )
}

fn summarize_ranks(ranks: &[Option<usize>]) -> BenchmarkSummary {
    let evaluated_items = ranks.len();
    let found = ranks.iter().filter_map(|rank| *rank).collect::<Vec<_>>();
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
            .map(|rank| rank.map_or(0.0_f64, |value| 1.0_f64 / value as f64))
            .sum::<f64>()
            / evaluated_items as f64
    };
    let mean_rank_found = if found.is_empty() {
        None
    } else {
        Some(found.iter().map(|rank| *rank as f64).sum::<f64>() / found.len() as f64)
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

fn summarize_candidate_rows(rows: &[SelectedGuessRow]) -> CandidateSummary {
    let counts = rows
        .iter()
        .map(|row| row.candidates.len() as f64)
        .collect::<Vec<_>>();
    let mut sorted = counts.clone();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    CandidateSummary {
        rows_total: rows.len(),
        rows_with_candidates: rows.iter().filter(|row| !row.candidates.is_empty()).count(),
        mean_count: mean(&counts),
        median_count: percentile_sorted(&sorted, 0.5_f64),
        p90_count: percentile_sorted(&sorted, 0.9_f64),
    }
}

fn summarize_quality(rows: &[SelectedGuessRow]) -> QualitySummary {
    let mut out = QualitySummary {
        rows_total: rows.len(),
        ..QualitySummary::default()
    };
    for row in rows {
        if row.anchor_mode.is_some() {
            out.anchored_rows += 1;
        }
        match row.anchor_mode.as_deref() {
            Some("two_sided") => out.anchor_two_sided_rows += 1,
            Some("left_only") | Some("right_only") => out.anchor_one_sided_rows += 1,
            _ => {}
        }
    }
    out
}

fn same_family_better_count(row: &SelectedGuessRow, target: &str) -> Option<usize> {
    let target_rank = rank_in_selected_row(row, target)?;
    let target_family = classify_name_family(target);
    Some(
        row.candidates
            .iter()
            .take(target_rank.saturating_sub(1))
            .filter(|candidate| classify_name_family(&candidate.text) == target_family)
            .count(),
    )
}

fn build_pairwise_reason(
    dataset: &VariantDatasetResult,
    target: &TargetResult,
) -> PairwiseWinnerExplanation {
    let row = target.best_row_key.as_ref().and_then(|row_key| {
        dataset
            .selected_rows
            .iter()
            .find(|row| &row.row_key == row_key)
    });
    let (reason, error_delta_pt, top1_family) = match row {
        None => ("target_missing_from_pool".to_owned(), None, None),
        Some(row) => {
            let top1 = row.candidates.first();
            let target_candidate = row.candidates.iter().find(|candidate| {
                normalized_text(&candidate.text) == normalized_text(&target.target)
            });
            match (top1, target_candidate) {
                (_, None) => ("target_missing_from_pool".to_owned(), None, None),
                (Some(top1), Some(_target_candidate))
                    if normalized_text(&top1.text) == normalized_text(&target.target) =>
                {
                    (
                        "target_already_top1".to_owned(),
                        Some(0.0_f32),
                        Some(classify_name_family(&top1.text)),
                    )
                }
                (Some(top1), Some(target_candidate)) => {
                    let delta = target_candidate.error_pt - top1.error_pt;
                    let reason = if delta > 0.0001_f32 {
                        "top1_lower_width_error"
                    } else if (delta).abs() <= 0.0001_f32
                        && normalized_text(&top1.text) < normalized_text(&target.target)
                    {
                        "lexical_tiebreak"
                    } else if matches!(
                        (
                            classify_name_family(&top1.text).as_str(),
                            classify_name_family(&target.target).as_str()
                        ),
                        ("comma" | "single_token", "plain_multi_token")
                    ) && delta.abs() <= 0.25_f32
                    {
                        "family_variant_width_tie"
                    } else {
                        "other_ranking_advantage"
                    };
                    (
                        reason.to_owned(),
                        Some(delta),
                        Some(classify_name_family(&top1.text)),
                    )
                }
                _ => ("target_missing_from_pool".to_owned(), None, None),
            }
        }
    };
    PairwiseWinnerExplanation {
        dataset: target.dataset.clone(),
        label: target.label.clone(),
        target: target.target.clone(),
        row_key: target.best_row_key.clone(),
        reason,
        top1_text: target.top1_text.clone(),
        target_error_pt: target.target_error_pt,
        top1_error_pt: target.top1_error_pt,
        error_delta_pt,
        top1_family,
        target_family: target.target_family.clone(),
    }
}

fn build_tie_density_row(target: &TargetResult, row: Option<&SelectedGuessRow>) -> TieDensityRow {
    let target_candidate = row.and_then(|row| {
        row.candidates
            .iter()
            .find(|candidate| normalized_text(&candidate.text) == normalized_text(&target.target))
    });
    let top1 = row.and_then(|row| row.candidates.first());
    TieDensityRow {
        dataset: target.dataset.clone(),
        label: target.label.clone(),
        target: target.target.clone(),
        row_key: target.best_row_key.clone(),
        within_target_005: count_within_threshold(
            row,
            target_candidate.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[0],
        ),
        within_target_010: count_within_threshold(
            row,
            target_candidate.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[1],
        ),
        within_target_025: count_within_threshold(
            row,
            target_candidate.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[2],
        ),
        within_target_050: count_within_threshold(
            row,
            target_candidate.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[3],
        ),
        within_target_100: count_within_threshold(
            row,
            target_candidate.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[4],
        ),
        within_top1_005: count_within_threshold(
            row,
            top1.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[0],
        ),
        within_top1_010: count_within_threshold(
            row,
            top1.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[1],
        ),
        within_top1_025: count_within_threshold(
            row,
            top1.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[2],
        ),
        within_top1_050: count_within_threshold(
            row,
            top1.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[3],
        ),
        within_top1_100: count_within_threshold(
            row,
            top1.map(|candidate| candidate.error_pt),
            TIE_THRESHOLDS_PT[4],
        ),
    }
}

fn build_perturbation_row(
    target: &TargetResult,
    row: Option<&SelectedGuessRow>,
) -> PerturbationRobustnessRow {
    let baseline_top1 = row
        .and_then(|row| row.candidates.first())
        .map(|candidate| candidate.text.clone());
    let baseline_for_compare = baseline_top1.clone();
    let changed = |delta: f32| {
        row.and_then(|row| perturbed_top1(row, delta))
            .map(|top1| Some(top1) != baseline_for_compare)
            .unwrap_or(false)
            || row
                .and_then(|row| perturbed_top1(row, -delta))
                .map(|top1| Some(top1) != baseline_for_compare)
                .unwrap_or(false)
    };
    PerturbationRobustnessRow {
        dataset: target.dataset.clone(),
        label: target.label.clone(),
        target: target.target.clone(),
        row_key: target.best_row_key.clone(),
        baseline_top1,
        changed_at_025: changed(0.25_f32),
        changed_at_050: changed(0.50_f32),
        changed_at_100: changed(1.0_f32),
    }
}

fn perturbed_top1(row: &SelectedGuessRow, delta: f32) -> Option<String> {
    let perturbed_target = row.target_width_pt + delta;
    let mut candidates = row.candidates.iter().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let left_error = (left.width_pt - perturbed_target).abs();
        let right_error = (right.width_pt - perturbed_target).abs();
        left_error
            .partial_cmp(&right_error)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| normalized_text(&left.text).cmp(&normalized_text(&right.text)))
    });
    candidates.first().map(|candidate| candidate.text.clone())
}

fn rank_in_selected_row(row: &SelectedGuessRow, target: &str) -> Option<usize> {
    rank_in_row(row, target, |_| true)
}

fn rank_in_row<F>(row: &SelectedGuessRow, target: &str, filter: F) -> Option<usize>
where
    F: Fn(&GuessCandidate) -> bool,
{
    let target_upper = normalized_text(target);
    row.candidates
        .iter()
        .filter(|candidate| filter(candidate))
        .enumerate()
        .find(|(_, candidate)| normalized_text(&candidate.text) == target_upper)
        .map(|(index, _)| index + 1)
}

fn normalized_text(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn count_within_threshold(
    row: Option<&SelectedGuessRow>,
    baseline_error: Option<f32>,
    threshold: f32,
) -> Option<usize> {
    let baseline = baseline_error?;
    let row = row?;
    Some(
        row.candidates
            .iter()
            .filter(|candidate| (candidate.error_pt - baseline).abs() <= threshold)
            .count(),
    )
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
    let center = mean(values)?;
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

fn increment_count(map: &mut BTreeMap<String, usize>, key: String) {
    *map.entry(key).or_insert(0) += 1;
}

fn counts_to_sorted_vec(map: BTreeMap<String, usize>) -> Vec<FamilyCount> {
    let mut values = map
        .into_iter()
        .map(|(family, count)| FamilyCount { family, count })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.family.cmp(&right.family))
    });
    values
}

fn hash_dataset(dataset: &VariantDatasetResult) -> Result<String, String> {
    let bytes = serde_json::to_vec(dataset).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn improves_rank(current: Option<usize>, candidate: Option<usize>) -> bool {
    match (current, candidate) {
        (Some(current), Some(candidate)) => candidate < current,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn option_f64_for_min(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::INFINITY)
}

fn render_opt_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.5}"))
        .unwrap_or_else(|| "n/a".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        build_hard_negative_full_name_dictionary, classify_name_family,
        render_definitions_markdown, selected_guess_row,
    };
    use crate::benchmarks::types::accuracy_benchmark_report_types::{
        BenchmarkSummary, CandidateSummary, QualitySummary, SelectedGuessRow, TargetResult,
        VariantDatasetResult, VariantSummary,
    };
    use crate::types::guess_types::{GuessCandidate, GuessContext, RedactionGuess};
    use crate::types::redaction_types::Rect;

    fn sample_row(texts: &[(&str, f32)]) -> SelectedGuessRow {
        selected_guess_row(
            "dataset",
            "dataset:row".to_owned(),
            &RedactionGuess {
                page_index: 0,
                bbox: Rect::new(0.0, 0.0, 10.0, 10.0),
                candidates: texts
                    .iter()
                    .map(|(text, error_pt)| GuessCandidate {
                        text: (*text).to_owned(),
                        width_pt: 10.0,
                        glyph_width_sum_pt: 10.0,
                        char_spacing_total_pt: 0.0,
                        word_spacing_total_pt: 0.0,
                        predicted_left_edge_x_pt: None,
                        predicted_right_edge_x_pt: None,
                        actual_right_edge_x_pt: None,
                        target_width_pt: 10.0,
                        error_pt: *error_pt,
                        normalized_error: Some(*error_pt),
                    })
                    .collect::<Vec<_>>(),
                context: GuessContext {
                    anchor_mode: Some("two_sided".to_owned()),
                    usable_left_edge_x_pt: None,
                    usable_right_edge_x_pt: None,
                    target_width_pt: 10.0,
                    font_key: None,
                    font_name: None,
                    base_font: None,
                    font_size_pt: None,
                    h_scale_pct: None,
                    char_spacing_pt: None,
                    word_spacing_pt: None,
                    width_source: None,
                    encoding_source: None,
                },
            },
        )
    }

    #[test]
    fn classify_name_family_covers_core_shapes() {
        assert_eq!(classify_name_family("SARAH KELLEN"), "plain_multi_token");
        assert_eq!(classify_name_family("PODESTA, TONY"), "comma");
        assert_eq!(classify_name_family("EPSTEIN"), "single_token");
        assert_eq!(classify_name_family("J. DOE"), "initial");
        assert_eq!(classify_name_family(": )"), "punctuation_heavy");
    }

    #[test]
    fn hard_negative_dictionary_keeps_targets_and_plain_multi_token_candidates() {
        let stage = VariantSummary {
            name: "baseline".to_owned(),
            overall: BenchmarkSummary {
                evaluated_items: 1,
                found_items: 1,
                recall_at_1: 1.0,
                recall_at_5: 1.0,
                recall_at_20: 1.0,
                mrr: 1.0,
                mean_rank_found: Some(1.0_f64),
            },
            datasets: vec![VariantDatasetResult {
                name: "dataset".to_owned(),
                summary: BenchmarkSummary {
                    evaluated_items: 1,
                    found_items: 1,
                    recall_at_1: 1.0,
                    recall_at_5: 1.0,
                    recall_at_20: 1.0,
                    mrr: 1.0,
                    mean_rank_found: Some(1.0_f64),
                },
                candidate_summary: CandidateSummary {
                    rows_total: 1,
                    rows_with_candidates: 1,
                    mean_count: Some(3.0_f64),
                    median_count: Some(3.0_f64),
                    p90_count: Some(3.0_f64),
                },
                quality_summary: QualitySummary {
                    rows_total: 1,
                    anchored_rows: 1,
                    anchor_two_sided_rows: 1,
                    anchor_one_sided_rows: 0,
                },
                targets: vec![TargetResult {
                    dataset: "dataset".to_owned(),
                    label: "target".to_owned(),
                    target: "SARAH KELLEN".to_owned(),
                    best_rank: Some(1),
                    best_row_key: Some("dataset:row".to_owned()),
                    eligible_row_count: 1,
                    present_in_pool: true,
                    top1_text: Some("SARAH KELLEN".to_owned()),
                    top1_family: Some("plain_multi_token".to_owned()),
                    target_family: Some("plain_multi_token".to_owned()),
                    candidate_count: Some(3),
                    target_error_pt: Some(0.1),
                    top1_error_pt: Some(0.1),
                    anchor_mode: Some("two_sided".to_owned()),
                }],
                selected_rows: vec![sample_row(&[
                    ("SARAH KELLEN", 0.1),
                    ("LES WEXNER", 0.2),
                    ("PODESTA, TONY", 0.2),
                ])],
            }],
        };
        let out = build_hard_negative_full_name_dictionary(&stage, 0.5, 10);
        assert!(out.contains(&"SARAH KELLEN".to_owned()));
        assert!(out.contains(&"LES WEXNER".to_owned()));
        assert!(!out.contains(&"PODESTA, TONY".to_owned()));
    }

    #[test]
    fn definitions_markdown_includes_core_sections() {
        let out = render_definitions_markdown();
        assert!(out.contains("# Benchmark Definitions"));
        assert!(out.contains("## Core Metrics"));
        assert!(out.contains("## Stages"));
        assert!(out.contains("## Signals"));
        assert!(out.contains("## Interpretation Rules"));
    }
}

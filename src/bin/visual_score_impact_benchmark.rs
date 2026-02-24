use clap::Parser;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use unredact::service::tooling_entry::{
    collect_underlying_text_hits_by_page, load_dictionary_from_bytes, run_guess_from_redactions,
    ToolingGuessRequest,
};
use unredact::types::guess_types::{GuessConfig, GuessReport, RedactionGuess};
use unredact::types::redaction_types::{
    Rect, RedactionKind, RedactionOccurrence, RedactionReport, UnderlyingTextHit,
};

const SAME_LINE_DELTA_PT: f32 = 3.0_f32;
const MIN_TARGET_WORD_LEN: usize = 4;
const MAX_TARGET_WORD_LEN: usize = 20;
const TARGET_OVERLAP_RATIO_MIN: f32 = 0.20_f32;
const MAX_TARGETS_PER_TRIAL: usize = 40;

#[derive(Debug, Parser)]
#[command(name = "visual_score_impact_benchmark")]
#[command(about = "Measure ranking impact of visual scoring via randomized synthetic redactions.")]
struct CliOptions {
    #[arg(
        long,
        default_value = "test_data/EFTA00038617.pdf",
        help = "Path to source PDF used for randomized synthetic redactions."
    )]
    input: PathBuf,
    #[arg(
        long,
        default_value_t = 2_u32,
        help = "1-based page number to sample targets from."
    )]
    page: u32,
    #[arg(
        long,
        default_value_t = 10_usize,
        help = "Number of randomized trials to run."
    )]
    trials: usize,
    #[arg(
        long,
        default_value_t = 10_usize,
        help = "Number of random targets per trial."
    )]
    targets_per_trial: usize,
    #[arg(
        long,
        default_value_t = 8_000_usize,
        help = "Maximum dictionary size (after normalization)."
    )]
    dictionary_size: usize,
    #[arg(
        long,
        default_value_t = 0xD1CE_BA5E_u64,
        help = "Seed for deterministic random target selection."
    )]
    seed: u64,
    #[arg(
        long,
        default_value = "benchmark/visual_score_impact.json",
        help = "Output path for JSON results."
    )]
    out: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct TrialSummary {
    trial_index: usize,
    sampled_targets: usize,
    no_visual: RankSummary,
    visual: RankSummary,
    pairwise: PairwiseSummary,
}

#[derive(Debug, Clone, Serialize)]
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
struct PairwiseSummary {
    visual_better: usize,
    visual_worse: usize,
    tie: usize,
    visual_found_when_no_visual_missed: usize,
    no_visual_found_when_visual_missed: usize,
    mean_rank_delta_visual_minus_no_visual: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct VisualScoreImpactReport {
    input_pdf: String,
    page_number: u32,
    trials: usize,
    targets_per_trial: usize,
    dictionary_size: usize,
    seed: u64,
    candidate_pool_size: usize,
    no_visual_overall: RankSummary,
    visual_overall: RankSummary,
    pairwise_overall: PairwiseSummary,
    trial_summaries: Vec<TrialSummary>,
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
        eprintln!("visual score impact benchmark failed: {error}");
        std::process::exit(1);
    }
}

fn run(options: CliOptions) -> Result<(), String> {
    if !options.input.exists() {
        return Err(format!("missing input pdf {}", options.input.display()));
    }
    if options.trials == 0 {
        return Err("trials must be positive".to_owned());
    }
    let targets_per_trial = options
        .targets_per_trial
        .clamp(1_usize, MAX_TARGETS_PER_TRIAL);
    let page_index = options.page.saturating_sub(1);

    let pdf_bytes = std::fs::read(&options.input)
        .map_err(|error| format!("failed to read {}: {error}", options.input.display()))?;
    let hits_by_page = collect_hits_by_page(&pdf_bytes)?;
    let page_hits = hits_by_page
        .get(&page_index)
        .ok_or_else(|| format!("page {} has no text hits", options.page))?;
    let page_word_hits = extract_word_hits(page_index, page_hits);
    let candidate_pool = build_target_candidates(page_index, &page_word_hits);
    if candidate_pool.len() < targets_per_trial {
        return Err(format!(
            "not enough target candidates on page {}: have {}, need {}",
            options.page,
            candidate_pool.len(),
            targets_per_trial
        ));
    }

    let dictionary_words = build_large_dictionary(
        &candidate_pool,
        &hits_by_page,
        options.dictionary_size,
        options.seed,
    );
    let dictionary_raw = dictionary_words.join("\n");
    let dictionary_inputs = load_dictionary_from_bytes(Some(dictionary_raw.as_bytes()))?;
    let dictionary = dictionary_inputs.dictionary;
    let dictionary_diagnostics = dictionary_inputs.diagnostics;

    let mut rng = LcgRng::new(options.seed);
    let mut all_no_visual_ranks = Vec::<Option<usize>>::new();
    let mut all_visual_ranks = Vec::<Option<usize>>::new();
    let mut trial_summaries = Vec::<TrialSummary>::new();

    for trial_index in 0..options.trials {
        let sampled = sample_trial_targets(&candidate_pool, targets_per_trial, &mut rng);
        if sampled.is_empty() {
            return Err(format!(
                "failed to sample targets for trial {} from pool size {}",
                trial_index + 1,
                candidate_pool.len()
            ));
        }
        let redaction_report = build_synthetic_redaction_report(&options.input, &sampled);
        let no_visual_report = run_guess_report(
            &options.input,
            &pdf_bytes,
            &redaction_report,
            &dictionary,
            &dictionary_diagnostics,
            false,
        )?;
        let visual_report = run_guess_report(
            &options.input,
            &pdf_bytes,
            &redaction_report,
            &dictionary,
            &dictionary_diagnostics,
            true,
        )?;

        let no_visual_ranks = sampled
            .iter()
            .map(|target| best_rank_for_target(&no_visual_report.guesses, target))
            .collect::<Vec<_>>();
        let visual_ranks = sampled
            .iter()
            .map(|target| best_rank_for_target(&visual_report.guesses, target))
            .collect::<Vec<_>>();

        all_no_visual_ranks.extend(no_visual_ranks.iter().copied());
        all_visual_ranks.extend(visual_ranks.iter().copied());

        trial_summaries.push(TrialSummary {
            trial_index: trial_index + 1,
            sampled_targets: sampled.len(),
            no_visual: summarize_ranks(&no_visual_ranks),
            visual: summarize_ranks(&visual_ranks),
            pairwise: summarize_pairwise(&no_visual_ranks, &visual_ranks),
        });
    }

    let no_visual_overall = summarize_ranks(&all_no_visual_ranks);
    let visual_overall = summarize_ranks(&all_visual_ranks);
    let pairwise_overall = summarize_pairwise(&all_no_visual_ranks, &all_visual_ranks);
    let report = VisualScoreImpactReport {
        input_pdf: options.input.display().to_string(),
        page_number: options.page,
        trials: options.trials,
        targets_per_trial,
        dictionary_size: options.dictionary_size,
        seed: options.seed,
        candidate_pool_size: candidate_pool.len(),
        no_visual_overall,
        visual_overall,
        pairwise_overall,
        trial_summaries,
    };

    print_report_summary(&report);
    if let Some(parent) = options.out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                )
            })?;
        }
    }
    let encoded = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("failed to encode benchmark json: {error}"))?;
    std::fs::write(&options.out, encoded)
        .map_err(|error| format!("failed to write {}: {error}", options.out.display()))?;
    println!("wrote {}", options.out.display());
    Ok(())
}

fn collect_hits_by_page(pdf_bytes: &[u8]) -> Result<BTreeMap<u32, Vec<UnderlyingTextHit>>, String> {
    collect_underlying_text_hits_by_page(pdf_bytes)
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
    if !out
        .chars()
        .all(|ch| ch.is_ascii_alphabetic() || ch == '\'' || ch == '-')
    {
        return None;
    }
    Some(out.to_ascii_uppercase())
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
        } else if !token.is_empty() {
            out.push((token.clone(), start, idx));
            token.clear();
        }
    }
    if !token.is_empty() {
        out.push((token, start, text.chars().count()));
    }
    out
}

fn center_y(rect: &Rect) -> f32 {
    (rect.y0 + rect.y1) * 0.5_f32
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
            *frequencies.entry(text).or_insert(0_usize) += 1;
        }
    }
    let mut out = Vec::<TargetCandidate>::new();
    for idx in 1..ordered.len().saturating_sub(1) {
        let hit = &ordered[idx];
        let Some(text) = canonical_word(&hit.text) else {
            continue;
        };
        if frequencies.get(&text).copied().unwrap_or(0_usize) != 1_usize {
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
        let pad_x = (height * 0.15_f32).clamp(0.6_f32, 1.8_f32);
        let pad_y = (height * 0.12_f32).clamp(0.4_f32, 1.4_f32);
        let bbox = Rect::new(
            hit.bbox.x0 - pad_x,
            hit.bbox.y0 - pad_y,
            hit.bbox.x1 + pad_x,
            hit.bbox.y1 + pad_y,
        );
        out.push(TargetCandidate {
            page_index,
            text,
            bbox,
            left_context: left.clone(),
            right_context: right.clone(),
        });
    }
    out
}

fn build_large_dictionary(
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
            let ch = (b'A' + (rng.next_u64() % 26_u64) as u8) as char;
            token.push(ch);
        }
        set.insert(token);
    }
    set.into_iter().collect::<Vec<_>>()
}

fn sample_trial_targets(
    pool: &[TargetCandidate],
    desired: usize,
    rng: &mut LcgRng,
) -> Vec<TargetCandidate> {
    let mut indices = (0..pool.len()).collect::<Vec<_>>();
    rng.shuffle(&mut indices);
    let mut chosen = Vec::<TargetCandidate>::new();
    let mut used_text = BTreeSet::<String>::new();
    for idx in indices {
        let candidate = &pool[idx];
        if !used_text.insert(candidate.text.clone()) {
            continue;
        }
        if chosen
            .iter()
            .any(|existing| rects_overlap(existing.bbox, candidate.bbox))
        {
            continue;
        }
        chosen.push(candidate.clone());
        if chosen.len() >= desired {
            break;
        }
    }
    chosen
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

fn run_guess_report(
    input_path: &Path,
    pdf_bytes: &[u8],
    redactions: &RedactionReport,
    dictionary: &[String],
    dictionary_diagnostics: &[String],
    visual_score: bool,
) -> Result<GuessReport, String> {
    let cfg = GuessConfig {
        visual_score,
        visual_score_dpi: 200.0_f32,
    };
    run_guess_from_redactions(ToolingGuessRequest {
        input_name: &input_path.to_string_lossy(),
        pdf_bytes,
        redactions,
        dictionary,
        dictionary_diagnostics,
        guess: &cfg,
    })
}

fn overlap_ratio(a: Rect, b: Rect) -> f32 {
    let overlap_w = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0_f32);
    let overlap_h = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0_f32);
    let overlap_area = overlap_w * overlap_h;
    let target_area = b.area().max(0.0001_f32);
    overlap_area / target_area
}

fn best_rank_for_target(guesses: &[RedactionGuess], target: &TargetCandidate) -> Option<usize> {
    let mut best = None::<usize>;
    for guess in guesses {
        if guess.page_index != target.page_index {
            continue;
        }
        if overlap_ratio(guess.bbox, target.bbox) < TARGET_OVERLAP_RATIO_MIN {
            continue;
        }
        if let Some(rank) = rank_in_guess(guess, &target.text) {
            best = match best {
                Some(current) => Some(current.min(rank)),
                None => Some(rank),
            };
        }
    }
    best
}

fn rank_in_guess(guess: &RedactionGuess, target: &str) -> Option<usize> {
    let target_upper = target.trim().to_ascii_uppercase();
    if target_upper.is_empty() {
        return None;
    }
    let mut rank = 1_usize;
    let mut seen = std::collections::BTreeSet::<String>::new();
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
        let hits = found.iter().filter(|rank| **rank <= k).count();
        hits as f64 / evaluated_items as f64
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

fn summarize_pairwise(no_visual: &[Option<usize>], visual: &[Option<usize>]) -> PairwiseSummary {
    let mut visual_better = 0_usize;
    let mut visual_worse = 0_usize;
    let mut tie = 0_usize;
    let mut visual_found_when_no_visual_missed = 0_usize;
    let mut no_visual_found_when_visual_missed = 0_usize;
    let mut deltas = Vec::<f64>::new();
    for (left, right) in no_visual.iter().zip(visual.iter()) {
        match (left, right) {
            (Some(no_rank), Some(vis_rank)) => {
                if vis_rank < no_rank {
                    visual_better += 1;
                } else if vis_rank > no_rank {
                    visual_worse += 1;
                } else {
                    tie += 1;
                }
                deltas.push(*vis_rank as f64 - *no_rank as f64);
            }
            (None, Some(_)) => {
                visual_better += 1;
                visual_found_when_no_visual_missed += 1;
            }
            (Some(_), None) => {
                visual_worse += 1;
                no_visual_found_when_visual_missed += 1;
            }
            (None, None) => tie += 1,
        }
    }
    let mean_rank_delta_visual_minus_no_visual = if deltas.is_empty() {
        None
    } else {
        Some(deltas.iter().sum::<f64>() / deltas.len() as f64)
    };
    PairwiseSummary {
        visual_better,
        visual_worse,
        tie,
        visual_found_when_no_visual_missed,
        no_visual_found_when_visual_missed,
        mean_rank_delta_visual_minus_no_visual,
    }
}

fn print_report_summary(report: &VisualScoreImpactReport) {
    println!("Visual Score Impact Benchmark");
    println!("input: {}", report.input_pdf);
    println!(
        "page={} trials={} targets_per_trial={} dictionary_size={} pool_size={} seed={}",
        report.page_number,
        report.trials,
        report.targets_per_trial,
        report.dictionary_size,
        report.candidate_pool_size,
        report.seed
    );
    println!(
        "NO_VISUAL   r@1={:.3} r@5={:.3} r@20={:.3} mrr={:.3} found={}/{} mean_rank={}",
        report.no_visual_overall.recall_at_1,
        report.no_visual_overall.recall_at_5,
        report.no_visual_overall.recall_at_20,
        report.no_visual_overall.mrr,
        report.no_visual_overall.found_items,
        report.no_visual_overall.evaluated_items,
        report
            .no_visual_overall
            .mean_rank_found
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".to_owned())
    );
    println!(
        "VISUAL      r@1={:.3} r@5={:.3} r@20={:.3} mrr={:.3} found={}/{} mean_rank={}",
        report.visual_overall.recall_at_1,
        report.visual_overall.recall_at_5,
        report.visual_overall.recall_at_20,
        report.visual_overall.mrr,
        report.visual_overall.found_items,
        report.visual_overall.evaluated_items,
        report
            .visual_overall
            .mean_rank_found
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".to_owned())
    );
    println!(
        "PAIRWISE    better={} worse={} tie={} found_gain={} found_loss={} mean_rank_delta(vis-no)={}",
        report.pairwise_overall.visual_better,
        report.pairwise_overall.visual_worse,
        report.pairwise_overall.tie,
        report.pairwise_overall.visual_found_when_no_visual_missed,
        report.pairwise_overall.no_visual_found_when_visual_missed,
        report
            .pairwise_overall
            .mean_rank_delta_visual_minus_no_visual
            .map(|value| format!("{value:.3}"))
            .unwrap_or_else(|| "-".to_owned())
    );
}

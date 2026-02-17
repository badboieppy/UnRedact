use serde::Serialize;
use std::path::{Path, PathBuf};

use unredact::service::unredact_entry::{run_from_paths, UnredactServiceConfig};
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
    targets: Vec<RankedTarget>,
}

#[derive(Debug, Clone, Serialize)]
struct AccuracyBenchmark {
    definitions: MetricDefinitions,
    datasets: Vec<DatasetResult>,
    overall: BenchmarkSummary,
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
}

fn parse_out_path() -> Result<PathBuf, String> {
    let mut out_path = PathBuf::from("benchmark/guess_accuracy.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => {
                let Some(path_value) = args.next() else {
                    return Err("missing value for --out".to_owned());
                };
                out_path = PathBuf::from(path_value);
            }
            "--help" | "-h" => {
                println!("Usage: cargo run --bin guess_accuracy_benchmark -- [--out <path>]");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    Ok(out_path)
}

fn benchmark_config() -> UnredactServiceConfig {
    UnredactServiceConfig {
        include_details: false,
        include_full_page_rects: false,
        enable_image_analysis: true,
        raster_dpi: 200.0_f32,
        guess: GuessConfig {
            max_words: 4,
            max_candidates: 2_000,
            max_dictionary: 5_000,
            tol_pt: 100.0,
            max_nodes: 200_000,
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

fn write_noisy_dictionary(path: &Path, targets: &[&str]) -> Result<(), String> {
    let mut lines = targets
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let target_set = targets
        .iter()
        .map(|value| value.to_ascii_uppercase())
        .collect::<std::collections::BTreeSet<_>>();

    for line in include_str!("../../assets/names.txt").lines() {
        let trimmed = line.trim();
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

fn evaluate_efta00101126(root: &Path) -> Result<DatasetResult, String> {
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
    Ok(DatasetResult {
        name: "EFTA00101126".to_owned(),
        summary: summarize_ranks(&ranks),
        targets,
    })
}

fn evaluate_efta00038617(root: &Path) -> Result<DatasetResult, String> {
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
    Ok(DatasetResult {
        name: "EFTA00038617".to_owned(),
        summary: summarize_ranks(&ranks),
        targets,
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
    }
}

fn main() {
    let out_path = match parse_out_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("argument error: {error}");
            std::process::exit(2);
        }
    };

    let benchmark_root = std::env::temp_dir().join(format!(
        "unredact_accuracy_benchmark_{}",
        std::process::id()
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
            eprintln!("benchmark failed for EFTA00101126: {error}");
            std::process::exit(1);
        }
    };
    let efta00038617 = match evaluate_efta00038617(&benchmark_root) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("benchmark failed for EFTA00038617: {error}");
            std::process::exit(1);
        }
    };

    let datasets = vec![efta00101126, efta00038617];
    let overall_ranks = datasets
        .iter()
        .flat_map(|dataset| dataset.targets.iter().map(|target| target.best_rank))
        .collect::<Vec<_>>();
    let overall = summarize_ranks(&overall_ranks);
    let definitions = metric_definitions();

    println!("Guess Accuracy Benchmark");
    println!("Metric definitions:");
    println!("  evaluated_items: {}", definitions.evaluated_items);
    println!("  found_items: {}", definitions.found_items);
    println!("  recall_at_1: {}", definitions.recall_at_1);
    println!("  recall_at_5: {}", definitions.recall_at_5);
    println!("  recall_at_20: {}", definitions.recall_at_20);
    println!("  mrr: {}", definitions.mrr);
    println!("  mean_rank_found: {}", definitions.mean_rank_found);
    println!("  best_rank: {}", definitions.best_rank);
    for dataset in &datasets {
        print_summary(&dataset.name, &dataset.summary);
    }
    print_summary("OVERALL", &overall);

    let payload = AccuracyBenchmark {
        definitions,
        datasets,
        overall,
    };
    if let Some(parent) = out_path.parent() {
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
    if let Err(error) = std::fs::write(&out_path, encoded) {
        eprintln!("failed to write {}: {error}", out_path.display());
        std::process::exit(1);
    }
    println!("wrote {}", out_path.display());
}

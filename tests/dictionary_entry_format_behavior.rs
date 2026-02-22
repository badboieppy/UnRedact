#![cfg(feature = "cli-entry")]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::{GuessConfig, GuessReport, RedactionGuess};
use unredact::types::visualizer_config::VisualizerConfig;

const TARGET_NAMES: [&str; 10] = [
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

const ALT_FORMAT_DICTIONARY_LINES: [&str; 10] = [
    "KELLEN, SARAH",
    "MUCINSKA, ADRIANA",
    "NADIA|MARCINKOVA",
    "MR.|LES|WEXNER",
    "GROFF, LESLEY",
    "BRUNEL, JEAN LUC JR.",
    "ROBSON, HALEY",
    "HAMMOND, WILLIAM",
    "RODGERS, DAVID",
    "DR. RICHARD BARNETT",
];

fn test_output_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "unredact_test_dictionary_entry_format_behavior_{}",
        std::process::id()
    ))
}

fn write_dictionary(path: &Path) {
    let mut lines = ALT_FORMAT_DICTIONARY_LINES
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    lines.extend(
        ["ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT"]
            .into_iter()
            .map(str::to_owned),
    );
    let write_result = std::fs::write(path, lines.join("\n"));
    assert!(
        write_result.is_ok(),
        "failed to write dictionary {}: {:?}",
        path.display(),
        write_result.err()
    );
}

fn load_report(path: &Path) -> GuessReport {
    let bytes_result = std::fs::read(path);
    assert!(
        bytes_result.is_ok(),
        "failed to read guesses report {}: {:?}",
        path.display(),
        bytes_result.err()
    );
    let report_result = serde_json::from_slice::<GuessReport>(
        &bytes_result.expect("guesses report bytes should exist"),
    );
    assert!(
        report_result.is_ok(),
        "failed to parse guesses report {}: {:?}",
        path.display(),
        report_result.err()
    );
    report_result.expect("guesses report should parse")
}

fn first_bullet_rows(report: &GuessReport) -> Vec<&RedactionGuess> {
    report
        .guesses
        .iter()
        .filter(|guess| guess.page_index == 1)
        .filter(|guess| guess.bbox.y0 >= 440.0_f32 && guess.bbox.y1 <= 505.0_f32)
        .collect::<Vec<_>>()
}

fn ordered_guess_texts_upper(guess: &RedactionGuess) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = BTreeSet::<String>::new();
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

fn target_tokens_upper(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>()
}

fn contains_all_tokens(candidate_upper: &str, target_tokens: &[String]) -> bool {
    target_tokens
        .iter()
        .all(|token| candidate_upper.contains(token))
}

fn pool_contains_target(pool: &BTreeSet<String>, target: &str) -> bool {
    let tokens = target_tokens_upper(target);
    pool.iter()
        .any(|candidate_upper| contains_all_tokens(candidate_upper, &tokens))
}

fn rank_in_guess_by_tokens(guess: &RedactionGuess, target: &str) -> Option<usize> {
    let target_tokens = target_tokens_upper(target);
    ordered_guess_texts_upper(guess)
        .iter()
        .position(|candidate_upper| contains_all_tokens(candidate_upper, &target_tokens))
        .map(|index| index + 1)
}

fn best_rank_in_rows(rows: &[&RedactionGuess], target: &str) -> Option<usize> {
    rows.iter()
        .filter_map(|row| rank_in_guess_by_tokens(row, target))
        .min()
}

#[test]
fn alternate_dictionary_entry_formats_are_honored_in_guesses() {
    let input = Path::new("test_data/EFTA00038617.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir();
    if output_dir.exists() {
        let remove_result = std::fs::remove_dir_all(&output_dir);
        assert!(
            remove_result.is_ok(),
            "failed to clean output dir {}: {:?}",
            output_dir.display(),
            remove_result.err()
        );
    }
    let create_result = std::fs::create_dir_all(&output_dir);
    assert!(
        create_result.is_ok(),
        "failed to create output dir {}: {:?}",
        output_dir.display(),
        create_result.err()
    );

    let dictionary_path = output_dir.join("alt_format_dictionary.txt");
    write_dictionary(&dictionary_path);

    let cfg = UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        raster_dpi: 200.0_f32,
        guess: GuessConfig {
            visual_score: true,
            visual_score_dpi: 200.0_f32,
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };

    let run_result = run_from_paths(input, &output_dir, Some(&dictionary_path), cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");
    let report = load_report(&outputs.guesses_path);
    let rows = first_bullet_rows(&report);
    assert!(
        rows.len() >= 10,
        "expected at least 10 first-bullet redactions on page 2, got {}",
        rows.len()
    );

    let mut pool = BTreeSet::<String>::new();
    for row in &rows {
        for value in ordered_guess_texts_upper(row) {
            pool.insert(value);
        }
    }

    let missing = TARGET_NAMES
        .iter()
        .copied()
        .filter(|target| !pool_contains_target(&pool, target))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing targets from first-bullet candidate pool: {:?}",
        missing
    );

    let ranks = TARGET_NAMES
        .iter()
        .map(|target| best_rank_in_rows(&rows, target))
        .collect::<Vec<_>>();
    let recall_at_20 = ranks
        .iter()
        .filter_map(|rank| *rank)
        .filter(|rank| *rank <= 20)
        .count() as f64
        / TARGET_NAMES.len() as f64;
    assert!(
        recall_at_20 >= 0.8_f64,
        "expected recall@20 >= 0.8 for alternate dictionary formats, got {:.3} (ranks={:?})",
        recall_at_20,
        ranks
    );
}

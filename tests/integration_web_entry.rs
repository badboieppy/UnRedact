#![cfg(feature = "web-entry")]

use std::path::Path;

use unredact::service::unredact_web_entry::{run, UnredactWebConfig, UnredactWebRequest};
use unredact::types::guess_types::{GuessConfig, GuessReport};
use unredact::types::visualizer_config::VisualizerConfig;

fn normalize_guess_text_for_exact_match(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_ascii_uppercase()
}

fn rank_in_guess(
    guess: &unredact::types::guess_types::RedactionGuess,
    target: &str,
) -> Option<usize> {
    let target = normalize_guess_text_for_exact_match(target);
    let mut ordered = Vec::<String>::new();
    for candidate in &guess.candidates {
        ordered.push(normalize_guess_text_for_exact_match(&candidate.text));
    }
    for value in &guess.exact_matches {
        ordered.push(normalize_guess_text_for_exact_match(value));
    }
    ordered
        .iter()
        .position(|value| value == &target)
        .map(|index| index + 1)
}

#[test]
fn web_entry_bytes_flow_matches_known_efta00101126_expectations() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());
    let pdf_bytes_result = std::fs::read(input);
    assert!(
        pdf_bytes_result.is_ok(),
        "failed to read input pdf {}: {:?}",
        input.display(),
        pdf_bytes_result.err()
    );
    let pdf_bytes = pdf_bytes_result.expect("pdf bytes should exist");

    let cfg = UnredactWebConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };

    let web_result = run(UnredactWebRequest {
        input_name: input.to_string_lossy().to_string(),
        pdf_bytes,
        dictionary_file_bytes: None,
        cfg,
    });
    assert!(
        web_result.is_ok(),
        "web pipeline run failed: {:?}",
        web_result.err()
    );
    let web_outputs = web_result.expect("web pipeline should succeed");
    let report_result = serde_json::from_slice::<GuessReport>(&web_outputs.guesses_json);
    assert!(
        report_result.is_ok(),
        "failed to decode web guesses json: {:?}",
        report_result.err()
    );
    let report = report_result.expect("web guesses should decode");

    assert!(
        report.guesses.len() >= 2,
        "expected at least 2 guesses, got {}",
        report.guesses.len()
    );

    let second_last = &report.guesses[report.guesses.len() - 2];
    let last = &report.guesses[report.guesses.len() - 1];
    assert!(
        second_last.context.has_anchor_pair,
        "second-to-last redaction should be guessable"
    );
    assert!(
        last.context.has_anchor_pair,
        "last redaction should be guessable"
    );
    let second_rank = rank_in_guess(second_last, "SARAH KELLEN");
    let last_rank = rank_in_guess(last, "SARAH KELLEN");
    assert!(
        matches!(second_rank, Some(rank) if rank <= 10),
        "expected second-to-last redaction to keep SARAH KELLEN within top 10, got {:?}",
        second_rank
    );
    assert!(
        matches!(last_rank, Some(rank) if rank <= 10),
        "expected last redaction to keep SARAH KELLEN within top 10, got {:?}",
        last_rank
    );
}

#![cfg(feature = "web-entry")]

use std::path::Path;

use unredact::service::unredact_web_entry::{run, UnredactWebConfig, UnredactWebRequest};
use unredact::types::guess_types::{GuessConfig, GuessReport};
use unredact::types::visualizer_config::VisualizerConfig;

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
        raster_dpi: 200.0_f32,
        guess: GuessConfig {
            visual_score: true,
            visual_score_dpi: 200.0_f32,
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
    assert_eq!(
        second_last
            .exact_matches
            .first()
            .map(|value| value.as_str()),
        Some("SARAH KELLEN"),
        "second-to-last top exact mismatch"
    );
    assert_eq!(
        last.exact_matches.first().map(|value| value.as_str()),
        Some("SARAH KELLEN"),
        "last top exact mismatch"
    );
}

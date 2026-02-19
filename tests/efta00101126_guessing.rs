use std::path::{Path, PathBuf};

use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::{GuessConfig, GuessReport};
use unredact::types::visualizer_config::VisualizerConfig;

fn test_output_dir() -> PathBuf {
    std::env::temp_dir().join(format!("unredact_test_efta00101126_{}", std::process::id()))
}

#[test]
fn efta00101126_last_two_redactions_include_sarah_kellen_uppercase() {
    let input = Path::new("test_data/EFTA00101126.pdf");
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

    let dictionary_path = Path::new("assets/names.txt");
    assert!(
        dictionary_path.exists(),
        "missing dictionary input: {}",
        dictionary_path.display()
    );

    let run_result = run_from_paths(input, &output_dir, Some(dictionary_path), cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");

    let bytes_result = std::fs::read(&outputs.guesses_path);
    assert!(
        bytes_result.is_ok(),
        "failed to read guesses report {}: {:?}",
        outputs.guesses_path.display(),
        bytes_result.err()
    );
    let bytes = bytes_result.expect("guesses report bytes should exist");
    let report_result = serde_json::from_slice::<GuessReport>(&bytes);
    assert!(
        report_result.is_ok(),
        "failed to parse guesses report {}: {:?}",
        outputs.guesses_path.display(),
        report_result.err()
    );
    let report = report_result.expect("guesses report should parse");

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

    let second_top_exact = second_last.exact_matches.first().map(|v| v.as_str());
    let last_top_exact = last.exact_matches.first().map(|v| v.as_str());
    assert_eq!(
        second_top_exact,
        Some("SARAH KELLEN"),
        "second-to-last top exact mismatch"
    );
    assert_eq!(
        last_top_exact,
        Some("SARAH KELLEN"),
        "last top exact mismatch"
    );
}

#![cfg(feature = "cli-entry")]

use std::path::Path;

use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::GuessConfig;
use unredact::types::redaction_types::RedactionKind;
use unredact::types::visualizer_config::VisualizerConfig;

mod common;
use common::{load_guess_report, load_redaction_report, test_output_dir};

fn smoke_cfg() -> UnredactServiceConfig {
    UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    }
}

#[test]
fn additional_epstein_files_run_without_file_specific_tuning() {
    let inputs = [
        Path::new("test_data/EFTA01083121.pdf"),
        Path::new("test_data/EFTA02238592.pdf"),
        Path::new("test_data/EFTA02717423.pdf"),
    ];
    let dictionary_path = Path::new("assets/names.txt");
    assert!(
        dictionary_path.exists(),
        "missing dictionary input: {}",
        dictionary_path.display()
    );

    let output_dir = test_output_dir("general_baseline");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));

    for input in inputs {
        assert!(input.exists(), "missing test input: {}", input.display());
        let outputs = run_from_paths(input, &output_dir, Some(dictionary_path), smoke_cfg())
            .unwrap_or_else(|error| panic!("pipeline run failed for {}: {error}", input.display()));
        let report = load_guess_report(&outputs.guesses_path);
        assert!(
            !report.guesses.is_empty(),
            "expected non-empty guesses for {}",
            input.display()
        );
    }
}

#[test]
fn fallback_dictionary_flow_runs_end_to_end() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir("general_fallback_dictionary");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));

    let outputs = run_from_paths(input, &output_dir, None, smoke_cfg())
        .unwrap_or_else(|error| panic!("pipeline run failed for {}: {error}", input.display()));
    let report = load_guess_report(&outputs.guesses_path);

    assert!(
        !report.guesses.is_empty(),
        "expected non-empty guesses for {}",
        input.display()
    );
    assert!(
        report
            .guesses
            .iter()
            .any(|guess| !guess.exact_matches.is_empty() || !guess.candidates.is_empty()),
        "expected at least one guess row with candidate output"
    );
}

#[test]
fn service_image_analysis_toggle_controls_raster_detection() {
    let input = Path::new("test_data/EFTA02238592.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let out_on = test_output_dir("general_image_on");
    let out_off = test_output_dir("general_image_off");
    std::fs::create_dir_all(&out_on)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", out_on.display()));
    std::fs::create_dir_all(&out_off)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", out_off.display()));

    let cfg_on = UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };
    let cfg_off = UnredactServiceConfig {
        enable_image_analysis: false,
        ..cfg_on.clone()
    };

    let outputs_on = run_from_paths(input, &out_on, None, cfg_on)
        .unwrap_or_else(|error| panic!("pipeline run with image analysis enabled failed: {error}"));
    let outputs_off = run_from_paths(input, &out_off, None, cfg_off).unwrap_or_else(|error| {
        panic!("pipeline run with image analysis disabled failed: {error}")
    });

    let report_on = load_redaction_report(&outputs_on.redactions_path);
    let report_off = load_redaction_report(&outputs_off.redactions_path);

    let has_raster_on = report_on
        .redactions
        .iter()
        .any(|value| matches!(value.kind, RedactionKind::RasterDarkRegion));
    let has_raster_off = report_off
        .redactions
        .iter()
        .any(|value| matches!(value.kind, RedactionKind::RasterDarkRegion));

    assert!(
        has_raster_on,
        "expected at least one raster dark region with image analysis enabled"
    );
    assert!(
        !has_raster_off,
        "expected no raster dark regions with image analysis disabled"
    );
}

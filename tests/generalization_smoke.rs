use std::path::{Path, PathBuf};

use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::{GuessConfig, GuessReport};
use unredact::types::visualizer_config::VisualizerConfig;

fn smoke_output_dir(tag: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "unredact_generalization_smoke_{}_{}_{}",
        tag,
        std::process::id(),
        stamp
    ))
}

fn load_report(path: &Path) -> GuessReport {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("failed to read guesses {}: {error}", path.display()));
    serde_json::from_slice::<GuessReport>(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse guesses {}: {error}", path.display()))
}

fn smoke_cfg() -> UnredactServiceConfig {
    UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        raster_dpi: 96.0_f32,
        guess: GuessConfig {
            visual_score: true,
            visual_score_dpi: 200.0_f32,
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

    let output_dir = smoke_output_dir("baseline");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));

    for input in inputs {
        assert!(input.exists(), "missing test input: {}", input.display());
        let outputs = run_from_paths(input, &output_dir, Some(dictionary_path), smoke_cfg())
            .unwrap_or_else(|error| panic!("pipeline run failed for {}: {error}", input.display()));
        let report = load_report(&outputs.guesses_path);
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

    let output_dir = smoke_output_dir("fallback_dictionary");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));

    let outputs = run_from_paths(input, &output_dir, None, smoke_cfg())
        .unwrap_or_else(|error| panic!("pipeline run failed for {}: {error}", input.display()));
    let report = load_report(&outputs.guesses_path);

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

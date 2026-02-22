#![cfg(feature = "cli-entry")]

use std::path::{Path, PathBuf};

use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::GuessConfig;
use unredact::types::redaction_types::{RedactionKind, RedactionReport};
use unredact::types::visualizer_config::VisualizerConfig;

fn output_dir(tag: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "unredact_raster_api_blackbox_{}_{}_{}",
        tag,
        std::process::id(),
        stamp
    ))
}

fn load_redactions(path: &Path) -> RedactionReport {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("failed to read redactions {}: {error}", path.display()));
    serde_json::from_slice::<RedactionReport>(&bytes).unwrap_or_else(|error| {
        panic!(
            "failed to parse redactions report {}: {error}",
            path.display()
        )
    })
}

#[test]
fn service_image_analysis_toggle_controls_raster_detection() {
    let input = Path::new("test_data/EFTA02238592.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let out_on = output_dir("image_on");
    let out_off = output_dir("image_off");
    std::fs::create_dir_all(&out_on)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", out_on.display()));
    std::fs::create_dir_all(&out_off)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", out_off.display()));

    let cfg_on = UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        raster_dpi: 96.0_f32,
        guess: GuessConfig {
            visual_score: true,
            visual_score_dpi: 200.0_f32,
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

    let report_on = load_redactions(&outputs_on.redactions_path);
    let report_off = load_redactions(&outputs_off.redactions_path);

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

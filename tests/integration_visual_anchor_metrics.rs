#![cfg(feature = "cli-entry")]

use std::path::Path;

use unredact::service::unredact_cli_entry::{
    run_from_paths_with_diagnostics, UnredactServiceConfig,
};
use unredact::service::visual_anchor_metrics_cli_entry::{
    run as run_visual_metrics, VisualAnchorMetricServiceRequest,
};
use unredact::types::guess_types::GuessConfig;
use unredact::types::visualizer_config::VisualizerConfig;

mod common;
use common::{load_visual_anchor_metrics_report, test_output_dir};

fn cfg() -> UnredactServiceConfig {
    UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig::default(),
        visualize: false,
        visualizer: VisualizerConfig::default(),
    }
}

fn row_by_id<'a>(
    report: &'a unredact::types::visual_anchor_metric_types::VisualAnchorMetricsReport,
    row_id: &str,
) -> &'a unredact::types::visual_anchor_metric_types::VisualAnchorMetricRow {
    report
        .rows
        .iter()
        .find(|row| row.row_id == row_id)
        .unwrap_or_else(|| panic!("missing visual metrics row {row_id}"))
}

#[test]
fn efta00101126_rows_show_invisible_current_anchors_and_tighter_visual_neighbors() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    let output_dir = test_output_dir("visual_anchor_metrics_efta00101126");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_from_paths_with_diagnostics(input, &output_dir, None, cfg(), true)
        .expect("diagnostics-enabled run should succeed");
    let report_path = outputs
        .visual_anchor_metrics_path
        .as_ref()
        .expect("visual metrics path should be present");
    let report = load_visual_anchor_metrics_report(report_path);

    for row_id in ["page7_row0", "page7_row1"] {
        let row = row_by_id(&report, row_id);
        assert!(
            row.current_left.as_ref().is_some_and(|side| side.visible),
            "expected visible current left anchor for {row_id}"
        );
        assert!(
            row.current_right.as_ref().is_some_and(|side| side.visible),
            "expected visible current right anchor for {row_id}"
        );
        let redaction_dark_width = row
            .redaction_dark_component
            .as_ref()
            .map(|component| component.width_pt)
            .expect("redaction dark component should exist");
        let redaction_box_width = row.width_comparison.redaction_box_width_pt;
        assert!(
            (redaction_dark_width - redaction_box_width).abs() <= 1.0_f32,
            "expected dark component width to be within 1pt of redaction box for {row_id}: dark={redaction_dark_width} box={redaction_box_width}"
        );
        assert!(
            row.nearest_left.is_some() && row.nearest_right.is_some(),
            "expected nearest visual neighbors on both sides for {row_id}"
        );
        let current_left_gap = row
            .current_left
            .as_ref()
            .and_then(|side| side.gap_pt)
            .expect("current left gap should exist");
        assert!(
            row.nearest_left
                .as_ref()
                .is_some_and(|span| span.gap_pt < current_left_gap),
            "expected visual left neighbor to be closer than current left anchor for {row_id}"
        );
        let nearest_visual_span_width = row
            .width_comparison
            .nearest_visual_span_width_pt
            .expect("expected nearest visual span width");
        assert!(
            (nearest_visual_span_width - redaction_box_width).abs() <= 3.0_f32,
            "expected nearest visual span width to stay close to the redaction box for {row_id}: nearest={nearest_visual_span_width} box={redaction_box_width}"
        );
        let current_anchor_target_width = row.width_comparison.current_anchor_target_width_pt;
        assert!(
            current_anchor_target_width - nearest_visual_span_width >= 35.0_f32,
            "expected current anchor target width to be materially wider than the visual span for {row_id}: current={current_anchor_target_width} nearest={nearest_visual_span_width}"
        );
    }
}

#[test]
fn efta00038617_reports_at_least_one_visually_empty_current_anchor() {
    let input = Path::new("test_data/EFTA00038617.pdf");
    let output_dir = test_output_dir("visual_anchor_metrics_efta00038617");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_from_paths_with_diagnostics(input, &output_dir, None, cfg(), true)
        .expect("diagnostics-enabled run should succeed");
    let report = load_visual_anchor_metrics_report(
        outputs
            .visual_anchor_metrics_path
            .as_ref()
            .expect("visual metrics path should be present"),
    );
    assert!(
        report
            .rows
            .iter()
            .any(|row| row.flags.current_anchor_visually_empty),
        "expected at least one visually empty current anchor row"
    );
}

#[test]
fn efta01083121_reports_grouped_visual_spans() {
    let input = Path::new("test_data/EFTA01083121.pdf");
    let output_dir = test_output_dir("visual_anchor_metrics_efta01083121");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_from_paths_with_diagnostics(input, &output_dir, None, cfg(), true)
        .expect("diagnostics-enabled run should succeed");
    let report = load_visual_anchor_metrics_report(
        outputs
            .visual_anchor_metrics_path
            .as_ref()
            .expect("visual metrics path should be present"),
    );
    assert!(
        report
            .rows
            .iter()
            .any(|row| row.grouped_left.is_some() || row.grouped_right.is_some()),
        "expected at least one grouped visual span"
    );
}

#[test]
fn main_cli_diagnostics_write_visual_metrics_for_all_current_test_data() {
    let inputs = [
        Path::new("test_data/EFTA00038617.pdf"),
        Path::new("test_data/EFTA00101126.pdf"),
        Path::new("test_data/EFTA01083121.pdf"),
        Path::new("test_data/EFTA02238592.pdf"),
        Path::new("test_data/EFTA02717423.pdf"),
    ];
    let output_root = test_output_dir("visual_anchor_metrics_main_cli_all");
    std::fs::create_dir_all(&output_root)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_root.display()));
    for input in inputs {
        let stem = input
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("output");
        let output_dir = output_root.join(stem);
        std::fs::create_dir_all(&output_dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
        let outputs = run_from_paths_with_diagnostics(input, &output_dir, None, cfg(), true)
            .unwrap_or_else(|error| panic!("run failed for {}: {error}", input.display()));
        let report_path = outputs
            .visual_anchor_metrics_path
            .as_ref()
            .expect("visual metrics path should exist");
        let crops_dir = outputs
            .visual_anchor_crops_dir
            .as_ref()
            .expect("visual crops dir should exist");
        assert!(
            report_path.exists(),
            "missing report {}",
            report_path.display()
        );
        assert!(crops_dir.exists(), "missing crops {}", crops_dir.display());
        let report = load_visual_anchor_metrics_report(report_path);
        assert!(!report.rows.is_empty(), "expected visual metrics rows");
    }
}

#[test]
fn standalone_visual_anchor_metric_extractor_writes_reports_and_crops_for_test_data_directory() {
    let output_dir = test_output_dir("visual_anchor_metrics_standalone_batch");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_visual_metrics(VisualAnchorMetricServiceRequest {
        input: Path::new("test_data").to_path_buf(),
        output_dir: output_dir.clone(),
        compact: false,
    })
    .expect("standalone visual metrics run should succeed");
    assert_eq!(outputs.report_paths.len(), 5, "expected one report per pdf");
    for report_path in outputs.report_paths {
        assert!(
            report_path.exists(),
            "missing report {}",
            report_path.display()
        );
        let crop_dir = report_path
            .parent()
            .expect("report path should have parent")
            .join("visual_crops");
        assert!(crop_dir.exists(), "missing crop dir {}", crop_dir.display());
        let report = load_visual_anchor_metrics_report(report_path.as_path());
        assert!(!report.rows.is_empty(), "expected non-empty report rows");
    }
}

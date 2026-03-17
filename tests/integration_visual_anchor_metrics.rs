#![cfg(feature = "cli-entry")]

use std::path::Path;

use serde_json::Value;

use unredact::service::anchor_span_visual_benchmark_cli_entry::{
    run as run_anchor_span_visual_benchmark, AnchorSpanVisualBenchmarkRequest,
};
use unredact::service::unredact_cli_entry::{
    run_from_paths_with_diagnostics, UnredactServiceConfig,
};
use unredact::service::visual_anchor_metrics_cli_entry::{
    run as run_visual_metrics, VisualAnchorMetricServiceRequest,
};
use unredact::types::guess_types::GuessConfig;
use unredact::types::visualizer_config::VisualizerConfig;

pub mod common;
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
fn efta00101126_rows_show_visible_current_anchors_and_aligned_visual_spans() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    let output_dir = test_output_dir("visual_anchor_metrics_efta00101126");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_visual_metrics(VisualAnchorMetricServiceRequest {
        input: input.to_path_buf(),
        output_dir: output_dir.clone(),
        compact: false,
    })
    .expect("visual extractor run should succeed");
    let report_path = outputs
        .report_paths
        .first()
        .expect("visual extractor should emit one report");
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
            (current_anchor_target_width - nearest_visual_span_width).abs() <= 3.0_f32,
            "expected current anchor target width to stay close to the visual span for {row_id}: current={current_anchor_target_width} nearest={nearest_visual_span_width}"
        );
    }
}

#[test]
fn efta00038617_reports_at_least_one_visually_empty_current_anchor() {
    let input = Path::new("test_data/EFTA00038617.pdf");
    let output_dir = test_output_dir("visual_anchor_metrics_efta00038617");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_visual_metrics(VisualAnchorMetricServiceRequest {
        input: input.to_path_buf(),
        output_dir: output_dir.clone(),
        compact: false,
    })
    .expect("visual extractor run should succeed");
    let report = load_visual_anchor_metrics_report(outputs.report_paths[0].as_path());
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
    let outputs = run_visual_metrics(VisualAnchorMetricServiceRequest {
        input: input.to_path_buf(),
        output_dir: output_dir.clone(),
        compact: false,
    })
    .expect("visual extractor run should succeed");
    let report = load_visual_anchor_metrics_report(outputs.report_paths[0].as_path());
    assert!(
        report
            .rows
            .iter()
            .any(|row| row.grouped_left.is_some() || row.grouped_right.is_some()),
        "expected at least one grouped visual span"
    );
}

#[test]
fn main_cli_diagnostics_do_not_write_visual_metrics_for_current_test_data() {
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
        assert!(
            outputs
                .diagnostics_path
                .as_ref()
                .is_some_and(|path| path.exists()),
            "missing diagnostics for {}",
            input.display()
        );
        let report_path = output_dir.join(format!("{stem}.visual_metrics.json"));
        let crops_dir = output_dir.join("visual_crops");
        assert!(
            !report_path.exists(),
            "normal CLI should not write visual metrics {}",
            report_path.display()
        );
        assert!(
            !crops_dir.exists(),
            "normal CLI should not write visual crops {}",
            crops_dir.display()
        );
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

#[test]
fn anchor_span_visual_benchmark_writes_summary_rows_experiments_and_crops() {
    let output_dir = test_output_dir("anchor_span_visual_benchmark");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_anchor_span_visual_benchmark(AnchorSpanVisualBenchmarkRequest {
        output_dir: output_dir.clone(),
        compact: false,
    })
    .expect("visual span benchmark should succeed");
    assert!(
        outputs.summary_path.exists(),
        "missing {}",
        outputs.summary_path.display()
    );
    assert!(
        outputs.rows_path.exists(),
        "missing {}",
        outputs.rows_path.display()
    );
    assert!(
        outputs.experiments_dir.exists(),
        "missing {}",
        outputs.experiments_dir.display()
    );
    assert!(
        outputs.crops_dir.exists(),
        "missing {}",
        outputs.crops_dir.display()
    );

    let summary = std::fs::read(&outputs.summary_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", outputs.summary_path.display())
    });
    let rows = std::fs::read(&outputs.rows_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", outputs.rows_path.display()));
    let summary_json = serde_json::from_slice::<Value>(&summary).unwrap_or_else(|error| {
        panic!(
            "failed to parse {}: {error}",
            outputs.summary_path.display()
        )
    });
    let rows_json = serde_json::from_slice::<Value>(&rows)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", outputs.rows_path.display()));
    assert!(
        summary_json["dataset"]["row_count"].as_u64().unwrap_or(0) > 0,
        "expected benchmark summary row count"
    );
    assert!(
        rows_json.as_array().is_some_and(|rows| !rows.is_empty()),
        "expected non-empty benchmark rows"
    );
    let benchmark_rows = rows_json.as_array().expect("rows json should be an array");
    let page7_row0 = benchmark_rows
        .iter()
        .find(|row| row["row_key"] == "EFTA00101126:page7_row0")
        .expect("missing EFTA00101126:page7_row0");
    assert_eq!(page7_row0["current_alignment"], "aligned");
    assert!(
        page7_row0["current_span_width_pt"]
            .as_f64()
            .zip(page7_row0["visual_reference_width_pt"].as_f64())
            .is_some_and(|(current, visual)| (current - visual).abs() <= 3.0_f64),
        "expected EFTA00101126:page7_row0 to stay close to the visual span"
    );
    for experiment in [
        "current_vs_visual_delta",
        "visual_aligned_rescore",
        "tie_zone_after_visual_alignment",
    ] {
        let path = outputs.experiments_dir.join(format!("{experiment}.json"));
        assert!(path.exists(), "missing experiment {}", path.display());
    }
    let rescore_path = outputs.experiments_dir.join("visual_aligned_rescore.json");
    let rescore_json = serde_json::from_slice::<Value>(
        &std::fs::read(&rescore_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", rescore_path.display())),
    )
    .unwrap_or_else(|error| panic!("failed to parse {}: {error}", rescore_path.display()));
    let rescore_rows = rescore_json["rows"]
        .as_array()
        .expect("visual_aligned_rescore rows should be an array");
    let improved_row = rescore_rows
        .iter()
        .find(|row| row["row_key"] == "EFTA00038617:page1_row5")
        .expect("missing visual_aligned_rescore row for EFTA00038617:page1_row5");
    assert!(
        improved_row["metrics"]["target_rank_before"]
            .as_u64()
            .zip(improved_row["metrics"]["target_rank_after"].as_u64())
            .is_some_and(|(before, after)| after < before),
        "expected visual-aligned rescore to improve EFTA00038617:page1_row5"
    );
    let tie_path = outputs
        .experiments_dir
        .join("tie_zone_after_visual_alignment.json");
    let tie_json = serde_json::from_slice::<Value>(
        &std::fs::read(&tie_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", tie_path.display())),
    )
    .unwrap_or_else(|error| panic!("failed to parse {}: {error}", tie_path.display()));
    let has_efta00038617_tie = tie_json["rows"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["row_key"]
                .as_str()
                .is_some_and(|row_key| row_key.starts_with("EFTA00038617:"))
                && row["metrics"]["target_rank_after"]
                    .as_u64()
                    .is_some_and(|rank| rank > 20)
                && row["metrics"]["target_vs_top1_error_gap_pt"]
                    .as_f64()
                    .is_some_and(|gap| gap <= 1.0_f64)
        })
    });
    assert!(
        has_efta00038617_tie,
        "expected at least one EFTA00038617 tie-zone row after visual alignment"
    );
}

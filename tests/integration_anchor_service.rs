#![cfg(feature = "cli-entry")]

use std::path::Path;

use unredact::service::tooling_entry::{run_anchor_from_redactions, ToolingAnchorRequest};
use unredact::service::unredact_cli_entry::{
    run_from_paths, run_from_paths_with_diagnostics, UnredactServiceConfig,
};
use unredact::types::diagnostic_types::{DiagnosticRecord, DiagnosticValue};
use unredact::types::guess_types::{AnchorDecisionRecord, AnchorType};
use unredact::types::visualizer_config::VisualizerConfig;

mod common;
use common::{load_diagnostic_report, load_guess_report, load_redaction_report, test_output_dir};

fn selected_candidate(decision: &AnchorDecisionRecord) -> Option<&AnchorDecisionRecord> {
    if decision.left.is_some() || decision.right.is_some() {
        Some(decision)
    } else {
        None
    }
}

fn anchor_signature(decisions: &[AnchorDecisionRecord]) -> Vec<String> {
    let mut out = Vec::<String>::new();
    for decision in decisions {
        let selected = selected_candidate(decision);
        let left = selected
            .and_then(|candidate| candidate.left.as_ref())
            .filter(|side| side.anchor_type == AnchorType::Left)
            .map(|side| format!("{}@{:.3}", side.text.trim(), side.x))
            .unwrap_or_else(|| "-".to_owned());
        let right = selected
            .and_then(|candidate| candidate.right.as_ref())
            .filter(|side| side.anchor_type == AnchorType::Right)
            .map(|side| format!("{}@{:.3}", side.text.trim(), side.x))
            .unwrap_or_else(|| "-".to_owned());
        out.push(format!(
            "{}|{}|{}|{}",
            decision.anchor_row_id, decision.anchor_mode, left, right
        ));
    }
    out
}

fn assert_anchor_contract(
    decisions: &[AnchorDecisionRecord],
    row_id: &str,
    expected_mode: &str,
    expected_right_text: &str,
) {
    let decision = decisions
        .iter()
        .find(|decision| decision.anchor_row_id == row_id)
        .unwrap_or_else(|| panic!("missing anchor decision for row_id={row_id}"));
    assert_eq!(
        decision.anchor_mode, expected_mode,
        "unexpected mode for {row_id}"
    );
    assert_eq!(
        decision
            .right
            .as_ref()
            .map(|side| side.text.trim())
            .unwrap_or_default(),
        expected_right_text,
        "unexpected right anchor text for {row_id}"
    );
}

fn diagnostic_float(record: &DiagnosticRecord, key: &str) -> f64 {
    match record.metrics.get(key) {
        Some(DiagnosticValue::Float(value)) => *value,
        other => panic!("expected float metric `{key}`, got {other:?}"),
    }
}

fn diagnostic_text<'a>(record: &'a DiagnosticRecord, key: &str) -> &'a str {
    match record.metrics.get(key) {
        Some(DiagnosticValue::Text(value)) => value.as_str(),
        other => panic!("expected text metric `{key}`, got {other:?}"),
    }
}

fn diagnostic_by_row_code_side<'a>(
    items: &'a [DiagnosticRecord],
    row_id: &str,
    code: &str,
    side: &str,
) -> &'a DiagnosticRecord {
    items
        .iter()
        .find(|item| {
            item.row_id.as_deref() == Some(row_id)
                && item.code == code
                && item.metrics.get("side") == Some(&DiagnosticValue::Text(side.to_owned()))
        })
        .unwrap_or_else(|| panic!("missing diagnostic code={code} row_id={row_id} side={side}"))
}

#[test]
fn run_anchor_from_redactions_is_deterministic_for_fixed_input() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    let redactions_path = Path::new("test_data/redactions/EFTA00101126.redactions.json");
    let pdf_bytes = std::fs::read(input)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()));
    let redactions = load_redaction_report(redactions_path);
    let diagnostics = ["integration_anchor_service=determinism_probe".to_owned()];

    let first = run_anchor_from_redactions(ToolingAnchorRequest {
        input_name: "EFTA00101126",
        pdf_bytes: &pdf_bytes,
        redactions: &redactions,
        diagnostics: &diagnostics,
    })
    .expect("first anchor run should succeed");
    let second = run_anchor_from_redactions(ToolingAnchorRequest {
        input_name: "EFTA00101126",
        pdf_bytes: &pdf_bytes,
        redactions: &redactions,
        diagnostics: &diagnostics,
    })
    .expect("second anchor run should succeed");

    assert_eq!(first.decisions.len(), second.decisions.len());
    assert_eq!(
        anchor_signature(first.decisions.as_slice()),
        anchor_signature(second.decisions.as_slice())
    );
}

#[test]
fn pipeline_anchor_output_matches_anchor_service_entrypoint() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    let pdf_bytes = std::fs::read(input)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()));
    let output_dir = test_output_dir("integration_anchor_pipeline_parity");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_from_paths(
        input,
        &output_dir,
        None,
        UnredactServiceConfig {
            include_details: false,
            enable_image_analysis: true,
            guess: unredact::types::guess_types::GuessConfig::default(),
            visualize: false,
            visualizer: VisualizerConfig::default(),
        },
    )
    .expect("pipeline run should succeed");
    let pipeline_redactions = load_redaction_report(&outputs.redactions_path);
    let diagnostics = ["integration_anchor_service=pipeline_parity".to_owned()];
    let direct_anchor = run_anchor_from_redactions(ToolingAnchorRequest {
        input_name: "EFTA00101126",
        pdf_bytes: &pdf_bytes,
        redactions: &pipeline_redactions,
        diagnostics: &diagnostics,
    })
    .expect("direct anchor run should succeed");
    let guess_report = load_guess_report(&outputs.guesses_path);
    let pipeline_anchor = guess_report.to_anchor_report();

    assert_eq!(
        anchor_signature(direct_anchor.decisions.as_slice()),
        anchor_signature(pipeline_anchor.decisions.as_slice())
    );
}

#[test]
fn deterministic_anchor_resolver_matches_expected_real_pdf_pairs() {
    let cfg = || UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: unredact::types::guess_types::GuessConfig::default(),
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };

    let efta00101126_input = Path::new("test_data/EFTA00101126.pdf");
    let efta00101126_output = test_output_dir("integration_anchor_contract_efta00101126");
    std::fs::create_dir_all(&efta00101126_output).unwrap_or_else(|error| {
        panic!(
            "failed to create {}: {error}",
            efta00101126_output.display()
        )
    });
    let efta00101126_outputs =
        run_from_paths(efta00101126_input, &efta00101126_output, None, cfg())
            .expect("pipeline run should succeed for EFTA00101126");
    let efta00101126_report = load_guess_report(&efta00101126_outputs.guesses_path);
    assert_anchor_contract(
        efta00101126_report.anchors.as_slice(),
        "page7_row0",
        "two_sided",
        "identity",
    );
    assert_anchor_contract(
        efta00101126_report.anchors.as_slice(),
        "page7_row1",
        "two_sided",
        "to,",
    );

    let efta00038617_input = Path::new("test_data/EFTA00038617.pdf");
    let efta00038617_output = test_output_dir("integration_anchor_contract_efta00038617");
    std::fs::create_dir_all(&efta00038617_output).unwrap_or_else(|error| {
        panic!(
            "failed to create {}: {error}",
            efta00038617_output.display()
        )
    });
    let efta00038617_outputs =
        run_from_paths(efta00038617_input, &efta00038617_output, None, cfg())
            .expect("pipeline run should succeed for EFTA00038617");
    let efta00038617_report = load_guess_report(&efta00038617_outputs.guesses_path);
    assert_anchor_contract(
        efta00038617_report.anchors.as_slice(),
        "page1_row3",
        "two_sided",
        "Maxwell,",
    );
    assert_anchor_contract(
        efta00038617_report.anchors.as_slice(),
        "page2_row14",
        "two_sided",
        "was",
    );

    let efta01083121_input = Path::new("test_data/EFTA01083121.pdf");
    let efta01083121_output = test_output_dir("integration_anchor_contract_efta01083121");
    std::fs::create_dir_all(&efta01083121_output).unwrap_or_else(|error| {
        panic!(
            "failed to create {}: {error}",
            efta01083121_output.display()
        )
    });
    let efta01083121_outputs =
        run_from_paths(efta01083121_input, &efta01083121_output, None, cfg())
            .expect("pipeline run should succeed for EFTA01083121");
    let efta01083121_report = load_guess_report(&efta01083121_outputs.guesses_path);
    assert_anchor_contract(
        efta01083121_report.anchors.as_slice(),
        "page0_row4",
        "two_sided",
        "Registry",
    );
    assert_anchor_contract(
        efta01083121_report.anchors.as_slice(),
        "page0_row6",
        "two_sided",
        "number",
    );
    assert_anchor_contract(
        efta01083121_report.anchors.as_slice(),
        "page0_row7",
        "two_sided",
        "Islands",
    );

    let efta02238592_input = Path::new("test_data/EFTA02238592.pdf");
    let efta02238592_output = test_output_dir("integration_anchor_contract_efta02238592");
    std::fs::create_dir_all(&efta02238592_output).unwrap_or_else(|error| {
        panic!(
            "failed to create {}: {error}",
            efta02238592_output.display()
        )
    });
    let efta02238592_outputs =
        run_from_paths(efta02238592_input, &efta02238592_output, None, cfg())
            .expect("pipeline run should succeed for EFTA02238592");
    let efta02238592_report = load_guess_report(&efta02238592_outputs.guesses_path);
    assert_anchor_contract(
        efta02238592_report.anchors.as_slice(),
        "page0_row4",
        "two_sided",
        "wrote:",
    );
}

#[test]
fn geometry_diagnostics_record_inner_edges_and_explicit_boundary_whitespace() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    let output_dir = test_output_dir("integration_anchor_geometry_diagnostics");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_from_paths_with_diagnostics(
        input,
        &output_dir,
        None,
        UnredactServiceConfig {
            include_details: false,
            enable_image_analysis: true,
            guess: unredact::types::guess_types::GuessConfig::default(),
            visualize: false,
            visualizer: VisualizerConfig::default(),
        },
        true,
    )
    .expect("diagnostics-enabled run should succeed");
    let diagnostics = load_diagnostic_report(
        outputs
            .diagnostics_path
            .as_ref()
            .expect("diagnostics path should be present"),
    );
    let inner_edges = diagnostic_by_row_code_side(
        diagnostics.items.as_slice(),
        "page7_row1",
        "anchor_inner_edges_selected",
        "left",
    );
    let trailing_whitespace_width_pt =
        diagnostic_float(inner_edges, "trailing_whitespace_width_pt");
    let span_x1 = diagnostic_float(inner_edges, "span_x1");
    let inner_right_edge_x_pt = diagnostic_float(inner_edges, "inner_right_edge_x_pt");
    assert!(
        trailing_whitespace_width_pt > 0.0_f64,
        "expected explicit trailing whitespace on the left anchor"
    );
    assert!(
        inner_right_edge_x_pt < span_x1,
        "expected inner right edge to exclude trailing whitespace"
    );

    let boundary_gap = diagnostic_by_row_code_side(
        diagnostics.items.as_slice(),
        "page7_row1",
        "anchor_boundary_gap_selected",
        "left",
    );
    assert_eq!(
        diagnostic_text(boundary_gap, "boundary_gap_source"),
        "explicit_trailing_whitespace"
    );
    assert_eq!(
        diagnostic_float(boundary_gap, "inferred_boundary_gap_pt"),
        0.0_f64
    );
    assert!(
        (diagnostic_float(boundary_gap, "explicit_boundary_gap_pt") - trailing_whitespace_width_pt)
            .abs()
            <= 0.001_f64,
        "expected the selected boundary gap to match the explicit trailing whitespace width"
    );

    let target_width = diagnostics
        .items
        .iter()
        .find(|item| {
            item.row_id.as_deref() == Some("page7_row1")
                && item.code == "anchor_target_width_computed"
        })
        .expect("missing target width diagnostic for page7_row1");
    let usable_left = diagnostic_float(target_width, "usable_left_edge_x_pt");
    let usable_right = diagnostic_float(target_width, "usable_right_edge_x_pt");
    let target_width_pt = diagnostic_float(target_width, "target_width_pt");
    assert!(
        (target_width_pt - (usable_right - usable_left)).abs() <= 0.001_f64,
        "expected target width to be derived directly from usable inner edges"
    );
}

#[test]
fn one_sided_rows_keep_redaction_width_as_public_fallback() {
    let input = Path::new("test_data/EFTA00038617.pdf");
    let output_dir = test_output_dir("integration_anchor_one_sided_width_fallback");
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    let outputs = run_from_paths(
        input,
        &output_dir,
        None,
        UnredactServiceConfig {
            include_details: false,
            enable_image_analysis: true,
            guess: unredact::types::guess_types::GuessConfig::default(),
            visualize: false,
            visualizer: VisualizerConfig::default(),
        },
    )
    .expect("pipeline run should succeed");
    let report = load_guess_report(&outputs.guesses_path);
    let one_sided = report
        .anchors
        .iter()
        .filter(|decision| decision.anchor_mode != "two_sided")
        .collect::<Vec<_>>();
    assert!(
        !one_sided.is_empty(),
        "expected one-sided rows in EFTA00038617"
    );
    for decision in one_sided {
        assert!(
            decision.usable_left_edge_x_pt.is_none() || decision.usable_right_edge_x_pt.is_none(),
            "one-sided rows should not publish both usable edges"
        );
        assert!(
            (decision.target_width_pt - decision.bbox.width().abs()).abs() <= 0.001_f32,
            "expected one-sided target width to fall back to the redaction width for {}",
            decision.anchor_row_id
        );
    }
}

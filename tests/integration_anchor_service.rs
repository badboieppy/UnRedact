#![cfg(feature = "cli-entry")]

use std::path::Path;

use unredact::service::tooling_entry::{run_anchor_from_redactions, ToolingAnchorRequest};
use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::{AnchorCandidateDecision, AnchorDecisionRecord, AnchorType};
use unredact::types::visualizer_config::VisualizerConfig;

mod common;
use common::{load_guess_report, load_redaction_report, test_output_dir};

fn selected_candidate(decision: &AnchorDecisionRecord) -> Option<&AnchorCandidateDecision> {
    if let Some(candidate_id) = decision.selected_candidate_id.as_deref() {
        if let Some(candidate) = decision
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id == candidate_id)
        {
            return Some(candidate);
        }
    }
    decision
        .candidates
        .iter()
        .find(|candidate| candidate.was_selected)
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
            "{}|{}|{}|{}|{}",
            decision.anchor_row_id,
            decision.selected_candidate_id.as_deref().unwrap_or("-"),
            decision.selected_mode.as_deref().unwrap_or("-"),
            left,
            right
        ));
    }
    out
}

#[test]
fn run_anchor_from_redactions_is_deterministic_for_fixed_input() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    let redactions_path =
        Path::new("benchmark/user_issue_default_baseline/EFTA00101126.redactions.json");
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
            guess: unredact::types::guess_types::GuessConfig {
                visual_score: false,
                ..unredact::types::guess_types::GuessConfig::default()
            },
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

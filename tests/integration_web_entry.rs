#![cfg(feature = "web-entry")]

use std::path::Path;

use unredact::service::unredact_web_entry::{run, UnredactWebConfig, UnredactWebRequest};
use unredact::types::diagnostic_types::DiagnosticReport;
use unredact::types::guess_types::{GuessConfig, GuessReport};
use unredact::types::visualizer_config::VisualizerConfig;

fn allowed_guess_width_pt(guess: &unredact::types::guess_types::RedactionGuess) -> f32 {
    match (
        guess.context.anchor_mode.as_deref(),
        guess.context.anchor_left_x,
        guess.context.anchor_right_x,
    ) {
        (Some("two_sided"), Some(left), Some(right)) if right > left => right - left,
        _ => guess.bbox.width().abs(),
    }
}

#[test]
fn web_entry_bytes_flow_emits_geometry_valid_efta00101126_candidates() {
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
    let diagnostics_result =
        serde_json::from_slice::<DiagnosticReport>(&web_outputs.diagnostics_json);
    assert!(
        diagnostics_result.is_ok(),
        "failed to decode web diagnostics json: {:?}",
        diagnostics_result.err()
    );
    let diagnostics = diagnostics_result.expect("web diagnostics should decode");
    assert!(
        !diagnostics.items.is_empty(),
        "expected typed diagnostics in web response"
    );

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
    for guess in [second_last, last] {
        let allowed_width = allowed_guess_width_pt(guess) + 0.01_f32;
        for candidate in &guess.candidates {
            if let Some(width_pt) = candidate.width_pt {
                assert!(
                    width_pt <= allowed_width,
                    "candidate {} overflowed allowed width {:.2} with width {:.2}",
                    candidate.text,
                    allowed_width,
                    width_pt
                );
            }
        }
    }
}

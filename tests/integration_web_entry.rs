#![cfg(feature = "web-entry")]

use std::path::Path;

use unredact::service::unredact_web_entry::{run, UnredactWebConfig, UnredactWebRequest};
use unredact::types::guess_types::{GuessCandidate, GuessConfig, GuessReport};
use unredact::types::visualizer_config::VisualizerConfig;

fn allowed_guess_width_pt(guess: &unredact::types::guess_types::RedactionGuess) -> f32 {
    match (
        guess.context.anchor_mode.as_deref(),
        guess.context.usable_left_edge_x_pt,
        guess.context.usable_right_edge_x_pt,
    ) {
        (Some("two_sided"), Some(left), Some(right)) if right > left => right - left,
        _ => guess.context.target_width_pt.max(guess.bbox.width().abs()),
    }
}

fn effective_candidate_error(candidate: &GuessCandidate) -> f32 {
    candidate.adjusted_error_pt.unwrap_or(candidate.error_pt)
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
        guess: GuessConfig::default(),
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
        report
            .stage_timings
            .iter()
            .any(|item| item.stage == "visualize"),
        "expected web guesses to include lightweight stage timings"
    );

    assert!(
        report.guesses.len() >= 2,
        "expected at least 2 guesses, got {}",
        report.guesses.len()
    );

    let second_last = &report.guesses[report.guesses.len() - 2];
    let last = &report.guesses[report.guesses.len() - 1];
    assert!(
        second_last.context.anchor_mode.is_some(),
        "second-to-last redaction should be guessable"
    );
    assert!(
        last.context.anchor_mode.is_some(),
        "last redaction should be guessable"
    );
    for guess in [second_last, last] {
        let _target_width = allowed_guess_width_pt(guess);
        let mut last_error = None::<f32>;
        for candidate in &guess.candidates {
            let width_pt = candidate.width_pt;
            assert!(
                width_pt.is_finite() && width_pt >= 0.0_f32,
                "candidate {} produced invalid width {}",
                candidate.text,
                width_pt
            );
            if let Some(previous_error) = last_error {
                let current_error = effective_candidate_error(candidate);
                assert!(
                    current_error + 0.0001_f32 >= previous_error,
                    "candidate list is not sorted by error: previous={} current={} text={}",
                    previous_error,
                    current_error,
                    candidate.text,
                );
            }
            last_error = Some(effective_candidate_error(candidate));
        }
    }

    let report_json = serde_json::from_slice::<serde_json::Value>(&web_outputs.guesses_json)
        .expect("guesses json should decode into value");
    let guesses = report_json
        .get("guesses")
        .and_then(serde_json::Value::as_array)
        .expect("guesses array should exist");
    let first_guess = guesses.first().expect("at least one guess should exist");
    let context = first_guess
        .get("context")
        .expect("guess context should be present");
    assert!(context.get("target_width_pt").is_some());
    assert!(context.get("target_guess_width_pt").is_none());
    let candidate = first_guess
        .get("candidates")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first());
    if let Some(candidate) = candidate {
        for removed in ["score", "exact_matches", "visual_score", "confidence_score"] {
            assert!(
                candidate.get(removed).is_none(),
                "candidate json should not contain removed field {removed}"
            );
        }
        for required in [
            "text",
            "width_pt",
            "glyph_width_sum_pt",
            "char_spacing_total_pt",
            "word_spacing_total_pt",
            "target_width_pt",
            "error_pt",
        ] {
            assert!(
                candidate.get(required).is_some(),
                "candidate json should contain field {required}"
            );
        }
        assert!(candidate.get("word_count").is_none());
        assert!(candidate.get("char_count").is_none());
    }
}

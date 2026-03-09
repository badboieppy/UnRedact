#![cfg(feature = "cli-entry")]

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;
use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::{GuessConfig, RedactionGuess};
use unredact::types::visualizer_config::VisualizerConfig;

mod common;
use common::{load_guess_report, load_redaction_report, test_output_dir};

fn normalize_guess_text_for_exact_match(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_ascii_uppercase()
}

fn ordered_guess_texts_upper(guess: &RedactionGuess) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = BTreeSet::<String>::new();
    for candidate in &guess.candidates {
        let normalized = normalize_guess_text_for_exact_match(&candidate.text);
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    for text in &guess.exact_matches {
        let normalized = normalize_guess_text_for_exact_match(text);
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

fn horizontal_overlap_pt(left: &RedactionGuess, right: &RedactionGuess) -> f32 {
    (left.bbox.x1.min(right.bbox.x1) - left.bbox.x0.max(right.bbox.x0)).max(0.0)
}

fn top_candidate_text(guess: &RedactionGuess) -> Option<&str> {
    guess
        .candidates
        .first()
        .map(|candidate| candidate.text.trim())
        .filter(|value| !value.is_empty())
}

fn allowed_guess_width_pt(guess: &RedactionGuess) -> f32 {
    match (
        guess.context.anchor_mode.as_deref(),
        guess.context.anchor_left_x,
        guess.context.anchor_right_x,
    ) {
        (Some("two_sided"), Some(left), Some(right)) if right > left => right - left,
        _ => guess.bbox.width().abs(),
    }
}

#[derive(Debug, Clone)]
struct BlueOverlayTextOp {
    text: String,
    font_size_pt: f32,
    y: f32,
}

fn parse_pdf_literal_text(payload: &str) -> Option<String> {
    let start = payload.find('(')?;
    let bytes = payload.as_bytes();
    let mut index = start + 1;
    let mut escaped = false;
    let mut out = String::new();
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if escaped {
            let mapped = match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '(' => '(',
                ')' => ')',
                '\\' => '\\',
                _ => ch,
            };
            out.push(mapped);
            escaped = false;
            index += 1;
            continue;
        }
        match ch {
            '\\' => {
                escaped = true;
            }
            ')' => return Some(out),
            _ => out.push(ch),
        }
        index += 1;
    }
    None
}

fn parse_blue_overlay_text_ops(pdf_bytes: &[u8]) -> Vec<BlueOverlayTextOp> {
    let text = String::from_utf8_lossy(pdf_bytes);
    let mut out = Vec::new();
    for section in text.split(" re W n BT ").skip(1) {
        let Some(op_end) = section.find(" ET Q") else {
            continue;
        };
        let op = &section[..op_end];
        let Some((h_scale_raw, after_h_scale)) = op.split_once(" Tz ") else {
            continue;
        };
        let Ok(_h_scale_pct) = h_scale_raw.trim().parse::<f32>() else {
            continue;
        };
        let mut font_tokens = after_h_scale.split_whitespace();
        let _font_key = font_tokens.next();
        let Some(font_size_raw) = font_tokens.next() else {
            continue;
        };
        let Ok(font_size_pt) = font_size_raw.trim().parse::<f32>() else {
            continue;
        };
        let Some(tf_token) = font_tokens.next() else {
            continue;
        };
        if tf_token != "Tf" {
            continue;
        }
        let Some((_, after_tf)) = after_h_scale.split_once(" Tf 1 0 0 1 ") else {
            continue;
        };
        let mut xy_tokens = after_tf.split_whitespace();
        let _x = xy_tokens.next();
        let Some(y_raw) = xy_tokens.next() else {
            continue;
        };
        let Ok(y) = y_raw.trim().parse::<f32>() else {
            continue;
        };
        let Some((_, text_payload)) = after_tf.split_once(" Tm ") else {
            continue;
        };
        let Some(parsed_text) = parse_pdf_literal_text(text_payload) else {
            continue;
        };
        let parsed_text = parsed_text.trim().to_owned();
        if parsed_text.is_empty() {
            continue;
        }
        out.push(BlueOverlayTextOp {
            text: parsed_text,
            font_size_pt,
            y,
        });
    }
    out
}

fn parse_context_span_texts(meta_raw: &str) -> Vec<(String, String)> {
    let parsed = serde_json::from_str::<Value>(meta_raw).unwrap_or(Value::Null);
    let Some(items) = parsed.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::<(String, String)>::new();
    for item in items {
        let text = item
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        if text.is_empty() {
            continue;
        }
        let role = item
            .get("role_hint")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        out.push((text, role));
    }
    out
}

#[test]
fn efta00038617_page2_served_rows_emit_geometry_valid_candidates_with_default_dictionary() {
    let input = Path::new("test_data/EFTA00038617.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir("efta00038617");
    if output_dir.exists() {
        let remove_result = std::fs::remove_dir_all(&output_dir);
        assert!(
            remove_result.is_ok(),
            "failed to clean output dir {}: {:?}",
            output_dir.display(),
            remove_result.err()
        );
    }
    let create_result = std::fs::create_dir_all(&output_dir);
    assert!(
        create_result.is_ok(),
        "failed to create output dir {}: {:?}",
        output_dir.display(),
        create_result.err()
    );

    let cfg = UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };

    let run_result = run_from_paths(input, &output_dir, None, cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");
    let report = load_guess_report(&outputs.guesses_path);

    let page_two = report
        .guesses
        .iter()
        .filter(|guess| guess.page_index == 1)
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        !page_two.is_empty(),
        "expected page 2 guesses for {}",
        input.display()
    );

    let first_bullet_rows = page_two
        .iter()
        .filter(|guess| guess.bbox.y0 >= 440.0_f32 && guess.bbox.y1 <= 505.0_f32)
        .collect::<Vec<_>>();
    assert!(
        first_bullet_rows.len() >= 10,
        "expected at least 10 first-bullet redactions on page 2, got {}",
        first_bullet_rows.len()
    );
    for i in 0..first_bullet_rows.len() {
        for j in (i + 1)..first_bullet_rows.len() {
            let left = first_bullet_rows[i];
            let right = first_bullet_rows[j];
            let left_center_y = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
            let right_center_y = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
            if (left_center_y - right_center_y).abs() <= 4.0_f32 {
                assert!(
                    horizontal_overlap_pt(left, right) <= 1.0_f32,
                    "first-bullet redaction boxes overlap on same row: left={:?} right={:?}",
                    left.bbox,
                    right.bbox
                );
            }
        }
    }

    let candidate_pool = first_bullet_rows
        .iter()
        .flat_map(|row| ordered_guess_texts_upper(row))
        .collect::<BTreeSet<_>>();
    for row in &first_bullet_rows {
        let allowed_width = allowed_guess_width_pt(row) + 0.01_f32;
        for candidate in &row.candidates {
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

    let expected_fragments = [
        "SARAH",
        "ADRIANA",
        "NADIA",
        "WEXNER",
        "HALEY",
        "WILLIAM",
        "DAVID",
        "BARNETT",
    ];
    let missing = expected_fragments
        .iter()
        .filter(|fragment| !candidate_pool.iter().any(|value| value.contains(**fragment)))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing expected geometry-valid target fragments from first-bullet candidate pool: {:?}",
        missing
    );

    let anchored_rows = first_bullet_rows
        .iter()
        .copied()
        .filter(|row| row.context.has_anchor_pair)
        .collect::<Vec<_>>();
    assert!(
        !anchored_rows.is_empty(),
        "expected first-bullet set to include trusted anchored rows"
    );
    let mean_anchored_candidates = anchored_rows
        .iter()
        .map(|row| row.candidates.len() as f64)
        .sum::<f64>()
        / anchored_rows.len() as f64;
    assert!(
        mean_anchored_candidates <= 900.0_f64,
        "mean anchored candidate volume too high: {:.1}",
        mean_anchored_candidates
    );
}

#[test]
fn efta00038617_served_row_metadata_exposes_multi_anchor_context_spans() {
    let input = Path::new("test_data/EFTA00038617.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir("efta00038617_context_spans");
    if output_dir.exists() {
        let remove_result = std::fs::remove_dir_all(&output_dir);
        assert!(
            remove_result.is_ok(),
            "failed to clean output dir {}: {:?}",
            output_dir.display(),
            remove_result.err()
        );
    }
    let create_result = std::fs::create_dir_all(&output_dir);
    assert!(
        create_result.is_ok(),
        "failed to create output dir {}: {:?}",
        output_dir.display(),
        create_result.err()
    );

    let cfg = UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };
    let run_result = run_from_paths(input, &output_dir, None, cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");
    let redaction_report = load_redaction_report(&outputs.redactions_path);

    let first_bullet_rows = redaction_report
        .redactions
        .iter()
        .filter(|row| row.page_index == 1 && row.bbox.y0 >= 440.0_f32 && row.bbox.y1 <= 505.0_f32)
        .collect::<Vec<_>>();
    assert!(
        first_bullet_rows.len() >= 10,
        "expected at least 10 first-bullet redactions, got {}",
        first_bullet_rows.len()
    );

    let mut total_span_entries = 0_usize;
    let mut has_center_anchor = false;
    let mut has_luc_anchor = false;
    let mut has_brunel_anchor = false;
    for row in first_bullet_rows {
        let Some(raw) = row.meta.get("context_spans_json_v1") else {
            continue;
        };
        let spans = parse_context_span_texts(raw);
        total_span_entries += spans.len();
        for (text, role) in spans {
            let normalized = normalize_guess_text_for_exact_match(&text);
            if role == "center" {
                has_center_anchor = true;
            }
            if normalized == "JEAN" || normalized == "LUC" || normalized.contains("JEAN LUC") {
                has_luc_anchor = true;
            }
            if normalized.contains("BRUNEL") {
                has_brunel_anchor = true;
            }
        }
    }
    assert!(
        total_span_entries >= 20,
        "expected rich multi-anchor context spans, got total_span_entries={total_span_entries}"
    );
    assert!(
        has_center_anchor,
        "expected at least one center role span in served-row metadata"
    );
    assert!(
        has_luc_anchor && has_brunel_anchor,
        "expected served-row metadata to expose Luc/Brunel center-anchor tokens"
    );
}

#[test]
fn efta00038617_multi_redaction_cluster_uses_shared_typography_for_guessing_and_visualization() {
    let input = Path::new("test_data/EFTA00038617.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir("efta00038617_multi_redaction_typography");
    if output_dir.exists() {
        let remove_result = std::fs::remove_dir_all(&output_dir);
        assert!(
            remove_result.is_ok(),
            "failed to clean output dir {}: {:?}",
            output_dir.display(),
            remove_result.err()
        );
    }
    let create_result = std::fs::create_dir_all(&output_dir);
    assert!(
        create_result.is_ok(),
        "failed to create output dir {}: {:?}",
        output_dir.display(),
        create_result.err()
    );

    let cfg = UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: true,
        visualizer: VisualizerConfig::default(),
    };
    let run_result = run_from_paths(input, &output_dir, None, cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");
    let report = load_guess_report(&outputs.guesses_path);
    let first_bullet_rows = report
        .guesses
        .iter()
        .filter(|guess| {
            guess.page_index == 1 && guess.bbox.y0 >= 440.0_f32 && guess.bbox.y1 <= 505.0_f32
        })
        .collect::<Vec<_>>();
    assert!(
        first_bullet_rows.len() >= 10,
        "expected at least 10 first-bullet rows, got {}",
        first_bullet_rows.len()
    );

    let styles = first_bullet_rows
        .iter()
        .filter_map(|row| {
            let font_name = row.context.anchor_font_name.as_ref()?;
            let font_size = row.context.anchor_font_size_pt?;
            Some(format!("{font_name}|{font_size:.2}"))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        styles.len(),
        1_usize,
        "expected shared font family and size in multi-redaction cluster, got {:?}",
        styles
    );

    let expected_texts = first_bullet_rows
        .iter()
        .filter_map(|row| top_candidate_text(row))
        .map(normalize_guess_text_for_exact_match)
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    assert!(
        !expected_texts.is_empty(),
        "expected non-empty top candidates for first-bullet rows"
    );

    let visualized_path = outputs
        .visualized_pdf_path
        .as_ref()
        .expect("visualized path should exist when visualize=true");
    let visualized_bytes = std::fs::read(visualized_path).unwrap_or_else(|error| {
        panic!(
            "failed to read visualized pdf {}: {error}",
            visualized_path.display()
        )
    });
    let ops = parse_blue_overlay_text_ops(&visualized_bytes);
    let matched = ops
        .iter()
        .filter(|op| op.y >= 440.0_f32 && op.y <= 500.0_f32)
        .filter(|op| {
            let normalized = normalize_guess_text_for_exact_match(&op.text);
            expected_texts
                .iter()
                .any(|expected| normalized.contains(expected))
        })
        .collect::<Vec<_>>();
    assert!(
        !matched.is_empty(),
        "expected blue overlay ops for first-bullet rows, parsed_ops={}",
        ops.len()
    );

    let matched_styles = matched
        .iter()
        .map(|op| format!("{:.2}", op.font_size_pt))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        matched_styles.len(),
        1_usize,
        "expected one overlay font size for first-bullet row, got {:?}",
        matched_styles
    );
}

#[test]
fn exact_matches_are_zero_error_candidates_when_present() {
    let input = Path::new("test_data/EFTA00038617.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir("exact_match_semantics");
    if output_dir.exists() {
        let remove_result = std::fs::remove_dir_all(&output_dir);
        assert!(
            remove_result.is_ok(),
            "failed to clean output dir {}: {:?}",
            output_dir.display(),
            remove_result.err()
        );
    }
    let create_result = std::fs::create_dir_all(&output_dir);
    assert!(
        create_result.is_ok(),
        "failed to create output dir {}: {:?}",
        output_dir.display(),
        create_result.err()
    );

    let cfg = UnredactServiceConfig {
        include_details: false,
        enable_image_analysis: true,
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };
    let run_result = run_from_paths(input, &output_dir, None, cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");
    let report = load_guess_report(&outputs.guesses_path);

    let exact_eps = 0.0001_f32;
    for (index, row) in report.guesses.iter().enumerate() {
        for exact in &row.exact_matches {
            let matched = row
                .candidates
                .iter()
                .find(|candidate| {
                    normalize_guess_text_for_exact_match(&candidate.text)
                        == normalize_guess_text_for_exact_match(exact)
                })
                .unwrap_or_else(|| {
                    panic!(
                        "exact match missing from candidates at row {}: exact='{}'",
                        index + 1,
                        exact
                    )
                });
            assert!(
                matched.error_pt.abs() <= exact_eps,
                "exact match has non-zero error at row {}: exact='{}' error_pt={}",
                index + 1,
                exact,
                matched.error_pt
            );
        }
    }
}

#[test]
fn efta00101126_last_two_redactions_do_not_use_cross_line_to_anchor() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir("efta00101126");
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
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };

    let run_result = run_from_paths(input, &output_dir, None, cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");

    let report = load_guess_report(&outputs.guesses_path);

    assert!(
        report.guesses.len() >= 2,
        "expected at least 2 guesses, got {}",
        report.guesses.len()
    );

    let second_last = &report.guesses[report.guesses.len() - 2];
    let last = &report.guesses[report.guesses.len() - 1];

    assert_ne!(second_last.context.right_anchor_text.trim(), "to,");
    assert_ne!(last.context.right_anchor_text.trim(), "to,");
    for row in [second_last, last] {
        if let Some(mode) = row.context.anchor_mode.as_deref() {
            assert!(
                matches!(mode, "two_sided" | "left_only" | "right_only"),
                "unexpected anchor mode for last-row contract: {mode}"
            );
        }
    }
}

#[test]
fn efta00101126_visualization_does_not_render_cross_line_to_anchor_context() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir("efta00101126_visual_contract");
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
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: true,
        visualizer: VisualizerConfig::default(),
    };
    let run_result = run_from_paths(input, &output_dir, None, cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");
    let guesses = load_guess_report(&outputs.guesses_path);
    assert!(guesses.guesses.len() >= 2, "expected at least 2 guesses");
    let second_last = &guesses.guesses[guesses.guesses.len() - 2];
    let last = &guesses.guesses[guesses.guesses.len() - 1];
    assert_ne!(second_last.context.right_anchor_text.trim(), "to,");
    assert_ne!(last.context.right_anchor_text.trim(), "to,");

    let visualized_path = outputs
        .visualized_pdf_path
        .as_ref()
        .expect("visualized path should exist when visualize=true");
    let visualized_bytes = std::fs::read(visualized_path).unwrap_or_else(|error| {
        panic!(
            "failed to read visualized pdf {}: {error}",
            visualized_path.display()
        )
    });
    let ops = parse_blue_overlay_text_ops(&visualized_bytes);
    let row_band_min = second_last.bbox.y0.min(last.bbox.y0) - 2.0_f32;
    let row_band_max = second_last.bbox.y1.max(last.bbox.y1) + 2.0_f32;
    let offending = ops
        .iter()
        .filter(|op| op.y >= row_band_min && op.y <= row_band_max)
        .filter(|op| normalize_guess_text_for_exact_match(&op.text).contains("TO,"))
        .count();
    assert!(
        offending == 0,
        "expected no cross-line 'to,' overlay text ops for the last rows, found {}",
        offending
    );
}

#[test]
fn efta00101126_last_two_rows_only_emit_geometry_valid_candidates() {
    let input = Path::new("test_data/EFTA00101126.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir("efta00101126_geometry_valid_candidates");
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
        guess: GuessConfig {
            visual_score: true,
            ..GuessConfig::default()
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };
    let run_result = run_from_paths(input, &output_dir, None, cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");
    let report = load_guess_report(&outputs.guesses_path);
    assert!(report.guesses.len() >= 2, "expected at least 2 guesses");

    for guess in report.guesses.iter().rev().take(2) {
        let allowed_width = allowed_guess_width_pt(guess);
        for candidate in &guess.candidates {
            let width_pt = candidate
                .width_pt
                .expect("measured candidate width should be present");
            assert!(
                width_pt <= allowed_width + 0.0001_f32,
                "candidate overflows allowed width: text='{}' width_pt={} allowed_width={} bbox={:?}",
                candidate.text,
                width_pt,
                allowed_width,
                guess.bbox
            );
        }
    }
}

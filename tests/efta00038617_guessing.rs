use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::{GuessConfig, GuessReport, RedactionGuess};
use unredact::types::visualizer_config::VisualizerConfig;

const TARGET_NAMES: [&str; 10] = [
    "SARAH KELLEN",
    "ADRIANA MUCINSKA",
    "NADIA MARCINKOVA",
    "LES WEXNER",
    "LESLEY GROFF",
    "JEAN LUC BRUNEL",
    "HALEY ROBSON",
    "WILLIAM HAMMOND",
    "DAVID RODGERS",
    "RICHARD BARNETT",
];

const NOISE_WORDS: [&str; 24] = [
    "ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT", "GOLF", "HOTEL", "INDIA", "JULIET",
    "KILO", "LIMA", "MIKE", "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO", "SIERRA", "TANGO",
    "UNIFORM", "VICTOR", "WHISKEY", "XRAY",
];

fn test_output_dir() -> PathBuf {
    std::env::temp_dir().join(format!("unredact_test_efta00038617_{}", std::process::id()))
}

fn write_name_dictionary(path: &Path) {
    let mut lines = TARGET_NAMES
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let target_set = TARGET_NAMES
        .iter()
        .map(|value| value.to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    for line in include_str!("../assets/names.txt").lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if target_set.contains(&trimmed.to_ascii_uppercase()) {
            continue;
        }
        lines.push(trimmed.to_owned());
        if lines.len() >= 1_200 {
            break;
        }
    }
    lines.extend(NOISE_WORDS.into_iter().map(str::to_owned));
    let content = lines.join("\n");
    let write_result = std::fs::write(path, content.as_bytes());
    assert!(
        write_result.is_ok(),
        "failed to write dictionary {}: {:?}",
        path.display(),
        write_result.err()
    );
}

fn load_report(path: &Path) -> GuessReport {
    let bytes_result = std::fs::read(path);
    assert!(
        bytes_result.is_ok(),
        "failed to read guesses report {}: {:?}",
        path.display(),
        bytes_result.err()
    );
    let bytes = bytes_result.expect("guesses report bytes should exist");
    let report_result = serde_json::from_slice::<GuessReport>(&bytes);
    assert!(
        report_result.is_ok(),
        "failed to parse guesses report {}: {:?}",
        path.display(),
        report_result.err()
    );
    report_result.expect("guesses report should parse")
}

fn collect_candidate_text_upper(rows: &[&RedactionGuess]) -> BTreeSet<String> {
    rows.iter()
        .flat_map(|row| {
            row.candidates
                .iter()
                .map(|candidate| candidate.text.to_ascii_uppercase())
        })
        .collect::<BTreeSet<_>>()
}

fn ordered_guess_texts_upper(guess: &RedactionGuess) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = BTreeSet::<String>::new();
    for text in &guess.exact_matches {
        let normalized = text.trim().to_ascii_uppercase();
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    for candidate in &guess.candidates {
        let normalized = candidate.text.trim().to_ascii_uppercase();
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

fn rank_in_guess(guess: &RedactionGuess, target: &str) -> Option<usize> {
    let normalized = target.trim().to_ascii_uppercase();
    ordered_guess_texts_upper(guess)
        .iter()
        .position(|value| value == &normalized)
        .map(|index| index + 1)
}

fn best_rank_in_rows(rows: &[&RedactionGuess], target: &str) -> Option<usize> {
    rows.iter()
        .filter_map(|guess| rank_in_guess(guess, target))
        .min()
}

fn is_multi_span_row(row: &RedactionGuess) -> bool {
    if !row.context.has_anchor_pair {
        return false;
    }
    let width = row.bbox.width().abs() as f64;
    if width <= 0.0 {
        return false;
    }
    (row.context.gap_pt as f64).abs() / width >= 2.0_f64
}

fn horizontal_overlap_pt(left: &RedactionGuess, right: &RedactionGuess) -> f32 {
    (left.bbox.x1.min(right.bbox.x1) - left.bbox.x0.max(right.bbox.x0)).max(0.0)
}

#[test]
fn efta00038617_page2_served_names_are_present_with_full_name_dictionary() {
    let input = Path::new("test_data/EFTA00038617.pdf");
    assert!(input.exists(), "missing test input: {}", input.display());

    let output_dir = test_output_dir();
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

    let dictionary_path = output_dir.join("names_full.txt");
    write_name_dictionary(&dictionary_path);

    let cfg = UnredactServiceConfig {
        include_details: false,
        include_full_page_rects: false,
        enable_image_analysis: true,
        raster_dpi: 200.0_f32,
        guess: GuessConfig {
            max_words: 4,
            max_candidates: 2_000,
            max_dictionary: 5_000,
            tol_pt: 100.0,
            max_nodes: 200_000,
            visual_score: true,
            visual_score_dpi: 200.0_f32,
            visual_min_ink_pixels: 64_u32,
            visual_drop_threshold: None,
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };

    let run_result = run_from_paths(input, &output_dir, Some(&dictionary_path), cfg);
    assert!(
        run_result.is_ok(),
        "pipeline run failed: {:?}",
        run_result.err()
    );
    let outputs = run_result.expect("pipeline run should succeed in test");
    let report = load_report(&outputs.guesses_path);

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

    let full_name_pool = collect_candidate_text_upper(&first_bullet_rows);
    let missing = TARGET_NAMES
        .iter()
        .copied()
        .filter(|name| !full_name_pool.contains(*name))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing expected names from first bullet candidate pool: {:?}",
        missing
    );

    let ranks = TARGET_NAMES
        .iter()
        .map(|target| best_rank_in_rows(&first_bullet_rows, target))
        .collect::<Vec<_>>();
    let found = ranks.iter().filter(|rank| rank.is_some()).count();
    let recall_at_5 = ranks
        .iter()
        .filter_map(|rank| *rank)
        .filter(|rank| *rank <= 5)
        .count() as f64
        / TARGET_NAMES.len() as f64;
    let recall_at_20 = ranks
        .iter()
        .filter_map(|rank| *rank)
        .filter(|rank| *rank <= 20)
        .count() as f64
        / TARGET_NAMES.len() as f64;
    assert_eq!(
        found,
        TARGET_NAMES.len(),
        "expected all targets found in first bullet rows, ranks={:?}",
        ranks
    );
    assert!(
        recall_at_5 >= 0.2_f64,
        "expected recall@5 >= 0.2 for noisy dictionary, got {:.3} (ranks={:?})",
        recall_at_5,
        ranks
    );
    assert!(
        recall_at_20 >= 0.6_f64,
        "expected recall@20 >= 0.6 for noisy dictionary, got {:.3} (ranks={:?})",
        recall_at_20,
        ranks
    );

    let multi_span_rows = first_bullet_rows
        .iter()
        .copied()
        .filter(|row| is_multi_span_row(row))
        .collect::<Vec<_>>();
    assert!(
        !multi_span_rows.is_empty(),
        "expected first-bullet set to include multi-span anchor rows"
    );
    let mean_multi_span_candidates = multi_span_rows
        .iter()
        .map(|row| row.candidates.len() as f64)
        .sum::<f64>()
        / multi_span_rows.len() as f64;
    assert!(
        mean_multi_span_candidates <= 900.0_f64,
        "mean multi-span candidate volume too high: {:.1}",
        mean_multi_span_candidates
    );
}

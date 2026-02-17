use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use unredact::service::unredact_entry::{run_from_paths, UnredactServiceConfig};
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

fn test_output_dir() -> PathBuf {
    std::env::temp_dir().join(format!("unredact_test_efta00038617_{}", std::process::id()))
}

fn write_name_dictionary(path: &Path) {
    let content = TARGET_NAMES.join("\n");
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
}

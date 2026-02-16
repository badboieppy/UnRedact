use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use unredact::service::unredact_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::{GuessConfig, GuessReport, RedactionGuess};
use unredact::types::visualizer_config::VisualizerConfig;

const TARGET_NAMES: [&str; 10] = [
    "GHISLAINE MAXWELL",
    "SARAH KELLEN",
    "ADRIANA MUCINSKA",
    "NADIA MARCINKOVA",
    "LES WEXNER",
    "LESLEY GROFF",
    "JEAN LUC BRUNEL",
    "HALEY ROBSON",
    "WILLIAM HAMMOND",
    "DAVID RODGERS",
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

fn find_page_row<'a>(
    rows: &'a [RedactionGuess],
    left: &str,
    right: &str,
) -> Option<&'a RedactionGuess> {
    rows.iter().find(|guess| {
        guess
            .context
            .left_anchor_text
            .trim()
            .eq_ignore_ascii_case(left)
            && guess
                .context
                .right_anchor_text
                .trim()
                .eq_ignore_ascii_case(right)
    })
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

    let row_those_jean = find_page_row(&page_two, "those", "Jean");
    let row_maxwell_among = find_page_row(&page_two, "Maxwell,", "Among");
    let row_included_luc = find_page_row(&page_two, "included", "Luc");
    assert!(
        row_those_jean.is_some(),
        "expected row with left=[those] right=[Jean] on page 2"
    );
    assert!(
        row_maxwell_among.is_some(),
        "expected row with left=[Maxwell,] right=[Among] on page 2"
    );
    assert!(
        row_included_luc.is_some(),
        "expected row with left=[included] right=[Luc] on page 2"
    );

    let row_those_jean = row_those_jean.expect("row should exist");
    let row_maxwell_among = row_maxwell_among.expect("row should exist");
    let row_included_luc = row_included_luc.expect("row should exist");
    for row in [row_those_jean, row_maxwell_among, row_included_luc] {
        assert!(
            row.context.has_anchor_pair,
            "target row should be anchored: left=[{}] right=[{}]",
            row.context.left_anchor_text, row.context.right_anchor_text
        );
    }

    let full_name_pool = collect_candidate_text_upper(&[row_those_jean, row_maxwell_among]);
    let missing = TARGET_NAMES
        .iter()
        .copied()
        .filter(|name| !full_name_pool.contains(*name))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing expected full names from served rows: {:?}",
        missing
    );

    let included_exact = row_included_luc
        .exact_matches
        .iter()
        .map(|value| value.to_ascii_uppercase())
        .collect::<Vec<_>>();
    assert!(
        included_exact.contains(&"JEAN LUC BRUNEL".to_owned()),
        "expected JEAN LUC BRUNEL exact hit for left=[included] right=[Luc], got {:?}",
        row_included_luc.exact_matches
    );
}

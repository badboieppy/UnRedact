use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use unredact::data::redactions_data::{PdfFileRetriever, RedactionDataRetriever, RedactionsData};
use unredact::data::{DictionaryData, FontsData, GuessValidationData};
use unredact::logic::{run_guess_from_paths, RunGuessRequest};
use unredact::service::unredact_entry::{run_from_paths, UnredactServiceConfig};
use unredact::types::guess_types::{GuessConfig, GuessReport};
use unredact::types::redaction_types::{
    Rect, RedactionKind, RedactionOccurrence, RedactionReport, UnderlyingTextHit,
};
use unredact::types::visualizer_config::VisualizerConfig;

const SYNTHETIC_TARGET_COUNT: usize = 3;
const MIN_TEXT_LEN: usize = 4;
const MAX_TEXT_LEN: usize = 18;
const TARGET_PICK_STEP: usize = 17;
const TARGET_PICK_START: usize = 11;
const MAX_LINE_DELTA_PT: f32 = 3.0_f32;
const MIN_OVERLAP_RATIO: f32 = 0.20_f32;
const COMMON_WORD_BLOCKLIST: [&str; 31] = [
    "ABOUT", "AFTER", "AMONG", "AROUND", "BECAUSE", "BEFORE", "BOTH", "DURING", "EVERY", "FIRST",
    "FOUND", "HAVE", "HAVING", "INTO", "KNOWN", "LATER", "MIGHT", "OTHER", "OVER", "SHOULD",
    "SINCE", "THERE", "THESE", "THOSE", "THROUGH", "UNDER", "UNTIL", "WHILE", "WOULD", "WITH",
    "BLACK",
];

#[derive(Debug, Clone)]
struct RedactionTarget {
    page_index: u32,
    text: String,
    rect: Rect,
}

fn smoke_output_dir(tag: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "unredact_generalization_smoke_{}_{}_{}",
        tag,
        std::process::id(),
        stamp
    ))
}

fn load_report(path: &Path) -> GuessReport {
    let bytes_result = std::fs::read(path);
    assert!(
        bytes_result.is_ok(),
        "failed to read guesses report {}: {:?}",
        path.display(),
        bytes_result.err()
    );
    let bytes = bytes_result.expect("report bytes should exist");
    let report_result = serde_json::from_slice::<GuessReport>(&bytes);
    assert!(
        report_result.is_ok(),
        "failed to parse guesses report {}: {:?}",
        path.display(),
        report_result.err()
    );
    report_result.expect("guesses report should parse")
}

fn source_candidates() -> [PathBuf; 5] {
    [
        PathBuf::from("test_data/EFTA01083121.pdf"),
        PathBuf::from("test_data/EFTA02238592.pdf"),
        PathBuf::from("test_data/EFTA02717423.pdf"),
        PathBuf::from("test_data/EFTA00101126.pdf"),
        PathBuf::from("test_data/EFTA00038617.pdf"),
    ]
}

fn split_name_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut buf = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphabetic() || ch == '\'' || ch == '-' {
            buf.push(ch);
        } else if !buf.is_empty() {
            words.push(buf.clone());
            buf.clear();
        }
    }
    if !buf.is_empty() {
        words.push(buf);
    }
    words
}

fn name_token_set() -> &'static BTreeSet<String> {
    static TOKENS: OnceLock<BTreeSet<String>> = OnceLock::new();
    TOKENS.get_or_init(|| {
        include_str!("../assets/names.txt")
            .lines()
            .flat_map(split_name_words)
            .map(|word| word.to_ascii_uppercase())
            .filter(|word| !word.is_empty())
            .collect::<BTreeSet<_>>()
    })
}

fn canonical_target_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first_alpha = trimmed.chars().find(|ch| ch.is_ascii_alphabetic())?;
    if !first_alpha.is_ascii_uppercase() {
        return None;
    }
    if !trimmed.chars().any(|ch| ch.is_ascii_lowercase()) {
        return None;
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphabetic() || ch.is_whitespace() || ch == '\'' || ch == '-')
    {
        return None;
    }
    let words = trimmed
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words.is_empty() || words.len() > 2 {
        return None;
    }
    let alpha_len = words
        .iter()
        .flat_map(|word| word.chars())
        .filter(|ch| ch.is_ascii_alphabetic())
        .count();
    if !(MIN_TEXT_LEN..=MAX_TEXT_LEN).contains(&alpha_len) {
        return None;
    }
    let upper = words.join(" ").to_ascii_uppercase();
    if COMMON_WORD_BLOCKLIST.contains(&upper.as_str()) {
        return None;
    }
    if words.len() == 1 && !name_token_set().contains(&upper) {
        return None;
    }
    Some(upper)
}

fn overlap_y(a: &Rect, b: &Rect) -> f32 {
    (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0_f32)
}

fn nearest_outer_context_pair(
    left_hit: &UnderlyingTextHit,
    right_hit: &UnderlyingTextHit,
    page_hits: &[UnderlyingTextHit],
) -> Option<(UnderlyingTextHit, UnderlyingTextHit)> {
    let line_center = ((left_hit.bbox.y0 + left_hit.bbox.y1) * 0.5_f32
        + (right_hit.bbox.y0 + right_hit.bbox.y1) * 0.5_f32)
        * 0.5_f32;
    let left = page_hits
        .iter()
        .filter(|hit| {
            hit.bbox.x1 <= left_hit.bbox.x0
                && ((hit.bbox.y0 + hit.bbox.y1) * 0.5_f32 - line_center).abs() <= MAX_LINE_DELTA_PT
        })
        .max_by(|a, b| a.bbox.x1.partial_cmp(&b.bbox.x1).unwrap_or(Ordering::Equal))
        .cloned();
    let right = page_hits
        .iter()
        .filter(|hit| {
            hit.bbox.x0 >= right_hit.bbox.x1
                && ((hit.bbox.y0 + hit.bbox.y1) * 0.5_f32 - line_center).abs() <= MAX_LINE_DELTA_PT
        })
        .min_by(|a, b| a.bbox.x0.partial_cmp(&b.bbox.x0).unwrap_or(Ordering::Equal))
        .cloned();
    match (left, right) {
        (Some(l), Some(r)) => Some((l, r)),
        _ => None,
    }
}

fn is_list_like_pair_context(
    left_hit: &UnderlyingTextHit,
    right_hit: &UnderlyingTextHit,
    page_hits: &[UnderlyingTextHit],
) -> bool {
    let Some((before, after)) = nearest_outer_context_pair(left_hit, right_hit, page_hits) else {
        return false;
    };
    let before_text = before.text.trim().to_ascii_lowercase();
    let after_text = after.text.trim().to_ascii_lowercase();
    before_text.contains("including")
        || before_text.contains("included")
        || before_text.contains("among")
        || before_text.contains("served")
        || before_text.ends_with(',')
        || after_text.starts_with(',')
        || after_text.starts_with("and ")
}

fn padded_rect(rect: Rect) -> Rect {
    let pad_y = (rect.height().abs() * 0.14_f32).clamp(0.4_f32, 1.5_f32);
    let pad_x = (rect.height().abs() * 0.10_f32).clamp(0.6_f32, 2.0_f32);
    Rect::new(
        rect.x0 - pad_x,
        rect.y0 - pad_y,
        rect.x1 + pad_x,
        rect.y1 + pad_y,
    )
}

fn collect_targets_from_pdf(bytes: &[u8], desired: usize) -> Result<Vec<RedactionTarget>, String> {
    let retriever = PdfFileRetriever::new_from_bytes(bytes, None)?;
    let mut hits_by_page = BTreeMap::<u32, Vec<UnderlyingTextHit>>::new();
    for page_index in retriever.page_indices() {
        let hits = retriever.underlying_text_hits(page_index)?;
        if !hits.is_empty() {
            hits_by_page.insert(page_index, hits);
        }
    }

    let mut eligible_pairs = Vec::<RedactionTarget>::new();
    for (page_index, page_hits) in &hits_by_page {
        let mut ordered_hits = page_hits.iter().collect::<Vec<_>>();
        ordered_hits.sort_by(|left, right| {
            let left_center = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
            let right_center = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
            left_center
                .partial_cmp(&right_center)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    left.bbox
                        .x0
                        .partial_cmp(&right.bbox.x0)
                        .unwrap_or(Ordering::Equal)
                })
        });
        for idx in 0..ordered_hits.len().saturating_sub(1) {
            let left_hit = ordered_hits[idx];
            let right_hit = ordered_hits[idx + 1];
            let left_center = (left_hit.bbox.y0 + left_hit.bbox.y1) * 0.5_f32;
            let right_center = (right_hit.bbox.y0 + right_hit.bbox.y1) * 0.5_f32;
            if (left_center - right_center).abs() > MAX_LINE_DELTA_PT {
                continue;
            }
            let gap = (right_hit.bbox.x0 - left_hit.bbox.x1).max(0.0_f32);
            if gap > 8.0_f32 {
                continue;
            }
            if nearest_outer_context_pair(left_hit, right_hit, page_hits).is_none() {
                continue;
            }
            if is_list_like_pair_context(left_hit, right_hit, page_hits) {
                continue;
            }
            let Some(left_text) = canonical_target_text(&left_hit.text) else {
                continue;
            };
            let Some(right_text) = canonical_target_text(&right_hit.text) else {
                continue;
            };
            let combined = format!("{left_text} {right_text}");
            let rect = padded_rect(Rect::new(
                left_hit.bbox.x0.min(right_hit.bbox.x0),
                left_hit.bbox.y0.min(right_hit.bbox.y0),
                left_hit.bbox.x1.max(right_hit.bbox.x1),
                left_hit.bbox.y1.max(right_hit.bbox.y1),
            ));
            if rect.width().abs() < 12.0_f32 || rect.height().abs() < 5.0_f32 {
                continue;
            }
            eligible_pairs.push(RedactionTarget {
                page_index: *page_index,
                text: combined,
                rect,
            });
        }
    }
    let mut eligible = eligible_pairs;

    eligible.sort_by(|left, right| {
        left.page_index
            .cmp(&right.page_index)
            .then_with(|| {
                left.rect
                    .y0
                    .partial_cmp(&right.rect.y0)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| {
                left.rect
                    .x0
                    .partial_cmp(&right.rect.x0)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| left.text.cmp(&right.text))
    });

    let mut frequency = BTreeMap::<String, usize>::new();
    for candidate in &eligible {
        *frequency.entry(candidate.text.clone()).or_insert(0) += 1;
    }
    let unique_only = eligible
        .iter()
        .filter(|candidate| frequency.get(&candidate.text).copied().unwrap_or(0) == 1)
        .cloned()
        .collect::<Vec<_>>();
    if unique_only.len() >= desired {
        eligible = unique_only;
    }

    if eligible.is_empty() {
        return Err("no_eligible_targets_found".to_owned());
    }

    let mut selected = Vec::<RedactionTarget>::new();
    let mut used_text = BTreeSet::<String>::new();
    let mut index = TARGET_PICK_START % eligible.len();
    let mut attempts = 0_usize;
    while selected.len() < desired && attempts < eligible.len().saturating_mul(4) {
        let candidate = &eligible[index];
        if used_text.insert(candidate.text.clone()) {
            selected.push(candidate.clone());
        }
        index = (index + TARGET_PICK_STEP) % eligible.len();
        attempts += 1;
    }
    if selected.len() < desired {
        for candidate in eligible {
            if selected.len() >= desired {
                break;
            }
            if used_text.insert(candidate.text.clone()) {
                selected.push(candidate);
            }
        }
    }
    Ok(selected)
}

fn empty_context_hit(page_index: u32, rect: Rect) -> UnderlyingTextHit {
    UnderlyingTextHit {
        page_index,
        bbox: rect,
        text: String::new(),
    }
}

fn collect_page_hits(bytes: &[u8]) -> Result<BTreeMap<u32, Vec<UnderlyingTextHit>>, String> {
    let retriever = PdfFileRetriever::new_from_bytes(bytes, None)?;
    let mut by_page = BTreeMap::<u32, Vec<UnderlyingTextHit>>::new();
    for page_index in retriever.page_indices() {
        let hits = retriever.underlying_text_hits(page_index)?;
        if !hits.is_empty() {
            by_page.insert(page_index, hits);
        }
    }
    Ok(by_page)
}

fn context_hits_for_target(
    target: &RedactionTarget,
    page_hits: &[UnderlyingTextHit],
) -> Vec<UnderlyingTextHit> {
    let target_center_y = (target.rect.y0 + target.rect.y1) * 0.5_f32;
    let left = page_hits
        .iter()
        .filter(|hit| {
            hit.bbox.x1 <= target.rect.x0
                && overlap_y(&hit.bbox, &target.rect) > 0.0_f32
                && ((hit.bbox.y0 + hit.bbox.y1) * 0.5_f32 - target_center_y).abs() <= 20.0_f32
        })
        .max_by(|a, b| a.bbox.x1.partial_cmp(&b.bbox.x1).unwrap_or(Ordering::Equal))
        .cloned()
        .unwrap_or_else(|| empty_context_hit(target.page_index, target.rect));

    let right = page_hits
        .iter()
        .filter(|hit| {
            hit.bbox.x0 >= target.rect.x1
                && overlap_y(&hit.bbox, &target.rect) > 0.0_f32
                && ((hit.bbox.y0 + hit.bbox.y1) * 0.5_f32 - target_center_y).abs() <= 20.0_f32
        })
        .min_by(|a, b| a.bbox.x0.partial_cmp(&b.bbox.x0).unwrap_or(Ordering::Equal))
        .cloned()
        .unwrap_or_else(|| empty_context_hit(target.page_index, target.rect));

    vec![left, right]
}

fn synthetic_redaction_report(
    input_path: &Path,
    targets: &[RedactionTarget],
    hits_by_page: &BTreeMap<u32, Vec<UnderlyingTextHit>>,
) -> RedactionReport {
    let mut page_counts = BTreeMap::<u32, u32>::new();
    let mut redactions = Vec::<RedactionOccurrence>::new();
    for target in targets {
        let page_hits = hits_by_page
            .get(&target.page_index)
            .cloned()
            .unwrap_or_default();
        redactions.push(RedactionOccurrence {
            page_index: target.page_index,
            bbox: target.rect,
            kind: RedactionKind::DrawnRect,
            score: 1.0_f32,
            meta: BTreeMap::new(),
            underlying_text: context_hits_for_target(target, &page_hits),
        });
        *page_counts.entry(target.page_index).or_insert(0) += 1;
    }

    RedactionReport {
        input: input_path.to_string_lossy().to_string(),
        redactions: redactions.clone(),
        count: redactions.len() as u32,
        page_counts,
        diagnostics: Vec::new(),
    }
}

fn write_noisy_dictionary(path: &Path, targets: &[RedactionTarget]) -> Result<(), String> {
    const EXTRA_NOISE_WORDS: [&str; 40] = [
        "ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT", "GOLF", "HOTEL", "INDIA",
        "JULIET", "KILO", "LIMA", "MIKE", "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO", "SIERRA",
        "TANGO", "UNIFORM", "VICTOR", "WHISKEY", "XRAY", "YANKEE", "ZULU", "ORCHARD", "LANTERN",
        "HARBOR", "VECTOR", "MATRIX", "CANYON", "RIVER", "SUMMIT", "CIPHER", "ORBIT", "TEMPO",
        "QUARTZ", "CIRRUS", "EMBER",
    ];

    let target_set = targets
        .iter()
        .map(|target| target.text.to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let mut lines = targets
        .iter()
        .map(|target| target.text.clone())
        .collect::<Vec<_>>();

    let names_raw = include_str!("../assets/names.txt");
    for line in names_raw.lines() {
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
    lines.extend(EXTRA_NOISE_WORDS.into_iter().map(str::to_owned));

    std::fs::write(path, lines.join("\n"))
        .map_err(|e| format!("failed to write dictionary {}: {e}", path.display()))
}

fn overlap_ratio(guess_rect: Rect, target_rect: Rect) -> f32 {
    let overlap_w =
        (guess_rect.x1.min(target_rect.x1) - guess_rect.x0.max(target_rect.x0)).max(0.0_f32);
    let overlap_h =
        (guess_rect.y1.min(target_rect.y1) - guess_rect.y0.max(target_rect.y0)).max(0.0_f32);
    let overlap_area = overlap_w * overlap_h;
    let target_area = target_rect.area().max(0.0001_f32);
    overlap_area / target_area
}

fn top_candidate_snapshot(report: &GuessReport, page_index: u32, rect: Rect) -> Vec<String> {
    report
        .guesses
        .iter()
        .filter(|guess| {
            guess.page_index == page_index && overlap_ratio(guess.bbox, rect) >= MIN_OVERLAP_RATIO
        })
        .flat_map(|guess| {
            guess
                .exact_matches
                .iter()
                .take(5)
                .cloned()
                .chain(
                    guess
                        .candidates
                        .iter()
                        .take(5)
                        .map(|candidate| candidate.text.clone()),
                )
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

#[test]
fn additional_epstein_files_run_without_file_specific_tuning() {
    let inputs = [
        Path::new("test_data/EFTA01083121.pdf"),
        Path::new("test_data/EFTA02238592.pdf"),
        Path::new("test_data/EFTA02717423.pdf"),
    ];

    let output_dir = smoke_output_dir("baseline");
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
        include_full_page_rects: false,
        enable_image_analysis: true,
        raster_dpi: 180.0_f32,
        guess: GuessConfig {
            max_words: 4,
            max_candidates: 200,
            max_dictionary: 5_000,
            tol_pt: 100.0,
            max_nodes: 200_000,
        },
        visualize: false,
        visualizer: VisualizerConfig::default(),
    };

    for input in inputs {
        assert!(input.exists(), "missing test input: {}", input.display());
        let run_result = run_from_paths(input, &output_dir, None, cfg.clone());
        assert!(
            run_result.is_ok(),
            "pipeline run failed for {}: {:?}",
            input.display(),
            run_result.err()
        );
        let outputs = run_result.expect("pipeline should succeed");
        let report = load_report(&outputs.guesses_path);
        assert!(
            !report.guesses.is_empty(),
            "expected non-empty guesses for {}",
            input.display()
        );
    }
}

#[test]
#[ignore = "use guess_accuracy_benchmark binary for continuous accuracy metrics"]
fn synthetic_redactions_are_guessed_from_noisy_dictionary() {
    let output_dir = smoke_output_dir("synthetic");
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

    let mut chosen_source = None::<PathBuf>;
    let mut source_bytes = None::<Vec<u8>>;
    let mut targets = None::<Vec<RedactionTarget>>;
    for source in source_candidates() {
        if !source.exists() {
            continue;
        }
        let bytes_result = std::fs::read(&source);
        if bytes_result.is_err() {
            continue;
        }
        let bytes = bytes_result.expect("bytes should exist when is_err is false");
        let candidate_targets = collect_targets_from_pdf(&bytes, SYNTHETIC_TARGET_COUNT);
        let Ok(candidate_targets) = candidate_targets else {
            continue;
        };
        if candidate_targets.len() < SYNTHETIC_TARGET_COUNT {
            continue;
        }
        chosen_source = Some(source);
        source_bytes = Some(bytes);
        targets = Some(candidate_targets);
        break;
    }

    let source = chosen_source.expect("expected at least one source file with eligible targets");
    let source_bytes = source_bytes.expect("source bytes should be set");
    let targets = targets.expect("target list should be set");

    let page_hits_result = collect_page_hits(&source_bytes);
    assert!(
        page_hits_result.is_ok(),
        "failed to collect page text hits from {}: {:?}",
        source.display(),
        page_hits_result.err()
    );
    let page_hits = page_hits_result.expect("page hits should exist");
    let synthetic_pdf = output_dir.join("synthetic_redacted.pdf");
    let redaction_report = synthetic_redaction_report(&synthetic_pdf, &targets, &page_hits);

    let dictionary_path = output_dir.join("synthetic_dictionary.txt");
    let dictionary_result = write_noisy_dictionary(&dictionary_path, &targets);
    assert!(
        dictionary_result.is_ok(),
        "failed to build noisy dictionary {}: {:?}",
        dictionary_path.display(),
        dictionary_result.err()
    );

    let redactions_data = RedactionsData::new();
    let redactions_path = output_dir.join("synthetic.redactions.json");
    let write_redactions_result =
        redactions_data.write_redactions(&redactions_path, &redaction_report);
    assert!(
        write_redactions_result.is_ok(),
        "failed to write redactions report {}: {:?}",
        redactions_path.display(),
        write_redactions_result.err()
    );

    let fonts_data = FontsData::new();
    let fonts_report_result = fonts_data.detect_fonts(&source, false);
    assert!(
        fonts_report_result.is_ok(),
        "failed to detect fonts for source {}: {:?}",
        source.display(),
        fonts_report_result.err()
    );
    let fonts_report = fonts_report_result.expect("fonts report should exist");
    let fonts_path = output_dir.join("synthetic.fonts.json");
    let write_fonts_result = fonts_data.write_fonts(&fonts_path, &fonts_report);
    assert!(
        write_fonts_result.is_ok(),
        "failed to write fonts report {}: {:?}",
        fonts_path.display(),
        write_fonts_result.err()
    );

    let guess_cfg = GuessConfig {
        max_words: 4,
        max_candidates: 500,
        max_dictionary: 5_000,
        tol_pt: 100.0,
        max_nodes: 200_000,
    };
    let guess_data = GuessValidationData::new();
    let dictionary_data = DictionaryData::new();
    let guess_report_result = run_guess_from_paths(RunGuessRequest {
        report_data: &guess_data,
        dictionary_data: &dictionary_data,
        font_run_data: &fonts_data,
        redactions_path: &redactions_path,
        fonts_path: &fonts_path,
        pdf_path: &source,
        dictionary_path: Some(&dictionary_path),
        cfg: &guess_cfg,
    });
    assert!(
        guess_report_result.is_ok(),
        "guess pass failed for synthetic redactions built from {}: {:?}",
        source.display(),
        guess_report_result.err()
    );
    let report = guess_report_result.expect("guess report should be produced");

    for target in &targets {
        let overlaps = report
            .guesses
            .iter()
            .filter(|guess| {
                guess.page_index == target.page_index
                    && overlap_ratio(guess.bbox, target.rect) >= MIN_OVERLAP_RATIO
            })
            .collect::<Vec<_>>();
        assert!(
            !overlaps.is_empty(),
            "no guessed redaction overlapped synthetic target '{}' on page {}",
            target.text,
            target.page_index + 1
        );

        let recovered = overlaps.iter().any(|guess| {
            guess
                .exact_matches
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(&target.text))
                || guess
                    .candidates
                    .iter()
                    .take(25)
                    .any(|candidate| candidate.text.eq_ignore_ascii_case(&target.text))
        });
        assert!(
            recovered,
            "target '{}' was not recovered near synthetic redaction on page {}. nearby candidates: {:?}",
            target.text,
            target.page_index + 1,
            top_candidate_snapshot(&report, target.page_index, target.rect)
        );
    }
}

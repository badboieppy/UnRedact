use super::visual_logic::{apply_visual_scores_from_bytes, VisualGuessScoreConfig};
use crate::data::dictionary_variant_data::build_dictionary_variants;
use crate::data::fonts_data::FontsData;
use crate::data::typography_width_data::{
    build_typography_profile_from_pdf_bytes, fallback_typography_width,
    measure_text_width_from_profile,
};
use crate::types::time::Instant;
use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect as FontRect};
use crate::types::guess_types::{
    GuessCandidate, GuessConfig, GuessContext, GuessReport, RedactionGuess,
};
use crate::types::redaction_types::{Rect, RedactionKind, RedactionOccurrence, RedactionReport};
use crate::types::runtime_defaults::{DEFAULT_FONT_METRICS_DPI, MULTI_SPAN_GAP_RATIO_THRESHOLD};
use crate::types::typography_types::{
    TypographyMeasureInput, TypographyMeasuredWidth, TypographyProfile, TypographyWidthSource,
};

pub struct RunGuessFromBytesRequest<'a> {
    pub pdf_name: &'a str,
    pub pdf_bytes: &'a [u8],
    pub redactions: &'a RedactionReport,
    pub dictionary: &'a [String],
    pub diagnostics: &'a [String],
    pub preloaded_font_runs: Option<&'a FontRunReport>,
    pub preloaded_font_runs_elapsed_ms: Option<u128>,
    pub cfg: &'a GuessConfig,
}

#[inline]
pub fn run_from_bytes(req: RunGuessFromBytesRequest<'_>) -> Result<GuessReport, String> {
    let started = Instant::now();
    let font_runs_started = Instant::now();
    let loaded_font_runs = match req.preloaded_font_runs {
        Some(_) => None,
        None => Some(
            FontsData::new()
                .load_font_runs_from_bytes(req.pdf_name, req.pdf_bytes)?
                .report,
        ),
    };
    let font_runs = req.preloaded_font_runs.unwrap_or_else(|| {
        loaded_font_runs
            .as_ref()
            .expect("font runs should exist after local load")
    });
    let font_runs_ms = req
        .preloaded_font_runs_elapsed_ms
        .unwrap_or_else(|| font_runs_started.elapsed().as_millis());
    let width_tables_started = Instant::now();
    let typography_profile =
        build_typography_profile_from_pdf_bytes(req.pdf_bytes).unwrap_or_default();
    let width_tables_ms = width_tables_started.elapsed().as_millis();
    let mut diagnostics = req.diagnostics.to_vec();
    diagnostics.push(format!(
        "timing_ms stage=guess_font_runs value={font_runs_ms}"
    ));
    diagnostics.push(format!(
        "timing_ms stage=guess_width_tables value={width_tables_ms}"
    ));
    let inputs = BuildReportWithFontsInputs {
        input_redactions_label: format!("memory://{}.redactions.json", req.pdf_name),
        input_fonts_label: format!("memory://{}.fonts.json", req.pdf_name),
        redactions: req.redactions.clone(),
        dictionary: req.dictionary.to_vec(),
        diagnostics,
        font_runs,
        typography_profile,
        pdf_bytes: Some(req.pdf_bytes),
    };
    let mut report = build_report_from_parts_with_fonts_inputs(inputs, req.cfg);
    report.diagnostics.push(format!(
        "timing_ms stage=guess_run_from_bytes_total value={}",
        started.elapsed().as_millis()
    ));
    Ok(report)
}

struct BuildReportWithFontsInputs<'a> {
    input_redactions_label: String,
    input_fonts_label: String,
    redactions: RedactionReport,
    dictionary: Vec<String>,
    diagnostics: Vec<String>,
    font_runs: &'a FontRunReport,
    typography_profile: TypographyProfile,
    pdf_bytes: Option<&'a [u8]>,
}

fn build_report_from_parts_with_fonts_inputs(
    inputs: BuildReportWithFontsInputs<'_>,
    cfg: &GuessConfig,
) -> GuessReport {
    let guess_anchor_started = Instant::now();
    let (mut guesses, guess_diagnostics) = build_anchor_validated_guesses(
        &inputs.redactions.redactions,
        &inputs.dictionary,
        inputs.font_runs,
        &inputs.typography_profile,
        cfg,
    );
    let guess_anchor_ms = guess_anchor_started.elapsed().as_millis();
    let mut all_diagnostics = inputs.diagnostics;
    all_diagnostics.extend(guess_diagnostics);
    all_diagnostics.push(format!(
        "timing_ms stage=guess_anchor_rows value={guess_anchor_ms}"
    ));
    if cfg.visual_score {
        let visual_started = Instant::now();
        let visual_cfg = VisualGuessScoreConfig {
            enabled: cfg.visual_score,
            dpi: cfg.visual_score_dpi,
            min_ink_pixels: FIXED_VISUAL_MIN_INK_PIXELS,
            drop_threshold: FIXED_VISUAL_DROP_THRESHOLD,
        };
        let visual_result = if let Some(pdf_bytes) = inputs.pdf_bytes {
            apply_visual_scores_from_bytes(
                pdf_bytes,
                &inputs.redactions,
                inputs.font_runs,
                &mut guesses,
                visual_cfg,
            )
        } else {
            all_diagnostics.push("visual_score=disabled_missing_pdf_bytes".to_owned());
            Ok(Vec::new())
        };
        match visual_result {
            Ok(visual_diagnostics) => all_diagnostics.extend(visual_diagnostics),
            Err(error) => {
                all_diagnostics.push(format!("visual_score_failed:{error}"));
            }
        }
        all_diagnostics.push(format!(
            "timing_ms stage=guess_visual_score value={}",
            visual_started.elapsed().as_millis()
        ));
    } else {
        all_diagnostics.push("visual_score=disabled".to_owned());
    }
    annotate_guess_confidence(&mut guesses);
    GuessReport {
        input_redactions: inputs.input_redactions_label,
        input_fonts: inputs.input_fonts_label,
        guesses,
        diagnostics: all_diagnostics,
    }
}

fn annotate_guess_confidence(guesses: &mut [RedactionGuess]) {
    for guess in guesses {
        let base = if !guess.exact_matches.is_empty() {
            1.0_f64
        } else {
            guess
                .candidates
                .first()
                .map(|candidate| candidate.score as f64)
                .unwrap_or(0.0_f64)
        };
        let anchor = if !guess.context.has_anchor_pair {
            0.35_f64
        } else if guess.context.anchor_mode.as_deref() == Some("two_sided") {
            1.0_f64
        } else {
            0.78_f64
        };
        let width = match guess
            .context
            .candidate_width_source
            .as_deref()
            .or(guess.context.anchor_width_source.as_deref())
        {
            Some("asset") => 1.0_f64,
            Some("pdf_width_table") => 0.93_f64,
            Some("core_font") => 0.82_f64,
            Some("fallback") => 0.64_f64,
            _ => 0.75_f64,
        };
        let visual = guess
            .visual_mean_abs_diff
            .map(|value| (1.0_f64 - (value as f64 / 0.28_f64)).clamp(0.30_f64, 1.0_f64))
            .unwrap_or(0.78_f64);
        let fallback_penalty = if guess.context.width_fallback_reason.is_some() {
            0.06_f64
        } else {
            0.0_f64
        };
        let confidence =
            (base * 0.50_f64 + anchor * 0.20_f64 + width * 0.20_f64 + visual * 0.10_f64
                - fallback_penalty)
                .clamp(0.0_f64, 1.0_f64);
        guess.context.confidence_score = Some(confidence as f32);
        guess.context.confidence_factors = Some(format!(
                "base={base:.3};anchor={anchor:.3};width={width:.3};visual={visual:.3};fallback_penalty={fallback_penalty:.3}"
            ));
    }
}

#[derive(Debug, Clone)]
enum AnchorMode {
    TwoSided,
    LeftOnly,
    RightOnly,
}

impl AnchorMode {
    fn as_str(&self) -> &'static str {
        match self {
            AnchorMode::TwoSided => "two_sided",
            AnchorMode::LeftOnly => "left_only",
            AnchorMode::RightOnly => "right_only",
        }
    }
}

#[derive(Debug, Clone)]
struct AnchorPairData {
    left_anchor_text: String,
    right_anchor_text: String,
    left_x: f64,
    right_x: f64,
    font_key: String,
    font_name: String,
    font_size_pt: f32,
    h_scale_pct: f32,
    left_bbox: FontRect,
    right_bbox: FontRect,
    epsilon_pt: f64,
    row_bias_pt: f64,
    mode: AnchorMode,
}

#[derive(Debug, Clone)]
struct ScoredDictionaryCandidate {
    text: String,
    raw_error_pt: f64,
    effective_error_pt: f64,
    word_count: u32,
    width_pt: f64,
    width_source: WidthSource,
}

#[derive(Debug, Clone, Copy, Default)]
struct CandidateFunnelMetrics {
    scanned: usize,
    after_char_units: usize,
    after_context: usize,
    after_shape: usize,
    after_anchor: usize,
    after_box: usize,
    scored: usize,
}

#[derive(Debug, Clone)]
struct JointAssignmentOption {
    text: String,
    key: String,
    base_cost: f64,
    start_x_pt: f64,
    end_x_pt: f64,
}

#[derive(Debug, Clone)]
struct JointAssignmentBeamState {
    cost: f64,
    selected: Vec<Option<String>>,
    used_keys: Vec<String>,
    prev_start_x_pt: f64,
    prev_end_x_pt: f64,
}

#[derive(Debug, Clone)]
struct PairCandidate {
    left_idx: usize,
    right_idx: usize,
    font_penalty: u8,
    hint_penalty: u8,
    contains_center_penalty: u8,
    baseline_distance: f64,
    y_distance: f64,
    x_distance: f64,
    gap_width: f64,
}

const FIXED_MAX_CANDIDATES: usize = 50;
const FIXED_TOLERANCE_PT: f64 = 4.0_f64;
const FIXED_VISUAL_MIN_INK_PIXELS: u32 = 64_u32;
const FIXED_VISUAL_DROP_THRESHOLD: Option<f32> = None;
const MULTI_SPAN_ANCHOR_PRIOR_WEIGHT: f64 = 0.15_f64;
const MULTI_SPAN_BOX_PRIOR_WEIGHT: f64 = 0.70_f64;
const SINGLE_SPAN_BOX_PRIOR_WEIGHT: f64 = 0.12_f64;
const RASTER_WIDTH_NOISE_PT: f64 = 2.50_f64;
const MULTI_SPAN_BOX_ERROR_RATIO: f64 = 0.45_f64;
const MULTI_SPAN_BOX_ERROR_PAD_PT: f64 = 2.5_f64;
const CLUSTER_CONSENSUS_MAX_GAP_RATIO: f64 = 1.9_f64;
const MULTI_SPAN_WIDTH_BAND_LIMIT: usize = 900;
const SINGLE_SPAN_WIDTH_BAND_LIMIT: usize = 700;
const JOINT_ASSIGNMENT_MIN_GROUP_ROWS: usize = 2;
const JOINT_ASSIGNMENT_MAX_ROWS: usize = 14;
const JOINT_ASSIGNMENT_MAX_OPTIONS_PER_ROW: usize = 24;
const JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT: usize = 500;
const JOINT_ASSIGNMENT_BEAM_WIDTH: usize = 160;
const JOINT_ASSIGNMENT_DUPLICATE_PENALTY: f64 = 6.0_f64;
const JOINT_ASSIGNMENT_OVERLAP_MARGIN_PT: f64 = 0.75_f64;
const JOINT_ASSIGNMENT_OVERLAP_PENALTY: f64 = 2.8_f64;
const JOINT_ASSIGNMENT_MAX_GROUP_GAP_PT: f64 = 140.0_f64;
const JOINT_ASSIGNMENT_NULL_DELTA: f64 = 0.75_f64;
const JOINT_ASSIGNMENT_NULL_MIN_BEST_COST: f64 = 1.4_f64;

type MeasuredWidth = TypographyMeasuredWidth;
type WidthSource = TypographyWidthSource;

struct WidthMeasureContext<'a> {
    page_index: u32,
    asset: Option<&'a FontAsset>,
    typography_profile: &'a TypographyProfile,
    h_scale_pct: f32,
}

struct RowCalibration {
    epsilon_pt: f64,
    bias_pt: f64,
}

struct AnchorHints<'a> {
    left_text: Option<&'a str>,
    left_x: Option<f64>,
    right_text: Option<&'a str>,
    right_x: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GuessClusterKey {
    page_index: u32,
    left_anchor_text: String,
    right_anchor_text: String,
    font_key: String,
    font_size_bits: u32,
    h_scale_bits: u32,
}

fn build_anchor_validated_guesses(
    redactions: &[RedactionOccurrence],
    dictionary: &[String],
    font_runs: &FontRunReport,
    typography_profile: &TypographyProfile,
    cfg: &GuessConfig,
) -> (Vec<RedactionGuess>, Vec<String>) {
    let dictionary_variants = build_dictionary_variants(dictionary);
    let assets = font_runs
        .assets
        .iter()
        .map(|asset| (asset.font_key.clone(), asset.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut by_page = std::collections::BTreeMap::<u32, Vec<&FontTextRun>>::new();
    for run in &font_runs.runs {
        by_page.entry(run.page_index).or_default().push(run);
    }
    for runs in by_page.values_mut() {
        runs.sort_by(|left_run, right_run| {
            left_run
                .bbox
                .x0
                .partial_cmp(&right_run.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left_run
                        .bbox
                        .y0
                        .partial_cmp(&right_run.bbox.y0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left_run.text.cmp(&right_run.text))
        });
    }

    let mut diagnostics = vec![format!(
        "font_run_count={} font_asset_count={} width_table_count={}",
        font_runs.runs.len(),
        font_runs.assets.len(),
        typography_profile.width_table_count()
    )];
    let mut guesses = Vec::with_capacity(redactions.len());

    for (index, redaction) in redactions.iter().enumerate() {
        let (left_text, right_text, left_bbox, right_bbox) = extract_context(redaction);
        let page_runs = by_page
            .get(&redaction.page_index)
            .cloned()
            .unwrap_or_default();
        let anchor = select_anchor_pair(redaction, &page_runs, &assets, typography_profile);
        let Some(anchor) = anchor else {
            diagnostics.push(format!(
                "redaction_index={index} page_index={} anchored_row=false reason=missing_anchor",
                redaction.page_index
            ));
            guesses.push(RedactionGuess {
                page_index: redaction.page_index,
                bbox: redaction.bbox,
                candidates: Vec::new(),
                exact_matches: Vec::new(),
                context: GuessContext {
                    left_anchor_text: left_text,
                    right_anchor_text: right_text,
                    gap_pt: compute_gap_pt(
                        redaction.bbox,
                        left_bbox,
                        right_bbox,
                        redaction
                            .underlying_text
                            .first()
                            .map(|hit| hit.text.as_str())
                            .unwrap_or_default(),
                        redaction
                            .underlying_text
                            .get(1)
                            .map(|hit| hit.text.as_str())
                            .unwrap_or_default(),
                    ) as f32,
                    char_width_pt: estimate_char_width_pt(
                        redaction
                            .underlying_text
                            .first()
                            .map(|hit| hit.text.as_str())
                            .unwrap_or_default(),
                        redaction
                            .underlying_text
                            .get(1)
                            .map(|hit| hit.text.as_str())
                            .unwrap_or_default(),
                        left_bbox,
                        right_bbox,
                        redaction.bbox,
                    ) as f32,
                    tol_pt: FIXED_TOLERANCE_PT as f32,
                    anchor_left_x: None,
                    anchor_right_x: None,
                    anchor_font_key: None,
                    anchor_font_name: None,
                    anchor_font_size_pt: None,
                    anchor_h_scale_pct: None,
                    anchor_row_bias_pt: None,
                    anchor_mode: None,
                    anchor_width_source: None,
                    space_width_source: None,
                    candidate_width_source: None,
                    width_fallback_reason: None,
                    confidence_score: None,
                    confidence_factors: None,
                    has_anchor_pair: false,
                },
                visual_compared_pixels: None,
                visual_mean_abs_diff: None,
                visual_changed_pixel_ratio: None,
                visual_reason: None,
                visual_dropped: false,
            });
            continue;
        };

        let (guess, funnel) = build_guess_for_anchor(
            redaction,
            &dictionary_variants,
            cfg,
            &anchor,
            &assets,
            typography_profile,
        );
        diagnostics.push(format!(
                "redaction_index={index} page_index={} anchor_mode={} funnel_scanned={} funnel_after_char_units={} funnel_after_context={} funnel_after_shape={} funnel_after_anchor={} funnel_after_box={} funnel_scored={}",
                redaction.page_index,
                anchor.mode.as_str(),
                funnel.scanned,
                funnel.after_char_units,
                funnel.after_context,
                funnel.after_shape,
                funnel.after_anchor,
                funnel.after_box,
                funnel.scored,
            ));
        guesses.push(guess);
    }

    apply_cluster_consensus(&mut guesses);
    let jointly_assigned = apply_row_joint_assignment(&mut guesses);
    apply_row_sequence_consensus(&mut guesses, &jointly_assigned);
    for (index, guess) in guesses.iter().enumerate() {
        if !guess.context.has_anchor_pair {
            continue;
        }
        let top = if let Some(first) = guess.exact_matches.first() {
            first.clone()
        } else {
            guess
                .candidates
                .first()
                .map(|candidate| candidate.text.clone())
                .unwrap_or_default()
        };
        diagnostics.push(format!(
                "redaction_index={index} page_index={} anchored_row=true exact_count={} candidate_count={} top_guess={} left_anchor=[{}] right_anchor=[{}] tol_pt={} anchor_mode={} anchor_width_source={} space_width_source={} candidate_width_source={} width_fallback_reason={}",
                guess.page_index,
                guess.exact_matches.len(),
                guess.candidates.len(),
                top,
                guess.context.left_anchor_text,
                guess.context.right_anchor_text,
                guess.context.tol_pt,
                guess.context
                    .anchor_mode
                    .as_deref()
                    .unwrap_or("unknown"),
                guess.context
                    .anchor_width_source
                    .as_deref()
                    .unwrap_or("unknown"),
                guess.context
                    .space_width_source
                    .as_deref()
                    .unwrap_or("unknown"),
                guess.context
                    .candidate_width_source
                    .as_deref()
                    .unwrap_or("unknown"),
                guess.context
                    .width_fallback_reason
                    .as_deref()
                    .unwrap_or("none"),
            ));
    }

    (guesses, diagnostics)
}

fn apply_cluster_consensus(guesses: &mut [RedactionGuess]) {
    let mut clusters = std::collections::BTreeMap::<GuessClusterKey, Vec<usize>>::new();
    for (index, guess) in guesses.iter().enumerate() {
        if !guess.context.has_anchor_pair || guess.candidates.is_empty() {
            continue;
        }
        if !is_two_sided_anchor_context(guess) {
            continue;
        }
        let redaction_width = (guess.bbox.width().abs() as f64).max(1.0_f64);
        let gap_ratio = (guess.context.gap_pt as f64).abs() / redaction_width;
        if gap_ratio >= CLUSTER_CONSENSUS_MAX_GAP_RATIO {
            continue;
        }
        let Some(font_key) = guess.context.anchor_font_key.clone() else {
            continue;
        };
        let Some(font_size_pt) = guess.context.anchor_font_size_pt else {
            continue;
        };
        let Some(h_scale_pct) = guess.context.anchor_h_scale_pct else {
            continue;
        };
        let key = GuessClusterKey {
            page_index: guess.page_index,
            left_anchor_text: guess.context.left_anchor_text.clone(),
            right_anchor_text: guess.context.right_anchor_text.clone(),
            font_key,
            font_size_bits: font_size_pt.to_bits(),
            h_scale_bits: h_scale_pct.to_bits(),
        };
        clusters.entry(key).or_default().push(index);
    }

    for indices in clusters.values() {
        if indices.len() < 2 {
            continue;
        }

        let mut aggregate = std::collections::BTreeMap::<String, (f64, u32)>::new();
        for index in indices {
            let guess = &guesses[*index];
            let denom = (guess.context.tol_pt as f64).max(0.0001_f64);
            for candidate in &guess.candidates {
                let entry = aggregate
                    .entry(candidate.text.clone())
                    .or_insert((0.0_f64, 0_u32));
                entry.0 += (candidate.error_pt as f64) / denom;
                entry.1 += 1;
            }
        }

        let cluster_size = indices.len() as u32;
        for index in indices {
            let guess = &mut guesses[*index];
            let mut local_error = std::collections::BTreeMap::<String, f64>::new();
            for candidate in &guess.candidates {
                local_error.insert(candidate.text.clone(), candidate.error_pt as f64);
            }

            guess.candidates.sort_by(|left_candidate, right_candidate| {
                let left_local = left_candidate.error_pt as f64;
                let right_local = right_candidate.error_pt as f64;
                let left_consensus = aggregate
                    .get(&left_candidate.text)
                    .map(|(score, count)| {
                        (*score / *count as f64)
                            + (cluster_size.saturating_sub(*count) as f64 * 1.5_f64)
                    })
                    .unwrap_or(f64::INFINITY);
                let right_consensus = aggregate
                    .get(&right_candidate.text)
                    .map(|(score, count)| {
                        (*score / *count as f64)
                            + (cluster_size.saturating_sub(*count) as f64 * 1.5_f64)
                    })
                    .unwrap_or(f64::INFINITY);
                left_consensus
                    .partial_cmp(&right_consensus)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        left_local
                            .partial_cmp(&right_local)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| {
                        left_candidate
                            .score
                            .partial_cmp(&right_candidate.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| left_candidate.word_count.cmp(&right_candidate.word_count))
                    .then_with(|| left_candidate.text.cmp(&right_candidate.text))
            });

            guess.exact_matches.sort_by(|left_text, right_text| {
                let left_local = local_error.get(left_text).copied().unwrap_or(f64::INFINITY);
                let right_local = local_error
                    .get(right_text)
                    .copied()
                    .unwrap_or(f64::INFINITY);
                let left_consensus = aggregate
                    .get(left_text)
                    .map(|(score, count)| {
                        (*score / *count as f64)
                            + (cluster_size.saturating_sub(*count) as f64 * 1.5_f64)
                    })
                    .unwrap_or(f64::INFINITY);
                let right_consensus = aggregate
                    .get(right_text)
                    .map(|(score, count)| {
                        (*score / *count as f64)
                            + (cluster_size.saturating_sub(*count) as f64 * 1.5_f64)
                    })
                    .unwrap_or(f64::INFINITY);
                left_consensus
                    .partial_cmp(&right_consensus)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        left_local
                            .partial_cmp(&right_local)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| left_text.cmp(right_text))
            });
        }
    }
}

fn is_multi_span_row_guess(guess: &RedactionGuess) -> bool {
    if !guess.context.has_anchor_pair {
        return false;
    }
    if !is_two_sided_anchor_context(guess) {
        return false;
    }
    let width = guess.bbox.width().abs() as f64;
    if width <= 0.0_f64 {
        return false;
    }
    (guess.context.gap_pt as f64).abs() / width >= MULTI_SPAN_GAP_RATIO_THRESHOLD
}

fn is_two_sided_anchor_context(guess: &RedactionGuess) -> bool {
    guess
        .context
        .anchor_mode
        .as_deref()
        .map(|mode| mode == "two_sided")
        .unwrap_or(true)
}

fn apply_row_joint_assignment(guesses: &mut [RedactionGuess]) -> std::collections::BTreeSet<usize> {
    let mut rows = std::collections::BTreeMap::<(u32, i32), Vec<usize>>::new();
    for (index, guess) in guesses.iter().enumerate() {
        if !guess.context.has_anchor_pair || guess.candidates.is_empty() {
            continue;
        }
        let center_y = ((guess.bbox.y0 + guess.bbox.y1) * 0.5_f32) as f64;
        let y_bucket = (center_y / 6.0_f64).round() as i32;
        rows.entry((guess.page_index, y_bucket))
            .or_default()
            .push(index);
    }

    let mut promotions = Vec::<(usize, String)>::new();
    for indices in rows.values_mut() {
        if indices.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
            continue;
        }
        indices.sort_by(|left_idx, right_idx| {
            guesses[*left_idx]
                .bbox
                .x0
                .partial_cmp(&guesses[*right_idx].bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    guesses[*left_idx]
                        .bbox
                        .x1
                        .partial_cmp(&guesses[*right_idx].bbox.x1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        let groups = collect_contiguous_multi_span_groups(indices, guesses);
        for group in groups {
            if let Some(selected) = solve_joint_assignment_group(guesses, &group) {
                for (guess_index, selected_text) in group.iter().copied().zip(selected.into_iter())
                {
                    if let Some(text) = selected_text {
                        promotions.push((guess_index, text));
                    }
                }
            }
        }
    }

    let mut assigned = std::collections::BTreeSet::<usize>::new();
    for (guess_index, selected_text) in promotions {
        if let Some(guess) = guesses.get_mut(guess_index) {
            promote_text_to_front(guess, &selected_text);
            assigned.insert(guess_index);
        }
    }
    assigned
}

fn collect_contiguous_multi_span_groups(
    indices: &[usize],
    guesses: &[RedactionGuess],
) -> Vec<Vec<usize>> {
    let mut groups = Vec::<Vec<usize>>::new();
    let mut current = Vec::<usize>::new();

    for guess_index in indices.iter().copied() {
        let guess = &guesses[guess_index];
        if !is_joint_assignment_candidate_row(guess) {
            if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
                groups.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            continue;
        }

        if current.is_empty() {
            current.push(guess_index);
            continue;
        }

        let prev_index = *current.last().unwrap_or(&guess_index);
        let prev = &guesses[prev_index];
        let x_gap = (guess.bbox.x0 as f64 - prev.bbox.x1 as f64).max(0.0_f64);
        let contiguous = x_gap <= JOINT_ASSIGNMENT_MAX_GROUP_GAP_PT
            && joint_assignment_rows_are_compatible(prev, guess);
        if contiguous {
            current.push(guess_index);
        } else {
            if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
                groups.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            current.push(guess_index);
        }
    }

    if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
        groups.push(current);
    }
    groups
        .into_iter()
        .filter(|group| {
            group.len() <= JOINT_ASSIGNMENT_MAX_ROWS
                && group_has_joint_assignment_signal(group, guesses)
        })
        .collect::<Vec<_>>()
}

fn is_joint_assignment_candidate_row(guess: &RedactionGuess) -> bool {
    guess.context.has_anchor_pair
        && !guess.candidates.is_empty()
        && is_two_sided_anchor_context(guess)
}

fn group_has_joint_assignment_signal(group: &[usize], guesses: &[RedactionGuess]) -> bool {
    group.iter().copied().any(|guess_index| {
        let guess = &guesses[guess_index];
        is_multi_span_row_guess(guess)
            || is_list_like_context(
                &guess.context.left_anchor_text,
                &guess.context.right_anchor_text,
            )
    })
}

fn joint_assignment_rows_are_compatible(left: &RedactionGuess, right: &RedactionGuess) -> bool {
    let same_font_key = match (
        left.context.anchor_font_key.as_deref(),
        right.context.anchor_font_key.as_deref(),
    ) {
        (Some(l), Some(r)) => l == r,
        _ => true,
    };
    let similar_font_size = match (
        left.context.anchor_font_size_pt,
        right.context.anchor_font_size_pt,
    ) {
        (Some(l), Some(r)) => (l - r).abs() <= 0.75_f32,
        _ => true,
    };
    let similar_h_scale = match (
        left.context.anchor_h_scale_pct,
        right.context.anchor_h_scale_pct,
    ) {
        (Some(l), Some(r)) => (l - r).abs() <= 8.0_f32,
        _ => true,
    };
    let close_row_bias = match (
        left.context.anchor_row_bias_pt,
        right.context.anchor_row_bias_pt,
    ) {
        (Some(l), Some(r)) => (l - r).abs() <= 5.0_f32,
        _ => true,
    };
    same_font_key && similar_font_size && similar_h_scale && close_row_bias
}

fn solve_joint_assignment_group(
    guesses: &[RedactionGuess],
    group: &[usize],
) -> Option<Vec<Option<String>>> {
    if group.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS || group.len() > JOINT_ASSIGNMENT_MAX_ROWS {
        return None;
    }

    let mut options_by_row = Vec::<Vec<JointAssignmentOption>>::with_capacity(group.len());
    let mut null_costs = Vec::<f64>::with_capacity(group.len());
    let mut allow_null_by_row = Vec::<bool>::with_capacity(group.len());
    for guess_index in group.iter().copied() {
        let guess = guesses.get(guess_index)?;
        let options = build_joint_assignment_options(
            guess,
            JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT,
            JOINT_ASSIGNMENT_MAX_OPTIONS_PER_ROW,
        );
        if options.is_empty() {
            return None;
        }
        let best_cost = options
            .first()
            .map(|option| option.base_cost)
            .unwrap_or(5.0_f64);
        null_costs.push(best_cost + JOINT_ASSIGNMENT_NULL_DELTA);
        allow_null_by_row.push(best_cost >= JOINT_ASSIGNMENT_NULL_MIN_BEST_COST);
        options_by_row.push(options);
    }

    let mut beam = vec![JointAssignmentBeamState {
        cost: 0.0_f64,
        selected: Vec::new(),
        used_keys: Vec::new(),
        prev_start_x_pt: f64::NEG_INFINITY,
        prev_end_x_pt: f64::NEG_INFINITY,
    }];
    let duplicate_penalty_amount = if group.len() >= 3 {
        JOINT_ASSIGNMENT_DUPLICATE_PENALTY
    } else {
        0.0_f64
    };

    for ((row_options, null_cost), allow_null) in options_by_row
        .iter()
        .zip(null_costs.iter().copied())
        .zip(allow_null_by_row.iter().copied())
    {
        let mut next = Vec::<JointAssignmentBeamState>::new();
        for state in &beam {
            if allow_null {
                let mut selected_skip = state.selected.clone();
                selected_skip.push(None);
                next.push(JointAssignmentBeamState {
                    cost: state.cost + null_cost,
                    selected: selected_skip,
                    used_keys: state.used_keys.clone(),
                    prev_start_x_pt: state.prev_start_x_pt,
                    prev_end_x_pt: state.prev_end_x_pt,
                });
            }
            for option in row_options {
                let mut cost = state.cost + option.base_cost;
                if duplicate_penalty_amount > 0.0_f64
                    && state.used_keys.iter().any(|key| key == &option.key)
                {
                    cost += duplicate_penalty_amount;
                }
                if state.prev_end_x_pt.is_finite() {
                    let overlap_pt = (state.prev_end_x_pt - option.start_x_pt
                        + JOINT_ASSIGNMENT_OVERLAP_MARGIN_PT)
                        .max(0.0_f64);
                    cost += overlap_pt * JOINT_ASSIGNMENT_OVERLAP_PENALTY;
                    if option.start_x_pt + 0.5_f64 < state.prev_start_x_pt {
                        cost += 2.5_f64;
                    }
                }

                let mut selected = state.selected.clone();
                selected.push(Some(option.text.clone()));
                let mut used_keys = state.used_keys.clone();
                if !used_keys.iter().any(|key| key == &option.key) {
                    used_keys.push(option.key.clone());
                }
                next.push(JointAssignmentBeamState {
                    cost,
                    selected,
                    used_keys,
                    prev_start_x_pt: option.start_x_pt,
                    prev_end_x_pt: option.end_x_pt,
                });
            }
        }

        if next.is_empty() {
            return None;
        }
        next.sort_by(|left, right| {
            left.cost
                .partial_cmp(&right.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        next.truncate(JOINT_ASSIGNMENT_BEAM_WIDTH);
        beam = next;
    }

    beam.into_iter().next().map(|state| state.selected)
}

fn build_joint_assignment_options(
    guess: &RedactionGuess,
    scan_limit: usize,
    max_options: usize,
) -> Vec<JointAssignmentOption> {
    if guess.candidates.is_empty() {
        return Vec::new();
    }
    let mut options = Vec::<JointAssignmentOption>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    let scan = guess.candidates.len().min(scan_limit);
    for (rank, candidate) in guess.candidates.iter().take(scan).enumerate() {
        let text = candidate.text.trim();
        if text.is_empty() {
            continue;
        }
        if !seen.insert(text.to_owned()) {
            continue;
        }
        let context_penalty = punctuation_context_penalty(
            &guess.context.left_anchor_text,
            &guess.context.right_anchor_text,
            text,
        );
        let width_penalty = candidate_width_penalty_pt(guess, text);
        let rank_penalty = ((rank + 2) as f64).ln() * 0.10_f64;
        let exact_bonus = if guess.exact_matches.iter().any(|value| value == text) {
            -0.25_f64
        } else {
            0.0_f64
        };
        let anchor_overlap_penalty = anchor_overlap_penalty_pt(
            &guess.context.left_anchor_text,
            &guess.context.right_anchor_text,
            text,
        );
        let base_cost = (candidate.error_pt as f64)
            + context_penalty
            + width_penalty
            + rank_penalty
            + exact_bonus
            + anchor_overlap_penalty;
        let (start_x_pt, end_x_pt) =
            estimate_candidate_interval_pt(guess, text, candidate.width_pt);
        options.push(JointAssignmentOption {
            text: text.to_owned(),
            key: normalize_candidate_key(text),
            base_cost,
            start_x_pt,
            end_x_pt,
        });
    }
    options.sort_by(|left, right| {
        left.base_cost
            .partial_cmp(&right.base_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    options.truncate(max_options);
    options
}

fn estimate_candidate_interval_pt(
    guess: &RedactionGuess,
    text: &str,
    measured_width_pt: Option<f32>,
) -> (f64, f64) {
    let char_width = (guess.context.char_width_pt as f64).max(0.1_f64);
    let fallback_width = candidate_char_units(text) * char_width;
    let width_pt = measured_width_pt
        .map(|value| value as f64)
        .filter(|value| value.is_finite() && *value > 0.0_f64)
        .unwrap_or(fallback_width)
        .max(0.1_f64);
    let start = match guess.context.anchor_mode.as_deref() {
        Some("right_only") => {
            let right_x = guess
                .context
                .anchor_right_x
                .map(|value| value as f64)
                .unwrap_or(guess.bbox.x1 as f64);
            (right_x - width_pt).min(guess.bbox.x1 as f64 - 0.1_f64)
        }
        Some("left_only") | Some("two_sided") | None | Some(_) => guess.bbox.x0 as f64,
    };
    (start, start + width_pt)
}

fn apply_row_sequence_consensus(
    guesses: &mut [RedactionGuess],
    skip_indices: &std::collections::BTreeSet<usize>,
) {
    let mut rows = std::collections::BTreeMap::<(u32, i32), Vec<usize>>::new();
    for (index, guess) in guesses.iter().enumerate() {
        if skip_indices.contains(&index) {
            continue;
        }
        if !guess.context.has_anchor_pair || guess.candidates.is_empty() {
            continue;
        }
        let center_y = ((guess.bbox.y0 + guess.bbox.y1) * 0.5_f32) as f64;
        let y_bucket = (center_y / 6.0_f64).round() as i32;
        rows.entry((guess.page_index, y_bucket))
            .or_default()
            .push(index);
    }

    for indices in rows.values_mut() {
        if indices.len() < 2 {
            continue;
        }
        indices.sort_by(|left_idx, right_idx| {
            guesses[*left_idx]
                .bbox
                .x0
                .partial_cmp(&guesses[*right_idx].bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    guesses[*left_idx]
                        .bbox
                        .x1
                        .partial_cmp(&guesses[*right_idx].bbox.x1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let mut used = std::collections::BTreeSet::<String>::new();
        let duplicate_penalty_amount = if indices.len() >= 3 { 3.0_f64 } else { 0.0_f64 };
        for guess_index in indices.iter().copied() {
            let guess = &mut guesses[guess_index];
            if guess.candidates.is_empty() {
                continue;
            }
            let mut best: Option<(String, f64)> = None;
            let max_scan = guess.candidates.len().min(80);
            for (rank, candidate) in guess.candidates.iter().take(max_scan).enumerate() {
                let key = normalize_candidate_key(&candidate.text);
                let duplicate_penalty = if duplicate_penalty_amount > 0.0_f64 && used.contains(&key)
                {
                    duplicate_penalty_amount
                } else {
                    0.0_f64
                };
                let width_penalty = candidate_width_penalty_pt(guess, &candidate.text);
                let rank_penalty = rank as f64 * 0.05_f64;
                let cost =
                    candidate.error_pt as f64 + duplicate_penalty + width_penalty + rank_penalty;
                match &best {
                    None => best = Some((candidate.text.clone(), cost)),
                    Some((_, best_cost)) if cost < *best_cost => {
                        best = Some((candidate.text.clone(), cost))
                    }
                    _ => {}
                }
            }
            let Some((selected_text, _)) = best else {
                continue;
            };
            used.insert(normalize_candidate_key(&selected_text));
            promote_text_to_front(guess, &selected_text);
        }
    }
}

fn normalize_candidate_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

fn candidate_width_penalty_pt(guess: &RedactionGuess, text: &str) -> f64 {
    let char_width = (guess.context.char_width_pt as f64).max(0.1_f64);
    let target = guess.bbox.width().abs() as f64;
    if target <= 0.0_f64 {
        return 0.0_f64;
    }
    let estimated = candidate_char_units(text) * char_width;
    ((estimated - target).abs() / target.max(1.0_f64)).min(3.0_f64)
}

fn candidate_char_units(text: &str) -> f64 {
    let glyph_count = text
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != ',')
        .count()
        .max(1) as f64;
    let spaces = text.chars().filter(|ch| ch.is_whitespace()).count() as f64;
    glyph_count + spaces * 0.45_f64
}

fn char_unit_band(target_width_pt: f64, char_width_pt: f64, tolerance_pt: f64) -> (f64, f64) {
    if !target_width_pt.is_finite()
        || target_width_pt <= 0.0_f64
        || !char_width_pt.is_finite()
        || char_width_pt <= 0.0_f64
    {
        return (1.0_f64, f64::INFINITY);
    }
    let target_units = target_width_pt / char_width_pt.max(0.1_f64);
    let slack = (tolerance_pt.abs() / char_width_pt.max(0.1_f64)).max(2.0_f64) + 2.0_f64;
    let lower = (target_units - slack).max(1.0_f64);
    let upper = (target_units + slack).max(lower + 1.0_f64);
    (lower, upper)
}

fn anchor_overlap_penalty_pt(
    left_anchor_text: &str,
    right_anchor_text: &str,
    candidate: &str,
) -> f64 {
    let anchor_tokens = tokenize_alpha_words(left_anchor_text)
        .into_iter()
        .chain(tokenize_alpha_words(right_anchor_text))
        .collect::<std::collections::BTreeSet<_>>();
    if anchor_tokens.is_empty() {
        return 0.0_f64;
    }
    let candidate_tokens = tokenize_alpha_words(candidate);
    if candidate_tokens.is_empty() {
        return 0.0_f64;
    }
    let mut matches = 0_u32;
    for token in &candidate_tokens {
        if anchor_tokens.contains(token) {
            matches += 1;
        }
    }
    if matches == 0 {
        return 0.0_f64;
    }
    let mut penalty = matches as f64 * 0.45_f64;
    if matches >= 2 {
        penalty += 0.35_f64;
    }
    penalty
}

fn tokenize_alpha_words(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '\'' && ch != '-')
        .map(|token| token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric()))
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>()
}

fn promote_text_to_front(guess: &mut RedactionGuess, selected_text: &str) {
    if let Some(pos) = guess
        .candidates
        .iter()
        .position(|candidate| candidate.text == selected_text)
    {
        let chosen = guess.candidates.remove(pos);
        guess.candidates.insert(0, chosen);
    }
    if let Some(pos) = guess
        .exact_matches
        .iter()
        .position(|value| value == selected_text)
    {
        let chosen = guess.exact_matches.remove(pos);
        guess.exact_matches.insert(0, chosen);
    }
}

fn effective_box_error_pt(
    redaction: &RedactionOccurrence,
    candidate_width_pt: f64,
    target_width_pt: f64,
) -> f64 {
    let raw = (candidate_width_pt - target_width_pt).abs();
    if matches!(redaction.kind, RedactionKind::RasterDarkRegion) {
        (raw - RASTER_WIDTH_NOISE_PT).max(0.0_f64)
    } else {
        raw
    }
}

fn build_guess_for_anchor(
    redaction: &RedactionOccurrence,
    dictionary_variants: &[String],
    _cfg: &GuessConfig,
    anchor: &AnchorPairData,
    assets: &std::collections::BTreeMap<String, FontAsset>,
    typography_profile: &TypographyProfile,
) -> (RedactionGuess, CandidateFunnelMetrics) {
    let mut funnel = CandidateFunnelMetrics::default();
    let asset = assets.get(&anchor.font_key);
    let fallback_char_width = estimate_char_width_pt(
        &anchor.left_anchor_text,
        &anchor.right_anchor_text,
        Some(Rect::new(
            anchor.left_bbox.x0,
            anchor.left_bbox.y0,
            anchor.left_bbox.x1,
            anchor.left_bbox.y1,
        )),
        Some(Rect::new(
            anchor.right_bbox.x0,
            anchor.right_bbox.y0,
            anchor.right_bbox.x1,
            anchor.right_bbox.y1,
        )),
        redaction.bbox,
    );
    let fallback_space_width = (fallback_char_width * 0.5_f64).max(0.5_f64);

    let measure_width = |text: &str| {
        let measured = measure_text_width_from_sources(
            &TypographyMeasureInput {
                page_index: redaction.page_index,
                font_key: &anchor.font_key,
                font_name: &anchor.font_name,
                font_size_pt: anchor.font_size_pt,
                h_scale_pct: anchor.h_scale_pct,
                text,
                metrics_dpi: DEFAULT_FONT_METRICS_DPI,
            },
            asset,
            typography_profile,
        );
        measured.or_else(|| {
            Some(fallback_typography_width(
                text,
                fallback_char_width,
                fallback_space_width,
                DEFAULT_FONT_METRICS_DPI,
            ))
        })
    };

    let has_width_table_for_anchor =
        typography_profile.has_width_table_for_font(redaction.page_index, &anchor.font_key);
    let left_anchor_text = anchor.left_anchor_text.trim();
    let left_width = measure_width(left_anchor_text).unwrap_or_else(|| {
        fallback_typography_width(
            left_anchor_text,
            fallback_char_width,
            fallback_space_width,
            DEFAULT_FONT_METRICS_DPI,
        )
    });
    let space_width = measure_width(" ").unwrap_or_else(|| {
        fallback_typography_width(
            " ",
            fallback_char_width,
            fallback_space_width,
            DEFAULT_FONT_METRICS_DPI,
        )
    });
    let redaction_width_pt = (redaction.bbox.width().abs() as f64).max(1.0_f64);
    let anchor_gap_pt = (anchor.right_x - anchor.left_x).abs().max(1.0_f64);
    let gap_ratio = anchor_gap_pt / redaction_width_pt;
    let multi_span_mode =
        matches!(anchor.mode, AnchorMode::TwoSided) && gap_ratio >= MULTI_SPAN_GAP_RATIO_THRESHOLD;
    let (min_char_units, max_char_units) = char_unit_band(
        redaction_width_pt,
        fallback_char_width.max(0.1_f64),
        anchor.epsilon_pt.max(FIXED_TOLERANCE_PT),
    );
    let candidate_width_index = build_row_candidate_width_index(
        dictionary_variants,
        min_char_units,
        max_char_units,
        &measure_width,
    );
    if candidate_width_index.is_empty() {
        return (
            RedactionGuess {
                page_index: redaction.page_index,
                bbox: redaction.bbox,
                candidates: Vec::new(),
                exact_matches: Vec::new(),
                context: GuessContext {
                    left_anchor_text: anchor.left_anchor_text.clone(),
                    right_anchor_text: anchor.right_anchor_text.clone(),
                    gap_pt: (anchor.right_x - anchor.left_x) as f32,
                    char_width_pt: fallback_char_width as f32,
                    tol_pt: anchor.epsilon_pt as f32,
                    anchor_left_x: Some(anchor.left_x as f32),
                    anchor_right_x: Some(anchor.right_x as f32),
                    anchor_font_key: Some(anchor.font_key.clone()),
                    anchor_font_name: Some(anchor.font_name.clone()),
                    anchor_font_size_pt: Some(anchor.font_size_pt),
                    anchor_h_scale_pct: Some(anchor.h_scale_pct),
                    anchor_row_bias_pt: Some(anchor.row_bias_pt as f32),
                    anchor_mode: Some(anchor.mode.as_str().to_owned()),
                    anchor_width_source: None,
                    space_width_source: None,
                    candidate_width_source: None,
                    width_fallback_reason: None,
                    confidence_score: None,
                    confidence_factors: None,
                    has_anchor_pair: true,
                },
                visual_compared_pixels: None,
                visual_mean_abs_diff: None,
                visual_changed_pixel_ratio: None,
                visual_reason: None,
                visual_dropped: false,
            },
            funnel,
        );
    }
    let mut scored = Vec::new();
    let anchor_filter_limit_pt =
        (anchor.epsilon_pt.max(1.0_f64) * 4.0_f64).max(FIXED_TOLERANCE_PT.max(4.0_f64));
    let box_filter_limit_pt = (redaction_width_pt * MULTI_SPAN_BOX_ERROR_RATIO
        + MULTI_SPAN_BOX_ERROR_PAD_PT)
        .max(anchor.epsilon_pt.max(2.5_f64));
    let list_like_context =
        is_list_like_context(&anchor.left_anchor_text, &anchor.right_anchor_text);
    if multi_span_mode {
        let lower_width = (redaction_width_pt - box_filter_limit_pt).max(0.0_f64);
        let upper_width = redaction_width_pt + box_filter_limit_pt;
        let ranged =
            candidate_width_entries_in_range(&candidate_width_index, lower_width, upper_width);
        let mut band = if ranged.is_empty() {
            candidate_width_index.as_slice()
        } else {
            ranged
        };
        band = trim_width_band_around_target(band, redaction_width_pt, MULTI_SPAN_WIDTH_BAND_LIMIT);
        for entry in band {
            funnel.scanned += 1;
            let trimmed = entry.text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let char_units = candidate_char_units(trimmed);
            if char_units < min_char_units || char_units > max_char_units {
                continue;
            }
            funnel.after_char_units += 1;
            if !passes_context_filter(&anchor.left_anchor_text, &anchor.right_anchor_text, trimmed)
            {
                continue;
            }
            funnel.after_context += 1;
            if list_like_context && !looks_like_alpha_phrase_candidate(trimmed) {
                continue;
            }
            funnel.after_shape += 1;

            let predicted_right = anchor.left_x
                + left_width.pt
                + space_width.pt
                + entry.width_pt
                + space_width.pt
                + anchor.row_bias_pt;
            let anchor_err = (predicted_right - anchor.right_x).abs();
            funnel.after_anchor += 1;
            let box_err = effective_box_error_pt(redaction, entry.width_pt, redaction_width_pt);
            if box_err > box_filter_limit_pt {
                continue;
            }
            funnel.after_box += 1;
            let raw_err = (box_err * MULTI_SPAN_BOX_PRIOR_WEIGHT)
                + (anchor_err * MULTI_SPAN_ANCHOR_PRIOR_WEIGHT);
            let context_penalty = punctuation_context_penalty(
                &anchor.left_anchor_text,
                &anchor.right_anchor_text,
                trimmed,
            );
            let effective_err = raw_err + context_penalty;
            scored.push(ScoredDictionaryCandidate {
                text: trimmed.to_owned(),
                raw_error_pt: raw_err,
                effective_error_pt: effective_err,
                word_count: trimmed.split_whitespace().count() as u32,
                width_pt: entry.width_pt,
                width_source: entry.source,
            });
        }
    } else {
        let single_span_width_slack_pt = (anchor.epsilon_pt.max(FIXED_TOLERANCE_PT) * 1.75_f64)
            .max(redaction_width_pt * 0.45_f64)
            .max(12.0_f64);
        let lower_width = (redaction_width_pt - single_span_width_slack_pt).max(0.0_f64);
        let upper_width = redaction_width_pt + single_span_width_slack_pt;
        let ranged =
            candidate_width_entries_in_range(&candidate_width_index, lower_width, upper_width);
        let mut band = if ranged.is_empty() {
            candidate_width_index.as_slice()
        } else {
            ranged
        };
        band =
            trim_width_band_around_target(band, redaction_width_pt, SINGLE_SPAN_WIDTH_BAND_LIMIT);
        for entry in band {
            let trimmed = entry.text.trim();
            if trimmed.is_empty() {
                continue;
            }
            funnel.scanned += 1;
            let char_units = candidate_char_units(trimmed);
            if char_units < min_char_units || char_units > max_char_units {
                continue;
            }
            funnel.after_char_units += 1;
            if !passes_context_filter(&anchor.left_anchor_text, &anchor.right_anchor_text, trimmed)
            {
                continue;
            }
            funnel.after_context += 1;
            if list_like_context && !looks_like_alpha_phrase_candidate(trimmed) {
                continue;
            }
            funnel.after_shape += 1;
            let box_err = effective_box_error_pt(redaction, entry.width_pt, redaction_width_pt);
            let side_alignment_err = match anchor.mode {
                AnchorMode::TwoSided => {
                    let predicted_right = anchor.left_x
                        + left_width.pt
                        + space_width.pt
                        + entry.width_pt
                        + space_width.pt
                        + anchor.row_bias_pt;
                    (predicted_right - anchor.right_x).abs()
                }
                AnchorMode::LeftOnly => {
                    let predicted_start =
                        anchor.left_x + left_width.pt + space_width.pt + anchor.row_bias_pt;
                    (predicted_start - redaction.bbox.x0 as f64).abs()
                }
                AnchorMode::RightOnly => {
                    let predicted_end = anchor.right_x - space_width.pt + anchor.row_bias_pt;
                    (predicted_end - redaction.bbox.x1 as f64).abs()
                }
            };
            let side_alignment_limit = match anchor.mode {
                AnchorMode::TwoSided => anchor_filter_limit_pt,
                AnchorMode::LeftOnly | AnchorMode::RightOnly => anchor_filter_limit_pt * 2.5_f64,
            };
            if side_alignment_err > side_alignment_limit {
                continue;
            }
            funnel.after_anchor += 1;
            funnel.after_box += 1;
            let raw_err = match anchor.mode {
                AnchorMode::TwoSided => {
                    side_alignment_err + (box_err * SINGLE_SPAN_BOX_PRIOR_WEIGHT)
                }
                AnchorMode::LeftOnly | AnchorMode::RightOnly => {
                    box_err + (side_alignment_err * 0.20_f64)
                }
            };
            let context_penalty = punctuation_context_penalty(
                &anchor.left_anchor_text,
                &anchor.right_anchor_text,
                trimmed,
            );
            let effective_err = raw_err + context_penalty;
            scored.push(ScoredDictionaryCandidate {
                text: trimmed.to_owned(),
                raw_error_pt: raw_err,
                effective_error_pt: effective_err,
                word_count: trimmed.split_whitespace().count() as u32,
                width_pt: entry.width_pt,
                width_source: entry.source,
            });
        }
    }
    funnel.scored = scored.len();
    scored.sort_by(|left_candidate, right_candidate| {
        left_candidate
            .effective_error_pt
            .partial_cmp(&right_candidate.effective_error_pt)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left_candidate.word_count.cmp(&right_candidate.word_count))
            .then_with(|| left_candidate.text.cmp(&right_candidate.text))
    });

    let epsilon = anchor.epsilon_pt.max(0.0);
    let exact_scored = scored
        .iter()
        .filter(|candidate| candidate.effective_error_pt <= epsilon)
        .cloned()
        .collect::<Vec<_>>();
    let exact_matches = exact_scored
        .iter()
        .map(|candidate| candidate.text.clone())
        .collect::<Vec<_>>();

    let selected = if exact_scored.is_empty() {
        scored
            .iter()
            .take(FIXED_MAX_CANDIDATES)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        exact_scored
            .iter()
            .take(FIXED_MAX_CANDIDATES)
            .cloned()
            .collect::<Vec<_>>()
    };

    let denom = if exact_scored.is_empty() {
        FIXED_TOLERANCE_PT.max(0.0001)
    } else {
        epsilon.max(0.0001)
    };
    let candidates = selected
        .iter()
        .map(|candidate| GuessCandidate {
            text: candidate.text.clone(),
            score: (1.0 - (candidate.effective_error_pt / denom)).clamp(0.0, 1.0) as f32,
            error_pt: candidate.raw_error_pt as f32,
            word_count: candidate.word_count,
            width_pt: Some(candidate.width_pt as f32),
        })
        .collect::<Vec<_>>();
    let candidate_width_source = selected
        .first()
        .map(|candidate| candidate.width_source.as_str().to_owned());
    let mut width_fallback_parts = Vec::<&str>::new();
    if asset.is_none() {
        width_fallback_parts.push("font_asset_missing");
    }
    if !has_width_table_for_anchor {
        width_fallback_parts.push("width_table_missing");
    }
    let width_fallback_reason = if width_fallback_parts.is_empty() {
        None
    } else {
        Some(width_fallback_parts.join("+"))
    };

    let char_width = if !anchor.left_anchor_text.trim().is_empty() {
        let chars = anchor.left_anchor_text.trim().chars().count().max(1) as f64;
        (left_width.pt / chars).max(0.0)
    } else {
        fallback_char_width
    };

    let guess = RedactionGuess {
        page_index: redaction.page_index,
        bbox: redaction.bbox,
        candidates,
        exact_matches,
        context: GuessContext {
            left_anchor_text: anchor.left_anchor_text.clone(),
            right_anchor_text: anchor.right_anchor_text.clone(),
            gap_pt: (anchor.right_x - anchor.left_x) as f32,
            char_width_pt: char_width as f32,
            tol_pt: epsilon as f32,
            anchor_left_x: Some(anchor.left_x as f32),
            anchor_right_x: Some(anchor.right_x as f32),
            anchor_font_key: Some(anchor.font_key.clone()),
            anchor_font_name: Some(anchor.font_name.clone()),
            anchor_font_size_pt: Some(anchor.font_size_pt),
            anchor_h_scale_pct: Some(anchor.h_scale_pct),
            anchor_row_bias_pt: Some(anchor.row_bias_pt as f32),
            anchor_mode: Some(anchor.mode.as_str().to_owned()),
            anchor_width_source: Some(left_width.source.as_str().to_owned()),
            space_width_source: Some(space_width.source.as_str().to_owned()),
            candidate_width_source,
            width_fallback_reason,
            confidence_score: None,
            confidence_factors: None,
            has_anchor_pair: true,
        },
        visual_compared_pixels: None,
        visual_mean_abs_diff: None,
        visual_changed_pixel_ratio: None,
        visual_reason: None,
        visual_dropped: false,
    };
    (guess, funnel)
}

fn select_anchor_pair(
    redaction: &RedactionOccurrence,
    runs: &[&FontTextRun],
    assets: &std::collections::BTreeMap<String, FontAsset>,
    typography_profile: &TypographyProfile,
) -> Option<AnchorPairData> {
    let red_center_y = rect_center_y(&redaction.bbox);
    let red_center_x = ((redaction.bbox.x0 + redaction.bbox.x1) * 0.5) as f64;
    let left_hint_hit = redaction
        .underlying_text
        .first()
        .filter(|hit| !hit.text.trim().is_empty());
    let right_hint_hit = redaction
        .underlying_text
        .get(1)
        .filter(|hit| !hit.text.trim().is_empty());
    let left_hint = left_hint_hit.map(|hit| hit.text.trim());
    let right_hint = right_hint_hit.map(|hit| hit.text.trim());
    let hints = AnchorHints {
        left_text: left_hint,
        left_x: left_hint_hit.map(|hit| hit.bbox.x0 as f64),
        right_text: right_hint,
        right_x: right_hint_hit.map(|hit| hit.bbox.x0 as f64),
    };
    let row_runs = sorted_row_runs_for_anchor(redaction, runs);
    if row_runs.is_empty() {
        return None;
    }
    if let Some(anchor) =
        try_same_run_hint_anchor(redaction, &row_runs, &hints, assets, typography_profile)
    {
        return Some(anchor);
    }

    let mut pairs = build_pair_candidates(
        &row_runs,
        redaction,
        left_hint,
        right_hint,
        red_center_x,
        red_center_y,
    );
    if pairs.is_empty() {
        return recover_one_sided_anchor(redaction, &row_runs, &hints, assets, typography_profile);
    }
    sort_pair_candidates(&mut pairs);
    let Some(selected_pair) = pairs
        .iter()
        .find(|pair| pair.font_penalty == 0)
        .or_else(|| pairs.first())
    else {
        return recover_one_sided_anchor(redaction, &row_runs, &hints, assets, typography_profile);
    };

    build_two_sided_anchor_from_pair(
        selected_pair,
        redaction,
        &row_runs,
        &hints,
        assets,
        typography_profile,
    )
    .or_else(|| recover_one_sided_anchor(redaction, &row_runs, &hints, assets, typography_profile))
}

fn sorted_row_runs_for_anchor<'a>(
    redaction: &RedactionOccurrence,
    runs: &[&'a FontTextRun],
) -> Vec<&'a FontTextRun> {
    let mut row_runs = collect_row_runs_for_anchor(redaction, runs, true);
    if row_runs.is_empty() {
        row_runs = collect_row_runs_for_anchor(redaction, runs, false);
    }
    row_runs.sort_by(|left_run, right_run| {
        left_run
            .bbox
            .x0
            .partial_cmp(&right_run.bbox.x0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left_run
                    .bbox
                    .y0
                    .partial_cmp(&right_run.bbox.y0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left_run.text.cmp(&right_run.text))
    });
    row_runs
}

fn try_same_run_hint_anchor(
    redaction: &RedactionOccurrence,
    row_runs: &[&FontTextRun],
    hints: &AnchorHints<'_>,
    assets: &std::collections::BTreeMap<String, FontAsset>,
    typography_profile: &TypographyProfile,
) -> Option<AnchorPairData> {
    let (Some(left_hint_text), Some(right_hint_text)) = (hints.left_text, hints.right_text) else {
        return None;
    };
    let same_run = row_runs.iter().copied().find(|run| {
        let run_text = run.text.trim();
        text_matches(run_text, left_hint_text) && text_matches(run_text, right_hint_text)
    })?;
    let asset = assets.get(&same_run.font_key);
    let (left_anchor_text, left_x) = resolve_anchor_text_and_x(
        redaction.page_index,
        same_run,
        Some(left_hint_text),
        hints.left_x,
        asset,
        typography_profile,
    );
    let (right_anchor_text, right_x) = resolve_anchor_text_and_x(
        redaction.page_index,
        same_run,
        Some(right_hint_text),
        hints.right_x,
        asset,
        typography_profile,
    );
    if right_x <= left_x {
        return None;
    }
    let measure_ctx = WidthMeasureContext {
        page_index: redaction.page_index,
        asset,
        typography_profile,
        h_scale_pct: same_run.h_scale_pct,
    };
    let calibration = estimate_row_epsilon(row_runs, same_run, redaction, &measure_ctx);
    Some(AnchorPairData {
        left_anchor_text,
        right_anchor_text,
        left_x,
        right_x,
        font_key: same_run.font_key.clone(),
        font_name: same_run.font_name.clone(),
        font_size_pt: same_run.font_size_pt,
        h_scale_pct: same_run.h_scale_pct,
        left_bbox: same_run.bbox,
        right_bbox: same_run.bbox,
        epsilon_pt: calibration.epsilon_pt,
        row_bias_pt: calibration.bias_pt,
        mode: AnchorMode::TwoSided,
    })
}

fn build_pair_candidates(
    row_runs: &[&FontTextRun],
    redaction: &RedactionOccurrence,
    left_hint: Option<&str>,
    right_hint: Option<&str>,
    red_center_x: f64,
    red_center_y: f32,
) -> Vec<PairCandidate> {
    let mut pairs = Vec::<PairCandidate>::new();
    for left_idx in 0..row_runs.len() {
        for right_idx in (left_idx + 1)..row_runs.len() {
            let left_run = row_runs[left_idx];
            let right_run = row_runs[right_idx];
            let left_end = left_run.bbox.x1 as f64;
            let right_start = right_run.bbox.x0 as f64;
            if right_start <= left_end {
                continue;
            }
            let font_penalty = if left_run.font_key == right_run.font_key
                && (left_run.font_size_pt - right_run.font_size_pt).abs() <= 2.0_f32
            {
                0_u8
            } else {
                1_u8
            };
            let left_hint_penalty = match left_hint {
                Some(hint) => u8::from(!text_matches(left_run.text.trim(), hint)),
                None => 0_u8,
            };
            let right_hint_penalty = match right_hint {
                Some(hint) => u8::from(!text_matches(right_run.text.trim(), hint)),
                None => 0_u8,
            };
            let contains_center_penalty =
                u8::from(red_center_x < left_end || red_center_x > right_start);
            let y_distance = (run_center_y(left_run) - red_center_y).abs()
                + (run_center_y(right_run) - red_center_y).abs();
            let baseline_distance = (left_run.bbox.y1 - redaction.bbox.y1).abs()
                + (right_run.bbox.y1 - redaction.bbox.y1).abs();
            let x_distance = (redaction.bbox.x0 as f64 - left_end).abs()
                + (right_start - redaction.bbox.x1 as f64).abs();
            pairs.push(PairCandidate {
                left_idx,
                right_idx,
                font_penalty,
                hint_penalty: left_hint_penalty + right_hint_penalty,
                contains_center_penalty,
                baseline_distance: baseline_distance as f64,
                y_distance: y_distance as f64,
                x_distance,
                gap_width: right_start - left_end,
            });
        }
    }
    pairs
}

fn sort_pair_candidates(pairs: &mut [PairCandidate]) {
    pairs.sort_by(|left_pair, right_pair| {
        left_pair
            .font_penalty
            .cmp(&right_pair.font_penalty)
            .then_with(|| left_pair.hint_penalty.cmp(&right_pair.hint_penalty))
            .then_with(|| {
                left_pair
                    .contains_center_penalty
                    .cmp(&right_pair.contains_center_penalty)
            })
            .then_with(|| {
                left_pair
                    .baseline_distance
                    .partial_cmp(&right_pair.baseline_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left_pair
                    .y_distance
                    .partial_cmp(&right_pair.y_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left_pair
                    .x_distance
                    .partial_cmp(&right_pair.x_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left_pair
                    .gap_width
                    .partial_cmp(&right_pair.gap_width)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
}

fn build_two_sided_anchor_from_pair(
    selected_pair: &PairCandidate,
    redaction: &RedactionOccurrence,
    row_runs: &[&FontTextRun],
    hints: &AnchorHints<'_>,
    assets: &std::collections::BTreeMap<String, FontAsset>,
    typography_profile: &TypographyProfile,
) -> Option<AnchorPairData> {
    let left_run = row_runs[selected_pair.left_idx];
    let right_run = row_runs[selected_pair.right_idx];
    let asset = assets
        .get(&left_run.font_key)
        .or_else(|| assets.get(&right_run.font_key));
    let (left_anchor_text, left_x) = resolve_anchor_text_and_x(
        redaction.page_index,
        left_run,
        hints.left_text,
        hints.left_x,
        asset,
        typography_profile,
    );
    let (right_anchor_text, right_x) = resolve_anchor_text_and_x(
        redaction.page_index,
        right_run,
        hints.right_text,
        hints.right_x,
        asset,
        typography_profile,
    );
    if left_anchor_text.trim().is_empty() || right_anchor_text.trim().is_empty() {
        return None;
    }
    if right_x <= left_x {
        return None;
    }
    let measure_ctx = WidthMeasureContext {
        page_index: redaction.page_index,
        asset,
        typography_profile,
        h_scale_pct: left_run.h_scale_pct,
    };
    let calibration = estimate_row_epsilon(row_runs, left_run, redaction, &measure_ctx);
    Some(AnchorPairData {
        left_anchor_text,
        right_anchor_text,
        left_x,
        right_x,
        font_key: left_run.font_key.clone(),
        font_name: left_run.font_name.clone(),
        font_size_pt: left_run.font_size_pt,
        h_scale_pct: left_run.h_scale_pct,
        left_bbox: left_run.bbox,
        right_bbox: right_run.bbox,
        epsilon_pt: calibration.epsilon_pt,
        row_bias_pt: calibration.bias_pt,
        mode: AnchorMode::TwoSided,
    })
}

fn recover_one_sided_anchor(
    redaction: &RedactionOccurrence,
    row_runs: &[&FontTextRun],
    hints: &AnchorHints<'_>,
    assets: &std::collections::BTreeMap<String, FontAsset>,
    typography_profile: &TypographyProfile,
) -> Option<AnchorPairData> {
    let left_only = row_runs
        .iter()
        .copied()
        .filter(|run| run.bbox.x1 <= redaction.bbox.x0 + 1.5_f32)
        .min_by(|left_run, right_run| {
            let left_gap = (redaction.bbox.x0 as f64 - left_run.bbox.x1 as f64).max(0.0_f64);
            let right_gap = (redaction.bbox.x0 as f64 - right_run.bbox.x1 as f64).max(0.0_f64);
            left_gap
                .partial_cmp(&right_gap)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    (run_center_y(left_run) - rect_center_y(&redaction.bbox))
                        .abs()
                        .partial_cmp(
                            &(run_center_y(right_run) - rect_center_y(&redaction.bbox)).abs(),
                        )
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    let right_only = row_runs
        .iter()
        .copied()
        .filter(|run| run.bbox.x0 >= redaction.bbox.x1 - 1.5_f32)
        .min_by(|left_run, right_run| {
            let left_gap = (left_run.bbox.x0 as f64 - redaction.bbox.x1 as f64).max(0.0_f64);
            let right_gap = (right_run.bbox.x0 as f64 - redaction.bbox.x1 as f64).max(0.0_f64);
            left_gap
                .partial_cmp(&right_gap)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    (run_center_y(left_run) - rect_center_y(&redaction.bbox))
                        .abs()
                        .partial_cmp(
                            &(run_center_y(right_run) - rect_center_y(&redaction.bbox)).abs(),
                        )
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

    if let Some(left_run) = left_only {
        let asset = assets.get(&left_run.font_key);
        let (left_anchor_text, left_x) = resolve_anchor_text_and_x(
            redaction.page_index,
            left_run,
            hints.left_text,
            hints.left_x,
            asset,
            typography_profile,
        );
        let right_anchor_text = hints.right_text.unwrap_or_default().to_owned();
        let right_x = (redaction.bbox.x1 as f64 + 0.5_f64).max(left_x + 1.0_f64);
        if !left_anchor_text.trim().is_empty() && right_x > left_x {
            let measure_ctx = WidthMeasureContext {
                page_index: redaction.page_index,
                asset,
                typography_profile,
                h_scale_pct: left_run.h_scale_pct,
            };
            let mut calibration = estimate_row_epsilon(row_runs, left_run, redaction, &measure_ctx);
            calibration.epsilon_pt = (calibration.epsilon_pt * 1.75_f64).max(4.0_f64);
            return Some(AnchorPairData {
                left_anchor_text,
                right_anchor_text,
                left_x,
                right_x,
                font_key: left_run.font_key.clone(),
                font_name: left_run.font_name.clone(),
                font_size_pt: left_run.font_size_pt,
                h_scale_pct: left_run.h_scale_pct,
                left_bbox: left_run.bbox,
                right_bbox: left_run.bbox,
                epsilon_pt: calibration.epsilon_pt,
                row_bias_pt: calibration.bias_pt,
                mode: AnchorMode::LeftOnly,
            });
        }
    }

    if let Some(right_run) = right_only {
        let asset = assets.get(&right_run.font_key);
        let (right_anchor_text, right_x) = resolve_anchor_text_and_x(
            redaction.page_index,
            right_run,
            hints.right_text,
            hints.right_x,
            asset,
            typography_profile,
        );
        let left_anchor_text = hints.left_text.unwrap_or_default().to_owned();
        let left_x = (redaction.bbox.x0 as f64 - 0.5_f64).min(right_x - 1.0_f64);
        if !right_anchor_text.trim().is_empty() && right_x > left_x {
            let measure_ctx = WidthMeasureContext {
                page_index: redaction.page_index,
                asset,
                typography_profile,
                h_scale_pct: right_run.h_scale_pct,
            };
            let mut calibration =
                estimate_row_epsilon(row_runs, right_run, redaction, &measure_ctx);
            calibration.epsilon_pt = (calibration.epsilon_pt * 1.75_f64).max(4.0_f64);
            return Some(AnchorPairData {
                left_anchor_text,
                right_anchor_text,
                left_x,
                right_x,
                font_key: right_run.font_key.clone(),
                font_name: right_run.font_name.clone(),
                font_size_pt: right_run.font_size_pt,
                h_scale_pct: right_run.h_scale_pct,
                left_bbox: right_run.bbox,
                right_bbox: right_run.bbox,
                epsilon_pt: calibration.epsilon_pt,
                row_bias_pt: calibration.bias_pt,
                mode: AnchorMode::RightOnly,
            });
        }
    }

    None
}

fn resolve_anchor_text_and_x(
    page_index: u32,
    run: &FontTextRun,
    hint: Option<&str>,
    hint_x: Option<f64>,
    asset: Option<&FontAsset>,
    typography_profile: &TypographyProfile,
) -> (String, f64) {
    let run_text = run.text.trim();
    if run_text.is_empty() {
        return (String::new(), run.bbox.x0 as f64);
    }
    let Some(hint_text) = hint else {
        return (run_text.to_owned(), run.bbox.x0 as f64);
    };
    if hint_text.is_empty() {
        return (run_text.to_owned(), run.bbox.x0 as f64);
    }
    if run_text == hint_text {
        return (hint_text.to_owned(), run.bbox.x0 as f64);
    }
    if let Some(prefix_bytes) = run_text.find(hint_text) {
        let prefix = &run_text[..prefix_bytes];
        let offset = prefix_width_from_run(run, prefix_bytes)
            .or_else(|| {
                measure_text_width_from_sources(
                    &TypographyMeasureInput {
                        page_index,
                        font_key: &run.font_key,
                        font_name: &run.font_name,
                        font_size_pt: run.font_size_pt,
                        h_scale_pct: run.h_scale_pct,
                        text: prefix,
                        metrics_dpi: DEFAULT_FONT_METRICS_DPI,
                    },
                    asset,
                    typography_profile,
                )
                .map(|value| value.pt)
            })
            .unwrap_or_else(|| {
                let run_chars = run_text.chars().count().max(1) as f64;
                let prefix_chars = prefix.chars().count() as f64;
                ((run.bbox.x1 - run.bbox.x0).abs() as f64) * (prefix_chars / run_chars)
            });
        return (hint_text.to_owned(), run.bbox.x0 as f64 + offset);
    }
    if hint_text.contains(run_text) {
        return (run_text.to_owned(), run.bbox.x0 as f64);
    }
    (hint_text.to_owned(), hint_x.unwrap_or(run.bbox.x0 as f64))
}

fn prefix_width_from_run(run: &FontTextRun, prefix_bytes: usize) -> Option<f64> {
    if run.char_advances_pt.is_empty() {
        return None;
    }
    if prefix_bytes == 0 {
        return Some(0.0_f64);
    }
    let prefix_char_count = run.text.get(..prefix_bytes)?.chars().count();
    if prefix_char_count == 0 {
        return Some(0.0_f64);
    }
    if prefix_char_count > run.char_advances_pt.len() {
        return None;
    }
    let width_pt = run
        .char_advances_pt
        .iter()
        .take(prefix_char_count)
        .map(|value| *value as f64)
        .sum::<f64>();
    (width_pt.is_finite() && width_pt >= 0.0_f64).then_some(width_pt)
}

fn estimate_row_epsilon(
    runs: &[&FontTextRun],
    left_run: &FontTextRun,
    redaction: &RedactionOccurrence,
    measure_ctx: &WidthMeasureContext<'_>,
) -> RowCalibration {
    let fallback_char_width = estimate_char_width_pt(
        left_run.text.trim(),
        "",
        Some(Rect::new(
            left_run.bbox.x0,
            left_run.bbox.y0,
            left_run.bbox.x1,
            left_run.bbox.y1,
        )),
        None,
        redaction.bbox,
    )
    .max(0.1_f64);
    let space_width = measure_text_width_from_sources(
        &TypographyMeasureInput {
            page_index: measure_ctx.page_index,
            font_key: &left_run.font_key,
            font_name: &left_run.font_name,
            font_size_pt: left_run.font_size_pt,
            h_scale_pct: measure_ctx.h_scale_pct,
            text: " ",
            metrics_dpi: DEFAULT_FONT_METRICS_DPI,
        },
        measure_ctx.asset,
        measure_ctx.typography_profile,
    )
    .map(|value| value.pt)
    .unwrap_or(0.5_f64 * fallback_char_width);
    let mut row = runs
        .iter()
        .copied()
        .filter(|run| {
            run.font_key == left_run.font_key
                && (run.font_size_pt - left_run.font_size_pt).abs() <= 0.25
                && (run_center_y(run) - run_center_y(left_run)).abs() <= 2.0
        })
        .collect::<Vec<_>>();
    row.sort_by(|left_entry, right_entry| {
        left_entry
            .bbox
            .x0
            .partial_cmp(&right_entry.bbox.x0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left_entry.text.cmp(&right_entry.text))
    });

    let mut residuals = Vec::new();
    for window in row.windows(2) {
        let current = window[0];
        let next = window[1];
        let current_text = current.text.trim();
        if current_text.is_empty() {
            continue;
        }
        let current_width = measure_text_width_from_sources(
            &TypographyMeasureInput {
                page_index: measure_ctx.page_index,
                font_key: &current.font_key,
                font_name: &current.font_name,
                font_size_pt: current.font_size_pt,
                h_scale_pct: current.h_scale_pct,
                text: current_text,
                metrics_dpi: DEFAULT_FONT_METRICS_DPI,
            },
            measure_ctx.asset,
            measure_ctx.typography_profile,
        )
        .map(|value| value.pt)
        .unwrap_or_else(|| {
            let chars = current_text.chars().count().max(1) as f64;
            chars * fallback_char_width
        });
        let current_space_width = measure_text_width_from_sources(
            &TypographyMeasureInput {
                page_index: measure_ctx.page_index,
                font_key: &current.font_key,
                font_name: &current.font_name,
                font_size_pt: current.font_size_pt,
                h_scale_pct: current.h_scale_pct,
                text: " ",
                metrics_dpi: DEFAULT_FONT_METRICS_DPI,
            },
            measure_ctx.asset,
            measure_ctx.typography_profile,
        )
        .map(|value| value.pt)
        .unwrap_or(space_width);
        let predicted_next = current.bbox.x0 as f64 + current_width + current_space_width;
        let residual = next.bbox.x0 as f64 - predicted_next;
        if residual.is_finite() {
            residuals.push(residual);
        }
    }
    residuals.sort_by(|left_value, right_value| {
        left_value
            .partial_cmp(right_value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if residuals.is_empty() {
        return RowCalibration {
            epsilon_pt: 2.0_f64,
            bias_pt: 0.0_f64,
        };
    }

    let median_index = ((residuals.len() as f64) * 0.5_f64).floor() as usize;
    let bias = residuals[median_index.min(residuals.len().saturating_sub(1))];
    let mut centered = residuals
        .iter()
        .map(|value| (value - bias).abs())
        .collect::<Vec<_>>();
    centered.sort_by(|left_value, right_value| {
        left_value
            .partial_cmp(right_value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let epsilon = if centered.is_empty() {
        2.0_f64
    } else {
        let idx = ((centered.len() as f64) * 0.75_f64).floor() as usize;
        centered[idx.min(centered.len().saturating_sub(1))]
    };
    RowCalibration {
        epsilon_pt: epsilon.clamp(3.5_f64, 8.0_f64),
        bias_pt: bias,
    }
}

fn measure_text_width_from_sources(
    input: &TypographyMeasureInput<'_>,
    asset: Option<&FontAsset>,
    typography_profile: &TypographyProfile,
) -> Option<MeasuredWidth> {
    measure_text_width_from_profile(input, asset, typography_profile)
}

fn run_center_y(run: &FontTextRun) -> f32 {
    (run.bbox.y0 + run.bbox.y1) * 0.5
}

fn rect_center_y(rect: &Rect) -> f32 {
    (rect.y0 + rect.y1) * 0.5
}

fn extract_context(
    redaction: &RedactionOccurrence,
) -> (String, String, Option<Rect>, Option<Rect>) {
    let left = redaction.underlying_text.first();
    let right = redaction.underlying_text.get(1);
    let left_text = left.map(|h| h.text.clone()).unwrap_or_default();
    let right_text = right.map(|h| h.text.clone()).unwrap_or_default();
    let left_bbox = left.map(|h| h.bbox);
    let right_bbox = right.map(|h| h.bbox);
    (left_text, right_text, left_bbox, right_bbox)
}

fn compute_gap_pt(
    red_bbox: Rect,
    left_bbox: Option<Rect>,
    right_bbox: Option<Rect>,
    left_anchor_text: &str,
    right_anchor_text: &str,
) -> f64 {
    if !left_anchor_text.trim().is_empty() && !right_anchor_text.trim().is_empty() {
        if let (Some(l), Some(r)) = (left_bbox, right_bbox) {
            return (r.x0 - l.x1).max(0.0) as f64;
        }
    }
    let w = red_bbox.width().abs();
    if w > 0.0 {
        return w as f64;
    }
    0.0
}

fn estimate_char_width_pt(
    left_anchor_text: &str,
    right_anchor_text: &str,
    left_bbox: Option<Rect>,
    right_bbox: Option<Rect>,
    red_bbox: Rect,
) -> f64 {
    let mut samples = Vec::new();
    if let (Some(b), count) = (left_bbox, left_anchor_text.chars().count()) {
        if count > 0 {
            let w = b.width().abs() as f64;
            if w > 0.0_f64 {
                samples.push(w / count as f64);
            }
        }
    }
    if let (Some(b), count) = (right_bbox, right_anchor_text.chars().count()) {
        if count > 0 {
            let w = b.width().abs() as f64;
            if w > 0.0_f64 {
                samples.push(w / count as f64);
            }
        }
    }
    if !samples.is_empty() {
        let sum = samples.iter().sum::<f64>();
        return sum / samples.len() as f64;
    }
    let fallback = red_bbox.height().abs() as f64 * 0.5_f64;
    if fallback > 0.0 {
        fallback
    } else {
        6.0
    }
}

#[derive(Debug, Clone)]
struct CandidateWidthEntry {
    text: String,
    width_pt: f64,
    source: WidthSource,
}

fn build_row_candidate_width_index(
    dictionary_variants: &[String],
    min_char_units: f64,
    max_char_units: f64,
    measure_width: &dyn Fn(&str) -> Option<MeasuredWidth>,
) -> Vec<CandidateWidthEntry> {
    let mut out = Vec::<CandidateWidthEntry>::new();
    for variant in dictionary_variants {
        let trimmed = variant.trim();
        if trimmed.is_empty() {
            continue;
        }
        let char_units = candidate_char_units(trimmed);
        if char_units < min_char_units || char_units > max_char_units {
            continue;
        }
        let measured = measure_width(trimmed).unwrap_or_else(|| {
            fallback_typography_width(trimmed, 0.0_f64, 0.0_f64, DEFAULT_FONT_METRICS_DPI)
        });
        out.push(CandidateWidthEntry {
            text: trimmed.to_owned(),
            width_pt: measured.pt,
            source: measured.source,
        });
    }
    out.sort_by(|left, right| {
        left.width_pt
            .partial_cmp(&right.width_pt)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.text.cmp(&right.text))
    });
    out
}

fn vertical_overlap_run(a: &FontRect, b: &Rect) -> f32 {
    (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0)
}

fn collect_row_runs_for_anchor<'a>(
    redaction: &RedactionOccurrence,
    runs: &[&'a FontTextRun],
    tight: bool,
) -> Vec<&'a FontTextRun> {
    let red_rect = Rect::new(
        redaction.bbox.x0,
        redaction.bbox.y0,
        redaction.bbox.x1,
        redaction.bbox.y1,
    );
    let red_center_y = rect_center_y(&redaction.bbox);
    let red_center_x = ((redaction.bbox.x0 + redaction.bbox.x1) * 0.5) as f64;
    let y_tolerance = if tight { 12.0_f32 } else { 20.0_f32 };
    let baseline_tolerance = if tight { 8.0_f32 } else { 16.0_f32 };
    let x_tolerance = if tight { 120.0_f64 } else { 220.0_f64 };

    runs.iter()
        .copied()
        .filter(|run| {
            let overlap = vertical_overlap_run(&run.bbox, &red_rect);
            let center_distance = (run_center_y(run) - red_center_y).abs();
            let baseline_distance = (run.bbox.y1 - redaction.bbox.y1).abs();
            let left_gap = (redaction.bbox.x0 as f64 - run.bbox.x1 as f64).abs();
            let right_gap = (run.bbox.x0 as f64 - redaction.bbox.x1 as f64).abs();
            let contains_center =
                run.bbox.x0 as f64 <= red_center_x && run.bbox.x1 as f64 >= red_center_x;
            let near_x = left_gap <= x_tolerance || right_gap <= x_tolerance || contains_center;
            let near_y = overlap > 0.0
                || center_distance <= y_tolerance
                || baseline_distance <= baseline_tolerance;
            if tight {
                near_x && near_y
            } else {
                near_y
            }
        })
        .collect::<Vec<_>>()
}

fn text_matches(run_text: &str, target: &str) -> bool {
    if run_text == target {
        return true;
    }
    run_text.contains(target) || target.contains(run_text)
}

fn candidate_width_entries_in_range(
    entries: &[CandidateWidthEntry],
    min_width_pt: f64,
    max_width_pt: f64,
) -> &[CandidateWidthEntry] {
    if entries.is_empty() || !min_width_pt.is_finite() || !max_width_pt.is_finite() {
        return &[];
    }
    if min_width_pt > max_width_pt {
        return &[];
    }
    let start = entries.partition_point(|entry| entry.width_pt < min_width_pt);
    let end = entries.partition_point(|entry| entry.width_pt <= max_width_pt);
    if start >= end || start >= entries.len() {
        return &[];
    }
    &entries[start..end.min(entries.len())]
}

fn trim_width_band_around_target(
    entries: &[CandidateWidthEntry],
    target_width_pt: f64,
    limit: usize,
) -> &[CandidateWidthEntry] {
    if entries.len() <= limit || limit == 0 {
        return entries;
    }
    let mid = entries.partition_point(|entry| entry.width_pt < target_width_pt);
    let half = limit >> 1_usize;
    let mut start = mid.saturating_sub(half);
    let mut end = start.saturating_add(limit).min(entries.len());
    if end - start < limit {
        start = end.saturating_sub(limit);
    }
    end = end.max(start);
    &entries[start..end]
}

fn passes_context_filter(left_anchor_text: &str, right_anchor_text: &str, candidate: &str) -> bool {
    let left_lower = left_anchor_text.to_ascii_lowercase();
    let right_lower = right_anchor_text.to_ascii_lowercase();
    if right_lower.starts_with("and") && candidate.contains(',') {
        return false;
    }
    if left_lower.contains("including") && right_lower.starts_with("and") {
        let count = candidate.split_whitespace().count();
        if count > 3 {
            return false;
        }
    }
    true
}

fn is_list_like_context(left_anchor_text: &str, right_anchor_text: &str) -> bool {
    let left_lower = left_anchor_text.trim().to_ascii_lowercase();
    let right_lower = right_anchor_text.trim().to_ascii_lowercase();
    left_lower.contains("including")
        || left_lower.contains("included")
        || left_lower.contains("among")
        || left_lower.contains("served")
        || left_lower.ends_with(',')
        || right_lower.starts_with(',')
        || right_lower.ends_with(',')
        || right_lower.starts_with("and ")
}

fn looks_like_alpha_phrase_candidate(candidate: &str) -> bool {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains(',')
        || trimmed.contains('(')
        || trimmed.contains(')')
        || trimmed.contains('/')
        || trimmed.contains('&')
    {
        return false;
    }
    if trimmed.chars().any(|ch| ch.is_ascii_digit()) {
        return false;
    }
    let words = trimmed.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() || words.len() > 4 {
        return false;
    }
    words.iter().all(|word| {
        !word.is_empty()
            && word
                .chars()
                .all(|ch| ch.is_ascii_alphabetic() || ch == '-' || ch == '\'')
    })
}

fn punctuation_context_penalty(
    left_anchor_text: &str,
    right_anchor_text: &str,
    candidate: &str,
) -> f64 {
    let left_lower = left_anchor_text.trim().to_ascii_lowercase();
    let right_lower = right_anchor_text.trim().to_ascii_lowercase();
    let candidate_trim = candidate.trim();
    if candidate_trim.is_empty() {
        return 5.0;
    }

    let word_count = candidate_trim.split_whitespace().count();
    let mut penalty = 0.0_f64;
    let list_context = is_list_like_context(&left_lower, &right_lower);

    if list_context {
        if word_count == 1 || word_count >= 5 {
            penalty += 0.20_f64;
        }
        if candidate_trim.contains('-') {
            penalty += 0.30_f64;
        }
        if candidate_trim.contains(',') {
            penalty += 0.45_f64;
        }
        if candidate_trim.contains('(') || candidate_trim.contains(')') {
            penalty += 0.40_f64;
        }
        if candidate_trim.contains('/') || candidate_trim.contains('&') {
            penalty += 0.50_f64;
        }
    }

    if (right_lower.starts_with(',') || right_lower.starts_with("and "))
        && (candidate_trim.ends_with(',') || candidate_trim.ends_with(';'))
    {
        penalty += 0.25_f64;
    }

    if candidate_trim.chars().any(|ch| ch.is_ascii_digit()) {
        penalty += 0.20_f64;
    }

    penalty.max(0.0)
}

use super::common::{
    anchor_overlap_penalty_pt, candidate_char_units, is_list_like_context,
    is_two_sided_anchor_context, punctuation_context_penalty,
};
use super::joint_assignment::{apply_row_joint_assignment, apply_row_sequence_consensus};
use super::visual_logic::{apply_visual_scores_from_bytes, VisualGuessScoreConfig};
use crate::data::dictionary_variant_data::build_dictionary_variants;
use crate::data::fonts_data::FontsData;
use crate::data::redaction_scan_data::CONTEXT_SPANS_META_KEY;
use crate::data::typography_width_data::{
    build_typography_profile_from_pdf_bytes, fallback_typography_width,
    measure_text_width_from_profile,
};
use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect as FontRect};
use crate::types::guess_types::{
    GuessCandidate, GuessConfig, GuessContext, GuessReport, RedactionGuess,
};
use crate::types::redaction_types::{Rect, RedactionKind, RedactionOccurrence, RedactionReport};
use crate::types::runtime_defaults::{DEFAULT_FONT_METRICS_DPI, MULTI_SPAN_GAP_RATIO_THRESHOLD};
use crate::types::time::Instant;
use crate::types::typography_types::{
    TypographyMeasureInput, TypographyMeasuredWidth, TypographyProfile, TypographyWidthSource,
};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::sync::OnceLock;

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
    let use_curated_name_prior = inputs
        .diagnostics
        .iter()
        .any(|line| line == "dictionary_source=default_names");
    let guess_anchor_started = Instant::now();
    let (mut guesses, guess_diagnostics) = build_anchor_validated_guesses(
        &inputs.redactions.redactions,
        &inputs.dictionary,
        inputs.font_runs,
        &inputs.typography_profile,
        cfg,
        use_curated_name_prior,
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
        let base = guess
            .candidates
            .first()
            .map(|candidate| candidate.score as f64)
            .unwrap_or(0.0_f64);
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
struct PairCandidate {
    left_idx: usize,
    right_idx: usize,
    font_penalty: u8,
    hint_penalty: u8,
    straddle_penalty: u8,
    overlap_penalty: u8,
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
const RASTER_WIDTH_NOISE_PT: f64 = 2.50_f64;
const CLUSTER_CONSENSUS_MAX_GAP_RATIO: f64 = 1.9_f64;
const SINGLE_SPAN_WIDTH_BAND_LIMIT: usize = 700;
const CURATED_NAME_PRIOR_BONUS_PT: f64 = 1.10_f64;
const LIST_CONTEXT_MAX_CANDIDATES: usize = 200;
const TWO_SIDED_MAX_CANDIDATES: usize = 600;
const EXACT_MATCH_RAW_ERROR_EPSILON_PT: f64 = 0.0001_f64;
const ANCHOR_PAIR_STRADDLE_TOLERANCE_PT: f64 = 2.0_f64;
const ANCHOR_PAIR_MAX_OVERLAP_PT: f64 = 1.0_f64;
const ONE_SIDED_ANCHOR_EDGE_TOLERANCE_PT: f32 = 4.0_f32;
const ONE_SIDED_LIST_CONTEXT_SIDE_ALIGNMENT_MIN_PT: f64 = 24.0_f64;
const CLUSTER_ANCHOR_HINT_EDGE_TOLERANCE_PT: f32 = 8.0_f32;
const CLUSTER_ANCHOR_HINT_MAX_GAP_PT: f32 = 240.0_f32;
const ROW_CONTEXT_MAX_GROUP_GAP_PT: f64 = 96.0_f64;
const ROW_CONTEXT_WRAP_LINE_MAX_Y_GAP_PT: f64 = 18.0_f64;
const ROW_CONTEXT_SAME_LINE_MAX_Y_DELTA_PT: f64 = 4.0_f64;
const ROW_CONTEXT_WRAP_RESET_X_DELTA_PT: f64 = 16.0_f64;
const ROW_CONTEXT_WRAP_FORWARD_X_ALLOWANCE_PT: f64 = 28.0_f64;
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

#[derive(Debug, Clone, Deserialize)]
struct ContextSpanRecord {
    text: String,
    bbox: Rect,
    line_bucket: i32,
    role_hint: String,
    #[serde(default)]
    source: String,
}

#[derive(Debug, Clone)]
struct ClusterAnchorHint {
    text: String,
    bbox: Rect,
    line_bucket: i32,
    role_hint: String,
    source: String,
}

#[derive(Debug, Clone)]
struct InferredTypography {
    font_key: String,
    font_name: String,
    font_size_pt: f32,
    h_scale_pct: f32,
}

#[derive(Debug, Clone)]
struct SharedClusterTypography {
    font_key: String,
    font_name: Option<String>,
    font_size_pt: f32,
    h_scale_pct: f32,
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
    use_curated_name_prior: bool,
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
    let shared_context_hints = build_shared_context_hints(redactions);

    for (index, redaction) in redactions.iter().enumerate() {
        let (left_text, right_text, left_bbox, right_bbox) = extract_context(redaction);
        let page_runs = by_page
            .get(&redaction.page_index)
            .cloned()
            .unwrap_or_default();
        let cluster_hints = shared_context_hints
            .get(index)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let anchor = select_anchor_pair(
            redaction,
            &page_runs,
            cluster_hints,
            &assets,
            typography_profile,
        );
        let Some(anchor) = anchor else {
            let inferred_typography = infer_local_typography(redaction, &page_runs);
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
                    anchor_font_key: inferred_typography
                        .as_ref()
                        .map(|value| value.font_key.clone()),
                    anchor_font_name: inferred_typography
                        .as_ref()
                        .map(|value| value.font_name.clone()),
                    anchor_font_size_pt: inferred_typography
                        .as_ref()
                        .map(|value| value.font_size_pt),
                    anchor_h_scale_pct: inferred_typography.as_ref().map(|value| value.h_scale_pct),
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
            use_curated_name_prior,
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

    propagate_row_context_to_guesses(&mut guesses);
    apply_cluster_consensus(&mut guesses, use_curated_name_prior);
    let jointly_assigned = apply_row_joint_assignment(&mut guesses);
    apply_row_sequence_consensus(&mut guesses, &jointly_assigned);
    for (index, guess) in guesses.iter().enumerate() {
        if !guess.context.has_anchor_pair {
            continue;
        }
        let top = guess
            .candidates
            .first()
            .map(|candidate| candidate.text.clone())
            .unwrap_or_default();
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

fn infer_local_typography(
    redaction: &RedactionOccurrence,
    page_runs: &[&FontTextRun],
) -> Option<InferredTypography> {
    let red_center_y = (redaction.bbox.y0 + redaction.bbox.y1) * 0.5_f32;
    let red_left_x = redaction.bbox.x0 as f64;
    let red_right_x = redaction.bbox.x1 as f64;
    let mut best: Option<(&FontTextRun, f64)> = None;
    for run in page_runs {
        if run.font_key.trim().is_empty() {
            continue;
        }
        let run_center_y = (run.bbox.y0 + run.bbox.y1) * 0.5_f32;
        let y_distance = (run_center_y - red_center_y).abs() as f64;
        if y_distance > 40.0_f64 {
            continue;
        }
        let run_left_x = run.bbox.x0 as f64;
        let run_right_x = run.bbox.x1 as f64;
        let x_gap = if run_right_x <= red_left_x {
            red_left_x - run_right_x
        } else if run_left_x >= red_right_x {
            run_left_x - red_right_x
        } else {
            0.0_f64
        };
        let vertical_overlap = vertical_overlap_run(&run.bbox, &redaction.bbox) as f64;
        let overlap_bonus = if vertical_overlap > 0.0_f64 {
            3.0_f64
        } else {
            0.0_f64
        };
        let score = y_distance * 2.0_f64 + x_gap - overlap_bonus;
        match best {
            None => best = Some((run, score)),
            Some((_, best_score)) if score < best_score => best = Some((run, score)),
            _ => {}
        }
    }
    best.map(|(run, _)| InferredTypography {
        font_key: run.font_key.clone(),
        font_name: run.font_name.clone(),
        font_size_pt: run.font_size_pt,
        h_scale_pct: run.h_scale_pct,
    })
}

fn propagate_row_context_to_guesses(guesses: &mut [RedactionGuess]) {
    let mut by_page = std::collections::BTreeMap::<u32, Vec<usize>>::new();
    for (index, guess) in guesses.iter().enumerate() {
        by_page.entry(guess.page_index).or_default().push(index);
    }

    for page_indices in by_page.values_mut() {
        page_indices.sort_by(|left_index, right_index| {
            let left = &guesses[*left_index];
            let right = &guesses[*right_index];
            let left_center_y = ((left.bbox.y0 + left.bbox.y1) * 0.5_f32) as f64;
            let right_center_y = ((right.bbox.y0 + right.bbox.y1) * 0.5_f32) as f64;
            left_center_y
                .partial_cmp(&right_center_y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.bbox
                        .x0
                        .partial_cmp(&right.bbox.x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let mut visited = std::collections::BTreeSet::<usize>::new();
        for seed_index in page_indices.iter().copied() {
            if visited.contains(&seed_index) {
                continue;
            }
            let mut cluster = Vec::<usize>::new();
            let mut queue = vec![seed_index];
            visited.insert(seed_index);
            while let Some(current_index) = queue.pop() {
                cluster.push(current_index);
                for other_index in page_indices.iter().copied() {
                    if visited.contains(&other_index) {
                        continue;
                    }
                    if row_context_rows_are_linked(&guesses[current_index], &guesses[other_index]) {
                        visited.insert(other_index);
                        queue.push(other_index);
                    }
                }
            }
            cluster.sort_by(|left_index, right_index| {
                let left = &guesses[*left_index];
                let right = &guesses[*right_index];
                let left_center_y = ((left.bbox.y0 + left.bbox.y1) * 0.5_f32) as f64;
                let right_center_y = ((right.bbox.y0 + right.bbox.y1) * 0.5_f32) as f64;
                left_center_y
                    .partial_cmp(&right_center_y)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        left.bbox
                            .x0
                            .partial_cmp(&right.bbox.x0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });
            apply_row_context_cluster(guesses, &cluster);
        }
    }
}

fn row_context_rows_are_contiguous(left: &RedactionGuess, right: &RedactionGuess) -> bool {
    let left_center_y = ((left.bbox.y0 + left.bbox.y1) * 0.5_f32) as f64;
    let right_center_y = ((right.bbox.y0 + right.bbox.y1) * 0.5_f32) as f64;
    let y_delta = (right_center_y - left_center_y).abs();
    if y_delta <= ROW_CONTEXT_SAME_LINE_MAX_Y_DELTA_PT {
        let x_gap = (right.bbox.x0 as f64 - left.bbox.x1 as f64).max(0.0_f64);
        return x_gap <= ROW_CONTEXT_MAX_GROUP_GAP_PT;
    }
    if right_center_y <= left_center_y || y_delta > ROW_CONTEXT_WRAP_LINE_MAX_Y_GAP_PT {
        return false;
    }
    let wraps_to_new_line =
        right.bbox.x0 as f64 + ROW_CONTEXT_WRAP_RESET_X_DELTA_PT < left.bbox.x0 as f64;
    let continues_forward =
        right.bbox.x0 as f64 <= left.bbox.x1 as f64 + ROW_CONTEXT_WRAP_FORWARD_X_ALLOWANCE_PT;
    wraps_to_new_line || continues_forward
}

fn row_context_rows_are_linked(left: &RedactionGuess, right: &RedactionGuess) -> bool {
    row_context_rows_are_contiguous(left, right) || row_context_rows_are_contiguous(right, left)
}

fn apply_row_context_cluster(guesses: &mut [RedactionGuess], cluster: &[usize]) {
    if cluster.len() < 2 {
        return;
    }
    let donor_index = cluster
        .iter()
        .copied()
        .filter(|index| {
            let context = &guesses[*index].context;
            context.anchor_font_key.is_some() && context.anchor_font_size_pt.is_some()
        })
        .max_by(|left_index, right_index| {
            let left = &guesses[*left_index];
            let right = &guesses[*right_index];
            let left_anchor_rank = if left.context.has_anchor_pair {
                2_i32
            } else {
                1_i32
            };
            let right_anchor_rank = if right.context.has_anchor_pair {
                2_i32
            } else {
                1_i32
            };
            left_anchor_rank
                .cmp(&right_anchor_rank)
                .then_with(|| left.candidates.len().cmp(&right.candidates.len()))
                .then_with(|| {
                    let left_score = left
                        .candidates
                        .first()
                        .map(|item| item.score)
                        .unwrap_or(0.0_f32);
                    let right_score = right
                        .candidates
                        .first()
                        .map(|item| item.score)
                        .unwrap_or(0.0_f32);
                    left_score
                        .partial_cmp(&right_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    let Some(donor_index) = donor_index else {
        return;
    };
    let donor = guesses[donor_index].context.clone();
    let shared_typography = derive_shared_cluster_typography(guesses, cluster);

    for guess_index in cluster.iter().copied() {
        let context = &mut guesses[guess_index].context;
        if let Some(shared) = &shared_typography {
            context.anchor_font_key = Some(shared.font_key.clone());
            if let Some(font_name) = &shared.font_name {
                context.anchor_font_name = Some(font_name.clone());
            }
            context.anchor_font_size_pt = Some(shared.font_size_pt);
            context.anchor_h_scale_pct = Some(shared.h_scale_pct);
        } else if !context.has_anchor_pair {
            context.anchor_font_key = donor.anchor_font_key.clone();
            context.anchor_font_name = donor.anchor_font_name.clone();
            context.anchor_font_size_pt = donor.anchor_font_size_pt;
            context.anchor_h_scale_pct = donor.anchor_h_scale_pct;
        } else {
            if context.anchor_font_key.is_none() {
                context.anchor_font_key = donor.anchor_font_key.clone();
            }
            if context.anchor_font_name.is_none() {
                context.anchor_font_name = donor.anchor_font_name.clone();
            }
            if context.anchor_font_size_pt.is_none() {
                context.anchor_font_size_pt = donor.anchor_font_size_pt;
            }
            if context.anchor_h_scale_pct.is_none() {
                context.anchor_h_scale_pct = donor.anchor_h_scale_pct;
            }
        }
        if guess_index == donor_index {
            continue;
        }
        if context.anchor_row_bias_pt.is_none() {
            context.anchor_row_bias_pt = donor.anchor_row_bias_pt;
        }
        if context.char_width_pt <= 0.0_f32 && donor.char_width_pt > 0.0_f32 {
            context.char_width_pt = donor.char_width_pt;
        }
        if context.tol_pt <= 0.0_f32 && donor.tol_pt > 0.0_f32 {
            context.tol_pt = donor.tol_pt;
        }
    }
}

fn derive_shared_cluster_typography(
    guesses: &[RedactionGuess],
    cluster: &[usize],
) -> Option<SharedClusterTypography> {
    if cluster.is_empty() {
        return None;
    }
    let mut samples = Vec::<(String, Option<String>, f32, f32)>::new();
    for guess_index in cluster.iter().copied() {
        let context = &guesses[guess_index].context;
        let Some(font_key) = context.anchor_font_key.as_ref() else {
            continue;
        };
        let Some(font_size_pt) = context.anchor_font_size_pt else {
            continue;
        };
        let Some(h_scale_pct) = context.anchor_h_scale_pct else {
            continue;
        };
        if !font_size_pt.is_finite()
            || font_size_pt <= 0.0_f32
            || !h_scale_pct.is_finite()
            || h_scale_pct <= 0.0_f32
        {
            continue;
        }
        let font_name = context
            .anchor_font_name
            .as_ref()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        samples.push((font_key.clone(), font_name, font_size_pt, h_scale_pct));
    }
    if samples.is_empty() {
        return None;
    }
    let mut font_key_counts = std::collections::BTreeMap::<String, usize>::new();
    for (font_key, _, _, _) in &samples {
        *font_key_counts.entry(font_key.clone()).or_insert(0_usize) += 1;
    }
    let target_font_key = font_key_counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
        .map(|(font_key, _)| font_key)?;
    let mut filtered = samples
        .into_iter()
        .filter(|(font_key, _, _, _)| *font_key == target_font_key)
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        return None;
    }
    let font_name = filtered
        .iter()
        .find_map(|(_, name, _, _)| name.as_ref().cloned());
    let mut font_sizes = filtered
        .iter_mut()
        .map(|(_, _, font_size_pt, _)| *font_size_pt)
        .collect::<Vec<_>>();
    let mut h_scales = filtered
        .iter_mut()
        .map(|(_, _, _, h_scale_pct)| *h_scale_pct)
        .collect::<Vec<_>>();
    let font_size_pt = median_f32(&mut font_sizes);
    let h_scale_pct = median_f32(&mut h_scales);
    Some(SharedClusterTypography {
        font_key: target_font_key,
        font_name,
        font_size_pt,
        h_scale_pct,
    })
}

fn median_f32(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0_f32;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() >> 1_usize;
    if (values.len() & 1_usize) == 0_usize {
        (values[mid - 1] + values[mid]) * 0.5_f32
    } else {
        values[mid]
    }
}

fn apply_cluster_consensus(guesses: &mut [RedactionGuess], use_effective_candidate_cost: bool) {
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
                let candidate_cost = if use_effective_candidate_cost {
                    (1.0_f64 - candidate.score as f64).clamp(0.0_f64, 1.0_f64)
                } else {
                    (candidate.error_pt as f64) / denom
                };
                entry.0 += candidate_cost;
                entry.1 += 1;
            }
        }

        let cluster_size = indices.len() as u32;
        for index in indices {
            let guess = &mut guesses[*index];
            let mut local_error = std::collections::BTreeMap::<String, f64>::new();
            let denom = (guess.context.tol_pt as f64).max(0.0001_f64);
            for candidate in &guess.candidates {
                let candidate_cost = if use_effective_candidate_cost {
                    (1.0_f64 - candidate.score as f64).clamp(0.0_f64, 1.0_f64)
                } else {
                    (candidate.error_pt as f64) / denom
                };
                local_error.insert(candidate.text.clone(), candidate_cost);
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
    use_curated_name_prior: bool,
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
    let has_two_sided_anchor = matches!(anchor.mode, AnchorMode::TwoSided);
    let anchor_target_guess_width_pt = if has_two_sided_anchor {
        (anchor.right_x
            - anchor.left_x
            - left_width.pt
            - (space_width.pt * 2.0_f64)
            - anchor.row_bias_pt)
            .max(0.1_f64)
    } else {
        redaction_width_pt
    };
    let target_width_pt_for_units = if has_two_sided_anchor {
        anchor_target_guess_width_pt.max(fallback_char_width.max(0.1_f64))
    } else {
        redaction_width_pt
    };
    let (min_char_units, max_char_units) = if has_two_sided_anchor {
        (1.0_f64, 40.0_f64)
    } else {
        char_unit_band(
            target_width_pt_for_units,
            fallback_char_width.max(0.1_f64),
            anchor.epsilon_pt.max(FIXED_TOLERANCE_PT),
        )
    };
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
    let list_like_context =
        is_list_like_context(&anchor.left_anchor_text, &anchor.right_anchor_text);
    let raw_left_context = redaction
        .underlying_text
        .first()
        .map(|hit| hit.text.as_str())
        .unwrap_or("");
    let raw_right_context = redaction
        .underlying_text
        .get(1)
        .map(|hit| hit.text.as_str())
        .unwrap_or("");
    let raw_list_like_context = is_list_like_context(raw_left_context, raw_right_context);
    if has_two_sided_anchor {
        let band = candidate_width_index.as_slice();
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

            let predicted_right_with_bias = anchor.left_x
                + left_width.pt
                + space_width.pt
                + entry.width_pt
                + space_width.pt
                + anchor.row_bias_pt;
            let predicted_right_without_bias =
                anchor.left_x + left_width.pt + space_width.pt + entry.width_pt + space_width.pt;
            let anchor_err = (predicted_right_with_bias - anchor.right_x)
                .abs()
                .min((predicted_right_without_bias - anchor.right_x).abs());
            if anchor_err > anchor_filter_limit_pt {
                continue;
            }
            funnel.after_anchor += 1;
            funnel.after_box += 1;
            let raw_err = anchor_err;
            let context_penalty = punctuation_context_penalty(
                &anchor.left_anchor_text,
                &anchor.right_anchor_text,
                trimmed,
            );
            let candidate_word_count = trimmed.split_whitespace().count();
            let raw_context_overlap_penalty = if use_curated_name_prior && raw_list_like_context {
                anchor_overlap_penalty_pt(raw_left_context, raw_right_context, trimmed)
            } else {
                0.0_f64
            };
            let list_shape_penalty =
                if use_curated_name_prior && raw_list_like_context && candidate_word_count == 3 {
                    0.18_f64
                } else {
                    0.0_f64
                };
            let two_sided_shape_penalty = if has_two_sided_anchor {
                if candidate_word_count <= 1 {
                    2.5_f64
                } else if candidate_word_count >= 4 {
                    0.8_f64
                } else {
                    0.0_f64
                }
            } else {
                0.0_f64
            };
            let curated_bonus =
                if use_curated_name_prior && (has_two_sided_anchor || raw_list_like_context) {
                    curated_name_prior_bonus_pt(trimmed)
                } else {
                    0.0_f64
                };
            let non_curated_variant_penalty =
                if use_curated_name_prior && has_two_sided_anchor && curated_bonus <= 0.0_f64 {
                    2.0_f64
                } else {
                    0.0_f64
                };
            let effective_err = (raw_err
                + context_penalty
                + raw_context_overlap_penalty
                + list_shape_penalty
                + two_sided_shape_penalty
                + non_curated_variant_penalty
                - curated_bonus)
                .max(0.0_f64);
            scored.push(ScoredDictionaryCandidate {
                text: trimmed.to_owned(),
                raw_error_pt: raw_err,
                effective_error_pt: effective_err,
                word_count: candidate_word_count as u32,
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
        let mut band = ranged;
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
                AnchorMode::LeftOnly => {
                    let predicted_start =
                        anchor.left_x + left_width.pt + space_width.pt + anchor.row_bias_pt;
                    (predicted_start - redaction.bbox.x0 as f64).abs()
                }
                AnchorMode::RightOnly => {
                    let predicted_end = anchor.right_x - space_width.pt + anchor.row_bias_pt;
                    (predicted_end - redaction.bbox.x1 as f64).abs()
                }
                AnchorMode::TwoSided => {
                    continue;
                }
            };
            let side_alignment_limit = match anchor.mode {
                AnchorMode::LeftOnly | AnchorMode::RightOnly => {
                    let base = anchor_filter_limit_pt * 2.5_f64;
                    if gap_ratio >= MULTI_SPAN_GAP_RATIO_THRESHOLD {
                        base.max(anchor_gap_pt * 0.75_f64)
                            .max(redaction_width_pt * 1.8_f64)
                            .max(ONE_SIDED_LIST_CONTEXT_SIDE_ALIGNMENT_MIN_PT * 2.0_f64)
                    } else if list_like_context {
                        base.max(redaction_width_pt * 1.4_f64)
                            .max(ONE_SIDED_LIST_CONTEXT_SIDE_ALIGNMENT_MIN_PT)
                    } else {
                        base
                    }
                }
                AnchorMode::TwoSided => {
                    continue;
                }
            };
            if side_alignment_err > side_alignment_limit {
                continue;
            }
            funnel.after_anchor += 1;
            funnel.after_box += 1;
            let raw_err = box_err + (side_alignment_err * 0.20_f64);
            let context_penalty = punctuation_context_penalty(
                &anchor.left_anchor_text,
                &anchor.right_anchor_text,
                trimmed,
            );
            let candidate_word_count = trimmed.split_whitespace().count();
            let raw_context_overlap_penalty = if use_curated_name_prior && raw_list_like_context {
                anchor_overlap_penalty_pt(raw_left_context, raw_right_context, trimmed)
            } else {
                0.0_f64
            };
            let list_shape_penalty =
                if use_curated_name_prior && raw_list_like_context && candidate_word_count == 3 {
                    0.18_f64
                } else {
                    0.0_f64
                };
            let two_sided_shape_penalty = if has_two_sided_anchor {
                if candidate_word_count <= 1 {
                    2.5_f64
                } else if candidate_word_count >= 4 {
                    0.8_f64
                } else {
                    0.0_f64
                }
            } else {
                0.0_f64
            };
            let curated_bonus =
                if use_curated_name_prior && (has_two_sided_anchor || raw_list_like_context) {
                    curated_name_prior_bonus_pt(trimmed)
                } else {
                    0.0_f64
                };
            let non_curated_variant_penalty =
                if use_curated_name_prior && has_two_sided_anchor && curated_bonus <= 0.0_f64 {
                    2.0_f64
                } else {
                    0.0_f64
                };
            let effective_err = (raw_err
                + context_penalty
                + raw_context_overlap_penalty
                + list_shape_penalty
                + two_sided_shape_penalty
                + non_curated_variant_penalty
                - curated_bonus)
                .max(0.0_f64);
            scored.push(ScoredDictionaryCandidate {
                text: trimmed.to_owned(),
                raw_error_pt: raw_err,
                effective_error_pt: effective_err,
                word_count: candidate_word_count as u32,
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
    let candidate_cap = if has_two_sided_anchor {
        TWO_SIDED_MAX_CANDIDATES
    } else if list_like_context || raw_list_like_context {
        LIST_CONTEXT_MAX_CANDIDATES
    } else {
        FIXED_MAX_CANDIDATES
    };
    let selected = scored
        .iter()
        .take(candidate_cap)
        .cloned()
        .collect::<Vec<_>>();
    let exact_matches = selected
        .iter()
        .filter(|candidate| candidate.raw_error_pt <= EXACT_MATCH_RAW_ERROR_EPSILON_PT)
        .map(|candidate| candidate.text.clone())
        .collect::<Vec<_>>();

    let denom = epsilon.max(FIXED_TOLERANCE_PT).max(0.0001);
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

fn build_shared_context_hints(redactions: &[RedactionOccurrence]) -> Vec<Vec<ClusterAnchorHint>> {
    let mut out = vec![Vec::<ClusterAnchorHint>::new(); redactions.len()];
    let mut by_page = std::collections::BTreeMap::<u32, Vec<usize>>::new();
    for (index, redaction) in redactions.iter().enumerate() {
        by_page.entry(redaction.page_index).or_default().push(index);
    }
    for page_indices in by_page.values_mut() {
        page_indices.sort_by(|left_index, right_index| {
            let left = &redactions[*left_index];
            let right = &redactions[*right_index];
            let left_center_y = ((left.bbox.y0 + left.bbox.y1) * 0.5_f32) as f64;
            let right_center_y = ((right.bbox.y0 + right.bbox.y1) * 0.5_f32) as f64;
            left_center_y
                .partial_cmp(&right_center_y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.bbox
                        .x0
                        .partial_cmp(&right.bbox.x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        let mut visited = std::collections::BTreeSet::<usize>::new();
        for seed_index in page_indices.iter().copied() {
            if visited.contains(&seed_index) {
                continue;
            }
            let mut cluster = Vec::<usize>::new();
            let mut queue = vec![seed_index];
            visited.insert(seed_index);
            while let Some(current_index) = queue.pop() {
                cluster.push(current_index);
                for other_index in page_indices.iter().copied() {
                    if visited.contains(&other_index) {
                        continue;
                    }
                    if redaction_rows_are_linked(
                        &redactions[current_index],
                        &redactions[other_index],
                    ) {
                        visited.insert(other_index);
                        queue.push(other_index);
                    }
                }
            }
            let mut merged = Vec::<ClusterAnchorHint>::new();
            let mut seen = BTreeSet::<String>::new();
            for redaction_index in cluster.iter().copied() {
                for span in parse_context_spans_from_meta(&redactions[redaction_index]) {
                    let key = format!(
                        "{:.1}:{:.1}:{:.1}:{:.1}:{}",
                        span.bbox.x0, span.bbox.y0, span.bbox.x1, span.bbox.y1, span.text
                    );
                    if seen.insert(key) {
                        merged.push(ClusterAnchorHint {
                            text: span.text,
                            bbox: span.bbox,
                            line_bucket: span.line_bucket,
                            role_hint: span.role_hint,
                            source: span.source,
                        });
                    }
                }
                for hit in redactions[redaction_index]
                    .underlying_text
                    .iter()
                    .filter(|hit| {
                        let value = hit.text.trim();
                        !value.is_empty()
                    })
                {
                    let key = format!(
                        "{:.1}:{:.1}:{:.1}:{:.1}:{}",
                        hit.bbox.x0,
                        hit.bbox.y0,
                        hit.bbox.x1,
                        hit.bbox.y1,
                        hit.text.trim()
                    );
                    if seen.insert(key) {
                        let role_hint = if hit.bbox.x1 <= redactions[redaction_index].bbox.x0 {
                            "left".to_owned()
                        } else if hit.bbox.x0 >= redactions[redaction_index].bbox.x1 {
                            "right".to_owned()
                        } else {
                            "center".to_owned()
                        };
                        merged.push(ClusterAnchorHint {
                            text: hit.text.trim().to_owned(),
                            bbox: hit.bbox,
                            line_bucket: ((hit.bbox.y0 + hit.bbox.y1) * 0.5_f32 / 2.0_f32).round()
                                as i32,
                            role_hint,
                            source: "legacy_hit".to_owned(),
                        });
                    }
                }
            }
            merged.sort_by(|left, right| {
                let left_center_y = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
                let right_center_y = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
                left_center_y
                    .partial_cmp(&right_center_y)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        left.bbox
                            .x0
                            .partial_cmp(&right.bbox.x0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| left.text.cmp(&right.text))
            });
            for redaction_index in cluster {
                out[redaction_index] = merged.clone();
            }
        }
    }
    out
}

fn parse_context_spans_from_meta(redaction: &RedactionOccurrence) -> Vec<ContextSpanRecord> {
    let Some(raw) = redaction.meta.get(CONTEXT_SPANS_META_KEY) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<ContextSpanRecord>>(raw).unwrap_or_default()
}

fn redaction_rows_are_contiguous(left: &RedactionOccurrence, right: &RedactionOccurrence) -> bool {
    let left_center_y = ((left.bbox.y0 + left.bbox.y1) * 0.5_f32) as f64;
    let right_center_y = ((right.bbox.y0 + right.bbox.y1) * 0.5_f32) as f64;
    let y_delta = (right_center_y - left_center_y).abs();
    if y_delta <= ROW_CONTEXT_SAME_LINE_MAX_Y_DELTA_PT {
        let x_gap = (right.bbox.x0 as f64 - left.bbox.x1 as f64).max(0.0_f64);
        return x_gap <= ROW_CONTEXT_MAX_GROUP_GAP_PT;
    }
    if right_center_y <= left_center_y || y_delta > ROW_CONTEXT_WRAP_LINE_MAX_Y_GAP_PT {
        return false;
    }
    let wraps_to_new_line =
        right.bbox.x0 as f64 + ROW_CONTEXT_WRAP_RESET_X_DELTA_PT < left.bbox.x0 as f64;
    let continues_forward =
        right.bbox.x0 as f64 <= left.bbox.x1 as f64 + ROW_CONTEXT_WRAP_FORWARD_X_ALLOWANCE_PT;
    wraps_to_new_line || continues_forward
}

fn redaction_rows_are_linked(left: &RedactionOccurrence, right: &RedactionOccurrence) -> bool {
    redaction_rows_are_contiguous(left, right) || redaction_rows_are_contiguous(right, left)
}

fn select_cluster_hint(
    redaction: &RedactionOccurrence,
    cluster_hints: &[ClusterAnchorHint],
    local_hint_hit: Option<&crate::types::redaction_types::UnderlyingTextHit>,
    want_left: bool,
) -> Option<ClusterAnchorHint> {
    if let Some(local) = local_hint_hit {
        let text = local.text.trim();
        if !text.is_empty() {
            let near_edge = if want_left {
                let gap = (redaction.bbox.x0 - local.bbox.x1).max(0.0_f32);
                gap <= CLUSTER_ANCHOR_HINT_MAX_GAP_PT
                    || local.bbox.x1 <= redaction.bbox.x0 + CLUSTER_ANCHOR_HINT_EDGE_TOLERANCE_PT
            } else {
                let gap = (local.bbox.x0 - redaction.bbox.x1).max(0.0_f32);
                gap <= CLUSTER_ANCHOR_HINT_MAX_GAP_PT
                    || local.bbox.x0 >= redaction.bbox.x1 - CLUSTER_ANCHOR_HINT_EDGE_TOLERANCE_PT
            };
            if near_edge {
                return Some(ClusterAnchorHint {
                    text: text.to_owned(),
                    bbox: local.bbox,
                    line_bucket: ((local.bbox.y0 + local.bbox.y1) * 0.5_f32 / 2.0_f32).round()
                        as i32,
                    role_hint: "local".to_owned(),
                    source: "local_hint".to_owned(),
                });
            }
        }
    }
    let mut candidates = Vec::<(ClusterAnchorHint, f64, bool)>::new();
    if let Some(local) = local_hint_hit {
        let text = local.text.trim();
        if !text.is_empty() {
            candidates.push((
                ClusterAnchorHint {
                    text: text.to_owned(),
                    bbox: local.bbox,
                    line_bucket: ((local.bbox.y0 + local.bbox.y1) * 0.5_f32 / 2.0_f32).round()
                        as i32,
                    role_hint: "local".to_owned(),
                    source: "local_hint".to_owned(),
                },
                0.0_f64,
                true,
            ));
        }
    }
    for hint in cluster_hints {
        let text = hint.text.trim();
        if text.is_empty() {
            continue;
        }
        candidates.push((hint.clone(), 0.0_f64, false));
    }
    let red_center_y = ((redaction.bbox.y0 + redaction.bbox.y1) * 0.5_f32) as f64;
    let mut best: Option<(ClusterAnchorHint, f64)> = None;
    for (hint, _score, is_local) in candidates {
        let hint_center_y = ((hint.bbox.y0 + hint.bbox.y1) * 0.5_f32) as f64;
        let y_distance = (hint_center_y - red_center_y).abs();
        if y_distance > ROW_CONTEXT_WRAP_LINE_MAX_Y_GAP_PT + 6.0_f64 {
            continue;
        }
        let (edge_gap, overlap_penalty) = if want_left {
            let gap = redaction.bbox.x0 as f64 - hint.bbox.x1 as f64;
            let overlap_penalty =
                if hint.bbox.x1 > redaction.bbox.x0 + CLUSTER_ANCHOR_HINT_EDGE_TOLERANCE_PT {
                    60.0_f64
                } else {
                    0.0_f64
                };
            (gap, overlap_penalty)
        } else {
            let gap = hint.bbox.x0 as f64 - redaction.bbox.x1 as f64;
            let overlap_penalty =
                if hint.bbox.x0 < redaction.bbox.x1 - CLUSTER_ANCHOR_HINT_EDGE_TOLERANCE_PT {
                    60.0_f64
                } else {
                    0.0_f64
                };
            (gap, overlap_penalty)
        };
        if edge_gap > CLUSTER_ANCHOR_HINT_MAX_GAP_PT as f64 {
            continue;
        }
        let role_penalty = match hint.role_hint.as_str() {
            "left" if want_left => 0.0_f64,
            "right" if !want_left => 0.0_f64,
            "center" => 8.0_f64,
            "local" => 0.0_f64,
            _ => 20.0_f64,
        };
        let source_penalty = match hint.source.as_str() {
            "line_cluster" => 0.0_f64,
            "local_hint" => -2.0_f64,
            "legacy_hit" => 2.0_f64,
            _ => 4.0_f64,
        };
        let local_bonus = if is_local { -12.0_f64 } else { 0.0_f64 };
        let line_bucket_penalty = (hint.line_bucket.abs() as f64) * 0.001_f64;
        let score = edge_gap.max(0.0_f64)
            + (y_distance * 4.0_f64)
            + overlap_penalty
            + role_penalty
            + source_penalty
            + line_bucket_penalty
            + local_bonus;
        match best {
            None => best = Some((hint, score)),
            Some((_, best_score)) if score < best_score => best = Some((hint, score)),
            _ => {}
        }
    }
    best.map(|(hint, _)| hint)
}

fn select_anchor_pair(
    redaction: &RedactionOccurrence,
    runs: &[&FontTextRun],
    cluster_hints: &[ClusterAnchorHint],
    assets: &std::collections::BTreeMap<String, FontAsset>,
    typography_profile: &TypographyProfile,
) -> Option<AnchorPairData> {
    let red_center_y = rect_center_y(&redaction.bbox);
    let red_center_x = ((redaction.bbox.x0 + redaction.bbox.x1) * 0.5) as f64;
    let local_left_hint_hit = redaction
        .underlying_text
        .first()
        .filter(|hit| !hit.text.trim().is_empty());
    let local_right_hint_hit = redaction
        .underlying_text
        .get(1)
        .filter(|hit| !hit.text.trim().is_empty());
    let left_cluster_hint =
        select_cluster_hint(redaction, cluster_hints, local_left_hint_hit, true);
    let right_cluster_hint =
        select_cluster_hint(redaction, cluster_hints, local_right_hint_hit, false);
    let left_hint = left_cluster_hint.as_ref().map(|hit| hit.text.trim());
    let right_hint = right_cluster_hint.as_ref().map(|hit| hit.text.trim());
    let hints = AnchorHints {
        left_text: left_hint,
        left_x: left_cluster_hint.as_ref().map(|hit| hit.bbox.x0 as f64),
        right_text: right_hint,
        right_x: right_cluster_hint.as_ref().map(|hit| hit.bbox.x0 as f64),
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
            let left_straddle_penalty =
                u8::from(left_end > redaction.bbox.x0 as f64 + ANCHOR_PAIR_STRADDLE_TOLERANCE_PT);
            let right_straddle_penalty = u8::from(
                right_start < redaction.bbox.x1 as f64 - ANCHOR_PAIR_STRADDLE_TOLERANCE_PT,
            );
            let straddle_penalty = left_straddle_penalty + right_straddle_penalty;
            let left_overlap =
                horizontal_overlap_font_rect_with_rect(&left_run.bbox, &redaction.bbox);
            let right_overlap =
                horizontal_overlap_font_rect_with_rect(&right_run.bbox, &redaction.bbox);
            let overlap_penalty = u8::from(left_overlap > ANCHOR_PAIR_MAX_OVERLAP_PT)
                + u8::from(right_overlap > ANCHOR_PAIR_MAX_OVERLAP_PT);
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
                straddle_penalty,
                overlap_penalty,
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
            .then_with(|| left_pair.straddle_penalty.cmp(&right_pair.straddle_penalty))
            .then_with(|| left_pair.overlap_penalty.cmp(&right_pair.overlap_penalty))
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
    let left_x_bound = redaction.bbox.x0 as f64 + ANCHOR_PAIR_STRADDLE_TOLERANCE_PT;
    let right_x_bound = redaction.bbox.x1 as f64 - ANCHOR_PAIR_STRADDLE_TOLERANCE_PT;
    if left_x > left_x_bound || right_x < right_x_bound {
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
        .filter(|run| run.bbox.x1 <= redaction.bbox.x0 + ONE_SIDED_ANCHOR_EDGE_TOLERANCE_PT)
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
        .filter(|run| run.bbox.x0 >= redaction.bbox.x1 - ONE_SIDED_ANCHOR_EDGE_TOLERANCE_PT)
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

fn horizontal_overlap_font_rect_with_rect(a: &FontRect, b: &Rect) -> f64 {
    (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0) as f64
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
        if count < 2 {
            return false;
        }
        if count > 3 {
            return false;
        }
    }
    true
}

fn looks_like_alpha_phrase_candidate(text: &str) -> bool {
    let trimmed = text.trim();
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

fn curated_name_prior_bonus_pt(candidate: &str) -> f64 {
    let normalized = candidate.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return 0.0_f64;
    }
    let curated = curated_name_prior_set();
    if curated.contains(&normalized) {
        CURATED_NAME_PRIOR_BONUS_PT
    } else {
        0.0_f64
    }
}

fn curated_name_prior_set() -> &'static BTreeSet<String> {
    static CURATED: OnceLock<BTreeSet<String>> = OnceLock::new();
    CURATED.get_or_init(|| {
        include_str!("../../../assets/names.txt")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| line.to_ascii_uppercase())
            .collect::<BTreeSet<_>>()
    })
}

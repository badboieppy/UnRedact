use super::common::{
    anchor_overlap_penalty_pt, candidate_char_units, is_list_like_context,
    punctuation_context_penalty,
};
use super::joint_assignment::{apply_row_joint_assignment, apply_row_sequence_consensus};
use super::visual_logic::{apply_visual_scores_from_bytes, VisualGuessScoreConfig};
use crate::data::fonts_data::FontsData;
use crate::data::guess_candidate_data::collect_guess_candidates;
use crate::data::redaction_evidence_data::collect_redaction_evidence;
use crate::data::types::guess_candidate_types::{
    CollectGuessCandidatesRequest, GuessCandidateSet, MeasuredCandidate,
};
use crate::data::types::redaction_evidence_types::{
    AnchorMode as EvidenceAnchorMode, AnchorSide, CollectRedactionEvidenceRequest,
    RedactionEvidenceDiagnostic,
};
use crate::logic::types::guess_input_types::{GuessInputRow, GuessInputSet};
use crate::types::diagnostic_types::{DiagnosticRecord, DiagnosticValue};
use crate::types::file_types::FontRunReport;
#[cfg(feature = "cli-entry")]
use crate::types::guess_types::AnchorReport;
use crate::types::guess_types::{
    AnchorCandidateDecision, AnchorDecisionRecord, AnchorSelectionReasonCode,
    AnchorSideDecision, AnchorSourceLabel, AnchorType, GuessCandidate, GuessConfig, GuessContext,
    GuessReport, RedactionGuess,
};
use crate::types::redaction_types::{RedactionOccurrence, RedactionReport};
use crate::types::time::Instant;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const FIXED_VISUAL_MIN_INK_PIXELS: u32 = 64_u32;
const FIXED_VISUAL_DROP_THRESHOLD: Option<f32> = None;
const CURATED_NAME_PRIOR_BONUS_PT: f64 = 1.10_f64;
const MAX_RANKED_CANDIDATES: usize = 900;
const MAX_EXACT_MATCHES: usize = 32;

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

#[cfg(feature = "cli-entry")]
pub struct RunAnchorFromBytesRequest<'a> {
    pub pdf_name: &'a str,
    pub pdf_bytes: &'a [u8],
    pub redactions: &'a RedactionReport,
    pub diagnostics: &'a [String],
    pub preloaded_font_runs: Option<&'a FontRunReport>,
    pub preloaded_font_runs_elapsed_ms: Option<u128>,
}

#[cfg(feature = "cli-entry")]
#[inline]
pub fn run_anchor_from_bytes(req: RunAnchorFromBytesRequest<'_>) -> Result<AnchorReport, String> {
    let cfg = GuessConfig {
        visual_score: false,
        ..GuessConfig::default()
    };
    let empty_dictionary = Vec::<String>::new();
    let report = run_from_bytes(RunGuessFromBytesRequest {
        pdf_name: req.pdf_name,
        pdf_bytes: req.pdf_bytes,
        redactions: req.redactions,
        dictionary: empty_dictionary.as_slice(),
        diagnostics: req.diagnostics,
        preloaded_font_runs: req.preloaded_font_runs,
        preloaded_font_runs_elapsed_ms: req.preloaded_font_runs_elapsed_ms,
        cfg: &cfg,
    })?;
    Ok(report.to_anchor_report())
}

#[inline]
pub fn run_from_bytes(req: RunGuessFromBytesRequest<'_>) -> Result<GuessReport, String> {
    let started = Instant::now();
    let mut diagnostics = translate_input_diagnostics(req.diagnostics);
    let use_curated_name_prior = req
        .diagnostics
        .iter()
        .any(|line| line == "dictionary_source=default_names");

    let evidence_started = Instant::now();
    let evidence = collect_redaction_evidence(CollectRedactionEvidenceRequest {
        input_name: req.pdf_name,
        pdf_bytes: req.pdf_bytes,
        redactions: req.redactions,
    })?;
    diagnostics.push(timing_diagnostic(
        "guess_redaction_evidence",
        evidence_started.elapsed().as_millis(),
    ));
    diagnostics.extend(
        evidence
            .diagnostics
            .iter()
            .map(translate_evidence_diagnostic),
    );

    let candidate_started = Instant::now();
    let candidate_set = collect_guess_candidates(CollectGuessCandidatesRequest {
        evidence: &evidence,
        dictionary: req.dictionary,
    })?;
    diagnostics.push(timing_diagnostic(
        "guess_candidate_data",
        candidate_started.elapsed().as_millis(),
    ));
    diagnostics.extend(
        candidate_set
            .diagnostics
            .iter()
            .map(translate_candidate_diagnostic),
    );

    let guess_inputs = map_guess_inputs(&candidate_set);
    let build_started = Instant::now();
    let (mut guesses, anchors) =
        build_report_rows(req.redactions, &guess_inputs, use_curated_name_prior);
    diagnostics.push(timing_diagnostic(
        "guess_rank_rows",
        build_started.elapsed().as_millis(),
    ));

    let assigned = apply_row_joint_assignment(&mut guesses);
    apply_row_sequence_consensus(&mut guesses, &assigned);

    if req.cfg.visual_score {
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
                .expect("font runs should exist when visual scoring is enabled")
        });
        let font_runs_ms = req
            .preloaded_font_runs_elapsed_ms
            .unwrap_or_else(|| font_runs_started.elapsed().as_millis());
        diagnostics.push(timing_diagnostic("guess_visual_font_runs", font_runs_ms));
        let visual_started = Instant::now();
        let visual_cfg = VisualGuessScoreConfig {
            enabled: req.cfg.visual_score,
            dpi: req.cfg.visual_score_dpi,
            min_ink_pixels: FIXED_VISUAL_MIN_INK_PIXELS,
            drop_threshold: FIXED_VISUAL_DROP_THRESHOLD,
        };
        match apply_visual_scores_from_bytes(
            req.pdf_bytes,
            req.redactions,
            font_runs,
            &mut guesses,
            visual_cfg,
        ) {
            Ok(visual_diagnostics) => diagnostics.extend(visual_diagnostics),
            Err(error) => diagnostics.push(DiagnosticRecord::error(
                "logic",
                "guess_visual_score",
                "visual_score_failed",
                &error,
            )),
        }
        diagnostics.push(timing_diagnostic(
            "guess_visual_score",
            visual_started.elapsed().as_millis(),
        ));
    } else {
        diagnostics.push(DiagnosticRecord::info(
            "logic",
            "guess_visual_score",
            "visual_score_disabled",
        ));
    }

    annotate_guess_confidence(&mut guesses);
    diagnostics.push(timing_diagnostic(
        "guess_run_from_bytes_total",
        started.elapsed().as_millis(),
    ));

    Ok(GuessReport {
        input_redactions: format!("memory://{}.redactions.json", req.pdf_name),
        input_fonts: format!("memory://{}.fonts.json", req.pdf_name),
        guesses,
        anchors,
        diagnostics,
    })
}

fn map_guess_inputs(candidate_set: &GuessCandidateSet) -> GuessInputSet {
    GuessInputSet {
        input: candidate_set.input.clone(),
        rows: candidate_set
            .rows
            .iter()
            .map(|row| GuessInputRow {
                row_id: row.row_id.clone(),
                page_index: row.page_index,
                redaction: row.redaction.clone(),
                anchor_set: row.anchor_set.clone(),
                font: row.font.clone(),
                neighbor_facts: row.neighbor_facts.clone(),
                candidates: row.candidates.clone(),
            })
            .collect(),
    }
}

fn build_report_rows(
    redactions: &RedactionReport,
    guess_inputs: &GuessInputSet,
    use_curated_name_prior: bool,
) -> (Vec<RedactionGuess>, Vec<AnchorDecisionRecord>) {
    let mut rows_by_redaction = guess_inputs
        .rows
        .iter()
        .map(|row| (row.redaction.redaction_id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let mut guesses = Vec::<RedactionGuess>::with_capacity(redactions.redactions.len());
    let mut anchors = Vec::<AnchorDecisionRecord>::with_capacity(redactions.redactions.len());

    for (index, redaction) in redactions.redactions.iter().enumerate() {
        let redaction_id = format!("page{}_redaction{index:03}", redaction.page_index);
        if let Some(row) = rows_by_redaction.remove(&redaction_id) {
            let (guess, anchor_record) = build_guess_for_row(row, use_curated_name_prior);
            guesses.push(guess);
            anchors.push(anchor_record);
        } else {
            guesses.push(build_placeholder_guess(redaction));
            anchors.push(build_missing_anchor_record(redaction, index));
        }
    }

    (guesses, anchors)
}

fn build_guess_for_row(
    row: &GuessInputRow,
    use_curated_name_prior: bool,
) -> (RedactionGuess, AnchorDecisionRecord) {
    let left_text = row
        .anchor_set
        .left
        .as_ref()
        .map(|side| side.text.trim().to_owned())
        .unwrap_or_default();
    let right_text = row
        .anchor_set
        .right
        .as_ref()
        .map(|side| side.text.trim().to_owned())
        .unwrap_or_default();
    let gap_pt = guess_gap_pt(row);
    let target_width_pt = row.anchor_set.geometry.target_guess_width_pt as f64;
    let tolerance_pt = row.anchor_set.geometry.tolerance_pt.max(0.5_f32) as f64;
    let char_width_pt = estimate_char_width_pt(&row.candidates);
    let mut exact_matches = Vec::<String>::new();
    let mut guesses = row
        .candidates
        .iter()
        .map(|candidate| {
            let fit_slack_pt = (target_width_pt - candidate.width_pt as f64).max(0.0_f64);
            if fit_slack_pt <= 0.0001_f64 && exact_matches.len() < MAX_EXACT_MATCHES {
                exact_matches.push(candidate.text.clone());
            }
            let context_penalty = punctuation_context_penalty(
                left_text.as_str(),
                right_text.as_str(),
                candidate.text.as_str(),
            );
            let overlap_penalty = anchor_overlap_penalty_pt(
                left_text.as_str(),
                right_text.as_str(),
                candidate.text.as_str(),
            );
            let density_penalty = candidate_density_penalty(candidate, char_width_pt, target_width_pt);
            let one_sided_penalty = if matches!(
                row.anchor_set.mode,
                EvidenceAnchorMode::LeftOnly | EvidenceAnchorMode::RightOnly
            ) {
                0.10_f64
            } else {
                0.0_f64
            };
            let curated_bonus = if use_curated_name_prior {
                curated_name_prior_bonus_pt(candidate.text.as_str())
            } else {
                0.0_f64
            };
            let list_bonus = if is_list_like_context(left_text.as_str(), right_text.as_str())
                && (2..=4).contains(&candidate.word_count)
            {
                0.08_f64
            } else {
                0.0_f64
            };
            let fit_cost = fit_slack_pt / tolerance_pt.max(1.0_f64);
            let context_cost = context_penalty
                + overlap_penalty
                + density_penalty;
            let total_cost = fit_cost + context_cost + one_sided_penalty - curated_bonus - list_bonus;
            GuessCandidate {
                text: candidate.text.clone(),
                score: (1.0_f64 / (1.0_f64 + total_cost.max(0.0_f64))).clamp(0.0_f64, 1.0_f64)
                    as f32,
                error_pt: fit_slack_pt as f32,
                word_count: candidate.word_count,
                width_pt: Some(candidate.width_pt),
            }
        })
        .collect::<Vec<_>>();
    guesses.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.error_pt
                    .partial_cmp(&right.error_pt)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.text.cmp(&right.text))
    });
    guesses.truncate(MAX_RANKED_CANDIDATES);
    exact_matches.sort();
    exact_matches.dedup();

    let guess = RedactionGuess {
        page_index: row.page_index,
        bbox: row.redaction.bbox,
        candidates: guesses,
        exact_matches,
        context: GuessContext {
            left_anchor_text: left_text.clone(),
            right_anchor_text: right_text.clone(),
            gap_pt,
            char_width_pt,
            tol_pt: row.anchor_set.geometry.tolerance_pt,
            anchor_left_x: row.anchor_set.geometry.usable_left_edge_x_pt,
            anchor_right_x: row.anchor_set.geometry.usable_right_edge_x_pt,
            anchor_font_name: Some(row.font.font_name.clone()),
            anchor_font_size_pt: Some(row.font.font_size_pt),
            anchor_h_scale_pct: Some(row.font.h_scale_pct),
            anchor_row_bias_pt: Some(row.anchor_set.geometry.line_bias_pt),
            anchor_mode: Some(row.anchor_set.mode.as_str().to_owned()),
            confidence_score: None,
            confidence_factors: None,
            anchor_row_id: Some(row.row_id.clone()),
            left_anchor_id: row.anchor_set.left.as_ref().map(|side| side.anchor_id.clone()),
            right_anchor_id: row.anchor_set.right.as_ref().map(|side| side.anchor_id.clone()),
            left_anchor_type: row.anchor_set.left.as_ref().map(|_| AnchorType::Left),
            right_anchor_type: row.anchor_set.right.as_ref().map(|_| AnchorType::Right),
            left_anchor_selected_source: row
                .anchor_set
                .left
                .as_ref()
                .map(|_| AnchorSourceLabel::RunExact),
            right_anchor_selected_source: row
                .anchor_set
                .right
                .as_ref()
                .map(|_| AnchorSourceLabel::RunExact),
            left_anchor_confidence: row.anchor_set.left.as_ref().map(|_| anchor_side_confidence()),
            right_anchor_confidence: row
                .anchor_set
                .right
                .as_ref()
                .map(|_| anchor_side_confidence()),
            row_anchor_confidence: Some(anchor_row_confidence(&row.anchor_set.mode)),
            has_anchor_pair: true,
        },
        visual_compared_pixels: None,
        visual_mean_abs_diff: None,
        visual_changed_pixel_ratio: None,
        visual_reason: None,
        visual_dropped: false,
    };
    let anchor_record = build_anchor_record(row);
    (guess, anchor_record)
}

fn build_placeholder_guess(redaction: &RedactionOccurrence) -> RedactionGuess {
    RedactionGuess {
        page_index: redaction.page_index,
        bbox: redaction.bbox,
        candidates: Vec::new(),
        exact_matches: Vec::new(),
        context: GuessContext {
            left_anchor_text: String::new(),
            right_anchor_text: String::new(),
            gap_pt: redaction.bbox.width().abs(),
            char_width_pt: 4.0_f32,
            tol_pt: 8.0_f32,
            anchor_left_x: None,
            anchor_right_x: None,
            anchor_font_name: None,
            anchor_font_size_pt: None,
            anchor_h_scale_pct: None,
            anchor_row_bias_pt: None,
            anchor_mode: None,
            confidence_score: Some(0.0_f32),
            confidence_factors: Some("base=0.000;anchor=0.000;visual=0.780".to_owned()),
            anchor_row_id: None,
            left_anchor_id: None,
            right_anchor_id: None,
            left_anchor_type: None,
            right_anchor_type: None,
            left_anchor_selected_source: None,
            right_anchor_selected_source: None,
            left_anchor_confidence: None,
            right_anchor_confidence: None,
            row_anchor_confidence: Some(0.0_f32),
            has_anchor_pair: false,
        },
        visual_compared_pixels: None,
        visual_mean_abs_diff: None,
        visual_changed_pixel_ratio: None,
        visual_reason: None,
        visual_dropped: false,
    }
}

fn build_anchor_record(row: &GuessInputRow) -> AnchorDecisionRecord {
    let selected_mode = row.anchor_set.mode.as_str().to_owned();
    let selected_candidate_id = format!("{}_{}_selected", row.row_id, selected_mode);
    let left = row
        .anchor_set
        .left
        .as_ref()
        .map(|side| build_anchor_side_decision(side, AnchorType::Left));
    let right = row
        .anchor_set
        .right
        .as_ref()
        .map(|side| build_anchor_side_decision(side, AnchorType::Right));
    let reason_code = match row.anchor_set.mode {
        EvidenceAnchorMode::TwoSided => AnchorSelectionReasonCode::SelectedPairTwoSided,
        EvidenceAnchorMode::LeftOnly => AnchorSelectionReasonCode::SelectedLeftOnlyFallback,
        EvidenceAnchorMode::RightOnly => AnchorSelectionReasonCode::SelectedRightOnlyFallback,
    };
    AnchorDecisionRecord {
        anchor_row_id: row.row_id.clone(),
        page_index: row.page_index,
        bbox: row.redaction.bbox,
        selected_candidate_id: Some(selected_candidate_id.clone()),
        selected_mode: Some(selected_mode.clone()),
        candidates: vec![AnchorCandidateDecision {
            candidate_id: selected_candidate_id,
            anchor_mode: selected_mode,
            was_selected: true,
            reason_code,
            tie_break_rank: Some(0),
            left,
            right,
            anchor_font_name: Some(row.font.font_name.clone()),
            anchor_font_size_pt: Some(row.font.font_size_pt),
            anchor_h_scale_pct: Some(row.font.h_scale_pct),
        }],
    }
}

fn build_missing_anchor_record(
    redaction: &RedactionOccurrence,
    index: usize,
) -> AnchorDecisionRecord {
    AnchorDecisionRecord {
        anchor_row_id: format!("page{}_row{index}", redaction.page_index),
        page_index: redaction.page_index,
        bbox: redaction.bbox,
        selected_candidate_id: None,
        selected_mode: None,
        candidates: Vec::new(),
    }
}

fn build_anchor_side_decision(side: &AnchorSide, anchor_type: AnchorType) -> AnchorSideDecision {
    AnchorSideDecision {
        anchor_id: side.anchor_id.clone(),
        anchor_type,
        text: side.text.clone(),
        x: side.text_edge_x_pt,
        selected_source: AnchorSourceLabel::RunExact,
        projection_source: None,
        alternate_x: None,
        selected_minus_alternate_delta_pt: None,
        confidence: Some(anchor_side_confidence()),
    }
}

fn estimate_char_width_pt(candidates: &[MeasuredCandidate]) -> f32 {
    let mut samples = candidates
        .iter()
        .filter_map(|candidate| {
            let units = candidate_char_units(candidate.text.as_str()) as f32;
            (units.is_finite() && units > 0.0_f32).then_some(candidate.width_pt / units)
        })
        .filter(|value| value.is_finite() && *value > 0.0_f32)
        .take(32)
        .collect::<Vec<_>>();
    if samples.is_empty() {
        return 4.0_f32;
    }
    samples.sort_by(|left, right| {
        left.partial_cmp(right)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let median_index = ((samples.len() as f32) * 0.5_f32).floor() as usize;
    samples[median_index.min(samples.len().saturating_sub(1))].max(0.1_f32)
}

fn guess_gap_pt(row: &GuessInputRow) -> f32 {
    match (&row.anchor_set.left, &row.anchor_set.right) {
        (Some(left), Some(right)) => {
            (row.anchor_set.geometry.usable_right_edge_x_pt.unwrap_or(right.text_edge_x_pt)
                - left.text_edge_x_pt)
                .abs()
                .max(row.anchor_set.geometry.target_guess_width_pt)
        }
        _ => row.anchor_set.geometry.target_guess_width_pt,
    }
}

fn candidate_density_penalty(
    candidate: &MeasuredCandidate,
    char_width_pt: f32,
    target_width_pt: f64,
) -> f64 {
    let estimated = candidate_char_units(candidate.text.as_str()) * char_width_pt as f64;
    if target_width_pt <= 0.0_f64 {
        return 0.0_f64;
    }
    ((estimated - target_width_pt).abs() / target_width_pt.max(1.0_f64)).min(1.2_f64)
}

fn anchor_side_confidence() -> f32 {
    0.92_f32
}

fn anchor_row_confidence(mode: &EvidenceAnchorMode) -> f32 {
    match mode {
        EvidenceAnchorMode::TwoSided => 1.0_f32,
        EvidenceAnchorMode::LeftOnly | EvidenceAnchorMode::RightOnly => 0.78_f32,
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
            0.0_f64
        } else if guess.context.anchor_mode.as_deref() == Some("two_sided") {
            1.0_f64
        } else {
            0.78_f64
        };
        let visual = guess
            .visual_mean_abs_diff
            .map(|value| (1.0_f64 - (value as f64 / 0.28_f64)).clamp(0.30_f64, 1.0_f64))
            .unwrap_or(0.78_f64);
        let confidence =
            (base * 0.70_f64 + anchor * 0.20_f64 + visual * 0.10_f64).clamp(0.0_f64, 1.0_f64);
        guess.context.confidence_score = Some(confidence as f32);
        guess.context.row_anchor_confidence = Some(confidence as f32);
        guess.context.confidence_factors = Some(format!(
            "base={base:.3};anchor={anchor:.3};visual={visual:.3}"
        ));
    }
}


fn translate_input_diagnostics(diagnostics: &[String]) -> Vec<DiagnosticRecord> {
    diagnostics
        .iter()
        .map(|line| {
            let mut record = DiagnosticRecord::info("logic", "dictionary", "dictionary_input");
            record.message = Some(line.clone());
            if let Some(source) = line.strip_prefix("dictionary_source=") {
                record.code = "dictionary_source".to_owned();
                record.metrics.insert(
                    "source".to_owned(),
                    DiagnosticValue::Text(source.to_owned()),
                );
            } else if let Some(size) = line.strip_prefix("dictionary_size=") {
                record.code = "dictionary_size".to_owned();
                if let Ok(size_value) = size.parse::<i64>() {
                    record.metrics.insert(
                        "size".to_owned(),
                        DiagnosticValue::Integer(size_value),
                    );
                }
            }
            record
        })
        .collect()
}

fn timing_diagnostic(stage: &str, value_ms: u128) -> DiagnosticRecord {
    let mut record = DiagnosticRecord::info("logic", stage, "timing_ms");
    record.metrics.insert(
        "value_ms".to_owned(),
        DiagnosticValue::Integer(value_ms as i64),
    );
    record
}

fn translate_evidence_diagnostic(diagnostic: &RedactionEvidenceDiagnostic) -> DiagnosticRecord {
    let mut record = DiagnosticRecord::warning(
        "data",
        &diagnostic.stage,
        &diagnostic.reason_code,
        &diagnostic.message,
    );
    record.row_id = diagnostic.row_id.clone();
    record.redaction_id = diagnostic.redaction_id.clone();
    record.page_index = Some(diagnostic.page_index);
    record.bbox = Some(diagnostic.bbox);
    record.metrics = diagnostic.metrics.clone();
    record
}

fn translate_candidate_diagnostic(
    diagnostic: &crate::data::types::guess_candidate_types::GuessCandidateDiagnostic,
) -> DiagnosticRecord {
    let mut record = DiagnosticRecord::warning(
        "data",
        &diagnostic.stage,
        &diagnostic.reason_code,
        &diagnostic.message,
    );
    record.row_id = Some(diagnostic.row_id.clone());
    record.page_index = Some(diagnostic.page_index);
    record.bbox = Some(diagnostic.bbox);
    record.metrics = diagnostic.metrics.clone();
    record
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

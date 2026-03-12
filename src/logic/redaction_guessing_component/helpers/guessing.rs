use std::collections::BTreeMap;

use crate::data::guess_candidate_data::collect_guess_candidates;
use crate::data::redaction_evidence_data::collect_redaction_evidence;
use crate::data::types::guess_candidate_types::{
    CollectGuessCandidatesRequest, GuessCandidateRow, GuessCandidateSet, MeasuredCandidate,
};
use crate::data::types::redaction_evidence_types::{
    AnchorMode as EvidenceAnchorMode, AnchorSide, CollectRedactionEvidenceRequest,
    RedactionEvidenceDiagnostic,
};
use crate::types::diagnostic_types::{DiagnosticRecord, DiagnosticValue};
use crate::types::file_types::FontRunReport;
use crate::types::guess_types::{
    AnchorDecisionRecord, AnchorSideDecision, AnchorType, GuessCandidate, GuessConfig,
    GuessContext, GuessReport, RedactionGuess,
};
use crate::types::redaction_types::{RedactionOccurrence, RedactionReport};
use crate::types::time::Instant;

#[cfg(feature = "cli-entry")]
use crate::data::types::redaction_evidence_types::{RedactionEvidenceRow, RedactionEvidenceSet};
#[cfg(feature = "cli-entry")]
use crate::types::guess_types::AnchorReport;

pub struct RunGuessFromBytesRequest<'a> {
    pub pdf_name: &'a str,
    pub pdf_bytes: &'a [u8],
    pub redactions: &'a RedactionReport,
    pub dictionary: &'a [String],
    pub diagnostics: &'a [String],
    pub preloaded_font_runs: Option<&'a FontRunReport>,
    pub preloaded_font_runs_elapsed_ms: Option<u128>,
    pub cfg: &'a GuessConfig,
    pub collect_diagnostics: bool,
}

#[cfg(feature = "cli-entry")]
pub struct RunAnchorFromBytesRequest<'a> {
    pub pdf_name: &'a str,
    pub pdf_bytes: &'a [u8],
    pub redactions: &'a RedactionReport,
    pub preloaded_font_runs: Option<&'a FontRunReport>,
    pub preloaded_font_runs_elapsed_ms: Option<u128>,
}

pub struct RunGuessFromBytesOutput {
    pub report: GuessReport,
    pub diagnostics: Option<Vec<DiagnosticRecord>>,
}

#[inline]
pub fn run_from_bytes(req: RunGuessFromBytesRequest<'_>) -> Result<RunGuessFromBytesOutput, String> {
    let started = Instant::now();
    let _preloaded_font_runs = req.preloaded_font_runs;
    let _preloaded_font_runs_elapsed_ms = req.preloaded_font_runs_elapsed_ms;
    let _cfg = req.cfg;

    let mut diagnostics = req
        .collect_diagnostics
        .then(|| translate_input_diagnostics(req.diagnostics));

    let evidence_started = Instant::now();
    let evidence = collect_redaction_evidence(CollectRedactionEvidenceRequest {
        input_name: req.pdf_name,
        pdf_bytes: req.pdf_bytes,
        redactions: req.redactions,
        collect_diagnostics: req.collect_diagnostics,
    })?;
    if let Some(items) = diagnostics.as_mut() {
        items.push(timing_diagnostic(
            "guess_redaction_evidence",
            evidence_started.elapsed().as_millis(),
        ));
        items.extend(
            evidence
                .diagnostics
                .iter()
                .map(translate_evidence_diagnostic),
        );
    }

    let candidate_started = Instant::now();
    let candidate_set = collect_guess_candidates(CollectGuessCandidatesRequest {
        evidence: &evidence,
        dictionary: req.dictionary,
        collect_diagnostics: req.collect_diagnostics,
    })?;
    if let Some(items) = diagnostics.as_mut() {
        items.push(timing_diagnostic(
            "guess_candidate_data",
            candidate_started.elapsed().as_millis(),
        ));
        items.extend(
            candidate_set
                .diagnostics
                .iter()
                .map(translate_candidate_diagnostic),
        );
    }

    let build_started = Instant::now();
    let (guesses, anchors) = build_guess_outputs(req.redactions, &candidate_set);
    if let Some(items) = diagnostics.as_mut() {
        items.push(timing_diagnostic(
            "guess_build_report",
            build_started.elapsed().as_millis(),
        ));
        items.push(timing_diagnostic(
            "guess_run_from_bytes_total",
            started.elapsed().as_millis(),
        ));
    }

    Ok(RunGuessFromBytesOutput {
        report: GuessReport {
            input_redactions: format!("memory://{}.redactions.json", req.pdf_name),
            input_fonts: format!("memory://{}.fonts.json", req.pdf_name),
            guesses,
            anchors,
            stage_timings: Vec::new(),
        },
        diagnostics,
    })
}

#[cfg(feature = "cli-entry")]
#[inline]
pub fn run_anchor_from_bytes(req: RunAnchorFromBytesRequest<'_>) -> Result<AnchorReport, String> {
    let _preloaded_font_runs = req.preloaded_font_runs;
    let _preloaded_font_runs_elapsed_ms = req.preloaded_font_runs_elapsed_ms;
    let evidence = collect_redaction_evidence(CollectRedactionEvidenceRequest {
        input_name: req.pdf_name,
        pdf_bytes: req.pdf_bytes,
        redactions: req.redactions,
        collect_diagnostics: false,
    })?;
    let decisions = build_anchor_outputs(req.redactions, &evidence);
    Ok(AnchorReport {
        input_redactions: format!("memory://{}.redactions.json", req.pdf_name),
        decisions,
    })
}

fn build_guess_outputs(
    redactions: &RedactionReport,
    candidate_set: &GuessCandidateSet,
) -> (Vec<RedactionGuess>, Vec<AnchorDecisionRecord>) {
    let rows_by_redaction = candidate_set
        .rows
        .iter()
        .map(|row| (row.redaction.redaction_id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let mut guesses = Vec::with_capacity(redactions.redactions.len());
    let mut anchors = Vec::with_capacity(redactions.redactions.len());

    for (index, redaction) in redactions.redactions.iter().enumerate() {
        let redaction_id = format!("page{}_redaction{index:03}", redaction.page_index);
        if let Some(row) = rows_by_redaction.get(&redaction_id) {
            guesses.push(build_guess_for_row(row));
            anchors.push(build_anchor_record_from_row(row));
        } else {
            guesses.push(build_placeholder_guess(redaction));
            anchors.push(build_placeholder_anchor_record(redaction, index));
        }
    }

    (guesses, anchors)
}

#[cfg(feature = "cli-entry")]
fn build_anchor_outputs(
    redactions: &RedactionReport,
    evidence: &RedactionEvidenceSet,
) -> Vec<AnchorDecisionRecord> {
    let rows_by_redaction = evidence
        .rows
        .iter()
        .map(|row| (row.redaction.redaction_id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let mut anchors = Vec::with_capacity(redactions.redactions.len());
    for (index, redaction) in redactions.redactions.iter().enumerate() {
        let redaction_id = format!("page{}_redaction{index:03}", redaction.page_index);
        if let Some(row) = rows_by_redaction.get(&redaction_id) {
            anchors.push(build_anchor_record_from_evidence(row));
        } else {
            anchors.push(build_placeholder_anchor_record(redaction, index));
        }
    }
    anchors
}

fn build_guess_for_row(row: &GuessCandidateRow) -> RedactionGuess {
    let candidates = row
        .candidates
        .iter()
        .map(build_guess_candidate)
        .collect::<Vec<_>>();
    RedactionGuess {
        page_index: row.page_index,
        bbox: row.redaction.bbox,
        candidates,
        context: GuessContext {
            anchor_mode: Some(row.anchor_set.mode.as_str().to_owned()),
            usable_left_edge_x_pt: row.anchor_set.geometry.usable_left_edge_x_pt,
            usable_right_edge_x_pt: row.anchor_set.geometry.usable_right_edge_x_pt,
            target_width_pt: row.anchor_set.geometry.target_width_pt,
            font_name: Some(row.font.font_name.clone()),
            font_size_pt: Some(row.font.font_size_pt),
            h_scale_pct: Some(row.font.h_scale_pct),
            char_spacing_pt: Some(row.font.char_spacing_pt),
            word_spacing_pt: Some(row.font.word_spacing_pt),
        },
    }
}

fn build_guess_candidate(candidate: &MeasuredCandidate) -> GuessCandidate {
    GuessCandidate {
        text: candidate.text.clone(),
        width_pt: candidate.width_pt,
        glyph_width_sum_pt: candidate.glyph_width_sum_pt,
        char_spacing_total_pt: candidate.char_spacing_total_pt,
        word_spacing_total_pt: candidate.word_spacing_total_pt,
        predicted_right_edge_x_pt: candidate.predicted_right_edge_x_pt,
        actual_right_edge_x_pt: candidate.actual_right_edge_x_pt,
        target_width_pt: candidate.target_width_pt,
        error_pt: candidate.error_pt,
    }
}

fn build_placeholder_guess(redaction: &RedactionOccurrence) -> RedactionGuess {
    RedactionGuess {
        page_index: redaction.page_index,
        bbox: redaction.bbox,
        candidates: Vec::new(),
        context: GuessContext {
            anchor_mode: None,
            usable_left_edge_x_pt: None,
            usable_right_edge_x_pt: None,
            target_width_pt: redaction.bbox.width().abs(),
            font_name: None,
            font_size_pt: None,
            h_scale_pct: None,
            char_spacing_pt: None,
            word_spacing_pt: None,
        },
    }
}

fn build_anchor_record_from_row(row: &GuessCandidateRow) -> AnchorDecisionRecord {
    build_anchor_record(AnchorRecordInput {
        anchor_row_id: &row.row_id,
        page_index: row.page_index,
        bbox: row.redaction.bbox,
        anchor_mode: &row.anchor_set.mode,
        left: row.anchor_set.left.as_ref(),
        right: row.anchor_set.right.as_ref(),
        usable_left_edge_x_pt: row.anchor_set.geometry.usable_left_edge_x_pt,
        usable_right_edge_x_pt: row.anchor_set.geometry.usable_right_edge_x_pt,
        target_width_pt: row.anchor_set.geometry.target_width_pt,
        font_name: &row.font.font_name,
        font_size_pt: row.font.font_size_pt,
        h_scale_pct: row.font.h_scale_pct,
        char_spacing_pt: row.font.char_spacing_pt,
        word_spacing_pt: row.font.word_spacing_pt,
    })
}

#[cfg(feature = "cli-entry")]
fn build_anchor_record_from_evidence(row: &RedactionEvidenceRow) -> AnchorDecisionRecord {
    build_anchor_record(AnchorRecordInput {
        anchor_row_id: &row.row_id,
        page_index: row.page_index,
        bbox: row.redaction.bbox,
        anchor_mode: &row.anchor_set.mode,
        left: row.anchor_set.left.as_ref(),
        right: row.anchor_set.right.as_ref(),
        usable_left_edge_x_pt: row.anchor_set.geometry.usable_left_edge_x_pt,
        usable_right_edge_x_pt: row.anchor_set.geometry.usable_right_edge_x_pt,
        target_width_pt: row.anchor_set.geometry.target_width_pt,
        font_name: &row.font.font_name,
        font_size_pt: row.font.font_size_pt,
        h_scale_pct: row.font.h_scale_pct,
        char_spacing_pt: row.font.char_spacing_pt,
        word_spacing_pt: row.font.word_spacing_pt,
    })
}

struct AnchorRecordInput<'a> {
    anchor_row_id: &'a str,
    page_index: u32,
    bbox: crate::types::redaction_types::Rect,
    anchor_mode: &'a EvidenceAnchorMode,
    left: Option<&'a AnchorSide>,
    right: Option<&'a AnchorSide>,
    usable_left_edge_x_pt: Option<f32>,
    usable_right_edge_x_pt: Option<f32>,
    target_width_pt: f32,
    font_name: &'a str,
    font_size_pt: f32,
    h_scale_pct: f32,
    char_spacing_pt: f32,
    word_spacing_pt: f32,
}

fn build_anchor_record(input: AnchorRecordInput<'_>) -> AnchorDecisionRecord {
    AnchorDecisionRecord {
        anchor_row_id: input.anchor_row_id.to_owned(),
        page_index: input.page_index,
        bbox: input.bbox,
        anchor_mode: input.anchor_mode.as_str().to_owned(),
        left: input
            .left
            .map(|side| build_anchor_side(side, AnchorType::Left)),
        right: input
            .right
            .map(|side| build_anchor_side(side, AnchorType::Right)),
        usable_left_edge_x_pt: input.usable_left_edge_x_pt,
        usable_right_edge_x_pt: input.usable_right_edge_x_pt,
        target_width_pt: input.target_width_pt,
        font_name: input.font_name.to_owned(),
        font_size_pt: input.font_size_pt,
        h_scale_pct: input.h_scale_pct,
        char_spacing_pt: input.char_spacing_pt,
        word_spacing_pt: input.word_spacing_pt,
    }
}

fn build_anchor_side(side: &AnchorSide, anchor_type: AnchorType) -> AnchorSideDecision {
    AnchorSideDecision {
        anchor_id: side.anchor_id.clone(),
        anchor_type,
        text: side.text.clone(),
        bbox: side.bbox,
        x: side.text_edge_x_pt,
    }
}

fn build_placeholder_anchor_record(
    redaction: &RedactionOccurrence,
    index: usize,
) -> AnchorDecisionRecord {
    AnchorDecisionRecord {
        anchor_row_id: format!("page{}_row{index}", redaction.page_index),
        page_index: redaction.page_index,
        bbox: redaction.bbox,
        anchor_mode: "unresolved".to_owned(),
        left: None,
        right: None,
        usable_left_edge_x_pt: None,
        usable_right_edge_x_pt: None,
        target_width_pt: redaction.bbox.width().abs(),
        font_name: String::new(),
        font_size_pt: 0.0_f32,
        h_scale_pct: 0.0_f32,
        char_spacing_pt: 0.0_f32,
        word_spacing_pt: 0.0_f32,
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
                    record
                        .metrics
                        .insert("size".to_owned(), DiagnosticValue::Integer(size_value));
                }
            }
            record
        })
        .collect()
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

fn timing_diagnostic(stage: &str, value_ms: u128) -> DiagnosticRecord {
    let mut record = DiagnosticRecord::info("logic", stage, "timing_ms");
    record.metrics.insert(
        "value_ms".to_owned(),
        DiagnosticValue::Integer(value_ms as i64),
    );
    record
}

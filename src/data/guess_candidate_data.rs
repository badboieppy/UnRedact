use std::collections::{BTreeMap, BTreeSet};

use crate::data::dictionary_variant_data::build_dictionary_variants;
use crate::data::helpers::character_measurement::{
    measure_text_width, AmbiguousCodeChoice, MeasuredTextWidth, MeasurementFailure,
};
use crate::data::types::guess_candidate_types::{
    CollectGuessCandidatesRequest, GuessCandidateDiagnostic, GuessCandidateRow, GuessCandidateSet,
    MeasuredCandidate,
};
use crate::data::types::redaction_evidence_types::{
    AnchorMode, CandidateWidthModel, MeasurementFontKey,
};
use crate::types::diagnostic_types::DiagnosticValue;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedCandidateSupport {
    glyph_width_sum_units: i32,
    chosen_codes: Vec<u16>,
    ambiguous_choices: Vec<AmbiguousCodeChoice>,
    char_count: usize,
    space_count: u32,
}

type CandidateScore = (Option<f32>, Option<f32>, Option<f32>, f32);

pub fn collect_guess_candidates(
    req: CollectGuessCandidatesRequest<'_>,
) -> Result<GuessCandidateSet, String> {
    let variants = build_dictionary_variants(req.dictionary);
    let mut rows = Vec::<GuessCandidateRow>::with_capacity(req.evidence.rows.len());
    let mut diagnostics = Vec::<GuessCandidateDiagnostic>::new();
    let mut support_cache = BTreeMap::<
        MeasurementFontKey,
        BTreeMap<String, Result<CachedCandidateSupport, MeasurementFailure>>,
    >::new();

    for row in &req.evidence.rows {
        if row.anchor_set.mode == AnchorMode::Unresolved {
            rows.push(GuessCandidateRow {
                row_id: row.row_id.clone(),
                page_index: row.page_index,
                redaction: row.redaction.clone(),
                anchor_set: row.anchor_set.clone(),
                font: row.font.clone(),
                neighbor_facts: row.neighbor_facts.clone(),
                candidates: Vec::new(),
            });
            continue;
        }
        let row_cache = support_cache
            .entry(row.measurement_model.resource_key.clone())
            .or_default();
        let mut candidates = Vec::<MeasuredCandidate>::new();

        for variant in &variants {
            let trimmed = variant.trim();
            if trimmed.is_empty() {
                if req.collect_diagnostics {
                    diagnostics.push(build_candidate_diagnostic(
                        row,
                        "candidate_empty_after_trim",
                        "dictionary variant is empty after trim",
                        BTreeMap::new(),
                    ));
                }
                continue;
            }

            let cached_support = row_cache
                .entry(trimmed.to_owned())
                .or_insert_with(|| build_candidate_support(&row.measurement_model, trimmed))
                .clone();

            let support = match cached_support {
                Ok(support) => support,
                Err(error) => {
                    if req.collect_diagnostics {
                        diagnostics
                            .push(build_measurement_failure_diagnostic(row, trimmed, &error));
                    }
                    continue;
                }
            };

            if req.collect_diagnostics {
                diagnostics.extend(
                    support
                        .ambiguous_choices
                        .iter()
                        .map(|choice| build_ambiguous_code_diagnostic(row, trimmed, choice)),
                );
            }

            let measurement = project_cached_measurement(&row.measurement_model, &support);
            let Some((
                predicted_left_edge_x_pt,
                predicted_right_edge_x_pt,
                actual_right_edge_x_pt,
                error_pt,
            )) = score_candidate(row, &measurement)
            else {
                if req.collect_diagnostics {
                    diagnostics.push(build_candidate_diagnostic(
                        row,
                        "row_anchor_geometry_missing",
                        "row is missing required anchor geometry for scoring",
                        base_candidate_metrics(row, trimmed),
                    ));
                }
                continue;
            };
            let normalized_error = error_pt / row.anchor_set.geometry.tolerance_pt.max(1.0_f32);

            if req.collect_diagnostics {
                diagnostics.push(build_measured_diagnostic(
                    row,
                    trimmed,
                    &measurement,
                    predicted_left_edge_x_pt,
                    predicted_right_edge_x_pt,
                ));
            }

            if overlaps_neighbor(
                row,
                predicted_left_edge_x_pt,
                predicted_right_edge_x_pt,
                row.anchor_set.geometry.tolerance_pt,
            ) {
                if req.collect_diagnostics {
                    diagnostics.push(build_overlap_diagnostic(
                        row,
                        trimmed,
                        predicted_left_edge_x_pt,
                        predicted_right_edge_x_pt,
                    ));
                }
                continue;
            }

            candidates.push(MeasuredCandidate {
                text: trimmed.to_owned(),
                width_pt: measurement.width_pt,
                glyph_width_sum_pt: measurement.glyph_width_sum_pt,
                char_spacing_total_pt: measurement.char_spacing_total_pt,
                word_spacing_total_pt: measurement.word_spacing_total_pt,
                predicted_left_edge_x_pt,
                predicted_right_edge_x_pt,
                actual_right_edge_x_pt,
                target_width_pt: row.anchor_set.geometry.target_width_pt,
                error_pt,
                normalized_error,
            });
        }

        candidates.sort_by(|left, right| {
            left.normalized_error
                .partial_cmp(&right.normalized_error)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.error_pt
                        .partial_cmp(&right.error_pt)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    normalized_candidate_text(&left.text)
                        .cmp(&normalized_candidate_text(&right.text))
                })
        });
        candidates = dedupe_candidates_by_normalized_text(candidates);

        if req.collect_diagnostics {
            diagnostics.extend(
                candidates
                    .iter()
                    .enumerate()
                    .map(|(index, candidate)| build_ranked_diagnostic(row, candidate, index + 1)),
            );
        }

        rows.push(GuessCandidateRow {
            row_id: row.row_id.clone(),
            page_index: row.page_index,
            redaction: row.redaction.clone(),
            anchor_set: row.anchor_set.clone(),
            font: row.font.clone(),
            neighbor_facts: row.neighbor_facts.clone(),
            candidates,
        });
    }

    diagnostics.sort_by(|left, right| {
        left.page_index
            .cmp(&right.page_index)
            .then_with(|| left.row_id.cmp(&right.row_id))
            .then_with(|| diagnostic_candidate_text(left).cmp(&diagnostic_candidate_text(right)))
            .then_with(|| left.stage.cmp(&right.stage))
            .then_with(|| left.reason_code.cmp(&right.reason_code))
    });

    Ok(GuessCandidateSet {
        input: req.evidence.input.clone(),
        rows,
        diagnostics,
    })
}

fn build_candidate_support(
    model: &CandidateWidthModel,
    text: &str,
) -> Result<CachedCandidateSupport, MeasurementFailure> {
    let measurement = measure_text_width(model, text)?;
    Ok(CachedCandidateSupport {
        glyph_width_sum_units: measurement.glyph_width_sum_units,
        chosen_codes: measurement.chosen_codes,
        ambiguous_choices: measurement.ambiguous_choices,
        char_count: text.chars().count(),
        space_count: text.chars().filter(|ch| *ch == ' ').count() as u32,
    })
}

fn project_cached_measurement(
    model: &CandidateWidthModel,
    support: &CachedCandidateSupport,
) -> MeasuredTextWidth {
    let scale = (model.h_scale_pct / 100.0_f32).max(0.01_f32);
    let glyph_width_sum_pt =
        support.glyph_width_sum_units as f32 * (model.font_size_pt / 1000.0_f32) * scale;
    let char_spacing_total_pt = model.char_spacing_pt * support.char_count.saturating_sub(1) as f32;
    let word_spacing_total_pt = model.word_spacing_pt * support.space_count as f32;
    MeasuredTextWidth {
        glyph_width_sum_units: support.glyph_width_sum_units,
        glyph_width_sum_pt,
        char_spacing_total_pt,
        word_spacing_total_pt,
        width_pt: glyph_width_sum_pt + char_spacing_total_pt + word_spacing_total_pt,
        chosen_codes: support.chosen_codes.clone(),
        ambiguous_choices: support.ambiguous_choices.clone(),
    }
}

fn score_candidate(
    row: &crate::data::types::redaction_evidence_types::RedactionEvidenceRow,
    measurement: &MeasuredTextWidth,
) -> Option<CandidateScore> {
    let predicted_left_edge_x_pt = match row.anchor_set.mode {
        AnchorMode::TwoSided | AnchorMode::LeftOnly => {
            row.anchor_set.geometry.usable_left_edge_x_pt
        }
        AnchorMode::RightOnly => row
            .anchor_set
            .geometry
            .usable_right_edge_x_pt
            .map(|usable_right_edge_x_pt| usable_right_edge_x_pt - measurement.width_pt),
        AnchorMode::Unresolved => None,
    };
    let predicted_right_edge_x_pt = match row.anchor_set.mode {
        AnchorMode::TwoSided | AnchorMode::LeftOnly => predicted_left_edge_x_pt
            .map(|predicted_left_edge_x_pt| predicted_left_edge_x_pt + measurement.width_pt),
        AnchorMode::RightOnly => row.anchor_set.geometry.usable_right_edge_x_pt,
        AnchorMode::Unresolved => None,
    };
    let actual_right_edge_x_pt = row.anchor_set.geometry.usable_right_edge_x_pt;
    let error_pt = match row.anchor_set.mode {
        AnchorMode::TwoSided => {
            let predicted_right_edge_x_pt = predicted_right_edge_x_pt?;
            let actual_right_edge_x_pt = actual_right_edge_x_pt?;
            (predicted_right_edge_x_pt - actual_right_edge_x_pt).abs()
        }
        AnchorMode::LeftOnly => {
            let predicted_right_edge_x_pt = predicted_right_edge_x_pt?;
            (predicted_right_edge_x_pt - row.anchor_set.geometry.redaction_right_x_pt).abs()
        }
        AnchorMode::RightOnly => {
            let predicted_left_edge_x_pt = predicted_left_edge_x_pt?;
            (predicted_left_edge_x_pt - row.anchor_set.geometry.redaction_left_x_pt).abs()
        }
        AnchorMode::Unresolved => return None,
    };

    Some((
        predicted_left_edge_x_pt,
        predicted_right_edge_x_pt,
        actual_right_edge_x_pt,
        error_pt,
    ))
}

fn overlaps_neighbor(
    row: &crate::data::types::redaction_evidence_types::RedactionEvidenceRow,
    predicted_left_edge_x_pt: Option<f32>,
    predicted_right_edge_x_pt: Option<f32>,
    tolerance_pt: f32,
) -> bool {
    let Some(predicted_left_edge_x_pt) = predicted_left_edge_x_pt else {
        return false;
    };
    let Some(predicted_right_edge_x_pt) = predicted_right_edge_x_pt else {
        return false;
    };
    if let Some(previous) = &row.neighbor_facts.previous_same_line {
        let overlap_pt = previous.bbox.x1 - predicted_left_edge_x_pt;
        if overlap_pt > tolerance_pt {
            return true;
        }
    }
    if let Some(next) = &row.neighbor_facts.next_same_line {
        let overlap_pt = predicted_right_edge_x_pt - next.bbox.x0;
        if overlap_pt > tolerance_pt {
            return true;
        }
    }
    false
}

fn build_candidate_diagnostic(
    row: &crate::data::types::redaction_evidence_types::RedactionEvidenceRow,
    reason_code: &str,
    message: &str,
    metrics: BTreeMap<String, DiagnosticValue>,
) -> GuessCandidateDiagnostic {
    GuessCandidateDiagnostic {
        row_id: row.row_id.clone(),
        page_index: row.page_index,
        bbox: row.redaction.bbox,
        stage: "guess_candidate_data".to_owned(),
        reason_code: reason_code.to_owned(),
        message: message.to_owned(),
        metrics,
    }
}

fn build_measurement_failure_diagnostic(
    row: &crate::data::types::redaction_evidence_types::RedactionEvidenceRow,
    candidate_text: &str,
    error: &MeasurementFailure,
) -> GuessCandidateDiagnostic {
    let mut metrics = base_candidate_metrics(row, candidate_text);
    match error {
        MeasurementFailure::UnicodeCharUnmapped { char_index, ch } => {
            metrics.insert(
                "candidate_char".to_owned(),
                DiagnosticValue::Text(ch.to_string()),
            );
            metrics.insert(
                "candidate_char_index".to_owned(),
                DiagnosticValue::Integer(*char_index as i64),
            );
            metrics.insert(
                "candidate_char_codepoint".to_owned(),
                DiagnosticValue::Integer(u32::from(*ch) as i64),
            );
        }
        MeasurementFailure::WidthEntryMissing {
            char_index,
            ch,
            available_codes,
        } => {
            metrics.insert(
                "candidate_char".to_owned(),
                DiagnosticValue::Text(ch.to_string()),
            );
            metrics.insert(
                "candidate_char_index".to_owned(),
                DiagnosticValue::Integer(*char_index as i64),
            );
            metrics.insert(
                "candidate_char_codepoint".to_owned(),
                DiagnosticValue::Integer(u32::from(*ch) as i64),
            );
            metrics.insert(
                "available_codes".to_owned(),
                DiagnosticValue::Text(
                    available_codes
                        .iter()
                        .map(|code| code.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            );
        }
        MeasurementFailure::RowUnicodeBackendMissing
        | MeasurementFailure::RowWidthBackendMissing => {}
    }

    build_candidate_diagnostic(
        row,
        error.reason_code(),
        &error.message(&row.font.font_name),
        metrics,
    )
}

fn build_ambiguous_code_diagnostic(
    row: &crate::data::types::redaction_evidence_types::RedactionEvidenceRow,
    candidate_text: &str,
    choice: &AmbiguousCodeChoice,
) -> GuessCandidateDiagnostic {
    let mut metrics = base_candidate_metrics(row, candidate_text);
    metrics.insert(
        "candidate_char".to_owned(),
        DiagnosticValue::Text(choice.ch.to_string()),
    );
    metrics.insert(
        "candidate_char_index".to_owned(),
        DiagnosticValue::Integer(choice.char_index as i64),
    );
    metrics.insert(
        "candidate_char_codepoint".to_owned(),
        DiagnosticValue::Integer(u32::from(choice.ch) as i64),
    );
    metrics.insert(
        "available_codes".to_owned(),
        DiagnosticValue::Text(
            choice
                .available_codes
                .iter()
                .map(|code| code.to_string())
                .collect::<Vec<_>>()
                .join(","),
        ),
    );
    metrics.insert(
        "chosen_code_if_any".to_owned(),
        DiagnosticValue::Integer(choice.chosen_code as i64),
    );
    build_candidate_diagnostic(
        row,
        "candidate_code_choice_ambiguous",
        "candidate character maps to multiple usable font codes",
        metrics,
    )
}

fn build_measured_diagnostic(
    row: &crate::data::types::redaction_evidence_types::RedactionEvidenceRow,
    candidate_text: &str,
    measurement: &MeasuredTextWidth,
    predicted_left_edge_x_pt: Option<f32>,
    predicted_right_edge_x_pt: Option<f32>,
) -> GuessCandidateDiagnostic {
    let mut metrics = base_candidate_metrics(row, candidate_text);
    metrics.insert(
        "glyph_width_sum_pt".to_owned(),
        DiagnosticValue::Float(measurement.glyph_width_sum_pt as f64),
    );
    metrics.insert(
        "char_spacing_total_pt".to_owned(),
        DiagnosticValue::Float(measurement.char_spacing_total_pt as f64),
    );
    metrics.insert(
        "word_spacing_total_pt".to_owned(),
        DiagnosticValue::Float(measurement.word_spacing_total_pt as f64),
    );
    metrics.insert(
        "width_pt".to_owned(),
        DiagnosticValue::Float(measurement.width_pt as f64),
    );
    if let Some(predicted_left_edge_x_pt) = predicted_left_edge_x_pt {
        metrics.insert(
            "predicted_left_edge_x_pt".to_owned(),
            DiagnosticValue::Float(predicted_left_edge_x_pt as f64),
        );
    }
    if let Some(predicted_right_edge_x_pt) = predicted_right_edge_x_pt {
        metrics.insert(
            "predicted_right_edge_x_pt".to_owned(),
            DiagnosticValue::Float(predicted_right_edge_x_pt as f64),
        );
    }
    build_candidate_diagnostic(
        row,
        "candidate_measured",
        "candidate measured against row geometry",
        metrics,
    )
}

fn build_overlap_diagnostic(
    row: &crate::data::types::redaction_evidence_types::RedactionEvidenceRow,
    candidate_text: &str,
    predicted_left_edge_x_pt: Option<f32>,
    predicted_right_edge_x_pt: Option<f32>,
) -> GuessCandidateDiagnostic {
    let mut metrics = base_candidate_metrics(row, candidate_text);
    if let Some(predicted_left_edge_x_pt) = predicted_left_edge_x_pt {
        metrics.insert(
            "predicted_left_edge_x_pt".to_owned(),
            DiagnosticValue::Float(predicted_left_edge_x_pt as f64),
        );
    }
    if let Some(predicted_right_edge_x_pt) = predicted_right_edge_x_pt {
        metrics.insert(
            "predicted_right_edge_x_pt".to_owned(),
            DiagnosticValue::Float(predicted_right_edge_x_pt as f64),
        );
    }
    build_candidate_diagnostic(
        row,
        "candidate_neighbor_overlap_rejected",
        "candidate span overlaps an adjacent same-line redaction beyond tolerance",
        metrics,
    )
}

fn build_ranked_diagnostic(
    row: &crate::data::types::redaction_evidence_types::RedactionEvidenceRow,
    candidate: &MeasuredCandidate,
    rank: usize,
) -> GuessCandidateDiagnostic {
    let mut metrics = base_candidate_metrics(row, &candidate.text);
    metrics.insert(
        "raw_error_pt".to_owned(),
        DiagnosticValue::Float(candidate.error_pt as f64),
    );
    metrics.insert(
        "normalized_error".to_owned(),
        DiagnosticValue::Float(candidate.normalized_error as f64),
    );
    metrics.insert("rank".to_owned(), DiagnosticValue::Integer(rank as i64));
    metrics.insert(
        "anchor_mode".to_owned(),
        DiagnosticValue::Text(row.anchor_set.mode.as_str().to_owned()),
    );
    metrics.insert("kept".to_owned(), DiagnosticValue::Bool(true));
    build_candidate_diagnostic(row, "candidate_ranked", "candidate ranked for row", metrics)
}

fn base_candidate_metrics(
    row: &crate::data::types::redaction_evidence_types::RedactionEvidenceRow,
    candidate_text: &str,
) -> BTreeMap<String, DiagnosticValue> {
    let mut metrics = BTreeMap::new();
    metrics.insert(
        "candidate_text".to_owned(),
        DiagnosticValue::Text(candidate_text.to_owned()),
    );
    metrics.insert(
        "font_key".to_owned(),
        DiagnosticValue::Text(row.font.font_key.clone()),
    );
    metrics.insert(
        "anchor_mode".to_owned(),
        DiagnosticValue::Text(row.anchor_set.mode.as_str().to_owned()),
    );
    metrics.insert(
        "tolerance_pt".to_owned(),
        DiagnosticValue::Float(row.anchor_set.geometry.tolerance_pt as f64),
    );
    metrics
}

fn diagnostic_candidate_text(diagnostic: &GuessCandidateDiagnostic) -> String {
    diagnostic
        .metrics
        .get("candidate_text")
        .and_then(|value| match value {
            DiagnosticValue::Text(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn normalized_candidate_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_uppercase()
}

fn dedupe_candidates_by_normalized_text(
    candidates: Vec<MeasuredCandidate>,
) -> Vec<MeasuredCandidate> {
    let mut seen = BTreeSet::<String>::new();
    let mut out = Vec::<MeasuredCandidate>::with_capacity(candidates.len());
    for candidate in candidates {
        let key = normalized_candidate_text(&candidate.text);
        if seen.insert(key) {
            out.push(candidate);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        collect_guess_candidates, dedupe_candidates_by_normalized_text, normalized_candidate_text,
    };
    use crate::data::types::guess_candidate_types::{
        CollectGuessCandidatesRequest, MeasuredCandidate,
    };
    use crate::data::types::redaction_evidence_types::{
        AnchorMode, AnchorSet, CandidateWidthModel, GuessGeometry, MeasurementEncodingSource,
        MeasurementFont, MeasurementFontKey, MeasurementWidthSource, NeighborFacts,
        RedactionEvidenceRow, RedactionEvidenceSet, TrustedRedaction,
    };
    use crate::types::redaction_types::{Rect, RedactionKind};
    use std::collections::BTreeMap;

    fn sample_row(anchor_mode: AnchorMode) -> RedactionEvidenceRow {
        let mut unicode_to_codes = BTreeMap::new();
        unicode_to_codes.insert('A', vec![65]);
        unicode_to_codes.insert('B', vec![66]);
        unicode_to_codes.insert(' ', vec![32]);
        let mut code_to_width_units = BTreeMap::new();
        code_to_width_units.insert(65, 300_i32);
        code_to_width_units.insert(66, 500_i32);
        code_to_width_units.insert(32, 100_i32);
        RedactionEvidenceRow {
            row_id: "row0".to_owned(),
            page_index: 0,
            redaction: TrustedRedaction {
                redaction_id: "redaction0".to_owned(),
                page_index: 0,
                bbox: Rect::new(10.0, 10.0, 20.0, 20.0),
                kind: RedactionKind::DrawnRect,
                score: 1.0,
            },
            anchor_set: AnchorSet {
                mode: anchor_mode,
                left: None,
                right: None,
                measurement_seed_side: None,
                selected_line_id: None,
                selection_reason: None,
                selected_left_gap_pt: None,
                selected_right_gap_pt: None,
                geometry: GuessGeometry {
                    redaction_left_x_pt: 10.0,
                    redaction_right_x_pt: 20.0,
                    redaction_width_pt: 10.0,
                    usable_left_edge_x_pt: Some(10.0),
                    usable_right_edge_x_pt: Some(18.0),
                    target_width_pt: 8.0,
                    line_bias_pt: 0.0,
                    tolerance_pt: 2.0,
                },
            },
            font: MeasurementFont {
                font_key: "F1".to_owned(),
                font_name: "Times-Roman".to_owned(),
                base_font: Some("Times-Roman".to_owned()),
                font_size_pt: 10.0,
                h_scale_pct: 100.0,
                char_spacing_pt: 0.0,
                word_spacing_pt: 0.0,
                width_source: Some("pdf_width_table".to_owned()),
                encoding_source: Some("named_encoding".to_owned()),
            },
            neighbor_facts: NeighborFacts::default(),
            measurement_model: CandidateWidthModel {
                resource_key: MeasurementFontKey {
                    page_index: 0,
                    font_key: "F1".to_owned(),
                },
                font_key: "F1".to_owned(),
                font_name: "Times-Roman".to_owned(),
                base_font: Some("Times-Roman".to_owned()),
                subtype: Some("Type1".to_owned()),
                font_size_pt: 10.0,
                h_scale_pct: 100.0,
                char_spacing_pt: 0.0,
                word_spacing_pt: 0.0,
                width_source: MeasurementWidthSource::PdfWidthTable,
                encoding_source: MeasurementEncodingSource::NamedEncoding,
                has_to_unicode: false,
                has_encoding_dictionary: false,
                has_named_encoding: true,
                has_explicit_widths: true,
                unicode_to_codes,
                code_to_width_units,
            },
        }
    }

    #[test]
    fn collect_guess_candidates_keeps_supported_candidates_and_sorts_by_error() {
        let evidence = RedactionEvidenceSet {
            input: "memory://test".to_owned(),
            rows: vec![sample_row(AnchorMode::LeftOnly)],
            diagnostics: Vec::new(),
        };
        let out = collect_guess_candidates(CollectGuessCandidatesRequest {
            evidence: &evidence,
            dictionary: &["AA".to_owned(), "AB".to_owned()],
            collect_diagnostics: false,
        })
        .expect("candidate collection should succeed");
        assert_eq!(out.rows.len(), 1);
        assert_eq!(
            normalized_candidate_text(&out.rows[0].candidates[0].text),
            "AB"
        );
        assert!(
            out.rows[0].candidates[0].normalized_error
                <= out.rows[0].candidates[1].normalized_error
        );
    }

    #[test]
    fn collect_guess_candidates_uses_two_sided_right_edge_error() {
        let evidence = RedactionEvidenceSet {
            input: "memory://test".to_owned(),
            rows: vec![sample_row(AnchorMode::TwoSided)],
            diagnostics: Vec::new(),
        };
        let out = collect_guess_candidates(CollectGuessCandidatesRequest {
            evidence: &evidence,
            dictionary: &["AA".to_owned()],
            collect_diagnostics: false,
        })
        .expect("candidate collection should succeed");
        let candidate = &out.rows[0].candidates[0];
        assert_eq!(candidate.predicted_left_edge_x_pt, Some(10.0));
        assert_eq!(candidate.predicted_right_edge_x_pt, Some(16.0));
        assert_eq!(candidate.actual_right_edge_x_pt, Some(18.0));
        assert!((candidate.error_pt - 2.0_f32).abs() < 0.0001_f32);
    }

    #[test]
    fn collect_guess_candidates_uses_right_only_projected_left_edge() {
        let mut row = sample_row(AnchorMode::RightOnly);
        row.anchor_set.geometry.usable_left_edge_x_pt = None;
        let evidence = RedactionEvidenceSet {
            input: "memory://test".to_owned(),
            rows: vec![row],
            diagnostics: Vec::new(),
        };
        let out = collect_guess_candidates(CollectGuessCandidatesRequest {
            evidence: &evidence,
            dictionary: &["AB".to_owned()],
            collect_diagnostics: false,
        })
        .expect("candidate collection should succeed");
        let candidate = &out.rows[0].candidates[0];
        assert_eq!(candidate.predicted_left_edge_x_pt, Some(10.0));
        assert_eq!(candidate.predicted_right_edge_x_pt, Some(18.0));
        assert!((candidate.error_pt - 0.0_f32).abs() < 0.0001_f32);
    }

    #[test]
    fn collect_guess_candidates_breaks_equal_error_ties_by_normalized_text() {
        let evidence = RedactionEvidenceSet {
            input: "memory://test".to_owned(),
            rows: vec![sample_row(AnchorMode::LeftOnly)],
            diagnostics: Vec::new(),
        };
        let out = collect_guess_candidates(CollectGuessCandidatesRequest {
            evidence: &evidence,
            dictionary: &["BA".to_owned(), "AB".to_owned()],
            collect_diagnostics: false,
        })
        .expect("candidate collection should succeed");
        let ordered = out.rows[0]
            .candidates
            .iter()
            .map(|candidate| normalized_candidate_text(&candidate.text))
            .collect::<Vec<_>>();
        assert_eq!(ordered[0], "AB");
        assert_eq!(ordered[1], "BA");
    }

    #[test]
    fn dedupe_candidates_by_normalized_text_keeps_best_scoring_variant() {
        let deduped = dedupe_candidates_by_normalized_text(vec![
            MeasuredCandidate {
                text: "Alpha".to_owned(),
                width_pt: 1.0,
                glyph_width_sum_pt: 1.0,
                char_spacing_total_pt: 0.0,
                word_spacing_total_pt: 0.0,
                predicted_left_edge_x_pt: Some(0.0),
                predicted_right_edge_x_pt: Some(1.0),
                actual_right_edge_x_pt: Some(1.0),
                target_width_pt: 1.0,
                error_pt: 0.1,
                normalized_error: 0.1,
            },
            MeasuredCandidate {
                text: "ALPHA".to_owned(),
                width_pt: 1.2,
                glyph_width_sum_pt: 1.2,
                char_spacing_total_pt: 0.0,
                word_spacing_total_pt: 0.0,
                predicted_left_edge_x_pt: Some(0.0),
                predicted_right_edge_x_pt: Some(1.2),
                actual_right_edge_x_pt: Some(1.0),
                target_width_pt: 1.0,
                error_pt: 0.2,
                normalized_error: 0.2,
            },
            MeasuredCandidate {
                text: "Bravo".to_owned(),
                width_pt: 2.0,
                glyph_width_sum_pt: 2.0,
                char_spacing_total_pt: 0.0,
                word_spacing_total_pt: 0.0,
                predicted_left_edge_x_pt: Some(0.0),
                predicted_right_edge_x_pt: Some(2.0),
                actual_right_edge_x_pt: Some(2.0),
                target_width_pt: 2.0,
                error_pt: 0.3,
                normalized_error: 0.3,
            },
        ]);

        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].text, "Alpha");
        assert_eq!(deduped[1].text, "Bravo");
    }
}

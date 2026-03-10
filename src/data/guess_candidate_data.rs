use crate::data::dictionary_variant_data::build_dictionary_variants;
use crate::data::helpers::character_measurement::{measure_text_width, supports_text};
use crate::data::types::guess_candidate_types::{
    CollectGuessCandidatesRequest, GuessCandidateDiagnostic, GuessCandidateRow, GuessCandidateSet,
    MeasuredCandidate,
};
use crate::data::types::redaction_evidence_types::AnchorMode;
use crate::types::diagnostic_types::DiagnosticValue;
use std::collections::BTreeMap;

pub fn collect_guess_candidates(
    req: CollectGuessCandidatesRequest<'_>,
) -> Result<GuessCandidateSet, String> {
    let variants = build_dictionary_variants(req.dictionary);
    let mut rows = Vec::<GuessCandidateRow>::with_capacity(req.evidence.rows.len());
    let mut diagnostics = Vec::<GuessCandidateDiagnostic>::new();

    for row in &req.evidence.rows {
        let mut candidates = Vec::<MeasuredCandidate>::new();
        for variant in &variants {
            let trimmed = variant.trim();
            if trimmed.is_empty() {
                diagnostics.push(GuessCandidateDiagnostic {
                    row_id: row.row_id.clone(),
                    page_index: row.page_index,
                    bbox: row.redaction.bbox,
                    stage: "guess_candidate_data".to_owned(),
                    reason_code: "candidate_empty_after_trim".to_owned(),
                    message: "dictionary variant is empty after trim".to_owned(),
                    metrics: BTreeMap::new(),
                });
                continue;
            }
            if !supports_text(&row.measurement_model, trimmed) {
                let mut metrics = BTreeMap::new();
                metrics.insert(
                    "candidate_text".to_owned(),
                    DiagnosticValue::Text(trimmed.to_owned()),
                );
                diagnostics.push(GuessCandidateDiagnostic {
                    row_id: row.row_id.clone(),
                    page_index: row.page_index,
                    bbox: row.redaction.bbox,
                    stage: "guess_candidate_data".to_owned(),
                    reason_code: "unsupported_candidate_character".to_owned(),
                    message: format!(
                        "candidate contains character outside the trusted character model: {trimmed}"
                    ),
                    metrics,
                });
                continue;
            }
            let measurement = measure_text_width(&row.measurement_model, trimmed)?;
            let predicted_right_edge_x_pt = row
                .anchor_set
                .geometry
                .usable_left_edge_x_pt
                .map(|left_edge| left_edge + measurement.width_pt);
            let actual_right_edge_x_pt = row.anchor_set.geometry.usable_right_edge_x_pt;
            let target_width_pt = row.anchor_set.geometry.target_width_pt;
            let error_pt = match row.anchor_set.mode {
                AnchorMode::TwoSided => {
                    let Some(predicted_right_edge_x_pt) = predicted_right_edge_x_pt else {
                        return Err(format!(
                            "missing usable left edge for two-sided row {}",
                            row.row_id
                        ));
                    };
                    let Some(actual_right_edge_x_pt) = actual_right_edge_x_pt else {
                        return Err(format!(
                            "missing usable right edge for two-sided row {}",
                            row.row_id
                        ));
                    };
                    (predicted_right_edge_x_pt - actual_right_edge_x_pt).abs()
                }
                AnchorMode::LeftOnly | AnchorMode::RightOnly => {
                    (measurement.width_pt - target_width_pt).abs()
                }
            };
            candidates.push(MeasuredCandidate {
                text: trimmed.to_owned(),
                width_pt: measurement.width_pt,
                glyph_width_sum_pt: measurement.glyph_width_sum_pt,
                char_spacing_total_pt: measurement.char_spacing_total_pt,
                word_spacing_total_pt: measurement.word_spacing_total_pt,
                predicted_right_edge_x_pt,
                actual_right_edge_x_pt,
                target_width_pt,
                error_pt,
            });
        }
        candidates.sort_by(|left, right| {
            left.error_pt
                .partial_cmp(&right.error_pt)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    normalized_candidate_text(&left.text)
                        .cmp(&normalized_candidate_text(&right.text))
                })
        });
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

    Ok(GuessCandidateSet {
        input: req.evidence.input.clone(),
        rows,
        diagnostics,
    })
}

fn normalized_candidate_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::{collect_guess_candidates, normalized_candidate_text};
    use crate::data::types::guess_candidate_types::CollectGuessCandidatesRequest;
    use crate::data::types::redaction_evidence_types::{
        AnchorMode, AnchorSet, CandidateWidthModel, GuessGeometry, MeasurementFont, NeighborFacts,
        RedactionEvidenceRow, RedactionEvidenceSet, TrustedRedaction,
    };
    use crate::types::redaction_types::{Rect, RedactionKind};
    use std::collections::BTreeMap;

    fn sample_row(target_width_pt: f32) -> RedactionEvidenceRow {
        let mut advances_pt = BTreeMap::new();
        for ch in ['A', 'B', 'E', 'H', 'L', 'P', 'T', 'a', 'b', 'e', 'h', 'l', 'p', 't'] {
            advances_pt.insert(ch, 3.0_f32);
        }
        advances_pt.insert(' ', 1.0_f32);
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
                mode: AnchorMode::LeftOnly,
                left: None,
                right: None,
                geometry: GuessGeometry {
                    redaction_left_x_pt: 10.0,
                    redaction_right_x_pt: 20.0,
                    redaction_width_pt: 10.0,
                    usable_left_edge_x_pt: Some(10.0),
                    usable_right_edge_x_pt: None,
                    target_width_pt,
                    line_bias_pt: 0.0,
                    tolerance_pt: 2.0,
                },
            },
            font: MeasurementFont {
                font_name: "Times-Roman".to_owned(),
                font_size_pt: 12.0,
                h_scale_pct: 100.0,
                char_spacing_pt: 0.0,
                word_spacing_pt: 0.0,
            },
            neighbor_facts: NeighborFacts::default(),
            measurement_model: CandidateWidthModel {
                font_name: "Times-Roman".to_owned(),
                font_size_pt: 12.0,
                h_scale_pct: 100.0,
                char_spacing_pt: 0.0,
                word_spacing_pt: 0.0,
                base_advances_pt: advances_pt,
            },
        }
    }

    #[test]
    fn collect_guess_candidates_keeps_supported_candidates_and_sorts_by_error() {
        let evidence = RedactionEvidenceSet {
            input: "memory://test".to_owned(),
            rows: vec![sample_row(10.0)],
            diagnostics: Vec::new(),
        };
        let out = collect_guess_candidates(CollectGuessCandidatesRequest {
            evidence: &evidence,
            dictionary: &["AAA".to_owned(), "AAAA".to_owned()],
        })
        .expect("candidate collection should succeed");
        assert_eq!(out.rows.len(), 1);
        assert!(
            out.rows[0].candidates.len() >= 2,
            "expected expanded variants for supported candidates"
        );
        assert_eq!(
            normalized_candidate_text(&out.rows[0].candidates[0].text),
            "AAA"
        );
        assert!(
            out.rows[0].candidates[0].error_pt <= out.rows[0].candidates[1].error_pt,
            "candidates should be sorted by ascending error"
        );
    }

    #[test]
    fn collect_guess_candidates_uses_two_sided_error_from_right_edge_delta() {
        let mut row = sample_row(10.0);
        row.anchor_set.mode = AnchorMode::TwoSided;
        row.anchor_set.geometry.usable_left_edge_x_pt = Some(10.0);
        row.anchor_set.geometry.usable_right_edge_x_pt = Some(16.0);
        let evidence = RedactionEvidenceSet {
            input: "memory://test".to_owned(),
            rows: vec![row],
            diagnostics: Vec::new(),
        };

        let out = collect_guess_candidates(CollectGuessCandidatesRequest {
            evidence: &evidence,
            dictionary: &["AA".to_owned()],
        })
        .expect("candidate collection should succeed");

        let candidate = &out.rows[0].candidates[0];
        assert_eq!(candidate.predicted_right_edge_x_pt, Some(16.0));
        assert_eq!(candidate.actual_right_edge_x_pt, Some(16.0));
        assert!((candidate.error_pt - 0.0_f32).abs() < 0.0001_f32);
    }

    #[test]
    fn collect_guess_candidates_uses_one_sided_absolute_width_delta() {
        let evidence = RedactionEvidenceSet {
            input: "memory://test".to_owned(),
            rows: vec![sample_row(5.5)],
            diagnostics: Vec::new(),
        };

        let out = collect_guess_candidates(CollectGuessCandidatesRequest {
            evidence: &evidence,
            dictionary: &["AA".to_owned()],
        })
        .expect("candidate collection should succeed");

        let candidate = &out.rows[0].candidates[0];
        assert!((candidate.width_pt - 6.0_f32).abs() < 0.0001_f32);
        assert!((candidate.error_pt - 0.5_f32).abs() < 0.0001_f32);
    }

    #[test]
    fn collect_guess_candidates_breaks_equal_error_ties_by_normalized_text() {
        let evidence = RedactionEvidenceSet {
            input: "memory://test".to_owned(),
            rows: vec![sample_row(6.0)],
            diagnostics: Vec::new(),
        };

        let out = collect_guess_candidates(CollectGuessCandidatesRequest {
            evidence: &evidence,
            dictionary: &[
                "beta".to_owned(),
                "Alph".to_owned(),
                "alph  ".to_owned(),
                "BETA".to_owned(),
            ],
        })
        .expect("candidate collection should succeed");

        let ordered = out.rows[0]
            .candidates
            .iter()
            .map(|candidate| normalized_candidate_text(&candidate.text))
            .collect::<Vec<_>>();
        assert_eq!(
            ordered,
            vec!["ALPH", "ALPH", "ALPH", "BETA", "BETA", "BETA"]
        );
    }
}

use crate::data::dictionary_variant_data::build_dictionary_variants;
use crate::data::helpers::character_measurement::{measure_text, supports_text};
use crate::data::types::guess_candidate_types::{
    CollectGuessCandidatesRequest, GuessCandidateDiagnostic, GuessCandidateRow, GuessCandidateSet,
    MeasuredCandidate,
};
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
            let width_pt = measure_text(&row.measurement_model, trimmed)?;
            if width_pt > row.anchor_set.geometry.target_guess_width_pt {
                let mut metrics = BTreeMap::new();
                metrics.insert(
                    "candidate_width_pt".to_owned(),
                    DiagnosticValue::Float(width_pt as f64),
                );
                metrics.insert(
                    "target_guess_width_pt".to_owned(),
                    DiagnosticValue::Float(row.anchor_set.geometry.target_guess_width_pt as f64),
                );
                diagnostics.push(GuessCandidateDiagnostic {
                    row_id: row.row_id.clone(),
                    page_index: row.page_index,
                    bbox: row.redaction.bbox,
                    stage: "guess_candidate_data".to_owned(),
                    reason_code: "candidate_overflows_geometry".to_owned(),
                    message: format!(
                        "candidate width {:.4}pt exceeds target width {:.4}pt: {trimmed}",
                        width_pt, row.anchor_set.geometry.target_guess_width_pt
                    ),
                    metrics,
                });
                continue;
            }
            candidates.push(MeasuredCandidate {
                text: trimmed.to_owned(),
                width_pt,
                word_count: trimmed.split_whitespace().count() as u32,
                char_count: trimmed.chars().count() as u32,
            });
        }
        candidates.sort_by(|left, right| {
            left.width_pt
                .partial_cmp(&right.width_pt)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.text.cmp(&right.text))
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

#[cfg(test)]
mod tests {
    use super::collect_guess_candidates;
    use crate::data::types::guess_candidate_types::CollectGuessCandidatesRequest;
    use crate::data::types::redaction_evidence_types::{
        AnchorMode, AnchorSet, CandidateWidthModel, GuessGeometry, MeasurementFont,
        NeighborFacts, RedactionEvidenceRow, RedactionEvidenceSet, TrustedRedaction,
    };
    use crate::types::redaction_types::{Rect, RedactionKind};
    use std::collections::BTreeMap;

    fn sample_row(target_width_pt: f32) -> RedactionEvidenceRow {
        let mut advances_pt = BTreeMap::new();
        advances_pt.insert('A', 3.0_f32);
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
                    target_guess_width_pt: target_width_pt,
                    line_bias_pt: 0.0,
                    tolerance_pt: 2.0,
                },
            },
            font: MeasurementFont {
                font_name: "Times-Roman".to_owned(),
                font_size_pt: 12.0,
                h_scale_pct: 100.0,
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
    fn collect_guess_candidates_rejects_overflowing_candidates() {
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
        assert_eq!(out.rows[0].candidates.len(), 1);
        assert_eq!(out.rows[0].candidates[0].text, "AAA");
        assert!(
            out.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.reason_code == "candidate_overflows_geometry")
        );
    }
}

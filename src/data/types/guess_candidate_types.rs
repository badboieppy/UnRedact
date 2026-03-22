use std::collections::BTreeMap;

use crate::types::diagnostic_types::DiagnosticValue;
use serde::{Deserialize, Serialize};

use super::redaction_evidence_types::{
    AnchorSet, MeasurementFont, NeighborFacts, RedactionEvidenceSet, TrustedRedaction,
};

#[derive(Debug, Clone, PartialEq)]
pub struct CollectGuessCandidatesRequest<'a> {
    pub evidence: &'a RedactionEvidenceSet,
    pub dictionary: &'a [String],
    pub collect_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessCandidateSet {
    pub input: String,
    pub rows: Vec<GuessCandidateRow>,
    #[serde(default)]
    pub diagnostics: Vec<GuessCandidateDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessCandidateRow {
    pub row_id: String,
    pub page_index: u32,
    pub redaction: TrustedRedaction,
    pub anchor_set: AnchorSet,
    pub font: MeasurementFont,
    pub neighbor_facts: NeighborFacts,
    pub candidates: Vec<MeasuredCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredCandidate {
    pub text: String,
    pub width_pt: f32,
    pub glyph_width_sum_pt: f32,
    pub char_spacing_total_pt: f32,
    pub word_spacing_total_pt: f32,
    pub adjusted_error_pt: f32,
    pub noncanonical_penalty_pt: f32,
    #[serde(default)]
    pub provenance: Option<MeasuredCandidateProvenance>,
    #[serde(default)]
    pub predicted_left_edge_x_pt: Option<f32>,
    #[serde(default)]
    pub predicted_right_edge_x_pt: Option<f32>,
    #[serde(default)]
    pub actual_right_edge_x_pt: Option<f32>,
    pub target_width_pt: f32,
    pub error_pt: f32,
    pub normalized_error: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredCandidateProvenance {
    pub raw_entry_index: usize,
    pub raw_entry_text: String,
    pub raw_entry_normalized: String,
    pub template_id: String,
    pub template_family: String,
    pub variant_family: String,
    #[serde(default)]
    pub alias_source: Option<String>,
    #[serde(default)]
    pub orthographic_source: Option<String>,
    #[serde(default)]
    pub case_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessCandidateDiagnostic {
    pub row_id: String,
    pub page_index: u32,
    pub bbox: crate::types::redaction_types::Rect,
    pub stage: String,
    pub reason_code: String,
    pub message: String,
    #[serde(default)]
    pub metrics: BTreeMap<String, DiagnosticValue>,
}

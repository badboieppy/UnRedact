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
    pub word_count: u32,
    pub char_count: u32,
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

use crate::data::types::guess_candidate_types::MeasuredCandidate;
use crate::data::types::redaction_evidence_types::{
    AnchorSet, MeasurementFont, NeighborFacts, TrustedRedaction,
};

#[derive(Debug, Clone, PartialEq)]
pub struct GuessInputSet {
    pub input: String,
    pub rows: Vec<GuessInputRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuessInputRow {
    pub row_id: String,
    pub page_index: u32,
    pub redaction: TrustedRedaction,
    pub anchor_set: AnchorSet,
    pub font: MeasurementFont,
    pub neighbor_facts: NeighborFacts,
    pub candidates: Vec<MeasuredCandidate>,
}

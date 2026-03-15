use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::types::diagnostic_types::DiagnosticValue;
use crate::types::redaction_types::{Rect, RedactionKind};

#[derive(Debug, Clone, PartialEq)]
pub struct CollectRedactionEvidenceRequest<'a> {
    pub input_name: &'a str,
    pub pdf_bytes: &'a [u8],
    pub redactions: &'a crate::types::redaction_types::RedactionReport,
    pub collect_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionEvidenceSet {
    pub input: String,
    pub rows: Vec<RedactionEvidenceRow>,
    #[serde(default)]
    pub diagnostics: Vec<RedactionEvidenceDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionEvidenceRow {
    pub row_id: String,
    pub page_index: u32,
    pub redaction: TrustedRedaction,
    pub anchor_set: AnchorSet,
    pub font: MeasurementFont,
    pub neighbor_facts: NeighborFacts,
    #[serde(skip_serializing, skip_deserializing)]
    pub(crate) measurement_model: CandidateWidthModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustedRedaction {
    pub redaction_id: String,
    pub page_index: u32,
    pub bbox: Rect,
    pub kind: RedactionKind,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorSet {
    pub mode: AnchorMode,
    #[serde(default)]
    pub left: Option<AnchorSide>,
    #[serde(default)]
    pub right: Option<AnchorSide>,
    pub geometry: GuessGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorMode {
    TwoSided,
    LeftOnly,
    RightOnly,
}

impl AnchorMode {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TwoSided => "two_sided",
            Self::LeftOnly => "left_only",
            Self::RightOnly => "right_only",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorSide {
    pub anchor_id: String,
    pub text: String,
    pub bbox: Rect,
    pub text_edge_x_pt: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessGeometry {
    pub redaction_left_x_pt: f32,
    pub redaction_right_x_pt: f32,
    pub redaction_width_pt: f32,
    #[serde(default)]
    pub usable_left_edge_x_pt: Option<f32>,
    #[serde(default)]
    pub usable_right_edge_x_pt: Option<f32>,
    pub target_width_pt: f32,
    pub line_bias_pt: f32,
    pub tolerance_pt: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementFont {
    pub font_key: String,
    pub font_name: String,
    #[serde(default)]
    pub base_font: Option<String>,
    pub font_size_pt: f32,
    pub h_scale_pct: f32,
    pub char_spacing_pt: f32,
    pub word_spacing_pt: f32,
    #[serde(default)]
    pub width_source: Option<String>,
    #[serde(default)]
    pub encoding_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) struct MeasurementFontKey {
    pub page_index: u32,
    pub font_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MeasurementWidthSource {
    PdfWidthTable,
    Standard14Font,
    #[default]
    None,
}

impl MeasurementWidthSource {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PdfWidthTable => "pdf_width_table",
            Self::Standard14Font => "standard_14_font",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MeasurementEncodingSource {
    ToUnicode,
    EncodingDictionary,
    NamedEncoding,
    StandardDefaultEncoding,
    #[default]
    None,
}

impl MeasurementEncodingSource {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToUnicode => "to_unicode",
            Self::EncodingDictionary => "encoding_dictionary",
            Self::NamedEncoding => "named_encoding",
            Self::StandardDefaultEncoding => "standard_default_encoding",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NeighborFacts {
    pub line_id: String,
    pub line_row_count: usize,
    pub line_order: usize,
    #[serde(default)]
    pub previous_same_line: Option<NeighborRef>,
    #[serde(default)]
    pub next_same_line: Option<NeighborRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeighborRef {
    pub row_id: String,
    pub redaction_id: String,
    pub bbox: Rect,
    pub gap_pt: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CandidateWidthModel {
    pub resource_key: MeasurementFontKey,
    pub font_key: String,
    pub font_name: String,
    pub base_font: Option<String>,
    pub subtype: Option<String>,
    pub font_size_pt: f32,
    pub h_scale_pct: f32,
    pub char_spacing_pt: f32,
    pub word_spacing_pt: f32,
    pub width_source: MeasurementWidthSource,
    pub encoding_source: MeasurementEncodingSource,
    pub has_to_unicode: bool,
    pub has_encoding_dictionary: bool,
    pub has_named_encoding: bool,
    pub has_explicit_widths: bool,
    pub unicode_to_codes: BTreeMap<char, Vec<u16>>,
    pub code_to_width_units: BTreeMap<u16, i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionEvidenceDiagnostic {
    #[serde(default)]
    pub row_id: Option<String>,
    #[serde(default)]
    pub redaction_id: Option<String>,
    pub page_index: u32,
    pub bbox: Rect,
    pub stage: String,
    pub reason_code: String,
    pub message: String,
    #[serde(default)]
    pub metrics: BTreeMap<String, DiagnosticValue>,
}

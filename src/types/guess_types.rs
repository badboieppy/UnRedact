use serde::{Deserialize, Serialize};

use crate::types::redaction_types::Rect;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageTimingRecord {
    pub stage: String,
    pub value_ms: u64,
}

impl StageTimingRecord {
    #[inline]
    pub fn new(stage: &str, value_ms: u128) -> Self {
        Self {
            stage: stage.to_owned(),
            value_ms: value_ms.min(u128::from(u64::MAX)) as u64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GuessConfig {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessReport {
    pub input_redactions: String,
    pub input_fonts: String,
    pub guesses: Vec<RedactionGuess>,
    #[serde(default)]
    pub anchors: Vec<AnchorDecisionRecord>,
    #[serde(default)]
    pub stage_timings: Vec<StageTimingRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorReport {
    pub input_redactions: String,
    pub decisions: Vec<AnchorDecisionRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionGuess {
    pub page_index: u32,
    pub bbox: Rect,
    pub candidates: Vec<GuessCandidate>,
    pub context: GuessContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessCandidate {
    pub text: String,
    pub width_pt: f32,
    pub glyph_width_sum_pt: f32,
    pub char_spacing_total_pt: f32,
    pub word_spacing_total_pt: f32,
    #[serde(default)]
    pub predicted_left_edge_x_pt: Option<f32>,
    #[serde(default)]
    pub predicted_right_edge_x_pt: Option<f32>,
    #[serde(default)]
    pub actual_right_edge_x_pt: Option<f32>,
    pub target_width_pt: f32,
    pub error_pt: f32,
    #[serde(default)]
    pub normalized_error: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessContext {
    #[serde(default)]
    pub anchor_mode: Option<String>,
    #[serde(default)]
    pub usable_left_edge_x_pt: Option<f32>,
    #[serde(default)]
    pub usable_right_edge_x_pt: Option<f32>,
    pub target_width_pt: f32,
    #[serde(default)]
    pub font_key: Option<String>,
    #[serde(default)]
    pub font_name: Option<String>,
    #[serde(default)]
    pub base_font: Option<String>,
    #[serde(default)]
    pub font_size_pt: Option<f32>,
    #[serde(default)]
    pub h_scale_pct: Option<f32>,
    #[serde(default)]
    pub char_spacing_pt: Option<f32>,
    #[serde(default)]
    pub word_spacing_pt: Option<f32>,
    #[serde(default)]
    pub width_source: Option<String>,
    #[serde(default)]
    pub encoding_source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorType {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorSideDecision {
    pub anchor_id: String,
    pub anchor_type: AnchorType,
    pub text: String,
    pub bbox: Rect,
    pub x: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorDecisionRecord {
    pub anchor_row_id: String,
    #[serde(default)]
    pub redaction_id: Option<String>,
    pub page_index: u32,
    pub bbox: Rect,
    pub anchor_mode: String,
    #[serde(default)]
    pub left: Option<AnchorSideDecision>,
    #[serde(default)]
    pub right: Option<AnchorSideDecision>,
    #[serde(default)]
    pub usable_left_edge_x_pt: Option<f32>,
    #[serde(default)]
    pub usable_right_edge_x_pt: Option<f32>,
    pub target_width_pt: f32,
    #[serde(default)]
    pub measurement_seed_side: Option<String>,
    #[serde(default)]
    pub selected_line_id: Option<String>,
    #[serde(default)]
    pub selection_reason: Option<String>,
    #[serde(default)]
    pub selected_left_gap_pt: Option<f32>,
    #[serde(default)]
    pub selected_right_gap_pt: Option<f32>,
    #[serde(default)]
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

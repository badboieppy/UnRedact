use serde::{Deserialize, Serialize};

use crate::types::redaction_grouping_types::RedactionSegmentKind;
use crate::types::redaction_types::Rect;
use crate::types::runtime_defaults::{DEFAULT_VISUAL_SCORE_DPI, DEFAULT_VISUAL_SCORE_ENABLED};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuessConfig {
    pub visual_score: bool,
    pub visual_score_dpi: f32,
}

impl Default for GuessConfig {
    #[inline]
    fn default() -> Self {
        Self {
            visual_score: DEFAULT_VISUAL_SCORE_ENABLED,
            visual_score_dpi: DEFAULT_VISUAL_SCORE_DPI,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessReport {
    pub input_redactions: String,
    pub input_fonts: String,
    pub guesses: Vec<RedactionGuess>,
    #[serde(default)]
    pub anchors: Vec<AnchorDecisionRecord>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorReport {
    pub input_redactions: String,
    pub decisions: Vec<AnchorDecisionRecord>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

impl GuessReport {
    #[inline]
    pub fn to_anchor_report(&self) -> AnchorReport {
        AnchorReport {
            input_redactions: self.input_redactions.clone(),
            decisions: self.anchors.clone(),
            diagnostics: self.diagnostics.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionGuess {
    pub page_index: u32,
    pub bbox: Rect,
    pub candidates: Vec<GuessCandidate>,
    pub exact_matches: Vec<String>,
    pub context: GuessContext,
    #[serde(default)]
    pub visual_compared_pixels: Option<u32>,
    #[serde(default)]
    pub visual_mean_abs_diff: Option<f32>,
    #[serde(default)]
    pub visual_changed_pixel_ratio: Option<f32>,
    #[serde(default)]
    pub visual_reason: Option<String>,
    #[serde(default)]
    pub visual_dropped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessCandidate {
    pub text: String,
    pub score: f32,
    pub error_pt: f32,
    pub word_count: u32,
    #[serde(default)]
    pub width_pt: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessContext {
    #[serde(rename = "left_text", alias = "left_anchor_text")]
    pub left_anchor_text: String,
    #[serde(rename = "right_text", alias = "right_anchor_text")]
    pub right_anchor_text: String,
    pub gap_pt: f32,
    pub char_width_pt: f32,
    pub tol_pt: f32,
    #[serde(default)]
    pub anchor_left_x: Option<f32>,
    #[serde(default)]
    pub anchor_right_x: Option<f32>,
    #[serde(default)]
    pub anchor_font_key: Option<String>,
    #[serde(default)]
    pub anchor_font_name: Option<String>,
    #[serde(default)]
    pub anchor_font_size_pt: Option<f32>,
    #[serde(default)]
    pub anchor_h_scale_pct: Option<f32>,
    #[serde(default, alias = "row_bias_pt")]
    pub anchor_row_bias_pt: Option<f32>,
    #[serde(default)]
    pub anchor_mode: Option<String>,
    #[serde(default)]
    pub anchor_width_source: Option<String>,
    #[serde(default)]
    pub space_width_source: Option<String>,
    #[serde(default)]
    pub candidate_width_source: Option<String>,
    #[serde(default)]
    pub width_fallback_reason: Option<String>,
    #[serde(default)]
    pub confidence_score: Option<f32>,
    #[serde(default)]
    pub confidence_factors: Option<String>,
    #[serde(default)]
    pub anchor_row_id: Option<String>,
    #[serde(default)]
    pub left_anchor_id: Option<String>,
    #[serde(default)]
    pub right_anchor_id: Option<String>,
    #[serde(default)]
    pub left_anchor_type: Option<AnchorType>,
    #[serde(default)]
    pub right_anchor_type: Option<AnchorType>,
    #[serde(default)]
    pub left_anchor_selected_source: Option<AnchorSourceLabel>,
    #[serde(default)]
    pub right_anchor_selected_source: Option<AnchorSourceLabel>,
    #[serde(default)]
    pub left_anchor_confidence: Option<f32>,
    #[serde(default)]
    pub right_anchor_confidence: Option<f32>,
    #[serde(default)]
    pub row_anchor_confidence: Option<f32>,
    #[serde(default)]
    pub flow_group_id: Option<String>,
    #[serde(default)]
    pub flow_segment_id: Option<String>,
    #[serde(default)]
    pub flow_segment_kind: Option<RedactionSegmentKind>,
    #[serde(default)]
    pub flow_redaction_order: Option<u32>,
    #[serde(default)]
    pub flow_group_redaction_count: Option<u32>,
    #[serde(default)]
    pub flow_segment_redaction_count: Option<u32>,
    #[serde(
        rename = "guessable",
        alias = "has_anchor_pair",
        alias = "anchored_row",
        default
    )]
    pub has_anchor_pair: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorType {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorSourceLabel {
    RunExact,
    RunPrefixProjection,
    SyntheticBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorProjectionSource {
    CharAdvances,
    MeasuredTypography,
    ProportionalBbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorSelectionReasonCode {
    SelectedSameRunTwoSided,
    SelectedPairTwoSided,
    SelectedLeftOnlyFallback,
    SelectedRightOnlyFallback,
    RejectedMissingAnchor,
    RejectedLowerPriorityCandidate,
    RejectedInvalidSpan,
    RejectedEmptyAnchorText,
    RejectedOutOfBounds,
    RejectedUnavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorSideDecision {
    pub anchor_id: String,
    pub anchor_type: AnchorType,
    pub text: String,
    pub x: f32,
    pub selected_source: AnchorSourceLabel,
    #[serde(default)]
    pub projection_source: Option<AnchorProjectionSource>,
    #[serde(default)]
    pub alternate_x: Option<f32>,
    #[serde(default)]
    pub selected_minus_alternate_delta_pt: Option<f32>,
    #[serde(default)]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorCandidateDecision {
    pub candidate_id: String,
    pub anchor_mode: String,
    pub was_selected: bool,
    pub reason_code: AnchorSelectionReasonCode,
    #[serde(default)]
    pub tie_break_rank: Option<u32>,
    #[serde(default)]
    pub left: Option<AnchorSideDecision>,
    #[serde(default)]
    pub right: Option<AnchorSideDecision>,
    #[serde(default)]
    pub anchor_font_key: Option<String>,
    #[serde(default)]
    pub anchor_font_name: Option<String>,
    #[serde(default)]
    pub anchor_font_size_pt: Option<f32>,
    #[serde(default)]
    pub anchor_h_scale_pct: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorDecisionRecord {
    pub anchor_row_id: String,
    pub page_index: u32,
    pub bbox: Rect,
    #[serde(default)]
    pub selected_candidate_id: Option<String>,
    #[serde(default)]
    pub selected_mode: Option<String>,
    pub candidates: Vec<AnchorCandidateDecision>,
}

use serde::{Deserialize, Serialize};

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
    pub diagnostics: Vec<String>,
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
    #[serde(
        rename = "guessable",
        alias = "has_anchor_pair",
        alias = "anchored_row",
        default
    )]
    pub has_anchor_pair: bool,
}

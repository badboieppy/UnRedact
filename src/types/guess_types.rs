use serde::{Deserialize, Serialize};

use crate::types::redaction_types::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GuessConfig {
    pub max_words: usize,
    pub max_candidates: usize,
    pub max_dictionary: usize,
    pub tol_pt: f64,
    pub max_nodes: usize,
    #[serde(default = "default_visual_score_enabled")]
    pub visual_score: bool,
    #[serde(default = "default_visual_score_dpi")]
    pub visual_score_dpi: f32,
    #[serde(default = "default_visual_min_ink_pixels")]
    pub visual_min_ink_pixels: u32,
    #[serde(default)]
    pub visual_drop_threshold: Option<f32>,
}

impl Default for GuessConfig {
    #[inline]
    fn default() -> Self {
        Self {
            max_words: 4,
            max_candidates: 50,
            max_dictionary: 2000,
            tol_pt: 4.0,
            max_nodes: 50_000,
            visual_score: default_visual_score_enabled(),
            visual_score_dpi: default_visual_score_dpi(),
            visual_min_ink_pixels: default_visual_min_ink_pixels(),
            visual_drop_threshold: None,
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

fn default_visual_score_enabled() -> bool {
    true
}

fn default_visual_score_dpi() -> f32 {
    200.0_f32
}

fn default_visual_min_ink_pixels() -> u32 {
    64_u32
}

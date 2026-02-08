use serde::{Deserialize, Serialize};

use crate::redaction_finder::types::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GuessConfig {
    pub max_words: usize,
    pub max_candidates: usize,
    pub max_dictionary: usize,
    pub tol_pt: f64,
    pub max_nodes: usize,
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
    pub context: GuessContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessCandidate {
    pub text: String,
    pub score: f32,
    pub error_pt: f32,
    pub word_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuessContext {
    pub left_text: String,
    pub right_text: String,
    pub gap_pt: f32,
    pub char_width_pt: f32,
    pub tol_pt: f32,
}

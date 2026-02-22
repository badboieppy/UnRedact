use serde::{Deserialize, Serialize};

use crate::types::file_types::{FontDetectionReport, FontRunReport};
use crate::types::guess_types::{GuessConfig, GuessReport};
use crate::types::redaction_types::RedactionReport;
use crate::types::runtime_defaults::{
    DEFAULT_ENABLE_IMAGE_ANALYSIS, DEFAULT_INCLUDE_DETAILS, DEFAULT_VISUALIZE_OUTPUT,
};
use crate::types::visualizer_config::VisualizerConfig;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    pub include_details: bool,
    pub enable_image_analysis: bool,
    pub guess: GuessConfig,
    pub visualize: bool,
    pub visualizer: VisualizerConfig,
}

impl Default for PipelineConfig {
    #[inline]
    fn default() -> Self {
        Self {
            include_details: DEFAULT_INCLUDE_DETAILS,
            enable_image_analysis: DEFAULT_ENABLE_IMAGE_ANALYSIS,
            guess: GuessConfig::default(),
            visualize: DEFAULT_VISUALIZE_OUTPUT,
            visualizer: VisualizerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BytesPipelineRequest {
    pub input_name: String,
    pub pdf_bytes: Vec<u8>,
    pub dictionary_entries: Vec<String>,
    pub dictionary_diagnostics: Vec<String>,
    pub cfg: PipelineConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualizationPayload {
    pub pdf_bytes: Vec<u8>,
    pub font_runs: FontRunReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BytesPipelineOutputs {
    pub redactions: RedactionReport,
    pub fonts: FontDetectionReport,
    pub guesses: GuessReport,
    pub visualization_payload: Option<VisualizationPayload>,
    pub visualized_pdf_bytes: Option<Vec<u8>>,
}

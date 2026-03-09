use serde::{Deserialize, Serialize};

use crate::types::diagnostic_types::DiagnosticReport;
use crate::types::file_types::{FontDetectionReport, FontRunReport};
use crate::types::guess_types::{AnchorReport, GuessConfig, GuessReport};
use crate::types::redaction_types::RedactionReport;
use crate::types::runtime_defaults::{
    DEFAULT_ENABLE_IMAGE_ANALYSIS, DEFAULT_INCLUDE_DETAILS, DEFAULT_VISUALIZE_OUTPUT,
};
use crate::types::visualizer_config::VisualizerConfig;

pub mod guess_input_types;

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
    pub dictionary_bytes: Option<Vec<u8>>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct EncodedPipelineOutputs {
    pub redactions_json: Vec<u8>,
    pub fonts_json: Vec<u8>,
    pub guesses_json: Vec<u8>,
    pub anchors_json: Vec<u8>,
    pub diagnostics_json: Vec<u8>,
    pub visualized_pdf_bytes: Option<Vec<u8>>,
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

#[inline]
pub fn encode_outputs(outputs: &BytesPipelineOutputs) -> Result<EncodedPipelineOutputs, String> {
    let redactions_json = serde_json::to_vec_pretty(&outputs.redactions)
        .map_err(|error| format!("failed to encode redactions json: {error}"))?;
    let fonts_json = serde_json::to_vec_pretty(&outputs.fonts)
        .map_err(|error| format!("failed to encode fonts json: {error}"))?;
    let guesses_json = serde_json::to_vec_pretty(&outputs.guesses)
        .map_err(|error| format!("failed to encode guesses json: {error}"))?;
    let anchors_report: AnchorReport = outputs.guesses.to_anchor_report();
    let anchors_json = serde_json::to_vec_pretty(&anchors_report)
        .map_err(|error| format!("failed to encode anchors json: {error}"))?;
    let diagnostics_json = serde_json::to_vec_pretty(&DiagnosticReport {
        items: outputs.guesses.diagnostics.clone(),
    })
    .map_err(|error| format!("failed to encode diagnostics json: {error}"))?;
    Ok(EncodedPipelineOutputs {
        redactions_json,
        fonts_json,
        guesses_json,
        anchors_json,
        diagnostics_json,
        visualized_pdf_bytes: outputs.visualized_pdf_bytes.clone(),
    })
}

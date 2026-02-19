use std::path::PathBuf;

use crate::types::file_types::FontDetectionReport;
use crate::types::guess_types::{GuessConfig, GuessReport};
use crate::types::redaction_types::RedactionReport;
use crate::types::visualizer_config::VisualizerConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineConfig {
    pub include_details: bool,
    pub enable_image_analysis: bool,
    pub raster_dpi: f32,
    pub guess: GuessConfig,
    pub visualize: bool,
    pub visualizer: VisualizerConfig,
}

impl Default for PipelineConfig {
    #[inline]
    fn default() -> Self {
        Self {
            include_details: false,
            enable_image_analysis: true,
            raster_dpi: 200.0_f32,
            guess: GuessConfig::default(),
            visualize: false,
            visualizer: VisualizerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutputFilePaths {
    pub redactions_path: PathBuf,
    pub fonts_path: PathBuf,
    pub guesses_path: PathBuf,
    pub visualized_pdf_path: Option<PathBuf>,
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
pub struct BytesPipelineOutputs {
    pub redactions: RedactionReport,
    pub fonts: FontDetectionReport,
    pub guesses: GuessReport,
    pub visualized_pdf_bytes: Option<Vec<u8>>,
}

use std::path::Path;

use crate::redaction_visualizer::data::{VisualizationDataSource, VisualizationInputs};
use crate::redaction_visualizer::dependency::PdfAnnotator;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualizerConfig {
    pub color: [f32; 3],
    pub text_color: [f32; 3],
    pub border_width: f32,
}

impl Default for VisualizerConfig {
    #[inline]
    fn default() -> Self {
        Self {
            color: [1.0, 0.0, 0.0],
            text_color: [0.0, 0.4, 1.0],
            border_width: 1.0,
        }
    }
}

#[inline]
pub fn run_visualizer(
    data: &dyn VisualizationDataSource,
    annotator: &PdfAnnotator,
    pdf_path: &Path,
    report: &crate::redaction_finder::types::RedactionReport,
    guesses: Option<&crate::redaction_guess::types::GuessReport>,
    font_runs: Option<&crate::font_detection::logic::types::file_types::FontRunReport>,
    cfg: VisualizerConfig,
) -> Result<Vec<u8>, String> {
    let inputs = data.load_inputs(pdf_path, report, guesses, font_runs)?;
    build_visualized_pdf(annotator, inputs, cfg)
}

#[inline]
pub fn build_visualized_pdf(
    annotator: &PdfAnnotator,
    inputs: VisualizationInputs,
    cfg: VisualizerConfig,
) -> Result<Vec<u8>, String> {
    annotator.annotate(
        &inputs.pdf_bytes,
        &inputs.rects,
        &inputs.overlays,
        cfg.color,
        cfg.text_color,
        cfg.border_width,
    )
}

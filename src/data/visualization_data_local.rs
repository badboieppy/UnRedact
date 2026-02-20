use std::path::Path;

use crate::dependency::file_store::FileStore;
use crate::dependency::pdf_annotator::PdfAnnotator;
use crate::types::file_types::FontRunReport;
use crate::types::guess_types::GuessReport;
use crate::types::redaction_types::RedactionReport;
use crate::types::visualizer_config::VisualizerConfig;

use super::visualization_data::{VisualizationData, VisualizationInputs};

pub trait VisualizationDataSource {
    fn load_inputs(
        &self,
        pdf_path: &Path,
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
    ) -> Result<VisualizationInputs, String>;
    fn write_output(&self, output_path: &Path, bytes: &[u8]) -> Result<(), String>;
}

impl VisualizationData {
    #[inline]
    pub fn render_visualized_pdf(
        &self,
        pdf_path: &Path,
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
        cfg: VisualizerConfig,
    ) -> Result<Vec<u8>, String> {
        let inputs = self.load_inputs(pdf_path, report, guesses, font_runs)?;
        let annotator = PdfAnnotator;
        annotator.annotate(
            &inputs.pdf_bytes,
            &inputs.rects,
            &inputs.overlays,
            cfg.color,
            cfg.text_color,
            cfg.border_width,
        )
    }

    #[inline]
    pub fn write_visualized_pdf(&self, output_path: &Path, bytes: &[u8]) -> Result<(), String> {
        let file_store = FileStore;
        file_store.write(output_path, bytes)
    }

    #[inline]
    pub fn render_and_write(
        &self,
        pdf_path: &Path,
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
        output_path: &Path,
        cfg: VisualizerConfig,
    ) -> Result<(), String> {
        let bytes = self.render_visualized_pdf(pdf_path, report, guesses, font_runs, cfg)?;
        self.write_output(output_path, &bytes)
    }
}

impl VisualizationDataSource for VisualizationData {
    #[inline]
    fn load_inputs(
        &self,
        pdf_path: &Path,
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
    ) -> Result<VisualizationInputs, String> {
        let file_store = FileStore;
        let pdf_bytes = file_store.read(pdf_path)?;
        self.load_inputs_from_bytes(&pdf_bytes, report, guesses, font_runs)
    }

    #[inline]
    fn write_output(&self, output_path: &Path, bytes: &[u8]) -> Result<(), String> {
        self.write_visualized_pdf(output_path, bytes)
    }
}

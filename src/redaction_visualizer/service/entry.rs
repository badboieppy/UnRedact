use std::path::Path;

use crate::redaction_visualizer::data::{VisualizationData, VisualizationDataSource as _};
use crate::redaction_visualizer::dependency::PdfAnnotator;
use crate::redaction_visualizer::logic::{run_visualizer, VisualizerConfig};

#[inline]
pub fn run_from_report(
    pdf_path: &Path,
    report: &crate::redaction_finder::types::RedactionReport,
    output_path: &Path,
    cfg: VisualizerConfig,
) -> Result<(), String> {
    let data = VisualizationData::new();
    let annotator = PdfAnnotator;
    let bytes = run_visualizer(&data, &annotator, pdf_path, report, cfg)?;
    data.write_output(output_path, &bytes)
}

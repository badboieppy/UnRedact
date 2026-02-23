use crate::data::visualization_data::VisualizationData;
use crate::logic::types::VisualizationPayload;
use crate::types::guess_types::GuessReport;
use crate::types::redaction_types::RedactionReport;
use crate::types::visualizer_config::VisualizerConfig;

pub struct VisualizationRenderRequest<'a> {
    pub redactions: &'a RedactionReport,
    pub guesses: &'a GuessReport,
    pub payload: Option<&'a VisualizationPayload>,
    pub visualizer: VisualizerConfig,
}

#[inline]
pub fn render_visualization(
    req: VisualizationRenderRequest<'_>,
) -> Result<Option<Vec<u8>>, String> {
    let Some(payload) = req.payload else {
        return Ok(None);
    };
    let visualization_data = VisualizationData::new();
    let bytes = visualization_data.render_visualized_pdf_from_bytes(
        &payload.pdf_bytes,
        req.redactions,
        Some(req.guesses),
        Some(&payload.font_runs),
        req.visualizer,
    )?;
    Ok(Some(bytes))
}

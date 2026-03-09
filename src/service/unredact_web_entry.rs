use serde::{Deserialize, Serialize};

use crate::logic::{
    encode_outputs, render_visualization, run_redaction_guessing_component, BytesPipelineRequest,
    EncodedPipelineOutputs, PipelineConfig, VisualizationRenderRequest,
};
use crate::types::time::Instant;

pub type UnredactWebConfig = PipelineConfig;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnredactWebRequest {
    pub input_name: String,
    pub pdf_bytes: Vec<u8>,
    pub dictionary_file_bytes: Option<Vec<u8>>,
    #[serde(default)]
    pub cfg: UnredactWebConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnredactWebOutputs {
    pub redactions_json: Vec<u8>,
    pub fonts_json: Vec<u8>,
    pub guesses_json: Vec<u8>,
    pub anchors_json: Vec<u8>,
    pub diagnostics_json: Vec<u8>,
    pub visualized_pdf_bytes: Option<Vec<u8>>,
}

#[inline]
pub fn run(req: UnredactWebRequest) -> Result<UnredactWebOutputs, String> {
    let should_visualize = req.cfg.visualize;
    let visualizer = req.cfg.visualizer;
    let mut outputs = run_redaction_guessing_component(BytesPipelineRequest {
        input_name: req.input_name,
        pdf_bytes: req.pdf_bytes,
        dictionary_bytes: req.dictionary_file_bytes,
        cfg: req.cfg,
    })?;
    let visualize_ms = if should_visualize {
        let visualize_started = Instant::now();
        let rendered = render_visualization(VisualizationRenderRequest {
            redactions: &outputs.redactions,
            guesses: &outputs.guesses,
            payload: outputs.visualization_payload.as_ref(),
            visualizer,
        })?;
        outputs.visualized_pdf_bytes = rendered;
        visualize_started.elapsed().as_millis()
    } else {
        0_u128
    };
    let mut visualize_record = crate::types::diagnostic_types::DiagnosticRecord::info(
        "service",
        "visualize",
        "timing_ms",
    );
    visualize_record.metrics.insert(
        "value_ms".to_owned(),
        crate::types::diagnostic_types::DiagnosticValue::Integer(visualize_ms as i64),
    );
    outputs.guesses.diagnostics.push(visualize_record);
    outputs.visualization_payload = None;
    let encoded: EncodedPipelineOutputs = encode_outputs(&outputs)?;
    Ok(UnredactWebOutputs {
        redactions_json: encoded.redactions_json,
        fonts_json: encoded.fonts_json,
        guesses_json: encoded.guesses_json,
        anchors_json: encoded.anchors_json,
        diagnostics_json: encoded.diagnostics_json,
        visualized_pdf_bytes: encoded.visualized_pdf_bytes,
    })
}

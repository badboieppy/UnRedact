use crate::logic::redaction_guessing_component::run_redaction_guessing_component;
use crate::logic::types::{
    BytesPipelineRequest, NamedBinaryArtifact, PipelineConfig, PipelineExecutionOptions,
};
use crate::logic::visual_anchor_metrics_component::{
    run_visual_anchor_metrics, RunVisualAnchorMetricsRequest,
};
use crate::types::diagnostic_types::DiagnosticRecord;
use crate::types::guess_types::GuessReport;
use crate::types::visual_anchor_metric_types::VisualAnchorMetricsReport;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunVisualAnchorResearchRequest<'a> {
    pub input_name: &'a str,
    pub pdf_bytes: &'a [u8],
    pub dictionary_bytes: Option<&'a [u8]>,
    pub cfg: &'a PipelineConfig,
    pub collect_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunVisualAnchorResearchOutput {
    pub guesses: GuessReport,
    pub diagnostics: Option<Vec<DiagnosticRecord>>,
    pub visual_report: VisualAnchorMetricsReport,
    pub visual_diagnostics: Vec<DiagnosticRecord>,
    pub visual_crops: Vec<NamedBinaryArtifact>,
}

#[inline]
pub(crate) fn run_visual_anchor_research(
    req: RunVisualAnchorResearchRequest<'_>,
) -> Result<RunVisualAnchorResearchOutput, String> {
    let pipeline = run_redaction_guessing_component(BytesPipelineRequest {
        input_name: req.input_name.to_owned(),
        pdf_bytes: req.pdf_bytes.to_vec(),
        dictionary_bytes: req.dictionary_bytes.map(ToOwned::to_owned),
        cfg: req.cfg.clone(),
        execution: PipelineExecutionOptions {
            collect_diagnostics: req.collect_diagnostics,
        },
    })?;
    let visual = run_visual_anchor_metrics(RunVisualAnchorMetricsRequest {
        pdf_name: req.input_name,
        pdf_bytes: req.pdf_bytes,
        guesses: &pipeline.guesses,
        collect_diagnostics: req.collect_diagnostics,
    })?;
    Ok(RunVisualAnchorResearchOutput {
        guesses: pipeline.guesses,
        diagnostics: pipeline.diagnostics,
        visual_report: visual.report,
        visual_diagnostics: visual.diagnostics,
        visual_crops: visual.crops,
    })
}

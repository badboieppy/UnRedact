use crate::data::dictionary_data::DictionaryData;
use crate::data::fonts_data::FontsData;
use crate::data::redaction_scan_data::{
    build_report_from_input_name, run_redaction_scan_from_bytes,
};
use crate::data::redactions_data::RedactionsData;
use crate::logic::types::{
    BytesPipelineOutputs, BytesPipelineRequest, PipelineConfig, VisualizationPayload,
};
use crate::types::diagnostic_types::{DiagnosticRecord, DiagnosticValue};
use crate::types::file_types::{FontDetectionReport, FontRunReport};
use crate::types::guess_types::{GuessReport, StageTimingRecord};
use crate::types::redaction_types::{RedactionFinderConfig, RedactionMode, RedactionReport};
use crate::types::runtime_defaults::RASTER_HIGHPASS_DPI;
use crate::types::time::Instant;

pub(crate) mod helpers;

#[cfg(feature = "cli-entry")]
pub use helpers::guessing::{run_anchor_from_bytes, RunAnchorFromBytesRequest};
pub use helpers::guessing::{run_from_bytes as run_guess_from_bytes, RunGuessFromBytesRequest};

const INCLUDE_FULL_PAGE_RECTS: bool = false;

struct RedactionStageOutput {
    redactions: RedactionReport,
    elapsed_ms: u128,
}

struct FontStageOutput {
    fonts: FontDetectionReport,
    elapsed_ms: u128,
}

struct GuessStageOutput {
    guesses: GuessReport,
    diagnostics: Option<Vec<DiagnosticRecord>>,
    elapsed_ms: u128,
}

struct RunGuessStageInputs<'a> {
    input_name: &'a str,
    pdf_bytes: &'a [u8],
    redactions: &'a RedactionReport,
    dictionary_entries: &'a [String],
    dictionary_diagnostics: &'a [String],
    font_runs: &'a FontRunReport,
    preloaded_font_runs_elapsed_ms: Option<u128>,
    cfg: &'a PipelineConfig,
    collect_diagnostics: bool,
}

struct FontRunsStageOutput {
    font_runs: FontRunReport,
    elapsed_ms: u128,
}

struct VisualizationPayloadStageOutput {
    payload: Option<VisualizationPayload>,
}

#[inline]
pub fn run_redaction_guessing_component(
    req: BytesPipelineRequest,
) -> Result<BytesPipelineOutputs, String> {
    let BytesPipelineRequest {
        input_name,
        pdf_bytes,
        dictionary_bytes,
        cfg,
        execution,
    } = req;
    let component_started = Instant::now();
    let redactions_data = RedactionsData::new();
    let fonts_data = FontsData::new();
    let dictionary_data = DictionaryData::new();
    let dictionary_inputs =
        dictionary_data.load_dictionary_from_bytes(dictionary_bytes.as_deref())?;

    let redaction_stage = run_redaction_stage(&input_name, &pdf_bytes, &cfg, &redactions_data)?;
    let font_stage = run_font_stage(&input_name, &pdf_bytes, cfg.include_details, &fonts_data)?;
    let font_runs_stage = run_font_runs_stage(&input_name, &pdf_bytes, &fonts_data)?;
    let mut guess_stage = run_guess_stage(RunGuessStageInputs {
        input_name: &input_name,
        pdf_bytes: &pdf_bytes,
        redactions: &redaction_stage.redactions,
        dictionary_entries: &dictionary_inputs.dictionary,
        dictionary_diagnostics: &dictionary_inputs.diagnostics,
        font_runs: &font_runs_stage.font_runs,
        preloaded_font_runs_elapsed_ms: Some(font_runs_stage.elapsed_ms),
        cfg: &cfg,
        collect_diagnostics: execution.collect_diagnostics,
    })?;
    push_stage_timing(
        &mut guess_stage.guesses,
        &mut guess_stage.diagnostics,
        "redactions",
        redaction_stage.elapsed_ms,
    );
    push_stage_timing(
        &mut guess_stage.guesses,
        &mut guess_stage.diagnostics,
        "fonts",
        font_stage.elapsed_ms,
    );
    push_stage_timing(
        &mut guess_stage.guesses,
        &mut guess_stage.diagnostics,
        "guess",
        guess_stage.elapsed_ms,
    );

    let visualization_stage =
        build_visualization_payload_stage(&pdf_bytes, &cfg, &font_runs_stage.font_runs)?;

    push_stage_timing(
        &mut guess_stage.guesses,
        &mut guess_stage.diagnostics,
        "orchestrator_total",
        component_started.elapsed().as_millis(),
    );
    sort_diagnostics(&mut guess_stage.diagnostics);

    Ok(BytesPipelineOutputs {
        redactions: redaction_stage.redactions,
        fonts: font_stage.fonts,
        guesses: guess_stage.guesses,
        diagnostics: guess_stage.diagnostics,
        visualization_payload: visualization_stage.payload,
        visualized_pdf_bytes: None,
    })
}

fn run_redaction_stage(
    input_name: &str,
    pdf_bytes: &[u8],
    cfg: &PipelineConfig,
    redactions_data: &RedactionsData,
) -> Result<RedactionStageOutput, String> {
    let started = Instant::now();
    let redaction_cfg = RedactionFinderConfig {
        include_details: cfg.include_details,
        mode: RedactionMode::All,
        include_full_page_rects: INCLUDE_FULL_PAGE_RECTS,
        enable_image_analysis: cfg.enable_image_analysis,
        raster_dpi: RASTER_HIGHPASS_DPI,
    };
    let output = if cfg.enable_image_analysis {
        let renderer = redactions_data.build_renderer(pdf_bytes)?;
        run_redaction_scan_from_bytes(pdf_bytes, Some(&renderer), redaction_cfg)?
    } else {
        run_redaction_scan_from_bytes(pdf_bytes, None, redaction_cfg)?
    };
    Ok(RedactionStageOutput {
        redactions: build_report_from_input_name(input_name, output),
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn run_font_stage(
    input_name: &str,
    pdf_bytes: &[u8],
    include_details: bool,
    fonts_data: &FontsData,
) -> Result<FontStageOutput, String> {
    let started = Instant::now();
    let fonts = fonts_data.detect_fonts_from_bytes(input_name, pdf_bytes, include_details)?;
    Ok(FontStageOutput {
        fonts,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn run_font_runs_stage(
    input_name: &str,
    pdf_bytes: &[u8],
    fonts_data: &FontsData,
) -> Result<FontRunsStageOutput, String> {
    let started = Instant::now();
    let font_runs = fonts_data
        .load_font_runs_from_bytes(input_name, pdf_bytes)?
        .report;
    Ok(FontRunsStageOutput {
        font_runs,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn run_guess_stage(inputs: RunGuessStageInputs<'_>) -> Result<GuessStageOutput, String> {
    let started = Instant::now();
    let guess_output = run_guess_from_bytes(RunGuessFromBytesRequest {
        pdf_name: inputs.input_name,
        pdf_bytes: inputs.pdf_bytes,
        redactions: inputs.redactions,
        dictionary: inputs.dictionary_entries,
        diagnostics: inputs.dictionary_diagnostics,
        preloaded_font_runs: Some(inputs.font_runs),
        preloaded_font_runs_elapsed_ms: inputs.preloaded_font_runs_elapsed_ms,
        cfg: &inputs.cfg.guess,
        collect_diagnostics: inputs.collect_diagnostics,
    })?;
    Ok(GuessStageOutput {
        guesses: guess_output.report,
        diagnostics: guess_output.diagnostics,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn build_visualization_payload_stage(
    pdf_bytes: &[u8],
    cfg: &PipelineConfig,
    font_runs: &FontRunReport,
) -> Result<VisualizationPayloadStageOutput, String> {
    if !cfg.visualize {
        return Ok(VisualizationPayloadStageOutput { payload: None });
    }
    Ok(VisualizationPayloadStageOutput {
        payload: Some(VisualizationPayload {
            pdf_bytes: pdf_bytes.to_vec(),
            font_runs: font_runs.clone(),
        }),
    })
}

pub(crate) fn stage_timing_record(stage: &str, elapsed_ms: u128) -> StageTimingRecord {
    StageTimingRecord::new(stage, elapsed_ms)
}

pub(crate) fn stage_timing_diagnostic(stage: &str, elapsed_ms: u128) -> DiagnosticRecord {
    let mut record = DiagnosticRecord::info("logic", stage, "timing_ms");
    record.metrics.insert(
        "value_ms".to_owned(),
        DiagnosticValue::Integer(elapsed_ms as i64),
    );
    record
}

fn push_stage_timing(
    guesses: &mut GuessReport,
    diagnostics: &mut Option<Vec<DiagnosticRecord>>,
    stage: &str,
    elapsed_ms: u128,
) {
    guesses
        .stage_timings
        .push(stage_timing_record(stage, elapsed_ms));
    if let Some(items) = diagnostics.as_mut() {
        items.push(stage_timing_diagnostic(stage, elapsed_ms));
    }
}

fn sort_diagnostics(diagnostics: &mut Option<Vec<DiagnosticRecord>>) {
    if let Some(items) = diagnostics.as_mut() {
        items.sort_by(|left, right| {
            left.page_index
                .cmp(&right.page_index)
                .then_with(|| left.row_id.cmp(&right.row_id))
                .then_with(|| left.stage.cmp(&right.stage))
                .then_with(|| left.code.cmp(&right.code))
                .then_with(|| left.layer.cmp(&right.layer))
        });
    }
}

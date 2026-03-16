use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::logic::{
    build_output_file_paths, discover_pdf_inputs, ensure_batch_output_dir_for_input,
    read_dictionary_input, read_input_pdf_bytes, render_visualization,
    run_redaction_guessing_component, validate_batch_input_directory, write_batch_manifest,
    write_encoded_outputs, BytesPipelineRequest, OutputFilePaths, PipelineConfig,
    PipelineExecutionOptions, VisualizationRenderRequest,
};
use crate::types::time::Instant;

pub type UnredactServiceConfig = PipelineConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct UnredactServiceRequest {
    pub input: PathBuf,
    pub output_dir: PathBuf,
    pub dictionary_path: Option<PathBuf>,
    pub cfg: UnredactServiceConfig,
    pub write_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnredactServiceOutputs {
    pub redactions_path: PathBuf,
    pub fonts_path: PathBuf,
    pub guesses_path: PathBuf,
    pub anchors_path: PathBuf,
    pub diagnostics_path: Option<PathBuf>,
    pub visual_anchor_metrics_path: Option<PathBuf>,
    pub visual_anchor_crops_dir: Option<PathBuf>,
    pub visualized_pdf_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchFileStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnredactBatchRequest {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub dictionary_path: Option<PathBuf>,
    pub cfg: UnredactServiceConfig,
    pub write_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnredactBatchFileResult {
    pub input: PathBuf,
    pub status: BatchFileStatus,
    pub redactions_path: Option<PathBuf>,
    pub fonts_path: Option<PathBuf>,
    pub guesses_path: Option<PathBuf>,
    pub anchors_path: Option<PathBuf>,
    pub diagnostics_path: Option<PathBuf>,
    pub visual_anchor_metrics_path: Option<PathBuf>,
    pub visual_anchor_crops_dir: Option<PathBuf>,
    pub visualized_pdf_path: Option<PathBuf>,
    pub error: Option<String>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnredactBatchOutputs {
    pub results: Vec<UnredactBatchFileResult>,
    pub success_count: usize,
    pub failure_count: usize,
    pub elapsed_ms: u128,
    pub manifest_path: PathBuf,
}

#[inline]
pub fn run_from_paths(
    input: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: UnredactServiceConfig,
) -> Result<UnredactServiceOutputs, String> {
    run_from_paths_with_diagnostics(input, output_dir, dictionary_path, cfg, false)
}

#[inline]
pub fn run_from_paths_with_diagnostics(
    input: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: UnredactServiceConfig,
    write_diagnostics: bool,
) -> Result<UnredactServiceOutputs, String> {
    run(UnredactServiceRequest {
        input: input.to_path_buf(),
        output_dir: output_dir.to_path_buf(),
        dictionary_path: dictionary_path.map(PathBuf::from),
        cfg,
        write_diagnostics,
    })
}

#[inline]
pub fn run_batch_from_paths(
    input_dir: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: UnredactServiceConfig,
) -> Result<UnredactBatchOutputs, String> {
    run_batch_from_paths_with_diagnostics(input_dir, output_dir, dictionary_path, cfg, false)
}

#[inline]
pub fn run_batch_from_paths_with_diagnostics(
    input_dir: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: UnredactServiceConfig,
    write_diagnostics: bool,
) -> Result<UnredactBatchOutputs, String> {
    run_batch(UnredactBatchRequest {
        input_dir: input_dir.to_path_buf(),
        output_dir: output_dir.to_path_buf(),
        dictionary_path: dictionary_path.map(PathBuf::from),
        cfg,
        write_diagnostics,
    })
}

#[inline]
pub fn run(req: UnredactServiceRequest) -> Result<UnredactServiceOutputs, String> {
    let output_paths: OutputFilePaths =
        build_output_file_paths(&req.input, &req.output_dir, req.write_diagnostics)?;
    let dictionary_bytes = read_dictionary_input(req.dictionary_path.as_deref())?;
    let should_visualize = req.cfg.visualize;
    let visualizer = req.cfg.visualizer;
    let bytes_req = BytesPipelineRequest {
        input_name: req.input.to_string_lossy().to_string(),
        pdf_bytes: read_input_pdf_bytes(&req.input)?,
        dictionary_bytes,
        cfg: req.cfg,
        execution: PipelineExecutionOptions {
            collect_diagnostics: req.write_diagnostics,
        },
    };
    let mut bytes_outputs = run_redaction_guessing_component(bytes_req)?;
    let visualize_ms = if should_visualize {
        let visualize_started = Instant::now();
        let rendered = render_visualization(VisualizationRenderRequest {
            redactions: &bytes_outputs.redactions,
            guesses: &bytes_outputs.guesses,
            payload: bytes_outputs.visualization_payload.as_ref(),
            visualizer,
        })?;
        bytes_outputs.visualized_pdf_bytes = rendered;
        visualize_started.elapsed().as_millis()
    } else {
        0_u128
    };
    append_visualize_timing(&mut bytes_outputs, visualize_ms);
    bytes_outputs.visualization_payload = None;
    let encoded_outputs = crate::logic::encode_outputs(&bytes_outputs)?;
    write_encoded_outputs(&output_paths, &encoded_outputs)?;
    Ok(UnredactServiceOutputs {
        redactions_path: output_paths.redactions_path,
        fonts_path: output_paths.fonts_path,
        guesses_path: output_paths.guesses_path,
        anchors_path: output_paths.anchors_path,
        diagnostics_path: output_paths.diagnostics_path,
        visual_anchor_metrics_path: output_paths.visual_anchor_metrics_path,
        visual_anchor_crops_dir: output_paths.visual_anchor_crops_dir,
        visualized_pdf_path: output_paths.visualized_pdf_path,
    })
}

#[inline]
pub fn run_batch(req: UnredactBatchRequest) -> Result<UnredactBatchOutputs, String> {
    validate_batch_input_directory(&req.input_dir)?;
    let inputs = discover_pdf_inputs(&req.input_dir)?;
    if inputs.is_empty() {
        return Err(format!(
            "no supported input files found in {}",
            req.input_dir.display()
        ));
    }

    let start = Instant::now();
    let mut results = run_batch_serial(
        &inputs,
        &req.input_dir,
        &req.output_dir,
        req.dictionary_path.as_deref(),
        &req.cfg,
        req.write_diagnostics,
    )?;
    results.sort_by(|left, right| left.input.cmp(&right.input));

    let success_count = results
        .iter()
        .filter(|result| result.status == BatchFileStatus::Ok)
        .count();
    let failure_count = results.len().saturating_sub(success_count);
    let elapsed_ms = start.elapsed().as_millis();

    let payload = serde_json::to_vec_pretty(&UnredactBatchOutputs {
        results: results.clone(),
        success_count,
        failure_count,
        elapsed_ms,
        manifest_path: req.output_dir.join("batch_manifest.json"),
    })
    .map_err(|error| format!("failed to encode batch manifest: {error}"))?;
    let manifest_path = write_batch_manifest(&req.output_dir, &payload)?;

    Ok(UnredactBatchOutputs {
        results,
        success_count,
        failure_count,
        elapsed_ms,
        manifest_path,
    })
}

fn run_batch_serial(
    inputs: &[PathBuf],
    input_dir: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: &UnredactServiceConfig,
    write_diagnostics: bool,
) -> Result<Vec<UnredactBatchFileResult>, String> {
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        let result = run_batch_item(
            input,
            input_dir,
            output_dir,
            dictionary_path,
            cfg,
            write_diagnostics,
        );
        out.push(result);
    }
    Ok(out)
}

fn run_batch_item(
    input: &Path,
    input_dir: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: &UnredactServiceConfig,
    write_diagnostics: bool,
) -> UnredactBatchFileResult {
    let started = Instant::now();
    let item_output_dir = match ensure_batch_output_dir_for_input(output_dir, input_dir, input) {
        Ok(path) => path,
        Err(error) => {
            return UnredactBatchFileResult {
                input: input.to_path_buf(),
                status: BatchFileStatus::Error,
                redactions_path: None,
                fonts_path: None,
                guesses_path: None,
                anchors_path: None,
                diagnostics_path: None,
                visual_anchor_metrics_path: None,
                visual_anchor_crops_dir: None,
                visualized_pdf_path: None,
                error: Some(error),
                elapsed_ms: started.elapsed().as_millis(),
            }
        }
    };

    let run = run_from_paths_with_diagnostics(
        input,
        &item_output_dir,
        dictionary_path,
        cfg.clone(),
        write_diagnostics,
    );
    match run {
        Ok(outputs) => UnredactBatchFileResult {
            input: input.to_path_buf(),
            status: BatchFileStatus::Ok,
            redactions_path: Some(outputs.redactions_path),
            fonts_path: Some(outputs.fonts_path),
            guesses_path: Some(outputs.guesses_path),
            anchors_path: Some(outputs.anchors_path),
            diagnostics_path: outputs.diagnostics_path,
            visual_anchor_metrics_path: outputs.visual_anchor_metrics_path,
            visual_anchor_crops_dir: outputs.visual_anchor_crops_dir,
            visualized_pdf_path: outputs.visualized_pdf_path,
            error: None,
            elapsed_ms: started.elapsed().as_millis(),
        },
        Err(error) => UnredactBatchFileResult {
            input: input.to_path_buf(),
            status: BatchFileStatus::Error,
            redactions_path: None,
            fonts_path: None,
            guesses_path: None,
            anchors_path: None,
            diagnostics_path: None,
            visual_anchor_metrics_path: None,
            visual_anchor_crops_dir: None,
            visualized_pdf_path: None,
            error: Some(format!("{}: {error}", input.display())),
            elapsed_ms: started.elapsed().as_millis(),
        },
    }
}

fn append_visualize_timing(
    outputs: &mut crate::logic::types::BytesPipelineOutputs,
    visualize_ms: u128,
) {
    outputs
        .guesses
        .stage_timings
        .push(crate::types::guess_types::StageTimingRecord::new(
            "visualize",
            visualize_ms,
        ));
    if let Some(diagnostics) = outputs.diagnostics.as_mut() {
        let mut record = crate::types::diagnostic_types::DiagnosticRecord::info(
            "service",
            "visualize",
            "timing_ms",
        );
        record.metrics.insert(
            "value_ms".to_owned(),
            crate::types::diagnostic_types::DiagnosticValue::Integer(visualize_ms as i64),
        );
        diagnostics.push(record);
        diagnostics.sort_by(|left, right| {
            left.page_index
                .cmp(&right.page_index)
                .then_with(|| left.row_id.cmp(&right.row_id))
                .then_with(|| left.stage.cmp(&right.stage))
                .then_with(|| left.code.cmp(&right.code))
                .then_with(|| left.layer.cmp(&right.layer))
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::types::diagnostic_types::DiagnosticReport;

    use super::{
        run_batch_from_paths, run_from_paths, run_from_paths_with_diagnostics, BatchFileStatus,
        UnredactServiceConfig,
    };

    fn test_dir(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "unredact_service_batch_test_{}_{}_{}",
            tag,
            std::process::id(),
            stamp
        ))
    }

    #[test]
    fn run_batch_errors_when_directory_has_no_supported_files() {
        let input_dir = test_dir("no_supported_files_input");
        let output_dir = test_dir("no_supported_files_output");
        std::fs::create_dir_all(&input_dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", input_dir.display()));
        std::fs::create_dir_all(&output_dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
        std::fs::write(input_dir.join("notes.txt"), b"not a pdf").unwrap_or_else(|error| {
            panic!(
                "failed to create {}: {error}",
                input_dir.join("notes.txt").display()
            )
        });

        let result = run_batch_from_paths(
            &input_dir,
            &output_dir,
            None,
            UnredactServiceConfig::default(),
        );
        let error = result.expect_err("batch run should fail when no PDF files are present");
        assert!(
            error.contains("no supported input files found"),
            "expected unsupported-input error, got: {error}"
        );
    }

    #[test]
    fn run_batch_recurses_pdf_inputs_and_preserves_relative_output_paths() {
        let input_dir = test_dir("nested_input");
        let output_dir = test_dir("nested_output");
        let nested_dir = input_dir.join("a").join("b");
        std::fs::create_dir_all(&nested_dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", nested_dir.display()));
        std::fs::create_dir_all(&output_dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));

        let sample = Path::new("test_data/EFTA00101126.pdf");
        let sample_bytes = std::fs::read(sample)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", sample.display()));
        let nested_pdf = nested_dir.join("report.pdf");
        std::fs::write(&nested_pdf, sample_bytes)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", nested_pdf.display()));
        std::fs::write(input_dir.join("ignored.txt"), b"not a pdf").unwrap_or_else(|error| {
            panic!(
                "failed to write {}: {error}",
                input_dir.join("ignored.txt").display()
            )
        });

        let outputs = run_batch_from_paths(
            &input_dir,
            &output_dir,
            None,
            UnredactServiceConfig::default(),
        )
        .expect("batch run should succeed");

        assert_eq!(
            outputs.results.len(),
            1,
            "expected only one supported PDF input"
        );
        assert_eq!(outputs.success_count, 1, "expected one successful item");
        assert_eq!(outputs.failure_count, 0, "expected no failed items");

        let item = &outputs.results[0];
        assert_eq!(item.status, BatchFileStatus::Ok);
        assert!(item.input.ends_with(Path::new("a/b/report.pdf")));
        let guesses_path = item
            .guesses_path
            .as_ref()
            .expect("batch output should include guesses path");
        assert!(
            guesses_path.exists(),
            "expected output file {}",
            guesses_path.display()
        );
        assert!(guesses_path.ends_with(Path::new("a/b/report/report.guesses.json")));
    }

    #[test]
    fn run_from_paths_skips_diagnostics_by_default() {
        let input = Path::new("test_data/EFTA00101126.pdf");
        assert!(input.exists(), "missing test input: {}", input.display());

        let output_dir = test_dir("service_diagnostics_output");
        std::fs::create_dir_all(&output_dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));

        let outputs = run_from_paths(input, &output_dir, None, UnredactServiceConfig::default())
            .expect("service run should succeed");
        assert!(
            outputs.diagnostics_path.is_none(),
            "default service run should not write diagnostics"
        );
        assert!(
            outputs.visual_anchor_metrics_path.is_none(),
            "default service run should not write visual metrics"
        );
    }

    #[test]
    fn run_from_paths_with_diagnostics_writes_diagnostics_json() {
        let input = Path::new("test_data/EFTA00101126.pdf");
        assert!(input.exists(), "missing test input: {}", input.display());

        let output_dir = test_dir("service_diagnostics_output_opt_in");
        std::fs::create_dir_all(&output_dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));

        let outputs = run_from_paths_with_diagnostics(
            input,
            &output_dir,
            None,
            UnredactServiceConfig::default(),
            true,
        )
        .expect("service run should succeed");
        let diagnostics_path = outputs
            .diagnostics_path
            .as_ref()
            .expect("diagnostics path should be present when explicitly enabled");
        let visual_anchor_metrics_path = outputs
            .visual_anchor_metrics_path
            .as_ref()
            .expect("visual metrics path should be present when diagnostics are enabled");
        let visual_anchor_crops_dir = outputs
            .visual_anchor_crops_dir
            .as_ref()
            .expect("visual crop dir should be present when diagnostics are enabled");
        assert!(
            diagnostics_path.exists(),
            "expected diagnostics file {}",
            diagnostics_path.display()
        );
        assert!(
            visual_anchor_metrics_path.exists(),
            "expected visual metrics file {}",
            visual_anchor_metrics_path.display()
        );
        assert!(
            visual_anchor_crops_dir.exists(),
            "expected visual crop dir {}",
            visual_anchor_crops_dir.display()
        );
        let bytes = std::fs::read(diagnostics_path).unwrap_or_else(|error| {
            panic!(
                "failed to read diagnostics {}: {error}",
                diagnostics_path.display()
            )
        });
        let report = serde_json::from_slice::<DiagnosticReport>(&bytes).unwrap_or_else(|error| {
            panic!(
                "failed to parse diagnostics {}: {error}",
                diagnostics_path.display()
            )
        });
        assert!(
            !report.items.is_empty(),
            "expected non-empty diagnostics report"
        );
        assert!(
            report.items.iter().any(|item| item.code == "timing_ms"),
            "expected diagnostics to include timing records"
        );
        let crop_count = std::fs::read_dir(visual_anchor_crops_dir)
            .unwrap_or_else(|error| {
                panic!(
                    "failed to read {}: {error}",
                    visual_anchor_crops_dir.display()
                )
            })
            .count();
        assert!(crop_count > 0, "expected visual crop files");
    }
}

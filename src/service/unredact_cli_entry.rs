use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::logic::{
    build_output_file_paths, read_input_pdf_bytes, run_dictionary_list_convertion_component,
    run_redaction_guessing_component, run_visualization_render_component, write_encoded_outputs,
    BytesPipelineRequest, DictionaryListInput, DictionaryListRequest, OutputFilePaths,
    PipelineConfig, VisualizationRenderRequest,
};
use crate::types::guess_types::GuessConfig;
use crate::types::visualizer_config::VisualizerConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct UnredactServiceConfig {
    pub include_details: bool,
    pub enable_image_analysis: bool,
    pub raster_dpi: f32,
    pub guess: GuessConfig,
    pub visualize: bool,
    pub visualizer: VisualizerConfig,
}

impl Default for UnredactServiceConfig {
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
pub struct UnredactServiceRequest {
    pub input: PathBuf,
    pub output_dir: PathBuf,
    pub dictionary_path: Option<PathBuf>,
    pub cfg: UnredactServiceConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnredactServiceOutputs {
    pub redactions_path: PathBuf,
    pub fonts_path: PathBuf,
    pub guesses_path: PathBuf,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnredactBatchFileResult {
    pub input: PathBuf,
    pub status: BatchFileStatus,
    pub redactions_path: Option<PathBuf>,
    pub fonts_path: Option<PathBuf>,
    pub guesses_path: Option<PathBuf>,
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
    run(UnredactServiceRequest {
        input: input.to_path_buf(),
        output_dir: output_dir.to_path_buf(),
        dictionary_path: dictionary_path.map(PathBuf::from),
        cfg,
    })
}

#[inline]
pub fn run_batch_from_paths(
    input_dir: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: UnredactServiceConfig,
) -> Result<UnredactBatchOutputs, String> {
    run_batch(UnredactBatchRequest {
        input_dir: input_dir.to_path_buf(),
        output_dir: output_dir.to_path_buf(),
        dictionary_path: dictionary_path.map(PathBuf::from),
        cfg,
    })
}

#[inline]
pub fn run(req: UnredactServiceRequest) -> Result<UnredactServiceOutputs, String> {
    let pipeline_cfg = PipelineConfig {
        include_details: req.cfg.include_details,
        enable_image_analysis: req.cfg.enable_image_analysis,
        raster_dpi: req.cfg.raster_dpi,
        guess: req.cfg.guess,
        visualize: req.cfg.visualize,
        visualizer: req.cfg.visualizer,
    };
    let output_paths: OutputFilePaths = build_output_file_paths(&req.input, &req.output_dir)?;
    let dictionary_input = req
        .dictionary_path
        .clone()
        .map(DictionaryListInput::FilePath)
        .unwrap_or(DictionaryListInput::Missing);
    let dictionary_outputs =
        run_dictionary_list_convertion_component(DictionaryListRequest { dictionary_input })?;
    let bytes_req = BytesPipelineRequest {
        input_name: req.input.to_string_lossy().to_string(),
        pdf_bytes: read_input_pdf_bytes(&req.input)?,
        dictionary_entries: dictionary_outputs.dictionary_entries,
        dictionary_diagnostics: dictionary_outputs.dictionary_diagnostics,
        cfg: pipeline_cfg,
    };
    let mut bytes_outputs = run_redaction_guessing_component(bytes_req)?;
    let visualize_ms = if req.cfg.visualize {
        let visualize_started = Instant::now();
        let rendered = run_visualization_render_component(VisualizationRenderRequest {
            redactions: &bytes_outputs.redactions,
            guesses: &bytes_outputs.guesses,
            payload: bytes_outputs.visualization_payload.as_ref(),
            visualizer: req.cfg.visualizer,
        })?;
        bytes_outputs.visualized_pdf_bytes = rendered;
        visualize_started.elapsed().as_millis()
    } else {
        0_u128
    };
    bytes_outputs
        .guesses
        .diagnostics
        .push(format!("timing_ms stage=visualize value={visualize_ms}"));
    bytes_outputs.visualization_payload = None;
    let encoded_outputs = crate::logic::encode_outputs(&bytes_outputs)?;
    write_encoded_outputs(&output_paths, &encoded_outputs)?;
    Ok(UnredactServiceOutputs {
        redactions_path: output_paths.redactions_path,
        fonts_path: output_paths.fonts_path,
        guesses_path: output_paths.guesses_path,
        visualized_pdf_path: output_paths.visualized_pdf_path,
    })
}

#[inline]
pub fn run_batch(req: UnredactBatchRequest) -> Result<UnredactBatchOutputs, String> {
    if !req.input_dir.exists() {
        return Err(format!(
            "batch input directory does not exist: {}",
            req.input_dir.display()
        ));
    }
    if !req.input_dir.is_dir() {
        return Err(format!(
            "batch input path is not a directory: {}",
            req.input_dir.display()
        ));
    }
    let inputs = discover_inputs(&req.input_dir)?;
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
    )?;
    results.sort_by(|left, right| left.input.cmp(&right.input));

    let success_count = results
        .iter()
        .filter(|result| result.status == BatchFileStatus::Ok)
        .count();
    let failure_count = results.len().saturating_sub(success_count);
    let elapsed_ms = start.elapsed().as_millis();

    let manifest_path = req.output_dir.join("batch_manifest.json");
    if let Some(parent) = manifest_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
    }
    let payload = serde_json::to_vec_pretty(&UnredactBatchOutputs {
        results: results.clone(),
        success_count,
        failure_count,
        elapsed_ms,
        manifest_path: manifest_path.clone(),
    })
    .map_err(|error| format!("failed to encode batch manifest: {error}"))?;
    std::fs::write(&manifest_path, payload).map_err(|error| {
        format!(
            "failed to write manifest {}: {error}",
            manifest_path.display()
        )
    })?;

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
) -> Result<Vec<UnredactBatchFileResult>, String> {
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        let result = run_batch_item(input, input_dir, output_dir, dictionary_path, cfg);
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
) -> UnredactBatchFileResult {
    let started = Instant::now();
    let item_output_dir = match batch_output_dir_for_input(output_dir, input_dir, input) {
        Ok(path) => path,
        Err(error) => {
            return UnredactBatchFileResult {
                input: input.to_path_buf(),
                status: BatchFileStatus::Error,
                redactions_path: None,
                fonts_path: None,
                guesses_path: None,
                visualized_pdf_path: None,
                error: Some(error),
                elapsed_ms: started.elapsed().as_millis(),
            }
        }
    };

    let run = run_from_paths(input, &item_output_dir, dictionary_path, cfg.clone());
    match run {
        Ok(outputs) => UnredactBatchFileResult {
            input: input.to_path_buf(),
            status: BatchFileStatus::Ok,
            redactions_path: Some(outputs.redactions_path),
            fonts_path: Some(outputs.fonts_path),
            guesses_path: Some(outputs.guesses_path),
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
            visualized_pdf_path: None,
            error: Some(format!("{}: {error}", input.display())),
            elapsed_ms: started.elapsed().as_millis(),
        },
    }
}

fn batch_output_dir_for_input(
    output_root: &Path,
    input_root: &Path,
    input: &Path,
) -> Result<PathBuf, String> {
    let relative = input.strip_prefix(input_root).map_err(|error| {
        format!(
            "failed to map {} relative to {}: {error}",
            input.display(),
            input_root.display()
        )
    })?;
    let stem = relative
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("failed to get file stem for {}", input.display()))?;

    let mut out = output_root.to_path_buf();
    if let Some(parent) = relative.parent() {
        if !parent.as_os_str().is_empty() {
            out.push(parent);
        }
    }
    out.push(stem);
    std::fs::create_dir_all(&out).map_err(|error| {
        format!(
            "failed to create output directory {}: {error}",
            out.display()
        )
    })?;
    Ok(out)
}

fn discover_inputs(input_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut inputs = Vec::<PathBuf>::new();
    let mut dirs = vec![input_dir.to_path_buf()];

    while let Some(dir) = dirs.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|error| format!("failed to read {}: {error}", dir.display()))?;
        let mut sorted = entries.filter_map(|entry| entry.ok()).collect::<Vec<_>>();
        sorted.sort_by(|left, right| {
            left.file_name()
                .to_string_lossy()
                .cmp(&right.file_name().to_string_lossy())
        });

        for entry in sorted {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            if is_supported_batch_input(path.as_path()) {
                inputs.push(path);
            }
        }
    }

    inputs.sort();
    Ok(inputs)
}

fn is_supported_batch_input(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_batch_input_only_accepts_pdf_files() {
        assert!(is_supported_batch_input(Path::new("A.pdf")));
        assert!(is_supported_batch_input(Path::new("A.PDF")));
        assert!(!is_supported_batch_input(Path::new("A.txt")));
        assert!(!is_supported_batch_input(Path::new("A")));
    }

    #[test]
    fn batch_output_dir_for_input_preserves_relative_path() {
        let input_root = Path::new("C:/data/in");
        let output_root = std::env::temp_dir().join("unredact_batch_path_test");
        let input = Path::new("C:/data/in/a/b/report.pdf");
        let out =
            batch_output_dir_for_input(&output_root, input_root, input).expect("path should map");
        assert!(out.ends_with(Path::new("a/b/report")));
    }
}

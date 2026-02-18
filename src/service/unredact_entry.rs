use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::logic::{run_orchestrator, OrchestratorConfig, OrchestratorRequest};
use crate::types::guess_types::GuessConfig;
use crate::types::visualizer_config::VisualizerConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct UnredactServiceConfig {
    pub include_details: bool,
    pub include_full_page_rects: bool,
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
            include_full_page_rects: false,
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
    pub recursive: bool,
    pub glob_pattern: String,
    pub jobs: usize,
    pub fail_fast: bool,
    pub manifest_path: Option<PathBuf>,
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
    pub manifest_path: Option<PathBuf>,
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
#[allow(clippy::too_many_arguments)]
pub fn run_batch_from_paths(
    input_dir: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: UnredactServiceConfig,
    recursive: bool,
    glob_pattern: &str,
    jobs: usize,
    fail_fast: bool,
    manifest_path: Option<&Path>,
) -> Result<UnredactBatchOutputs, String> {
    run_batch(UnredactBatchRequest {
        input_dir: input_dir.to_path_buf(),
        output_dir: output_dir.to_path_buf(),
        dictionary_path: dictionary_path.map(PathBuf::from),
        cfg,
        recursive,
        glob_pattern: glob_pattern.to_owned(),
        jobs,
        fail_fast,
        manifest_path: manifest_path.map(PathBuf::from),
    })
}

#[inline]
pub fn run(req: UnredactServiceRequest) -> Result<UnredactServiceOutputs, String> {
    let orchestrator_req = OrchestratorRequest {
        input: req.input,
        output_dir: req.output_dir,
        dictionary_path: req.dictionary_path,
        cfg: OrchestratorConfig {
            include_details: req.cfg.include_details,
            include_full_page_rects: req.cfg.include_full_page_rects,
            enable_image_analysis: req.cfg.enable_image_analysis,
            raster_dpi: req.cfg.raster_dpi,
            guess: req.cfg.guess,
            visualize: req.cfg.visualize,
            visualizer: req.cfg.visualizer,
        },
    };
    let out = run_orchestrator(orchestrator_req)?;
    Ok(UnredactServiceOutputs {
        redactions_path: out.redactions_path,
        fonts_path: out.fonts_path,
        guesses_path: out.guesses_path,
        visualized_pdf_path: out.visualized_pdf_path,
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
    if req.glob_pattern.trim().is_empty() {
        return Err("glob_pattern must not be empty".to_owned());
    }
    let jobs = req.jobs.max(1);
    let inputs = discover_inputs(&req.input_dir, req.recursive, req.glob_pattern.trim())?;
    if inputs.is_empty() {
        return Err(format!(
            "no input files matched '{}' in {}",
            req.glob_pattern,
            req.input_dir.display()
        ));
    }

    let start = Instant::now();
    let mut results = if jobs == 1 || inputs.len() == 1 {
        run_batch_serial(
            &inputs,
            &req.input_dir,
            &req.output_dir,
            req.dictionary_path.as_deref(),
            &req.cfg,
            req.fail_fast,
        )?
    } else {
        run_batch_parallel(
            &inputs,
            &req.input_dir,
            &req.output_dir,
            req.dictionary_path.as_deref(),
            &req.cfg,
            req.fail_fast,
            jobs,
        )?
    };
    results.sort_by(|left, right| left.input.cmp(&right.input));

    let success_count = results
        .iter()
        .filter(|result| result.status == BatchFileStatus::Ok)
        .count();
    let failure_count = results.len().saturating_sub(success_count);
    let elapsed_ms = start.elapsed().as_millis();

    let manifest_path = req.manifest_path.clone();
    if let Some(path) = &manifest_path {
        if let Some(parent) = path.parent() {
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
            manifest_path: Some(path.clone()),
        })
        .map_err(|error| format!("failed to encode batch manifest: {error}"))?;
        std::fs::write(path, payload)
            .map_err(|error| format!("failed to write manifest {}: {error}", path.display()))?;
    }

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
    fail_fast: bool,
) -> Result<Vec<UnredactBatchFileResult>, String> {
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        let result = run_batch_item(input, input_dir, output_dir, dictionary_path, cfg);
        let is_error = result.status == BatchFileStatus::Error;
        out.push(result);
        if fail_fast && is_error {
            let message = out
                .last()
                .and_then(|value| value.error.clone())
                .unwrap_or_else(|| "batch item failed".to_owned());
            return Err(message);
        }
    }
    Ok(out)
}

fn run_batch_parallel(
    inputs: &[PathBuf],
    input_dir: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: &UnredactServiceConfig,
    fail_fast: bool,
    jobs: usize,
) -> Result<Vec<UnredactBatchFileResult>, String> {
    let input_values = Arc::new(inputs.to_vec());
    let next_index = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let first_error = Arc::new(Mutex::new(None::<String>));
    let results = Arc::new(Mutex::new(Vec::<UnredactBatchFileResult>::new()));
    let input_root = Arc::new(input_dir.to_path_buf());
    let output_root = Arc::new(output_dir.to_path_buf());
    let dictionary_value = dictionary_path.map(PathBuf::from);
    let cfg_value = cfg.clone();

    let mut workers = Vec::with_capacity(jobs);
    for _ in 0..jobs {
        let input_values = Arc::clone(&input_values);
        let next_index = Arc::clone(&next_index);
        let stop = Arc::clone(&stop);
        let first_error = Arc::clone(&first_error);
        let results = Arc::clone(&results);
        let input_root = Arc::clone(&input_root);
        let output_root = Arc::clone(&output_root);
        let dictionary_value = dictionary_value.clone();
        let cfg_value = cfg_value.clone();
        let worker = std::thread::spawn(move || loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let index = next_index.fetch_add(1, Ordering::SeqCst);
            if index >= input_values.len() {
                break;
            }
            let input = &input_values[index];
            let result = run_batch_item(
                input,
                &input_root,
                &output_root,
                dictionary_value.as_deref(),
                &cfg_value,
            );
            if result.status == BatchFileStatus::Error && fail_fast {
                stop.store(true, Ordering::Relaxed);
                if let Ok(mut guard) = first_error.lock() {
                    if guard.is_none() {
                        *guard = result.error.clone();
                    }
                }
            }
            if let Ok(mut guard) = results.lock() {
                guard.push(result);
            }
        });
        workers.push(worker);
    }

    for worker in workers {
        worker
            .join()
            .map_err(|_| "batch worker thread panicked".to_owned())?;
    }

    if fail_fast {
        if let Ok(guard) = first_error.lock() {
            if let Some(message) = guard.clone() {
                return Err(message);
            }
        }
    }

    let values = results
        .lock()
        .map_err(|_| "failed to lock batch results".to_owned())?
        .clone();
    Ok(values)
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

fn discover_inputs(
    input_dir: &Path,
    recursive: bool,
    glob_pattern: &str,
) -> Result<Vec<PathBuf>, String> {
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
                if recursive {
                    dirs.push(path);
                }
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if wildcard_match(glob_pattern, file_name) {
                inputs.push(path);
            }
        }
    }

    inputs.sort();
    Ok(inputs)
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern_bytes = pattern.as_bytes();
    let value_bytes = value.as_bytes();
    let mut p = 0_usize;
    let mut v = 0_usize;
    let mut star = None::<usize>;
    let mut match_index = 0_usize;

    while v < value_bytes.len() {
        if p < pattern_bytes.len()
            && (pattern_bytes[p] == b'?' || pattern_bytes[p].eq_ignore_ascii_case(&value_bytes[v]))
        {
            p += 1;
            v += 1;
            continue;
        }
        if p < pattern_bytes.len() && pattern_bytes[p] == b'*' {
            star = Some(p);
            p += 1;
            match_index = v;
            continue;
        }
        let Some(star_index) = star else {
            return false;
        };
        p = star_index + 1;
        match_index += 1;
        v = match_index;
    }

    while p < pattern_bytes.len() && pattern_bytes[p] == b'*' {
        p += 1;
    }
    p == pattern_bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_match_supports_star_and_question_mark() {
        assert!(wildcard_match("*.pdf", "A.pdf"));
        assert!(wildcard_match("EFTA????????.pdf", "EFTA00101126.pdf"));
        assert!(wildcard_match("report-??.json", "report-01.json"));
        assert!(!wildcard_match("*.pdf", "A.txt"));
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

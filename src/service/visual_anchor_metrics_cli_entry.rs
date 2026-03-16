use std::path::{Path, PathBuf};

use crate::logic::{
    build_visual_metric_file_paths, discover_pdf_inputs, ensure_batch_output_dir_for_input,
    read_input_pdf_bytes, run_redaction_guessing_component, validate_batch_input_directory,
    write_visual_metric_outputs, BytesPipelineRequest, PipelineConfig, PipelineExecutionOptions,
};

#[derive(Debug, Clone, PartialEq)]
pub struct VisualAnchorMetricServiceRequest {
    pub input: PathBuf,
    pub output_dir: PathBuf,
    pub compact: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualAnchorMetricServiceOutputs {
    pub report_paths: Vec<PathBuf>,
}

#[inline]
pub fn run(
    req: VisualAnchorMetricServiceRequest,
) -> Result<VisualAnchorMetricServiceOutputs, String> {
    if req.input.is_dir() {
        return run_batch(req);
    }
    let report_path = run_single(
        req.input.as_path(),
        single_output_dir(req.input.as_path(), req.output_dir.as_path()).as_path(),
        req.compact,
    )?;
    Ok(VisualAnchorMetricServiceOutputs {
        report_paths: vec![report_path],
    })
}

fn run_batch(
    req: VisualAnchorMetricServiceRequest,
) -> Result<VisualAnchorMetricServiceOutputs, String> {
    validate_batch_input_directory(&req.input)?;
    let inputs = discover_pdf_inputs(&req.input)?;
    if inputs.is_empty() {
        return Err(format!(
            "no supported input files found in {}",
            req.input.display()
        ));
    }
    let mut report_paths = Vec::<PathBuf>::new();
    for input in &inputs {
        let pdf_output_dir = ensure_batch_output_dir_for_input(
            req.output_dir.as_path(),
            req.input.as_path(),
            input,
        )?;
        report_paths.push(run_single(
            input.as_path(),
            pdf_output_dir.as_path(),
            req.compact,
        )?);
    }
    report_paths.sort();
    Ok(VisualAnchorMetricServiceOutputs { report_paths })
}

fn run_single(input: &Path, output_dir: &Path, compact: bool) -> Result<PathBuf, String> {
    let outputs = run_redaction_guessing_component(BytesPipelineRequest {
        input_name: input.to_string_lossy().to_string(),
        pdf_bytes: read_input_pdf_bytes(input)?,
        dictionary_bytes: None,
        cfg: visual_metrics_config(),
        execution: PipelineExecutionOptions {
            collect_diagnostics: true,
        },
    })?;
    let report = outputs
        .visual_anchor_metrics
        .as_ref()
        .ok_or_else(|| "visual anchor metrics report missing".to_owned())?;
    let crops = outputs
        .visual_anchor_crops
        .as_ref()
        .ok_or_else(|| "visual anchor crops missing".to_owned())?;
    let report_json = if compact {
        serde_json::to_vec(report)
    } else {
        serde_json::to_vec_pretty(report)
    }
    .map_err(|error| format!("failed to encode visual metrics json: {error}"))?;
    let paths = build_visual_metric_file_paths(input, output_dir)?;
    write_visual_metric_outputs(&paths, report_json.as_slice(), crops.as_slice())?;
    Ok(paths.report_path)
}

fn single_output_dir(input: &Path, output_root: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("output");
    output_root.join(stem)
}

fn visual_metrics_config() -> PipelineConfig {
    PipelineConfig {
        visualize: false,
        ..PipelineConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::single_output_dir;

    #[test]
    fn single_output_dir_uses_file_stem() {
        let output = single_output_dir(Path::new("docs/sample.pdf"), Path::new("out"));
        assert_eq!(output, Path::new("out").join("sample"));
    }
}

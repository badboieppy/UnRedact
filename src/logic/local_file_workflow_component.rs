use std::path::{Path, PathBuf};

use crate::data::local_file_workflow_data::LocalFileWorkflowData;
use crate::data::result_data_publisher::{
    ResultDataPublisher, ResultPublishPaths, ResultPublishPayload, ResultPublishRequest,
};

use super::types::EncodedPipelineOutputs;

const BATCH_MANIFEST_NAME: &str = "batch_manifest.json";

#[derive(Debug, Clone, PartialEq)]
pub struct OutputFilePaths {
    pub redactions_path: PathBuf,
    pub fonts_path: PathBuf,
    pub guesses_path: PathBuf,
    pub anchors_path: PathBuf,
    pub diagnostics_path: Option<PathBuf>,
    pub visualized_pdf_path: Option<PathBuf>,
}

#[inline]
pub fn read_input_pdf_bytes(input: &Path) -> Result<Vec<u8>, String> {
    let local_data = LocalFileWorkflowData::new();
    local_data.read_bytes(input)
}

#[inline]
pub fn build_output_file_paths(
    input: &Path,
    output_dir: &Path,
    include_diagnostics: bool,
) -> Result<OutputFilePaths, String> {
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "input file has no stem".to_owned())?;
    Ok(OutputFilePaths {
        redactions_path: output_dir.join(format!("{stem}.redactions.json")),
        fonts_path: output_dir.join(format!("{stem}.fonts.json")),
        guesses_path: output_dir.join(format!("{stem}.guesses.json")),
        anchors_path: output_dir.join(format!("{stem}.anchors.json")),
        diagnostics_path: include_diagnostics
            .then(|| output_dir.join(format!("{stem}.diagnostics.json"))),
        visualized_pdf_path: Some(output_dir.join(format!("{stem}.visualized.pdf"))),
    })
}

#[inline]
pub fn write_encoded_outputs(
    output_paths: &OutputFilePaths,
    encoded: &EncodedPipelineOutputs,
) -> Result<(), String> {
    let publisher = ResultDataPublisher::new();
    publisher.publish(ResultPublishRequest {
        paths: ResultPublishPaths {
            redactions_path: output_paths.redactions_path.as_path(),
            fonts_path: output_paths.fonts_path.as_path(),
            guesses_path: output_paths.guesses_path.as_path(),
            anchors_path: output_paths.anchors_path.as_path(),
            diagnostics_path: output_paths.diagnostics_path.as_deref(),
            visualized_pdf_path: output_paths.visualized_pdf_path.as_deref(),
        },
        payload: ResultPublishPayload {
            redactions_json: encoded.redactions_json.as_slice(),
            fonts_json: encoded.fonts_json.as_slice(),
            guesses_json: encoded.guesses_json.as_slice(),
            anchors_json: encoded.anchors_json.as_slice(),
            diagnostics_json: encoded.diagnostics_json.as_deref(),
            visualized_pdf_bytes: encoded.visualized_pdf_bytes.as_deref(),
        },
    })
}

#[inline]
pub fn read_dictionary_input(dictionary_path: Option<&Path>) -> Result<Option<Vec<u8>>, String> {
    let path = match dictionary_path {
        Some(path) => path,
        None => return Ok(None),
    };
    let local_data = LocalFileWorkflowData::new();
    let bytes = local_data.read_bytes(path)?;
    Ok(Some(bytes))
}

#[inline]
pub fn validate_batch_input_directory(input_dir: &Path) -> Result<(), String> {
    let local_data = LocalFileWorkflowData::new();
    let exists = local_data.exists(input_dir)?;
    if !exists {
        return Err(format!(
            "batch input directory does not exist: {}",
            input_dir.display()
        ));
    }
    if !local_data.is_dir(input_dir)? {
        return Err(format!(
            "batch input path is not a directory: {}",
            input_dir.display()
        ));
    }
    Ok(())
}

#[inline]
pub fn discover_pdf_inputs(input_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let local_data = LocalFileWorkflowData::new();
    let mut inputs = Vec::<PathBuf>::new();
    let mut dirs = vec![input_dir.to_path_buf()];

    while let Some(dir) = dirs.pop() {
        let mut entries = local_data.read_dir_paths(&dir)?;
        entries.sort_by(|left, right| {
            left.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .cmp(&right.file_name().unwrap_or_default().to_string_lossy())
        });
        for path in entries {
            if local_data.is_dir(&path)? {
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

#[inline]
pub fn ensure_batch_output_dir_for_input(
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

    let local_data = LocalFileWorkflowData::new();
    local_data.create_dir_all(&out)?;
    Ok(out)
}

#[inline]
pub fn write_batch_manifest(output_dir: &Path, payload: &[u8]) -> Result<PathBuf, String> {
    let manifest_path = output_dir.join(BATCH_MANIFEST_NAME);
    let publisher = ResultDataPublisher::new();
    publisher.publish_bytes(&manifest_path, payload)?;
    Ok(manifest_path)
}

#[inline]
fn is_supported_batch_input(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::build_output_file_paths;

    #[test]
    fn build_output_file_paths_uses_stem_and_dir() {
        let input = Path::new("C:/data/report.pdf");
        let output_dir = std::env::temp_dir().join("unredact_output_path_test");
        let out = build_output_file_paths(input, &output_dir, true).expect("expected output paths");
        assert_eq!(
            out.redactions_path,
            output_dir.join("report.redactions.json")
        );
        assert_eq!(out.fonts_path, output_dir.join("report.fonts.json"));
        assert_eq!(out.guesses_path, output_dir.join("report.guesses.json"));
        assert_eq!(out.anchors_path, output_dir.join("report.anchors.json"));
        assert_eq!(
            out.diagnostics_path,
            Some(output_dir.join("report.diagnostics.json"))
        );
        assert_eq!(
            out.visualized_pdf_path,
            Some(output_dir.join("report.visualized.pdf"))
        );
    }
}

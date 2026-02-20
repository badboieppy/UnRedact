use std::path::{Path, PathBuf};

use crate::data::{LocalFileWorkflowData, ResultDataPublisher};

use super::dictionary_list_convertion_component::DictionaryListInput;

const BATCH_MANIFEST_NAME: &str = "batch_manifest.json";

#[inline]
pub fn read_dictionary_input(
    dictionary_path: Option<&Path>,
) -> Result<DictionaryListInput, String> {
    let path = match dictionary_path {
        Some(path) => path,
        None => return Ok(DictionaryListInput::Missing),
    };
    let local_data = LocalFileWorkflowData::new();
    let bytes = local_data.read_bytes(path)?;
    Ok(DictionaryListInput::FileBytes(bytes))
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

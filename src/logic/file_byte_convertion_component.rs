use std::path::Path;

use crate::data::redactions_data::RedactionsData;
use crate::data::{
    ResultDataPublisher, ResultPublishPaths, ResultPublishPayload, ResultPublishRequest,
};
use crate::logic::types::{BytesPipelineOutputs, OutputFilePaths};

#[derive(Debug, Clone, PartialEq)]
pub struct EncodedPipelineOutputs {
    pub redactions_json: Vec<u8>,
    pub fonts_json: Vec<u8>,
    pub guesses_json: Vec<u8>,
    pub visualized_pdf_bytes: Option<Vec<u8>>,
}

#[inline]
pub fn read_input_pdf_bytes(input: &Path) -> Result<Vec<u8>, String> {
    let redactions_data = RedactionsData::new();
    redactions_data.read_input_bytes(input)
}

#[inline]
pub fn build_output_file_paths(input: &Path, output_dir: &Path) -> Result<OutputFilePaths, String> {
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "input file has no stem".to_owned())?;
    Ok(OutputFilePaths {
        redactions_path: output_dir.join(format!("{stem}.redactions.json")),
        fonts_path: output_dir.join(format!("{stem}.fonts.json")),
        guesses_path: output_dir.join(format!("{stem}.guesses.json")),
        visualized_pdf_path: Some(output_dir.join(format!("{stem}.visualized.pdf"))),
    })
}

#[inline]
pub fn encode_outputs(outputs: &BytesPipelineOutputs) -> Result<EncodedPipelineOutputs, String> {
    let redactions_json = serde_json::to_vec_pretty(&outputs.redactions)
        .map_err(|error| format!("failed to encode redactions json: {error}"))?;
    let fonts_json = serde_json::to_vec_pretty(&outputs.fonts)
        .map_err(|error| format!("failed to encode fonts json: {error}"))?;
    let guesses_json = serde_json::to_vec_pretty(&outputs.guesses)
        .map_err(|error| format!("failed to encode guesses json: {error}"))?;
    Ok(EncodedPipelineOutputs {
        redactions_json,
        fonts_json,
        guesses_json,
        visualized_pdf_bytes: outputs.visualized_pdf_bytes.clone(),
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
            visualized_pdf_path: output_paths.visualized_pdf_path.as_deref(),
        },
        payload: ResultPublishPayload {
            redactions_json: encoded.redactions_json.as_slice(),
            fonts_json: encoded.fonts_json.as_slice(),
            guesses_json: encoded.guesses_json.as_slice(),
            visualized_pdf_bytes: encoded.visualized_pdf_bytes.as_deref(),
        },
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::build_output_file_paths;

    #[test]
    fn build_output_file_paths_uses_stem_and_dir() {
        let input = Path::new("C:/data/report.pdf");
        let output_dir = std::env::temp_dir().join("unredact_output_path_test");
        let out = build_output_file_paths(input, &output_dir).expect("expected output paths");
        assert_eq!(
            out.redactions_path,
            output_dir.join("report.redactions.json")
        );
        assert_eq!(out.fonts_path, output_dir.join("report.fonts.json"));
        assert_eq!(out.guesses_path, output_dir.join("report.guesses.json"));
        assert_eq!(
            out.visualized_pdf_path,
            Some(output_dir.join("report.visualized.pdf"))
        );
    }
}

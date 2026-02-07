use crate::font_detection::data::file_data_builder::FileDataBuilder;
use crate::font_detection::dependency::file_accessor::FileAccessor;
use crate::font_detection::logic::file_font_process::{
    encode_report, process_many, validate_inputs, FontProcessConfig,
};
use crate::font_detection::logic::types::file_types::{
    EncodedOutput, FontDetectionReport, FontProcessInput, OutputFormat,
};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectFontsRequest {
    pub inputs: Vec<PathBuf>,
    pub include_details: bool,
}

impl From<FontProcessInput> for DetectFontsRequest {
    fn from(value: FontProcessInput) -> Self {
        Self {
            inputs: value.inputs,
            include_details: value.include_details,
        }
    }
}

pub fn detect_fonts(
    accessor: &dyn FileAccessor,
    req: DetectFontsRequest,
) -> Result<FontDetectionReport, String> {
    validate_inputs(&req.inputs)?;
    let builder = FileDataBuilder::new(accessor);

    process_many(
        &builder,
        &req.inputs,
        FontProcessConfig {
            include_details: req.include_details,
        },
    )
}

pub fn detect_fonts_encoded(
    accessor: &dyn FileAccessor,
    req: DetectFontsRequest,
    format: OutputFormat,
) -> Result<EncodedOutput, String> {
    let report = detect_fonts(accessor, req)?;
    let bytes = encode_report(&report, format)?;
    Ok(EncodedOutput {
        path: None,
        format,
        bytes,
    })
}

pub fn run_font_detection(
    accessor: &dyn FileAccessor,
    input: FontProcessInput,
) -> Result<EncodedOutput, String> {
    let format = input.format;
    let path = input.output.clone();

    let req: DetectFontsRequest = input.into();
    let mut out = detect_fonts_encoded(accessor, req, format)?;
    out.path = path;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_detection::dependency::file_accessor::{FileReadRequest, FileReadResponse};
    use std::collections::BTreeMap;
    use std::path::Path;

    struct FakeAccessor {
        files: BTreeMap<String, Vec<u8>>,
    }

    impl FakeAccessor {
        fn new(files: BTreeMap<String, Vec<u8>>) -> Self {
            Self { files }
        }
    }

    impl FileAccessor for FakeAccessor {
        fn read(&self, req: FileReadRequest) -> Result<FileReadResponse, String> {
            let key = req.path.to_string_lossy().to_string();
            let bytes = self
                .files
                .get(&key)
                .cloned()
                .ok_or_else(|| "not found".to_owned())?;
            Ok(FileReadResponse { bytes })
        }
    }

    #[test]
    fn detect_fonts_rejects_empty_inputs() {
        let accessor = FakeAccessor::new(BTreeMap::new());
        let err = detect_fonts(
            &accessor,
            DetectFontsRequest {
                inputs: vec![],
                include_details: false,
            },
        )
        .expect_err("expected error in test");

        assert_eq!(err, "no inputs provided".to_owned());
    }

    #[test]
    fn detect_fonts_encoded_sets_format_and_bytes_nonempty_json() {
        let accessor = FakeAccessor::new(BTreeMap::new());

        let out = detect_fonts_encoded(
            &accessor,
            DetectFontsRequest {
                inputs: vec![PathBuf::from("x.bin")],
                include_details: false,
            },
            OutputFormat::Json,
        )
        .expect("expected value in test");

        assert_eq!(out.format, OutputFormat::Json);
        assert_eq!(out.path, None);

        let s = String::from_utf8(out.bytes).expect("expected value in test");
        assert!(s.ends_with('\n'));
        assert!(s.contains("\"inputs\""));
    }

    #[test]
    fn run_font_detection_preserves_output_path() {
        let accessor = FakeAccessor::new(BTreeMap::new());

        let out = run_font_detection(
            &accessor,
            FontProcessInput {
                inputs: vec![PathBuf::from("x.bin")],
                output: Some(PathBuf::from("out.json")),
                format: OutputFormat::Json,
                include_details: false,
            },
        )
        .expect("expected value in test");

        assert_eq!(out.path, Some(PathBuf::from("out.json")));
        assert_eq!(out.format, OutputFormat::Json);
    }

    #[test]
    fn detect_fonts_accepts_single_unknown_path() {
        let accessor = FakeAccessor::new(BTreeMap::new());

        let report = detect_fonts(
            &accessor,
            DetectFontsRequest {
                inputs: vec![PathBuf::from("x.bin")],
                include_details: false,
            },
        )
        .expect("expected value in test");

        assert_eq!(report.inputs.len(), 1);
        assert_eq!(report.inputs[0].path, "x.bin".to_owned());
    }

    #[test]
    fn run_font_detection_accepts_path_reference() {
        let accessor = FakeAccessor::new(BTreeMap::new());

        let report = detect_fonts(
            &accessor,
            DetectFontsRequest {
                inputs: vec![PathBuf::from("x.bin")],
                include_details: false,
            },
        )
        .expect("expected value in test");

        assert_eq!(report.inputs.len(), 1);
        assert_eq!(
            Path::new("x.bin").to_string_lossy().to_string(),
            "x.bin".to_owned()
        );
    }
}


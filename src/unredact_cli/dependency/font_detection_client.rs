use std::path::Path;

use crate::font_detection::dependency::file_accessor::OsFileAccessor;
use crate::font_detection::logic::types::file_types::{
    FontDetectionReport, FontProcessInput, OutputFormat,
};
use crate::font_detection::service::entry::run_font_detection;

#[derive(Debug, Clone, Copy)]
pub struct FontDetectionClient;

impl FontDetectionClient {
    #[inline]
    pub fn detect_fonts(
        &self,
        input: &Path,
        include_details: bool,
    ) -> Result<FontDetectionReport, String> {
        let accessor = OsFileAccessor;
        let out = run_font_detection(
            &accessor,
            FontProcessInput {
                inputs: vec![input.to_path_buf()],
                output: None,
                format: OutputFormat::Json,
                include_details,
            },
        )?;
        serde_json::from_slice(&out.bytes).map_err(|e| format!("failed to decode fonts: {e}"))
    }
}

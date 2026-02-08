use std::path::Path;

use crate::font_detection::logic::types::file_types::FontDetectionReport;
use crate::unredact_orchestrator::dependency::{FileStore, FontDetectionClient};

#[derive(Debug, Clone, Copy)]
pub struct FontData {
    file_store: FileStore,
    client: FontDetectionClient,
}

impl FontData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
            client: FontDetectionClient,
        }
    }

    #[inline]
    pub fn detect_fonts(
        &self,
        input: &Path,
        include_details: bool,
    ) -> Result<FontDetectionReport, String> {
        self.client.detect_fonts(input, include_details)
    }

    #[inline]
    pub fn write_fonts(&self, path: &Path, report: &FontDetectionReport) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(report)
            .map_err(|e| format!("failed to encode fonts json: {e}"))?;
        self.file_store.write(path, &json)
    }
}

impl Default for FontData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

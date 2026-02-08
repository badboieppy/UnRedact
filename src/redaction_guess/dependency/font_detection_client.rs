use std::path::Path;

use crate::font_detection::dependency::file_accessor::OsFileAccessor;
use crate::font_detection::logic::types::file_types::FontRunReport;
use crate::font_detection::service::entry::detect_font_runs;

#[derive(Debug, Clone, Copy)]
pub struct FontDetectionClient;

impl FontDetectionClient {
    #[inline]
    pub fn detect_font_runs(&self, path: &Path) -> Result<FontRunReport, String> {
        let accessor = OsFileAccessor::new();
        detect_font_runs(&accessor, path)
    }
}

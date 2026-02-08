use std::path::Path;

use crate::font_detection::logic::types::file_types::FontRunReport;
use crate::redaction_guess::dependency::FontDetectionClient;

#[derive(Debug, Clone)]
pub struct FontRunInputs {
    pub report: FontRunReport,
}

pub trait FontRunDataSource {
    fn load_font_runs(&self, pdf_path: &Path) -> Result<FontRunInputs, String>;
}

#[derive(Debug, Clone, Copy)]
pub struct FontRunData {
    client: FontDetectionClient,
}

impl FontRunData {
    #[inline]
    pub fn new() -> Self {
        Self {
            client: FontDetectionClient,
        }
    }
}

impl Default for FontRunData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl FontRunDataSource for FontRunData {
    #[inline]
    fn load_font_runs(&self, pdf_path: &Path) -> Result<FontRunInputs, String> {
        let report = self.client.detect_font_runs(pdf_path)?;
        Ok(FontRunInputs { report })
    }
}

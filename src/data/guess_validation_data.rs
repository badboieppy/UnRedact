use std::path::Path;

use crate::dependency::file_store::FileStore;
use crate::types::file_types::FontDetectionReport;
use crate::types::guess_types::GuessReport;
use crate::types::redaction_types::RedactionReport;

#[derive(Debug, Clone)]
pub struct GuessReportInputs {
    pub redactions: RedactionReport,
    pub fonts: FontDetectionReport,
    pub diagnostics: Vec<String>,
}

pub trait ReportDataSource {
    fn load_reports(
        &self,
        redactions_path: &Path,
        fonts_path: &Path,
    ) -> Result<GuessReportInputs, String>;
}

#[derive(Debug, Clone, Copy)]
pub struct GuessValidationData {
    file_store: FileStore,
}

impl GuessValidationData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
        }
    }

    #[inline]
    pub fn write_guesses(&self, path: &Path, report: &GuessReport) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(report)
            .map_err(|e| format!("failed to encode guesses json: {e}"))?;
        self.file_store.write(path, &json)
    }
}

impl Default for GuessValidationData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
pub fn load_reports(
    file_store: &FileStore,
    redactions_path: &Path,
    fonts_path: &Path,
) -> Result<GuessReportInputs, String> {
    let redactions_bytes = file_store.read(redactions_path)?;
    let fonts_bytes = file_store.read(fonts_path)?;

    let redactions: RedactionReport = serde_json::from_slice(&redactions_bytes)
        .map_err(|e| format!("failed to decode redactions: {e}"))?;
    let fonts: FontDetectionReport =
        serde_json::from_slice(&fonts_bytes).map_err(|e| format!("failed to decode fonts: {e}"))?;

    Ok(GuessReportInputs {
        redactions,
        fonts,
        diagnostics: Vec::new(),
    })
}

impl ReportDataSource for GuessValidationData {
    #[inline]
    fn load_reports(
        &self,
        redactions_path: &Path,
        fonts_path: &Path,
    ) -> Result<GuessReportInputs, String> {
        load_reports(&self.file_store, redactions_path, fonts_path)
    }
}

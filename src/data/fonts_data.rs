use std::path::Path;

use crate::dependency::file_store::FileStore;
use crate::dependency::pdf_font_occurrence_accessor::{
    build_file_font_report, build_file_font_report_from_bytes, DataBuildConfig, FileDataBuilder,
};
use crate::dependency::pdf_font_run_accessor::build_font_run_report;
use crate::types::file_types::{
    aggregate_counts, distinct_fonts_from_counts, FileFontReport, FontDetectionReport,
    FontRunReport, FontsFound,
};

#[derive(Debug, Clone)]
pub struct FontRunInputs {
    pub report: FontRunReport,
}

pub trait FontRunDataSource {
    fn load_font_runs(&self, pdf_path: &Path) -> Result<FontRunInputs, String>;
}

#[derive(Debug, Clone, Copy)]
pub struct FontsData {
    file_store: FileStore,
}

impl FontsData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
        }
    }

    #[inline]
    pub fn detect_fonts(
        &self,
        input: &Path,
        include_details: bool,
    ) -> Result<FontDetectionReport, String> {
        let builder = FileDataBuilder::new(&self.file_store);
        let report = build_file_font_report(&builder, input, DataBuildConfig { include_details })?;
        Ok(FontDetectionReport {
            inputs: vec![finalize_file_font_report(report, include_details)],
        })
    }

    #[inline]
    pub fn detect_fonts_from_bytes(
        &self,
        input_name: &str,
        input_bytes: &[u8],
        include_details: bool,
    ) -> Result<FontDetectionReport, String> {
        let report = build_file_font_report_from_bytes(
            input_name,
            input_bytes,
            DataBuildConfig { include_details },
        )?;
        Ok(FontDetectionReport {
            inputs: vec![finalize_file_font_report(report, include_details)],
        })
    }

    #[inline]
    pub fn write_fonts(&self, path: &Path, report: &FontDetectionReport) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(report)
            .map_err(|e| format!("failed to encode fonts json: {e}"))?;
        self.file_store.write(path, &json)
    }

    #[inline]
    pub fn load_font_runs_from_bytes(
        &self,
        input_name: &str,
        pdf_bytes: &[u8],
    ) -> Result<FontRunInputs, String> {
        let report = build_font_run_report(Path::new(input_name), pdf_bytes)?;
        Ok(FontRunInputs { report })
    }
}

impl Default for FontsData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl FontRunDataSource for FontsData {
    #[inline]
    fn load_font_runs(&self, pdf_path: &Path) -> Result<FontRunInputs, String> {
        let bytes = self.file_store.read(pdf_path)?;
        self.load_font_runs_from_bytes(&pdf_path.to_string_lossy(), &bytes)
    }
}

#[inline]
fn finalize_file_font_report(report: FileFontReport, include_details: bool) -> FileFontReport {
    let extracted = report.occurrences.clone();
    let counts = extracted
        .as_ref()
        .map(|items| aggregate_counts(&items.items))
        .unwrap_or_default();
    let distinct = distinct_fonts_from_counts(&counts);
    let occurrences = include_details.then_some(extracted).flatten();
    FileFontReport {
        fonts: FontsFound { distinct, counts },
        occurrences,
        ..report
    }
}

#[cfg(test)]
mod tests {
    use super::FontsData;

    #[test]
    fn detect_fonts_from_bytes_hides_occurrences_when_details_are_disabled() {
        let input_path = std::path::Path::new("test_data/EFTA00101126.pdf");
        let bytes = std::fs::read(input_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", input_path.display()));
        let data = FontsData::new();

        let report = data
            .detect_fonts_from_bytes("EFTA00101126.pdf", &bytes, false)
            .expect("font detection should succeed");

        assert_eq!(report.inputs.len(), 1);
        let file = &report.inputs[0];
        assert!(file.occurrences.is_none());
        assert!(
            !file.fonts.distinct.is_empty(),
            "expected distinct fonts in detected report"
        );
    }
}

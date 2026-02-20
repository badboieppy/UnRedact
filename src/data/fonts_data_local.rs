use std::path::Path;

use crate::dependency::file_store::FileStore;
use crate::dependency::pdf_font_occurrence_accessor::{build_file_font_report, FileDataBuilder};
use crate::types::file_types::FontDetectionReport;

use super::fonts_data::{finalize_file_font_report, FontRunInputs, FontsData};

pub trait FontRunDataSource {
    fn load_font_runs(&self, pdf_path: &Path) -> Result<FontRunInputs, String>;
}

impl FontsData {
    #[inline]
    pub fn detect_fonts(
        &self,
        input: &Path,
        include_details: bool,
    ) -> Result<FontDetectionReport, String> {
        let file_store = FileStore;
        let builder = FileDataBuilder::new(&file_store);
        let report = build_file_font_report(
            &builder,
            input,
            crate::dependency::pdf_font_occurrence_accessor::DataBuildConfig { include_details },
        )?;
        Ok(FontDetectionReport {
            inputs: vec![finalize_file_font_report(report, include_details)],
        })
    }

    #[inline]
    pub fn write_fonts(&self, path: &Path, report: &FontDetectionReport) -> Result<(), String> {
        let file_store = FileStore;
        let json = serde_json::to_vec_pretty(report)
            .map_err(|e| format!("failed to encode fonts json: {e}"))?;
        file_store.write(path, &json)
    }

    #[inline]
    pub fn load_font_runs(&self, pdf_path: &Path) -> Result<FontRunInputs, String> {
        let file_store = FileStore;
        let bytes = file_store.read(pdf_path)?;
        self.load_font_runs_from_bytes(&pdf_path.to_string_lossy(), &bytes)
    }
}

impl FontRunDataSource for FontsData {
    #[inline]
    fn load_font_runs(&self, pdf_path: &Path) -> Result<FontRunInputs, String> {
        FontsData::load_font_runs(self, pdf_path)
    }
}

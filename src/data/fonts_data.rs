use crate::dependency::pdf_font_occurrence_accessor::{
    build_file_font_report_from_bytes, DataBuildConfig,
};
use crate::dependency::pdf_font_run_accessor::build_font_run_report_from_input_name;
use crate::dependency::pdf_font_run_types::PdfFontRunReport;
use crate::types::file_types::{
    aggregate_counts, distinct_fonts_from_counts, FileFontReport, FontDetectionReport,
    FontRunReport, FontsFound,
};

#[derive(Debug, Clone)]
pub struct FontRunInputs {
    pub report: FontRunReport,
    pub(crate) pdf_report: PdfFontRunReport,
}

#[derive(Debug, Clone, Copy)]
pub struct FontsData;

impl FontsData {
    #[inline]
    pub fn new() -> Self {
        Self
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
    pub fn load_font_runs_from_bytes(
        &self,
        input_name: &str,
        pdf_bytes: &[u8],
    ) -> Result<FontRunInputs, String> {
        let pdf_report = build_font_run_report_from_input_name(input_name, pdf_bytes)?;
        let report = pdf_report.to_public_report();
        Ok(FontRunInputs { report, pdf_report })
    }
}

impl Default for FontsData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
pub(super) fn finalize_file_font_report(
    report: FileFontReport,
    include_details: bool,
) -> FileFontReport {
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

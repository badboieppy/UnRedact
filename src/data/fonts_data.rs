use std::path::Path;

use crate::dependency::file_store::FileStore;
use crate::dependency::pdf_font_occurrence_accessor::{
    build_file_font_report, DataBuildConfig, FileDataBuilder,
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
    pub fn write_fonts(&self, path: &Path, report: &FontDetectionReport) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(report)
            .map_err(|e| format!("failed to encode fonts json: {e}"))?;
        self.file_store.write(path, &json)
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
        let report = build_font_run_report(pdf_path, &bytes)?;
        Ok(FontRunInputs { report })
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
    use super::*;
    use crate::types::file_types::{
        DocumentLocation, FontId, FontOccurrence, FontOccurrences, InputFileKind, Rect, Region,
        TextSourceKind,
    };

    fn occ(font: &str) -> FontOccurrence {
        FontOccurrence {
            font: FontId {
                family: font.to_owned(),
                variant: None,
            },
            location: DocumentLocation {
                page_index: Some(0),
                region: Region {
                    bbox: Rect::new(0.0, 0.0, 1.0, 1.0),
                },
            },
            text: None,
            confidence: None,
        }
    }

    #[test]
    fn finalize_file_font_report_hides_occurrences_when_disabled() {
        let report = FileFontReport {
            path: "a.pdf".to_owned(),
            kind: InputFileKind::Pdf,
            text_source: TextSourceKind::EmbeddedText,
            fonts: FontsFound {
                distinct: vec![],
                counts: vec![],
            },
            occurrences: Some(FontOccurrences {
                items: vec![occ("Arial"), occ("Arial"), occ("Calibri")],
            }),
        };

        let out = finalize_file_font_report(report, false);
        assert!(out.occurrences.is_none());
        assert_eq!(out.fonts.counts.len(), 2);
    }
}

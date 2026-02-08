use std::path::Path;

use crate::redaction_guess::data::{DictionaryData, FontRunData, ReportData};
use crate::redaction_guess::logic::{run_from_paths as run_from_paths_logic, RunGuessRequest};
use crate::redaction_guess::types::{GuessConfig, GuessReport};

#[inline]
pub fn run_from_paths(
    redactions_path: &Path,
    fonts_path: &Path,
    pdf_path: &Path,
    dictionary_path: Option<&Path>,
    cfg: GuessConfig,
) -> Result<GuessReport, String> {
    let report_data = ReportData::new();
    let dictionary_data = DictionaryData::new();
    let font_run_data = FontRunData::new();
    run_from_paths_logic(RunGuessRequest {
        report_data: &report_data,
        dictionary_data: &dictionary_data,
        font_run_data: &font_run_data,
        redactions_path,
        fonts_path,
        pdf_path,
        dictionary_path,
        cfg: &cfg,
    })
}

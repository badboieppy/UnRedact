use std::path::Path;

use crate::redaction_guess::data::{DictionaryData, ReportData};
use crate::redaction_guess::logic::run_from_paths as run_from_paths_logic;
use crate::redaction_guess::types::{GuessConfig, GuessReport};

#[inline]
pub fn run_from_paths(
    redactions_path: &Path,
    fonts_path: &Path,
    dictionary_path: Option<&Path>,
    cfg: GuessConfig,
) -> Result<GuessReport, String> {
    let report_data = ReportData::new();
    let dictionary_data = DictionaryData::new();
    run_from_paths_logic(
        &report_data,
        &dictionary_data,
        redactions_path,
        fonts_path,
        dictionary_path,
        &cfg,
    )
}

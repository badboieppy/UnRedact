use std::path::Path;

use crate::data::redactions_data::RedactionsData;
use crate::data::{DictionaryDataSource, FontRunDataSource, ReportDataSource};
use crate::types::guess_types::{GuessConfig, GuessReport};

use super::redaction_guessing_component::{run_guess_from_bytes, RunGuessFromBytesRequest};

pub struct RunGuessRequest<'a> {
    pub report_data: &'a dyn ReportDataSource,
    pub dictionary_data: &'a dyn DictionaryDataSource,
    pub font_run_data: &'a dyn FontRunDataSource,
    pub redactions_path: &'a Path,
    pub fonts_path: &'a Path,
    pub pdf_path: &'a Path,
    pub dictionary_path: Option<&'a Path>,
    pub cfg: &'a GuessConfig,
}

#[inline]
pub fn run_guess_from_paths(req: RunGuessRequest<'_>) -> Result<GuessReport, String> {
    let reports = req
        .report_data
        .load_reports(req.redactions_path, req.fonts_path)?;
    let dictionary = req.dictionary_data.load_dictionary(req.dictionary_path)?;

    let redactions_data = RedactionsData::new();
    let pdf_bytes = redactions_data.read_input_bytes(req.pdf_path)?;

    // Preserve prior behavior of validating font-run extraction in path mode.
    drop(req.font_run_data.load_font_runs(req.pdf_path)?);

    let mut diagnostics = reports.diagnostics;
    diagnostics.extend(dictionary.diagnostics);
    let pdf_name = req.pdf_path.to_string_lossy().to_string();
    run_guess_from_bytes(RunGuessFromBytesRequest {
        pdf_name: &pdf_name,
        pdf_bytes: &pdf_bytes,
        redactions: &reports.redactions,
        dictionary: &dictionary.dictionary,
        diagnostics: &diagnostics,
        cfg: req.cfg,
    })
}

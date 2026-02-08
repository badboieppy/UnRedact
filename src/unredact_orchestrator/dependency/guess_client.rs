use std::path::Path;

use crate::redaction_finder::types::RedactionReport;
use crate::redaction_guess::logic::build_report_from_parts;
use crate::redaction_guess::types::{GuessConfig, GuessReport};

#[derive(Debug, Clone, Copy)]
pub struct GuessClient;

impl GuessClient {
    #[inline]
    pub fn build_report(
        &self,
        redactions_path: &Path,
        fonts_path: &Path,
        redactions: RedactionReport,
        dictionary: Vec<String>,
        diagnostics: Vec<String>,
        cfg: &GuessConfig,
    ) -> GuessReport {
        build_report_from_parts(
            redactions_path,
            fonts_path,
            redactions,
            dictionary,
            diagnostics,
            cfg,
        )
    }
}

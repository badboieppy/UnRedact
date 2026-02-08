use std::path::Path;

use crate::redaction_finder::types::RedactionReport;
use crate::redaction_guess::types::{GuessConfig, GuessReport};
use crate::unredact_cli::dependency::{FileStore, GuessClient};

#[derive(Debug, Clone, Copy)]
pub struct GuessData {
    file_store: FileStore,
    client: GuessClient,
}

impl GuessData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
            client: GuessClient,
        }
    }

    #[inline]
    pub fn build_guess_report(
        &self,
        redactions_path: &Path,
        fonts_path: &Path,
        redactions: RedactionReport,
        dictionary: Vec<String>,
        diagnostics: Vec<String>,
        cfg: &GuessConfig,
    ) -> GuessReport {
        self.client.build_report(
            redactions_path,
            fonts_path,
            redactions,
            dictionary,
            diagnostics,
            cfg,
        )
    }

    #[inline]
    pub fn write_guesses(&self, path: &Path, report: &GuessReport) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(report)
            .map_err(|e| format!("failed to encode guesses json: {e}"))?;
        self.file_store.write(path, &json)
    }
}

impl Default for GuessData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

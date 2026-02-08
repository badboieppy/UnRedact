use std::path::Path;

use crate::redaction_finder::types::{RedactionFinderConfig, RedactionReport};
use crate::unredact_cli::dependency::{FileStore, RedactionFinderClient};

#[derive(Debug, Clone, Copy)]
pub struct RedactionData {
    file_store: FileStore,
    client: RedactionFinderClient,
}

impl RedactionData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
            client: RedactionFinderClient,
        }
    }

    #[inline]
    pub fn read_input_bytes(&self, input: &Path) -> Result<Vec<u8>, String> {
        self.file_store.read(input)
    }

    #[inline]
    pub fn detect_redactions(
        &self,
        input: &Path,
        bytes: &[u8],
        cfg: &RedactionFinderConfig,
    ) -> Result<RedactionReport, String> {
        self.client.detect_redactions(input, bytes, cfg)
    }

    #[inline]
    pub fn write_redactions(&self, path: &Path, report: &RedactionReport) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(report)
            .map_err(|e| format!("failed to encode redactions json: {e}"))?;
        self.file_store.write(path, &json)
    }
}

impl Default for RedactionData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

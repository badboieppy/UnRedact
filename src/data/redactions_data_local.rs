use std::path::Path;

use crate::dependency::file_store::FileStore;
use crate::types::redaction_types::RedactionReport;

use super::redactions_data::RedactionsData;

impl RedactionsData {
    #[inline]
    pub fn read_input_bytes(&self, input: &Path) -> Result<Vec<u8>, String> {
        let file_store = FileStore;
        file_store.read(input)
    }

    #[inline]
    pub fn write_redactions(&self, path: &Path, report: &RedactionReport) -> Result<(), String> {
        let file_store = FileStore;
        let json = serde_json::to_vec_pretty(report)
            .map_err(|e| format!("failed to encode redactions json: {e}"))?;
        file_store.write(path, &json)
    }
}

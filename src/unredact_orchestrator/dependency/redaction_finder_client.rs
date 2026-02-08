use std::path::Path;

use crate::redaction_finder::dependency::hayro_renderer::HayroRenderer;
use crate::redaction_finder::logic::redaction_finder_component::build_report;
use crate::redaction_finder::service::redaction_finder_entry::{
    find_redactions_in_pdf_bytes_vector_only, find_redactions_in_pdf_bytes_with_renderer,
};
use crate::redaction_finder::types::{RedactionFinderConfig, RedactionReport};

#[derive(Debug, Clone, Copy)]
pub struct RedactionFinderClient;

impl RedactionFinderClient {
    #[inline]
    pub fn detect_redactions(
        &self,
        input: &Path,
        bytes: &[u8],
        cfg: &RedactionFinderConfig,
    ) -> Result<RedactionReport, String> {
        let output = if cfg.enable_image_analysis {
            let renderer = HayroRenderer::new_from_bytes(bytes)
                .map_err(|e| format!("failed to initialize hayro renderer: {e}"))?;
            find_redactions_in_pdf_bytes_with_renderer(bytes, &renderer, *cfg)
                .map_err(|e| format!("redaction_scan_failed: {e}"))?
        } else {
            find_redactions_in_pdf_bytes_vector_only(bytes, *cfg)
                .map_err(|e| format!("redaction_scan_failed: {e}"))?
        };

        Ok(build_report(input, output))
    }
}

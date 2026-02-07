use std::fmt::{Display, Formatter};
use std::path::Path;

use crate::redaction_finder::logic::redaction_finder_component::run_redaction_finder_from_bytes;
use crate::redaction_finder::types::{PdfRenderer, RedactionFinderConfig, RedactionFinderOutput};

#[derive(Debug)]
pub enum RedactionError {
    Io(std::io::Error),
    Pdf(lopdf::Error),
    Render(String),
    Internal(String),
}

impl Display for RedactionError {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io_error: {e}"),
            Self::Pdf(e) => write!(f, "pdf_error: {e}"),
            Self::Render(e) => write!(f, "render_error: {e}"),
            Self::Internal(e) => write!(f, "internal_error: {e}"),
        }
    }
}

impl std::error::Error for RedactionError {
    #[inline]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Pdf(e) => Some(e),
            Self::Render(_) | Self::Internal(_) => None,
        }
    }
}

impl From<std::io::Error> for RedactionError {
    #[inline]
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<lopdf::Error> for RedactionError {
    #[inline]
    fn from(value: lopdf::Error) -> Self {
        Self::Pdf(value)
    }
}

#[inline]
pub fn find_redactions_in_pdf_bytes_with_renderer(
    bytes: &[u8],
    renderer: &dyn PdfRenderer,
    cfg: RedactionFinderConfig,
) -> Result<RedactionFinderOutput, RedactionError> {
    run_redaction_finder_from_bytes(bytes, Some(renderer), cfg).map_err(RedactionError::Internal)
}

#[inline]
pub fn find_redactions_in_pdf_path_with_renderer(
    path: &Path,
    renderer: &dyn PdfRenderer,
    cfg: RedactionFinderConfig,
) -> Result<RedactionFinderOutput, RedactionError> {
    let bytes = std::fs::read(path)?;
    find_redactions_in_pdf_bytes_with_renderer(&bytes, renderer, cfg)
}

#[inline]
pub fn find_redactions_in_pdf_bytes_vector_only(
    bytes: &[u8],
    cfg: RedactionFinderConfig,
) -> Result<RedactionFinderOutput, RedactionError> {
    let mut effective_cfg = cfg;
    effective_cfg.enable_image_analysis = false;
    run_redaction_finder_from_bytes(bytes, None, effective_cfg).map_err(RedactionError::Internal)
}

#[inline]
pub fn find_redactions_in_pdf_bytes(
    bytes: &[u8],
    cfg: RedactionFinderConfig,
) -> RedactionFinderOutput {
    match find_redactions_in_pdf_bytes_vector_only(bytes, cfg) {
        Ok(v) => v,
        Err(e) => RedactionFinderOutput {
            redactions: Vec::new(),
            diagnostics: vec![e.to_string()],
        },
    }
}

pub mod data;
pub mod dependency;
pub mod logic;
pub mod service;
pub mod types;

pub use dependency::hayro_renderer::HayroRenderer;
pub use logic::redaction_finder_component::build_report;
pub use service::redaction_finder_entry::{
    find_redactions_in_pdf_bytes, find_redactions_in_pdf_bytes_vector_only,
    find_redactions_in_pdf_bytes_with_renderer, find_redactions_in_pdf_path_with_renderer,
    RedactionError,
};
pub use types::{
    PdfRenderer, Rect, RedactionFinderConfig, RedactionFinderOutput, RedactionKind, RedactionMode,
    RedactionOccurrence, RedactionReport, RenderedPage, UnderlyingTextHit,
};
